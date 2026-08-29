//! Deterministic placement and island-wide batching of procedural trees.
//!
//! Forest membership is decided from the final LOD0 terrain mesh.  This
//! module deliberately does not know about Unity tiles or terrain slicing:
//! the combined meshes are authoritative and the owner-grid helpers copy a
//! complete tree's wood or a complete foliage cluster into one tile when a
//! native caller requests a batch.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    collections::{BTreeMap, HashMap},
    f32::consts::TAU,
};

use crate::{
    BoundingBox, ISLAND_WORLD_METRES, Mesh, Vec2, Vec3, Vec4,
    clustered_foliage::{ClusterFoliageMeshes, FoliageCrown, generate_cluster_foliage},
    noise,
    terrain::{LOOSE_DEPTH_EPSILON, Terrain},
    trees::{TreeHabit, TreeMeshes, decode_bark_axis, encode_bark_axis, generate_tree_with_habit},
};

const FOREST_FOLIAGE_DOMAIN: u64 = 0x4f46_4f4c_4941_4745;
const CANOPY_PATCH_RESOLUTION: usize = 64;
const DEFAULT_FOREST_NOISE_THRESHOLD: f32 = 0.62;

/// Physical and deterministic controls for forest generation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForestOptions {
    pub patch_size_metres: f32,
    pub noise_threshold: f32,
    pub noise_octaves: u8,
    pub snowline_metres: f32,
    pub prototype_count: u8,
    pub minimum_scale: f32,
    pub maximum_scale: f32,
}

impl Default for ForestOptions {
    fn default() -> Self {
        Self {
            patch_size_metres: 200.0,
            noise_threshold: DEFAULT_FOREST_NOISE_THRESHOLD,
            noise_octaves: 4,
            snowline_metres: 100.0,
            prototype_count: 8,
            minimum_scale: 1.0,
            maximum_scale: 2.0,
        }
    }
}

impl ForestOptions {
    /// Validates values before they can influence allocation or generation.
    ///
    /// # Errors
    ///
    /// Returns an error when a value is non-finite or outside its supported
    /// range.
    pub fn validate(self) -> Result<Self, String> {
        if !self.patch_size_metres.is_finite() || self.patch_size_metres <= 0.0 {
            return Err("forest patch_size_metres must be finite and greater than zero".into());
        }
        if !self.noise_threshold.is_finite() || !(0.0..=1.0).contains(&self.noise_threshold) {
            return Err("forest noise_threshold must be finite and between 0 and 1".into());
        }
        if self.noise_octaves == 0 || self.noise_octaves > 16 {
            return Err("forest noise_octaves must be between 1 and 16".into());
        }
        if !self.snowline_metres.is_finite() || self.snowline_metres <= 0.0 {
            return Err("forest snowline_metres must be finite and greater than zero".into());
        }
        if self.prototype_count == 0 || self.prototype_count > 64 {
            return Err("forest prototype_count must be between 1 and 64".into());
        }
        if !self.minimum_scale.is_finite()
            || !self.maximum_scale.is_finite()
            || self.minimum_scale <= 0.0
            || self.maximum_scale < self.minimum_scale
        {
            return Err(
                "forest scales must be finite, positive, and maximum_scale must be at least minimum_scale"
                    .into(),
            );
        }
        Ok(self)
    }
}

/// The dedicated seed domain for the coherent forest coverage field.
pub(crate) const FOREST_NOISE_DOMAIN: u64 = 0x8c2d_4e7a_51b3_9f06;
const FOREST_PLACEMENT_DOMAIN: u64 = 0x1f3d_5b79_a7c9_e2d4;
const FOREST_PROTOTYPE_DOMAIN: u64 = 0x6a09_e667_f3bc_c909;
const FOREST_YAW_DOMAIN: u64 = 0x243f_6a88_85a3_08d3;
const FOREST_ANCHOR_FAN_DOMAIN: u64 = 0xa409_3822_299f_31d0;
const FOREST_ANCHOR_OFFSET_DOMAIN: u64 = 0x082e_fa98_ec4e_6c89;
pub(crate) const TREE_CLEARANCE_PER_SCALE_METRES: f32 = 3.0;
const EXCLUSION_NUMERICAL_EPSILON_METRES: f32 = 0.001;
const FOREST_SCALE_FINE_OCTAVE_WEIGHT: f32 = 0.2;
const FOREST_HABIT_DOMAIN: u64 = 0x666f_7265_7374_6874;
const FOREST_HABIT_PATCH_METRES: f32 = 90.0;
const SHADER_SAND_PATCH_SIZE_METRES: f32 = 32.0;
const SHADER_PATCH_NOISE_LATTICE_PERIOD: i32 = 64;
const SHADER_PATCH_NOISE_RED_SEED: u32 = 0xb529_7a4d;
const SHADER_PATCH_NOISE_GREEN_SEED: u32 = 0x68e3_1da4;
pub(crate) const MINIMUM_NORMAL_Z: f32 = 0.927_183_87;

/// Borrowed final-terrain fields needed to classify a physical tree anchor.
#[derive(Clone, Copy)]
pub(crate) struct ForestSurface<'a> {
    pub(crate) river_bed: &'a [bool],
    /// Sorted final-LOD0 vertices supporting settled stones.
    pub(crate) stones: &'a [u32],
    pub(crate) deposited_depths: &'a [f32],
    pub(crate) sea_proximity: &'a [f32],
}

/// Which combined source stream should be returned by an owner grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForestMeshKind {
    Wood,
    Foliage,
}

/// One owner-grid tile and its optional shader sidecar.
///
/// Wood stores the normalized island-space root of each owning tree in RGB.
/// Foliage stores the nearest member-tree root in RGB and its height above
/// that root, in metres, in UV.x. Alpha is `0.5` so Unity can distinguish
/// either colour stream from its default white vertex colour.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ForestMeshTile {
    pub(crate) mesh: Mesh,
    pub(crate) material: Vec<Vec4>,
}

/// A contiguous source range belonging to one complete placed tree.
///
/// `triangle_count` counts entries in `Mesh::triangles` (and is therefore a
/// multiple of three), rather than counting geometric triangles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MeshRange {
    pub(crate) vertex_start: u32,
    pub(crate) vertex_count: u32,
    pub(crate) triangle_start: u32,
    pub(crate) triangle_count: u32,
}

impl MeshRange {
    fn vertex_end(self) -> Option<usize> {
        usize::try_from(self.vertex_start)
            .ok()?
            .checked_add(usize::try_from(self.vertex_count).ok()?)
    }

    fn triangle_end(self) -> Option<usize> {
        usize::try_from(self.triangle_start)
            .ok()?
            .checked_add(usize::try_from(self.triangle_count).ok()?)
    }
}

/// One accepted candidate and its stable appearance variation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TreePlacement {
    pub(crate) terrain_vertex: u32,
    pub(crate) anchor: Vec3,
    pub(crate) yaw_radians: f32,
    pub(crate) scale: f32,
    pub(crate) prototype: u8,
}

