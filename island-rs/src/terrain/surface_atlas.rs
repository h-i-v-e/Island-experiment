//! Optional native-renderer surface atlases. Source meshes and serialized island
//! data keep their existing terrain UVs; only the returned mesh is unwrapped.
use std::sync::{Arc, OnceLock};

use rayon::prelude::*;

use super::surface_maps::{OCCLUSION_OFFSETS, signed_normal_byte};
use super::{Island, Mesh, SurfaceMaps, TERRAIN_RENDER_FLOOR, Terrain, Vec2, Vec3};
use crate::BoundingBox;
use crate::{forest::ForestMeshKind, profiling::StageTimer};

const PADDING: u32 = 8;
const EMPTY: u32 = u32::MAX;
const MAX_TEXTURE_DIMENSION: u32 = 8192;
// Keep the same physical AO radius across atlas resolutions and LODs: one
// sample step is about one metre on the 2 km island (legacy 2048px spacing).
const AO_SAMPLE_SPACING: f32 = 1.0 / 2047.0;

/// One whole-island LOD mesh and its linear normal/occlusion atlas.
///
/// `mesh.uv` contains atlas coordinates. Normals use absolute island-space XYZ
/// encoded from [-1, 1] into RGB8, unlike the legacy surface-map corrections.
/// Chart gutters are dilated by eight texels. Use clamp sampling and restrict
/// mip footprints to those gutters; the viewer uses only mip levels 0 to 2.
/// An unrestricted mip chain would mix unrelated charts.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceAtlas {
    pub mesh: Mesh,
    pub maps: SurfaceMaps,
}

/// Derived renderer data is lazy and excluded from snapshot/semantic equality.
#[derive(Clone, Debug, Default)]
pub(super) struct SurfaceAtlasCache {
    lods: [OnceLock<Result<Arc<SurfaceAtlas>, String>>; 2],
}

impl PartialEq for SurfaceAtlasCache {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Island {
    pub(crate) const fn surface_atlas_dimension(lod: usize) -> Option<u32> {
        match lod {
            1 => Some(2048),
            2 => Some(1024),
            _ => None,
        }
    }

    pub(crate) fn cached_surface_atlas(&self, lod: usize) -> Result<&Arc<SurfaceAtlas>, &str> {
        let dimension = Self::surface_atlas_dimension(lod)
            .ok_or("surface atlases support only LOD1 and LOD2")?;
        self.surface_atlases.lods[lod - 1]
            .get_or_init(|| {
                let target = self.lod(lod).ok_or("target LOD is unavailable")?;
                // Cave lips generate new island-space UVs. Cut the whole island
                // before unwrapping so those vertices acquire valid atlas charts.
                let render_mesh = self
                    .caves
                    .cut_surface(target.clone().clipped_above(TERRAIN_RENDER_FLOOR));
                bake_atlas(
                    &self.terrain,
                    &render_mesh,
                    self.forest.mesh(ForestMeshKind::Foliage, 2),
                    dimension,
                )
                .map(Arc::new)
            })
            .as_ref()
            .map_err(String::as_str)
    }

    pub(crate) fn terrain_atlas_mesh_grid(
        &self,
        lod: usize,
        bounds: BoundingBox,
        divisions: usize,
        clamp_sides: u8,
    ) -> Result<Vec<Mesh>, &str> {
        let atlas = self.cached_surface_atlas(lod)?;
        let tiles = self.lod(lod + 1).filter(|_| clamp_sides != 0).map_or_else(
            || {
                atlas
                    .mesh
                    .sliced_grid_preserving_uv_seams(bounds, divisions)
            },
            |coarser| {
                atlas.mesh.sliced_grid_clamped_preserving_uv_seams(
                    bounds,
                    divisions,
                    coarser,
                    clamp_sides,
                )
            },
        );
        Ok(tiles
            .into_iter()
            .map(|tile| tile.clipped_above(TERRAIN_RENDER_FLOOR))
            .collect())
    }

