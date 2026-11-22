//! Capital waterfront fitting against the final resident terrain.

use super::{Island, town_footpaths::PierAccess};
use crate::{
    DecorationClearance, ISLAND_WORLD_METRES, Vec2,
    piers::{PierLayout, PierMeshes, build_pier_meshes, survey_piers},
};

pub(crate) struct CapitalPier {
    pub(crate) layout: PierLayout,
    pub(crate) meshes: PierMeshes,
    pub(crate) clearance: DecorationClearance,
    pub(crate) access_connected: bool,
}

impl Island {
    pub(crate) fn prepare_capital_pier(
        &mut self,
        centre: Vec2,
        mouth: Option<Vec2>,
        blockers: &[DecorationClearance],
    ) -> Result<Option<CapitalPier>, String> {
        let Some((layout, meshes, access)) = self.fit_capital_pier(centre, mouth, blockers) else {
            return Ok(None);
        };
        let clearance = layout.land_clearance();
        self.clear_settlement_ground(&[clearance])?;
        let access_connected = access.is_some();
        if let Some(access) = access {
            self.install_pier_access(access);
        }
        Ok(Some(CapitalPier {
            layout,
            meshes,
            clearance,
            access_connected,
        }))
    }

    pub(crate) fn survey_capital_piers(
        &self,
        centre: Vec2,
        mouth: Option<Vec2>,
        blockers: &[DecorationClearance],
        limit: usize,
    ) -> Vec<PierLayout> {
        let protected = self.town_protected_terrain();
        let outlets: Vec<_> = self
            .rivers()
            .iter()
            .filter(|river| river.join.is_none())
            .filter_map(|river| river.nodes.last())
            .map(|node| node.position.truncate() * ISLAND_WORLD_METRES)
            .chain(mouth)
            .collect();
        let usable = |point: Vec2| {
            !outlets
                .iter()
                .any(|p| p.distance_squared(point) < 12. * 12.)
                && !blockers
                    .iter()
                    .any(|area| area.intersects_circle(point, 3.))
                && !protected.iter().any(|area| {
                    point.cmpge(Vec2::from_array(area.min) - 2.5).all()
                        && point.cmple(Vec2::from_array(area.max) + 2.5).all()
                })
        };
        let bounds = [Vec2::ZERO, Vec2::splat(ISLAND_WORLD_METRES)];
        survey_piers(
            centre,
            0.,
            bounds,
            &|point| self.pier_height(point),
            &usable,
            limit,
        )
    }

    fn fit_capital_pier(
        &self,
        centre: Vec2,
        mouth: Option<Vec2>,
        blockers: &[DecorationClearance],
    ) -> Option<(PierLayout, PierMeshes, Option<PierAccess>)> {
        let has_roads = self.terrain().has_road_coverage();
        let layouts =
            self.survey_capital_piers(centre, mouth, blockers, if has_roads { 4 } else { 1 });
        let &first = layouts.first()?;
        let (layout, access) = layouts
            .into_iter()
            .find_map(|layout| {
                Some((
                    layout,
                    Some(self.find_pier_access(layout.start.truncate(), blockers)?),
                ))
            })
            .unwrap_or((first, None));
        let meshes = build_pier_meshes(&layout, &|point| self.pier_height(point))?;
        Some((layout, meshes, access))
    }

    pub(crate) fn paved_capital_pier(&self, layout: PierLayout) -> Option<CapitalPier> {
        let landing = layout.start.truncate();
        if !has_paved_landing(self.terrain(), landing) {
            return None;
        }
        let meshes = build_pier_meshes(&layout, &|point| self.pier_height(point))?;
        Some(CapitalPier {
            clearance: layout.land_clearance(),
            layout,
            meshes,
            access_connected: true,
        })
    }

    fn pier_height(&self, point: Vec2) -> Option<f32> {
        self.terrain()
            .sample_accurate_surface(point / ISLAND_WORLD_METRES)
            .map(|hit| hit.position.z * ISLAND_WORLD_METRES)
    }
}

fn has_paved_landing(terrain: &super::Terrain, landing: Vec2) -> bool {
    // Road paint fades to dirt at an open end. Check the physical pavement
    // with a small tolerance for rounding at the shared landing instead.
    const LANDING_TOLERANCE: f32 = 0.01;
    terrain.paved_road_overlaps(landing - LANDING_TOLERANCE, landing + LANDING_TOLERANCE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ForestOptions,
        roads::{RoadNetwork, RoadStation, RoadStrip},
    };

    #[test]
    fn open_pavement_edge_accepts_a_pier_despite_material_falloff() {
        let mut island = Island::shared_forest_fixture(ForestOptions::default());
        let landing = Vec2::new(675., 665.);
        let start = Vec2::new(645., 650.);
        let station = |point| RoadStation { point, height: 39. };
        island
            .apply_town_roads(&RoadNetwork {
                strips: vec![RoadStrip {
                    source: 0,
                    start: 0,
                    end: 1,
                    half_width: 2.,
                    stations: vec![station(start), station(landing)],
                }],
                junctions: Vec::new(),
                shoulder: 2.,
            })
            .unwrap();
        let terrain = island.terrain();
        let forward = (landing - start).normalize();
        let on_join = landing + forward * 0.002;
        assert!(terrain.road_coverage_at(on_join / ISLAND_WORLD_METRES) < 0.5);
        assert!(has_paved_landing(terrain, on_join));
        assert!(!has_paved_landing(terrain, landing + forward * 0.25));
    }
}