/// A bounded spatial canopy patch before foliage ranges are assembled.
#[derive(Clone, Debug, Default, PartialEq)]
struct ForestCluster {
    member_tree_indices: Vec<usize>,
    owner_anchor: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlacementCandidate {
    terrain_vertex: usize,
    anchor: Vec3,
    coverage: f32,
    scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SelectedFanFace {
    vertices: [usize; 3],
    centroid: Vec3,
}

/// One deterministically selected incident triangle per terrain vertex. This
/// keeps displacement and final-anchor material sampling in bounded flat
/// buffers regardless of vertex valence.
struct SelectedFanFaces {
    faces: Vec<Option<SelectedFanFace>>,
}

impl SelectedFanFaces {
    fn new(seed: u64, terrain: &Mesh) -> Self {
        let mut face_counts = vec![0_usize; terrain.vertices.len()];
        for triangle in terrain.triangles.chunks_exact(3) {
            let centroid = triangle_centroid(terrain, triangle);
            if centroid.is_none() {
                continue;
            }
            for &vertex in triangle {
                face_counts[vertex as usize] += 1;
            }
        }

        let selected_faces = face_counts
            .iter()
            .enumerate()
            .map(|(vertex, &count)| {
                (count != 0).then(|| {
                    let key = stable_key(seed, vertex as u64, FOREST_ANCHOR_FAN_DOMAIN);
                    (key % count as u64) as usize
                })
            })
            .collect::<Vec<_>>();
        let mut seen_faces = vec![0_usize; terrain.vertices.len()];
        let mut faces = vec![None; terrain.vertices.len()];
        for triangle in terrain.triangles.chunks_exact(3) {
            let Some(centroid) = triangle_centroid(terrain, triangle) else {
                continue;
            };
            let face = SelectedFanFace {
                vertices: [
                    triangle[0] as usize,
                    triangle[1] as usize,
                    triangle[2] as usize,
                ],
                centroid,
            };
            for &vertex in triangle {
                let vertex = vertex as usize;
                if selected_faces[vertex] == Some(seen_faces[vertex]) {
                    faces[vertex] = Some(face);
                }
                seen_faces[vertex] += 1;
            }
        }
        Self { faces }
    }

    fn get(&self, vertex: usize) -> Option<SelectedFanFace> {
        self.faces[vertex]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DisplacedAnchor {
    position: Vec3,
    fan_face: Option<SelectedFanFace>,
    centroid_fraction: f32,
}

#[derive(Clone, Copy)]
struct AcceptedTreeAnchor {
    position: Vec2,
    clearance: f32,
}

/// Spatial hash of final displaced anchors. A candidate is accepted only when
/// no previously accepted higher-priority anchor occupies either tree's
/// scale-aware clearance zone.
struct TreeExclusionZone {
    cells: HashMap<(i64, i64), Vec<AcceptedTreeAnchor>>,
    cell_size: f32,
}

impl TreeExclusionZone {
    fn new(maximum_scale: f32) -> Self {
        let cell_size = (TREE_CLEARANCE_PER_SCALE_METRES * maximum_scale
            + EXCLUSION_NUMERICAL_EPSILON_METRES)
            / ISLAND_WORLD_METRES;
        Self {
            cells: HashMap::new(),
            cell_size,
        }
    }

    fn accept(&mut self, position: Vec3, scale: f32) -> bool {
        let position = position.truncate();
        let clearance = (TREE_CLEARANCE_PER_SCALE_METRES * scale
            + EXCLUSION_NUMERICAL_EPSILON_METRES)
            / ISLAND_WORLD_METRES;
        let (cell_x, cell_y) = self.cell(position);
        for delta_y in -1_i64..=1 {
            for delta_x in -1_i64..=1 {
                if self
                    .cells
                    .get(&(cell_x + delta_x, cell_y + delta_y))
                    .is_some_and(|anchors| {
                        anchors.iter().any(|accepted| {
                            let pair_clearance = clearance.max(accepted.clearance);
                            accepted.position.distance_squared(position)
                                <= pair_clearance * pair_clearance
                        })
                    })
                {
                    return false;
                }
            }
        }
        self.cells
            .entry((cell_x, cell_y))
            .or_default()
            .push(AcceptedTreeAnchor {
                position,
                clearance,
            });
        true
    }

    fn cell(&self, anchor: Vec2) -> (i64, i64) {
        (
            (anchor.x / self.cell_size).floor() as i64,
            (anchor.y / self.cell_size).floor() as i64,
        )
    }
}

/// Ranges for the two wood streams belonging to a single tree.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ForestTreeRanges {
    pub(crate) terrain_vertex: u32,
    pub(crate) anchor: Vec3,
    pub(crate) prototype: u8,
    pub(crate) lod0_wood: MeshRange,
    pub(crate) lod1_wood: MeshRange,
}

/// One complete foliage owner unit.
///
/// A cluster owns one contiguous range in each combined foliage stream. Its
/// member indices refer to `ForestMeshes::trees` and remain in terrain-vertex
/// order, so grid extraction can copy a complete cluster exactly once.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ForestClusterRanges {
    pub(crate) owner_anchor: Vec3,
    pub(crate) member_tree_indices: Vec<usize>,
    pub(crate) lod0_foliage: MeshRange,
    /// The coarse control blob used by visual LOD1 and LOD2.
    pub(crate) lod1_foliage: MeshRange,
}

/// Authoritative combined forest streams with tree-wood and cluster-foliage
/// ownership metadata.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ForestMeshes {
    pub(crate) lod0_wood: Mesh,
    pub(crate) lod0_foliage: Mesh,
    pub(crate) lod1_wood: Mesh,
    pub(crate) lod1_foliage: Mesh,
    pub(crate) trees: Vec<ForestTreeRanges>,
    pub(crate) clusters: Vec<ForestClusterRanges>,
    pub(crate) placements: Vec<TreePlacement>,
}

impl ForestMeshes {
    #[must_use]
    pub(crate) fn mesh(&self, kind: ForestMeshKind, visual_lod: usize) -> Option<&Mesh> {
        match (kind, visual_lod) {
            (ForestMeshKind::Wood, 0) => Some(&self.lod0_wood),
            (ForestMeshKind::Wood, 1) => Some(&self.lod1_wood),
            (ForestMeshKind::Foliage, 0) => Some(&self.lod0_foliage),
            (ForestMeshKind::Foliage, 1 | 2) => Some(&self.lod1_foliage),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn placements(&self) -> &[TreePlacement] {
        &self.placements
    }

    /// Copies complete tree-wood or cluster-foliage ranges into owner tiles
    /// without geometric clipping.
    ///
    /// A wood LOD2 request returns an empty but correctly sized grid.  Bounds
    /// use normalized XY coordinates, matching all existing terrain grids.
    #[must_use]
    pub(crate) fn mesh_grid(
        &self,
        kind: ForestMeshKind,
        visual_lod: usize,
        bounds: BoundingBox,
        divisions: usize,
    ) -> Option<Vec<ForestMeshTile>> {
        if divisions == 0 || !valid_grid_bounds(bounds) {
            return None;
        }
        let tile_count = divisions.checked_mul(divisions)?;
        if kind == ForestMeshKind::Wood && visual_lod == 2 {
            return Some(vec![ForestMeshTile::default(); tile_count]);
        }
        let source = self.mesh(kind, visual_lod)?;
        let mut tiles = vec![ForestMeshTile::default(); tile_count];
        let span = Vec2::new(bounds.max.x - bounds.min.x, bounds.max.y - bounds.min.y);
        match kind {
            ForestMeshKind::Wood => {
                for tree in &self.trees {
                    if !bounds.contains_xy(tree.anchor.truncate()) {
                        continue;
                    }
                    let tile_x = owner_coordinate(tree.anchor.x, bounds.min.x, span.x, divisions);
                    let tile_y = owner_coordinate(tree.anchor.y, bounds.min.y, span.y, divisions);
                    let tile = tile_y * divisions + tile_x;
                    let range = tree_range(tree, visual_lod)?;
                    let vertex_start = tiles[tile].mesh.vertices.len();
                    append_range(&mut tiles[tile].mesh, source, range).ok()?;
                    let vertex_count = tiles[tile].mesh.vertices.len() - vertex_start;
                    tiles[tile]
                        .material
                        .extend(std::iter::repeat_n(tree.anchor.extend(0.5), vertex_count));
                }
            }
            ForestMeshKind::Foliage => {
                for cluster in &self.clusters {
                    if !bounds.contains_xy(cluster.owner_anchor.truncate()) {
                        continue;
                    }
                    let tile_x =
                        owner_coordinate(cluster.owner_anchor.x, bounds.min.x, span.x, divisions);
                    let tile_y =
                        owner_coordinate(cluster.owner_anchor.y, bounds.min.y, span.y, divisions);
                    let tile = tile_y * divisions + tile_x;
                    let range = cluster_range(cluster, visual_lod)?;
                    let source_vertex_start = usize::try_from(range.vertex_start).ok()?;
                    let source_vertices = source
                        .vertices
                        .get(source_vertex_start..range.vertex_end()?)?;
                    let output = &mut tiles[tile];
                    let destination_vertex_start = output.mesh.vertices.len();
                    append_range(&mut output.mesh, source, range).ok()?;
                    let destination_vertex_end = output.mesh.vertices.len();
                    if output.mesh.uv.len() == destination_vertex_start {
                        output.mesh.uv.resize(destination_vertex_end, Vec2::ZERO);
                    } else if output.mesh.uv.len() != destination_vertex_end {
                        return None;
                    }
                    for (offset, &vertex) in source_vertices.iter().enumerate() {
                        let tree_root = nearest_member_tree_anchor(
                            &self.trees,
                            &cluster.member_tree_indices,
                            vertex,
                        )?;
                        let height_metres =
                            ((vertex.z - tree_root.z) * ISLAND_WORLD_METRES).max(0.0);
                        output.material.push(tree_root.extend(0.5));
                        output.mesh.uv[destination_vertex_start + offset] =
                            Vec2::new(height_metres, 0.0);
                    }
                }
            }
        }
        Some(tiles)
    }
}

fn nearest_member_tree_anchor(
    trees: &[ForestTreeRanges],
    member_tree_indices: &[usize],
    vertex: Vec3,
) -> Option<Vec3> {
    let (&first_index, remaining_indices) = member_tree_indices.split_first()?;
    let first = trees.get(first_index)?.anchor;
    let point = vertex.truncate();
    remaining_indices.iter().try_fold(first, |nearest, &index| {
        let candidate = trees.get(index)?.anchor;
        Some(
            if candidate
                .truncate()
                .distance_squared(point)
                .total_cmp(&nearest.truncate().distance_squared(point))
                .is_lt()
            {
                candidate
            } else {
                nearest
            },
        )
    })
}

/// Marks the terrain triangle supporting each accepted tree. Tree anchors are
/// displaced toward one deterministic incident face, so reconstructing that
/// same face gives the exact forest-floor ownership without a radius guess.
pub(crate) fn forest_floor_mask(
    island_seed: u64,
    terrain: &Mesh,
    placements: &[TreePlacement],
) -> Vec<bool> {
    let selected_faces = SelectedFanFaces::new(island_seed, terrain);
    let mut forest_floor = vec![false; terrain.vertices.len()];
    for placement in placements {
        let terrain_vertex = placement.terrain_vertex as usize;
        if terrain_vertex >= forest_floor.len() {
            debug_assert!(
                false,
                "forest placement references an invalid terrain vertex"
            );
            continue;
        }
        let Some(face) = selected_faces.get(terrain_vertex) else {
            forest_floor[terrain_vertex] = true;
            continue;
        };
        for vertex in face.vertices {
            forest_floor[vertex] = true;
        }
    }
    forest_floor
}

/// Mutually exclusive diagnostics for final-LOD0 placement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ForestGenerationStats {
    pub(crate) total_lod0_vertices: usize,
    pub(crate) invalid: usize,
    pub(crate) sea: usize,
    pub(crate) snowline: usize,
    pub(crate) slope: usize,
    pub(crate) river_bed: usize,
    pub(crate) stones: usize,
    pub(crate) zero_soil: usize,
    pub(crate) beach: usize,
    pub(crate) below_or_equal_noise_threshold: usize,
    pub(crate) exclusion_zone: usize,
    pub(crate) accepted_trees: usize,
}

impl ForestGenerationStats {
    #[must_use]
    pub(crate) const fn rejected(&self) -> usize {
        self.invalid
            + self.sea
            + self.snowline
            + self.slope
            + self.river_bed
            + self.stones
            + self.zero_soil
            + self.beach
            + self.below_or_equal_noise_threshold
            + self.exclusion_zone
    }
}

/// Generates placements and the four combined streams from the final LOD0
/// terrain data.
pub(crate) fn generate_forest(
    island_seed: u64,
    terrain: &Terrain,
    surface: ForestSurface<'_>,
    options: ForestOptions,
) -> Result<(ForestMeshes, ForestGenerationStats), String> {
    let options = options.validate()?;
    let (placements, stats) = select_placements(island_seed, terrain.mesh(), surface, options)?;
    let prototypes = generate_prototypes(island_seed, options)?;
    let meshes = assemble_forest(island_seed, &placements, &prototypes, terrain)?;
    debug_assert_eq!(stats.accepted_trees, meshes.trees.len());
    Ok((meshes, stats))
}

/// Selects every eligible final LOD0 vertex exactly once, in vertex order.
pub(crate) fn select_placements(
    island_seed: u64,
    terrain: &Mesh,
    surface: ForestSurface<'_>,
    options: ForestOptions,
) -> Result<(Vec<TreePlacement>, ForestGenerationStats), String> {
    let options = options.validate()?;
    validate_placement_inputs(terrain, surface)?;
    let vertex_count = terrain.vertices.len();
    let mut stats = ForestGenerationStats {
        total_lod0_vertices: vertex_count,
        ..ForestGenerationStats::default()
    };
    let fan_faces = SelectedFanFaces::new(island_seed, terrain);
    let mut candidates = collect_candidates(
        island_seed,
        terrain,
        surface,
        options,
        &fan_faces,
        &mut stats,
    );
    prioritize_candidates(&mut candidates);
    let accepted =
        select_candidates_outside_exclusion_zone(candidates, options.maximum_scale, &mut stats);
    let placements = build_placements(island_seed, accepted, options)?;
    stats.accepted_trees = placements.len();
    debug_assert_eq!(
        stats.total_lod0_vertices,
        stats.rejected() + stats.accepted_trees
    );
    Ok((placements, stats))
}

fn validate_placement_inputs(terrain: &Mesh, surface: ForestSurface<'_>) -> Result<(), String> {
    let vertex_count = terrain.vertices.len();
    if terrain.normals.len() != vertex_count {
        return Err(format!(
            "forest final LOD0 vertices/normals length mismatch: {} != {}",
            vertex_count,
            terrain.normals.len()
        ));
    }
    if surface.river_bed.len() != vertex_count {
        return Err(format!(
            "forest final LOD0 vertices/river_bed length mismatch: {} != {}",
            vertex_count,
            surface.river_bed.len()
        ));
    }
    if surface
        .stones
        .iter()
        .any(|&vertex| vertex as usize >= vertex_count)
        || !surface.stones.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err("forest final LOD0 stones must be sorted, unique, and in range".into());
    }
    if surface.deposited_depths.len() != vertex_count {
        return Err(format!(
            "forest final LOD0 vertices/deposited-depth length mismatch: {} != {}",
            vertex_count,
            surface.deposited_depths.len()
        ));
    }
    if surface.sea_proximity.len() != vertex_count {
        return Err(format!(
            "forest final LOD0 vertices/sea-proximity length mismatch: {} != {}",
            vertex_count,
            surface.sea_proximity.len()
        ));
    }
    if !terrain.triangles.len().is_multiple_of(3) {
        return Err("forest final LOD0 triangle index count is not divisible by three".into());
    }
    if terrain.triangles.iter().any(|&index| {
        usize::try_from(index)
            .ok()
            .is_none_or(|index| index >= vertex_count)
    }) {
        return Err("forest final LOD0 contains an out-of-range triangle index".into());
    }
    Ok(())
}

fn collect_candidates(
    island_seed: u64,
    terrain: &Mesh,
    surface: ForestSurface<'_>,
    options: ForestOptions,
    fan_faces: &SelectedFanFaces,
    stats: &mut ForestGenerationStats,
) -> Vec<PlacementCandidate> {
    let mut candidates = Vec::new();
    for index in 0..terrain.vertices.len() {
        let vertex = terrain.vertices[index];
        let normal = terrain.normals[index];
        let depth = surface.deposited_depths[index];
        let sea_proximity = surface.sea_proximity[index];
        if !vertex.is_finite()
            || !normal.is_finite()
            || !depth.is_finite()
            || !sea_proximity.is_finite()
        {
            stats.invalid += 1;
            continue;
        }
        if vertex.z <= 0.0 {
            stats.sea += 1;
            continue;
        }
        if vertex.z * ISLAND_WORLD_METRES >= options.snowline_metres {
            stats.snowline += 1;
            continue;
        }
        if normal.z < MINIMUM_NORMAL_Z {
            stats.slope += 1;
            continue;
        }
        // This is intentionally the exact final LOD0 river marker.  No
        // centerline, radius, trunk-footprint, or geometry intersection test
        // belongs in this decision.
        if surface.river_bed[index] {
            stats.river_bed += 1;
            continue;
        }
        if surface.stones.binary_search(&(index as u32)).is_ok() {
            stats.stones += 1;
            continue;
        }
        if depth <= LOOSE_DEPTH_EPSILON {
            stats.zero_soil += 1;
            continue;
        }
        let displaced = displaced_tree_anchor(island_seed, index, vertex, fan_faces.get(index));
        let Some((loose_cover, sea_proximity)) = shader_material_at_displaced_anchor(
            index,
            displaced,
            surface.deposited_depths,
            surface.sea_proximity,
        ) else {
            stats.invalid += 1;
            continue;
        };
        if shader_beach_candidate(displaced.position, loose_cover, sea_proximity) {
            stats.beach += 1;
            continue;
        }
        let point_metres = vertex.truncate() * ISLAND_WORLD_METRES;
        let raw = noise::fractal(
            island_seed ^ FOREST_NOISE_DOMAIN,
            point_metres.x / options.patch_size_metres,
            point_metres.y / options.patch_size_metres,
            options.noise_octaves,
        );
        let coverage = raw.mul_add(0.5, 0.5).clamp(0.0, 1.0);
        if !coverage.is_finite() {
            stats.invalid += 1;
            continue;
        }
        if coverage <= options.noise_threshold {
            stats.below_or_equal_noise_threshold += 1;
            continue;
        }
        let scale = coherent_tree_scale(island_seed, point_metres, coverage, options);
        candidates.push(PlacementCandidate {
            terrain_vertex: index,
            anchor: displaced.position,
            coverage,
            scale,
        });
    }
    candidates
}

fn coherent_tree_scale(
    island_seed: u64,
    point_metres: Vec2,
    coverage: f32,
    options: ForestOptions,
) -> f32 {
    let fine_frequency = noise::FRACTAL_LACUNARITY.powi(i32::from(options.noise_octaves));
    let fine_raw = noise::value(
        (island_seed ^ FOREST_NOISE_DOMAIN).wrapping_add(u64::from(options.noise_octaves)),
        point_metres.x / options.patch_size_metres * fine_frequency,
        point_metres.y / options.patch_size_metres * fine_frequency,
    );
    let fine = fine_raw.mul_add(0.5, 0.5).clamp(0.0, 1.0);
    let coverage_above_threshold = ((coverage - options.noise_threshold)
        / (1.0 - options.noise_threshold).max(f32::EPSILON))
    .clamp(0.0, 1.0);
    let fine_variation = (fine - 0.5)
        * FOREST_SCALE_FINE_OCTAVE_WEIGHT
        * 4.0
        * coverage_above_threshold
        * (1.0 - coverage_above_threshold);
    let scale_signal = (coverage_above_threshold + fine_variation).clamp(0.0, 1.0);
    (options.maximum_scale - options.minimum_scale).mul_add(scale_signal, options.minimum_scale)
}

fn prioritize_candidates(candidates: &mut [PlacementCandidate]) {
    // Higher-coverage vertices own contested exclusion zones first, keeping
    // the largest cluster-centre trees in preference to fringe trees.
    candidates.sort_unstable_by(|left, right| {
        right
            .coverage
            .total_cmp(&left.coverage)
            .then_with(|| left.terrain_vertex.cmp(&right.terrain_vertex))
    });
}

fn select_candidates_outside_exclusion_zone(
    candidates: Vec<PlacementCandidate>,
    maximum_scale: f32,
    stats: &mut ForestGenerationStats,
) -> Vec<PlacementCandidate> {
    let mut exclusion_zone = TreeExclusionZone::new(maximum_scale);
    let mut accepted = Vec::new();
    for candidate in candidates {
        if !exclusion_zone.accept(candidate.anchor, candidate.scale) {
            stats.exclusion_zone += 1;
            continue;
        }
        accepted.push(candidate);
    }
    accepted.sort_unstable_by_key(|candidate| candidate.terrain_vertex);
    accepted
}

fn build_placements(
    island_seed: u64,
    accepted: Vec<PlacementCandidate>,
    options: ForestOptions,
) -> Result<Vec<TreePlacement>, String> {
    let mut placements = Vec::new();
    placements
        .try_reserve(accepted.len())
        .map_err(|error| format!("forest placement allocation failed: {error}"))?;
    for candidate in accepted {
        let index = candidate.terrain_vertex;
        let index_u64 = u64::try_from(index)
            .map_err(|_| "forest terrain vertex index does not fit in u64".to_owned())?;
        let placement_key = stable_key(island_seed, index_u64, FOREST_PLACEMENT_DOMAIN);
        let yaw_key = stable_key(placement_key, index_u64, FOREST_YAW_DOMAIN);
        let prototype_key = stable_key(placement_key, index_u64, FOREST_PROTOTYPE_DOMAIN);
        let prototype = coherent_prototype(
            island_seed,
            candidate.anchor,
            prototype_key,
            options.prototype_count,
        );
        placements.push(TreePlacement {
            terrain_vertex: u32::try_from(index)
                .map_err(|_| "forest terrain vertex index does not fit in u32".to_owned())?,
            anchor: candidate.anchor,
            yaw_radians: stable_unit(yaw_key) * TAU,
            scale: candidate.scale,
            prototype,
        });
    }
    Ok(placements)
}

fn coherent_prototype(
    island_seed: u64,
    anchor: Vec3,
    variation_key: u64,
    prototype_count: u8,
) -> u8 {
    if prototype_count < 3 {
        return u8::try_from(variation_key % u64::from(prototype_count))
            .expect("prototype modulus fits u8");
    }
    let point_metres = anchor.truncate() * ISLAND_WORLD_METRES;
    let habit_signal = noise::fractal(
        island_seed ^ FOREST_HABIT_DOMAIN,
        point_metres.x / FOREST_HABIT_PATCH_METRES,
        point_metres.y / FOREST_HABIT_PATCH_METRES,
        2,
    )
    .mul_add(0.5, 0.5)
    .clamp(0.0, 1.0 - f32::EPSILON);
    let habit = (habit_signal * 3.0).floor() as u8;
    let variants = (prototype_count - 1 - habit) / 3 + 1;
    habit
        + 3 * u8::try_from(variation_key % u64::from(variants))
            .expect("prototype variant modulus fits u8")
}

/// Groups all placed trees in each fine streaming cell into one canopy input.
///
/// The foliage builder's alpha filtering remains responsible for separating
/// genuinely disconnected support footprints. Keeping the ownership patch
/// aligned with the 64 x 64 fine stream prevents a connected forest from
/// becoming one island-wide LOD range while eliminating the per-tree shells
/// and their overlapping internal surfaces within each patch.
fn build_clusters(placements: &[TreePlacement]) -> Vec<ForestCluster> {
    let mut patches = BTreeMap::<(usize, usize), Vec<usize>>::new();
    for (placement_index, placement) in placements.iter().enumerate() {
        patches
            .entry(canopy_patch(placement.anchor))
            .or_default()
            .push(placement_index);
    }

    patches
        .into_values()
        .map(|mut member_tree_indices| {
            member_tree_indices.sort_unstable_by(|&left, &right| {
                placements[left]
                    .terrain_vertex
                    .cmp(&placements[right].terrain_vertex)
                    .then_with(|| left.cmp(&right))
            });
            let sum = member_tree_indices
                .iter()
                .map(|&member_index| placements[member_index].anchor)
                .fold(Vec3::ZERO, |sum, anchor| sum + anchor);
            let count = member_tree_indices.len() as f32;
            ForestCluster {
                member_tree_indices,
                owner_anchor: sum / count,
            }
        })
        .collect()
}

fn canopy_patch(anchor: Vec3) -> (usize, usize) {
    let coordinate = |value: f32| {
        ((value.clamp(0.0, 1.0) * CANOPY_PATCH_RESOLUTION as f32).floor() as usize)
            .min(CANOPY_PATCH_RESOLUTION - 1)
    };
    (coordinate(anchor.x), coordinate(anchor.y))
}

fn triangle_centroid(terrain: &Mesh, triangle: &[u32]) -> Option<Vec3> {
    let centroid = triangle
        .iter()
        .map(|&vertex| terrain.vertices[vertex as usize])
        .fold(Vec3::ZERO, |sum, vertex| sum + vertex)
        / 3.0;
    centroid.is_finite().then_some(centroid)
}

fn shader_material_at_displaced_anchor(
    terrain_vertex: usize,
    displaced: DisplacedAnchor,
    deposited_depths: &[f32],
    sea_proximity: &[f32],
) -> Option<(f32, f32)> {
    let source_cover = shader_loose_cover(deposited_depths[terrain_vertex]);
    let source_sea_proximity = sea_proximity[terrain_vertex];
    let Some(face) = displaced.fan_face else {
        return (source_cover.is_finite() && source_sea_proximity.is_finite())
            .then_some((source_cover, source_sea_proximity));
    };
    let centroid_cover = face
        .vertices
        .iter()
        .map(|&vertex| shader_loose_cover(deposited_depths[vertex]))
        .sum::<f32>()
        / 3.0;
    let centroid_sea_proximity = face
        .vertices
        .iter()
        .map(|&vertex| sea_proximity[vertex])
        .sum::<f32>()
        / 3.0;
    let cover = (centroid_cover - source_cover).mul_add(displaced.centroid_fraction, source_cover);
    let sea_proximity = (centroid_sea_proximity - source_sea_proximity)
        .mul_add(displaced.centroid_fraction, source_sea_proximity);
    (cover.is_finite() && sea_proximity.is_finite()).then_some((cover, sea_proximity))
}

fn shader_loose_cover(deposited_depth: f32) -> f32 {
    let cover = (deposited_depth / 0.002).clamp(0.0, 1.0);
    cover * cover * (3.0 - 2.0 * cover)
}

/// Mirrors the terrain shader's generated-texture beach candidate at its
/// 50-percent antialias boundary. Exposed-rock overlays are intentionally not
/// part of this predicate: a sand-capable coastal deposit remains unsuitable
/// for a tree even where the visual rock layer partially covers it.
fn shader_beach_candidate(anchor: Vec3, loose_cover: f32, sea_proximity: f32) -> bool {
    let elevation_metres = anchor.z * ISLAND_WORLD_METRES;
    let altitude_weight = 1.0 - smoothstep(2.0, 4.0, elevation_metres);
    let sand_richness =
        loose_cover.clamp(0.0, 1.0) * sea_proximity.clamp(0.0, 1.0) * altitude_weight;
    if sand_richness < 1.0e-4 {
        return false;
    }
    let centered_point_metres = (anchor.truncate() - Vec2::splat(0.5)) * ISLAND_WORLD_METRES;
    let patch_uv = centered_point_metres / SHADER_SAND_PATCH_SIZE_METRES + Vec2::new(0.37, 0.73);
    let lattice_position = patch_uv * SHADER_PATCH_NOISE_LATTICE_PERIOD as f32;
    let red = shader_periodic_noise_2d(lattice_position, SHADER_PATCH_NOISE_RED_SEED);
    let green = shader_periodic_noise_2d(lattice_position, SHADER_PATCH_NOISE_GREEN_SEED);
    let sand_patch_noise = red * 0.40 + green * 0.60;
    sand_patch_noise >= 1.0 - sand_richness
}

fn shader_periodic_noise_2d(position: Vec2, seed: u32) -> f32 {
    let lattice_x = position.x.floor() as i32;
    let lattice_y = position.y.floor() as i32;
    let x0 = lattice_x.rem_euclid(SHADER_PATCH_NOISE_LATTICE_PERIOD);
    let y0 = lattice_y.rem_euclid(SHADER_PATCH_NOISE_LATTICE_PERIOD);
    let x1 = (x0 + 1) % SHADER_PATCH_NOISE_LATTICE_PERIOD;
    let y1 = (y0 + 1) % SHADER_PATCH_NOISE_LATTICE_PERIOD;
    let fade_x = quintic_fade(position.x - lattice_x as f32);
    let fade_y = quintic_fade(position.y - lattice_y as f32);
    let near = lerp(
        shader_lattice_noise(x0, y0, seed),
        shader_lattice_noise(x1, y0, seed),
        fade_x,
    );
    let far = lerp(
        shader_lattice_noise(x0, y1, seed),
        shader_lattice_noise(x1, y1, seed),
        fade_x,
    );
    lerp(near, far, fade_y)
}

fn shader_lattice_noise(x: i32, y: i32, seed: u32) -> f32 {
    let value = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841);
    let mut value = value ^ seed;
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    (value & 0x00ff_ffff) as f32 / 16_777_215.0
}