    /// Unwraps final LOD1 or LOD2 before chunk clipping and bakes LOD0 detail.
    ///
    /// The returned texture can be cropped vertically or contain several packed
    /// pages; read its actual size from `maps`. Source geometry and snapshots
    /// retain their original terrain UVs.
    ///
    /// # Errors
    /// Returns an error for unsupported LODs, invalid dimensions, unwrap
    /// failure, or layouts exceeding the supported texture dimensions.
    pub fn surface_atlas(&self, lod: usize, dimension: u32) -> Result<SurfaceAtlas, String> {
        if !matches!(lod, 1 | 2) {
            return Err("surface atlases support only LOD1 and LOD2".into());
        }
        let target = self.lod(lod).ok_or("target LOD is unavailable")?;
        bake_atlas(
            &self.terrain,
            target,
            self.forest.mesh(ForestMeshKind::Foliage, 2),
            dimension,
        )
    }
}

fn validate_dimension(dimension: u32) -> Result<(), String> {
    if !(32..=8192).contains(&dimension) {
        return Err("surface atlas dimension must be between 32 and 8192".into());
    }
    Ok(())
}

/// Keep every page at its original texel density and padding. Prefer the
/// smallest texture area, then the most square layout, within the upload limit.
fn combined_page_size(width: u32, height: u32, pages: u32) -> Result<[u32; 2], String> {
    if width == 0 || height == 0 || pages == 0 || height > MAX_TEXTURE_DIMENSION {
        return Err("terrain unwrap returned invalid page dimensions".into());
    }
    (1..=pages.min(MAX_TEXTURE_DIMENSION / width))
        .map(|columns| (columns, pages.div_ceil(columns)))
        .filter(|&(_, rows)| rows <= MAX_TEXTURE_DIMENSION / height)
        .map(|(columns, rows)| [columns * width, rows * height])
        .min_by_key(|&[width, height]| (u64::from(width) * u64::from(height), width.max(height)))
        .ok_or_else(|| {
            format!(
                "terrain unwrap needs {pages} pages of {width}x{height}px, exceeding the {MAX_TEXTURE_DIMENSION}px texture limit"
            )
        })
}

fn unwrap_mesh(source: &Mesh, dimension: u32) -> Result<(Mesh, Vec<u32>, [u32; 2]), String> {
    validate_dimension(dimension)?;
    if source.vertices.is_empty() || source.triangles.is_empty() {
        return Err("cannot unwrap an empty terrain mesh".into());
    }
    let positions: Vec<_> = source.vertices.iter().map(Vec3::to_array).collect();
    let options = uvunwrap::Options {
        density: uvunwrap::Density::Auto {
            page_size: dimension,
        },
        padding: PADDING,
        max_chart_angle: 65.0,
        max_area_distortion: 2.0,
        packing: uvunwrap::Packing::Fast,
        ..Default::default()
    };
    let atlas = uvunwrap::generate(
        &[uvunwrap::MeshInput::new(&positions, &source.triangles)],
        &options,
    )
    .map_err(|error| format!("terrain UV unwrap failed: {error}"))?;
    let size = combined_page_size(atlas.width, atlas.height, atlas.page_count)?;
    let columns = size[0] / atlas.width;
    let page_scale = Vec2::new(
        atlas.width as f32 / size[0] as f32,
        atlas.height as f32 / size[1] as f32,
    );
    let mut output = atlas
        .meshes
        .into_iter()
        .next()
        .ok_or("unwrap returned no mesh")?;
    let mut mesh = Mesh {
        vertices: Vec::with_capacity(output.vertices.len()),
        normals: Vec::with_capacity(output.vertices.len()),
        triangles: std::mem::take(&mut output.indices),
        uv: Vec::with_capacity(output.vertices.len()),
    };
    let mut charts = Vec::with_capacity(output.vertices.len());
    for vertex in output.vertices {
        let original = vertex.xref as usize;
        mesh.vertices.push(source.vertices[original]);
        mesh.normals
            .push(source.normals.get(original).copied().unwrap_or(Vec3::Z));
        let page = vertex.page.unwrap_or(0);
        let offset = Vec2::new((page % columns) as f32, (page / columns) as f32);
        mesh.uv
            .push((Vec2::from_array(vertex.uv) + offset) * page_scale);
        charts.push(vertex.chart.unwrap_or(EMPTY));
    }
    Ok((mesh, charts, size))
}

fn bake_atlas(
    high_detail: &Terrain,
    target: &Mesh,
    canopy: Option<&Mesh>,
    dimension: u32,
) -> Result<SurfaceAtlas, String> {
    let _timer = StageTimer::new("surface_atlas.bake");
    validate_dimension(dimension)?;
    if target.triangles.is_empty() {
        // Completely submerged islands can have no terrain above the render
        // floor. Keep their coarse atlas contract valid without unwrapping an
        // empty surface or allocating a full-resolution blank texture.
        return Ok(SurfaceAtlas {
            mesh: Mesh::default(),
            maps: SurfaceMaps {
                width: dimension,
                height: 1,
                normal_rgb: [127, 127, 255].repeat(dimension as usize),
                occlusion: vec![255; dimension as usize],
            },
        });
    }
    let (mesh, charts, [width, height]) = unwrap_mesh(target, dimension)?;
    let maps = bake_unwrapped_maps(
        high_detail,
        &mesh,
        &charts,
        canopy,
        width,
        height,
        dimension,
    );
    Ok(SurfaceAtlas { mesh, maps })
}

#[derive(Clone, Copy)]
struct AtlasTriangle {
    pixel_origin: glam::DVec2,
    barycentric_x: glam::DVec2,
    barycentric_y: glam::DVec2,
    world: [Vec2; 3],
    subpixel_height: bool,
}

impl AtlasTriangle {
    fn new(uv: [Vec2; 3], world: [Vec2; 3], scale: Vec2) -> Option<Self> {
        let pixels = uv.map(|vertex| (vertex * scale - Vec2::splat(0.5)).as_dvec2());
        let first = pixels[1] - pixels[0];
        let second = pixels[2] - pixels[0];
        let area = first.perp_dot(second);
        if !area.is_finite() || area == 0.0 {
            return None;
        }
        let longest = first
            .length()
            .max(second.length())
            .max((second - first).length());
        Some(Self {
            pixel_origin: pixels[0],
            barycentric_x: glam::DVec2::new(second.y, -first.y) / area,
            barycentric_y: glam::DVec2::new(-second.x, first.x) / area,
            world,
            subpixel_height: area.abs() < longest,
        })
    }

