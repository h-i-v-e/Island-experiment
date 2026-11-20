//! Capital waterfront fitting against the final resident terrain.

use super::{Island, town_footpaths::PierAccess};
use crate::{
    DecorationClearance, ISLAND_WORLD_METRES, Vec2,
    piers::{PierLayout, PierMeshes, build_pier_meshes, plan_pier},
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
        let landing = layout.start.truncate();
        let clearance = DecorationClearance {
            centre: landing.midpoint(layout.shore).to_array(),
            half_extents: [
                landing.distance(layout.shore) * 0.5 + 2.,
                layout.width * 0.5 + 1.,
            ],
            yaw: layout.axis.to_angle(),
        };
        self.clear_decorations(&[clearance])?;
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

    fn fit_capital_pier(
        &self,
        centre: Vec2,
        mouth: Option<Vec2>,
        blockers: &[DecorationClearance],
    ) -> Option<(PierLayout, PierMeshes, Option<PierAccess>)> {
        let terrain = self.terrain();
        let sample = |point: Vec2| {
            terrain
                .sample_accurate_surface(point / ISLAND_WORLD_METRES)
                .map(|hit| hit.position.z * ISLAND_WORLD_METRES)
        };
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
        let first = plan_pier(centre, 0., bounds, &sample, &usable)?;
        let (layout, access) = self.connect_pier_landing(first, blockers, |tried| {
            plan_pier(centre, 0., bounds, &sample, &|point| {
                usable(point)
                    && !tried
                        .iter()
                        .any(|shore| point.distance_squared(*shore) < 5. * 5.)
            })
        });
        let meshes = build_pier_meshes(&layout, &sample)?;
        Some((layout, meshes, access))
    }

    fn connect_pier_landing(
        &self,
        first: PierLayout,
        blockers: &[DecorationClearance],
        next: impl Fn(&[Vec2]) -> Option<PierLayout>,
    ) -> (PierLayout, Option<PierAccess>) {
        let access = self.find_pier_access(first.start.truncate(), blockers);
        if access.is_some() || !self.terrain().has_road_coverage() {
            return (first, access);
        }

        // If the first landing is cut off from the streets, try nearby
        // alternatives without repeating the path search for the same landing.
        let mut tried = vec![first.shore];
        for _ in 0..3 {
            let Some(layout) = next(&tried) else { break };
            if let Some(access) = self.find_pier_access(layout.start.truncate(), blockers) {
                return (layout, Some(access));
            }
            tried.push(layout.shore);
        }
        (first, None)
    }
}
