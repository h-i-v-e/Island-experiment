#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    fs::File,
    io::{self, Read, Write},
    mem::size_of,
    path::Path,
    sync::OnceLock,
    thread,
};

use rayon::prelude::*;

use crate::{
    Adjacency, BoundingBox, Mesh, Raster, River, Vec2, Vec3,
    coast::{self, CoastScale, GeologyField},
    mesh::{NewVertexStencil, TessellationResult},
    mesh_clipper::MeshClipper,
    noise,
    profiling::StageTimer,
    rivers::RiverNetwork,
    rng::Rng,
};

const DETAIL_DISPLACEMENT_RATIO: f32 = 0.025;
const SHARP_ROCK_DISPLACEMENT_RATIO: f32 = 0.15;
const FORCED_ROCK_HARDNESS: f32 = 2.0;
const HYDRAULIC_EDGE_SHIFT_LIMIT: f32 = 0.08;
const HYDRAULIC_MIN_PROJECTED_AREA_RATIO: f32 = 0.2;
const MINIMUM_BEDROCK_EROSION_RATE: f32 = 0.05;
const TRIANGLE_INDEX_OFFSET_BUDGET_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const LOOSE_DEPTH_EPSILON: f32 = 1.0e-8;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SurfaceMaterial {
    deposited_depth: Vec<f32>,
    bedrock_hardness: Vec<f32>,
}

impl SurfaceMaterial {
    pub(crate) fn empty(vertex_count: usize) -> Self {
        Self {
            deposited_depth: vec![0.0; vertex_count],
            bedrock_hardness: vec![0.0; vertex_count],
        }
    }

    pub(crate) fn initialize_geology(&mut self, mesh: &Mesh, geology: GeologyField) {
        debug_assert_eq!(self.bedrock_hardness.len(), mesh.vertices.len());
        self.bedrock_hardness
            .iter_mut()
            .zip(&mesh.vertices)
            .for_each(|(hardness, vertex)| *hardness = geology.hardness(vertex.truncate()));
    }

    pub(crate) fn depths(&self) -> &[f32] {
        &self.deposited_depth
    }

    pub(crate) fn depths_mut(&mut self) -> &mut [f32] {
        &mut self.deposited_depth
    }

    pub(crate) fn hardnesses(&self) -> &[f32] {
        &self.bedrock_hardness
    }

    fn into_tessellated(mut self, source: &Mesh, tessellation: TessellationResult) -> (Mesh, Self) {
        let old_volume = self.volume(source);
        self.extend_after_tessellation(old_volume, &tessellation.mesh, &tessellation.new_vertices);
        (tessellation.mesh, self)
    }

    pub(crate) fn volume(&self, mesh: &Mesh) -> f64 {
        debug_assert_eq!(self.deposited_depth.len(), mesh.vertices.len());
        mesh.triangles
            .chunks_exact(3)
            .map(|triangle| {
                let [a, b, c] = [
                    mesh.vertices[triangle[0] as usize].truncate(),
                    mesh.vertices[triangle[1] as usize].truncate(),
                    mesh.vertices[triangle[2] as usize].truncate(),
                ];
                let third_area = f64::from((b - a).perp_dot(c - a).abs() / 6.0);
                let depth = triangle
                    .iter()
                    .map(|&vertex| f64::from(self.deposited_depth[vertex as usize].max(0.0)))
                    .sum::<f64>();
                third_area * depth
            })
            .sum()
    }

    pub(crate) fn extend_after_tessellation(
        &mut self,
        old_volume: f64,
        mesh: &Mesh,
        stencils: &[NewVertexStencil],
    ) {
        let old_vertex_count = self.deposited_depth.len();
        self.deposited_depth.reserve(stencils.len());
        self.bedrock_hardness.reserve(stencils.len());
        for stencil in stencils {
            debug_assert_eq!(stencil.vertex as usize, self.deposited_depth.len());
            let count = usize::from(stencil.count);
            debug_assert!((3..=4).contains(&count));
            debug_assert!(
                stencil.surrounding[..count]
                    .iter()
                    .all(|&vertex| (vertex as usize) < old_vertex_count)
            );
            let depth = stencil.surrounding[..count]
                .iter()
                .map(|&vertex| self.deposited_depth[vertex as usize])
                .sum::<f32>()
                / count as f32;
            let hardness = stencil.surrounding[..count]
                .iter()
                .map(|&vertex| self.bedrock_hardness[vertex as usize])
                .sum::<f32>()
                / count as f32;
            self.deposited_depth.push(depth.max(0.0));
            self.bedrock_hardness.push(hardness.clamp(0.0, 1.0));
        }
        debug_assert_eq!(self.deposited_depth.len(), mesh.vertices.len());
        debug_assert_eq!(self.bedrock_hardness.len(), mesh.vertices.len());
        self.rescale_to_volume(mesh, old_volume);
    }

    pub(crate) fn rescale_to_volume(&mut self, mesh: &Mesh, target_volume: f64) {
        debug_assert_eq!(self.deposited_depth.len(), mesh.vertices.len());
        if target_volume <= f64::EPSILON {
            self.deposited_depth.fill(0.0);
            return;
        }
        let provisional = self.volume(mesh);
        if provisional <= f64::EPSILON {
            debug_assert!(false, "positive loose volume vanished during mesh mutation");
            return;
        }
        let scale = (target_volume / provisional) as f32;
        self.deposited_depth
            .iter_mut()
            .for_each(|depth| *depth = (*depth * scale).max(0.0));
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TerrainMaterialField {
    values: Vec<Vec3>,
}

impl TerrainMaterialField {
    fn from_surface(material: &SurfaceMaterial, river_bed: &[bool], forced_rock: &[bool]) -> Self {
        debug_assert_eq!(material.hardnesses().len(), material.depths().len());
        debug_assert_eq!(river_bed.len(), material.depths().len());
        debug_assert_eq!(forced_rock.len(), material.depths().len());
        let values = material
            .hardnesses()
            .iter()
            .zip(material.depths())
            .zip(river_bed)
            .zip(forced_rock)
            .map(|(((&hardness, &depth), &is_river_bed), &is_forced_rock)| {
                let cover = (depth / 0.002).clamp(0.0, 1.0);
                let cover = cover * cover * (3.0 - 2.0 * cover);
                Vec3::new(
                    if is_forced_rock {
                        FORCED_ROCK_HARDNESS
                    } else {
                        hardness.clamp(0.0, 1.0)
                    },
                    cover,
                    if is_river_bed { 1.0 } else { 0.0 },
                )
            })
            .collect();
        Self { values }
    }

    fn sample(&self, terrain: &Terrain, point: Vec2) -> Vec3 {
        sample_mesh_triangle(&terrain.mesh, &terrain.triangle_index, point).map_or_else(
            || {
                let nearest = terrain.triangle_index.nearest_vertex(&terrain.mesh, point);
                self.values[nearest]
            },
            |(triangle, weights)| {
                self.values[triangle[0]] * weights[0]
                    + self.values[triangle[1]] * weights[1]
                    + self.values[triangle[2]] * weights[2]
            },
        )
    }
}

pub(crate) fn projected_vertex_control_areas(mesh: &Mesh) -> Vec<f32> {
    let mut areas = vec![0.0; mesh.vertices.len()];
    for triangle in mesh.triangles.chunks_exact(3) {
        let [a, b, c] = [
            mesh.vertices[triangle[0] as usize].truncate(),
            mesh.vertices[triangle[1] as usize].truncate(),
            mesh.vertices[triangle[2] as usize].truncate(),
        ];
        let share = (b - a).perp_dot(c - a).abs() / 6.0;
        for &vertex in triangle {
            areas[vertex as usize] += share;
        }
    }
    areas
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct IslandOptions {
    pub max_height: f32,
    pub water_ratio: f32,
    pub slope_multiplier: f32,
    pub coastal_slope_multiplier: f32,
    /// Multiplies mesh-native wave erosion and rocky-platform formation.
    pub coastal_erosion_strength: f32,
    /// Controls redistribution of eroded coastal sediment into beaches.
    pub beach_formation_strength: f32,
    /// Multiplies the strength of every staged hydraulic erosion pass.
    /// Zero disables hydraulic erosion while preserving thermal erosion.
    pub hydraulic_erosion_strength: f32,
    /// Controls how quickly excess carried sediment settles on gentle slopes.
    pub hydraulic_deposition_strength: f32,
    /// Slope angle at which hydraulic deposition falls to zero.
    pub hydraulic_deposition_slope_degrees: f32,
    /// River-source flow thresholds measured in standard deviations above the
    /// mean flow for each successive mesh-detail stage.
    pub river_lod2_source_threshold: f32,
    pub river_lod1_source_threshold: f32,
    pub river_broad_source_threshold: f32,
    pub river_land_source_threshold: f32,
    pub river_final_source_threshold: f32,
    /// Number of free-form XY seed points used by Delaunay triangulation.
    pub terrain_size: u32,
}

impl Default for IslandOptions {
    fn default() -> Self {
        Self {
            max_height: 0.2,
            water_ratio: 0.6,
            slope_multiplier: 1.3,
            coastal_slope_multiplier: 1.0,
            coastal_erosion_strength: 1.0,
            beach_formation_strength: 1.0,
            hydraulic_erosion_strength: 1.0,
            hydraulic_deposition_strength: 1.5,
            hydraulic_deposition_slope_degrees: 12.0,
            river_lod2_source_threshold: 0.35,
            river_lod1_source_threshold: 0.65,
            river_broad_source_threshold: 1.0,
            river_land_source_threshold: 1.3,
            river_final_source_threshold: 1.6,
            terrain_size: 1024,
        }
    }
}

impl IslandOptions {
    const fn river_source_thresholds(self) -> [f32; 5] {
        [
            self.river_lod2_source_threshold,
            self.river_lod1_source_threshold,
            self.river_broad_source_threshold,
            self.river_land_source_threshold,
            self.river_final_source_threshold,
        ]
    }

    fn validate(self) -> Result<Self, String> {
        if !self.max_height.is_finite() || self.max_height <= 0.0 {
            return Err("max_height must be finite and greater than zero".into());
        }
        if !self.hydraulic_erosion_strength.is_finite()
            || !(0.0..=8.0).contains(&self.hydraulic_erosion_strength)
        {
            return Err("hydraulic_erosion_strength must be between 0 and 8".into());
        }
        if !self.hydraulic_deposition_strength.is_finite()
            || !(0.0..=4.0).contains(&self.hydraulic_deposition_strength)
        {
            return Err("hydraulic_deposition_strength must be between 0 and 4".into());
        }
        if !self.hydraulic_deposition_slope_degrees.is_finite()
            || !(1.0..=45.0).contains(&self.hydraulic_deposition_slope_degrees)
        {
            return Err("hydraulic_deposition_slope_degrees must be between 1 and 45".into());
        }
        if self.terrain_size < 16 || self.terrain_size > 4096 {
            return Err("terrain_size must contain between 16 and 4096 seed points".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TriangleIndex {
    dimension: usize,
    offsets: Vec<usize>,
    faces: Vec<u32>,
}

impl TriangleIndex {
    fn new(mesh: &Mesh) -> Self {
        let face_count = mesh.triangles.len() / 3;
        let maximum_dimension =
            ((TRIANGLE_INDEX_OFFSET_BUDGET_BYTES / size_of::<usize>()) as f64).sqrt() as usize;
        let dimension =
            ((face_count as f64 / 8.0).sqrt().ceil() as usize).clamp(8, maximum_dimension.max(8));
        let bin_count = dimension * dimension;
        let mut counts = vec![0_usize; bin_count];
        for triangle in mesh.triangles.chunks_exact(3) {
            let (min_x, max_x, min_y, max_y) = triangle_bin_bounds(mesh, triangle, dimension);
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    counts[y * dimension + x] += 1;
                }
            }
        }
        let mut offsets = Vec::with_capacity(bin_count + 1);
        offsets.push(0);
        for count in counts {
            offsets.push(offsets.last().copied().unwrap_or_default() + count);
        }
        let mut cursor = offsets[..bin_count].to_vec();
        let mut faces = vec![0_u32; *offsets.last().unwrap_or(&0)];
        for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
            let (min_x, max_x, min_y, max_y) = triangle_bin_bounds(mesh, triangle, dimension);
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let bin = y * dimension + x;
                    faces[cursor[bin]] = face as u32;
                    cursor[bin] += 1;
                }
            }
        }
        Self {
            dimension,
            offsets,
            faces,
        }
    }

    fn candidates(&self, point: Vec2) -> &[u32] {
        let [x, y] = self.point_bin(point);
        let bin = y * self.dimension + x;
        &self.faces[self.offsets[bin]..self.offsets[bin + 1]]
    }

    fn point_bin(&self, point: Vec2) -> [usize; 2] {
        [
            bin_coordinate(point.x, self.dimension),
            bin_coordinate(point.y, self.dimension),
        ]
    }

    fn bin_faces(&self, x: usize, y: usize) -> &[u32] {
        let bin = y * self.dimension + x;
        &self.faces[self.offsets[bin]..self.offsets[bin + 1]]
    }

    fn nearest_vertex(&self, mesh: &Mesh, point: Vec2) -> usize {
        let [origin_x, origin_y] = self.point_bin(point);
        let mut best = None::<(f32, usize)>;
        for radius in 0..self.dimension {
            let minimum_x = origin_x.saturating_sub(radius);
            let maximum_x = (origin_x + radius).min(self.dimension - 1);
            let minimum_y = origin_y.saturating_sub(radius);
            let maximum_y = (origin_y + radius).min(self.dimension - 1);
            for y in minimum_y..=maximum_y {
                for x in minimum_x..=maximum_x {
                    if radius != 0
                        && x != minimum_x
                        && x != maximum_x
                        && y != minimum_y
                        && y != maximum_y
                    {
                        continue;
                    }
                    for &face in self.bin_faces(x, y) {
                        let offset = face as usize * 3;
                        for &vertex in &mesh.triangles[offset..offset + 3] {
                            let vertex = vertex as usize;
                            let distance = mesh.vertices[vertex].truncate().distance_squared(point);
                            if best.is_none_or(|current| match distance.total_cmp(&current.0) {
                                Ordering::Less => true,
                                Ordering::Equal => vertex < current.1,
                                Ordering::Greater => false,
                            }) {
                                best = Some((distance, vertex));
                            }
                        }
                    }
                }
            }
            if let Some((distance, vertex)) = best {
                let cell_width = 1.0 / self.dimension as f32;
                if radius == 0 || distance.sqrt() <= (radius as f32 + 1.0) * cell_width {
                    return vertex;
                }
            }
        }
        debug_assert!(
            mesh.vertices.is_empty(),
            "triangle index did not reference any vertex"
        );
        0
    }
}

/// The finest irregular terrain mesh plus a derived spatial lookup index.
///
/// The index uses bins only to locate triangles for sampling. It never defines
/// vertices, connectivity, erosion, flow, rivers, or levels of detail.
#[derive(Clone, Debug, PartialEq)]
pub struct Terrain {
    mesh: Mesh,
    triangle_index: TriangleIndex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceMaps {
    width: u32,
    height: u32,
    normal_rgb: Vec<u8>,
    occlusion: Vec<u8>,
}

impl SurfaceMaps {
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn normal_rgb(&self) -> &[u8] {
        &self.normal_rgb
    }

    #[must_use]
    pub fn occlusion(&self) -> &[u8] {
        &self.occlusion
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SurfaceSample {
    position: Vec3,
    normal: Vec3,
}

impl Terrain {
    #[cfg(test)]
    fn new(mesh: Mesh) -> Self {
        let triangle_index = TriangleIndex::new(&mesh);
        Self::with_index(mesh, triangle_index)
    }

    fn with_index(mesh: Mesh, triangle_index: TriangleIndex) -> Self {
        Self {
            mesh,
            triangle_index,
        }
    }

    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub fn vertices(&self) -> &[Vec3] {
        &self.mesh.vertices
    }

    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.mesh.vertices.len()
    }

    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.mesh.triangles.len() / 3
    }

    #[must_use]
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        sample_mesh_triangle(
            &self.mesh,
            &self.triangle_index,
            Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)),
        )
        .map_or_else(
            || {
                let point = Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                self.mesh.vertices[self.triangle_index.nearest_vertex(&self.mesh, point)].z
            },
            |(triangle, weights)| {
                weights[0].mul_add(
                    self.mesh.vertices[triangle[0]].z,
                    weights[1].mul_add(
                        self.mesh.vertices[triangle[1]].z,
                        weights[2] * self.mesh.vertices[triangle[2]].z,
                    ),
                )
            },
        )
    }