    fn world_point(self, x: usize, y: usize) -> Vec2 {
        let delta = glam::DVec2::new(x as f64, y as f64) - self.pixel_origin;
        let bc = self.barycentric_x * delta.x + self.barycentric_y * delta.y;
        let mut weights = [1.0 - bc.x - bc.y, bc.x, bc.y];
        if self.subpixel_height {
            // A texel outside a very thin face can extrapolate kilometres away.
            // Such a face cannot resolve that axis: extend its boundary values
            // instead, while normal charts retain affine, seamless gutters.
            weights = weights.map(|weight| weight.max(0.0));
            let sum: f64 = weights.iter().sum();
            weights = weights.map(|weight| weight / sum);
        }
        (self.world[0].as_dvec2() * weights[0]
            + self.world[1].as_dvec2() * weights[1]
            + self.world[2].as_dvec2() * weights[2])
            .as_vec2()
    }
}

fn bake_unwrapped_maps(
    high_detail: &Terrain,
    mesh: &Mesh,
    charts: &[u32],
    canopy: Option<&Mesh>,
    width: u32,
    height: u32,
    canopy_dimension: u32,
) -> SurfaceMaps {
    let width_usize = width as usize;
    let height_usize = height as usize;
    let pixel_count = width_usize * height_usize;
    let scale = Vec2::new(width as f32, height as f32);
    let mut triangles = Vec::with_capacity(mesh.triangles.len() / 3);
    let mut owners = vec![EMPTY; pixel_count];
    for triangle in mesh.triangles.chunks_exact(3) {
        let indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let uv = indices.map(|index| mesh.uv[index]);
        let world = indices.map(|index| mesh.vertices[index].truncate());
        let owner = triangles.len() as u32;
        triangles.push(AtlasTriangle::new(uv, world, scale));
        if charts[indices[0]] != EMPTY && triangles[owner as usize].is_some() {
            rasterize(uv, width_usize, height_usize, |pixel, _| {
                owners[pixel] = owner;
            });
        }
    }
    // Seed every intersected texel, including charts too small to contain a
    // pixel centre. Preserve all exact interior owners before claiming borders.
    for (owner, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
        if triangles[owner].is_none() || charts[triangle[0] as usize] == EMPTY {
            continue;
        }
        let uv = [triangle[0], triangle[1], triangle[2]].map(|index| mesh.uv[index as usize]);
        rasterize_conservative(uv, width_usize, height_usize, |pixel| {
            if owners[pixel] == EMPTY {
                owners[pixel] = owner as u32;
            }
        });
    }
    // Extend triangle ownership rather than copying border pixels. Sampling
    // each gutter through its source face's affine UV->world mapping keeps
    // bilinear filtering continuous on both copies of a chart boundary.
    dilate_triangle_owners(&mut owners, width_usize, height_usize, PADDING as u8);
    // Canopy sampling follows the requested page resolution rather than the
    // combined texture width, so adding pages does not change its world scale.
    let canopy_dimension = canopy_dimension as usize;
    let canopy_mask = canopy.map(|canopy| canopy_mask(canopy, canopy_dimension));
    let mut maps = SurfaceMaps {
        width,
        height,
        normal_rgb: [127, 127, 255].repeat(pixel_count),
        occlusion: vec![255; pixel_count],
    };
    maps.normal_rgb
        .par_chunks_exact_mut(3)
        .zip(maps.occlusion.par_iter_mut())
        .enumerate()
        .for_each(|(pixel, (normal_rgb, occlusion))| {
            let owner = owners[pixel];
            if owner == EMPTY {
                return;
            }
            let Some(triangle) = triangles[owner as usize] else {
                return;
            };
            let point = triangle
                .world_point(pixel % width_usize, pixel / width_usize)
                .clamp(Vec2::ZERO, Vec2::ONE);
            let (elevation, normal) = high_detail.sample_atlas_surface(point.x, point.y);
            normal_rgb.copy_from_slice(&normal.to_array().map(signed_normal_byte));
            *occlusion =
                world_occlusion(high_detail, Vec3::new(point.x, point.y, elevation), normal);
            if let Some(mask) = &canopy_mask {
                let x = (point.x * (canopy_dimension - 1) as f32).round() as usize;
                let y = (point.y * (canopy_dimension - 1) as f32).round() as usize;
                *occlusion = (*occlusion).min(mask[y * canopy_dimension + x]);
            }
        });
    maps
}

fn world_occlusion(terrain: &Terrain, position: Vec3, normal: Vec3) -> u8 {
    let mut total = 0.0;
    let mut count = 0_u32;
    for (x, y) in OCCLUSION_OFFSETS {
        let neighbour = position.truncate() + Vec2::new(x as f32, y as f32) * AO_SAMPLE_SPACING;
        if neighbour.min_element() < 0.0 || neighbour.max_element() > 1.0 {
            continue;
        }
        let elevation = terrain.sample_atlas_surface(neighbour.x, neighbour.y).0;
        let direction = Vec3::new(neighbour.x, neighbour.y, elevation) - position;
        if let Some(direction) = direction.try_normalize() {
            total += direction.dot(normal).max(0.0);
            count += 1;
        }
    }
    if count == 0 {
        255
    } else {
        ((1.0 - total / count as f32) * 255.0).clamp(0.0, 255.0) as u8
    }
}

/// Rasterizes UVs at pixel centres; weights always reference the input order.
fn rasterize(mut uv: [Vec2; 3], width: usize, height: usize, mut visit: impl FnMut(usize, Vec3)) {
    let scale = Vec2::new(width as f32, height as f32);
    for vertex in &mut uv {
        *vertex = *vertex * scale - Vec2::splat(0.5);
    }
    let edge = |a: Vec2, b: Vec2, p: Vec2| (b - a).perp_dot(p - a);
    let area = edge(uv[0], uv[1], uv[2]);
    if !area.is_finite() || area.abs() < 1.0e-7 {
        return;
    }
    let min = uv[0].min(uv[1]).min(uv[2]).floor().max(Vec2::ZERO);
    let max = uv[0]
        .max(uv[1])
        .max(uv[2])
        .ceil()
        .min(Vec2::new((width - 1) as f32, (height - 1) as f32));
    for y in min.y as usize..=max.y as usize {
        for x in min.x as usize..=max.x as usize {
            let point = Vec2::new(x as f32, y as f32);
            let weights = Vec3::new(
                edge(uv[1], uv[2], point),
                edge(uv[2], uv[0], point),
                edge(uv[0], uv[1], point),
            ) / area;
            if weights.min_element() >= -1.0e-5 {
                visit(y * width + x, weights);
            }
        }
    }
}

/// Triangle-square overlap covers subpixel charts and their complete filtering
/// footprint. The edge-normal test works for either triangle winding.
fn rasterize_conservative(
    uv: [Vec2; 3],
    width: usize,
    height: usize,
    mut visit: impl FnMut(usize),
) {
    let pixels = uv.map(|point| point * Vec2::new(width as f32, height as f32) - Vec2::splat(0.5));
    let area = (pixels[1] - pixels[0]).perp_dot(pixels[2] - pixels[0]);
    if !area.is_finite() || area == 0.0 {
        return;
    }
    let min = pixels[0].min(pixels[1]).min(pixels[2]);
    let max = pixels[0].max(pixels[1]).max(pixels[2]);
    let first = (min - Vec2::splat(0.5)).ceil().max(Vec2::ZERO);
    let last = (max + Vec2::splat(0.5))
        .floor()
        .min(Vec2::new((width - 1) as f32, (height - 1) as f32));
    for y in first.y as usize..=last.y as usize {
        for x in first.x as usize..=last.x as usize {
            let centre = Vec2::new(x as f32, y as f32);
            if (0..3).all(|edge| {
                let a = pixels[edge];
                let direction = pixels[(edge + 1) % 3] - a;
                direction.perp_dot(centre - a) * area.signum()
                    >= -0.5 * (direction.x.abs() + direction.y.abs()) - 1.0e-5
            }) {
                visit(y * width + x);
            }
        }
    }
}

fn canopy_mask(canopy: &Mesh, dimension: usize) -> Vec<u8> {
    let mut mask = vec![255; dimension * dimension];
    for triangle in canopy.triangles.chunks_exact(3) {
        let uv = [triangle[0], triangle[1], triangle[2]]
            .map(|index| canopy.vertices[index as usize].truncate());
        rasterize(uv, dimension, dimension, |pixel, _| mask[pixel] = 112);
    }
    mask
}

/// Multi-source dilation only claims empty texels, preserving every chart
/// interior. Source triangle IDs retain the appropriate UV-to-world mapping.
fn dilate_triangle_owners(owners: &mut [u32], width: usize, height: usize, padding: u8) {
    let neighbours = |pixel: usize| {
        let x = pixel % width;
        let y = pixel / width;
        [
            x.checked_sub(1).map(|_| pixel - 1),
            (x + 1 < width).then_some(pixel + 1),
            y.checked_sub(1).map(|_| pixel - width),
            (y + 1 < height).then_some(pixel + width),
            (x > 0 && y > 0).then(|| pixel - width - 1),
            (x + 1 < width && y > 0).then(|| pixel - width + 1),
            (x > 0 && y + 1 < height).then(|| pixel + width - 1),
            (x + 1 < width && y + 1 < height).then(|| pixel + width + 1),
        ]
    };
    let mut frontier: Vec<_> = owners
        .iter()
        .enumerate()
        .filter(|(pixel, owner)| {
            **owner != EMPTY
                && neighbours(*pixel)
                    .into_iter()
                    .flatten()
                    .any(|neighbour| owners[neighbour] == EMPTY)
        })
        .map(|(pixel, _)| pixel)
        .collect();
    for _ in 0..padding {
        let mut next = Vec::new();
        for pixel in frontier {
            for neighbour in neighbours(pixel).into_iter().flatten() {
                if owners[neighbour] != EMPTY {
                    continue;
                }
                owners[neighbour] = owners[pixel];
                next.push(neighbour);
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BoundingBox;

    fn slope_mesh() -> Mesh {
        let normal = Vec3::new(-4.0, 0.0, 1.0).normalize();
        Mesh {
            vertices: vec![
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 4.0),
                Vec3::new(1.0, 1.0, 4.0),
                Vec3::Y,
            ],
            normals: vec![normal; 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y],
        }
    }

    #[test]
    fn combined_pages_keep_cropped_single_pages_and_bound_texture_allocation() {
        assert_eq!(combined_page_size(1024, 829, 1).unwrap(), [1024, 829]);
        assert_eq!(combined_page_size(1024, 1024, 2).unwrap(), [1024, 2048]);
        assert_eq!(combined_page_size(1024, 1024, 3).unwrap(), [1024, 3072]);
        assert_eq!(combined_page_size(1024, 1024, 4).unwrap(), [2048, 2048]);
        assert_eq!(combined_page_size(1024, 1024, 64).unwrap(), [8192, 8192]);
        for (width, height, pages) in [
            (0, 32, 1),
            (32, 0, 1),
            (32, 32, 0),
            (1024, 1024, 65),
            (8192, 8192, u32::MAX),
        ] {
            assert!(combined_page_size(width, height, pages).is_err());
        }
    }

    #[test]
    fn fragmented_mesh_bakes_every_page_with_separate_uvs_and_correct_normals() {
        let normals = [
            Vec3::new(-1.0, 0.0, 1.0).normalize(),
            Vec3::new(0.0, -1.0, 1.0).normalize(),
        ];
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.4, 0.0, 0.4),
                Vec3::new(0.0, 0.4, 0.0),
                Vec3::new(0.6, 0.6, 0.0),
                Vec3::new(1.0, 0.6, 0.0),
                Vec3::new(0.6, 1.0, 0.4),
            ],
            normals: normals.into_iter().flat_map(|normal| [normal; 3]).collect(),
            triangles: vec![0, 1, 2, 3, 4, 5],
            uv: vec![Vec2::ZERO; 6],
        };
        // Two disconnected charts, each with an eight-pixel gutter, cannot
        // share a 32px page even at the lowest density. This exercises the
        // actual multipage unwrap path that used to stop Unity generation.
        let atlas = bake_atlas(&Terrain::new(mesh.clone()), &mesh, None, 32).unwrap();
        let width = atlas.maps.width();
        let height = atlas.maps.height();
        assert_eq!([width, height], [32, 64]);
        assert_eq!(atlas.mesh.triangles.len(), mesh.triangles.len());
        let mut pages = Vec::new();
        for (face, indices) in atlas.mesh.triangles.chunks_exact(3).enumerate() {
            let uv =
                [indices[0], indices[1], indices[2]].map(|index| atlas.mesh.uv[index as usize]);
            let centre = (uv[0] + uv[1] + uv[2]) / 3.0;
            pages.push((centre.y * 2.0).floor() as u32);
            let expected = normals[face].to_array().map(signed_normal_byte);
            let mut samples = 0;
            rasterize(uv, width as usize, height as usize, |pixel, _| {
                assert_eq!(
                    &atlas.maps.normal_rgb()[pixel * 3..pixel * 3 + 3],
                    &expected
                );
                samples += 1;
            });
            assert!(
                samples > 0,
                "each page must retain rasterized chart interiors"
            );
        }
        pages.sort_unstable();
        assert_eq!(pages, [0, 1]);
        // Grid clipping must keep the remapped UVs on both atlas pages.
        let tiles = atlas
            .mesh
            .sliced_grid_preserving_uv_seams(BoundingBox::default(), 2);
        assert_eq!(
            tiles.iter().map(|tile| tile.triangles.len()).sum::<usize>(),
            mesh.triangles.len()
        );
        assert!(tiles.iter().flat_map(|tile| &tile.uv).any(|uv| uv.y > 0.5));
    }

