//! Deterministic waterside reeds and rushes built from owner-tiled card clumps.
//!
//! Placement starts from the exact final river-bed mask and the final below-sea
//! edge. River distance is propagated only across dry final-LOD0 terrain. A
//! coastal path is enabled only where preserved pre-carve sea proximity is
//! zero, identifying shoreline introduced later by submerged river carving.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    f32::consts::FRAC_PI_4,
};

use crate::{
    Adjacency, ISLAND_WORLD_METRES, Mesh, Vec2, Vec3, Vec4, noise, rivers::WaterfallFoot, rng::Rng,
};

pub(crate) const REED_TILE_RESOLUTION: usize = 64;
const REED_NOISE_DOMAIN: u64 = 0x7265_6564_5f6e_6f69;
const REED_PLACEMENT_DOMAIN: u64 = 0x7265_6564_5f70_6c63;
const CARD_PLANE_COUNT: usize = 4;
const CARD_LEVEL_COUNT: usize = 4;
const WATERFALL_CLEARANCE_METRES: f32 = 3.0;
const WATERWARD_PULL_BANK_FRACTION: f32 = 0.45;
const ROOT_SINK_HEIGHT_FRACTION: f32 = 0.18;
const PRE_CARVE_SEA_PROXIMITY_EPSILON: f32 = 1.0e-4;

/// Physical and deterministic controls for riverbank vegetation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReedOptions {
    pub bank_width_metres: f32,
    pub patch_size_metres: f32,
    pub coverage_threshold: f32,
    pub spacing_metres: f32,
    pub rush_ratio: f32,
    pub minimum_height_metres: f32,
    pub maximum_height_metres: f32,
    pub maximum_slope_degrees: f32,
}

impl Default for ReedOptions {
    fn default() -> Self {
        Self {
            bank_width_metres: 0.8,
            patch_size_metres: 8.0,
            coverage_threshold: 0.18,
            spacing_metres: 0.36,
            rush_ratio: 0.45,
            minimum_height_metres: 0.65,
            maximum_height_metres: 2.1,
            maximum_slope_degrees: 32.0,
        }
    }
}

