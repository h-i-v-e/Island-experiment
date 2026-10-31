//! Chunk preprocessing owns render snapshots; streamed meshes live on disk
//! until an IO task hands an owned buffer to the GPU. Collision remains exact.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use crate::{
    Orbit, Scene, TerrainChunk,
    chunk_cache::{CachedChunk, CachedSurface, CachedWorld, ChunkCache, SurfaceAtlasData},
    chunk_policy::{prefetch_lod, select_lod, should_evict},
    command::{BedView, GenerationMode, RiverDisplay},
    lod_material::{LodMaterial, atlas_image, river_image},
    surface,
    terrain_material::TerrainTextures,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::primitives::Sphere as BoundsSphere,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
    tasks::{AsyncComputeTaskPool, IoTaskPool, Task, block_on, poll_once},
};
use planet_rs::{
    DVec3, PlanetTerrain,
    chunk_lod::{ChunkLod, build_lods, build_unpinned_lods},
    chunks::{ChunkMesh, ChunkVertex, occluder_radius, partition},
    surface_atlas::SurfaceSampler,
};
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const LEVELS: usize = 4;
const LOAD_LIMIT: usize = 4;
// Retire cached chunks baked before waterfall support patches were separated.
const CACHE_GENERATION: u64 = 9;

#[derive(Resource, Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct StreamingSettings {
    pub level: u8,
    pub horizon: bool,
    pub colours: bool,
    pub target_pixels: f64,
    pub budget_mb: usize,
    pub forced_lod: Option<usize>,
    pub surface_maps: bool,
    pub surface_debug: u32,
}
impl Default for StreamingSettings {
    fn default() -> Self {
        Self {
            level: 2,
            horizon: true,
            colours: false,
            target_pixels: 2.0,
            budget_mb: 192,
            forced_lod: None,
            surface_maps: true,
            surface_debug: 0,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct StreamingStats {
    pub total: usize,
    pub visible: usize,
    pub hidden_horizon: usize,
    pub hidden_frustum: usize,
    pub triangles: usize,
    pub lods: [usize; LEVELS],
    pub resident_bytes: usize,
    pub coarse_bytes: usize,
    pub mapped: usize,
    pub river_meshes: usize,
    pub river_textures: usize,
    pub loading: usize,
    pub status: String,
}

struct Prepared {
    ocean: Option<planet_rs::ocean_surface::CoastMap>,
    cache: ChunkCache,
    index: CachedWorld,
    coarse: Vec<CachedSurface>,
}
struct Entry {
    metadata: CachedChunk,
    entity: Entity,
    material: Handle<LodMaterial>,
    handles: [Option<Handle<Mesh>>; LEVELS],
    atlases: [Option<Handle<Image>>; LEVELS],
    river_atlases: [Option<Handle<Image>>; LEVELS],
    bytes: [usize; LEVELS],
    last_needed: [f64; LEVELS],
    failed: [bool; LEVELS],
    selected: usize,
    desired: usize,
    visible: bool,
}
struct Load {
    entry: usize,
    lod: usize,
    reserved: usize,
    task: Task<std::io::Result<CachedSurface>>,
}
#[derive(Resource)]
pub(crate) struct ChunkWorld {
    pub rebuild: bool,
    job: Option<Task<Result<Prepared, String>>>,
    entries: Vec<Entry>,
    loads: Vec<Load>,
    cache: Option<ChunkCache>,
    occluder: f64,
    dirty: bool,
    budget_limited: bool,
    progress: Arc<AtomicUsize>,
    partition_level: u8,
    flow: crate::water::FlowField,
}
impl Default for ChunkWorld {
    fn default() -> Self {
        Self {
            rebuild: true,
            job: None,
            entries: Vec::new(),
            loads: Vec::new(),
            cache: None,
            occluder: 0.0,
            dirty: false,
            budget_limited: false,
            progress: Arc::new(AtomicUsize::new(0)),
            partition_level: 0,
            flow: default(),
        }
    }
}
impl ChunkWorld {
    pub(crate) fn partition_level(&self) -> u8 {
        self.partition_level
    }
    pub(crate) fn fine_chunks_near(
        &self,
        point: Vec3,
        range: f64,
    ) -> impl Iterator<Item = (u32, &Handle<Mesh>, planet_rs::chunks::ChunkBounds)> {
        let point = DVec3::from_array(point.to_array().map(f64::from));
        self.entries
            .iter()
            .filter(move |entry| {
                entry.metadata.layer == 0
                    && entry.visible
                    && entry.selected == 0
                    && entry.metadata.bounds.centre.distance(point)
                        <= entry.metadata.bounds.radius + range
            })
            .filter_map(|entry| {
                entry.handles[0]
                    .as_ref()
                    .map(|handle| (entry.metadata.id, handle, entry.metadata.bounds))
            })
    }
    pub(crate) fn fine_visible(&self, id: u32) -> bool {
        self.entries.iter().any(|entry| {
            entry.metadata.layer == 0
                && entry.metadata.id == id
                && entry.visible
                && entry.selected == 0
        })
    }
    pub fn settled(&self) -> bool {
        !self.rebuild
            && self.job.is_none()
            && !self.entries.is_empty()
            && self.loads.is_empty()
            && (self.budget_limited
                || self.entries.iter().all(|entry| {
                    entry.handles[entry.desired].is_some() || entry.failed[entry.desired]
                }))
    }
}

fn texture_bytes(size: [u32; 2]) -> usize {
    let [mut width, mut height] = size;
    if width == 0 || height == 0 {
        return 0;
    }
    let levels = width.min(height).ilog2().min(2) + 1;
    let mut bytes = 0;
    for _ in 0..levels {
        bytes += width as usize * height as usize * 4;
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    bytes
}

fn expected_bytes(metadata: &CachedChunk, lod: usize, smooth: bool) -> usize {
    let mapped = metadata.atlas_sizes[lod] != [0, 0] || metadata.layer == 2;
    let stride = if mapped { 48 } else { 40 };
    let geometry = if smooth {
        metadata.vertex_counts[lod] * stride + metadata.triangles[lod] * 12
    } else {
        metadata.triangles[lod] * 3 * stride
    };
    geometry + texture_bytes(metadata.atlas_sizes[lod]) * 5 / 4
}

fn mesh_bytes(surface: &CachedSurface, smooth: bool) -> usize {
    let mapped = surface.atlas.is_some();
    let stride = if mapped { 48 } else { 40 };
    let geometry = if smooth {
        surface.mesh.vertices.len() * stride + surface.mesh.triangles.len() * 12
    } else {
        surface.mesh.triangles.len() * 3 * stride
    };
    geometry
        + surface.atlas.as_ref().map_or(0, |atlas| {
            texture_bytes([atlas.width, atlas.height]) * 5 / 4
        })
}

fn render_mesh(source: &ChunkMesh, smooth: bool, uv: Option<&[[f32; 2]]>) -> Mesh {
    let mut mesh = if smooth {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            source
                .vertices
                .iter()
                .map(|v| v.position.as_vec3().to_array())
                .collect::<Vec<_>>(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            source
                .vertices
                .iter()
                .map(|v| v.normal.as_vec3().to_array())
                .collect::<Vec<_>>(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            source.vertices.iter().map(|v| v.colour).collect::<Vec<_>>(),
        );
        mesh.insert_indices(Indices::U32(
            source.triangles.iter().flatten().copied().collect(),
        ));
        mesh
    } else {
        let mut positions = Vec::with_capacity(source.triangles.len() * 3);
        let mut normals = Vec::with_capacity(positions.capacity());
        let mut colours = Vec::with_capacity(positions.capacity());
        for face in &source.triangles {
            let [a, b, c] = face.map(|i| source.vertices[i as usize].position);
            let normal = (b - a).cross(c - a).normalize().as_vec3().to_array();
            for &i in face {
                let vertex = &source.vertices[i as usize];
                positions.push(vertex.position.as_vec3().to_array());
                normals.push(normal);
                colours.push(vertex.colour);
            }
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
    };
    if let Some(uv) = uv {
        let coordinates: Vec<_> = if smooth {
            uv.to_vec()
        } else {
            source
                .triangles
                .iter()
                .flatten()
                .map(|&i| uv[i as usize])
                .collect()
        };
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, coordinates);
    }
    mesh
}

fn upload(
    surface: &CachedSurface,
    smooth: bool,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    flow: Option<&crate::water::FlowField>,
) -> (Handle<Mesh>, Option<Handle<Image>>, Option<Handle<Image>>) {
    // Empty higher river LODs are absent render geometry, not zero-sized GPU
    // allocations. Bevy's mesh slab allocator requires a real vertex range.
    if surface.mesh.triangles.is_empty() {
        return (Handle::default(), None, None);
    }
    let flow_uv = flow.map(|flow| {
        surface
            .mesh
            .vertices
            .iter()
            .map(|v| flow.uv(v.position))
            .collect::<Vec<_>>()
    });
    let mesh = meshes.add(render_mesh(
        &surface.mesh,
        smooth,
        flow_uv
            .as_deref()
            .or_else(|| surface.atlas.as_ref().map(|atlas| atlas.uv.as_slice())),
    ));
    let image = surface
        .atlas
        .as_ref()
        .map(|atlas| images.add(atlas_image(atlas.width, atlas.height, &atlas.pixels)));
    let river = surface
        .atlas
        .as_ref()
        .map(|atlas| images.add(river_image(atlas.width, atlas.height, &atlas.rivers)));
    (mesh, image, river)
}

fn terrain_source(terrain: &PlanetTerrain, generation: GenerationMode, bed: BedView) -> ChunkMesh {
    let uncarved = bed == BedView::Original;
    let mut vertices: Vec<_> = (0..terrain.elevations.len())
        .map(|i| {
            let position = surface::displayed_position(terrain, i, uncarved);
            ChunkVertex {
                position,
                normal: terrain.normals[i],
                colour: if generation == GenerationMode::Sphere {
                    Color::srgb(0.27, 0.62, 0.72).to_linear().to_f32_array()
                } else {
                    surface::terrain_material(terrain, i, uncarved)
                },
                protected: terrain.channel_depths[i] > 1.0e-12 || terrain.bank_lifts[i] > 1.0e-12,
            }
        })
        .collect();
    if uncarved {
        let mut normals = vec![DVec3::ZERO; vertices.len()];
        for face in &terrain.topology.triangles {
            let [a, b, c] = face.map(|i| vertices[i as usize].position);
            let n = (b - a).cross(c - a);
            for &i in face {
                normals[i as usize] += n;
            }
        }
        for (vertex, normal) in vertices.iter_mut().zip(normals) {
            vertex.normal = normal.normalize();
        }
    }
    // Pin both endpoints of every shoreline edge. LOD never changes the
    // visible coastline or creates a second sea/river contact surface.
    for face in &terrain.topology.triangles {
        for [a, b] in [[face[0], face[1]], [face[1], face[2]], [face[2], face[0]]] {
            if (vertices[a as usize].position.length() < 1.0)
                != (vertices[b as usize].position.length() < 1.0)
            {
                vertices[a as usize].protected = true;
                vertices[b as usize].protected = true;
            }
        }
    }
    ChunkMesh {
        vertices,
        triangles: terrain.topology.triangles.clone(),
    }
}

fn river_source(terrain: &PlanetTerrain) -> ChunkMesh {
    let water = &terrain.river_surface;
    ChunkMesh {
        vertices: water
            .positions
            .iter()
            .zip(&water.normals)
            .map(|(&position, &normal)| ChunkVertex {
                position,
                normal,
                colour: [1.0; 4],
                protected: true,
            })
            .collect(),
        triangles: water
            .triangles
            .iter()
            .copied()
            .filter(|face| {
                let [a, b, c] = face.map(|i| water.positions[i as usize]);
                (b - a).cross(c - a).length_squared() > 0.0
            })
            .collect(),
    }
}

fn sea_source(contacts: &[DVec3]) -> ChunkMesh {
    // Retain the original sub-centimetre sea tessellation near waterfall feet.
    const BAND: f64 = 0.025;
    let key = |point: DVec3| point.to_array().map(|v| (v / BAND).floor() as i32);
    let mut bins = HashMap::<[i32; 3], Vec<DVec3>>::new();
    for &point in contacts {
        bins.entry(key(point)).or_default().push(point);
    }
    let mesh = Sphere::new(1.0).mesh().uv(1024, 512);
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        unreachable!("sphere positions")
    };
    let vertices = points
        .iter()
        .map(|&p| {
            let position = DVec3::from_array(p.map(f64::from));
            let [x, y, z] = key(position);
            let protected = (-1..=1).any(|dx| {
                (-1..=1).any(|dy| {
                    (-1..=1).any(|dz| {
                        bins.get(&[x + dx, y + dy, z + dz]).is_some_and(|points| {
                            points
                                .iter()
                                .any(|p| p.distance_squared(position) < BAND * BAND)
                        })
                    })
                })
            });
            ChunkVertex {
                position,
                normal: position.normalize(),
                colour: [1.0; 4],
                protected,
            }
        })
        .collect();
    let indices: Vec<_> = mesh
        .indices()
        .expect("indexed sphere")
        .iter()
        .map(|i| i as u32)
        .collect();
    let triangles = indices
        .chunks_exact(3)
        .map(|f| [f[0], f[1], f[2]])
        .filter(|f| {
            let [a, b, c] = f.map(|i| DVec3::from_array(points[i as usize].map(f64::from)));
            (b - a).cross(c - a).length_squared() > 0.0
        })
        .collect();
    ChunkMesh {
        vertices,
        triangles,
    }
}

fn cache_key(parts: &[(u8, ChunkMesh)], level: u8, water: bool) -> u64 {
    let mut state = DefaultHasher::new();
    CACHE_GENERATION.hash(&mut state);
    level.hash(&mut state);
    water.hash(&mut state);
    for (layer, mesh) in parts {
        layer.hash(&mut state);
        mesh.vertices.len().hash(&mut state);
        for vertex in &mesh.vertices {
            for value in vertex
                .position
                .to_array()
                .into_iter()
                .chain(vertex.normal.to_array())
            {
                value.to_bits().hash(&mut state);
            }
            for value in vertex.colour {
                value.to_bits().hash(&mut state);
            }
            vertex.protected.hash(&mut state);
        }
        mesh.triangles.hash(&mut state);
    }
    state.finish()
}

fn cached_world(cache: &ChunkCache) -> Option<(CachedWorld, Vec<CachedSurface>)> {
    let index = cache.read_index().ok()?;
    if !index
        .chunks
        .iter()
        .all(|entry| entry.errors.len() == LEVELS && entry.triangles.len() == LEVELS)
    {
        return None;
    }
    let coarse = index
        .chunks
        .iter()
        .map(|entry| {
            if entry.atlas_sizes[LEVELS - 1] == [0, 0] {
                cache
                    .read(entry.id, LEVELS - 1)
                    .map(|mesh| CachedSurface { mesh, atlas: None })
            } else {
                cache.read_surface(entry.id, LEVELS - 1)
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some((index, coarse))
}

fn sea_contacts(parts: &[(u8, ChunkMesh)]) -> Vec<DVec3> {
    parts
        .iter()
        .flat_map(|(layer, mesh)| {
            mesh.vertices
                .iter()
                .filter(move |vertex| {
                    (*layer == 2 || vertex.protected)
                        && (vertex.position.length() - 1.0).abs() < 0.025
                })
                .map(|vertex| vertex.position.normalize())
        })
        .collect()
}

#[allow(clippy::too_many_lines)] // Shared source geometry remains borrowed through all dependent bakes.
fn prepare(
    mut parts: Vec<(u8, ChunkMesh)>,
    level: u8,
    water: bool,
    radius: f64,
    progress: &AtomicUsize,
    detailed_ocean: bool,
) -> Result<Prepared, String> {
    let key = cache_key(&parts, level, water) ^ radius.to_bits().rotate_left(17);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/planet-chunks");
    let cache = ChunkCache::new(directory, key);
    let ocean = if water && detailed_ocean {
        progress.store(usize::MAX, Ordering::Relaxed);
        Some(crate::ocean::prepare_maps(&cache, &parts[0].1, radius)?)
    } else {
        None
    };
    progress.store(0, Ordering::Relaxed);
    if let Some((index, coarse)) = cached_world(&cache) {
        return Ok(Prepared {
            ocean,
            cache,
            index,
            coarse,
        });
    }
    let mut occluder = occluder_radius(&parts[0].1);
    if water {
        let contacts = sea_contacts(&parts);
        parts.push((1, sea_source(&contacts)));
    }
    let river_sampler = parts
        .iter()
        .find(|(layer, mesh)| *layer == 2 && !mesh.triangles.is_empty())
        .map(|(_, mesh)| SurfaceSampler::new(mesh))
        .transpose()?;
    let mut index = CachedWorld {
        occluder_radius: occluder,
        chunks: Vec::new(),
    };
    let mut coarse = Vec::new();
    for (layer, mesh) in parts
        .iter()
        .map(|(layer, mesh)| (*layer, mesh))
        .filter(|(_, mesh)| !mesh.triangles.is_empty())
    {
        let sampler = (layer == 0)
            .then(|| SurfaceSampler::new(mesh))
            .transpose()?;
        for chunk in partition(mesh, level).map_err(|e| e.to_string())? {
            if chunk.mesh.triangles.is_empty() {
                continue;
            }
            let lods = layer_lods(layer, chunk.mesh, chunk.bounds.max_radius);
            let id = u32::from(layer) * 100_000 + chunk.id;
            let errors = (0..LEVELS)
                .map(|level| lods[level.min(lods.len() - 1)].error)
                .collect();
            let mut triangles = Vec::with_capacity(LEVELS);
            let mut vertex_counts = Vec::with_capacity(LEVELS);
            let mut atlas_sizes = Vec::with_capacity(LEVELS);
            for level in 0..LEVELS {
                let lod = &lods[level.min(lods.len() - 1)];
                let surface = bake_surface(
                    sampler.as_ref(),
                    river_sampler.as_ref(),
                    &lod.mesh,
                    level,
                    radius,
                )
                .map_err(|error| format!("Chunk {} LOD {level}: {error}", chunk.id))?;
                triangles.push(surface.mesh.triangles.len());
                vertex_counts.push(surface.mesh.vertices.len());
                atlas_sizes.push(
                    surface
                        .atlas
                        .as_ref()
                        .map_or([0, 0], |atlas| [atlas.width, atlas.height]),
                );
                (if surface.atlas.is_some() {
                    cache.write_surface(id, level, &surface.mesh, surface.atlas.as_ref())
                } else {
                    cache.write(id, level, &surface.mesh)
                })
                .map_err(|e| format!("Chunk cache: {e}"))?;
                if layer == 0 {
                    occluder = occluder.min(occluder_radius(&surface.mesh));
                }
                if level == LEVELS - 1 {
                    coarse.push(surface);
                }
            }
            index.chunks.push(CachedChunk {
                id,
                layer,
                bounds: chunk.bounds,
                errors,
                triangles,
                vertex_counts,
                atlas_sizes,
            });
            if layer == 0 {
                progress.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    index.occluder_radius = occluder;
    cache
        .write_index(&index)
        .map_err(|e| format!("Chunk index: {e}"))?;
    Ok(Prepared {
        ocean,
        cache,
        index,
        coarse,
    })
}

fn layer_lods(layer: u8, mesh: ChunkMesh, max_radius: f64) -> Vec<ChunkLod> {
    if layer == 2 {
        return std::iter::once(ChunkLod { mesh, error: 0.0 })
            .chain(std::iter::repeat_with(|| ChunkLod {
                mesh: ChunkMesh::default(),
                error: 0.0,
            }))
            .take(LEVELS)
            .collect();
    }
    let mut lods = if layer == 0 {
        build_unpinned_lods(&mesh)
    } else {
        build_lods(&mesh)
    };
    if layer == 1 {
        // Sea LODs are outward radial graphs over an identical angular domain.
        // Radial range bounds their true error more tightly than endpoint travel.
        let mut error: f64 = 0.0;
        for lod in lods.iter_mut().skip(1) {
            error = error.max(max_radius - occluder_radius(&lod.mesh));
            lod.error = error;
        }
    }
    lods
}

fn bake_surface(
    sampler: Option<&SurfaceSampler<'_>>,
    rivers: Option<&SurfaceSampler<'_>>,
    mesh: &ChunkMesh,
    level: usize,
    radius: f64,
) -> Result<CachedSurface, String> {
    if let Some(sampler) = sampler
        && level > 0
    {
        let atlas = sampler.bake_with_rivers(mesh, [0, 256, 128, 64][level], radius, rivers)?;
        Ok(CachedSurface {
            mesh: atlas.mesh,
            atlas: Some(SurfaceAtlasData {
                uv: atlas.uv,
                width: atlas.width,
                height: atlas.height,
                pixels: atlas.pixels,
                rivers: atlas.rivers,
            }),
        })
    } else {
        Ok(CachedSurface {
            mesh: mesh.clone(),
            atlas: None,
        })
    }
}

fn layer_colour(layer: u8) -> Color {
    match layer {
        1 => Color::srgb(0.025, 0.20, 0.34),
        2 => Color::srgb(0.045, 0.39, 0.65),
        _ => Color::WHITE,
    }
}
fn cell_colour(id: u32) -> Color {
    let hue = (id % 100_000).wrapping_mul(137) % 360;
    Color::hsl(hue as f32, 0.65, 0.65)
}
fn layer_material(layer: u8) -> StandardMaterial {
    StandardMaterial {
        base_color: layer_colour(layer),
        perceptual_roughness: if layer == 0 { 0.85 } else { 0.35 },
        metallic: if layer == 1 { 0.08 } else { 0.0 },
        specular_transmission: if layer == 2 { 0.001 } else { 0.0 },
        cull_mode: if layer == 2 {
            None
        } else {
            Some(bevy::render::render_resource::Face::Back)
        },
        double_sided: layer == 2,
        ..default()
    }
}

#[allow(clippy::too_many_arguments)] // Separate asset stores borrow the published scene.
fn install(
    prepared: Prepared,
    world: &mut ChunkWorld,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<LodMaterial>,
    images: &mut Assets<Image>,
    scene: &Scene,
    textures: &TerrainTextures,
    detailed_ocean: bool,
) {
    world.loads.clear();
    for entry in world.entries.drain(..) {
        commands.entity(entry.entity).despawn();
        for handle in entry.handles.into_iter().flatten() {
            meshes.remove(handle.id());
        }
        for image in entry
            .atlases
            .into_iter()
            .chain(entry.river_atlases)
            .flatten()
        {
            images.remove(image.id());
        }
        materials.remove(entry.material.id());
    }
    world.cache = Some(prepared.cache);
    world.flow = crate::water::FlowField::new(&scene.terrain);
    let terrain_cells = prepared
        .index
        .chunks
        .iter()
        .filter(|chunk| chunk.layer == 0)
        .count();
    world.partition_level =
        u8::try_from((terrain_cells / 20).max(1).ilog(4)).expect("bounded partition depth");
    world.occluder = prepared.index.occluder_radius;
    for (metadata, mesh) in prepared.index.chunks.into_iter().zip(prepared.coarse) {
        if metadata.layer == 1 && detailed_ocean {
            continue;
        }
        let smooth = scene.smooth || metadata.layer != 0;
        let bytes = mesh_bytes(&mesh, smooth)
            + if metadata.layer == 2 {
                mesh.mesh.vertices.len() * 8
            } else {
                0
            };
        let (handle, atlas, river) = upload(
            &mesh,
            smooth,
            meshes,
            images,
            (metadata.layer == 2).then_some(&world.flow),
        );
        let material = materials.add(LodMaterial {
            base: layer_material(metadata.layer),
            extension: textures.extension(
                if (metadata.layer == 0 && scene.generation == GenerationMode::Terrain)
                    || metadata.layer == 2
                {
                    scene.terrain.topology.radius as f32
                } else {
                    0.0
                },
            ),
        });
        let entity = commands
            .spawn((
                Mesh3d(handle.clone()),
                MeshMaterial3d(material.clone()),
                Visibility::Visible,
            ))
            .id();
        if metadata.layer == 0 {
            commands.entity(entity).insert(TerrainChunk);
        } else {
            commands.entity(entity).insert(bevy::light::NotShadowCaster);
            if metadata.layer == 2 {
                commands.entity(entity).insert(crate::water::RiverChunk);
            }
        }
        let mut handles = std::array::from_fn(|_| None);
        let mut atlases = std::array::from_fn(|_| None);
        atlases[LEVELS - 1] = atlas;
        let mut river_atlases = std::array::from_fn(|_| None);
        river_atlases[LEVELS - 1] = river;
        let mut sizes = [0; LEVELS];
        handles[LEVELS - 1] = Some(handle);
        sizes[LEVELS - 1] = bytes;
        world.entries.push(Entry {
            metadata,
            entity,
            material,
            handles,
            atlases,
            river_atlases,
            bytes: sizes,
            last_needed: [0.0; LEVELS],
            failed: [false; LEVELS],
            selected: LEVELS - 1,
            desired: LEVELS - 1,
            visible: false,
        });
    }
    world.dirty = true;
}

// Publish geometry before first-person support updates; select streamed LODs
// afterwards, using the resulting camera transform in the same frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_update(
    mut commands: Commands,
    mut scene: ResMut<Scene>,
    mut panel: ResMut<crate::panel::Panel>,
    settings: Res<StreamingSettings>,
    mut world: ResMut<ChunkWorld>,
    mut stats: ResMut<StreamingStats>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<LodMaterial>>,
    mut images: ResMut<Assets<Image>>,
    textures: Res<TerrainTextures>,
    mut ocean: Option<ResMut<crate::ocean::OceanWorld>>,
) {
    if world.rebuild && world.job.is_none() {
        world.rebuild = false;
        let (terrain, generation) = panel
            .pending_terrain()
            .unwrap_or((&scene.terrain, scene.generation));
        let mut parts = vec![(0, terrain_source(terrain, generation, scene.bed))];
        let water = generation != GenerationMode::Sphere;
        if water {
            parts.push((2, river_source(terrain)));
        }
        let level = settings.level;
        let radius = terrain.topology.radius;
        let detailed_ocean = ocean.is_some();
        let progress = Arc::new(AtomicUsize::new(0));
        world.progress = Arc::clone(&progress);
        world.job =
            Some(AsyncComputeTaskPool::get().spawn(async move {
                prepare(parts, level, water, radius, &progress, detailed_ocean)
            }));
        stats.status = String::from("Preparing planet chunks…");
    }
    if world.progress.load(Ordering::Relaxed) == usize::MAX {
        stats.status = "Baking ocean depth & shore-distance maps…".into();
    } else if world.job.is_some() {
        stats.status = format!(
            "Preparing LOD maps · terrain chunks {} / {}",
            world.progress.load(Ordering::Relaxed),
            20 * 4_usize.pow(u32::from(settings.level))
        );
    }
    if let Some(task) = &mut world.job
        && let Some(result) = block_on(poll_once(task))
    {
        world.job = None;
        match result {
            _ if world.rebuild => { /* A newer request superseded this preparation. */ }
            Ok(mut prepared) => {
                // Scene is also the collision source. Publish it only alongside
                // its matching render snapshot, then update the explorer below.
                panel.activate_pending(&mut scene);
                if let Some(ocean) = ocean.as_mut()
                    && let Some(map) = prepared.ocean.take()
                {
                    ocean.install_maps(map);
                }
                install(
                    prepared,
                    &mut world,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &scene,
                    &textures,
                    ocean.is_some(),
                );
                stats.status = String::from("Streaming ready");
            }
            Err(error) => {
                error!("{error}");
                panel.render_failed(&error);
                stats.status = error;
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn update(
    scene: Res<Scene>,
    lighting: Option<Res<crate::lighting::LightingSettings>>,
    settings: Res<StreamingSettings>,
    time: Res<Time>,
    mut world: ResMut<ChunkWorld>,
    mut stats: ResMut<StreamingStats>,
    cameras: Query<(&Camera, &Transform, &Projection), With<Orbit>>,
    mut entities: Query<(&mut Mesh3d, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<LodMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let now = time.elapsed_secs_f64();
    let mut index = 0;
    while index < world.loads.len() {
        if let Some(result) = block_on(poll_once(&mut world.loads[index].task)) {
            let load = world.loads.swap_remove(index);
            let ChunkWorld { entries, flow, .. } = &mut *world;
            let entry = &mut entries[load.entry];
            match result {
                Ok(mesh) => {
                    let smooth = scene.smooth || entry.metadata.layer != 0;
                    entry.bytes[load.lod] = mesh_bytes(&mesh, smooth)
                        + if entry.metadata.layer == 2 {
                            mesh.mesh.vertices.len() * 8
                        } else {
                            0
                        };
                    let (handle, atlas, river) = upload(
                        &mesh,
                        smooth,
                        &mut meshes,
                        &mut images,
                        (entry.metadata.layer == 2).then_some(flow),
                    );
                    entry.handles[load.lod] = Some(handle);
                    entry.atlases[load.lod] = atlas;
                    entry.river_atlases[load.lod] = river;
                }
                Err(error) => {
                    warn!("Chunk {} LOD {}: {error}", entry.metadata.id, load.lod);
                    entry.failed[load.lod] = true;
                    if let Some(cache) = &world.cache {
                        let _ = cache.invalidate_index();
                    }
                    world.rebuild = true;
                }
            }
        } else {
            index += 1;
        }
    }
    let Ok((camera, transform, projection)) = cameras.single() else {
        return;
    };
    let Projection::Perspective(perspective) = projection else {
        return;
    };
    let frustum = projection.compute_frustum(&GlobalTransform::from(*transform));
    let eye = DVec3::from_array(transform.translation.to_array().map(f64::from));
    let pixel_height = camera
        .physical_viewport_size()
        .map_or(1000.0, |s| f64::from(s.y));
    let projection_scale = pixel_height / (2.0 * f64::from(perspective.fov * 0.5).tan());
    stats.total = world.entries.len();
    stats.visible = 0;
    stats.hidden_horizon = 0;
    stats.hidden_frustum = 0;
    stats.triangles = 0;
    stats.lods = [0; LEVELS];
    let mut requests = Vec::new();
    let occluder = world.occluder;
    let mut terrain_lods = HashMap::new();
    for (entry_index, entry) in world.entries.iter_mut().enumerate() {
        let bounds = entry.metadata.bounds;
        let show_layer = match entry.metadata.layer {
            1 => !scene.no_ocean,
            2 => {
                scene.river_display == RiverDisplay::Show
                    && terrain_lods.get(&(entry.metadata.id % 100_000)) == Some(&0)
            }
            _ => true,
        };
        let horizon = !settings.horizon || bounds.horizon_visible(eye, occluder);
        let in_frustum = frustum.intersects_sphere(
            &BoundsSphere {
                center: bounds.centre.as_vec3().to_array().into(),
                radius: bounds.radius as f32 + 1.0e-5,
            },
            true,
        );
        let visible = show_layer && horizon && in_frustum;
        entry.visible = visible;
        let distance = (eye.distance(bounds.centre) - bounds.radius).max(1.0e-7);
        let near = show_layer && horizon && distance < 0.08;
        let desired = if entry.metadata.layer == 2 {
            if show_layer { 0 } else { LEVELS - 1 }
        } else {
            settings.forced_lod.unwrap_or_else(|| {
                select_lod(
                    &entry.metadata.errors,
                    distance,
                    projection_scale,
                    settings.target_pixels,
                    entry.desired,
                )
            })
        };
        entry.desired = if visible { desired } else { LEVELS - 1 };
        if visible || near {
            let needed = if near {
                prefetch_lod(desired, true)
            } else {
                desired
            };
            for lod in [desired, needed] {
                entry.last_needed[lod] = now;
                if entry.handles[lod].is_none() && !entry.failed[lod] {
                    requests.push((distance, entry_index, lod));
                }
            }
        }
        let selected = resident_lod(&entry.handles, entry.desired);
        entry.selected = selected;
        if entry.metadata.layer == 0 {
            terrain_lods.insert(entry.metadata.id, selected);
        }
        if let Ok((mut mesh, mut visibility)) = entities.get_mut(entry.entity) {
            if let Some(handle) = &entry.handles[selected]
                && mesh.0 != *handle
            {
                mesh.0 = handle.clone();
            }
            // Keep offscreen terrain available to the sun shadow frusta. The
            // camera still frustum-culls it; horizon-hidden cells stay hidden.
            let shadow_caster = entry.metadata.layer == 0
                && horizon
                && lighting.as_ref().is_some_and(|settings| settings.shadows);
            let next_visibility = if visible || shadow_caster {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            if *visibility != next_visibility {
                *visibility = next_visibility;
            }
        }
        if !horizon && show_layer {
            stats.hidden_horizon += 1;
        } else if !in_frustum && show_layer {
            stats.hidden_frustum += 1;
        }
        if visible {
            stats.visible += 1;
            stats.triangles += entry.metadata.triangles[selected];
            stats.lods[selected] += 1;
        }
    }
    // Keep the displayed LOD alive until replacement; evict unused fine assets
    // after a grace period, then oldest unused assets if the budget is exceeded.
    let coarse_bytes: usize = world.entries.iter().map(|e| e.bytes[LEVELS - 1]).sum();
    let budget = coarse_bytes + settings.budget_mb * 1024 * 1024;
    let mut bytes: usize = world.entries.iter().flat_map(|e| e.bytes).sum();
    let mut candidates = Vec::new();
    for (i, entry) in world.entries.iter().enumerate() {
        for lod in 0..LEVELS - 1 {
            if entry.handles[lod].is_some()
                && entry.selected != lod
                && now - entry.last_needed[lod] > 0.5
            {
                candidates.push((entry.last_needed[lod], i, lod));
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (last, i, lod) in candidates {
        if bytes > budget || should_evict(last, now, 8.0, false) {
            let entry = &mut world.entries[i];
            if let Some(handle) = entry.handles[lod].take() {
                meshes.remove(handle.id());
                bytes -= entry.bytes[lod];
                entry.bytes[lod] = 0;
                if let Some(image) = entry.atlases[lod].take() {
                    images.remove(image.id());
                }
                if let Some(image) = entry.river_atlases[lod].take() {
                    images.remove(image.id());
                }
            }
        }
    }
    // A smaller budget takes effect even if every fine mesh is currently on
    // screen. Swap the farthest chunks to their permanent coarse mesh first.
    let mut farthest: Vec<_> = world
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            (
                (eye.distance(entry.metadata.bounds.centre) - entry.metadata.bounds.radius)
                    .max(0.0),
                i,
            )
        })
        .collect();
    farthest.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    for &(_, i) in &farthest {
        if bytes <= budget {
            break;
        }
        bytes -= release_detail(
            &mut world.entries[i],
            &mut meshes,
            &mut images,
            &mut entities,
        );
    }
    requests.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    requests.dedup_by(|a, b| a.1 == b.1 && a.2 == b.2);
    let mut reserved: usize = world.loads.iter().map(|load| load.reserved).sum();
    world.budget_limited = false;
    for (distance, entry, lod) in requests {
        if world.loads.len() >= LOAD_LIMIT {
            break;
        }
        if world
            .loads
            .iter()
            .any(|load| load.entry == entry && load.lod == lod)
        {
            continue;
        }
        let estimate = expected_bytes(
            &world.entries[entry].metadata,
            lod,
            scene.smooth || world.entries[entry].metadata.layer != 0,
        );
        // An oversized request can never fit. Reject it before evicting other
        // detail, otherwise it repeatedly flushes meshes that do fit the budget.
        if estimate > settings.budget_mb * 1024 * 1024 {
            world.budget_limited = true;
            continue;
        }
        // Reservations cannot be reclaimed from unfinished reads. Preflight
        // the whole admission before evicting anything, or a request that still
        // cannot fit can continually flush and reload smaller useful meshes.
        let releasable: usize = farthest
            .iter()
            .filter(|&&(other_distance, other)| {
                other_distance > distance + 1.0e-6 && other != entry
            })
            .map(|&(_, other)| {
                world.entries[other].bytes[..LEVELS - 1]
                    .iter()
                    .sum::<usize>()
            })
            .sum();
        if bytes
            .saturating_add(reserved)
            .saturating_add(estimate)
            .saturating_sub(releasable)
            > budget
        {
            world.budget_limited = true;
            continue;
        }
        // A newly approached chunk may displace older, more distant detail.
        for &(other_distance, other) in &farthest {
            if bytes.saturating_add(reserved).saturating_add(estimate) <= budget {
                break;
            }
            if other_distance <= distance + 1.0e-6 || other == entry {
                continue;
            }
            bytes -= release_detail(
                &mut world.entries[other],
                &mut meshes,
                &mut images,
                &mut entities,
            );
        }
        if bytes.saturating_add(reserved).saturating_add(estimate) > budget {
            world.budget_limited = true;
            continue;
        }
        if let Some(cache) = &world.cache {
            let cache = cache.clone();
            let id = world.entries[entry].metadata.id;
            world.loads.push(Load {
                entry,
                lod,
                reserved: estimate,
                task: IoTaskPool::get().spawn(async move { cache.read_surface(id, lod) }),
            });
            reserved += estimate;
        }
    }
    sync_rivers(&mut world.entries, &mut entities);
    stats.loading = world.loads.len();
    stats.resident_bytes = bytes;
    stats.coarse_bytes = coarse_bytes;
    stats.triangles = 0;
    stats.lods = [0; LEVELS];
    stats.mapped = 0;
    stats.visible = 0;
    stats.river_meshes = 0;
    stats.river_textures = 0;
    for entry in &world.entries {
        update_material(
            entry,
            &settings,
            scene.smooth,
            scene.river_display == RiverDisplay::Show,
            &mut materials,
        );
        if entry.visible {
            stats.visible += 1;
            stats.river_meshes += usize::from(entry.metadata.layer == 2);
            stats.river_textures += usize::from(
                entry.metadata.layer == 0
                    && entry.selected > 0
                    && scene.river_display == RiverDisplay::Show
                    && entry.river_atlases[entry.selected].is_some(),
            );
            if settings.surface_maps
                && scene.smooth
                && entry.selected > 0
                && entry.atlases[entry.selected].is_some()
            {
                stats.mapped += 1;
            }
            stats.triangles += entry.metadata.triangles[entry.selected];
            stats.lods[entry.selected] += 1;
        }
    }
    if world.job.is_none() && stats.status.starts_with("Streaming ready") {
        stats.status = if world.budget_limited {
            "Streaming ready · detail budget reached"
        } else {
            "Streaming ready"
        }
        .into();
    }
    if world.settled() && world.dirty {
        info!(
            "Chunks: {} visible / {}, {} horizon culled, {} triangles, LOD {:?}, {:.1} MiB resident",
            stats.visible,
            stats.total,
            stats.hidden_horizon,
            stats.triangles,
            stats.lods,
            bytes as f64 / (1024.0 * 1024.0)
        );
        world.dirty = false;
    }
}

fn sync_rivers(entries: &mut [Entry], entities: &mut Query<(&mut Mesh3d, &mut Visibility)>) {
    // Recheck after budget eviction: never leave a water mesh floating above
    // a terrain cell that has just fallen back to a textured coarse level.
    let terrain: HashMap<_, _> = entries
        .iter()
        .filter(|e| e.metadata.layer == 0)
        .map(|e| (e.metadata.id, e.selected))
        .collect();
    for entry in entries.iter_mut().filter(|e| e.metadata.layer == 2) {
        if terrain.get(&(entry.metadata.id % 100_000)) != Some(&0) {
            entry.selected = LEVELS - 1;
            entry.desired = LEVELS - 1;
            entry.visible = false;
        }
        entry.visible &= entry.selected == 0;
        if let Ok((mut mesh, mut visibility)) = entities.get_mut(entry.entity) {
            if let Some(handle) = &entry.handles[entry.selected] {
                mesh.0 = handle.clone();
            }
            *visibility = if entry.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn update_material(
    entry: &Entry,
    settings: &StreamingSettings,
    smooth: bool,
    show_rivers: bool,
    materials: &mut Assets<LodMaterial>,
) {
    if let Some(mut material) = materials.get_mut(&entry.material) {
        let colour = if settings.colours {
            cell_colour(entry.metadata.id)
        } else {
            layer_colour(entry.metadata.layer)
        };
        if material.base.base_color != colour {
            material.base.base_color = colour;
        }
        let terrain_mode = if entry.metadata.layer == 2 && !settings.colours {
            4
        } else if material.extension.terrain.radius > 0.0 {
            if settings.colours { 2 } else { 1 }
        } else {
            0
        };
        if material.extension.settings.terrain != terrain_mode {
            material.extension.settings.terrain = terrain_mode;
        }
        let tint = Vec4::from_array(colour.to_linear().to_f32_array());
        if material.extension.terrain.colour != tint {
            material.extension.terrain.colour = tint;
        }
        let map = entry.atlases[entry.selected].as_ref();
        let enabled = u32::from(
            settings.surface_maps
                && smooth
                && entry.metadata.layer == 0
                && entry.selected > 0
                && map.is_some(),
        );
        if material.extension.settings.enabled != enabled {
            material.extension.settings.enabled = enabled;
        }
        let rivers = u32::from(
            show_rivers
                && entry.metadata.layer == 0
                && entry.selected > 0
                && entry.river_atlases[entry.selected].is_some(),
        );
        if material.extension.settings.rivers != rivers {
            material.extension.settings.rivers = rivers;
        }
        let river_map = entry.river_atlases[entry.selected].as_ref();
        if material.extension.river_map.as_ref() != river_map {
            material.extension.river_map = river_map.cloned();
        }
        if material.extension.settings.debug != settings.surface_debug {
            material.extension.settings.debug = settings.surface_debug;
        }
        if material.extension.surface_map.as_ref() != map {
            material.extension.surface_map = map.cloned();
        }
    }
}

fn release_detail(
    entry: &mut Entry,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    entities: &mut Query<(&mut Mesh3d, &mut Visibility)>,
) -> usize {
    if let Ok((mut mesh, _)) = entities.get_mut(entry.entity)
        && let Some(coarse) = &entry.handles[LEVELS - 1]
    {
        mesh.0 = coarse.clone();
    }
    entry.selected = LEVELS - 1;
    let mut released = 0;
    for lod in 0..LEVELS - 1 {
        if let Some(handle) = entry.handles[lod].take() {
            meshes.remove(handle.id());
            released += entry.bytes[lod];
            entry.bytes[lod] = 0;
            if let Some(image) = entry.atlases[lod].take() {
                images.remove(image.id());
            }
            if let Some(image) = entry.river_atlases[lod].take() {
                images.remove(image.id());
            }
        }
    }
    released
}

fn resident_lod<T>(handles: &[Option<T>; LEVELS], desired: usize) -> usize {
    // A finer resident mesh satisfies the requested error while an intermediate
    // level loads; avoid briefly dropping all the way to the coarse proxy.
    (0..=desired)
        .rev()
        .find(|&lod| handles[lod].is_some())
        .or_else(|| (desired + 1..LEVELS).find(|&lod| handles[lod].is_some()))
        .unwrap_or(LEVELS - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offscreen_terrain_keeps_casting_but_horizon_culling_and_toggles_still_apply() {
        let mut app = budget_fixture(100);
        app.insert_resource(crate::lighting::LightingSettings::default());
        {
            let mut chunks = app.world_mut().resource_mut::<ChunkWorld>();
            chunks.entries[0].metadata.bounds.centre = DVec3::X * 10.0;
        }
        app.update();
        let entity = app.world().resource::<ChunkWorld>().entries[0].entity;
        assert!(!app.world().resource::<ChunkWorld>().entries[0].visible);
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Visible)
        );
        app.world_mut()
            .resource_mut::<crate::lighting::LightingSettings>()
            .shadows = false;
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Hidden)
        );
        app.world_mut()
            .resource_mut::<crate::lighting::LightingSettings>()
            .shadows = true;
        app.world_mut().resource_mut::<StreamingSettings>().horizon = true;
        {
            let mut chunks = app.world_mut().resource_mut::<ChunkWorld>();
            chunks.occluder = 0.9;
            chunks.entries[0].metadata.bounds.centre = -DVec3::Z;
            chunks.entries[0].metadata.bounds.direction = -DVec3::Z;
        }
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Hidden)
        );
    }
    #[test]
    fn detail_sources_follow_active_render_lod_and_visibility() {
        let mut app = budget_fixture(100);
        {
            let mut chunks = app.world_mut().resource_mut::<ChunkWorld>();
            chunks.entries[0].handles[0] = chunks.entries[0].handles[3].clone();
            chunks.entries[0].bytes[0] = 1000;
        }
        for requested in [0, 3, 0] {
            app.world_mut()
                .resource_mut::<StreamingSettings>()
                .forced_lod = Some(requested);
            app.update();
            let chunks = app.world().resource::<ChunkWorld>();
            assert_eq!(chunks.fine_visible(0), requested == 0);
            let sources: Vec<_> = chunks.fine_chunks_near(Vec3::Z, 0.2).collect();
            assert_eq!(sources.len(), usize::from(requested == 0));
            if let Some((_, handle, _)) = sources.first() {
                let rendered = app.world().get::<Mesh3d>(chunks.entries[0].entity).unwrap();
                assert_eq!(*handle, &rendered.0);
            }
        }
        app.world_mut().resource_mut::<ChunkWorld>().entries[0].visible = false;
        assert!(!app.world().resource::<ChunkWorld>().fine_visible(0));
    }
    #[test]
    fn pending_intermediate_lod_keeps_the_finer_resident_surface() {
        assert_eq!(resident_lod(&[Some(()), None, None, Some(())], 1), 0);
        assert_eq!(resident_lod(&[None, None, Some(()), Some(())], 1), 2);
        assert_eq!(resident_lod(&[Some(()), Some(()), None, Some(())], 1), 1);
    }

    #[test]
    fn atlas_budget_matches_uploaded_mips_and_vertex_uvs() {
        let atlas = SurfaceAtlasData {
            uv: vec![[0.5, 0.5]; 3],
            width: 32,
            height: 64,
            pixels: [128, 128, 128, 200].repeat(32 * 64),
            rivers: vec![0; 32 * 64],
        };
        let image = atlas_image(atlas.width, atlas.height, &atlas.pixels);
        assert_eq!(texture_bytes([32, 64]), image.data.unwrap().len());
        let river = river_image(atlas.width, atlas.height, &atlas.rivers);
        assert_eq!(texture_bytes([32, 64]) / 4, river.data.unwrap().len());
        let surface = CachedSurface {
            mesh: ChunkMesh {
                vertices: vec![
                    ChunkVertex {
                        position: DVec3::Z,
                        normal: DVec3::Z,
                        colour: [1.0; 4],
                        protected: false,
                    };
                    3
                ],
                triangles: vec![[0, 1, 2]],
            },
            atlas: Some(atlas),
        };
        assert_eq!(
            mesh_bytes(&surface, true),
            3 * 48 + 12 + texture_bytes([32, 64]) * 5 / 4
        );
        assert_eq!(
            mesh_bytes(&surface, false),
            3 * 48 + texture_bytes([32, 64]) * 5 / 4
        );
        assert_eq!(texture_bytes([0, 0]), 0);
    }

    #[test]
    fn only_selected_mapped_terrain_lods_enable_surface_shading() {
        let mut app = budget_fixture(100);
        let mut chunks = app.world_mut().remove_resource::<ChunkWorld>().unwrap();
        let entry = &mut chunks.entries[0];
        let mut materials = app.world_mut().resource_mut::<Assets<LodMaterial>>();
        entry.material = materials.add(LodMaterial::default());
        entry.atlases[1] = Some(Handle::default());
        for (layer, selected, maps, smooth, enabled) in [
            (0, 1, true, true, 1),
            (0, 0, true, true, 0),
            (0, 2, true, true, 0),
            (0, 1, false, true, 0),
            (0, 1, true, false, 0),
            (1, 1, true, true, 0),
            (2, 1, true, true, 0),
        ] {
            entry.metadata.layer = layer;
            entry.selected = selected;
            update_material(
                entry,
                &StreamingSettings {
                    surface_maps: maps,
                    ..default()
                },
                smooth,
                true,
                &mut materials,
            );
            let extension = &materials.get(&entry.material).unwrap().extension;
            assert_eq!(extension.settings.enabled, enabled);
            assert_eq!(extension.surface_map, entry.atlases[selected]);
        }
    }

    #[allow(clippy::too_many_lines)] // Complete ECS setup shared by the admission lifecycle regressions.
    fn budget_fixture(near_triangles: usize) -> App {
        let settings = crate::panel::Settings {
            planet: planet_rs::PlanetOptions {
                vertex_count: 32,
                ..default()
            },
            terrain: planet_rs::TerrainOptions::default(),
            mode: GenerationMode::Sphere,
        };
        let terrain = settings.generate().unwrap();
        let orbit = crate::initial_orbit(crate::command::View::Front, &terrain);
        let scene = Scene {
            terrain: terrain.into(),
            seed: 42,
            generation: GenerationMode::Sphere,
            bed: BedView::Carved,
            river_display: RiverDisplay::Show,
            no_ocean: true,
            wireframe: false,
            smooth: true,
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<LodMaterial>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<StreamingStats>()
            .insert_resource(StreamingSettings {
                budget_mb: 1,
                forced_lod: Some(0),
                horizon: false,
                ..default()
            })
            .insert_resource(scene)
            .add_systems(Update, (update, crate::update_edges).chain());
        app.world_mut().spawn((
            Camera::default(),
            Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
            Projection::Perspective(PerspectiveProjection::default()),
            orbit,
        ));
        let mut entries = Vec::new();
        for id in 0..2 {
            let coarse = app
                .world_mut()
                .resource_mut::<Assets<Mesh>>()
                .add(Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                ));
            let fine = app
                .world_mut()
                .resource_mut::<Assets<Mesh>>()
                .add(Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                ));
            let handle = if id == 0 {
                coarse.clone()
            } else {
                fine.clone()
            };
            let entity = app
                .world_mut()
                .spawn((TerrainChunk, Mesh3d(handle), Visibility::Visible))
                .id();
            entries.push(Entry {
                metadata: CachedChunk {
                    id,
                    layer: 0,
                    bounds: planet_rs::chunks::ChunkBounds {
                        centre: DVec3::Z * (1.0 - f64::from(id)),
                        radius: 0.1,
                        direction: DVec3::Z,
                        half_angle: 0.1,
                        max_radius: 1.1,
                    },
                    errors: vec![0.0, 0.01, 0.02, 0.03],
                    vertex_counts: vec![
                        near_triangles * 3,
                        near_triangles * 2,
                        near_triangles,
                        near_triangles / 2,
                    ],
                    atlas_sizes: vec![[0, 0]; LEVELS],
                    triangles: if id == 0 {
                        vec![
                            near_triangles,
                            near_triangles / 2,
                            near_triangles / 4,
                            near_triangles / 8,
                        ]
                    } else {
                        vec![100, 50, 25, 12]
                    },
                },
                entity,
                material: Handle::default(),
                handles: [(id == 1).then_some(fine), None, None, Some(coarse)],
                atlases: std::array::from_fn(|_| None),
                river_atlases: std::array::from_fn(|_| None),
                bytes: [if id == 1 { 16_000 } else { 0 }, 0, 0, 1_000],
                last_needed: [0.0; LEVELS],
                failed: [false; LEVELS],
                selected: if id == 1 { 0 } else { 3 },
                desired: 0,
                visible: true,
            });
        }
        app.insert_resource(ChunkWorld {
            rebuild: false,
            entries,
            ..default()
        });
        app
    }

    #[test]
    fn river_meshes_follow_actual_terrain_lod_and_hide_on_coarse_fallback() {
        let mut app = budget_fixture(100);
        {
            let mut chunks = app.world_mut().resource_mut::<ChunkWorld>();
            chunks.entries[0].handles[0] = chunks.entries[0].handles[3].clone();
            chunks.entries[0].bytes[0] = 1000;
            chunks.entries[1].metadata.layer = 2;
            chunks.entries[1].metadata.id = 200_000;
        }
        for (requested, river_visible) in [(0, true), (1, true), (3, false), (0, true)] {
            app.world_mut()
                .resource_mut::<StreamingSettings>()
                .forced_lod = Some(requested);
            app.update();
            let chunks = app.world().resource::<ChunkWorld>();
            let river = &chunks.entries[1];
            assert_eq!(river.visible, river_visible);
            assert_eq!(
                *app.world().get::<Visibility>(river.entity).unwrap(),
                if river_visible {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                }
            );
            if river_visible {
                assert_eq!(chunks.entries[0].selected, 0);
            }
        }
        app.world_mut().resource_mut::<Scene>().river_display = RiverDisplay::Hide;
        app.update();
        assert!(!app.world().resource::<ChunkWorld>().entries[1].visible);
    }

    #[test]
    fn empty_river_fallback_does_not_allocate_gpu_assets() {
        let mut meshes = Assets::<Mesh>::default();
        let mut images = Assets::<Image>::default();
        let (mesh, atlas, river) = upload(
            &CachedSurface {
                mesh: ChunkMesh::default(),
                atlas: None,
            },
            true,
            &mut meshes,
            &mut images,
            None,
        );
        assert_eq!(mesh, Handle::default());
        assert!(atlas.is_none() && river.is_none());
        assert!(meshes.is_empty() && images.is_empty());
    }

    #[test]
    fn river_geometry_has_no_coarse_resident_payload() {
        let source = ChunkMesh {
            vertices: vec![
                ChunkVertex {
                    position: DVec3::Z,
                    normal: DVec3::Z,
                    colour: [1.0; 4],
                    protected: true
                };
                3
            ],
            triangles: vec![[0, 1, 2]],
        };
        let levels = layer_lods(2, source.clone(), 1.0);
        assert_eq!(levels[0].mesh, source);
        assert!(
            levels[1..]
                .iter()
                .all(|level| level.mesh.vertices.is_empty() && level.mesh.triangles.is_empty())
        );
    }

    #[test]
    fn river_texture_obeys_river_visibility_independently_of_normal_maps() {
        let mut app = budget_fixture(100);
        let mut chunks = app.world_mut().remove_resource::<ChunkWorld>().unwrap();
        let entry = &mut chunks.entries[0];
        entry.selected = 1;
        entry.river_atlases[1] = Some(Handle::default());
        let mut materials = app.world_mut().resource_mut::<Assets<LodMaterial>>();
        entry.material = materials.add(LodMaterial::default());
        let settings = StreamingSettings {
            surface_maps: false,
            ..default()
        };
        for (lod, show, expected) in [(1, true, 1), (1, false, 0), (0, true, 0)] {
            entry.selected = lod;
            update_material(entry, &settings, true, show, &mut materials);
            let material = materials.get(&entry.material).unwrap();
            assert_eq!(material.extension.settings.enabled, 0);
            assert_eq!(material.extension.settings.rivers, expected);
        }
    }

    #[test]
    fn island_ground_shading_preserves_flat_chunk_colours_and_standard_water() {
        let mut app = budget_fixture(100);
        let mut chunks = app.world_mut().remove_resource::<ChunkWorld>().unwrap();
        let entry = &mut chunks.entries[0];
        let mut materials = app.world_mut().resource_mut::<Assets<LodMaterial>>();
        entry.material = materials.add(LodMaterial {
            extension: TerrainTextures::default().extension(1000.0),
            ..default()
        });
        for (colours, expected) in [(false, 1), (true, 2)] {
            update_material(
                entry,
                &StreamingSettings {
                    colours,
                    ..default()
                },
                true,
                true,
                &mut materials,
            );
            let material = materials.get(&entry.material).unwrap();
            assert_eq!(material.extension.settings.terrain, expected);
            assert_eq!(
                material.extension.terrain.colour,
                Vec4::from_array(material.base.base_color.to_linear().to_f32_array())
            );
        }
        // Water and the original sphere retain the StandardMaterial path.
        materials
            .get_mut(&entry.material)
            .unwrap()
            .extension
            .terrain
            .radius = 0.0;
        update_material(
            entry,
            &StreamingSettings::default(),
            true,
            true,
            &mut materials,
        );
        assert_eq!(
            materials
                .get(&entry.material)
                .unwrap()
                .extension
                .settings
                .terrain,
            0
        );
    }

    #[test]
    fn edge_overlay_follows_resident_lods_and_toggles_without_extra_meshes() {
        use bevy::pbr::wireframe::Wireframe;
        let mut app = budget_fixture(100_000);
        app.world_mut().resource_mut::<Scene>().wireframe = true;
        let water = app.world_mut().spawn(Mesh3d(Handle::default())).id();
        app.update();
        let mesh_count = app.world().resource::<Assets<Mesh>>().len();
        for lod in [3, 1, 0] {
            app.world_mut()
                .resource_mut::<StreamingSettings>()
                .forced_lod = Some(lod);
            app.update();
            let world = app.world().resource::<ChunkWorld>();
            for entry in &world.entries {
                let entity = app.world().entity(entry.entity);
                assert!(entity.contains::<Wireframe>());
                assert_eq!(
                    &entity.get::<Mesh3d>().unwrap().0,
                    entry.handles[entry.selected].as_ref().unwrap(),
                    "edges draw the actual resident mesh, including fine/coarse loading fallbacks"
                );
            }
            assert!(!app.world().entity(water).contains::<Wireframe>());
            assert_eq!(app.world().resource::<Assets<Mesh>>().len(), mesh_count);
        }
        app.world_mut().resource_mut::<Scene>().wireframe = false;
        app.update();
        for entry in &app.world().resource::<ChunkWorld>().entries {
            assert!(!app.world().entity(entry.entity).contains::<Wireframe>());
        }
    }

    #[test]
    fn oversized_request_keeps_other_resident_detail_and_settles_on_coarse() {
        let mut app = budget_fixture(100_000);
        app.update();
        let world = app.world().resource::<ChunkWorld>();
        assert!(
            world.entries[1].handles[0].is_some(),
            "an impossible closer request must not evict usable detail"
        );
        assert!(world.budget_limited && world.settled());
        assert_eq!(world.entries[0].selected, LEVELS - 1);
        assert!(world.loads.is_empty());
    }
    #[test]
    fn inflight_reservations_do_not_trigger_speculative_eviction() {
        let mut app = budget_fixture(5_000);
        app.world_mut()
            .resource_mut::<ChunkWorld>()
            .loads
            .push(Load {
                entry: 1,
                lod: 1,
                reserved: 600 * 1024,
                task: IoTaskPool::get().spawn(std::future::pending()),
            });
        app.update();
        let world = app.world().resource::<ChunkWorld>();
        assert!(
            world.entries[1].handles[0].is_some(),
            "in-flight reads leave too little space even after eviction; keep useful resident geometry"
        );
        assert_eq!(world.loads.len(), 1);
        assert!(world.budget_limited);
        assert_eq!(world.entries[1].selected, 0);
    }
}