    #[test]
    fn steep_slope_unwrap_preserves_surface_lengths_and_is_deterministic() {
        let source = slope_mesh();
        let (unwrapped, charts, size) = unwrap_mesh(&source, 128).unwrap();
        assert_eq!(
            unwrap_mesh(&source, 128).unwrap(),
            (unwrapped.clone(), charts, size)
        );
        assert_eq!(source.vertices, unwrapped.vertices);
        let pixel_scale = Vec2::new(size[0] as f32, size[1] as f32);
        let triangle = &unwrapped.triangles[..3];
        let texel_lengths: Vec<_> = [(0, 1), (1, 2), (2, 0)]
            .into_iter()
            .map(|(a, b)| {
                let a = triangle[a] as usize;
                let b = triangle[b] as usize;
                ((unwrapped.uv[a] - unwrapped.uv[b]) * pixel_scale).length()
                    / source.vertices[a].distance(source.vertices[b])
            })
            .collect();
        let minimum = texel_lengths.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum = texel_lengths.iter().copied().fold(0.0, f32::max);
        assert!(
            maximum / minimum < 1.02,
            "unwrapped steep plane has unequal texel density: {texel_lengths:?}"
        );
        assert!(Vec3::new(1.0, 0.0, 4.0).length() > 4.0); // Top-down UVs stretch this direction by >4x.
    }

