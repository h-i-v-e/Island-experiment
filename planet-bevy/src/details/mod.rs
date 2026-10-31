//! Camera-local Island detail: shared sixteen-layer fur shells and streamed rocks.
mod rocks;
mod snapshot;

use crate::{
    Orbit, Scene,
    command::{BedView, GenerationMode},
    explore::Explorer,
    lod_material::LodMaterial,
    streaming::{ChunkWorld, StreamingSettings},
    terrain_material::TerrainTextures,
};
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    camera::primitives::Aabb,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat},
    render::storage::ShaderBuffer,
    shader::ShaderRef,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use planet_rs::DVec3;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub struct DetailPlugin;
impl Plugin for DetailPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "grass.wgsl");
        app.add_plugins(MaterialPlugin::<GrassMaterial>::default())
            .init_resource::<DetailSettings>()
            .init_resource::<Details>();
    }
}

#[derive(Resource, Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DetailSettings {
    pub grass: bool,
    pub stones: bool,
    pub boulders: bool,
    pub height: f32,
    pub density: f32,
    pub rock_density: f32,
}
impl Default for DetailSettings {
    fn default() -> Self {
        Self {
            grass: true,
            stones: true,
            boulders: true,
            height: 0.14,
            density: 24.0,
            rock_density: 1.0,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct GrassSettings {
    pub player: Vec4,
    pub shape: Vec4,
    pub layer: Vec4,
}
#[derive(Asset, AsBindGroup, Clone, Debug, Default, Reflect)]
pub struct GrassExtension {
    #[uniform(115)]
    pub settings: GrassSettings,
}
pub type GrassMaterial = ExtendedMaterial<LodMaterial, GrassExtension>;
impl MaterialExtension for GrassExtension {
    fn vertex_shader() -> ShaderRef {
        "embedded://planet_bevy/details/grass.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Geometry {
    pointer: usize,
    seed: u64,
    bed: BedView,
    mode: GenerationMode,
    level: u8,
}
struct Job {
    geometry: Geometry,
    config: (bool, bool, u32),
    task: Task<rocks::Patch>,
}
struct Models {
    owner: u32,
    entity: Entity,
    mesh: Handle<Mesh>,
}
struct Active {
    patch: rocks::Patch,
    models: Vec<Models>,
    visible: BTreeSet<u32>,
    config: (bool, bool, u32),
    dirty: bool,
}
struct Shells {
    mesh: Handle<Mesh>,
    entities: Vec<Entity>,
}
#[derive(Resource, Default)]
pub struct Details {
    geometry: Option<Geometry>,
    job: Option<Job>,
    active: Option<Active>,
    grass: BTreeMap<u32, Shells>,
    materials: Vec<Handle<GrassMaterial>>,
    rock_material: Option<Handle<LodMaterial>>,
    pub stones: usize,
    pub boulders: usize,
    pub grass_patches: usize,
    upper: f64,
}
impl Details {
    pub fn settled(&self) -> bool {
        self.job.is_none()
    }
}

pub fn mask_image(pixels: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: 512,
            height: 512,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}
fn mesh(data: rocks::MeshData) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data.colours);
    mesh.insert_indices(Indices::U32(data.indices));
    mesh
}
fn clear_shells(world: &mut Details, commands: &mut Commands) {
    for (_, shells) in std::mem::take(&mut world.grass) {
        for entity in shells.entities {
            commands.entity(entity).despawn();
        }
    }
    world.grass_patches = 0;
}
fn clear_models(world: &mut Details, commands: &mut Commands, meshes: &mut Assets<Mesh>) {
    if let Some(active) = world.active.take() {
        for model in active.models {
            commands.entity(model.entity).despawn();
            meshes.remove(model.mesh.id());
        }
    }
    world.stones = 0;
    world.boulders = 0;
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_possible_truncation
)]
pub fn update(
    mut commands: Commands,
    scene: Res<Scene>,
    settings: Res<DetailSettings>,
    streaming: Res<StreamingSettings>,
    chunks: Res<ChunkWorld>,
    mut textures: ResMut<TerrainTextures>,
    mut explorer: ResMut<Explorer>,
    mut world: ResMut<Details>,
    cameras: Query<&Transform, With<Orbit>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<LodMaterial>>,
    mut grass_materials: ResMut<Assets<GrassMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut frame_buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let geometry = Geometry {
        pointer: scene.terrain.elevations.as_ptr() as usize,
        seed: scene.seed,
        bed: scene.bed,
        mode: scene.generation,
        level: chunks.partition_level(),
    };
    if world.geometry != Some(geometry) {
        clear_shells(&mut world, &mut commands);
        clear_models(&mut world, &mut commands, &mut meshes);
        world.job = None;
        world.geometry = Some(geometry);
        explorer.clear_detail_rocks();
        world.upper = 1.0 + scene.terrain.elevations.iter().copied().fold(0.0, f64::max);
        if let Some(mut image) = images.get_mut(&textures.detail_mask) {
            *image = mask_image(vec![0; 512 * 512 * 4]);
        }
        for handle in world.materials.drain(..) {
            grass_materials.remove(handle.id());
        }
        if let Some(handle) = world.rock_material.take() {
            materials.remove(handle.id());
        }
    }
    let radius = scene.terrain.topology.radius;
    let eye = camera.translation;
    // No local mesh work while looking at the whole planet from orbit.
    let upper = world.upper;
    let far = f64::from(eye.length()) > upper + 160.0 / radius
        || scene.generation != GenerationMode::Terrain;
    if !settings.stones && !settings.boulders && (world.active.is_some() || world.job.is_some()) {
        clear_models(&mut world, &mut commands, &mut meshes);
        world.job = None;
        explorer.clear_detail_rocks();
        if let Some(mut image) = images.get_mut(&textures.detail_mask) {
            *image = mask_image(vec![0; 512 * 512 * 4]);
        }
    }
    if far || (!settings.grass && !settings.stones && !settings.boulders) {
        clear_shells(&mut world, &mut commands);
        if let Some(active) = &world.active {
            for model in &active.models {
                commands.entity(model.entity).insert(Visibility::Hidden);
            }
        }
        return;
    }
    let ground = explorer.terrain_ground(&scene);
    let Some(floor) = ground.radius(&scene.terrain, eye.normalize()) else {
        return;
    };
    let foot = eye.normalize() * floor;
    if f64::from(eye.distance(foot)) * radius > 160.0 {
        clear_shells(&mut world, &mut commands);
        return;
    }
    if world.materials.is_empty() {
        for layer in 1..=16_u8 {
            world.materials.push(grass_materials.add(GrassMaterial {
                base: LodMaterial {
                    base: StandardMaterial {
                        alpha_mode: AlphaMode::Mask(0.5),
                        cull_mode: None,
                        double_sided: true,
                        perceptual_roughness: 0.95,
                        ..default()
                    },
                    extension: textures.extension(radius as f32),
                },
                extension: GrassExtension {
                    settings: GrassSettings {
                        layer: Vec4::new(f32::from(layer) / 16.0, 0.0, 0.0, 0.0),
                        ..default()
                    },
                },
            }));
        }
        let mut extension = textures.extension(radius as f32);
        extension.settings.terrain = 3;
        world.rock_material = Some(materials.add(LodMaterial {
            base: StandardMaterial {
                perceptual_roughness: 0.9,
                ..default()
            },
            extension,
        }));
    }
    let config = (
        settings.stones,
        settings.boulders,
        settings.rock_density.to_bits(),
    );
    let centre = DVec3::from_array(foot.to_array().map(f64::from));
    let needs_patch = (settings.stones || settings.boulders)
        && world.active.as_ref().is_none_or(|active| {
            active.config != config || active.patch.centre.distance(centre) * radius > 12.0
        });
    if needs_patch && world.job.is_none() {
        // The worker retains this generation and its immutable BVH. Movement
        // keeps using the same index without copying or locking the terrain.
        let terrain = Arc::clone(&scene.terrain);
        let query = ground.terrain_query();
        let original = scene.bed == BedView::Original;
        let seed = scene.seed;
        let level = chunks.partition_level();
        let options = *settings;
        world.job = Some(Job {
            geometry,
            config,
            task: AsyncComputeTaskPool::get().spawn(async move {
                let faces = query.nearby_faces(&terrain, foot, (150.0 / radius) as f32);
                let snapshot = snapshot::Snapshot::new(&terrain, original, &faces);
                rocks::build(snapshot, centre, radius, seed, level, options)
            }),
        });
    }
    if let Some(job) = &mut world.job
        && let Some(mut patch) = block_on(poll_once(&mut job.task))
    {
        let matching = job.geometry == geometry && job.config == config;
        world.job = None;
        if matching {
            clear_models(&mut world, &mut commands, &mut meshes);
            let mut models = Vec::new();
            for group in std::mem::take(&mut patch.groups) {
                if group.mesh.indices.is_empty() {
                    continue;
                }
                let handle = meshes.add(mesh(group.mesh));
                let entity = commands
                    .spawn((
                        Mesh3d(handle.clone()),
                        MeshMaterial3d(world.rock_material.as_ref().unwrap().clone()),
                        Visibility::Hidden,
                    ))
                    .id();
                models.push(Models {
                    owner: group.owner,
                    entity,
                    mesh: handle,
                });
            }
            world.stones = patch.stones;
            world.boulders = patch.boulders;
            info!(
                "Surface detail: {} stones, {} large boulders in {} nearby cells",
                patch.stones,
                patch.boulders,
                models.len()
            );
            world.active = Some(Active {
                patch,
                models,
                visible: BTreeSet::new(),
                config,
                dirty: true,
            });
        }
    }
    let mut mapping_update = None;
    if let Some(active) = &mut world.active {
        let visible: BTreeSet<_> = active
            .models
            .iter()
            .filter(|m| chunks.fine_visible(m.owner))
            .map(|m| m.owner)
            .collect();
        for model in &active.models {
            commands
                .entity(model.entity)
                .insert(if visible.contains(&model.owner) {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                });
        }
        if active.visible != visible || active.dirty {
            active.dirty = false;
            active.visible = visible;
            let pixels = rocks::mask(&active.patch, radius, |owner| {
                active.visible.contains(&owner)
            });
            if let Some(mut image) = images.get_mut(&textures.detail_mask) {
                *image = mask_image(pixels);
            }
            // The contact list is tiny and owned by the collision index; copying
            // these spheres avoids retaining renderer objects in movement code.
            ground.set_rocks(
                active
                    .patch
                    .colliders
                    .iter()
                    .filter(|(owner, _)| active.visible.contains(owner))
                    .map(|(_, collider)| *collider)
                    .collect(),
            );
            mapping_update = Some(active.patch.mapping);
        }
    }
    if let Some(mapping) = mapping_update {
        // One 48-byte write updates every terrain/grass shader without
        // marking hundreds of materials dirty and rebuilding their bindings.
        textures.set_detail_mapping(mapping, &mut frame_buffers);
    }

    let grass_enabled = settings.grass
        && !streaming.colours
        && streaming.surface_debug == 0
        && f64::from(eye.distance(foot)) * radius < 30.0;
    let selected: Vec<_> = if grass_enabled {
        chunks
            .fine_chunks_near(foot, 20.0 / radius)
            .map(|(id, handle, bounds)| (id, handle.clone(), bounds))
            .collect()
    } else {
        Vec::new()
    };
    let mut old = std::mem::take(&mut world.grass);
    for (id, handle, bounds) in selected {
        if let Some(shells) = old.remove(&id) {
            if shells.mesh == handle {
                world.grass.insert(id, shells);
                continue;
            }
            for entity in shells.entities {
                commands.entity(entity).despawn();
            }
        }
        let mut entities = Vec::with_capacity(16);
        for material in &world.materials {
            let centre = crate::surface::vector(bounds.centre);
            let half = Vec3::splat((bounds.radius + 0.6 / radius) as f32);
            entities.push(
                commands
                    .spawn((
                        Mesh3d(handle.clone()),
                        MeshMaterial3d(material.clone()),
                        NotShadowCaster,
                        Aabb::from_min_max(centre - half, centre + half),
                    ))
                    .id(),
            );
        }
        world.grass.insert(
            id,
            Shells {
                mesh: handle,
                entities,
            },
        );
    }
    for (_, shells) in old {
        for entity in shells.entities {
            commands.entity(entity).despawn();
        }
    }
    world.grass_patches = world.grass.len();
    for handle in &world.materials {
        if let Some(mut material) = grass_materials.get_mut(handle) {
            material.extension.settings.player = foot.extend(radius as f32);
            material.extension.settings.shape =
                Vec4::new(settings.height, settings.density, 20.0, 10.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let scene = crate::collision::test_scene();
        let orbit = crate::initial_orbit(crate::command::View::Front, &scene.terrain);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(scene)
            .init_resource::<DetailSettings>()
            .init_resource::<Details>()
            .init_resource::<StreamingSettings>()
            .init_resource::<ChunkWorld>()
            .init_resource::<TerrainTextures>()
            .init_resource::<Explorer>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<LodMaterial>>()
            .init_resource::<Assets<GrassMaterial>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<ShaderBuffer>>()
            .add_systems(Update, update);
        app.world_mut()
            .spawn((orbit, Transform::from_xyz(0.0, 3.0, 0.0)));
        app
    }

    #[test]
    fn generation_change_removes_old_meshes_shells_and_pending_jobs() {
        let mut app = app();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            ));
        let rock = app.world_mut().spawn_empty().id();
        let shell = app.world_mut().spawn_empty().id();
        let old_geometry = Geometry {
            pointer: 0,
            seed: 0,
            bed: BedView::Original,
            mode: GenerationMode::Terrain,
            level: 0,
        };
        let task = AsyncComputeTaskPool::get().spawn(std::future::pending());
        let mut details = app.world_mut().resource_mut::<Details>();
        details.geometry = Some(old_geometry);
        details.job = Some(Job {
            geometry: old_geometry,
            config: (true, true, 1),
            task,
        });
        details.grass.insert(
            0,
            Shells {
                mesh: mesh.clone(),
                entities: vec![shell],
            },
        );
        details.active = Some(Active {
            patch: rocks::Patch {
                centre: DVec3::Y,
                groups: Vec::new(),
                colliders: Vec::new(),
                placements: Vec::new(),
                mapping: default(),
                stones: 1,
                boulders: 1,
            },
            models: vec![Models {
                owner: 0,
                entity: rock,
                mesh: mesh.clone(),
            }],
            visible: BTreeSet::new(),
            config: (true, true, 1),
            dirty: false,
        });
        app.update();
        assert!(app.world().get_entity(rock).is_err());
        assert!(app.world().get_entity(shell).is_err());
        assert!(app.world().resource::<Assets<Mesh>>().get(&mesh).is_none());
        let details = app.world().resource::<Details>();
        assert!(details.active.is_none() && details.job.is_none() && details.grass.is_empty());
        assert_eq!(
            (details.stones, details.boulders, details.grass_patches),
            (0, 0, 0)
        );
    }

    #[test]
    fn disabling_rocks_cancels_a_pending_patch_even_before_it_finishes() {
        let mut app = app();
        app.update();
        let geometry = app.world().resource::<Details>().geometry.unwrap();
        let task = AsyncComputeTaskPool::get().spawn(std::future::pending());
        app.world_mut().resource_mut::<Details>().job = Some(Job {
            geometry,
            config: (true, true, 1),
            task,
        });
        let mut settings = app.world_mut().resource_mut::<DetailSettings>();
        settings.stones = false;
        settings.boulders = false;
        app.update();
        assert!(app.world().resource::<Details>().settled());
    }
}