fn quintic_fade(value: f32) -> f32 {
    value * value * value * value.mul_add(value.mul_add(6.0, -15.0), 10.0)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let normalized = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    normalized * normalized * (3.0 - 2.0 * normalized)
}

fn lerp(from: f32, to: f32, fraction: f32) -> f32 {
    (to - from).mul_add(fraction, from)
}

fn displaced_tree_anchor(
    island_seed: u64,
    terrain_vertex: usize,
    vertex: Vec3,
    fan_face: Option<SelectedFanFace>,
) -> DisplacedAnchor {
    let Some(fan_face) = fan_face else {
        return DisplacedAnchor {
            position: vertex,
            fan_face: None,
            centroid_fraction: 0.0,
        };
    };
    let terrain_vertex = terrain_vertex as u64;
    let placement_key = stable_key(island_seed, terrain_vertex, FOREST_PLACEMENT_DOMAIN);
    let offset_key = stable_key(placement_key, terrain_vertex, FOREST_ANCHOR_OFFSET_DOMAIN);
    let centroid_fraction = stable_unit(offset_key) * 0.5;
    DisplacedAnchor {
        position: vertex + (fan_face.centroid - vertex) * centroid_fraction,
        fan_face: Some(fan_face),
        centroid_fraction,
    }
}