    #[test]
    fn atlas_bakes_absolute_lod0_normals_and_world_space_occlusion() {
        let mesh = slope_mesh();
        let terrain = Terrain::new(mesh.clone());
        let atlas = bake_atlas(&terrain, &mesh, None, 64).unwrap();
        let normal = mesh.normals[0].to_array().map(signed_normal_byte);
        let samples: Vec<_> = atlas
            .maps
            .normal_rgb
            .chunks_exact(3)
            .zip(&atlas.maps.occlusion)
            .filter(|(rgb, _)| *rgb == normal)
            .collect();
        assert!(samples.len() > 64);
        assert!(
            samples.iter().all(|(_, ao)| **ao >= 253),
            "a planar slope cannot self-occlude"
        );
        assert_eq!(bake_atlas(&terrain, &mesh, None, 64).unwrap(), atlas);
        assert_eq!(terrain.mesh(), &mesh);
    }

    #[test]
    fn steep_slope_retains_varying_lod0_normal_detail() {
        let target = slope_mesh();
        let mut detail = target.clone();
        detail.normals = vec![
            Vec3::new(-4.0, -1.0, 1.0).normalize(),
            Vec3::new(-4.0, 1.0, 1.0).normalize(),
            Vec3::new(-3.0, 0.0, 1.5).normalize(),
            Vec3::new(-5.0, 0.0, 0.5).normalize(),
        ];
        let atlas = bake_atlas(&Terrain::new(detail.clone()), &target, None, 128).unwrap();
        let mut checked = 0;
        for (face, triangle) in atlas.mesh.triangles.chunks_exact(3).enumerate() {
            let uv =
                [triangle[0], triangle[1], triangle[2]].map(|index| atlas.mesh.uv[index as usize]);
            let normals = [
                detail.triangles[face * 3],
                detail.triangles[face * 3 + 1],
                detail.triangles[face * 3 + 2],
            ]
            .map(|index| detail.normals[index as usize]);
            rasterize(
                uv,
                atlas.maps.width as usize,
                atlas.maps.height as usize,
                |pixel, weights| {
                    if weights.min_element() < 0.1 {
                        return;
                    }
                    let expected =
                        (normals[0] * weights.x + normals[1] * weights.y + normals[2] * weights.z)
                            .normalize()
                            .to_array()
                            .map(signed_normal_byte);
                    let actual = &atlas.maps.normal_rgb[pixel * 3..pixel * 3 + 3];
                    assert!(
                        actual
                            .iter()
                            .zip(expected)
                            .all(|(&a, e)| a.abs_diff(e) <= 1),
                        "normal detail changed basis: {actual:?} vs {expected:?}"
                    );
                    checked += 1;
                },
            );
        }
        assert!(checked > 100);
    }