    #[must_use]
    pub fn sample_normal(&self, u: f32, v: f32) -> Vec3 {
        sample_mesh_triangle(
            &self.mesh,
            &self.triangle_index,
            Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)),
        )
        .map_or_else(
            || {
                let point = Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                let nearest = self.triangle_index.nearest_vertex(&self.mesh, point);
                self.mesh.normals[nearest]
            },
            |(triangle, weights)| {
                (self.mesh.normals[triangle[0]] * weights[0]
                    + self.mesh.normals[triangle[1]] * weights[1]
                    + self.mesh.normals[triangle[2]] * weights[2])
                    .try_normalize()
                    .unwrap_or(Vec3::Z)
            },
        )
    }

    pub(crate) fn sample_surface(&self, u: f32, v: f32) -> (f32, Vec3) {
        sample_mesh_surface(&self.mesh, &self.triangle_index, u, v)
    }
}

fn sample_mesh_surface(mesh: &Mesh, triangle_index: &TriangleIndex, u: f32, v: f32) -> (f32, Vec3) {
    let point = Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    sample_mesh_triangle(mesh, triangle_index, point).map_or_else(
        || {
            let nearest = triangle_index.nearest_vertex(mesh, point);
            (mesh.vertices[nearest].z, mesh.normals[nearest])
        },
        |(triangle, weights)| {
            let elevation = weights[0].mul_add(
                mesh.vertices[triangle[0]].z,
                weights[1].mul_add(
                    mesh.vertices[triangle[1]].z,
                    weights[2] * mesh.vertices[triangle[2]].z,
                ),
            );
            let normal = (mesh.normals[triangle[0]] * weights[0]
                + mesh.normals[triangle[1]] * weights[1]
                + mesh.normals[triangle[2]] * weights[2])
                .try_normalize()
                .unwrap_or(Vec3::Z);
            (elevation, normal)
        },
    )
}