fn generate_prototypes(seed: u64, options: ForestOptions) -> Result<Vec<TreeMeshes>, String> {
    let mut prototypes = Vec::new();
    prototypes
        .try_reserve(usize::from(options.prototype_count))
        .map_err(|error| format!("forest prototype allocation failed: {error}"))?;
    for prototype in 0..options.prototype_count {
        let prototype_seed = stable_key(
            seed ^ FOREST_PROTOTYPE_DOMAIN,
            u64::from(prototype),
            FOREST_PROTOTYPE_DOMAIN,
        );
        prototypes.push(generate_tree_with_habit(
            prototype_seed,
            TreeHabit::from_index(prototype),
        ));
    }
    Ok(prototypes)
}

fn assemble_forest(
    island_seed: u64,
    placements: &[TreePlacement],
    prototypes: &[TreeMeshes],
    terrain: &Terrain,
) -> Result<ForestMeshes, String> {
    let clusters = build_clusters(placements);
    let foliage_meshes =
        generate_cluster_foliage_meshes(island_seed, placements, prototypes, &clusters)?;
    let capacities = combined_capacities(placements, prototypes, &foliage_meshes)?;
    let mut out = ForestMeshes {
        placements: placements.to_vec(),
        ..ForestMeshes::default()
    };
    reserve_combined_streams(&mut out, capacities)?;
    out.trees
        .try_reserve(placements.len())
        .map_err(|error| format!("forest tree-range allocation failed: {error}"))?;
    out.clusters
        .try_reserve(clusters.len())
        .map_err(|error| format!("forest cluster-range allocation failed: {error}"))?;
    append_tree_ranges(&mut out, placements, prototypes, terrain)?;
    append_cluster_ranges(&mut out, &clusters, foliage_meshes)?;
    Ok(out)
}

fn generate_cluster_foliage_meshes(
    island_seed: u64,
    placements: &[TreePlacement],
    prototypes: &[TreeMeshes],
    clusters: &[ForestCluster],
) -> Result<Vec<ClusterFoliageMeshes>, String> {
    let mut foliage_meshes = Vec::new();
    foliage_meshes
        .try_reserve(clusters.len())
        .map_err(|error| format!("forest cluster foliage allocation failed: {error}"))?;
    for (cluster_index, cluster) in clusters.iter().enumerate() {
        let mut support_storage = Vec::new();
        support_storage
            .try_reserve(cluster.member_tree_indices.len())
            .map_err(|error| format!("forest foliage support allocation failed: {error}"))?;
        for &tree_index in &cluster.member_tree_indices {
            let placement = placements[tree_index];
            let prototype = prototypes
                .get(usize::from(placement.prototype))
                .ok_or_else(|| {
                    format!("forest prototype {} is unavailable", placement.prototype)
                })?;
            support_storage.push(
                prototype
                    .foliage_supports
                    .iter()
                    .copied()
                    .map(|support| transform_position_for_placement(support, placement))
                    .collect::<Vec<_>>(),
            );
        }
        let mut crowns = Vec::new();
        crowns
            .try_reserve(cluster.member_tree_indices.len())
            .map_err(|error| format!("forest foliage crown allocation failed: {error}"))?;
        for (&tree_index, tips) in cluster.member_tree_indices.iter().zip(&support_storage) {
            crowns.push(FoliageCrown {
                trunk: placements[tree_index].anchor,
                tips,
                scale: placements[tree_index].scale,
            });
        }
        let cluster_seed = stable_key(
            island_seed,
            u64::from(
                cluster
                    .member_tree_indices
                    .first()
                    .map_or(0, |&index| placements[index].terrain_vertex),
            ),
            FOREST_FOLIAGE_DOMAIN,
        );
        let foliage = match generate_cluster_foliage(cluster_seed, &crowns) {
            Ok(foliage) => foliage,
            Err(error) if is_empty_cluster_foliage_error(&error) => ClusterFoliageMeshes::default(),
            Err(error) => {
                return Err(format!(
                    "forest cluster foliage generation failed for cluster {cluster_index} with {} crowns: {error}",
                    crowns.len()
                ));
            }
        };
        foliage_meshes.push(foliage);
    }
    Ok(foliage_meshes)
}

fn combined_capacities(
    placements: &[TreePlacement],
    prototypes: &[TreeMeshes],
    foliage_meshes: &[ClusterFoliageMeshes],
) -> Result<[usize; 4], String> {
    let mut capacities = [0_usize; 4];
    for placement in placements {
        let prototype = prototypes
            .get(usize::from(placement.prototype))
            .ok_or_else(|| format!("forest prototype {} is unavailable", placement.prototype))?;
        capacities[0] = capacities[0]
            .checked_add(prototype.lod0_wood.vertices.len())
            .ok_or_else(|| "forest combined vertex capacity overflow".to_owned())?;
        capacities[2] = capacities[2]
            .checked_add(prototype.lod1_wood.vertices.len())
            .ok_or_else(|| "forest combined vertex capacity overflow".to_owned())?;
    }
    for foliage in foliage_meshes {
        capacities[1] = capacities[1]
            .checked_add(foliage.lod0.vertices.len())
            .ok_or_else(|| "forest combined vertex capacity overflow".to_owned())?;
        capacities[3] = capacities[3]
            .checked_add(foliage.lod1.vertices.len())
            .ok_or_else(|| "forest combined vertex capacity overflow".to_owned())?;
    }
    Ok(capacities)
}

fn reserve_combined_streams(out: &mut ForestMeshes, capacities: [usize; 4]) -> Result<(), String> {
    out.lod0_wood
        .vertices
        .try_reserve(capacities[0])
        .map_err(|error| format!("forest LOD0 wood allocation failed: {error}"))?;
    out.lod0_foliage
        .vertices
        .try_reserve(capacities[1])
        .map_err(|error| format!("forest LOD0 foliage allocation failed: {error}"))?;
    out.lod1_wood
        .vertices
        .try_reserve(capacities[2])
        .map_err(|error| format!("forest LOD1 wood allocation failed: {error}"))?;
    out.lod1_foliage
        .vertices
        .try_reserve(capacities[3])
        .map_err(|error| format!("forest LOD1 foliage allocation failed: {error}"))?;
    Ok(())
}

