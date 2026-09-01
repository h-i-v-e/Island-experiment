#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use crate::forest::{ForestMeshes, shader_beach_candidate, shader_loose_cover};
use crate::terrain::{Terrain, TerrainSupportSample};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, Vec3, Vec4, noise, rng::Rng};

pub(crate) const FERN_TILE_RESOLUTION: usize = 64;
const FERN_PLACEMENT_DOMAIN: u64 = 0x6665_726e_706c_6163;
const FERN_NOISE_DOMAIN: u64 = 0x6665_726e_6e6f_6973;
const GOLDEN_ANGLE: f32 = 2.399_963_1;
const FROND_SEGMENTS: usize = 5;

/// Controls deterministic fern beds generated around accepted tree trunks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FernOptions {
    pub bark_clearance_metres: f32,
    pub outer_radius_metres: f32,
    pub spacing_metres: f32,
    pub patch_size_metres: f32,
    pub coverage_threshold: f32,
    pub minimum_length_metres: f32,
    pub maximum_length_metres: f32,
    pub maximum_slope_degrees: f32,
}

impl Default for FernOptions {
    fn default() -> Self {
        Self {
            bark_clearance_metres: 0.18,
            outer_radius_metres: 1.65,
            spacing_metres: 0.58,
            patch_size_metres: 12.0,
            coverage_threshold: 0.28,
            minimum_length_metres: 0.45,
            maximum_length_metres: 1.15,
            maximum_slope_degrees: 34.0,
        }
    }
}