fn sample_mesh_triangle(
    mesh: &Mesh,
    triangle_index: &TriangleIndex,
    point: Vec2,
) -> Option<([usize; 3], [f32; 3])> {
    triangle_index.candidates(point).iter().find_map(|&face| {
        let offset = face as usize * 3;
        let triangle = [
            mesh.triangles[offset] as usize,
            mesh.triangles[offset + 1] as usize,
            mesh.triangles[offset + 2] as usize,
        ];
        barycentric(
            point,
            mesh.vertices[triangle[0]].truncate(),
            mesh.vertices[triangle[1]].truncate(),
            mesh.vertices[triangle[2]].truncate(),
        )
        .map(|weights| (triangle, weights))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoration {
    Tree,
    Bush,
    Rock,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decorations {
    trees: Vec<Vec3>,
    bushes: Vec<Vec3>,
    rocks: Vec<Vec3>,
}

struct RiverPointIndex {
    dimension: usize,
    offsets: Vec<usize>,
    points: Vec<Vec2>,
}

impl RiverPointIndex {
    fn new(rivers: &[River]) -> Self {
        let point_count = rivers.iter().map(|river| river.nodes.len()).sum::<usize>();
        let dimension = ((point_count as f32 / 4.0).sqrt().ceil() as usize).clamp(8, 512);
        let mut counts = vec![0_usize; dimension * dimension];
        for point in rivers
            .iter()
            .flat_map(|river| river.nodes.iter().map(|node| node.position.truncate()))
        {
            let x = bin_coordinate(point.x, dimension);
            let y = bin_coordinate(point.y, dimension);
            counts[y * dimension + x] += 1;
        }
        let mut offsets = Vec::with_capacity(counts.len() + 1);
        offsets.push(0);
        for count in counts {
            offsets.push(offsets.last().copied().unwrap_or_default() + count);
        }
        let mut cursor = offsets[..dimension * dimension].to_vec();
        let mut points = vec![Vec2::ZERO; point_count];
        for point in rivers
            .iter()
            .flat_map(|river| river.nodes.iter().map(|node| node.position.truncate()))
        {
            let x = bin_coordinate(point.x, dimension);
            let y = bin_coordinate(point.y, dimension);
            let bin = y * dimension + x;
            points[cursor[bin]] = point;
            cursor[bin] += 1;
        }
        Self {
            dimension,
            offsets,
            points,
        }
    }

    fn contains_within(&self, point: Vec2, distance_squared: f32) -> bool {
        let radius = distance_squared.sqrt();
        let cell_radius = (radius * self.dimension as f32).ceil() as usize;
        let origin_x = bin_coordinate(point.x, self.dimension);
        let origin_y = bin_coordinate(point.y, self.dimension);
        let minimum_x = origin_x.saturating_sub(cell_radius);
        let maximum_x = (origin_x + cell_radius).min(self.dimension - 1);
        let minimum_y = origin_y.saturating_sub(cell_radius);
        let maximum_y = (origin_y + cell_radius).min(self.dimension - 1);
        (minimum_y..=maximum_y).any(|y| {
            (minimum_x..=maximum_x).any(|x| {
                let bin = y * self.dimension + x;
                self.points[self.offsets[bin]..self.offsets[bin + 1]]
                    .iter()
                    .any(|river_point| river_point.distance_squared(point) < distance_squared)
            })
        })
    }
}

impl Decorations {
    #[must_use]
    pub fn trees(&self) -> &[Vec3] {
        &self.trees
    }

    #[must_use]
    pub fn bushes(&self) -> &[Vec3] {
        &self.bushes
    }

    #[must_use]
    pub fn rocks(&self) -> &[Vec3] {
        &self.rocks
    }

    fn generate(seed: u64, terrain: &Terrain, rivers: &[River], target: usize) -> Self {
        let _timer = StageTimer::new("decorations.lazy");
        let mut rng = Rng::new(seed ^ 0xe703_7ed1_a0b4_28db);
        let mut out = Self::default();
        out.trees.reserve(target * 3 / 5);
        out.bushes.reserve(target / 4);
        out.rocks.reserve(target / 6);
        let river_index = RiverPointIndex::new(rivers);
        for _ in 0..target * 6 {
            if out.trees.len() + out.bushes.len() + out.rocks.len() >= target {
                break;
            }
            let u = rng.range(0.01, 0.99);
            let v = rng.range(0.01, 0.99);
            let (height, normal) = terrain.sample_surface(u, v);
            if height <= 0.001 {
                continue;
            }
            let slope = 1.0 - normal.z;
            let point = Vec3::new(u, v, height);
            if river_index.contains_within(point.truncate(), 0.000_025) {
                continue;
            }
            let moisture = noise::fractal(seed ^ 0x8ebc_6af0_9c88_c6e3, u * 7.0, v * 7.0, 3);
            if slope > 0.38 || height > 0.145 {
                if rng.unit() < 0.38 {
                    out.rocks.push(point);
                }
            } else if moisture > -0.05 && height > 0.012 && height < 0.13 {
                if rng.unit() < 0.72 {
                    out.trees.push(point);
                }
            } else if rng.unit() < 0.54 {
                out.bushes.push(point);
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Island {
    seed: u64,
    options: IslandOptions,
    terrain: Terrain,
    material: TerrainMaterialField,
    coarser_lods: [Mesh; 2],
    rivers: Vec<River>,
    river_mesh: Mesh,
    decorations: OnceLock<Decorations>,
}

impl Island {
    /// Generates an island and all derived assets.
    ///
    /// # Errors
    ///
    /// Returns an error when an option is non-finite or outside its supported
    /// range.
    pub fn generate(seed: u64, options: IslandOptions) -> Result<Self, String> {
        let _timer = StageTimer::new("island.generate");
        let options = options.validate()?;
        let mut scratch = GenerationScratch::default();
        let (base, material) = generate_base(seed, options, &mut scratch);
        let context = GenerationContext::new(seed, options);
        let (mut lod2, material) = generate_lod2(&base, material, context, &mut scratch);
        let (lod1, material) = generate_first_lod1(&lod2, material, context, &mut scratch);
        let (mut lod1, material) = refine_lod1_again(
            &lod1,
            material,
            options,
            context.river_thresholds[2],
            &mut scratch,
        );
        let (lod0, material) = generate_broad_lod0(&lod1, material, context, &mut scratch);
        let (mut lod0, mut material) = generate_detail_lod0(&lod0, material, context, &mut scratch);
        let (rivers, river_mesh, river_bed) = {
            let _timer = StageTimer::new("rivers.final");
            let detail_adjacency = lod0.adjacency();
            let mut final_rivers =
                RiverNetwork::generate(&mut lod0, &detail_adjacency, context.river_thresholds[4]);
            final_rivers.shape(&mut lod0, &detail_adjacency, &mut material, true, false);
            final_rivers.into_parts(&mut lod0, &detail_adjacency, &mut material)
        };
        let lod0_index = {
            let _timer = StageTimer::new("lod.correct");
            correct_lods(&mut lod0, &mut lod1, &mut lod2)
        };

        let forced_rock = sharp_rock_mask(&lod0);
        let material = TerrainMaterialField::from_surface(&material, &river_bed, &forced_rock);

        let terrain = {
            let _timer = StageTimer::new("terrain.index");
            Terrain::with_index(lod0, lod0_index)
        };
        Ok(Self {
            seed,
            options,
            terrain,
            material,
            coarser_lods: [lod1, lod2],
            rivers,
            river_mesh,
            decorations: OnceLock::new(),
        })
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn options(&self) -> IslandOptions {
        self.options
    }

    #[must_use]
    pub const fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    #[must_use]
    pub fn lod(&self, level: usize) -> Option<&Mesh> {
        match level {
            0 => Some(self.terrain.mesh()),
            1 => Some(&self.coarser_lods[0]),
            2 => Some(&self.coarser_lods[1]),
            _ => None,
        }
    }

    /// Returns the corrected support mesh intended for display.
    #[must_use]
    pub fn render_lod(&self, level: usize) -> Option<&Mesh> {
        self.lod(level)
    }

    #[must_use]
    pub const fn river_mesh(&self) -> &Mesh {
        &self.river_mesh
    }

    /// Derives sparse rough-water locations from the authoritative unsliced
    /// river mesh without retaining a second copy on the island.
    #[must_use]
    pub fn river_emitters(
        &self,
        sharpness_degrees: f32,
        spacing_metres: f32,
    ) -> Vec<crate::RiverEmitter> {
        crate::extract_river_emitters(&self.river_mesh, sharpness_degrees, spacing_metres)
    }

    #[must_use]
    pub fn rivers(&self) -> &[River] {
        &self.rivers
    }

    #[must_use]
    pub fn decorations(&self) -> &Decorations {
        self.decorations.get_or_init(|| {
            Decorations::generate(
                self.seed,
                &self.terrain,
                &self.rivers,
                self.options.terrain_size as usize * 4,
            )
        })
    }

    #[must_use]
    pub fn render(&self, width: u32, height: u32) -> Raster {
        let mut raster = Raster::new(width, height);
        raster.render(self);
        raster
    }

    #[must_use]
    pub fn height_map(&self, width: u32, height: u32) -> Vec<f32> {
        sample_grid(width, height, |u, v| self.terrain.sample(u, v))
    }

    #[must_use]
    pub fn sea_depth_map(&self, width: u32, height: u32) -> Vec<f32> {
        sample_grid(width, height, |u, v| {
            (-self.terrain.sample(u, v) / (self.options.max_height * 0.28)).clamp(0.0, 1.0)
        })
    }

    /// Bakes high-detail normal corrections and the original directional
    /// occlusion kernel for a target terrain LOD.
    #[must_use]
    pub fn surface_maps(&self, lod: usize, width: u32, height: u32) -> Option<SurfaceMaps> {
        let target = self.lod(lod)?;
        Some(bake_surface_maps(
            &self.terrain,
            (lod != 0).then_some(target),
            width.max(1),
            height.max(1),
        ))
    }

    #[must_use]
    pub fn normal_map(&self, width: u32, height: u32) -> Vec<u8> {
        let mut output = Vec::with_capacity(width as usize * height as usize * 3);
        for y in 0..height {
            let v = y as f32 / height.saturating_sub(1).max(1) as f32;
            for x in 0..width {
                let u = x as f32 / width.saturating_sub(1).max(1) as f32;
                let normal = self.terrain.sample_normal(u, v);
                output.extend([
                    ((normal.x * 0.5 + 0.5) * 255.0) as u8,
                    ((normal.y * 0.5 + 0.5) * 255.0) as u8,
                    ((normal.z * 0.5 + 0.5) * 255.0) as u8,
                ]);
            }
        }
        output
    }

    #[must_use]
    pub fn foliage_map(&self, dimension: u32) -> Vec<u32> {
        let mut map = vec![0_u32; dimension as usize * dimension as usize];
        let put = |map: &mut [u32], points: &[Vec3], shift: u32, value: u32| {
            for point in points {
                let x = (point.x * (dimension - 1) as f32).round() as usize;
                let y = (point.y * (dimension - 1) as f32).round() as usize;
                map[y * dimension as usize + x] |= value << shift;
            }
        };
        let decorations = self.decorations();
        put(&mut map, &decorations.trees, 24, 255);
        put(&mut map, &decorations.bushes, 16, 210);
        put(&mut map, &decorations.rocks, 8, 255);
        for y in 0..dimension {
            for x in 0..dimension {
                let u = x as f32 / dimension.saturating_sub(1).max(1) as f32;
                let v = y as f32 / dimension.saturating_sub(1).max(1) as f32;
                let richness = noise::fractal(self.seed, u * 5.0, v * 5.0, 3);
                map[y as usize * dimension as usize + x as usize] |=
                    ((richness * 0.5 + 0.5) * 255.0) as u32;
            }
        }
        map
    }

    /// Saves the reproducible seed and generation options.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the destination cannot be created or written.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(b"MOTURS\0\x0a")?;
        file.write_all(&self.seed.to_le_bytes())?;
        for value in [
            self.options.max_height,
            self.options.water_ratio,
            self.options.slope_multiplier,
            self.options.coastal_slope_multiplier,
            self.options.coastal_erosion_strength,
            self.options.beach_formation_strength,
            self.options.hydraulic_erosion_strength,
            self.options.hydraulic_deposition_strength,
            self.options.hydraulic_deposition_slope_degrees,
            self.options.river_lod2_source_threshold,
            self.options.river_lod1_source_threshold,
            self.options.river_broad_source_threshold,
            self.options.river_land_source_threshold,
            self.options.river_final_source_threshold,
        ] {
            file.write_all(&value.to_le_bytes())?;
        }
        file.write_all(&self.options.terrain_size.to_le_bytes())
    }

    /// Loads a saved seed/options file and deterministically regenerates it.
    ///
    /// # Errors
    ///
    /// Returns an I/O error for unreadable, truncated, or invalid input.
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut magic = [0_u8; 8];
        file.read_exact(&mut magic)?;
        if &magic[..7] != b"MOTURS\0" || !matches!(magic[7], 3..=10) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid Motu Rust free-form mesh file",
            ));
        }
        let seed = read_u64(&mut file)?;
        let max_height = read_f32(&mut file)?;
        let water_ratio = read_f32(&mut file)?;
        let slope_multiplier = read_f32(&mut file)?;
        let coastal_slope_multiplier = read_f32(&mut file)?;
        if magic[7] <= 9 {
            let _obsolete_noise_multiplier = read_f32(&mut file)?;
        }
        let defaults = IslandOptions::default();
        let coastal_erosion_strength = if magic[7] >= 7 {
            read_f32(&mut file)?
        } else {
            defaults.coastal_erosion_strength
        };
        let beach_formation_strength = if magic[7] >= 7 {
            read_f32(&mut file)?
        } else {
            defaults.beach_formation_strength
        };
        let hydraulic_erosion_strength = if magic[7] >= 4 {
            read_f32(&mut file)?
        } else {
            defaults.hydraulic_erosion_strength
        };
        let hydraulic_deposition_strength = if magic[7] >= 6 {
            read_f32(&mut file)?
        } else {
            defaults.hydraulic_deposition_strength
        };
        let hydraulic_deposition_slope_degrees = if magic[7] >= 6 {
            read_f32(&mut file)?
        } else {
            defaults.hydraulic_deposition_slope_degrees
        };
        let mut options = IslandOptions {
            max_height,
            water_ratio,
            slope_multiplier,
            coastal_slope_multiplier,
            coastal_erosion_strength,
            beach_formation_strength,
            hydraulic_erosion_strength,
            hydraulic_deposition_strength,
            hydraulic_deposition_slope_degrees,
            ..defaults
        };
        if magic[7] >= 5 {
            options.river_lod2_source_threshold = read_f32(&mut file)?;
            options.river_lod1_source_threshold = read_f32(&mut file)?;
            options.river_broad_source_threshold = read_f32(&mut file)?;
            options.river_land_source_threshold = read_f32(&mut file)?;
            options.river_final_source_threshold = read_f32(&mut file)?;
        }
        if magic[7] == 8 {
            let _obsolete_cliff_render_strength = read_f32(&mut file)?;
        }
        options.terrain_size = read_u32(&mut file)?;
        Self::generate(seed, options)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    #[must_use]
    pub fn mesh_in(&self, lod: usize, bounds: BoundingBox) -> Option<Mesh> {
        self.lod(lod).map(|mesh| mesh.sliced(bounds))
    }

    /// Clips a display mesh while retaining corrected LOD transitions.
    #[must_use]
    pub fn render_mesh_in(&self, lod: usize, bounds: BoundingBox, clamp_sides: u8) -> Option<Mesh> {
        match lod {
            0 => Some(MeshClipper::new(self.terrain.mesh()).sliced(
                bounds,
                (clamp_sides != 0).then_some(&self.coarser_lods[0]),
                clamp_sides,
            )),
            1 | 2 => {
                let mesh = self.lod(lod)?;
                self.lod(lod + 1).filter(|_| clamp_sides != 0).map_or_else(
                    || Some(mesh.sliced(bounds)),
                    |coarser| {
                        mesh.sliced_grid_clamped(bounds, 1, coarser, clamp_sides)
                            .pop()
                    },
                )
            }
            _ => None,
        }
    }

    /// Clips a display LOD into one tile batch. The global render mesh is
    /// borrowed and processed once; only returned tile buffers are allocated.
    #[must_use]
    pub fn render_mesh_grid(
        &self,
        lod: usize,
        bounds: BoundingBox,
        divisions: usize,
        clamp_sides: u8,
    ) -> Option<Vec<Mesh>> {
        match lod {
            0 => Some(MeshClipper::new(self.terrain.mesh()).sliced_grid(
                bounds,
                divisions,
                (clamp_sides != 0).then_some(&self.coarser_lods[0]),
                clamp_sides,
            )),
            1 | 2 => {
                let mesh = self.lod(lod)?;
                Some(self.lod(lod + 1).filter(|_| clamp_sides != 0).map_or_else(
                    || mesh.sliced_grid(bounds, divisions),
                    |coarser| mesh.sliced_grid_clamped(bounds, divisions, coarser, clamp_sides),
                ))
            }
            _ => None,
        }
    }

    pub(crate) fn material_values_for(&self, mesh: &Mesh) -> Vec<Vec3> {
        mesh.vertices
            .iter()
            .enumerate()
            .map(|(index, vertex)| {
                let point = mesh
                    .uv
                    .get(index)
                    .copied()
                    .unwrap_or_else(|| vertex.truncate())
                    .clamp(Vec2::ZERO, Vec2::ONE);
                self.material
                    .sample(&self.terrain, point)
                    .clamp(Vec3::ZERO, Vec3::ONE)
            })
            .collect()
    }
}

fn sharp_rock_mask(mesh: &Mesh) -> Vec<bool> {
    let _timer = StageTimer::new("material.sharp_rock");
    let adjacency = mesh.adjacency();
    let perimeter = mesh.perimeter_mask();
    mesh.vertices
        .iter()
        .enumerate()
        .map(|(vertex, position)| {
            position.z > 0.0
                && !perimeter[vertex]
                && adjacency[vertex].len() >= 3
                && mesh
                    .normal_displacement_ratio(&adjacency, vertex)
                    .is_some_and(|ratio| ratio > SHARP_ROCK_DISPLACEMENT_RATIO)
        })
        .collect()
}

#[derive(Clone, Copy)]
struct GenerationContext {
    seed: u64,
    options: IslandOptions,
    river_thresholds: [f32; 5],
}

#[derive(Default)]
struct GenerationScratch {
    hydraulic: HydraulicScratch,
    bedrock_rates: Vec<f32>,
}

impl GenerationContext {
    fn new(seed: u64, options: IslandOptions) -> Self {
        Self {
            seed,
            options,
            river_thresholds: options.river_source_thresholds(),
        }
    }
}

fn generate_base(
    seed: u64,
    options: IslandOptions,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
    let _timer = StageTimer::new("generation.base");
    let points = create_seed_points(seed, options.terrain_size as usize);
    let mut mesh = Mesh::delaunay(&points);
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let adjacency = mesh.adjacency();
    let geology = assign_elevations(&mut mesh, &adjacency, seed, options);
    material.initialize_geology(&mesh, geology);
    hydraulic_erode_stage(&mut mesh, &adjacency, &mut material, 0.45, options, scratch);
    erode_mesh(&mut mesh, &adjacency, &mut material, options, 5);
    mesh.calculate_normals();
    (mesh, material)
}

fn generate_lod2(
    base: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
    let _timer = StageTimer::new("generation.lod2");
    let tessellation = base.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(base, tessellation);
    let adjacency = mesh.adjacency();
    mesh.smooth_with(&adjacency);
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.55,
        context.options,
        scratch,
    );
    erode_mesh(&mut mesh, &adjacency, &mut material, context.options, 4);
    let mut rivers = RiverNetwork::generate(&mut mesh, &adjacency, context.river_thresholds[0]);
    rivers.shape(&mut mesh, &adjacency, &mut material, true, true);
    mesh.calculate_normals();
    (mesh, material)
}

fn generate_first_lod1(
    lod2: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
    let _timer = StageTimer::new("generation.lod1.first");
    let tessellation = lod2.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(lod2, tessellation);
    let adjacency = mesh.adjacency();
    mesh.smooth_with(&adjacency);
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.65,
        context.options,
        scratch,
    );
    erode_mesh(&mut mesh, &adjacency, &mut material, context.options, 4);
    let mut rivers = RiverNetwork::generate(&mut mesh, &adjacency, context.river_thresholds[1]);
    rivers.shape(&mut mesh, &adjacency, &mut material, false, true);
    mesh.calculate_normals();
    (mesh, material)
}

fn generate_broad_lod0(
    lod1: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
    let _timer = StageTimer::new("generation.lod0.broad");
    let tessellation = lod1.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(lod1, tessellation);
    let adjacency = mesh.adjacency();
    mesh.smooth_with(&adjacency);
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.8,
        context.options,
        scratch,
    );
    mesh.calculate_normals();

    let tessellation = mesh.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    (mesh, material) = material.into_tessellated(&mesh, tessellation);
    let adjacency = mesh.adjacency();
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.75,
        context.options,
        scratch,
    );
    erode_mesh(&mut mesh, &adjacency, &mut material, context.options, 2);
    let mut rivers = RiverNetwork::generate(&mut mesh, &adjacency, context.river_thresholds[3]);
    rivers.shape(&mut mesh, &adjacency, &mut material, true, true);
    mesh.smooth_land_with(&adjacency);
    apply_coastal_stage(
        &mut mesh,
        &mut material,
        context.seed,
        context.options,
        CoastScale::Coarse,
        1.0,
    );
    (mesh, material)
}

fn generate_detail_lod0(
    lod0: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
    let _timer = StageTimer::new("generation.lod0.detail");
    let tessellation = lod0.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(lod0, tessellation);
    let adjacency = mesh.adjacency();
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.5,
        context.options,
        scratch,
    );
    mesh.smooth_land_with(&adjacency);
    mesh.smooth_seabed_with(&adjacency);
    apply_coastal_stage(
        &mut mesh,
        &mut material,
        context.seed,
        context.options,
        CoastScale::Detail,
        0.55,
    );
    (mesh, material)
}

