//! Disturb the rendered ground beside settlement features.
use super::{Island, spatial_bins::SpatialBins, town::DecorationClearanceStats};
use crate::{
    DecorationClearance, ISLAND_WORLD_METRES, Vec2, Vec3,
    procedural_textures::periodic::{Period2D, value_noise},
};
use rayon::prelude::*;

const MAX_FEATHER_METRES: f32 = 5.5;
const SEAM_FADE_METRES: f32 = 3.0;
const NOISE_PERIOD: Period2D = Period2D::new_unchecked(1024, 1024);
const WEAR_NOISE_DOMAIN: u64 = 0x3bd9_335b_0cca_6223;

impl Island {
    pub(crate) fn clear_settlement_ground(
        &mut self,
        clearance: &[DecorationClearance],
    ) -> Result<DecorationClearanceStats, String> {
        let stats = self.clear_decorations(clearance)?;
        self.wear_settlement_ground(clearance);
        Ok(stats)
    }

    pub(super) fn wear_settlement_ground(&mut self, clearance: &[DecorationClearance]) {
        if clearance.is_empty() {
            return;
        }
        let mut areas = SpatialBins::new();
        for &area in clearance {
            let area = WearArea {
                centre: Vec2::from_array(area.centre),
                half: Vec2::from_array(area.half_extents),
                axis: Vec2::from_angle(area.yaw),
            };
            let extent = area.axis.abs() * area.half.x
                + area.axis.perp().abs() * area.half.y
                + MAX_FEATHER_METRES;
            areas.insert(area.centre - extent, area.centre + extent, area);
        }
        let seed = self.seed ^ WEAR_NOISE_DOMAIN;
        let terrain = &mut self.terrain;
        terrain
            .disturbed_ground
            .resize(terrain.mesh.vertices.len(), 0.);
        terrain
            .disturbed_ground
            .par_iter_mut()
            .zip(&terrain.mesh.vertices)
            .for_each(|(worn, &vertex)| *worn = worn.max(vertex_wear(vertex, &areas, seed)));
    }
}

struct WearArea {
    centre: Vec2,
    half: Vec2,
    axis: Vec2,
}

impl WearArea {
    fn outside_distance_squared(&self, point: Vec2) -> f32 {
        let delta = point - self.centre;
        let local = Vec2::new(self.axis.dot(delta), self.axis.perp_dot(delta));
        (local.abs() - self.half).max(Vec2::ZERO).length_squared()
    }
}

fn vertex_wear(vertex: Vec3, areas: &SpatialBins<WearArea>, seed: u64) -> f32 {
    if vertex.z <= 0.0 {
        return 0.;
    }
    let point = vertex.truncate() * ISLAND_WORLD_METRES;
    let distance_squared = areas
        .nearby(point, point)
        .map(|area| area.outside_distance_squared(point))
        .fold(MAX_FEATHER_METRES.powi(2), f32::min);
    if distance_squared >= MAX_FEATHER_METRES.powi(2) {
        return 0.;
    }

    // Vary the width and depth along each edge in world space so bare patches
    // stay in place across LODs and adjacent footprints share the same pattern.
    let patch = 0.5 + 0.5 * value_noise(seed, (point / 3.0).to_array(), NOISE_PERIOD);
    let width = 2.8 + (MAX_FEATHER_METRES - 2.8) * patch;
    let wear = 0.66 + 0.22 * patch;
    let distance = distance_squared.sqrt();
    let feather = smooth_unit(distance / width);

    // Match the undisturbed material at the boundary between island tiles.
    let seam_distance = point.min(Vec2::splat(ISLAND_WORLD_METRES) - point);
    let seam_weight = smooth_unit(seam_distance.min_element() / SEAM_FADE_METRES);
    wear * (1. - feather * feather) * seam_weight
}

fn smooth_unit(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForestOptions, Vec4};

    #[test]
    fn settlement_wear_is_patchy_local_and_repeatable() {
        let mut island = Island::shared_forest_fixture(ForestOptions::default());
        let original = Vec4::new(0.3, 0.9, 0.1, 0.2);
        island.material.values.fill(original);
        let feature = DecorationClearance {
            centre: [680.0, 641.25],
            half_extents: [90.0, 0.5],
            yaw: 0.0,
        };
        island.clear_settlement_ground(&[feature]).unwrap();
        let mesh = island.terrain.mesh();
        let values = island.surface_weights_for(mesh);
        let shoulder = &values[7..11];
        assert!(shoulder.iter().all(|value| value.y < 0.5));
        assert!(shoulder.iter().any(|value| value.y < 0.35));
        assert!(shoulder.iter().any(|value| value.y != shoulder[0].y));
        assert_eq!(values[0], Vec4::new(1., 1., 0.1, 0.));
        assert!(
            island
                .material
                .values
                .iter()
                .all(|value| *value == original)
        );
        island.clear_settlement_ground(&[feature]).unwrap();
        assert_eq!(island.surface_weights_for(island.terrain.mesh()), values);
    }

    #[test]
    fn settlement_wear_leaves_underwater_ground_and_tile_edges_alone() {
        let mut areas = SpatialBins::new();
        let centre = Vec2::new(1.0, 500.0);
        areas.insert(
            centre - 5.0,
            centre + 5.0,
            WearArea {
                centre,
                half: Vec2::ONE,
                axis: Vec2::X,
            },
        );
        for point in [centre.extend(-1.0), Vec3::new(0.0, centre.y, 1.0)] {
            assert!(vertex_wear(point / ISLAND_WORLD_METRES, &areas, 7).abs() < f32::EPSILON);
        }
    }
}
