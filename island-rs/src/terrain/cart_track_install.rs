use std::sync::Arc;
#[cfg(feature = "surface-atlas")]
use std::sync::OnceLock;

use serde::{Serialize, Serializer};

use super::{
    Island, Terrain,
    cart_track_signs::{CartTrackDeparture, cart_track_departures},
    cart_track_surface::CartTrackSurface,
    cart_tracks::{CartTrackBlocker, CartTrackExit, CartTrackPlan, CartTrackPlanner, CartTrackRun},
    town::DecorationClearance,
};
use crate::{Mesh, Vec2};

#[derive(Clone, Debug)]
pub(super) struct InstalledCartTracks {
    surface: CartTrackSurface,
    runs: Arc<[CartTrackRun]>,
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
    #[serde(serialize_with = "serialize_runs")]
    runs: Arc<[CartTrackRun]>,
    run_count: usize,
    portals: Vec<CartTrackExit>,
    departures: Vec<CartTrackDeparture>,
}

fn serialize_runs<S: Serializer>(runs: &[CartTrackRun], serializer: S) -> Result<S::Ok, S::Error> {
    runs.serialize(serializer)
}

impl Terrain {
    pub(crate) fn has_cart_tracks(&self) -> bool {
        self.cart_tracks.is_some()
    }

    pub(crate) fn cart_track_runs(&self) -> &[CartTrackRun] {
        self.cart_tracks.as_ref().map_or(&[], |tracks| &tracks.runs)
    }

    pub(crate) fn cart_track_coverage(&self, xy: Vec2) -> f32 {
        self.cart_tracks
            .as_ref()
            .map_or(0., |tracks| tracks.surface.coverage_at(xy))
    }

    pub(super) fn cart_track_corridor_overlaps(&self, min: Vec2, max: Vec2) -> bool {
        self.cart_tracks
            .as_ref()
            .is_some_and(|tracks| tracks.surface.corridor_overlaps(min, max))
    }

    pub(super) fn cart_track_height(&self, xy: Vec2, height: f32) -> f32 {
        self.cart_tracks.as_ref().map_or(height, |tracks| {
            height - tracks.surface.rut_cut(self, xy, height)
        })
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
        capital: Option<usize>,
        centres: &[Option<Vec2>],
    ) -> Result<CartTrackReport, String> {
        if self.terrain.has_cart_tracks() {
            return Err("cart tracks have already been prepared on this island".into());
        }
        let mut inputs = exits.to_vec();
        let mut approaches = Vec::new();
        let mut portal_sides = Vec::new();
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
                    curb: false,
                });
                approaches.push(points);
                portal_sides.push(i32::from(side != 0));
            }
        }
        let mut plan = if !inputs.is_empty() {
            planner.plan(&inputs, capital, centres, exits.len())
        } else {
            CartTrackPlan::default()
        };
        for (link_index, link) in plan.links.iter().enumerate() {
            let mut runs = plan
                .runs
                .iter_mut()
                .filter(|run| run.link_index == link_index);
            let Some(first) = runs.next() else { continue };
            if !link.from_track
                && let Some(approach) = link
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
        let surface = CartTrackSurface::new(&plan.runs, &plan.bridges);
        let departures =
            cart_track_departures(self, &planner, &plan, &surface, &inputs, &portal_sides);
        let clearance = cart_track_clearance(&plan);
        let runs = Arc::from(std::mem::take(&mut plan.runs));
        if !surface.is_empty() {
            let detail = surface.rutted_mesh(&self.terrain)?;
            for batch in clearance.chunks(4096) {
                self.clear_settlement_ground(batch)?;
            }
            self.terrain.cart_tracks = Some(InstalledCartTracks {
                surface,
                runs: Arc::clone(&runs),
                detail,
                #[cfg(feature = "surface-atlas")]
                atlas: Default::default(),
            });
        }
        Ok(CartTrackReport {
            run_count: runs.len(),
            runs,
            plan,
            portals: inputs.split_off(exits.len()),
            departures,
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
    result.extend(plan.bridge_clearance());
    result
}