/// Runs the second adaptive LOD1 shaping pass while keeping flatter faces at
/// their existing density.
fn refine_lod1_again(
    lod1: &Mesh,
    material: SurfaceMaterial,
    options: IslandOptions,
    river_threshold: f32,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
    let _timer = StageTimer::new("generation.lod1.refine");
    let tessellation = lod1.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut refined, mut material) = material.into_tessellated(lod1, tessellation);
    let adjacency = refined.adjacency();
    refined.smooth_with(&adjacency);
    hydraulic_erode_stage(
        &mut refined,
        &adjacency,
        &mut material,
        0.7,
        options,
        scratch,
    );
    erode_mesh(&mut refined, &adjacency, &mut material, options, 3);
    let mut rivers = RiverNetwork::generate(&mut refined, &adjacency, river_threshold);
    rivers.shape(&mut refined, &adjacency, &mut material, false, true);
    refined.calculate_normals();
    (refined, material)
}

fn apply_coastal_stage(
    mesh: &mut Mesh,
    material: &mut SurfaceMaterial,
    seed: u64,
    options: IslandOptions,
    scale: CoastScale,
    strength_multiplier: f32,
) {
    let _timer = StageTimer::new(match scale {
        CoastScale::Coarse => "coast.coarse",
        CoastScale::Detail => "coast.detail",
    });
    let scale_seed = match scale {
        CoastScale::Coarse => seed ^ 0x94d0_49bb_1331_11eb,
        CoastScale::Detail => seed ^ 0xbf58_476d_1ce4_e5b9,
    };
    coast::evolve(
        mesh,
        material,
        scale_seed,
        options.coastal_erosion_strength * strength_multiplier,
        options.beach_formation_strength,
        scale,
    );
}

fn correct_lods(lod0: &mut Mesh, lod1: &mut Mesh, lod2: &mut Mesh) -> TriangleIndex {
    let lod1_refinement = lod1.tessellated_attributed();
    let lod2_refinement = lod2.tessellated_attributed();
    *lod1 = lod1_refinement.mesh;
    *lod2 = lod2_refinement.mesh;

    let lod0_index = TriangleIndex::new(lod0);
    pin_refined_lod(lod1, &lod1_refinement.new_vertices, lod0, &lod0_index);
    pin_refined_lod(lod2, &lod2_refinement.new_vertices, lod0, &lod0_index);

    for mesh in [lod0, lod1, lod2] {
        mesh.uv
            .iter_mut()
            .zip(&mesh.vertices)
            .for_each(|(uv, vertex)| *uv = vertex.truncate());
        mesh.calculate_normals();
    }
    lod0_index
}

fn pin_refined_lod(
    mesh: &mut Mesh,
    new_vertices: &[NewVertexStencil],
    lod0: &Mesh,
    lod0_index: &TriangleIndex,
) {
    let shared_vertex_count = mesh.vertices.len() - new_vertices.len();
    debug_assert!(lod0.vertices.len() >= shared_vertex_count);
    mesh.vertices[..shared_vertex_count].copy_from_slice(&lod0.vertices[..shared_vertex_count]);

    for stencil in new_vertices {
        let [a, b] = [
            stencil.surrounding[0] as usize,
            stencil.surrounding[1] as usize,
        ];
        debug_assert!(a < shared_vertex_count && b < shared_vertex_count);
        let point = (mesh.vertices[a].truncate() + mesh.vertices[b].truncate()) * 0.5;
        let elevation = sample_mesh_surface(lod0, lod0_index, point.x, point.y).0;
        mesh.vertices[stencil.vertex as usize] = point.extend(elevation);
    }
}

#[derive(Clone, Copy, Debug)]
struct DistanceState {
    cost: f32,
    vertex: usize,
}

impl PartialEq for DistanceState {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex && self.cost.to_bits() == other.cost.to_bits()
    }
}

impl Eq for DistanceState {}

impl PartialOrd for DistanceState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DistanceState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

fn create_seed_points(seed: u64, count: usize) -> Vec<Vec2> {
    let mut rng = Rng::new(seed);
    let mut points = Vec::with_capacity(count);
    for _ in 0..count.saturating_sub(8) {
        points.push(Vec2::new(rng.unit(), rng.unit()));
    }
    points.extend([
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 0.5),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.5, 1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(1.0, 0.5),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.5, 0.0),
    ]);
    points
}

fn assign_elevations(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    seed: u64,
    options: IslandOptions,
) -> GeologyField {
    let geology =
        GeologyField::calibrated(seed, mesh.vertices.iter().map(|vertex| vertex.truncate()));
    let scores: Vec<f32> = mesh
        .vertices
        .iter()
        .map(|vertex| {
            let dx = vertex.x.mul_add(2.0, -1.0);
            let dy = vertex.y.mul_add(2.0, -1.0);
            let radius = dx.hypot(dy);
            coast::terrain_noise(seed, vertex.truncate()).height_component() + 0.82
                - radius.powf(1.65)
        })
        .collect();
    let mut ranked = scores.clone();
    ranked.sort_unstable_by(f32::total_cmp);
    let sea_index = ((ranked.len() - 1) as f32 * options.water_ratio) as usize;
    let sea_level = ranked[sea_index];
    let perimeter = mesh.perimeter_vertices();
    let candidate_sea: Vec<bool> = scores.iter().map(|score| *score < sea_level).collect();
    let mut sea = vec![false; mesh.vertices.len()];
    let mut fringe: Vec<usize> = perimeter
        .into_iter()
        .filter(|index| candidate_sea[*index])
        .collect();
    for &vertex in &fringe {
        sea[vertex] = true;
    }
    while let Some(vertex) = fringe.pop() {
        for &neighbour in &adjacency[vertex] {
            if candidate_sea[neighbour] && !sea[neighbour] {
                sea[neighbour] = true;
                fringe.push(neighbour);
            }
        }
    }

    let distance_to_sea = graph_distances(mesh, adjacency, &sea);
    let land: Vec<bool> = sea.iter().map(|value| !value).collect();
    let distance_to_land = graph_distances(mesh, adjacency, &land);
    let max_land = distance_to_sea
        .iter()
        .zip(&land)
        .filter_map(|(distance, is_land)| is_land.then_some(*distance))
        .fold(f32::EPSILON, f32::max);
    let max_sea = distance_to_land
        .iter()
        .zip(&sea)
        .filter_map(|(distance, is_sea)| is_sea.then_some(*distance))
        .fold(f32::EPSILON, f32::max);

    for (index, vertex) in mesh.vertices.iter_mut().enumerate() {
        vertex.z = if sea[index] {
            -distance_to_land[index] / max_sea * options.max_height * 0.28
        } else {
            let normalized = distance_to_sea[index] / max_land;
            normalized.powf(options.coastal_slope_multiplier.max(0.1)) * options.max_height
        };
    }
    geology
}

fn graph_distances(mesh: &Mesh, adjacency: &Adjacency, target: &[bool]) -> Vec<f32> {
    let mut distances = vec![f32::INFINITY; mesh.vertices.len()];
    let mut queue = BinaryHeap::new();
    for (vertex, &is_target) in target.iter().enumerate() {
        if is_target {
            distances[vertex] = 0.0;
            queue.push(DistanceState { cost: 0.0, vertex });
        }
    }
    while let Some(DistanceState { cost, vertex }) = queue.pop() {
        if cost > distances[vertex] {
            continue;
        }
        for &neighbour in &adjacency[vertex] {
            let edge =
                (mesh.vertices[vertex].truncate() - mesh.vertices[neighbour].truncate()).length();
            let next = cost + edge;
            if next < distances[neighbour] {
                distances[neighbour] = next;
                queue.push(DistanceState {
                    cost: next,
                    vertex: neighbour,
                });
            }
        }
    }
    distances
}

fn hydraulic_erode_stage(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    stage_strength: f32,
    options: IslandOptions,
    scratch: &mut GenerationScratch,
) {
    let _timer = StageTimer::new("hydraulic.stage");
    scratch.bedrock_rates.clear();
    scratch.bedrock_rates.extend(
        material
            .hardnesses()
            .iter()
            .map(|&hardness| bedrock_erosion_rate(hardness)),
    );
    if std::env::var_os("MOTU_EXPERIMENTAL_MESH_FLOW").is_some() {
        hydraulic_erode_with_scratch(
            mesh,
            adjacency,
            material,
            &scratch.bedrock_rates,
            false,
            HydraulicErosionSettings::new(stage_strength, options),
            &mut scratch.hydraulic,
        );
    } else {
        hydraulic_erode_reference(
            mesh,
            adjacency,
            material,
            &scratch.bedrock_rates,
            false,
            HydraulicErosionSettings::new(stage_strength, options),
        );
    }
}