fn append_tree_ranges(
    out: &mut ForestMeshes,
    placements: &[TreePlacement],
    prototypes: &[TreeMeshes],
    terrain: &Terrain,
) -> Result<(), String> {
    for placement in placements {
        let prototype = prototypes
            .get(usize::from(placement.prototype))
            .ok_or_else(|| format!("forest prototype {} is unavailable", placement.prototype))?;
        let lod0_wood = append_transformed_to_terrain(
            &mut out.lod0_wood,
            &prototype.lod0_wood,
            *placement,
            terrain,
        )?;
        let lod1_wood = append_transformed_to_terrain(
            &mut out.lod1_wood,
            &prototype.lod1_wood,
            *placement,
            terrain,
        )?;
        out.trees.push(ForestTreeRanges {
            terrain_vertex: placement.terrain_vertex,
            anchor: placement.anchor,
            prototype: placement.prototype,
            lod0_wood,
            lod1_wood,
        });
    }
    out.lod0_wood.calculate_normals();
    out.lod1_wood.calculate_normals();
    Ok(())
}

fn append_cluster_ranges(
    out: &mut ForestMeshes,
    clusters: &[ForestCluster],
    foliage_meshes: Vec<ClusterFoliageMeshes>,
) -> Result<(), String> {
    for (cluster, foliage) in clusters.iter().zip(foliage_meshes) {
        let lod0_foliage = append_identity(&mut out.lod0_foliage, &foliage.lod0)?;
        let lod1_foliage = append_identity(&mut out.lod1_foliage, &foliage.lod1)?;
        out.clusters.push(ForestClusterRanges {
            owner_anchor: cluster.owner_anchor,
            member_tree_indices: cluster.member_tree_indices.clone(),
            lod0_foliage,
            lod1_foliage,
        });
    }
    Ok(())
}

fn is_empty_cluster_foliage_error(error: &str) -> bool {
    error.contains("not enough support samples")
        || error.contains("no valid triangles")
        || error.contains("no surface")
}

struct MeshAppender<'a> {
    destination: &'a mut Mesh,
    anchor: Vec3,
    yaw_radians: f32,
    scale: f32,
    transform_uv_as_axis: bool,
    terrain: Option<&'a Terrain>,
}