    #[test]
    fn canopy_is_projected_in_world_space_into_the_atlas() {
        let mesh = slope_mesh();
        let terrain = Terrain::new(mesh.clone());
        let canopy = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 6.0),
                Vec3::new(1.0, 0.0, 6.0),
                Vec3::new(1.0, 1.0, 6.0),
                Vec3::new(0.0, 1.0, 6.0),
            ],
            triangles: mesh.triangles.clone(),
            ..Mesh::default()
        };
        let atlas = bake_atlas(&terrain, &mesh, Some(&canopy), 64).unwrap();
        assert!(
            atlas
                .maps
                .occlusion
                .iter()
                .filter(|&&ao| ao == 112)
                .nth(64)
                .is_some()
        );
    }

    #[test]
    fn dilation_preserves_each_chart_interior() {
        let mut owners = vec![1, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, 2];
        dilate_triangle_owners(&mut owners, 7, 1, 3);
        assert_eq!(owners, [1, 1, 1, 1, 2, 2, 2]);
    }

    #[test]
    fn padding_covers_diagonal_filter_footprints() {
        let mut owners = vec![EMPTY; 19 * 19];
        owners[9 * 19 + 9] = 7;
        dilate_triangle_owners(&mut owners, 19, 19, 8);
        for y in 1..18 {
            for x in 1..18 {
                assert_eq!(owners[y * 19 + x], 7, "missing diagonal gutter at {x},{y}");
            }
        }
    }

    #[test]
    fn subpixel_charts_have_baked_filter_samples() {
        let detail = Mesh {
            vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            normals: vec![Vec3::new(-0.6, 0.0, 0.8); 3],
            triangles: vec![0, 1, 2],
            uv: vec![Vec2::ZERO; 3],
        };
        let uv = [
            Vec2::new(8.1, 8.1),
            Vec2::new(8.4, 8.1),
            Vec2::new(8.1, 8.4),
        ]
        .map(|pixel| (pixel + Vec2::splat(0.5)) / 32.0);
        let target = Mesh {
            uv: uv.to_vec(),
            ..detail.clone()
        };
        let maps = bake_unwrapped_maps(&Terrain::new(detail), &target, &[0; 3], None, 32, 32, 32);
        for sample in uv.into_iter().chain([(uv[0] + uv[1] + uv[2]) / 3.0]) {
            assert!(bilinear_normal(&maps, sample).dot(Vec3::new(-0.6, 0.0, 0.8)) > 0.999);
        }
    }

    #[test]
    fn thin_chart_gutters_do_not_sample_distant_terrain() {
        let uv = [
            Vec2::new(8.1, 8.1),
            Vec2::new(20.1, 8.1),
            Vec2::new(8.1, 8.11),
        ]
        .map(|pixel| (pixel + Vec2::splat(0.5)) / 32.0);
        let world = [
            Vec2::new(0.2, 0.2),
            Vec2::new(0.4, 0.2),
            Vec2::new(0.2, 0.4),
        ];
        let triangle = AtlasTriangle::new(uv, world, Vec2::splat(32.0)).unwrap();
        for y in 0..25 {
            for x in 0..29 {
                let point = triangle.world_point(x, y);
                assert!(point.min_element() >= 0.19999 && point.max_element() <= 0.40001);
                assert!(point.x + point.y <= 0.60001);
            }
        }
    }

    fn bilinear_normal(maps: &SurfaceMaps, uv: Vec2) -> Vec3 {
        let pixel = uv * Vec2::new(maps.width as f32, maps.height as f32) - Vec2::splat(0.5);
        let x = pixel.x.floor() as usize;
        let y = pixel.y.floor() as usize;
        let fraction = pixel.fract();
        let normal = |x: usize, y: usize| {
            let start = (y * maps.width as usize + x) * 3;
            Vec3::new(
                f32::from(maps.normal_rgb[start]),
                f32::from(maps.normal_rgb[start + 1]),
                f32::from(maps.normal_rgb[start + 2]),
            ) / 127.5
                - Vec3::ONE
        };
        normal(x, y)
            .lerp(normal(x + 1, y), fraction.x)
            .lerp(
                normal(x, y + 1).lerp(normal(x + 1, y + 1), fraction.x),
                fraction.y,
            )
            .normalize()
    }

    #[test]
    fn filtered_normals_match_across_independently_rotated_uv_chart_seams() {
        let target = slope_mesh();
        let mut detail = target.clone();
        detail.normals = vec![
            Vec3::new(-4.0, -2.0, 1.0).normalize(),
            Vec3::new(-3.0, 1.0, 1.0).normalize(),
            Vec3::new(-4.0, 2.0, 1.0).normalize(),
            Vec3::new(-5.0, -1.0, 1.0).normalize(),
        ];
        // Two separately rotated and offset charts of the same square. Their
        // duplicated diagonal lies on different fractional texel coordinates.
        let chart_a = [
            Vec2::new(0.10, 0.12),
            Vec2::new(0.42, 0.12),
            Vec2::new(0.42, 0.44),
        ];
        let chart_b = [
            Vec2::new(0.62, 0.85),
            Vec2::new(0.89, 0.58),
            Vec2::new(0.89, 0.85),
        ];
        let mesh = Mesh {
            vertices: [0, 1, 2, 0, 2, 3]
                .map(|index| target.vertices[index])
                .to_vec(),
            normals: vec![target.normals[0]; 6],
            triangles: vec![0, 1, 2, 3, 4, 5],
            uv: chart_a.into_iter().chain(chart_b).collect(),
        };
        let maps = bake_unwrapped_maps(
            &Terrain::new(detail),
            &mesh,
            &[0, 0, 0, 1, 1, 1],
            None,
            64,
            64,
            64,
        );
        let mut maximum_difference = 0.0_f32;
        for step in 2..19 {
            let along = step as f32 / 20.0;
            let first = bilinear_normal(&maps, chart_a[0].lerp(chart_a[2], along));
            let second = bilinear_normal(&maps, chart_b[0].lerp(chart_b[1], along));
            maximum_difference = maximum_difference.max(first.distance(second));
        }
        assert!(
            maximum_difference < 0.008,
            "filtered chart boundary normal discontinuity: {maximum_difference}"
        );
    }

    #[test]
    fn clipping_retains_discontinuous_uvs_at_grid_corner() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::ZERO,
                Vec3::X,
                Vec3::ONE - Vec3::Z,
                Vec3::ZERO,
                Vec3::ONE - Vec3::Z,
                Vec3::Y,
            ],
            normals: vec![Vec3::Z; 6],
            uv: vec![
                Vec2::ZERO,
                Vec2::new(0.4, 0.0),
                Vec2::new(0.4, 0.4),
                Vec2::new(0.6, 0.6),
                Vec2::ONE,
                Vec2::new(0.6, 1.0),
            ],
            triangles: vec![0, 1, 2, 3, 4, 5],
        };
        let tiles = mesh.sliced_grid_preserving_uv_seams(BoundingBox::default(), 2);
        let corner = Vec3::new(0.5, 0.5, 0.0);
        let values: Vec<_> = tiles[0]
            .vertices
            .iter()
            .zip(&tiles[0].uv)
            .filter(|(point, _)| point.distance(corner) < 1.0e-6)
            .map(|(_, &uv)| uv)
            .collect();
        assert!(
            values
                .iter()
                .any(|uv| uv.distance(Vec2::splat(0.2)) < 1.0e-5),
            "{values:?}"
        );
        assert!(
            values
                .iter()
                .any(|uv| uv.distance(Vec2::splat(0.8)) < 1.0e-5),
            "{values:?}"
        );
    }

    #[test]
    fn atlas_grid_corners_preserve_overlapping_cave_surface_heights() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y,
                Vec3::Z,
                Vec3::X + Vec3::Z,
                Vec3::Y + Vec3::Z,
            ],
            normals: vec![Vec3::Z, Vec3::Z, Vec3::Z, -Vec3::Z, -Vec3::Z, -Vec3::Z],
            triangles: vec![0, 1, 2, 3, 4, 5],
            uv: vec![Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ZERO, Vec2::X, Vec2::Y],
        };
        let tiles = mesh.sliced_grid_preserving_uv_seams(BoundingBox::default(), 2);
        let corner_heights: Vec<_> = tiles[0]
            .vertices
            .iter()
            .filter(|point| point.truncate().distance(Vec2::splat(0.5)) < 1.0e-6)
            .map(|point| point.z)
            .collect();
        assert!(corner_heights.contains(&0.0), "{corner_heights:?}");
        assert!(corner_heights.contains(&1.0), "{corner_heights:?}");
    }

    #[test]
    fn clamped_boundary_splits_preserve_both_chart_uvs() {
        let fine = Mesh {
            vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::ZERO, Vec3::X, Vec3::Y],
            normals: vec![Vec3::Z; 6],
            triangles: vec![0, 1, 2, 3, 4, 5],
            uv: vec![
                Vec2::ZERO,
                Vec2::new(0.4, 0.0),
                Vec2::new(0.0, 0.4),
                Vec2::new(0.6, 0.6),
                Vec2::new(1.0, 0.6),
                Vec2::new(0.6, 1.0),
            ],
        };
        let coarse = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.1),
                Vec3::new(0.5, 0.0, 0.2),
                Vec3::new(1.0, 0.0, 0.1),
                Vec3::Y,
            ],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 3, 1, 2, 3],
            uv: vec![Vec2::ZERO; 4],
        };
        let tiles = fine.sliced_grid_clamped_preserving_uv_seams(
            BoundingBox::default(),
            1,
            &coarse,
            crate::mesh::CLAMP_BOTTOM,
        );
        let mesh = &tiles[0];
        let uv: Vec<_> = mesh
            .vertices
            .iter()
            .zip(&mesh.uv)
            .filter(|(position, _)| position.distance(Vec3::new(0.5, 0.0, 0.2)) < 1.0e-6)
            .map(|(_, &uv)| uv)
            .collect();
        assert!(
            uv.iter()
                .any(|&uv| uv.distance(Vec2::new(0.2, 0.0)) < 1.0e-5),
            "{uv:?}"
        );
        assert!(
            uv.iter()
                .any(|&uv| uv.distance(Vec2::new(0.8, 0.6)) < 1.0e-5),
            "{uv:?}"
        );
    }

    #[test]
    fn empty_render_surface_has_neutral_one_row_atlas() {
        let terrain = Terrain::new(slope_mesh());
        let empty = Mesh::default();
        assert!(bake_atlas(&terrain, &empty, None, 0).is_err());
        for dimension in [1024, 2048] {
            let atlas = bake_atlas(&terrain, &empty, None, dimension).unwrap();
            assert_eq!(atlas.mesh, Mesh::default());
            assert_eq!(atlas.maps.width(), dimension);
            assert_eq!(atlas.maps.height(), 1);
            assert!(
                atlas
                    .maps
                    .normal_rgb()
                    .chunks_exact(3)
                    .all(|pixel| pixel == [127, 127, 255])
            );
            assert!(atlas.maps.occlusion().iter().all(|&ao| ao == 255));
        }
    }

    #[test]
    fn invalid_dimensions_and_empty_meshes_are_rejected() {
        assert!(unwrap_mesh(&slope_mesh(), 0).is_err());
        assert!(unwrap_mesh(&slope_mesh(), 16).is_err());
        assert!(unwrap_mesh(&Mesh::default(), 128).is_err());
    }

    #[test]
    fn canonical_lod2_cache_stitches_thousands_of_disconnected_charts() {
        let started = std::time::Instant::now();
        let mut island = Island::generate_with_forest(
            73,
            crate::IslandOptions {
                terrain_size: 16,
                ..crate::IslandOptions::default()
            },
            crate::ForestOptions {
                noise_threshold: 1.0,
                prototype_count: 1,
                ..crate::ForestOptions::default()
            },
        )
        .unwrap();
        let mut target = Mesh::default();
        for index in 0..5_000 {
            let x = 0.08 + (index % 100) as f32 * 0.008;
            let y = 0.08 + (index / 100) as f32 * 0.016;
            let first = target.vertices.len() as u32;
            for point in [
                Vec2::new(x, y),
                Vec2::new(x + 0.0008, y),
                Vec2::new(x, y + 0.0008),
            ] {
                target.vertices.push(point.extend(0.05));
                target.normals.push(Vec3::Z);
                target.uv.push(point);
            }
            target.triangles.extend([first, first + 1, first + 2]);
        }
        island.coarser_lods[1] = target;
        let atlas = island.cached_surface_atlas(2).unwrap();
        let width = atlas.maps.width();
        let height = atlas.maps.height();
        assert!(
            width > 1024 || height > 1024,
            "5,000 separately padded charts require multiple canonical 1024px pages: {width}x{height}"
        );
        assert!(width <= 8192 && height <= 8192);
        assert_eq!(atlas.mesh.triangles.len(), 15_000);
        assert!(
            atlas
                .mesh
                .uv
                .iter()
                .all(|uv| uv.is_finite() && uv.min_element() >= 0.0 && uv.max_element() <= 1.0)
        );
        let tiles = island
            .terrain_atlas_mesh_grid(2, BoundingBox::default(), 2, 0)
            .unwrap();
        assert_eq!(tiles.len(), 4);
        assert!(tiles.iter().all(|tile| {
            tile.vertices.len() == tile.uv.len()
                && tile
                    .uv
                    .iter()
                    .all(|uv| uv.is_finite() && uv.min_element() >= 0.0 && uv.max_element() <= 1.0)
        }));
        if let Some(path) = std::env::var_os("MOTU_ATLAS_WRITE_MULTIPAGE_FIXTURE") {
            island.save(path).unwrap();
        }
        eprintln!(
            "Canonical LOD2 multipage regression: {width}x{height}, elapsed {:?}",
            started.elapsed()
        );
    }
}