/// Proven sequential hydraulic model. Each source path observes the terrain
/// mutations made by earlier paths, which is part of its ridge and drainage
/// formation rather than an implementation detail that can be reordered.
fn hydraulic_erode_reference(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    include_sea: bool,
    settings: HydraulicErosionSettings,
) {
    if settings.erosion_strength == 0.0 {
        return;
    }
    let vertex_faces = VertexFaceAdjacency::new(mesh);
    let mut projected_areas = ProjectedFaceAreas::new(mesh);
    let mut order: Vec<usize> = (0..mesh.vertices.len()).collect();
    order
        .sort_unstable_by(|left, right| mesh.vertices[*right].z.total_cmp(&mesh.vertices[*left].z));
    let max_shift = mesh
        .vertices
        .iter()
        .map(|vertex| vertex.z)
        .fold(0.0_f32, f32::max)
        * 0.012;
    for source in order {
        if mesh.vertices[source].z <= 0.0 {
            break;
        }
        erode_reference_path(
            mesh,
            adjacency,
            material,
            bedrock_rates,
            &vertex_faces,
            &mut projected_areas,
            include_sea,
            settings,
            max_shift,
            source,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn erode_reference_path(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    vertex_faces: &VertexFaceAdjacency,
    projected_areas: &mut ProjectedFaceAreas,
    include_sea: bool,
    settings: HydraulicErosionSettings,
    max_shift: f32,
    source: usize,
) {
    let mut current = source;
    let mut speed = 0.0_f32;
    let mut sediment = 0.0_f32;
    for _ in 0..mesh.vertices.len() {
        let next = adjacency[current]
            .iter()
            .copied()
            .filter(|neighbour| mesh.vertices[*neighbour].z < mesh.vertices[current].z)
            .min_by(|left, right| mesh.vertices[*left].z.total_cmp(&mesh.vertices[*right].z));
        let Some(next) = next else {
            deposit_sediment_fan(
                mesh,
                adjacency,
                material,
                current,
                &mut sediment,
                max_shift,
                settings,
            );
            break;
        };
        if mesh.vertices[next].z < 0.0 && !include_sea {
            deposit_sediment_fan(
                mesh,
                adjacency,
                material,
                current,
                &mut sediment,
                max_shift,
                settings,
            );
            break;
        }
        let direction = mesh.vertices[current] - mesh.vertices[next];
        let distance = direction.length().max(f32::EPSILON);
        let horizontal_distance = direction.truncate().length().max(f32::EPSILON);
        let slope = direction.z / horizontal_distance;
        let sin_slope = direction.z / distance;
        let acceleration = sin_slope * sin_slope * sin_slope * distance;
        speed = speed.mul_add(0.75, acceleration * 0.25);
        let deposition_weight = deposition_weight(slope, settings);
        let (erosion_direction, slope_erosion_weight, erosion_cap, available_material) =
            if sediment <= speed {
                let normal = surface_normal_at(mesh, vertex_faces, current);
                let erosion_direction = hydraulic_erosion_direction(normal);
                let slope_erosion_weight = hydraulic_slope_erosion_weight(normal.z);
                let edge_cap = local_hydraulic_erosion_cap(mesh, adjacency, current, max_shift);
                let erosion_cap = projected_areas.safe_erosion_cap(
                    mesh,
                    vertex_faces,
                    current,
                    erosion_direction,
                    edge_cap,
                );
                let available_material = if include_sea {
                    f32::INFINITY
                } else if erosion_direction.z > 0.0 {
                    mesh.vertices[current].z.max(0.0) / erosion_direction.z
                } else {
                    0.0
                };
                (
                    erosion_direction,
                    slope_erosion_weight,
                    erosion_cap,
                    available_material,
                )
            } else {
                (Vec3::Z, 0.0, 0.0, f32::INFINITY)
            };
        let transfer = exchange_sediment(
            &mut sediment,
            settings,
            HydraulicExchange {
                capacity: speed,
                deposition_weight,
                slope_erosion_weight,
                limits: HydraulicShiftLimits {
                    deposition: max_shift,
                    erosion: erosion_cap,
                    available_material,
                },
                loose_available: material.depths()[current],
                bedrock_rate: bedrock_rates[current],
            },
        );
        apply_hydraulic_transfer(&mut mesh.vertices[current], erosion_direction, transfer);
        let loose_depth = &mut material.depths_mut()[current];
        *loose_depth = (*loose_depth - transfer.loose_removed).max(0.0) + transfer.vertical_deposit;
        if *loose_depth < LOOSE_DEPTH_EPSILON {
            *loose_depth = 0.0;
        }
        if transfer.normal_retreat > 0.0 {
            projected_areas.update_incident(mesh, vertex_faces, current);
        }
        current = next;
    }
}

fn surface_normal_at(mesh: &Mesh, vertex_faces: &VertexFaceAdjacency, vertex: usize) -> Vec3 {
    vertex_faces
        .faces(vertex)
        .iter()
        .fold(Vec3::ZERO, |normal, &face| {
            let offset = face * 3;
            let a = mesh.vertices[mesh.triangles[offset] as usize];
            let b = mesh.vertices[mesh.triangles[offset + 1] as usize];
            let c = mesh.vertices[mesh.triangles[offset + 2] as usize];
            normal + (b - a).cross(c - a)
        })
        .try_normalize()
        .unwrap_or(Vec3::Z)
}

pub(crate) fn bedrock_erosion_rate(hardness: f32) -> f32 {
    let softness = 1.0 - hardness.clamp(0.0, 1.0);
    MINIMUM_BEDROCK_EROSION_RATE + (1.0 - MINIMUM_BEDROCK_EROSION_RATE) * softness * softness
}

#[derive(Clone, Copy, Debug)]
struct HydraulicErosionSettings {
    erosion_strength: f32,
    deposition_strength: f32,
    full_deposition_slope: f32,
    maximum_deposition_slope: f32,
}

#[derive(Clone, Copy, Debug)]
struct HydraulicShiftLimits {
    deposition: f32,
    erosion: f32,
    available_material: f32,
}

#[derive(Clone, Copy, Debug)]
struct HydraulicExchange {
    capacity: f32,
    deposition_weight: f32,
    slope_erosion_weight: f32,
    limits: HydraulicShiftLimits,
    loose_available: f32,
    bedrock_rate: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct HydraulicTransfer {
    normal_retreat: f32,
    vertical_deposit: f32,
    loose_removed: f32,
    bedrock_removed: f32,
}

#[derive(Clone, Copy)]
struct ThermalTransfer {
    target: usize,
    height: f32,
    loose: f32,
}

struct VertexFaceAdjacency {
    offsets: Vec<usize>,
    faces: Vec<usize>,
}

struct ProjectedFaceAreas {
    reference: Vec<f32>,
    current: Vec<f32>,
}

impl VertexFaceAdjacency {
    fn new(mesh: &Mesh) -> Self {
        let mut offsets = vec![0; mesh.vertices.len() + 1];
        for &vertex in &mesh.triangles {
            offsets[vertex as usize + 1] += 1;
        }
        for index in 1..offsets.len() {
            offsets[index] += offsets[index - 1];
        }

        let mut next = offsets[..mesh.vertices.len()].to_vec();
        let mut faces = vec![0; mesh.triangles.len()];
        for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
            for &vertex in triangle {
                let vertex = vertex as usize;
                faces[next[vertex]] = face;
                next[vertex] += 1;
            }
        }
        Self { offsets, faces }
    }

    fn faces(&self, vertex: usize) -> &[usize] {
        &self.faces[self.offsets[vertex]..self.offsets[vertex + 1]]
    }
}

impl ProjectedFaceAreas {
    fn new(mesh: &Mesh) -> Self {
        let reference: Vec<f32> = (0..mesh.triangles.len() / 3)
            .map(|face| projected_face_area(mesh, face))
            .collect();
        let current = reference.clone();
        Self { reference, current }
    }

    fn safe_erosion_cap(
        &self,
        mesh: &Mesh,
        vertex_faces: &VertexFaceAdjacency,
        vertex: usize,
        normal: Vec3,
        requested_cap: f32,
    ) -> f32 {
        let candidate = mesh.vertices[vertex] - normal * requested_cap;
        vertex_faces
            .faces(vertex)
            .iter()
            .fold(requested_cap, |safe_cap, &face| {
                let reference = self.reference[face];
                if reference.abs() <= f32::EPSILON {
                    return 0.0;
                }
                let orientation = reference.signum();
                let current = self.current[face] * orientation;
                if current <= 0.0 {
                    return 0.0;
                }
                let minimum = (reference.abs() * HYDRAULIC_MIN_PROJECTED_AREA_RATIO).min(current);
                let candidate =
                    projected_face_area_with_vertex(mesh, face, vertex, candidate) * orientation;
                if candidate >= minimum {
                    safe_cap
                } else {
                    let fraction = ((current - minimum) / (current - candidate)).clamp(0.0, 1.0);
                    safe_cap.min(requested_cap * fraction)
                }
            })
    }

    fn update_incident(&mut self, mesh: &Mesh, vertex_faces: &VertexFaceAdjacency, vertex: usize) {
        for &face in vertex_faces.faces(vertex) {
            self.current[face] = projected_face_area(mesh, face);
        }
    }
}

impl HydraulicErosionSettings {
    fn new(stage_strength: f32, options: IslandOptions) -> Self {
        let maximum_deposition_slope = options
            .hydraulic_deposition_slope_degrees
            .to_radians()
            .tan();
        Self {
            erosion_strength: stage_strength * options.hydraulic_erosion_strength,
            deposition_strength: options.hydraulic_deposition_strength,
            full_deposition_slope: (options.hydraulic_deposition_slope_degrees / 3.0)
                .to_radians()
                .tan(),
            maximum_deposition_slope,
        }
    }
}

#[cfg(test)]
fn hydraulic_erode(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    include_sea: bool,
    settings: HydraulicErosionSettings,
) {
    let mut scratch = HydraulicScratch::default();
    hydraulic_erode_with_scratch(
        mesh,
        adjacency,
        material,
        bedrock_rates,
        include_sea,
        settings,
        &mut scratch,
    );
}

const HYDRAULIC_FLOW_ITERATIONS: usize = 16;
const NO_DOWNSTREAM: usize = usize::MAX;

#[derive(Default)]
struct HydraulicScratch {
    order: Vec<usize>,
    downstream: Vec<usize>,
    control_areas: Vec<f32>,
    water: Vec<f32>,
    sediment: Vec<f32>,
}

impl HydraulicScratch {
    fn resize(&mut self, vertex_count: usize) {
        self.order.clear();
        self.order.extend(0..vertex_count);
        self.downstream.resize(vertex_count, NO_DOWNSTREAM);
        self.control_areas.resize(vertex_count, 0.0);
        self.water.resize(vertex_count, 0.0);
        self.sediment.resize(vertex_count, 0.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn hydraulic_erode_with_scratch(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    include_sea: bool,
    settings: HydraulicErosionSettings,
    scratch: &mut HydraulicScratch,
) {
    if settings.erosion_strength == 0.0 {
        return;
    }

    debug_assert_eq!(material.depths().len(), mesh.vertices.len());
    debug_assert_eq!(bedrock_rates.len(), mesh.vertices.len());
    let vertex_faces = VertexFaceAdjacency::new(mesh);
    let mut projected_areas = ProjectedFaceAreas::new(mesh);
    let max_shift = mesh
        .vertices
        .iter()
        .map(|vertex| vertex.z)
        .fold(0.0_f32, f32::max)
        * 0.012;
    scratch.resize(mesh.vertices.len());

    for _ in 0..HYDRAULIC_FLOW_ITERATIONS {
        calculate_normals_with_faces(mesh, &vertex_faces);
        prepare_hydraulic_flow(mesh, adjacency, include_sea, scratch);
        apply_hydraulic_flow(
            mesh,
            adjacency,
            material,
            bedrock_rates,
            &vertex_faces,
            &mut projected_areas,
            include_sea,
            settings,
            max_shift,
            scratch,
        );
    }
}

fn calculate_normals_with_faces(mesh: &mut Mesh, vertex_faces: &VertexFaceAdjacency) {
    let vertices = &mesh.vertices;
    let triangles = &mesh.triangles;
    mesh.normals = (0..vertices.len())
        .into_par_iter()
        .map(|vertex| {
            vertex_faces
                .faces(vertex)
                .iter()
                .fold(Vec3::ZERO, |normal, &face| {
                    let offset = face * 3;
                    let a = vertices[triangles[offset] as usize];
                    let b = vertices[triangles[offset + 1] as usize];
                    let c = vertices[triangles[offset + 2] as usize];
                    normal + (b - a).cross(c - a)
                })
                .try_normalize()
                .unwrap_or(Vec3::Z)
        })
        .collect();
}

fn prepare_hydraulic_flow(
    mesh: &Mesh,
    adjacency: &Adjacency,
    include_sea: bool,
    scratch: &mut HydraulicScratch,
) {
    scratch
        .downstream
        .par_iter_mut()
        .enumerate()
        .for_each(|(vertex, output)| {
            let source = mesh.vertices[vertex];
            *output = adjacency[vertex]
                .iter()
                .copied()
                .filter(|&candidate| {
                    let height = mesh.vertices[candidate].z;
                    height < source.z && (include_sea || height >= 0.0)
                })
                .max_by(|&left, &right| {
                    downhill_gradient(source, mesh.vertices[left])
                        .total_cmp(&downhill_gradient(source, mesh.vertices[right]))
                        .then_with(|| right.cmp(&left))
                })
                .unwrap_or(NO_DOWNSTREAM);
        });

    scratch.control_areas.fill(0.0);
    for triangle in mesh.triangles.chunks_exact(3) {
        let [a, b, c] = [
            mesh.vertices[triangle[0] as usize].truncate(),
            mesh.vertices[triangle[1] as usize].truncate(),
            mesh.vertices[triangle[2] as usize].truncate(),
        ];
        let share = (b - a).perp_dot(c - a).abs() / 6.0;
        for &vertex in triangle {
            scratch.control_areas[vertex as usize] += share;
        }
    }
    let (land_area, land_vertices) = mesh
        .vertices
        .iter()
        .zip(&scratch.control_areas)
        .filter(|(vertex, _)| include_sea || vertex.z > 0.0)
        .fold((0.0_f32, 0_usize), |(area, count), (_, &control_area)| {
            (area + control_area, count + 1)
        });
    let mean_area = land_area / land_vertices.max(1) as f32;
    scratch
        .water
        .par_iter_mut()
        .zip(&scratch.control_areas)
        .zip(&mesh.vertices)
        .for_each(|((water, &area), vertex)| {
            *water = if (include_sea || vertex.z > 0.0) && mean_area > f32::EPSILON {
                area / mean_area
            } else {
                0.0
            };
        });
    scratch.sediment.fill(0.0);
    scratch.order.sort_unstable_by(|&left, &right| {
        mesh.vertices[right]
            .z
            .total_cmp(&mesh.vertices[left].z)
            .then_with(|| left.cmp(&right))
    });
    for &vertex in &scratch.order {
        let downstream = scratch.downstream[vertex];
        if downstream != NO_DOWNSTREAM {
            scratch.water[downstream] += scratch.water[vertex];
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_hydraulic_flow(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    bedrock_rates: &[f32],
    vertex_faces: &VertexFaceAdjacency,
    projected_areas: &mut ProjectedFaceAreas,
    include_sea: bool,
    settings: HydraulicErosionSettings,
    max_shift: f32,
    scratch: &mut HydraulicScratch,
) {
    for &current in &scratch.order {
        if mesh.vertices[current].z <= 0.0 {
            continue;
        }
        let next = scratch.downstream[current];
        let mut sediment = scratch.sediment[current];
        if next == NO_DOWNSTREAM {
            deposit_sediment_fan(
                mesh,
                adjacency,
                material,
                current,
                &mut sediment,
                max_shift,
                settings,
            );
            continue;
        }

        let direction = mesh.vertices[current] - mesh.vertices[next];
        let distance = direction.length().max(f32::EPSILON);
        let horizontal_distance = direction.truncate().length().max(f32::EPSILON);
        let slope = direction.z / horizontal_distance;
        let sin_slope = direction.z / distance;
        let acceleration = sin_slope * sin_slope * sin_slope * distance;
        let capacity = acceleration * scratch.water[current].max(1.0);
        let deposition_weight = deposition_weight(slope, settings);
        let normal = mesh.normals[current];
        let erosion_direction = hydraulic_erosion_direction(normal);
        let slope_erosion_weight = hydraulic_slope_erosion_weight(normal.z);
        let edge_cap = local_hydraulic_erosion_cap(mesh, adjacency, current, max_shift);
        let erosion_cap = projected_areas.safe_erosion_cap(
            mesh,
            vertex_faces,
            current,
            erosion_direction,
            edge_cap,
        );
        let available_material = if include_sea {
            f32::INFINITY
        } else if erosion_direction.z > 0.0 {
            mesh.vertices[current].z.max(0.0) / erosion_direction.z
        } else {
            0.0
        };
        let transfer = exchange_sediment(
            &mut sediment,
            settings,
            HydraulicExchange {
                capacity,
                deposition_weight,
                slope_erosion_weight,
                limits: HydraulicShiftLimits {
                    deposition: max_shift,
                    erosion: erosion_cap,
                    available_material,
                },
                loose_available: material.depths()[current],
                bedrock_rate: bedrock_rates[current],
            },
        );
        apply_hydraulic_transfer(&mut mesh.vertices[current], erosion_direction, transfer);
        let loose_depth = &mut material.depths_mut()[current];
        *loose_depth = (*loose_depth - transfer.loose_removed).max(0.0) + transfer.vertical_deposit;
        if *loose_depth < LOOSE_DEPTH_EPSILON {
            *loose_depth = 0.0;
        }
        if transfer.normal_retreat > 0.0 {
            projected_areas.update_incident(mesh, vertex_faces, current);
        }
        scratch.sediment[next] += sediment;
    }
}

fn downhill_gradient(source: Vec3, target: Vec3) -> f32 {
    (source.z - target.z)
        / source
            .truncate()
            .distance(target.truncate())
            .max(f32::EPSILON)
}

fn hydraulic_slope_erosion_weight(normal_z: f32) -> f32 {
    let vertical_alignment = normal_z.clamp(0.0, 1.0);
    let horizontal_alignment = (1.0 - vertical_alignment * vertical_alignment).sqrt();
    2.0 * vertical_alignment * horizontal_alignment
}

fn hydraulic_erosion_direction(normal: Vec3) -> Vec3 {
    let vertical_alignment = normal.z.clamp(0.0, 1.0);
    let beyond_forty_five_degrees =
        (1.0 - 2.0 * vertical_alignment * vertical_alignment).clamp(0.0, 1.0);
    let vertical_blend = smooth_unit_interval(beyond_forty_five_degrees);
    normal.lerp(Vec3::Z, vertical_blend).normalize_or_zero()
}

fn smooth_unit_interval(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn local_hydraulic_erosion_cap(
    mesh: &Mesh,
    adjacency: &Adjacency,
    vertex: usize,
    global_cap: f32,
) -> f32 {
    let minimum_edge = adjacency[vertex]
        .iter()
        .map(|&neighbour| mesh.vertices[vertex].distance(mesh.vertices[neighbour]))
        .fold(f32::INFINITY, f32::min);
    global_cap.min(minimum_edge * HYDRAULIC_EDGE_SHIFT_LIMIT)
}

fn projected_face_area(mesh: &Mesh, face: usize) -> f32 {
    let offset = face * 3;
    let [a, b, c] = [
        mesh.vertices[mesh.triangles[offset] as usize].truncate(),
        mesh.vertices[mesh.triangles[offset + 1] as usize].truncate(),
        mesh.vertices[mesh.triangles[offset + 2] as usize].truncate(),
    ];
    (b - a).perp_dot(c - a)
}

fn projected_face_area_with_vertex(
    mesh: &Mesh,
    face: usize,
    moved_vertex: usize,
    candidate: Vec3,
) -> f32 {
    let offset = face * 3;
    let indices = [
        mesh.triangles[offset] as usize,
        mesh.triangles[offset + 1] as usize,
        mesh.triangles[offset + 2] as usize,
    ];
    let points = indices.map(|index| {
        if index == moved_vertex {
            candidate.truncate()
        } else {
            mesh.vertices[index].truncate()
        }
    });
    (points[1] - points[0]).perp_dot(points[2] - points[0])
}

fn apply_hydraulic_transfer(vertex: &mut Vec3, normal: Vec3, transfer: HydraulicTransfer) {
    *vertex -= normal * transfer.normal_retreat;
    vertex.z += transfer.vertical_deposit;
}

fn deposition_weight(slope: f32, settings: HydraulicErosionSettings) -> f32 {
    let width =
        (settings.maximum_deposition_slope - settings.full_deposition_slope).max(f32::EPSILON);
    let normalized = ((slope - settings.full_deposition_slope) / width).clamp(0.0, 1.0);
    1.0 - smooth_unit_interval(normalized)
}

fn exchange_sediment(
    sediment: &mut f32,
    settings: HydraulicErosionSettings,
    exchange: HydraulicExchange,
) -> HydraulicTransfer {
    let difference = *sediment - exchange.capacity;
    if difference > 0.0 {
        let rate = (settings.deposition_strength * 0.35 * exchange.deposition_weight).min(1.0);
        let deposited = (difference * rate)
            .min(exchange.limits.deposition)
            .min(*sediment);
        *sediment -= deposited;
        HydraulicTransfer {
            vertical_deposit: deposited,
            ..HydraulicTransfer::default()
        }
    } else {
        let erosion_weight = 1.0 - exchange.deposition_weight;
        let requested = (-difference
            * settings.erosion_strength
            * erosion_weight
            * exchange.slope_erosion_weight)
            .min(exchange.limits.erosion)
            .min(exchange.limits.available_material);
        let loose_removed = requested.min(exchange.loose_available.max(0.0));
        let bedrock_removed =
            (requested - loose_removed).max(0.0) * exchange.bedrock_rate.clamp(0.0, 1.0);
        let normal_retreat = loose_removed + bedrock_removed;
        *sediment += normal_retreat;
        HydraulicTransfer {
            normal_retreat,
            loose_removed,
            bedrock_removed,
            ..HydraulicTransfer::default()
        }
    }
}

fn deposit_sediment_fan(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    center: usize,
    sediment: &mut f32,
    max_shift: f32,
    settings: HydraulicErosionSettings,
) {
    let rate = (settings.deposition_strength * 0.35).min(1.0);
    if *sediment <= 0.0 || rate == 0.0 {
        return;
    }

    let center_position = mesh.vertices[center];
    let mut total_weight = 1.0_f32;
    for &neighbour in &adjacency[center] {
        let candidate = mesh.vertices[neighbour];
        if candidate.z < 0.0 {
            continue;
        }
        let horizontal_distance = (candidate - center_position)
            .truncate()
            .length()
            .max(f32::EPSILON);
        let slope = ((candidate.z - center_position.z) / horizontal_distance).abs();
        total_weight += deposition_weight(slope, settings);
    }

    let total_deposit = (*sediment * rate).min(max_shift * total_weight);
    let deposit_per_weight = total_deposit / total_weight;
    mesh.vertices[center].z += deposit_per_weight;
    material.depths_mut()[center] += deposit_per_weight;
    for &neighbour in &adjacency[center] {
        let candidate = mesh.vertices[neighbour];
        if candidate.z < 0.0 {
            continue;
        }
        let horizontal_distance = (candidate - center_position)
            .truncate()
            .length()
            .max(f32::EPSILON);
        let slope = ((candidate.z - center_position.z) / horizontal_distance).abs();
        let deposit = deposit_per_weight * deposition_weight(slope, settings);
        mesh.vertices[neighbour].z += deposit;
        material.depths_mut()[neighbour] += deposit;
    }
    *sediment -= total_deposit;
}

fn erode_mesh(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    material: &mut SurfaceMaterial,
    options: IslandOptions,
    passes: usize,
) {
    let _timer = StageTimer::new("thermal.stage");
    let mut delta = vec![0.0; mesh.vertices.len()];
    let mut loose_delta = vec![0.0; mesh.vertices.len()];
    let talus = options.max_height * 0.006 / options.slope_multiplier.max(0.1);
    for _ in 0..passes {
        delta.fill(0.0);
        loose_delta.fill(0.0);
        let transfers: Vec<Option<ThermalTransfer>> = (0..adjacency.len())
            .into_par_iter()
            .map(|index| {
                let neighbours = &adjacency[index];
                let height = mesh.vertices[index].z;
                if height <= 0.0 {
                    return None;
                }
                let lowest = neighbours
                    .iter()
                    .copied()
                    .filter(|neighbour| mesh.vertices[*neighbour].z > 0.0)
                    .min_by(|left, right| {
                        mesh.vertices[*left].z.total_cmp(&mesh.vertices[*right].z)
                    })?;
                let difference = height - mesh.vertices[lowest].z;
                (difference > talus).then(|| {
                    let transfer = (difference - talus) * 0.18;
                    ThermalTransfer {
                        target: lowest,
                        height: transfer,
                        loose: material.depths()[index].min(transfer),
                    }
                })
            })
            .collect();
        for (index, transfer) in transfers.into_iter().enumerate() {
            let Some(transfer) = transfer else {
                continue;
            };
            delta[index] -= transfer.height;
            delta[transfer.target] += transfer.height;
            loose_delta[index] -= transfer.loose;
            loose_delta[transfer.target] += transfer.height;
        }
        mesh.vertices
            .iter_mut()
            .zip(&delta)
            .for_each(|(vertex, change)| vertex.z += change);
        material
            .depths_mut()
            .iter_mut()
            .zip(&loose_delta)
            .for_each(|(depth, change)| *depth = (*depth + change).max(0.0));
    }
}

fn barycentric(point: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Option<[f32; 3]> {
    let denominator = (b.y - c.y).mul_add(a.x - c.x, (c.x - b.x) * (a.y - c.y));
    if denominator.abs() <= f32::EPSILON {
        return None;
    }
    let first = (b.y - c.y).mul_add(point.x - c.x, (c.x - b.x) * (point.y - c.y)) / denominator;
    let second = (c.y - a.y).mul_add(point.x - c.x, (a.x - c.x) * (point.y - c.y)) / denominator;
    let third = 1.0 - first - second;
    (first >= -1.0e-5 && second >= -1.0e-5 && third >= -1.0e-5).then_some([first, second, third])
}

fn bin_coordinate(value: f32, dimension: usize) -> usize {
    ((value.clamp(0.0, 1.0) * dimension as f32).floor() as usize).min(dimension - 1)
}

fn triangle_bin_bounds(
    mesh: &Mesh,
    triangle: &[u32],
    dimension: usize,
) -> (usize, usize, usize, usize) {
    let points = [
        mesh.vertices[triangle[0] as usize],
        mesh.vertices[triangle[1] as usize],
        mesh.vertices[triangle[2] as usize],
    ];
    let min_x = points.iter().map(|point| point.x).fold(f32::MAX, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(f32::MIN, f32::max);
    let min_y = points.iter().map(|point| point.y).fold(f32::MAX, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(f32::MIN, f32::max);
    (
        bin_coordinate(min_x, dimension),
        bin_coordinate(max_x, dimension),
        bin_coordinate(min_y, dimension),
        bin_coordinate(max_y, dimension),
    )
}

const OCCLUSION_OFFSETS: [(isize, isize); 15] = [
    (-8, -2),
    (-4, 2),
    (-2, 1),
    (-1, 1),
    (-1, -1),
    (-1, -4),
    (1, 2),
    (1, 1),
    (1, -1),
    (1, -2),
    (2, 4),
    (2, -1),
    (2, -8),
    (4, -2),
    (8, 2),
];

fn bake_surface_maps(
    high_detail: &Terrain,
    target: Option<&Mesh>,
    width: u32,
    height: u32,
) -> SurfaceMaps {
    let _timer = StageTimer::new("surface_maps.bake");
    let width_usize = width as usize;
    let height_usize = height as usize;
    let pixel_count = width_usize * height_usize;
    let mut samples = vec![SurfaceSample::default(); pixel_count];
    let mut normal_rgb = vec![0_u8; pixel_count * 3];
    let thread_count = surface_map_thread_count(pixel_count, height_usize);
    let rows_per_chunk = height_usize.div_ceil(thread_count);

    if let Some(target) = target {
        let target_index = TriangleIndex::new(target);
        thread::scope(|scope| {
            for (chunk, (sample_rows, normal_rows)) in samples
                .chunks_mut(rows_per_chunk * width_usize)
                .zip(normal_rgb.chunks_mut(rows_per_chunk * width_usize * 3))
                .enumerate()
            {
                let start_y = chunk * rows_per_chunk;
                let target_index = &target_index;
                scope.spawn(move || {
                    bake_surface_rows(
                        high_detail,
                        target,
                        target_index,
                        width,
                        height,
                        start_y,
                        sample_rows,
                        normal_rows,
                    );
                });
            }
        });
    } else {
        thread::scope(|scope| {
            for (chunk, (sample_rows, normal_rows)) in samples
                .chunks_mut(rows_per_chunk * width_usize)
                .zip(normal_rgb.chunks_mut(rows_per_chunk * width_usize * 3))
                .enumerate()
            {
                let start_y = chunk * rows_per_chunk;
                scope.spawn(move || {
                    bake_surface_sample_rows(
                        high_detail,
                        width,
                        height,
                        start_y,
                        sample_rows,
                        normal_rows,
                    );
                });
            }
        });
    }

    let mut occlusion = vec![u8::MAX; pixel_count];
    thread::scope(|scope| {
        for (chunk, rows) in occlusion
            .chunks_mut(rows_per_chunk * width_usize)
            .enumerate()
        {
            let start_y = chunk * rows_per_chunk;
            let samples = &samples;
            scope.spawn(move || {
                bake_occlusion_rows(samples, width_usize, height_usize, start_y, rows);
            });
        }
    });

    SurfaceMaps {
        width,
        height,
        normal_rgb,
        occlusion,
    }
}

fn bake_surface_sample_rows(
    high_detail: &Terrain,
    width: u32,
    height: u32,
    start_y: usize,
    samples: &mut [SurfaceSample],
    normal_rgb: &mut [u8],
) {
    let width_usize = width as usize;
    for (local_y, (sample_row, normal_row)) in samples
        .chunks_exact_mut(width_usize)
        .zip(normal_rgb.chunks_exact_mut(width_usize * 3))
        .enumerate()
    {
        let y = start_y + local_y;
        let v = y as f32 / height.saturating_sub(1).max(1) as f32;
        for (x, (sample, normal_pixel)) in sample_row
            .iter_mut()
            .zip(normal_row.chunks_exact_mut(3))
            .enumerate()
        {
            let u = x as f32 / width.saturating_sub(1).max(1) as f32;
            let (elevation, normal) = high_detail.sample_surface(u, v);
            *sample = SurfaceSample {
                position: Vec3::new(u, v, elevation),
                normal,
            };
            normal_pixel[0] = signed_normal_byte(normal.x);
            normal_pixel[1] = signed_normal_byte(normal.y);
            normal_pixel[2] = signed_normal_byte(normal.z);
        }
    }
}

fn surface_map_thread_count(pixel_count: usize, height: usize) -> usize {
    if pixel_count < 65_536 {
        return 1;
    }
    thread::available_parallelism()
        .map_or(1, usize::from)
        .min(height)
}

#[allow(clippy::too_many_arguments)]
fn bake_surface_rows(
    high_detail: &Terrain,
    target: &Mesh,
    target_index: &TriangleIndex,
    width: u32,
    height: u32,
    start_y: usize,
    samples: &mut [SurfaceSample],
    normal_rgb: &mut [u8],
) {
    let width_usize = width as usize;
    for (local_y, (sample_row, normal_row)) in samples
        .chunks_exact_mut(width_usize)
        .zip(normal_rgb.chunks_exact_mut(width_usize * 3))
        .enumerate()
    {
        let y = start_y + local_y;
        let v = y as f32 / height.saturating_sub(1).max(1) as f32;
        for (x, (sample, normal_pixel)) in sample_row
            .iter_mut()
            .zip(normal_row.chunks_exact_mut(3))
            .enumerate()
        {
            let u = x as f32 / width.saturating_sub(1).max(1) as f32;
            let (elevation, high_normal) = high_detail.sample_surface(u, v);
            let (_, target_normal) = sample_mesh_surface(target, target_index, u, v);
            *sample = SurfaceSample {
                position: Vec3::new(u, v, elevation),
                normal: high_normal,
            };
            let detail_normal = (Vec3::Z + high_normal - target_normal)
                .try_normalize()
                .unwrap_or(Vec3::Z);
            normal_pixel[0] = signed_normal_byte(detail_normal.y);
            normal_pixel[1] = signed_normal_byte(detail_normal.x);
            normal_pixel[2] = (detail_normal.z.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
}

fn signed_normal_byte(value: f32) -> u8 {
    value.mul_add(127.5, 127.5).clamp(0.0, 255.0) as u8
}

fn bake_occlusion_rows(
    samples: &[SurfaceSample],
    width: usize,
    height: usize,
    start_y: usize,
    output: &mut [u8],
) {
    for (local_y, row) in output.chunks_exact_mut(width).enumerate() {
        let y = start_y + local_y;
        for (x, value) in row.iter_mut().enumerate() {
            let sample = samples[y * width + x];
            let mut total = 0.0_f32;
            let mut count = 0_u32;
            for (offset_x, offset_y) in OCCLUSION_OFFSETS {
                let Some(px) = x.checked_add_signed(offset_x) else {
                    continue;
                };
                let Some(py) = y.checked_add_signed(offset_y) else {
                    continue;
                };
                if px >= width || py >= height {
                    continue;
                }
                let direction = samples[py * width + px].position - sample.position;
                let Some(direction) = direction.try_normalize() else {
                    continue;
                };
                total += direction.dot(sample.normal).max(0.0);
                count += 1;
            }
            if count > 0 {
                *value = ((1.0 - total / count as f32) * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn sample_grid(width: u32, height: u32, mut sample: impl FnMut(f32, f32) -> f32) -> Vec<f32> {
    let mut output = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        let v = y as f32 / height.saturating_sub(1).max(1) as f32;
        for x in 0..width {
            let u = x as f32 / width.saturating_sub(1).max(1) as f32;
            output.push(sample(u, v));
        }
    }
    output
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_f32(reader: &mut impl Read) -> io::Result<f32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(f32::from_le_bytes(bytes))
}

#[cfg(test)]
mod hydraulic_tests {
    use super::{
        HydraulicErosionSettings, HydraulicExchange, HydraulicShiftLimits, HydraulicTransfer, Mesh,
        ProjectedFaceAreas, SurfaceMaterial, Terrain, TerrainMaterialField, TriangleIndex, Vec2,
        Vec3, VertexFaceAdjacency, apply_hydraulic_transfer, correct_lods, deposition_weight,
        exchange_sediment, hydraulic_erode, hydraulic_erosion_direction,
        hydraulic_slope_erosion_weight, local_hydraulic_erosion_cap, sample_mesh_surface,
        sharp_rock_mask,
    };

    fn settings() -> HydraulicErosionSettings {
        HydraulicErosionSettings {
            erosion_strength: 1.0,
            deposition_strength: 2.0,
            full_deposition_slope: 4.0_f32.to_radians().tan(),
            maximum_deposition_slope: 12.0_f32.to_radians().tan(),
        }
    }

    fn shift_limits(maximum: f32, available_material: f32) -> HydraulicShiftLimits {
        HydraulicShiftLimits {
            deposition: maximum,
            erosion: maximum,
            available_material,
        }
    }

    fn exchange(
        capacity: f32,
        deposition_weight: f32,
        slope_erosion_weight: f32,
        limits: HydraulicShiftLimits,
        loose_available: f32,
        bedrock_rate: f32,
    ) -> HydraulicExchange {
        HydraulicExchange {
            capacity,
            deposition_weight,
            slope_erosion_weight,
            limits,
            loose_available,
            bedrock_rate,
        }
    }

    #[test]
    fn deposition_fades_smoothly_between_gentle_and_steep_slopes() {
        let settings = settings();
        let gentle = deposition_weight(2.0_f32.to_radians().tan(), settings);
        let transition = deposition_weight(8.0_f32.to_radians().tan(), settings);
        let steep = deposition_weight(20.0_f32.to_radians().tan(), settings);
        assert!((gentle - 1.0).abs() < f32::EPSILON);
        assert!(transition > 0.0 && transition < gentle);
        assert!(steep.abs() < f32::EPSILON);
    }

    #[test]
    fn final_sharp_rock_pass_marks_a_protrusion_but_not_an_inclined_plane() {
        let points: Vec<Vec2> = (0..=4)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 / 4.0, y as f32 / 4.0)))
            .collect();
        let center = points
            .iter()
            .position(|point| *point == Vec2::splat(0.5))
            .unwrap();
        let mut mesh = Mesh::delaunay(&points);
        mesh.vertices.iter_mut().for_each(|vertex| vertex.z = 0.05);
        mesh.vertices[center].z = 0.25;
        mesh.calculate_normals();

        let forced_rock = sharp_rock_mask(&mesh);
        assert!(forced_rock[center]);
        let material = SurfaceMaterial::empty(mesh.vertices.len());
        let field = TerrainMaterialField::from_surface(
            &material,
            &vec![false; mesh.vertices.len()],
            &forced_rock,
        );
        assert_eq!(
            field.values[center].x.to_bits(),
            super::FORCED_ROCK_HARDNESS.to_bits()
        );

        mesh.vertices
            .iter_mut()
            .for_each(|vertex| vertex.z = vertex.x.mul_add(0.2, vertex.y * 0.1) + 0.05);
        mesh.calculate_normals();
        assert!(!sharp_rock_mask(&mesh).into_iter().any(|marked| marked));
    }

    #[test]
    fn sediment_exchange_conserves_every_applied_transfer() {
        let settings = settings();
        let mut sediment = 1.0;
        let before = sediment;
        let deposited = exchange_sediment(
            &mut sediment,
            settings,
            exchange(0.2, 1.0, 1.0, shift_limits(10.0, f32::INFINITY), 0.0, 1.0),
        );
        assert!(deposited.vertical_deposit > 0.0);
        assert!((sediment + deposited.vertical_deposit - before).abs() < 1.0e-6);

        let mut sediment = 0.0;
        let before = sediment;
        let eroded = exchange_sediment(
            &mut sediment,
            settings,
            exchange(1.0, 0.0, 1.0, shift_limits(0.5, 0.3), 0.0, 1.0),
        );
        assert!(eroded.normal_retreat > 0.0);
        assert!((sediment - eroded.normal_retreat - before).abs() < 1.0e-6);

        let mut sediment = 1.0;
        let steep_deposit = exchange_sediment(
            &mut sediment,
            settings,
            exchange(0.2, 0.0, 1.0, shift_limits(10.0, f32::INFINITY), 0.0, 1.0),
        );
        assert!(steep_deposit.vertical_deposit.abs() < f32::EPSILON);
    }

    #[test]
    fn loose_material_is_removed_before_hard_bedrock() {
        let settings = settings();
        let mut sediment = 0.0;
        let transfer = exchange_sediment(
            &mut sediment,
            settings,
            exchange(1.0, 0.0, 1.0, shift_limits(0.5, f32::INFINITY), 0.2, 0.1),
        );

        assert!((transfer.loose_removed - 0.2).abs() < 1.0e-6);
        assert!((transfer.bedrock_removed - 0.03).abs() < 1.0e-6);
        assert!((transfer.normal_retreat - 0.23).abs() < 1.0e-6);
        assert!((sediment - transfer.normal_retreat).abs() < 1.0e-6);
    }

    #[test]
    fn material_volume_is_conserved_across_adaptive_tessellation() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(-1.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 0.8),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4],
            uv: Vec::new(),
        };
        mesh.calculate_normals();
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        material.deposited_depth = vec![0.01, 0.02, 0.03, 0.04, 0.08];
        let old_volume = material.volume(&mesh);
        let tessellation = mesh.tessellated_displaced_attributed(0.1);

        let (tessellated, material) = material.into_tessellated(&mesh, tessellation);
        let new_volume = material.volume(&tessellated);
        let relative_error = (new_volume - old_volume).abs() / old_volume;

        assert!(tessellated.vertices.len() > mesh.vertices.len());
        assert_eq!(material.depths().len(), tessellated.vertices.len());
        assert!(relative_error < 1.0e-6, "relative error: {relative_error}");
        assert!(
            material
                .depths()
                .iter()
                .all(|depth| depth.is_finite() && *depth >= 0.0)
        );
    }

    #[test]
    fn hydraulic_erosion_peaks_at_forty_five_degrees_and_stops_at_vertical() {
        let flat = hydraulic_slope_erosion_weight(1.0);
        let thirty_degrees = hydraulic_slope_erosion_weight(0.866_025_4);
        let forty_five_degrees = hydraulic_slope_erosion_weight(0.707_106_77);
        let sixty_degrees = hydraulic_slope_erosion_weight(0.5);
        let steep = hydraulic_slope_erosion_weight(0.207_911_69);
        let near_vertical = hydraulic_slope_erosion_weight(0.017_452_406);

        assert_eq!(flat.to_bits(), 0.0_f32.to_bits());
        assert!((forty_five_degrees - 1.0).abs() < 1.0e-6);
        assert!((thirty_degrees - sixty_degrees).abs() < 1.0e-6);
        assert!(forty_five_degrees > thirty_degrees);
        assert!(sixty_degrees > steep);
        assert!(near_vertical < steep);
        assert_eq!(
            hydraulic_slope_erosion_weight(0.0).to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            hydraulic_slope_erosion_weight(-0.1).to_bits(),
            0.0_f32.to_bits()
        );
    }

    #[test]
    fn steep_hydraulic_erosion_blends_from_the_normal_toward_vertical() {
        let thirty_degrees = Vec3::new(0.5, 0.0, 0.866_025_4);
        let forty_five_degrees = Vec3::new(0.707_106_77, 0.0, 0.707_106_77);
        let sixty_degrees = Vec3::new(0.866_025_4, 0.0, 0.5);
        let near_vertical = Vec3::new(0.999_847_7, 0.0, 0.017_452_406);

        assert!(hydraulic_erosion_direction(thirty_degrees).abs_diff_eq(thirty_degrees, 1.0e-6));
        assert!(
            hydraulic_erosion_direction(forty_five_degrees).abs_diff_eq(forty_five_degrees, 1.0e-6)
        );
        let blended_sixty = hydraulic_erosion_direction(sixty_degrees);
        assert!(blended_sixty.z > sixty_degrees.z);
        assert!(blended_sixty.x < sixty_degrees.x);
        assert!(hydraulic_erosion_direction(near_vertical).z > 0.999);
    }

    #[test]
    fn hydraulic_erosion_moves_down_normal_but_deposition_stays_vertical() {
        let normal = Vec3::new(0.6, 0.0, 0.8);
        let original = Vec3::new(0.4, 0.5, 0.2);
        let mut eroded = original;
        apply_hydraulic_transfer(
            &mut eroded,
            normal,
            HydraulicTransfer {
                normal_retreat: 0.05,
                ..HydraulicTransfer::default()
            },
        );
        assert!((eroded - (original - normal * 0.05)).length() < 1.0e-6);

        let mut deposited = original;
        apply_hydraulic_transfer(
            &mut deposited,
            normal,
            HydraulicTransfer {
                vertical_deposit: 0.05,
                ..HydraulicTransfer::default()
            },
        );
        assert!((deposited - (original + Vec3::Z * 0.05)).length() < 1.0e-6);
    }

    #[test]
    fn vertical_faces_do_not_supply_hydraulic_sediment() {
        let settings = settings();
        let mut sediment = 0.0;
        let eroded = exchange_sediment(
            &mut sediment,
            settings,
            exchange(1.0, 0.0, 0.0, shift_limits(0.5, 0.3), 0.0, 1.0),
        );

        assert!(eroded.normal_retreat.abs() < f32::EPSILON);
        assert!(sediment.abs() < f32::EPSILON);
    }

    #[test]
    fn hydraulic_pass_retreats_sloped_mesh_laterally() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.5),
                Vec3::new(0.0, 1.0, 0.5),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2],
            uv: Vec::new(),
        };
        let adjacency = mesh.adjacency();
        let original = mesh.vertices[0];
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        let bedrock_rates = vec![1.0; mesh.vertices.len()];

        hydraulic_erode(
            &mut mesh,
            &adjacency,
            &mut material,
            &bedrock_rates,
            true,
            settings(),
        );

        assert!(mesh.vertices[0].x < original.x);
        assert!(mesh.vertices[0].y < original.y);
        assert!(mesh.vertices[0].z < original.z);
    }

    #[test]
    fn hydraulic_cap_preserves_projected_triangle_orientation_and_area() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2],
            uv: Vec::new(),
        };
        let vertex_faces = VertexFaceAdjacency::new(&mesh);
        let mut projected_areas = ProjectedFaceAreas::new(&mesh);
        let normal = Vec3::new(-0.7, -0.1, 0.707_106_77).normalize();
        let requested = 2.0;
        let cap = projected_areas.safe_erosion_cap(&mesh, &vertex_faces, 0, normal, requested);
        let mut moved_mesh = mesh.clone();
        moved_mesh.vertices[0] -= normal * cap;
        projected_areas.update_incident(&moved_mesh, &vertex_faces, 0);
        let second_cap =
            projected_areas.safe_erosion_cap(&moved_mesh, &vertex_faces, 0, normal, requested);
        let before_area = (mesh.vertices[1] - mesh.vertices[0])
            .truncate()
            .perp_dot((mesh.vertices[2] - mesh.vertices[0]).truncate());
        let after_area = (moved_mesh.vertices[1] - moved_mesh.vertices[0])
            .truncate()
            .perp_dot((moved_mesh.vertices[2] - moved_mesh.vertices[0]).truncate());

        assert!(cap < requested);
        assert!(before_area * after_area > 0.0);
        assert!(after_area.abs() >= before_area.abs() * 0.2);
        assert!(second_cap < f32::EPSILON);
    }

    #[test]
    fn local_hydraulic_cap_is_edge_relative() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.5, 0.0, 0.5),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2],
            uv: Vec::new(),
        };
        let adjacency = mesh.adjacency();
        let shortest_edge = mesh.vertices[0].distance(mesh.vertices[1]);
        let cap = local_hydraulic_erosion_cap(&mesh, &adjacency, 0, 10.0);

        assert!((cap - shortest_edge * 0.08).abs() < 1.0e-6);
    }

    #[test]
    fn terrain_material_field_interpolates_at_export_positions() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2],
            uv: vec![Vec2::ZERO, Vec2::X, Vec2::Y],
        };
        mesh.calculate_normals();
        let terrain = Terrain::new(mesh);
        let field = TerrainMaterialField {
            values: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        };

        let sample = field.sample(&terrain, Vec2::new(0.25, 0.5));

        assert!(sample.abs_diff_eq(Vec3::new(0.25, 0.5, 0.0), 1.0e-6));
    }

    #[test]
    fn final_lod_correction_refines_and_pins_both_coarser_meshes() {
        let mut lod2 = Mesh::delaunay(&[Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]);
        let mut lod1 = lod2.tessellated();
        let mut lod0 = lod1.tessellated().tessellated();
        lod0.vertices.iter_mut().for_each(|vertex| {
            vertex.z = vertex.x.mul_add(vertex.x * 0.2, vertex.y * vertex.y * 0.1);
        });
        lod0.calculate_normals();
        let lod1_vertex_count = lod1.vertices.len();
        let lod2_vertex_count = lod2.vertices.len();
        let lod1_triangle_count = lod1.triangles.len();
        let lod2_triangle_count = lod2.triangles.len();
        let lod1_shared = lod0.vertices[..lod1_vertex_count].to_vec();
        let lod2_shared = lod0.vertices[..lod2_vertex_count].to_vec();

        correct_lods(&mut lod0, &mut lod1, &mut lod2);

        assert_eq!(lod1.triangles.len(), lod1_triangle_count * 4);
        assert_eq!(lod2.triangles.len(), lod2_triangle_count * 4);
        assert_eq!(lod1.vertices[..lod1_vertex_count], lod1_shared);
        assert_eq!(lod2.vertices[..lod2_vertex_count], lod2_shared);
        let index = TriangleIndex::new(&lod0);
        for mesh in [&lod1, &lod2] {
            assert!(mesh.vertices.iter().all(|vertex| {
                let elevation = sample_mesh_surface(&lod0, &index, vertex.x, vertex.y).0;
                (elevation - vertex.z).abs() < 1.0e-6
            }));
        }
    }
}