impl ReedOptions {
    pub(crate) fn validate(self) -> Result<Self, String> {
        let finite_positive = |value: f32| value.is_finite() && value > 0.0;
        if !finite_positive(self.bank_width_metres) || self.bank_width_metres > 20.0 {
            return Err("reed bank_width_metres must be finite and between 0 and 20".into());
        }
        if !finite_positive(self.patch_size_metres) {
            return Err("reed patch_size_metres must be finite and greater than zero".into());
        }
        if !self.coverage_threshold.is_finite() || !(0.0..=1.0).contains(&self.coverage_threshold) {
            return Err("reed coverage_threshold must be finite and between 0 and 1".into());
        }
        if !finite_positive(self.spacing_metres) || self.spacing_metres > 10.0 {
            return Err("reed spacing_metres must be finite and between 0 and 10".into());
        }
        if !self.rush_ratio.is_finite() || !(0.0..=1.0).contains(&self.rush_ratio) {
            return Err("reed rush_ratio must be finite and between 0 and 1".into());
        }
        if !finite_positive(self.minimum_height_metres)
            || !self.maximum_height_metres.is_finite()
            || self.maximum_height_metres < self.minimum_height_metres
        {
            return Err("reed heights must be finite, positive, and ordered".into());
        }
        if !self.maximum_slope_degrees.is_finite()
            || !(0.0..=60.0).contains(&self.maximum_slope_degrees)
        {
            return Err("reed maximum_slope_degrees must be finite and between 0 and 60".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ReedSurface<'a> {
    pub(crate) river_bed: &'a [bool],
    pub(crate) deposited_depths: &'a [f32],
    pub(crate) sea_proximity: &'a [f32],
    pub(crate) forced_rock: &'a [bool],
    pub(crate) stones: &'a [u32],
    pub(crate) waterfall_feet: &'a [WaterfallFoot],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ReedMeshTile {
    pub(crate) mesh: Mesh,
    pub(crate) material: Vec<Vec4>,
    pub(crate) environment: Vec<Vec2>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReedMeshes {
    tiles: Vec<ReedMeshTile>,
    forest_exclusion_vertices: Vec<u32>,
    clump_count: usize,
}

impl Default for ReedMeshes {
    fn default() -> Self {
        Self {
            tiles: vec![ReedMeshTile::default(); REED_TILE_RESOLUTION * REED_TILE_RESOLUTION],
            forest_exclusion_vertices: Vec::new(),
            clump_count: 0,
        }
    }
}

impl ReedMeshes {
    pub(crate) fn tiles(&self) -> &[ReedMeshTile] {
        &self.tiles
    }

    pub(crate) fn forest_exclusion_vertices(&self) -> &[u32] {
        &self.forest_exclusion_vertices
    }

    #[cfg(test)]
    fn clump_count(&self) -> usize {
        self.clump_count
    }
}

#[derive(Clone, Copy, Debug)]
struct QueueEntry {
    distance: f32,
    vertex: usize,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.distance.to_bits() == other.distance.to_bits() && self.vertex == other.vertex
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

#[derive(Clone, Copy, Debug)]
struct Clump {
    root: Vec3,
    ground_normal: Vec3,
    height_metres: f32,
    width_metres: f32,
    variant: f32,
    tint: f32,
    stiffness: f32,
    phase: f32,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn generate_reeds(
    island_seed: u64,
    terrain: &Mesh,
    surface: ReedSurface<'_>,
    options: ReedOptions,
) -> Result<ReedMeshes, String> {
    let options = options.validate()?;
    validate_inputs(terrain, surface)?;
    if terrain.vertices.is_empty() {
        return Ok(ReedMeshes::default());
    }

    let adjacency = terrain.adjacency();
    let sea: Vec<bool> = terrain
        .vertices
        .iter()
        .map(|vertex| vertex.z <= 0.0)
        .collect();
    if !surface.river_bed.iter().any(|river| *river) && !sea.iter().any(|water| *water) {
        return Ok(ReedMeshes::default());
    }
    let river_distances = dry_bank_distances(
        terrain,
        &adjacency,
        surface.river_bed,
        options.bank_width_metres,
    );
    let coast_distances = dry_bank_distances(terrain, &adjacency, &sea, options.bank_width_metres);
    let minimum_normal_z = options.maximum_slope_degrees.to_radians().cos();
    let spacing_normalized = options.spacing_metres / ISLAND_WORLD_METRES;
    let spacing_squared = options.spacing_metres * options.spacing_metres;
    let waterward_pull =
        options.bank_width_metres * WATERWARD_PULL_BANK_FRACTION / ISLAND_WORLD_METRES;
    let root_sink = options.minimum_height_metres * ROOT_SINK_HEIGHT_FRACTION / ISLAND_WORLD_METRES;
    let mut occupied = HashMap::<(i32, i32), Vec<Vec2>>::new();
    let mut rng = Rng::new(island_seed ^ REED_PLACEMENT_DOMAIN);
    let mut clumps = Vec::new();
    let mut exclusion = Vec::new();

    for triangle in terrain.triangles.chunks_exact(3) {
        let indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if indices.iter().any(|&index| surface.river_bed[index]) {
            continue;
        }
        let mean_river_distance = indices
            .iter()
            .map(|&index| river_distances[index])
            .sum::<f32>()
            / 3.0;
        let mean_coast_distance = indices
            .iter()
            .map(|&index| coast_distances[index])
            .sum::<f32>()
            / 3.0;
        let mean_coast_proximity = indices
            .iter()
            .map(|&index| surface.sea_proximity[index])
            .sum::<f32>()
            / 3.0;
        let river_bank = bank_distance_is_eligible(mean_river_distance, options.bank_width_metres);
        let coast_bank = bank_distance_is_eligible(mean_coast_distance, options.bank_width_metres)
            && mean_coast_proximity <= PRE_CARVE_SEA_PROXIMITY_EPSILON;
        if !river_bank && !coast_bank {
            continue;
        }
        if indices.iter().any(|&index| {
            terrain.vertices[index].z <= 0.0
                || terrain.normals[index].z < minimum_normal_z
                || surface.forced_rock[index]
                || surface.stones.binary_search(&(index as u32)).is_ok()
        }) {
            continue;
        }

        let vertices = indices.map(|index| terrain.vertices[index]);
        let centroid = (vertices[0] + vertices[1] + vertices[2]) / 3.0;
        if inside_waterfall_clearance(centroid, surface.waterfall_feet) {
            continue;
        }
        let soil_metres = indices
            .iter()
            .map(|&index| surface.deposited_depths[index].max(0.0) * ISLAND_WORLD_METRES)
            .sum::<f32>()
            / 3.0;
        let soil = (soil_metres / 0.12).clamp(0.0, 1.0);
        if soil <= 0.0 {
            continue;
        }
        let frequency = ISLAND_WORLD_METRES / options.patch_size_metres;
        let coherent = noise::fractal(
            island_seed ^ REED_NOISE_DOMAIN,
            centroid.x * frequency,
            centroid.y * frequency,
            4,
        )
        .mul_add(0.5, 0.5);
        let bank_proximity = if river_bank {
            1.0 - (mean_river_distance / options.bank_width_metres).clamp(0.0, 1.0)
        } else {
            1.0 - (mean_coast_distance / options.bank_width_metres).clamp(0.0, 1.0)
        };
        let coverage = ((coherent - options.coverage_threshold)
            / (1.0 - options.coverage_threshold).max(f32::EPSILON))
        .clamp(0.0, 1.0)
            * soil
            * bank_proximity.sqrt();
        if coverage <= 0.0 {
            continue;
        }

        let area_metres = 0.5
            * (vertices[1].truncate() - vertices[0].truncate())
                .perp_dot(vertices[2].truncate() - vertices[0].truncate())
                .abs()
            * ISLAND_WORLD_METRES
            * ISLAND_WORLD_METRES;
        let expected = area_metres * coverage / spacing_squared;
        let count = expected.floor() as usize + usize::from(rng.unit() < expected.fract());
        for _ in 0..count {
            let barycentric = random_barycentric(&mut rng);
            let river_distance =
                barycentric_mix(indices.map(|index| river_distances[index]), barycentric);
            let coast_proximity = barycentric_mix(
                indices.map(|index| surface.sea_proximity[index]),
                barycentric,
            );
            let coast_distance =
                barycentric_mix(indices.map(|index| coast_distances[index]), barycentric);
            let (root_distance, target_water) =
                if bank_distance_is_eligible(river_distance, options.bank_width_metres) {
                    (river_distance, surface.river_bed)
                } else if bank_distance_is_eligible(coast_distance, options.bank_width_metres)
                    && coast_proximity <= PRE_CARVE_SEA_PROXIMITY_EPSILON
                {
                    (coast_distance, sea.as_slice())
                } else {
                    continue;
                };
            let sampled_root = vertices[0] * barycentric.x
                + vertices[1] * barycentric.y
                + vertices[2] * (1.0 - barycentric.x - barycentric.y);
            let mut root = pull_toward_water(
                sampled_root,
                indices,
                terrain,
                &adjacency,
                target_water,
                waterward_pull,
            );
            root.z -= root_sink;
            if inside_waterfall_clearance(root, surface.waterfall_feet)
                || !insert_if_spaced(&mut occupied, root.truncate(), spacing_normalized)
            {
                continue;
            }
            let inner = root_distance / options.bank_width_metres;
            let rush = inner > 1.0 - options.rush_ratio;
            let variant = f32::from(rush);
            let height_range = options.maximum_height_metres - options.minimum_height_metres;
            let height = options.minimum_height_metres
                + height_range
                    * rng.range(if rush { 0.0 } else { 0.35 }, if rush { 0.6 } else { 1.0 });
            clumps.push(Clump {
                root,
                ground_normal: (terrain.normals[indices[0]]
                    + terrain.normals[indices[1]]
                    + terrain.normals[indices[2]])
                    .normalize_or_zero(),
                height_metres: height,
                width_metres: height * rng.range(0.34, 0.48),
                variant,
                tint: rng.range(0.0, 1.0),
                stiffness: rng.range(if rush { 0.55 } else { 0.25 }, if rush { 0.9 } else { 0.7 }),
                phase: rng.range(0.0, 1.0),
            });
            exclusion.extend(indices.iter().map(|&index| index as u32));
        }
    }

    exclusion.sort_unstable();
    exclusion.dedup();
    let mut result = ReedMeshes {
        forest_exclusion_vertices: exclusion,
        clump_count: clumps.len(),
        ..ReedMeshes::default()
    };
    for clump in clumps {
        let tile = owner_tile(clump.root);
        append_clump(&mut result.tiles[tile], clump);
    }
    Ok(result)
}

fn validate_inputs(terrain: &Mesh, surface: ReedSurface<'_>) -> Result<(), String> {
    let count = terrain.vertices.len();
    if terrain.normals.len() != count
        || surface.river_bed.len() != count
        || surface.deposited_depths.len() != count
        || surface.sea_proximity.len() != count
        || surface.forced_rock.len() != count
    {
        return Err("reed final LOD0 attribute lengths do not match vertices".into());
    }
    if !surface.stones.windows(2).all(|pair| pair[0] < pair[1])
        || surface.stones.iter().any(|&index| index as usize >= count)
    {
        return Err("reed stone vertices must be sorted, unique, and in range".into());
    }
    if terrain
        .triangles
        .iter()
        .any(|&index| index as usize >= count)
    {
        return Err("reed final LOD0 contains an out-of-range triangle index".into());
    }
    if !terrain.triangles.len().is_multiple_of(3) {
        return Err("reed final LOD0 triangle index count is not divisible by three".into());
    }
    Ok(())
}

fn dry_bank_distances(
    terrain: &Mesh,
    adjacency: &Adjacency,
    river_bed: &[bool],
    limit: f32,
) -> Vec<f32> {
    let mut distances = vec![f32::INFINITY; terrain.vertices.len()];
    let mut queue = BinaryHeap::new();
    for index in 0..terrain.vertices.len() {
        if river_bed[index] {
            continue;
        }
        if adjacency[index]
            .iter()
            .any(|&neighbour| river_bed[neighbour])
        {
            distances[index] = 0.0;
            queue.push(QueueEntry {
                distance: 0.0,
                vertex: index,
            });
        }
    }
    while let Some(current) = queue.pop() {
        if current.distance > distances[current.vertex] || current.distance > limit {
            continue;
        }
        for &next in &adjacency[current.vertex] {
            if river_bed[next] {
                continue;
            }
            let edge = (terrain.vertices[next].truncate()
                - terrain.vertices[current.vertex].truncate())
            .length()
                * ISLAND_WORLD_METRES;
            let candidate = current.distance + edge;
            if candidate <= limit && candidate < distances[next] {
                distances[next] = candidate;
                queue.push(QueueEntry {
                    distance: candidate,
                    vertex: next,
                });
            }
        }
    }
    distances
}

fn pull_toward_water(
    root: Vec3,
    triangle: [usize; 3],
    terrain: &Mesh,
    adjacency: &Adjacency,
    water: &[bool],
    maximum_pull: f32,
) -> Vec3 {
    let target = triangle
        .into_iter()
        .flat_map(|vertex| adjacency[vertex].iter().copied())
        .filter(|&vertex| water[vertex])
        .min_by(|&left, &right| {
            terrain.vertices[left]
                .truncate()
                .distance_squared(root.truncate())
                .total_cmp(
                    &terrain.vertices[right]
                        .truncate()
                        .distance_squared(root.truncate()),
                )
        })
        .map(|vertex| terrain.vertices[vertex]);
    let Some(target) = target else {
        return root;
    };
    let horizontal_distance = target.truncate().distance(root.truncate());
    if horizontal_distance <= f32::EPSILON {
        return root;
    }
    root.lerp(target, (maximum_pull / horizontal_distance).min(1.0))
}

fn bank_distance_is_eligible(distance: f32, limit: f32) -> bool {
    distance.is_finite() && distance <= limit
}

fn barycentric_mix(values: [f32; 3], barycentric: Vec2) -> f32 {
    values[0] * barycentric.x
        + values[1] * barycentric.y
        + values[2] * (1.0 - barycentric.x - barycentric.y)
}

fn random_barycentric(rng: &mut Rng) -> Vec2 {
    let a = rng.unit();
    let b = rng.unit();
    if a + b <= 1.0 {
        Vec2::new(a, b)
    } else {
        Vec2::new(1.0 - a, 1.0 - b)
    }
}

fn insert_if_spaced(
    occupied: &mut HashMap<(i32, i32), Vec<Vec2>>,
    point: Vec2,
    spacing: f32,
) -> bool {
    let cell = (
        (point.x / spacing).floor() as i32,
        (point.y / spacing).floor() as i32,
    );
    for y in cell.1 - 1..=cell.1 + 1 {
        for x in cell.0 - 1..=cell.0 + 1 {
            if occupied
                .get(&(x, y))
                .is_some_and(|points| points.iter().any(|other| other.distance(point) < spacing))
            {
                return false;
            }
        }
    }
    occupied.entry(cell).or_default().push(point);
    true
}

fn inside_waterfall_clearance(point: Vec3, feet: &[WaterfallFoot]) -> bool {
    feet.iter().any(|foot| {
        let centre = foot.position.truncate()
            + foot.direction.truncate().normalize_or_zero()
                * (foot.drop.max(0.0) + WATERFALL_CLEARANCE_METRES / ISLAND_WORLD_METRES);
        let clearance = foot.half_width + WATERFALL_CLEARANCE_METRES / ISLAND_WORLD_METRES;
        point.truncate().distance(centre) <= clearance
    })
}

fn owner_tile(root: Vec3) -> usize {
    let x = (root.x.clamp(0.0, 1.0 - f32::EPSILON) * REED_TILE_RESOLUTION as f32) as usize;
    let y = (root.y.clamp(0.0, 1.0 - f32::EPSILON) * REED_TILE_RESOLUTION as f32) as usize;
    y * REED_TILE_RESOLUTION + x
}

fn append_clump(tile: &mut ReedMeshTile, clump: Clump) {
    let height = clump.height_metres / ISLAND_WORLD_METRES;
    let half_width = 0.5 * clump.width_metres / ISLAND_WORLD_METRES;
    let material = Vec4::new(clump.variant, clump.tint, clump.stiffness, clump.phase);
    for plane in 0..CARD_PLANE_COUNT {
        let angle = plane as f32 * FRAC_PI_4;
        let tangent = Vec2::new(angle.cos(), angle.sin());
        let normal = Vec3::new(-tangent.y, tangent.x, 0.0);
        let base = tile.mesh.vertices.len() as u32;
        for level in 0..CARD_LEVEL_COUNT {
            let v = level as f32 / (CARD_LEVEL_COUNT - 1) as f32;
            for side in [-1.0_f32, 1.0] {
                let horizontal = tangent * (side * half_width);
                let ground_offset = if clump.ground_normal.z.abs() > f32::EPSILON {
                    -(clump.ground_normal.x * horizontal.x + clump.ground_normal.y * horizontal.y)
                        / clump.ground_normal.z
                } else {
                    0.0
                };
                let conformed_ground = ground_offset * (1.0 - v);
                tile.mesh
                    .vertices
                    .push(clump.root + horizontal.extend(v * height + conformed_ground));
                tile.mesh.normals.push(normal);
                tile.mesh.uv.push(Vec2::new(side.mul_add(0.5, 0.5), v));
                tile.material.push(material);
                tile.environment.push(clump.root.truncate());
            }
        }
        for level in 0..CARD_LEVEL_COUNT - 1 {
            let row = base + (level * 2) as u32;
            tile.mesh.triangles.extend_from_slice(&[
                row,
                row + 1,
                row + 2,
                row + 1,
                row + 3,
                row + 2,
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn river_bank_mesh() -> (Mesh, Vec<bool>) {
        let vertices = vec![
            Vec3::new(0.0, 0.0, 0.01),
            Vec3::new(0.0005, 0.0, 0.01),
            Vec3::new(0.001, 0.0, 0.01),
            Vec3::new(0.0, 0.0005, 0.01),
            Vec3::new(0.0005, 0.0005, 0.01),
            Vec3::new(0.001, 0.0005, 0.01),
        ];
        let triangles = vec![0, 1, 3, 1, 4, 3, 1, 2, 4, 2, 5, 4];
        (
            Mesh {
                normals: vec![Vec3::Z; vertices.len()],
                uv: vertices.iter().map(|vertex| vertex.truncate()).collect(),
                vertices,
                triangles,
            },
            vec![true, false, false, false, false, false],
        )
    }

    #[test]
    fn options_reject_non_finite_values() {
        assert!(
            ReedOptions {
                spacing_metres: f32::NAN,
                ..ReedOptions::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn defaults_form_dense_patches_on_the_immediate_bank() {
        let options = ReedOptions::default();
        assert!(options.bank_width_metres <= 0.8);
        assert!(options.patch_size_metres <= 8.0);
        assert!(options.coverage_threshold <= 0.2);
        assert!(options.spacing_metres <= 0.36);
    }

    #[test]
    fn root_pulls_horizontally_and_down_toward_a_connected_water_vertex() {
        let (mut mesh, river_bed) = river_bank_mesh();
        mesh.vertices[0].z = 0.005;
        let adjacency = mesh.adjacency();
        let root = (mesh.vertices[1] + mesh.vertices[3] + mesh.vertices[4]) / 3.0;
        let pulled = pull_toward_water(
            root,
            [1, 4, 3],
            &mesh,
            &adjacency,
            &river_bed,
            0.2 / ISLAND_WORLD_METRES,
        );
        assert!(
            pulled.truncate().distance(mesh.vertices[0].truncate())
                < root.truncate().distance(mesh.vertices[0].truncate())
        );
        assert!(pulled.z < root.z);
    }

    #[test]
    fn card_base_corners_follow_the_local_ground_plane() {
        let root = Vec3::new(0.5, 0.5, 0.01);
        let ground_normal = Vec3::new(-0.4, 0.2, 1.0).normalize();
        let mut tile = ReedMeshTile::default();
        append_clump(
            &mut tile,
            Clump {
                root,
                ground_normal,
                height_metres: 1.0,
                width_metres: 0.5,
                variant: 0.0,
                tint: 0.5,
                stiffness: 0.5,
                phase: 0.5,
            },
        );
        for plane in 0..CARD_PLANE_COUNT {
            for side in 0..2 {
                let corner = tile.mesh.vertices[plane * CARD_LEVEL_COUNT * 2 + side];
                assert!((corner - root).dot(ground_normal).abs() < 1.0e-6);
            }
        }
    }

    #[test]
    fn placement_is_deterministic_and_builds_parallel_card_attributes() {
        let (mesh, river_bed) = river_bank_mesh();
        let depths = vec![0.001; mesh.vertices.len()];
        let forced_rock = vec![false; mesh.vertices.len()];
        let sea_proximity = vec![0.0; mesh.vertices.len()];
        let surface = ReedSurface {
            river_bed: &river_bed,
            deposited_depths: &depths,
            sea_proximity: &sea_proximity,
            forced_rock: &forced_rock,
            stones: &[],
            waterfall_feet: &[],
        };
        let options = ReedOptions {
            bank_width_metres: 5.0,
            patch_size_metres: 1000.0,
            coverage_threshold: 0.0,
            spacing_metres: 0.2,
            ..ReedOptions::default()
        };
        let first = generate_reeds(7, &mesh, surface, options).unwrap();
        let second = generate_reeds(7, &mesh, surface, options).unwrap();
        assert_eq!(first, second);
        assert!(first.clump_count() > 0);
        assert!(
            first
                .tiles()
                .iter()
                .flat_map(|tile| &tile.mesh.vertices)
                .any(|vertex| vertex.z < 0.01)
        );
        for tile in first.tiles() {
            assert_eq!(tile.mesh.vertices.len(), tile.mesh.normals.len());
            assert_eq!(tile.mesh.vertices.len(), tile.mesh.uv.len());
            assert_eq!(tile.mesh.vertices.len(), tile.material.len());
            assert_eq!(tile.mesh.vertices.len(), tile.environment.len());
            assert!(tile.mesh.vertices.iter().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn terrain_without_river_or_sea_produces_no_clumps() {
        let (mesh, _) = river_bank_mesh();
        let empty = vec![false; mesh.vertices.len()];
        let depths = vec![0.001; mesh.vertices.len()];
        let sea_proximity = vec![0.0; mesh.vertices.len()];
        let result = generate_reeds(
            7,
            &mesh,
            ReedSurface {
                river_bed: &empty,
                deposited_depths: &depths,
                sea_proximity: &sea_proximity,
                forced_rock: &empty,
                stones: &[],
                waterfall_feet: &[],
            },
            ReedOptions::default(),
        )
        .unwrap();
        assert_eq!(result.clump_count(), 0);
    }

    #[test]
    fn post_carve_coast_with_zero_sea_proximity_produces_reed_clumps() {
        let (mut mesh, _) = river_bank_mesh();
        for vertex in &mut mesh.vertices {
            vertex.z = 0.0005;
        }
        mesh.vertices[0].z = -0.001;
        let empty = vec![false; mesh.vertices.len()];
        let depths = vec![0.0001; mesh.vertices.len()];
        let sea_proximity = vec![0.0; mesh.vertices.len()];
        let options = ReedOptions {
            bank_width_metres: 5.0,
            patch_size_metres: 1000.0,
            coverage_threshold: 0.0,
            spacing_metres: 0.2,
            ..ReedOptions::default()
        };

        let result = generate_reeds(
            7,
            &mesh,
            ReedSurface {
                river_bed: &empty,
                deposited_depths: &depths,
                sea_proximity: &sea_proximity,
                forced_rock: &empty,
                stones: &[],
                waterfall_feet: &[],
            },
            options,
        )
        .unwrap();

        assert!(result.clump_count() > 0);
    }

    #[test]
    fn original_coast_with_nonzero_sea_proximity_suppresses_coastal_reeds() {
        let (mut mesh, _) = river_bank_mesh();
        for vertex in &mut mesh.vertices {
            vertex.z = 0.0005;
        }
        mesh.vertices[0].z = -0.001;
        let empty = vec![false; mesh.vertices.len()];
        let depths = vec![0.002; mesh.vertices.len()];
        let sea_proximity = vec![1.0; mesh.vertices.len()];
        let result = generate_reeds(
            7,
            &mesh,
            ReedSurface {
                river_bed: &empty,
                deposited_depths: &depths,
                sea_proximity: &sea_proximity,
                forced_rock: &empty,
                stones: &[],
                waterfall_feet: &[],
            },
            ReedOptions {
                bank_width_metres: 5.0,
                patch_size_metres: 1000.0,
                coverage_threshold: 0.0,
                spacing_metres: 0.2,
                ..ReedOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.clump_count(), 0);
    }
}