impl FernOptions {
    pub(crate) fn validate(self) -> Result<Self, String> {
        let positive = |value: f32| value.is_finite() && value > 0.0;
        if !self.bark_clearance_metres.is_finite()
            || !(0.0..=2.0).contains(&self.bark_clearance_metres)
        {
            return Err("fern bark_clearance_metres must be finite and between 0 and 2".into());
        }
        if !positive(self.outer_radius_metres) || self.outer_radius_metres > 8.0 {
            return Err("fern outer_radius_metres must be finite and between 0 and 8".into());
        }
        if !positive(self.spacing_metres) || self.spacing_metres > 4.0 {
            return Err("fern spacing_metres must be finite and between 0 and 4".into());
        }
        if !positive(self.patch_size_metres) {
            return Err("fern patch_size_metres must be finite and greater than zero".into());
        }
        if !self.coverage_threshold.is_finite() || !(0.0..=1.0).contains(&self.coverage_threshold) {
            return Err("fern coverage_threshold must be finite and between 0 and 1".into());
        }
        if !positive(self.minimum_length_metres)
            || !self.maximum_length_metres.is_finite()
            || self.maximum_length_metres < self.minimum_length_metres
        {
            return Err("fern lengths must be finite, positive, and ordered".into());
        }
        if !self.maximum_slope_degrees.is_finite()
            || !(0.0..=60.0).contains(&self.maximum_slope_degrees)
        {
            return Err("fern maximum_slope_degrees must be finite and between 0 and 60".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FernSurface<'a> {
    pub(crate) river_bed: &'a [bool],
    pub(crate) deposited_depths: &'a [f32],
    pub(crate) sea_proximity: &'a [f32],
    pub(crate) forced_rock: &'a [bool],
    pub(crate) stones: &'a [u32],
    pub(crate) reeds: &'a [u32],
    pub(crate) snowline_metres: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FernMeshTile {
    pub(crate) mesh: Mesh,
    pub(crate) material: Vec<Vec4>,
    pub(crate) environment: Vec<Vec2>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FernMeshes {
    tiles: Vec<FernMeshTile>,
    support_vertices: Vec<u32>,
    clump_count: usize,
}

impl Default for FernMeshes {
    fn default() -> Self {
        Self {
            tiles: vec![FernMeshTile::default(); FERN_TILE_RESOLUTION * FERN_TILE_RESOLUTION],
            support_vertices: Vec::new(),
            clump_count: 0,
        }
    }
}

impl FernMeshes {
    pub(crate) fn tiles(&self) -> &[FernMeshTile] {
        &self.tiles
    }

    pub(crate) fn support_vertices(&self) -> &[u32] {
        &self.support_vertices
    }
}

#[derive(Clone, Copy, Debug)]
struct FernClump {
    root: Vec3,
    normal: Vec3,
    length_metres: f32,
    fronds: usize,
    rotation: f32,
    variant: f32,
    tint: f32,
    flexibility: f32,
    phase: f32,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn generate_ferns(
    island_seed: u64,
    terrain: &Terrain,
    forest: &ForestMeshes,
    surface: FernSurface<'_>,
    options: FernOptions,
) -> Result<FernMeshes, String> {
    let options = options.validate()?;
    validate_inputs(terrain, forest, surface)?;
    if terrain.vertex_count() == 0 || forest.placements().is_empty() {
        return Ok(FernMeshes::default());
    }

    let minimum_normal_z = options.maximum_slope_degrees.to_radians().cos();
    let spacing_normalized = options.spacing_metres / ISLAND_WORLD_METRES;
    let mut occupied = HashMap::<(i32, i32), Vec<Vec2>>::new();
    let mut clumps = Vec::new();
    let mut support_vertices = Vec::new();

    for (tree_index, (placement, collider)) in forest
        .placements()
        .iter()
        .zip(forest.trunk_colliders())
        .enumerate()
    {
        let collider = collider?;
        let inner_metres = collider.radius * ISLAND_WORLD_METRES
            + options.bark_clearance_metres * placement.scale.sqrt();
        let outer_metres = options.outer_radius_metres * placement.scale.sqrt();
        if outer_metres <= inner_metres + options.spacing_metres * 0.5 {
            continue;
        }
        let annulus_area = PI * (outer_metres * outer_metres - inner_metres * inner_metres);
        let candidate_count = ((annulus_area / (options.spacing_metres * options.spacing_metres)
            * 1.35)
            .ceil() as usize)
            .max(4);
        let mut rng = Rng::new(
            island_seed
                ^ FERN_PLACEMENT_DOMAIN
                ^ (tree_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
        );
        let angle_offset = rng.range(0.0, TAU);

        for candidate in 0..candidate_count {
            let radial_fraction = (candidate as f32 + 0.5) / candidate_count as f32;
            let radius_metres = (inner_metres * inner_metres
                + radial_fraction * (outer_metres * outer_metres - inner_metres * inner_metres))
                .sqrt();
            let angle = angle_offset + candidate as f32 * GOLDEN_ANGLE;
            let point = collider.owner
                + Vec2::new(angle.cos(), angle.sin()) * radius_metres / ISLAND_WORLD_METRES;
            if !(0.0..=1.0).contains(&point.x) || !(0.0..=1.0).contains(&point.y) {
                continue;
            }
            let sample = terrain.sample_support(point);
            if !eligible_sample(sample, surface, minimum_normal_z) {
                continue;
            }
            let loose_cover = interpolate(sample, surface.deposited_depths, shader_loose_cover);
            if loose_cover <= 0.0
                || sample.position.z * ISLAND_WORLD_METRES >= surface.snowline_metres
            {
                continue;
            }
            let sea_proximity = interpolate(sample, surface.sea_proximity, |value| value);
            if shader_beach_candidate(sample.position, loose_cover, sea_proximity) {
                continue;
            }
            let point_metres = point * ISLAND_WORLD_METRES;
            let coherent = noise::fractal(
                island_seed ^ FERN_NOISE_DOMAIN,
                point_metres.x / options.patch_size_metres,
                point_metres.y / options.patch_size_metres,
                4,
            )
            .mul_add(0.5, 0.5)
            .clamp(0.0, 1.0);
            let coverage = coherent * loose_cover;
            if coverage < options.coverage_threshold
                || !insert_if_spaced(&mut occupied, point, spacing_normalized)
            {
                continue;
            }

            let strength = ((coverage - options.coverage_threshold)
                / (1.0 - options.coverage_threshold).max(f32::EPSILON))
            .clamp(0.0, 1.0);
            let length_metres = rng
                .range(options.minimum_length_metres, options.maximum_length_metres)
                * placement.scale.sqrt()
                * (0.82 + 0.18 * strength);
            clumps.push(FernClump {
                root: sample.position - Vec3::Z * (0.012 / ISLAND_WORLD_METRES),
                normal: sample.normal,
                length_metres,
                fronds: 4 + (rng.next_u64() % 4) as usize,
                rotation: rng.range(0.0, TAU),
                variant: rng.unit(),
                tint: rng.unit(),
                flexibility: rng.range(0.55, 1.0),
                phase: rng.unit(),
            });
            support_vertices.extend(sample.triangle.map(|index| index as u32));
        }
    }

    support_vertices.sort_unstable();
    support_vertices.dedup();
    let mut result = FernMeshes {
        support_vertices,
        clump_count: clumps.len(),
        ..FernMeshes::default()
    };
    for clump in clumps {
        let tile = owner_tile(clump.root);
        append_clump(&mut result.tiles[tile], clump);
    }
    Ok(result)
}

fn validate_inputs(
    terrain: &Terrain,
    forest: &ForestMeshes,
    surface: FernSurface<'_>,
) -> Result<(), String> {
    let count = terrain.vertex_count();
    if terrain.mesh().normals.len() != count
        || surface.river_bed.len() != count
        || surface.deposited_depths.len() != count
        || surface.sea_proximity.len() != count
        || surface.forced_rock.len() != count
    {
        return Err("fern final LOD0 attribute lengths do not match vertices".into());
    }
    if forest.placements().len() != forest.trunk_colliders().count() {
        return Err("fern tree placement and trunk collider counts differ".into());
    }
    for (name, vertices) in [("stone", surface.stones), ("reed", surface.reeds)] {
        if !vertices.windows(2).all(|pair| pair[0] < pair[1])
            || vertices.iter().any(|&index| index as usize >= count)
        {
            return Err(format!(
                "fern {name} vertices must be sorted, unique, and in range"
            ));
        }
    }
    if !surface.snowline_metres.is_finite() || surface.snowline_metres <= 0.0 {
        return Err("fern snowline_metres must be finite and positive".into());
    }
    Ok(())
}

fn eligible_sample(
    sample: TerrainSupportSample,
    surface: FernSurface<'_>,
    minimum_normal_z: f32,
) -> bool {
    sample.position.z > 0.0
        && sample.normal.z >= minimum_normal_z
        && sample.triangle.into_iter().all(|index| {
            !surface.river_bed[index]
                && !surface.forced_rock[index]
                && surface.stones.binary_search(&(index as u32)).is_err()
                && surface.reeds.binary_search(&(index as u32)).is_err()
        })
}

fn interpolate(sample: TerrainSupportSample, values: &[f32], map: impl Fn(f32) -> f32) -> f32 {
    sample
        .triangle
        .into_iter()
        .zip(sample.weights)
        .map(|(index, weight)| map(values[index]) * weight)
        .sum()
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

fn owner_tile(root: Vec3) -> usize {
    let coordinate =
        |value: f32| (value.clamp(0.0, 1.0 - f32::EPSILON) * FERN_TILE_RESOLUTION as f32) as usize;
    coordinate(root.y) * FERN_TILE_RESOLUTION + coordinate(root.x)
}

fn append_clump(tile: &mut FernMeshTile, clump: FernClump) {
    let material = Vec4::new(clump.variant, clump.tint, clump.flexibility, clump.phase);
    for frond in 0..clump.fronds {
        let angle = clump.rotation + TAU * frond as f32 / clump.fronds as f32;
        let direction = Vec2::new(angle.cos(), angle.sin());
        let side = Vec2::new(-direction.y, direction.x);
        let length = clump.length_metres
            * (0.82 + 0.23 * ((frond as f32 * 1.71 + clump.variant * 4.3).sin() * 0.5 + 0.5))
            / ISLAND_WORLD_METRES;
        let width = clump.length_metres * (0.18 + 0.04 * clump.variant) / ISLAND_WORLD_METRES;
        let base = tile.mesh.vertices.len() as u32;
        for row in 0..=FROND_SEGMENTS {
            let progress = row as f32 / FROND_SEGMENTS as f32;
            let taper = (PI * progress).sin().max(0.035).powf(0.72);
            let centre = direction * (length * progress);
            let lift = length
                * (0.24 * (PI * progress).sin() + 0.035 * progress - 0.025 * progress * progress);
            for (column, sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
                let horizontal = centre + side * (sign * width * taper);
                let ground_follow = if clump.normal.z.abs() > f32::EPSILON {
                    -(clump.normal.x * horizontal.x + clump.normal.y * horizontal.y)
                        / clump.normal.z
                        * (1.0 - progress)
                } else {
                    0.0
                };
                tile.mesh
                    .vertices
                    .push(clump.root + horizontal.extend(lift + ground_follow));
                tile.mesh.normals.push(side.extend(0.15).normalize());
                tile.mesh.uv.push(Vec2::new(column as f32, progress));
                tile.material.push(material);
                tile.environment.push(clump.root.truncate());
            }
        }
        for row in 0..FROND_SEGMENTS {
            let index = base + (row * 2) as u32;
            tile.mesh.triangles.extend_from_slice(&[
                index,
                index + 1,
                index + 2,
                index + 1,
                index + 3,
                index + 2,
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forest::{ForestTreeRanges, MeshRange, TreePlacement};

    #[test]
    fn defaults_are_valid() {
        assert_eq!(
            FernOptions::default().validate(),
            Ok(FernOptions::default())
        );
    }

    #[test]
    fn invalid_lengths_are_rejected() {
        assert!(
            FernOptions {
                minimum_length_metres: 2.0,
                maximum_length_metres: 1.0,
                ..FernOptions::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn clump_builds_curved_segmented_fronds_with_sidecars() {
        let mut tile = FernMeshTile::default();
        append_clump(
            &mut tile,
            FernClump {
                root: Vec3::new(0.5, 0.5, 0.1),
                normal: Vec3::Z,
                length_metres: 1.0,
                fronds: 4,
                rotation: 0.0,
                variant: 0.5,
                tint: 0.4,
                flexibility: 0.8,
                phase: 0.2,
            },
        );
        assert_eq!(tile.mesh.vertices.len(), 4 * (FROND_SEGMENTS + 1) * 2);
        assert_eq!(tile.mesh.triangles.len(), 4 * FROND_SEGMENTS * 6);
        assert_eq!(tile.material.len(), tile.mesh.vertices.len());
        assert_eq!(tile.environment.len(), tile.mesh.vertices.len());
        assert!(tile.mesh.vertices.iter().all(|vertex| vertex.is_finite()));
    }

    #[test]
    fn placement_is_deterministic_and_records_terrain_support() {
        let terrain = Terrain::new(Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.02),
                Vec3::new(1.0, 0.0, 0.02),
                Vec3::new(1.0, 1.0, 0.02),
                Vec3::new(0.0, 1.0, 0.02),
            ],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: Vec::new(),
        });
        let trunk_radius = 0.12 / ISLAND_WORLD_METRES;
        let trunk_bottom = Vec3::new(0.5, 0.5, 0.02);
        let mut forest = ForestMeshes::default();
        forest.placements.push(TreePlacement {
            terrain_vertex: 0,
            anchor: trunk_bottom,
            yaw_radians: 0.0,
            scale: 1.0,
            prototype: 0,
        });
        forest.lod2_wood.vertices = vec![
            trunk_bottom + Vec3::new(-trunk_radius, -trunk_radius, 0.0),
            trunk_bottom + Vec3::new(trunk_radius, -trunk_radius, 0.0),
            trunk_bottom + Vec3::new(trunk_radius, trunk_radius, 0.0),
            trunk_bottom + Vec3::new(-trunk_radius, trunk_radius, 0.0),
            trunk_bottom + Vec3::new(-trunk_radius, -trunk_radius, 0.01),
            trunk_bottom + Vec3::new(trunk_radius, -trunk_radius, 0.01),
            trunk_bottom + Vec3::new(trunk_radius, trunk_radius, 0.01),
            trunk_bottom + Vec3::new(-trunk_radius, trunk_radius, 0.01),
        ];
        forest.trees.push(ForestTreeRanges {
            terrain_vertex: 0,
            anchor: trunk_bottom,
            prototype: 0,
            lod0_wood: MeshRange::default(),
            lod1_wood: MeshRange::default(),
            lod2_wood: MeshRange {
                vertex_start: 0,
                vertex_count: 8,
                triangle_start: 0,
                triangle_count: 0,
            },
        });
        let river = [false; 4];
        let depths = [0.002; 4];
        let proximity = [0.0; 4];
        let rock = [false; 4];
        let surface = FernSurface {
            river_bed: &river,
            deposited_depths: &depths,
            sea_proximity: &proximity,
            forced_rock: &rock,
            stones: &[],
            reeds: &[],
            snowline_metres: 100.0,
        };
        let options = FernOptions {
            coverage_threshold: 0.0,
            ..FernOptions::default()
        };
        let first = generate_ferns(77, &terrain, &forest, surface, options).unwrap();
        let second = generate_ferns(77, &terrain, &forest, surface, options).unwrap();
        assert_eq!(first, second);
        assert!(first.clump_count > 0);
        assert!(!first.support_vertices.is_empty());
        assert!(
            first
                .support_vertices
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
    }
}
