#[cfg(feature = "surface-atlas")]
use std::sync::OnceLock;

use serde::Serialize;

use super::{
    Island, Terrain,
    cart_track_surface::CartTrackSurface,
    cart_tracks::{CartTrackBlocker, CartTrackExit, CartTrackPlan, CartTrackPlanner},
    town::DecorationClearance,
};
use crate::{Mesh, Vec2};

#[derive(Clone, Debug)]
pub(super) struct InstalledCartTracks {
    surface: CartTrackSurface,
    detail: Option<Mesh>,
    #[cfg(feature = "surface-atlas")]
    atlas: [OnceLock<Option<Mesh>>; 2],
}

impl PartialEq for InstalledCartTracks {
    fn eq(&self, other: &Self) -> bool {
        self.surface == other.surface
    }
}

#[derive(Serialize)]
pub(crate) struct CartTrackReport {
    #[serde(flatten)]
    plan: CartTrackPlan,
    run_count: usize,
    portals: Vec<CartTrackExit>,
}

impl Terrain {
    pub(crate) fn has_cart_tracks(&self) -> bool {
        self.cart_tracks.is_some()
    }

    pub(crate) fn cart_track_coverage(&self, xy: Vec2) -> f32 {
        self.cart_tracks
            .as_ref()
            .map_or(0., |tracks| tracks.surface.coverage_at(xy))
    }

    pub(super) fn cart_track_detail_mesh(&self) -> &Mesh {
        self.cart_tracks
            .as_ref()
            .and_then(|tracks| tracks.detail.as_ref())
            .unwrap_or(self.mesh())
    }

    #[cfg(feature = "surface-atlas")]
    pub(super) fn cart_track_atlas_mesh<'a>(&'a self, lod: usize, mesh: &'a Mesh) -> &'a Mesh {
        let Some(tracks) = &self.cart_tracks else {
            return mesh;
        };
        tracks.atlas[lod - 1]
            .get_or_init(|| match tracks.surface.refined_mesh(mesh) {
                Ok(result) => result.map(|(refined, _)| refined),
                Err(reason) => {
                    eprintln!("Cart-track render refinement at LOD {lod}: {reason}");
                    None
                }
            })
            .as_ref()
            .unwrap_or(mesh)
    }
}

impl Island {
    pub(crate) fn prepare_cart_tracks(
        &mut self,
        exits: &[CartTrackExit],
        blockers: &[CartTrackBlocker],
        connect_coastal_seams: bool,
        settlement_count: usize,
    ) -> Result<CartTrackReport, String> {
        if self.terrain.has_cart_tracks() {
            return Err("cart tracks have already been prepared on this island".into());
        }
        let mut inputs = exits.to_vec();
        let mut approaches = Vec::new();
        let planner = CartTrackPlanner::new(self, blockers);
        if connect_coastal_seams {
            let next_settlement = exits
                .iter()
                .map(|exit| exit.settlement_index + 1)
                .max()
                .unwrap_or(0)
                .max(settlement_count);
            for (side, boundary) in self.coastal_track_portals().into_iter().enumerate() {
                let inward = if side == 0 { Vec2::X } else { Vec2::NEG_X };
                let Some(points) =
                    boundary.and_then(|point| planner.portal_approach(point, inward))
                else {
                    continue;
                };
                let Some(&position) = points.last() else {
                    continue;
                };
                inputs.push(CartTrackExit {
                    settlement_index: next_settlement + side,
                    position,
                    direction: inward.to_array(),
                    half_width: 0.9,
                });
                approaches.push(points);
            }
        }
        let mut plan = if inputs.len() > 1 {
            planner.plan(&inputs)
        } else {
            CartTrackPlan::default()
        };
        for (link_index, link) in plan.links.iter().enumerate() {
            let mut runs = plan
                .runs
                .iter_mut()
                .filter(|run| run.link_index == link_index);
            let Some(first) = runs.next() else { continue };
            if let Some(approach) = link
                .from_exit
                .checked_sub(exits.len())
                .and_then(|i| approaches.get(i))
            {
                first
                    .points
                    .splice(..0, approach[..approach.len() - 1].iter().copied());
            }
            if let Some(approach) = link
                .to_exit
                .checked_sub(exits.len())
                .and_then(|i| approaches.get(i))
            {
                let last = runs.last().unwrap_or(first);
                last.points
                    .extend(approach[..approach.len() - 1].iter().rev().copied());
            }
        }
        let surface = CartTrackSurface::new(plan.runs.iter().map(|run| run.points.as_slice()));
        if !surface.is_empty() {
            let detail = surface
                .refined_mesh(self.terrain.mesh())?
                .map(|(mesh, _)| mesh);
            let clearance = cart_track_clearance(&plan);
            for batch in clearance.chunks(4096) {
                self.clear_decorations(batch)?;
            }
            self.terrain.cart_tracks = Some(InstalledCartTracks {
                surface,
                detail,
                #[cfg(feature = "surface-atlas")]
                atlas: Default::default(),
            });
        }
        Ok(CartTrackReport {
            run_count: plan.runs.len(),
            plan,
            portals: inputs.split_off(exits.len()),
        })
    }
}

fn cart_track_clearance(plan: &CartTrackPlan) -> Vec<DecorationClearance> {
    const POINTS_PER_SPAN: usize = 6;
    const TRACK_CLEARANCE_METRES: f32 = 1.05;
    let mut result = Vec::new();
    for run in &plan.runs {
        for begin in (0..run.points.len().saturating_sub(1)).step_by(POINTS_PER_SPAN) {
            let end = (begin + POINTS_PER_SPAN).min(run.points.len() - 1);
            let xy = |point: [f32; 3]| Vec2::new(point[0], point[1]);
            let origin = xy(run.points[begin]);
            let tangent = (xy(run.points[end]) - origin).normalize_or(Vec2::X);
            let normal = tangent.perp();
            let (mut min, mut max) = (Vec2::ZERO, Vec2::ZERO);
            for &point in &run.points[begin + 1..=end] {
                let offset = xy(point) - origin;
                let local = Vec2::new(tangent.dot(offset), normal.dot(offset));
                min = min.min(local);
                max = max.max(local);
            }
            let centre = (min + max) * 0.5;
            result.push(DecorationClearance {
                centre: (origin + tangent * centre.x + normal * centre.y).to_array(),
                half_extents: ((max - min) * 0.5 + TRACK_CLEARANCE_METRES).to_array(),
                yaw: tangent.to_angle(),
            });
        }
    }
    result
}