impl MeshAppender<'_> {
    fn append(&mut self, source: &Mesh) -> Result<MeshRange, String> {
        if source.normals.len() != source.vertices.len() {
            return Err("forest prototype has mismatched vertices and normals".into());
        }
        if !source.uv.is_empty() && source.uv.len() != source.vertices.len() {
            return Err("forest prototype has mismatched vertices and UVs".into());
        }
        if source.triangles.iter().any(|&index| {
            usize::try_from(index)
                .ok()
                .is_none_or(|index| index >= source.vertices.len())
        }) {
            return Err("forest prototype contains an out-of-range triangle".into());
        }
        let vertex_start = self.destination.vertices.len();
        let triangle_start = self.destination.triangles.len();
        let vertex_end = vertex_start
            .checked_add(source.vertices.len())
            .ok_or_else(|| "forest combined vertex count overflow".to_owned())?;
        if vertex_end > usize::try_from(u32::MAX).unwrap_or(usize::MAX) {
            return Err("forest combined mesh exceeds u32 vertex indices".into());
        }
        let triangle_end = triangle_start
            .checked_add(source.triangles.len())
            .ok_or_else(|| "forest combined triangle count overflow".to_owned())?;
        self.destination
            .vertices
            .try_reserve(source.vertices.len())
            .map_err(|error| format!("forest vertex allocation failed: {error}"))?;
        self.destination
            .normals
            .try_reserve(source.normals.len())
            .map_err(|error| format!("forest normal allocation failed: {error}"))?;
        self.destination
            .triangles
            .try_reserve(source.triangles.len())
            .map_err(|error| format!("forest triangle allocation failed: {error}"))?;
        if !source.uv.is_empty() {
            if !self.destination.uv.is_empty() && self.destination.uv.len() != vertex_start {
                return Err("forest prototype UV streams are inconsistent".into());
            }
            self.destination
                .uv
                .try_reserve(source.uv.len())
                .map_err(|error| format!("forest UV allocation failed: {error}"))?;
        }
        let (sin, cos) = self.yaw_radians.sin_cos();
        for (&position, &normal) in source.vertices.iter().zip(&source.normals) {
            let mut transformed = transform_position(position, self.anchor, self.scale, sin, cos);
            if let Some(terrain) = self.terrain
                && position.z.abs() <= f32::EPSILON
            {
                transformed.z = terrain.sample(transformed.x, transformed.y);
            }
            self.destination.vertices.push(transformed);
            self.destination
                .normals
                .push(transform_normal(normal, sin, cos));
        }
        if !source.uv.is_empty() {
            if self.transform_uv_as_axis {
                self.destination.uv.extend(
                    source.uv.iter().map(|&uv| {
                        encode_bark_axis(transform_normal(decode_bark_axis(uv), sin, cos))
                    }),
                );
            } else {
                self.destination.uv.extend_from_slice(&source.uv);
            }
        }
        let vertex_start_u32 = u32::try_from(vertex_start)
            .map_err(|_| "forest vertex start does not fit in u32".to_owned())?;
        self.destination.triangles.extend(
            source
                .triangles
                .iter()
                .map(|&index| {
                    index
                        .checked_add(vertex_start_u32)
                        .ok_or_else(|| "forest triangle index overflow".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        Ok(MeshRange {
            vertex_start: vertex_start_u32,
            vertex_count: u32::try_from(source.vertices.len())
                .map_err(|_| "forest source vertex count does not fit in u32".to_owned())?,
            triangle_start: u32::try_from(triangle_start)
                .map_err(|_| "forest triangle start does not fit in u32".to_owned())?,
            triangle_count: u32::try_from(triangle_end - triangle_start)
                .map_err(|_| "forest source triangle count does not fit in u32".to_owned())?,
        })
    }
}

#[cfg(test)]
fn append_transformed(
    destination: &mut Mesh,
    source: &Mesh,
    placement: TreePlacement,
) -> Result<MeshRange, String> {
    MeshAppender {
        destination,
        anchor: placement.anchor,
        yaw_radians: placement.yaw_radians,
        scale: placement.scale,
        transform_uv_as_axis: true,
        terrain: None,
    }
    .append(source)
}

fn append_transformed_to_terrain(
    destination: &mut Mesh,
    source: &Mesh,
    placement: TreePlacement,
    terrain: &Terrain,
) -> Result<MeshRange, String> {
    MeshAppender {
        destination,
        anchor: placement.anchor,
        yaw_radians: placement.yaw_radians,
        scale: placement.scale,
        transform_uv_as_axis: true,
        terrain: Some(terrain),
    }
    .append(source)
}

fn append_identity(destination: &mut Mesh, source: &Mesh) -> Result<MeshRange, String> {
    MeshAppender {
        destination,
        anchor: Vec3::ZERO,
        yaw_radians: 0.0,
        scale: 1.0,
        transform_uv_as_axis: false,
        terrain: None,
    }
    .append(source)
}

fn transform_position_for_placement(position: Vec3, placement: TreePlacement) -> Vec3 {
    let (sin, cos) = placement.yaw_radians.sin_cos();
    transform_position(position, placement.anchor, placement.scale, sin, cos)
}

fn transform_position(position: Vec3, anchor: Vec3, scale: f32, sin: f32, cos: f32) -> Vec3 {
    let local = position * scale;
    anchor
        + Vec3::new(
            local.x.mul_add(cos, -(local.y * sin)),
            local.x.mul_add(sin, local.y * cos),
            local.z,
        )
}

fn transform_normal(normal: Vec3, sin: f32, cos: f32) -> Vec3 {
    Vec3::new(
        normal.x.mul_add(cos, -(normal.y * sin)),
        normal.x.mul_add(sin, normal.y * cos),
        normal.z,
    )
}

fn append_range(destination: &mut Mesh, source: &Mesh, range: MeshRange) -> Result<(), String> {
    let source_vertex_start = range
        .vertex_start
        .try_into()
        .map_err(|_| "forest tile vertex range does not fit usize".to_owned())?;
    let source_vertex_end = range
        .vertex_end()
        .ok_or_else(|| "forest tile vertex range overflow".to_owned())?;
    let source_triangle_start = range
        .triangle_start
        .try_into()
        .map_err(|_| "forest tile triangle range does not fit usize".to_owned())?;
    let source_triangle_end = range
        .triangle_end()
        .ok_or_else(|| "forest tile triangle range overflow".to_owned())?;
    if source_vertex_end > source.vertices.len()
        || source_triangle_end > source.triangles.len()
        || source_vertex_start > source_vertex_end
        || source_triangle_start > source_triangle_end
    {
        return Err("forest tile range is outside its source mesh".into());
    }
    let vertex_offset = u32::try_from(destination.vertices.len())
        .map_err(|_| "forest tile exceeds u32 vertex indices".to_owned())?;
    destination
        .vertices
        .extend_from_slice(&source.vertices[source_vertex_start..source_vertex_end]);
    if source.normals.len() == source.vertices.len() {
        destination
            .normals
            .extend_from_slice(&source.normals[source_vertex_start..source_vertex_end]);
    } else {
        return Err("forest source mesh has mismatched normals".into());
    }
    if !source.uv.is_empty() {
        if source.uv.len() != source.vertices.len() {
            return Err("forest source mesh has mismatched UVs".into());
        }
        destination
            .uv
            .extend_from_slice(&source.uv[source_vertex_start..source_vertex_end]);
    }
    destination.triangles.extend(
        source.triangles[source_triangle_start..source_triangle_end]
            .iter()
            .map(|&index| {
                index
                    .checked_sub(range.vertex_start)
                    .and_then(|index| index.checked_add(vertex_offset))
                    .ok_or_else(|| "forest tile triangle index is outside its range".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    Ok(())
}

fn tree_range(tree: &ForestTreeRanges, visual_lod: usize) -> Option<MeshRange> {
    match visual_lod {
        0 => Some(tree.lod0_wood),
        1 => Some(tree.lod1_wood),
        _ => None,
    }
}

fn cluster_range(cluster: &ForestClusterRanges, visual_lod: usize) -> Option<MeshRange> {
    match visual_lod {
        0 => Some(cluster.lod0_foliage),
        1 | 2 => Some(cluster.lod1_foliage),
        _ => None,
    }
}

fn stable_key(seed: u64, index: u64, domain: u64) -> u64 {
    splitmix64(seed ^ domain ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

fn stable_unit(key: u64) -> f32 {
    (key >> 40) as f32 / 16_777_216.0
}

fn splitmix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn valid_grid_bounds(bounds: BoundingBox) -> bool {
    bounds.min.x.is_finite()
        && bounds.min.y.is_finite()
        && bounds.max.x.is_finite()
        && bounds.max.y.is_finite()
        && bounds.max.x > bounds.min.x
        && bounds.max.y > bounds.min.y
}

fn owner_coordinate(value: f32, minimum: f32, span: f32, divisions: usize) -> usize {
    (((value - minimum) / span * divisions as f32).floor() as usize).min(divisions - 1)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::terrain::{Island, IslandOptions, Terrain};

    use super::*;

    fn flat_mesh(vertices: &[Vec3]) -> Mesh {
        Mesh {
            vertices: vertices.to_vec(),
            normals: vec![Vec3::Z; vertices.len()],
            ..Mesh::default()
        }
    }

    fn test_surface<'a>(
        river_bed: &'a [bool],
        deposited_depths: &'a [f32],
        sea_proximity: &'a [f32],
    ) -> ForestSurface<'a> {
        ForestSurface {
            river_bed,
            stones: &[],
            deposited_depths,
            sea_proximity,
        }
    }

    #[test]
    fn options_reject_non_finite_and_out_of_range_values() {
        assert!(
            ForestOptions {
                patch_size_metres: f32::NAN,
                ..ForestOptions::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            ForestOptions {
                noise_threshold: 1.1,
                ..ForestOptions::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            ForestOptions {
                prototype_count: 0,
                ..ForestOptions::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn forest_floor_marks_the_selected_support_triangle() {
        let terrain = Mesh {
            vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let placement = TreePlacement {
            terrain_vertex: 0,
            anchor: Vec3::new(0.1, 0.1, 0.0),
            yaw_radians: 0.0,
            scale: 1.0,
            prototype: 0,
        };

        assert_eq!(
            forest_floor_mask(2018, &terrain, &[placement]),
            vec![true, true, true]
        );
    }

    #[test]
    fn placement_uses_exact_masks_and_exclusion_precedence() {
        let vertices = [
            Vec3::new(0.1, 0.1, 0.0),
            Vec3::new(0.2, 0.1, 0.02),
            Vec3::new(0.3, 0.1, 0.02),
            Vec3::new(0.4, 0.1, 0.02),
            Vec3::new(0.5, 0.1, 0.02),
        ];
        let terrain = flat_mesh(&vertices);
        let (placements, stats) = select_placements(
            2018,
            &terrain,
            test_surface(&[false, true, false, false, false], &[1.0; 5], &[0.0; 5]),
            ForestOptions {
                noise_threshold: 0.0,
                snowline_metres: 100.0,
                ..ForestOptions::default()
            },
        )
        .unwrap();
        assert_eq!(stats.total_lod0_vertices, 5);
        assert_eq!(stats.sea, 1);
        assert_eq!(stats.river_bed, 1);
        assert!(placements.iter().all(|placement| placement.anchor.z > 0.0));
        assert!(
            placements
                .windows(2)
                .all(|pair| pair[0].terrain_vertex < pair[1].terrain_vertex)
        );
        assert_eq!(stats.rejected() + stats.accepted_trees, 5);
    }

    fn eligible_test_options() -> ForestOptions {
        ForestOptions {
            noise_threshold: 0.0,
            snowline_metres: 100.0,
            ..ForestOptions::default()
        }
    }

    fn test_coverage(vertex: Vec3, seed: u64, options: ForestOptions) -> f32 {
        let point_metres = vertex.truncate() * ISLAND_WORLD_METRES;
        noise::fractal(
            seed ^ FOREST_NOISE_DOMAIN,
            point_metres.x / options.patch_size_metres,
            point_metres.y / options.patch_size_metres,
            options.noise_octaves,
        )
        .mul_add(0.5, 0.5)
        .clamp(0.0, 1.0)
    }

    #[test]
    fn coherent_scale_combines_forest_coverage_with_the_next_finer_octave() {
        let seed = 2018;
        let options = ForestOptions::default();
        let point_metres = Vec2::new(347.0, 829.0);
        let coverage = 0.7_f32;
        let fine_frequency = noise::FRACTAL_LACUNARITY.powi(i32::from(options.noise_octaves));
        let fine = noise::value(
            (seed ^ FOREST_NOISE_DOMAIN).wrapping_add(u64::from(options.noise_octaves)),
            point_metres.x / options.patch_size_metres * fine_frequency,
            point_metres.y / options.patch_size_metres * fine_frequency,
        )
        .mul_add(0.5, 0.5)
        .clamp(0.0, 1.0);
        let coverage_above_threshold =
            (coverage - options.noise_threshold) / (1.0 - options.noise_threshold);
        let fine_variation = (fine - 0.5)
            * FOREST_SCALE_FINE_OCTAVE_WEIGHT
            * 4.0
            * coverage_above_threshold
            * (1.0 - coverage_above_threshold);
        let expected_signal = coverage_above_threshold + fine_variation;
        let expected = (options.maximum_scale - options.minimum_scale)
            .mul_add(expected_signal, options.minimum_scale);
        let actual = coherent_tree_scale(seed, point_metres, coverage, options);

        assert_eq!(actual.to_bits(), expected.to_bits());
        assert!((options.minimum_scale..=options.maximum_scale).contains(&actual));
        assert!(coherent_tree_scale(seed, point_metres, 0.9, options) > actual);
        assert_eq!(
            coherent_tree_scale(seed, point_metres, options.noise_threshold, options).to_bits(),
            options.minimum_scale.to_bits()
        );
        assert_eq!(
            coherent_tree_scale(seed, point_metres, 1.0, options).to_bits(),
            options.maximum_scale.to_bits()
        );
    }

    #[test]
    fn coherent_habit_patches_keep_local_variants_in_one_growth_family() {
        let anchor = Vec3::new(0.37, 0.41, 0.02);
        let prototypes = (0..32)
            .map(|variation| coherent_prototype(2018, anchor, variation, 64))
            .collect::<Vec<_>>();
        let habit = prototypes[0] % 3;

        assert!(prototypes.iter().all(|prototype| prototype % 3 == habit));
        assert!(prototypes.iter().all(|&prototype| prototype < 64));
        assert!(prototypes.iter().copied().collect::<HashSet<_>>().len() > 1);
        assert!(coherent_prototype(2018, anchor, 7, 1) < 1);
        assert!(coherent_prototype(2018, anchor, 7, 2) < 2);
    }

    fn placement(terrain_vertex: u32, x_metres: f32, y_metres: f32) -> TreePlacement {
        TreePlacement {
            terrain_vertex,
            anchor: Vec3::new(
                x_metres / ISLAND_WORLD_METRES,
                y_metres / ISLAND_WORLD_METRES,
                0.02,
            ),
            yaw_radians: 0.0,
            scale: 1.0,
            prototype: 0,
        }
    }

    #[test]
    fn canopy_patches_cover_singletons_and_combine_a_fine_streaming_cell() {
        let singleton = build_clusters(&[placement(7, 0.0, 0.0)]);
        assert_eq!(singleton.len(), 1);
        assert_eq!(singleton[0].member_tree_indices, vec![0]);
        assert_eq!(singleton[0].owner_anchor, placement(7, 0.0, 0.0).anchor);

        let pair = build_clusters(&[placement(0, 0.0, 0.0), placement(1, 30.0, 0.0)]);
        assert_eq!(pair.len(), 1);
        assert_eq!(pair[0].member_tree_indices, vec![0, 1]);
        assert!((pair[0].owner_anchor.x - 15.0 / ISLAND_WORLD_METRES).abs() < 1.0e-7);

        let patch_width_metres = ISLAND_WORLD_METRES / CANOPY_PATCH_RESOLUTION as f32;
        let exact_boundary = build_clusters(&[
            placement(0, 0.0, 0.0),
            placement(1, patch_width_metres, 0.0),
        ]);
        assert_eq!(exact_boundary.len(), 2);
    }

    #[test]
    fn canopy_patch_keeps_all_member_trees_in_one_foliage_input() {
        let clusters = build_clusters(&[
            placement(0, 0.0, 0.0),
            placement(1, 10.0, 0.0),
            placement(2, 20.0, 0.0),
        ]);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].member_tree_indices, vec![0, 1, 2]);
    }

    #[test]
    fn canopy_patch_members_are_ordered_deterministically() {
        let placements = [
            placement(10, 0.0, 0.0),
            placement(20, 4.0, 0.0),
            placement(30, 2.0, 0.0),
        ];
        let clusters = build_clusters(&placements);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].member_tree_indices, vec![0, 1, 2]);

        let reversed = [placements[2], placements[1], placements[0]];
        let reversed_clusters = build_clusters(&reversed);
        let canonical = clusters
            .iter()
            .map(|cluster| {
                cluster
                    .member_tree_indices
                    .iter()
                    .map(|&index| placements[index].terrain_vertex)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let reversed_canonical = reversed_clusters
            .iter()
            .map(|cluster| {
                cluster
                    .member_tree_indices
                    .iter()
                    .map(|&index| reversed[index].terrain_vertex)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(canonical, reversed_canonical);
    }

    fn select_one(vertex: Vec3, normal: Vec3, depth: f32, river: bool) -> ForestGenerationStats {
        select_one_with_sea_proximity(vertex, normal, depth, river, 0.0)
    }

    fn select_one_with_sea_proximity(
        vertex: Vec3,
        normal: Vec3,
        depth: f32,
        river: bool,
        sea_proximity: f32,
    ) -> ForestGenerationStats {
        let terrain = Mesh {
            vertices: vec![vertex],
            normals: vec![normal],
            ..Mesh::default()
        };
        let (_, stats) = select_placements(
            2018,
            &terrain,
            test_surface(&[river], &[depth], &[sea_proximity]),
            eligible_test_options(),
        )
        .unwrap();
        stats
    }

    #[test]
    fn placement_boundaries_enforce_sea_snow_slope_river_and_soil_rules() {
        let seed = 2018;
        let options = eligible_test_options();
        let base = Vec3::new(0.37, 0.41, 0.02);
        assert!(test_coverage(base, seed, options) > options.noise_threshold);

        let sea = select_one(Vec3::new(base.x, base.y, 0.0), Vec3::Z, 1.0, false);
        assert_eq!(sea.sea, 1);
        let below_sea = select_one(Vec3::new(base.x, base.y, -0.001), Vec3::Z, 1.0, false);
        assert_eq!(below_sea.sea, 1);

        let below_snow = select_one(Vec3::new(base.x, base.y, 0.049_999), Vec3::Z, 1.0, false);
        assert_eq!(below_snow.accepted_trees, 1);
        let at_snow = select_one(Vec3::new(base.x, base.y, 0.05), Vec3::Z, 1.0, false);
        assert_eq!(at_snow.snowline, 1);

        let exact_slope = select_one(
            base,
            Vec3::new(
                (1.0 - MINIMUM_NORMAL_Z * MINIMUM_NORMAL_Z).sqrt(),
                0.0,
                MINIMUM_NORMAL_Z,
            ),
            1.0,
            false,
        );
        assert_eq!(exact_slope.accepted_trees, 1);
        let too_steep = select_one(
            base,
            Vec3::new(
                (1.0 - (MINIMUM_NORMAL_Z - 1.0e-5).powi(2)).sqrt(),
                0.0,
                MINIMUM_NORMAL_Z - 1.0e-5,
            ),
            1.0,
            false,
        );
        assert_eq!(too_steep.slope, 1);

        let river = select_one(base, Vec3::Z, 1.0, true);
        assert_eq!(river.river_bed, 1);
        for depth in [0.0, LOOSE_DEPTH_EPSILON, -LOOSE_DEPTH_EPSILON] {
            let zero_soil = select_one(base, Vec3::Z, depth, false);
            assert_eq!(zero_soil.zero_soil, 1);
            assert_eq!(zero_soil.accepted_trees, 0);
        }
    }

    #[test]
    fn shader_beach_candidates_are_rejected_at_the_final_anchor() {
        let one_metre = 1.0 / ISLAND_WORLD_METRES;
        let low_coast = Vec3::new(0.37, 0.41, one_metre);
        let beach = select_one_with_sea_proximity(low_coast, Vec3::Z, 1.0, false, 1.0);
        assert_eq!(beach.beach, 1);
        assert_eq!(beach.accepted_trees, 0);

        let inland_material = select_one_with_sea_proximity(low_coast, Vec3::Z, 1.0, false, 0.0);
        assert_eq!(inland_material.beach, 0);
        assert_eq!(inland_material.accepted_trees, 1);

        let above_beach = select_one_with_sea_proximity(
            Vec3::new(0.37, 0.41, 4.0 / ISLAND_WORLD_METRES),
            Vec3::Z,
            1.0,
            false,
            1.0,
        );
        assert_eq!(above_beach.beach, 0);
        assert_eq!(above_beach.accepted_trees, 1);
    }

    #[test]
    fn shader_patch_noise_matches_the_unity_lattice_hash() {
        assert!(
            (shader_lattice_noise(0, 0, SHADER_PATCH_NOISE_RED_SEED) - 0.657_478_5).abs() < 1.0e-7
        );
        assert!(
            (shader_lattice_noise(0, 0, SHADER_PATCH_NOISE_GREEN_SEED) - 0.869_201_2).abs()
                < 1.0e-7
        );
        assert!(
            (shader_lattice_noise(1, 7, SHADER_PATCH_NOISE_RED_SEED) - 0.304_857_34).abs() < 1.0e-7
        );
    }

    #[test]
    fn zero_soil_is_visible_to_forest_selection() {
        let mut material = crate::terrain::SurfaceMaterial::empty(1);
        material.depths_mut()[0] = 0.0;
        let terrain = flat_mesh(&[Vec3::new(0.37, 0.41, 0.02)]);
        let (_, stats) = select_placements(
            2018,
            &terrain,
            test_surface(&[false], material.depths(), &[0.0]),
            eligible_test_options(),
        )
        .unwrap();
        assert_eq!(stats.zero_soil, 1);
        assert_eq!(stats.accepted_trees, 0);
    }

    #[test]
    fn settled_stone_vertices_are_excluded_without_mutating_soil() {
        let terrain = flat_mesh(&[Vec3::new(0.37, 0.41, 0.02)]);
        let (_, stats) = select_placements(
            2018,
            &terrain,
            ForestSurface {
                river_bed: &[false],
                stones: &[0],
                deposited_depths: &[1.0],
                sea_proximity: &[0.0],
            },
            eligible_test_options(),
        )
        .unwrap();

        assert_eq!(stats.stones, 1);
        assert_eq!(stats.accepted_trees, 0);
    }

    fn candidate(
        terrain_vertex: usize,
        x_metres: f32,
        y_metres: f32,
        coverage: f32,
        scale: f32,
    ) -> PlacementCandidate {
        PlacementCandidate {
            terrain_vertex,
            anchor: Vec3::new(
                x_metres / ISLAND_WORLD_METRES,
                y_metres / ISLAND_WORLD_METRES,
                0.02,
            ),
            coverage,
            scale,
        }
    }

    #[test]
    fn actual_exclusion_zone_rejects_close_nonadjacent_candidates() {
        let mut candidates = vec![
            candidate(7, 0.0, 0.0, 0.9, 1.0),
            candidate(3, 1.0, 0.0, 0.8, 1.0),
            candidate(11, 4.0, 0.0, 0.7, 1.0),
        ];
        prioritize_candidates(&mut candidates);
        let mut stats = ForestGenerationStats::default();
        let accepted = select_candidates_outside_exclusion_zone(candidates, 1.0, &mut stats);

        assert_eq!(
            accepted
                .iter()
                .map(|candidate| candidate.terrain_vertex)
                .collect::<Vec<_>>(),
            vec![7, 11]
        );
        assert_eq!(stats.exclusion_zone, 1);
    }

    #[test]
    fn actual_exclusion_zone_includes_scaled_clearance_boundary() {
        let mut candidates = vec![
            candidate(0, 0.0, 0.0, 0.9, 2.0),
            candidate(1, TREE_CLEARANCE_PER_SCALE_METRES * 2.0, 0.0, 0.8, 1.0),
            candidate(2, 12.0, 0.0, 0.7, 1.0),
        ];
        prioritize_candidates(&mut candidates);
        let mut stats = ForestGenerationStats::default();
        let accepted = select_candidates_outside_exclusion_zone(candidates, 2.0, &mut stats);

        assert_eq!(
            accepted
                .iter()
                .map(|candidate| candidate.terrain_vertex)
                .collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(stats.exclusion_zone, 1);
    }

    #[test]
    fn candidate_scale_clearance_applies_even_when_the_existing_tree_is_smaller() {
        let mut candidates = vec![
            candidate(0, 0.0, 0.0, 0.9, 1.0),
            candidate(1, 5.0, 0.0, 0.8, 2.0),
        ];
        prioritize_candidates(&mut candidates);
        let mut stats = ForestGenerationStats::default();
        let accepted = select_candidates_outside_exclusion_zone(candidates, 2.0, &mut stats);

        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].terrain_vertex, 0);
        assert_eq!(stats.exclusion_zone, 1);
    }

    #[test]
    fn tree_anchor_moves_at_most_halfway_toward_a_selected_fan_centroid() {
        let metres = |value: f32| value / ISLAND_WORLD_METRES;
        let vertices = [
            Vec3::new(0.3, 0.4, 0.02),
            Vec3::new(0.3 + metres(6.0), 0.4, 0.02),
            Vec3::new(0.3, 0.4 + metres(6.0), 0.02),
        ];
        let mut terrain = flat_mesh(&vertices);
        terrain.triangles = vec![0, 1, 2];
        let centroid = (vertices[0] + vertices[1] + vertices[2]) / 3.0;
        let selected = SelectedFanFaces::new(2018, &terrain);
        assert_eq!(selected.get(0).map(|face| face.centroid), Some(centroid));

        let anchor = displaced_tree_anchor(2018, 0, vertices[0], selected.get(0));
        let toward_centroid = centroid - vertices[0];
        let displacement = anchor.position - vertices[0];
        let fraction = displacement.dot(toward_centroid) / toward_centroid.length_squared();
        assert!((0.0..=0.5).contains(&fraction));
        assert!((displacement - toward_centroid * fraction).length() < 1.0e-7);
        let (cover, sea_proximity) =
            shader_material_at_displaced_anchor(0, anchor, &[0.0, 1.0, 1.0], &[0.0, 1.0, 1.0])
                .unwrap();
        let expected_interpolation = fraction * (2.0 / 3.0);
        assert!((cover - expected_interpolation).abs() < 1.0e-7);
        assert!((sea_proximity - expected_interpolation).abs() < 1.0e-7);
        assert_eq!(
            displaced_tree_anchor(2018, 0, vertices[0], None).position,
            vertices[0],
        );
        assert_eq!(
            anchor,
            displaced_tree_anchor(2018, 0, vertices[0], selected.get(0))
        );
    }

    #[test]
    fn placement_rejects_malformed_topology_before_sampling_triangle_fans() {
        let mut terrain = flat_mesh(&[Vec3::new(0.37, 0.41, 0.02)]);
        terrain.triangles = vec![0];
        assert!(
            select_placements(
                2018,
                &terrain,
                test_surface(&[false], &[1.0], &[0.0]),
                eligible_test_options()
            )
            .is_err()
        );

        terrain.triangles = vec![0, 1, 0];
        assert!(
            select_placements(
                2018,
                &terrain,
                test_surface(&[false], &[1.0], &[0.0]),
                eligible_test_options()
            )
            .is_err()
        );

        terrain.triangles.clear();
        assert!(
            select_placements(
                2018,
                &terrain,
                test_surface(&[false], &[1.0], &[]),
                eligible_test_options()
            )
            .is_err()
        );
    }

    #[test]
    fn completeness_matches_every_well_separated_vertex_above_the_strict_noise_threshold() {
        let seed = 7;
        let options = ForestOptions {
            noise_threshold: 0.55,
            ..ForestOptions::default()
        };
        let vertices = (0..48)
            .map(|index| {
                Vec3::new(
                    0.05 + (index % 8) as f32 * 0.1,
                    0.05 + (index / 8) as f32 * 0.1,
                    0.02,
                )
            })
            .collect::<Vec<_>>();
        let terrain = flat_mesh(&vertices);
        let river_bed = vec![false; vertices.len()];
        let deposited_depths = vec![1.0; vertices.len()];
        let sea_proximity = vec![0.0; vertices.len()];
        let (placements, stats) = select_placements(
            seed,
            &terrain,
            test_surface(&river_bed, &deposited_depths, &sea_proximity),
            options,
        )
        .unwrap();
        let expected = vertices
            .iter()
            .enumerate()
            .filter_map(|(index, &vertex)| {
                (test_coverage(vertex, seed, options) > options.noise_threshold)
                    .then_some(index as u32)
            })
            .collect::<Vec<_>>();
        let actual = placements
            .iter()
            .map(|placement| placement.terrain_vertex)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert!(
            placements.iter().all(|placement| {
                vertices[placement.terrain_vertex as usize] == placement.anchor
            })
        );
        assert_eq!(stats.accepted_trees, expected.len());
        assert_eq!(stats.rejected() + stats.accepted_trees, vertices.len());

        let equal_threshold = test_coverage(vertices[0], seed, options);
        let (equal_placements, equal_stats) = select_placements(
            seed,
            &terrain,
            test_surface(&river_bed, &deposited_depths, &sea_proximity),
            ForestOptions {
                noise_threshold: equal_threshold,
                ..options
            },
        )
        .unwrap();
        assert!(
            !equal_placements
                .iter()
                .any(|placement| placement.terrain_vertex == 0)
        );
        assert!(equal_stats.below_or_equal_noise_threshold > 0);
    }

    #[test]
    fn threshold_remaps_retained_scales_without_changing_other_appearance() {
        let metre = 1.0 / ISLAND_WORLD_METRES;
        let vertices = (0..32)
            .map(|index| Vec3::new(0.1 + index as f32 * 10.0 * metre, 0.2, 0.02))
            .collect::<Vec<_>>();
        let terrain = flat_mesh(&vertices);
        let base = ForestOptions {
            noise_threshold: 0.0,
            ..ForestOptions::default()
        };
        let mut coverages = vertices
            .iter()
            .map(|&vertex| test_coverage(vertex, 7, base))
            .collect::<Vec<_>>();
        coverages.sort_unstable_by(f32::total_cmp);
        let higher_threshold = coverages[coverages.len() / 2];
        let (all, _) = select_placements(
            7,
            &terrain,
            test_surface(&[false; 32], &[1.0; 32], &[0.0; 32]),
            base,
        )
        .unwrap();
        let (subset, _) = select_placements(
            7,
            &terrain,
            test_surface(&[false; 32], &[1.0; 32], &[0.0; 32]),
            ForestOptions {
                noise_threshold: higher_threshold,
                ..base
            },
        )
        .unwrap();
        assert!(subset.len() <= all.len());
        let mut changed_scale = false;
        for retained in subset {
            let original = all
                .iter()
                .find(|placement| placement.terrain_vertex == retained.terrain_vertex)
                .unwrap();
            assert_eq!(
                retained.yaw_radians.to_bits(),
                original.yaw_radians.to_bits()
            );
            assert!(retained.scale <= original.scale);
            changed_scale |= retained.scale.to_bits() != original.scale.to_bits();
            assert_eq!(retained.prototype, original.prototype);
            assert_eq!(retained.anchor.x.to_bits(), original.anchor.x.to_bits());
            assert_eq!(retained.anchor.y.to_bits(), original.anchor.y.to_bits());
            assert_eq!(retained.anchor.z.to_bits(), original.anchor.z.to_bits());
        }
        assert!(changed_scale);
    }

    #[test]
    fn owner_grid_copies_whole_tree_and_maps_visual_lods() {
        let source = Mesh {
            vertices: vec![Vec3::ZERO, Vec3::Z],
            normals: vec![Vec3::Z; 2],
            triangles: vec![0, 1, 1],
            ..Mesh::default()
        };
        let range = MeshRange {
            vertex_start: 0,
            vertex_count: 2,
            triangle_start: 0,
            triangle_count: 3,
        };
        let forest = ForestMeshes {
            lod0_wood: source.clone(),
            lod1_wood: source.clone(),
            lod0_foliage: source.clone(),
            lod1_foliage: source.clone(),
            trees: vec![ForestTreeRanges {
                terrain_vertex: 0,
                anchor: Vec3::new(0.5, 0.5, 0.2),
                prototype: 0,
                lod0_wood: range,
                lod1_wood: range,
            }],
            clusters: vec![ForestClusterRanges {
                owner_anchor: Vec3::new(0.5, 0.5, 0.2),
                member_tree_indices: vec![0],
                lod0_foliage: range,
                lod1_foliage: range,
            }],
            placements: Vec::new(),
        };
        let bounds = BoundingBox::new(Vec3::new(0.0, 0.0, f32::MIN), Vec3::new(1.0, 1.0, f32::MAX));
        assert_eq!(
            forest
                .mesh_grid(ForestMeshKind::Wood, 2, bounds, 4)
                .unwrap()
                .len(),
            16
        );
        let tiles = forest
            .mesh_grid(ForestMeshKind::Foliage, 2, bounds, 2)
            .unwrap();
        assert_eq!(tiles.len(), 4);
        assert_eq!(tiles[3].mesh.vertices.len(), 2);
        assert_eq!(tiles[3].mesh.triangles, vec![0, 1, 1]);
        assert_eq!(tiles[3].material, vec![Vec4::new(0.5, 0.5, 0.2, 0.5); 2]);
        assert_eq!(
            tiles[3].mesh.uv,
            vec![Vec2::ZERO, Vec2::new(0.8 * ISLAND_WORLD_METRES, 0.0)]
        );

        let wood_tiles = forest
            .mesh_grid(ForestMeshKind::Wood, 1, bounds, 2)
            .unwrap();
        assert_eq!(
            wood_tiles[3].material.len(),
            wood_tiles[3].mesh.vertices.len()
        );
        assert!(
            wood_tiles[3]
                .material
                .iter()
                .all(|&anchor| anchor == Vec4::new(0.5, 0.5, 0.2, 0.5))
        );
    }

    #[test]
    fn foliage_wind_data_uses_the_nearest_tree_root_per_vertex() {
        let source = Mesh {
            vertices: vec![
                Vec3::new(0.2, 0.25, 0.5),
                Vec3::new(0.3, 0.25, 0.6),
                Vec3::new(0.7, 0.25, 0.7),
                Vec3::new(0.8, 0.25, 0.8),
            ],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        };
        let range = MeshRange {
            vertex_start: 0,
            vertex_count: 4,
            triangle_start: 0,
            triangle_count: 6,
        };
        let left = Vec3::new(0.25, 0.25, 0.1);
        let right = Vec3::new(0.75, 0.25, 0.2);
        let forest = ForestMeshes {
            lod1_foliage: source,
            trees: vec![
                ForestTreeRanges {
                    anchor: left,
                    ..ForestTreeRanges::default()
                },
                ForestTreeRanges {
                    anchor: right,
                    ..ForestTreeRanges::default()
                },
            ],
            clusters: vec![ForestClusterRanges {
                owner_anchor: Vec3::new(0.5, 0.25, 0.15),
                member_tree_indices: vec![0, 1],
                lod1_foliage: range,
                ..ForestClusterRanges::default()
            }],
            ..ForestMeshes::default()
        };

        let tile = forest
            .mesh_grid(
                ForestMeshKind::Foliage,
                1,
                BoundingBox::new(Vec3::new(0.0, 0.0, f32::MIN), Vec3::new(1.0, 1.0, f32::MAX)),
                1,
            )
            .unwrap()
            .pop()
            .unwrap();

        assert_eq!(
            tile.material,
            vec![
                left.extend(0.5),
                left.extend(0.5),
                right.extend(0.5),
                right.extend(0.5),
            ]
        );
        assert_eq!(
            tile.mesh.uv,
            vec![
                Vec2::new(0.4 * ISLAND_WORLD_METRES, 0.0),
                Vec2::new(0.5 * ISLAND_WORLD_METRES, 0.0),
                Vec2::new(0.5 * ISLAND_WORLD_METRES, 0.0),
                Vec2::new(0.6 * ISLAND_WORLD_METRES, 0.0),
            ]
        );
    }

    fn simple_tree_mesh() -> Mesh {
        Mesh {
            vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Z],
            normals: vec![Vec3::Z; 3],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        }
    }

    #[test]
    fn transformed_wood_rotates_its_encoded_bark_axis() {
        let mut source = simple_tree_mesh();
        source.uv = vec![encode_bark_axis(Vec3::X); source.vertices.len()];
        let mut destination = Mesh::default();
        append_transformed(
            &mut destination,
            &source,
            TreePlacement {
                terrain_vertex: 0,
                anchor: Vec3::ZERO,
                yaw_radians: std::f32::consts::FRAC_PI_2,
                scale: 1.0,
                prototype: 0,
            },
        )
        .unwrap();

        assert!(
            destination
                .uv
                .iter()
                .all(|&axis| (decode_bark_axis(axis) - Vec3::Y).length() < 1.0e-6)
        );
    }

    #[test]
    fn identity_append_preserves_non_axis_uvs() {
        let mut source = simple_tree_mesh();
        source.uv = vec![
            Vec2::new(0.1, 0.9),
            Vec2::new(0.2, 0.8),
            Vec2::new(0.3, 0.7),
        ];
        let mut destination = Mesh::default();
        append_identity(&mut destination, &source).unwrap();

        assert_eq!(destination.uv, source.uv);
    }

    #[test]
    fn assembly_transforms_and_rebases_each_tree_range_deterministically() {
        let source = Mesh {
            vertices: vec![
                Vec3::ZERO,
                Vec3::new(0.1, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 0.2),
            ],
            normals: vec![Vec3::Z; 3],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let prototype = TreeMeshes {
            lod0_wood: source.clone(),
            lod0_foliage: source.clone(),
            lod1_wood: source.clone(),
            lod1_foliage: source,
            foliage_supports: vec![
                Vec3::new(-0.001, 0.0, 0.0),
                Vec3::new(0.001, 0.0, 0.0),
                Vec3::new(0.0, 0.001, 0.0),
            ],
            ..TreeMeshes::default()
        };
        let placements = [
            TreePlacement {
                terrain_vertex: 4,
                anchor: Vec3::new(0.25, 0.25, 0.25),
                yaw_radians: std::f32::consts::FRAC_PI_2,
                scale: 2.0,
                prototype: 0,
            },
            TreePlacement {
                terrain_vertex: 9,
                anchor: Vec3::new(0.5, 0.25, 0.5),
                yaw_radians: 0.0,
                scale: 1.0,
                prototype: 0,
            },
        ];
        let terrain = Terrain::new(Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 1.0, 1.0),
            ],
            normals: vec![Vec3::new(-1.0, 0.0, 1.0).normalize(); 4],
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        });
        let first = assemble_forest(
            2018,
            &placements,
            std::slice::from_ref(&prototype),
            &terrain,
        )
        .unwrap();
        let second = assemble_forest(2018, &placements, &[prototype], &terrain).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.trees.len(), 2);
        assert_eq!(first.trees[0].lod0_wood.vertex_start, 0);
        assert_eq!(first.trees[1].lod0_wood.vertex_start, 3);
        assert_eq!(first.lod0_wood.triangles, vec![0, 1, 2, 3, 4, 5]);
        assert!((first.lod0_wood.vertices[1] - Vec3::new(0.25, 0.45, 0.25)).length() < 1.0e-6);
        assert_eq!(first.lod0_wood.vertices[3], Vec3::new(0.5, 0.25, 0.5));
        assert_eq!(first.lod0_wood.vertices[4], Vec3::new(0.6, 0.25, 0.6));

        let terrain = Terrain::new(Mesh {
            vertices: vec![
                Vec3::new(0.37, 0.41, 0.02),
                Vec3::new(0.52, 0.41, 0.02),
                Vec3::new(0.37, 0.56, 0.02),
            ],
            normals: vec![Vec3::Z; 3],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        });
        let options = ForestOptions {
            prototype_count: 1,
            noise_threshold: 0.0,
            ..ForestOptions::default()
        };
        let generated_a = generate_forest(
            2018,
            &terrain,
            test_surface(&[false; 3], &[1.0; 3], &[0.0; 3]),
            options,
        )
        .unwrap();
        let generated_b = generate_forest(
            2018,
            &terrain,
            test_surface(&[false; 3], &[1.0; 3], &[0.0; 3]),
            options,
        )
        .unwrap();
        assert_eq!(generated_a, generated_b);
    }

    #[test]
    fn custom_forest_options_round_trip_through_current_save() {
        let options = ForestOptions {
            patch_size_metres: 173.0,
            noise_threshold: 0.71,
            noise_octaves: 5,
            snowline_metres: 123.0,
            prototype_count: 3,
            minimum_scale: 0.7,
            maximum_scale: 1.4,
        };
        let island = Island::generate_with_forest(
            31,
            IslandOptions {
                terrain_size: 24,
                ..IslandOptions::default()
            },
            options,
        )
        .unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("island-rs-forest-{unique}.motu"));
        island.save(&path).unwrap();
        let loaded = Island::load(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(loaded.forest_options(), options);
        assert_eq!(island, loaded);
    }
}
