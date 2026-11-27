//! Terrain-following links between outward settlement road terminals, in native XY metres.
mod search;
mod surface;
#[cfg(test)]
mod tests;

use super::{Island, town::DecorationClearance};
pub(crate) use crate::bridges::BridgeKind as Crossing;
use crate::bridges::{BridgeChannel, BridgeRequest, BridgeSurface};
use crate::{Mesh, Vec2, Vec3};
use search::{Entrance, Graph, Route};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use surface::{BridgeFootprint, Surface};

pub(crate) type CartTrackBlocker = DecorationClearance;
const HALF_WIDTH: f32 = 0.9;
const WHEEL_OFFSET: f32 = 0.6;
const BRIDGE_WIDTH: f32 = 2.4;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct CartTrackExit {
    pub(crate) settlement_index: usize,
    pub(crate) position: [f32; 3],
    pub(crate) direction: [f32; 2],
    pub(crate) half_width: f32,
    #[serde(default)]
    pub(crate) curb: bool,
}

impl CartTrackExit {
    fn xy(&self) -> Vec2 {
        Vec3::from_array(self.position).truncate()
    }
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct CartTrackPlan {
    #[serde(skip)]
    pub(crate) runs: Vec<CartTrackRun>,
    pub(crate) bridges: Vec<CartTrackBridge>,
    pub(crate) links: Vec<CartTrackLink>,
    pub(crate) diagnostics: Vec<CartTrackDiagnostic>,
    #[serde(skip)]
    bridge_footprints: Vec<BridgeFootprint>,
}

impl CartTrackPlan {
    pub(super) fn bridge_clearance(&self) -> impl Iterator<Item = DecorationClearance> + '_ {
        self.bridge_footprints
            .iter()
            .flat_map(|bridge| bridge.sections.iter().copied())
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct CartTrackRun {
    pub(crate) link_index: usize,
    pub(crate) points: Vec<[f32; 3]>,
    #[serde(skip)]
    pub(crate) joins_track: bool,
}

fn cart_bridge_request(
    start: [f32; 3],
    end: [f32; 3],
    width: f32,
    kind: Crossing,
) -> BridgeRequest {
    BridgeRequest {
        start,
        end,
        start_heading: None,
        end_heading: None,
        width,
        kind,
        surface: BridgeSurface::CartTrack,
        channel: None,
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct CartTrackBridge {
    pub(crate) link_index: usize,
    pub(crate) start: [f32; 3],
    pub(crate) end: [f32; 3],
    pub(crate) width: f32,
    pub(crate) start_heading: Option<[f32; 2]>,
    pub(crate) end_heading: Option<[f32; 2]>,
    pub(crate) kind: Crossing,
}

impl CartTrackBridge {
    pub(crate) fn request(&self, channel: Option<BridgeChannel>) -> BridgeRequest {
        BridgeRequest {
            start_heading: self.start_heading,
            end_heading: self.end_heading,
            channel,
            ..cart_bridge_request(self.start, self.end, self.width, self.kind)
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct CartTrackLink {
    pub(crate) from_settlement: usize,
    pub(crate) to_settlement: usize,
    pub(crate) from_exit: usize,
    pub(crate) to_exit: usize,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) from_track: bool,
    #[serde(skip)]
    pub(super) parent_run: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CartTrackDiagnostic {
    pub(crate) settlement_index: usize,
    pub(crate) reason: &'static str,
}

pub(crate) struct CartTrackPlanner<'a> {
    mesh: &'a Mesh,
    surface: Surface<'a>,
}

impl<'a> CartTrackPlanner<'a> {
    pub(crate) fn new(island: &'a Island, blockers: &[CartTrackBlocker]) -> Self {
        Self {
            mesh: island
                .terrain
                .cart_track_routing_mesh
                .as_ref()
                .unwrap_or(&island.coarser_lods[0]),
            surface: Surface::new(island, blockers),
        }
    }

    pub(crate) fn portal_approach(&self, boundary: Vec2, inward: Vec2) -> Option<Vec<[f32; 3]>> {
        let end = boundary + inward.try_normalize()? * 8.;
        self.surface.dry_cost(boundary, end)?;
        self.ground_points(&[boundary, end])
    }

    pub(super) fn clear_sign_ground(&self, point: Vec2, footprint_radius: f32) -> bool {
        self.surface.clear_sign_ground(point, footprint_radius)
    }

    pub(crate) fn plan(
        &self,
        exits: &[CartTrackExit],
        capital: Option<usize>,
        centres: &[Option<Vec2>],
        first_portal: usize,
    ) -> CartTrackPlan {
        let graph = Graph::new(self.mesh, &self.surface);
        let entrances: Vec<_> = exits.iter().map(|exit| graph.entrance(exit)).collect();
        let mut sites = BTreeMap::<usize, Site>::new();
        for (index, exit) in exits.iter().enumerate() {
            sites
                .entry(exit.settlement_index)
                .or_default()
                .exits
                .push(index);
        }
        for (&id, site) in &mut sites {
            site.centre = centres.get(id).copied().flatten().unwrap_or_else(|| {
                site.exits.iter().map(|&i| exits[i].xy()).sum::<Vec2>() / site.exits.len() as f32
            });
        }

        let mut network = Network {
            planner: self,
            exits,
            entrances: &entrances,
            sites: &sites,
            graph,
            result: CartTrackPlan::default(),
            origin: capital
                .and_then(|id| sites.get(&id).map(|site| site.centre))
                .or_else(|| exits.get(first_portal).map(CartTrackExit::xy))
                .unwrap_or_default(),
        };
        let (mut villages, portals): (Vec<_>, Vec<_>) = sites
            .keys()
            .copied()
            .filter(|id| Some(*id) != capital)
            .partition(|id| sites[id].exits[0] < first_portal);
        let mut connected = Vec::new();
        if let Some(capital) = capital.filter(|id| sites.contains_key(id)) {
            connected.push(capital);
            network.order_by_distance(&mut villages, sites[&capital].centre);
            // A later village may provide the first viable approach to a site
            // skipped earlier. Retry only the unconnected targets.
            loop {
                let before = villages.len();
                villages.retain(|&village| {
                    if network.join(village, &connected) {
                        connected.push(village);
                        false
                    } else {
                        true
                    }
                });
                if villages.is_empty() || villages.len() == before {
                    break;
                }
            }
        } else if first_portal == 0 {
            // A section without town exits can still carry a through track between seams.
            connected.extend(portals.first().copied());
        }

        // Append seam routes after the settlement tree. A portal can never
        // become the starting point for a new village branch.
        for portal in portals {
            if !connected.contains(&portal) && network.join(portal, &connected) {
                connected.push(portal);
            }
        }
        for (&id, site) in &sites {
            let reason = if !site.exits.iter().any(|&i| entrances[i].usable()) {
                "no_dry_outward_approach"
            } else if !connected.contains(&id) {
                "disconnected_settlement_component"
            } else {
                continue;
            };
            network.result.diagnostics.push(CartTrackDiagnostic {
                settlement_index: id,
                reason,
            });
        }
        network.result
    }

    fn ground_points(&self, polyline: &[Vec2]) -> Option<Vec<[f32; 3]>> {
        let &first = polyline.first()?;
        let mut points = vec![first.extend(self.surface.height(first)?).to_array()];
        for edge in polyline.windows(2) {
            let count = (edge[0].distance(edge[1]) / 2.).ceil() as usize;
            for i in 1..count {
                let p = edge[0].lerp(edge[1], i as f32 / count as f32);
                points.push(p.extend(self.surface.height(p)?).to_array());
            }
            if count > 0 {
                let p = edge[1];
                points.push(p.extend(self.surface.height(p)?).to_array());
            }
        }
        Some(points)
    }

    fn finish_run(&self, route: &[Vec2], link_index: usize) -> Option<CartTrackRun> {
        let mut path = Vec::with_capacity(route.len() * 2);
        path.push(*route.first()?);
        for window in route.windows(3) {
            let [a, b, c] = [window[0], window[1], window[2]];
            let before = b.lerp(a, 0.28);
            let after = b.lerp(c, 0.28);
            let middle = (before + after + 2. * b) * 0.25;
            let previous = *path.last()?;
            // A short quadratic through the bend. Recheck each emitted chord;
            // if a wheel would leave safe terrain retain the verified mesh turn.
            if [
                (previous, before),
                (before, middle),
                (middle, after),
                (after, c),
            ]
            .into_iter()
            .all(|(p, q)| self.surface.dry_cost(p, q).is_some())
            {
                path.extend([before, middle, after]);
            } else {
                path.push(b);
            }
        }
        path.push(*route.last()?);
        Some(CartTrackRun {
            link_index,
            points: self.ground_points(&path)?,
            joins_track: false,
        })
    }

    fn align_landing(
        &self,
        run: &mut CartTrackRun,
        at_start: bool,
        away: Vec2,
    ) -> Option<[f32; 2]> {
        let points = &mut run.points;
        if points.len() < 2 {
            return None;
        }
        let insertion = if at_start { 1 } else { points.len() - 1 };
        let [bank, adjacent] = if at_start {
            [points[0], points[1]]
        } else {
            [points[insertion], points[insertion - 1]]
        }
        .map(|point| Vec3::from_array(point).truncate());
        let travel = (adjacent - bank).try_normalize()?;
        if travel.distance_squared(away) > 1e-6 {
            // Put a short tangent on the bank so both wheel ruts meet the deck.
            let maximum = 0.75 * bank.distance(adjacent);
            let lead = [1.5_f32, 0.75, 0.35].into_iter().find_map(|distance| {
                let lead = bank + away * distance.min(maximum);
                self.surface.dry_cost(bank, lead)?;
                self.surface.dry_cost(lead, adjacent)?;
                Some(lead.extend(self.surface.height(lead)?).to_array())
            })?;
            points.insert(insertion, lead);
        }
        let pair = if at_start {
            &points[..2]
        } else {
            &points[points.len() - 2..]
        };
        heading(pair)
    }

    fn geometry(
        &self,
        route: &Route,
        link_index: usize,
    ) -> Option<(Vec<CartTrackRun>, Vec<CartTrackBridge>)> {
        let mut runs = Vec::new();
        let mut run = Vec::new();
        for &(p, crossing) in &route.stations {
            if crossing.is_some() {
                runs.push(self.finish_run(&run, link_index)?);
                run.clear();
            }
            if run.last() != Some(&p) {
                run.push(p);
            }
        }
        runs.push(self.finish_run(&run, link_index)?);
        let bridges = route
            .crossings()
            .zip(runs.windows(2))
            .map(|(crossing, banks)| {
                let before = &banks[0].points;
                let after = &banks[1].points;
                Some(CartTrackBridge {
                    link_index,
                    start: *before.last()?,
                    end: *after.first()?,
                    width: BRIDGE_WIDTH,
                    start_heading: None,
                    end_heading: None,
                    kind: crossing.kind,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some((runs, bridges))
    }
}

fn heading(points: &[[f32; 3]]) -> Option<[f32; 2]> {
    let delta = Vec3::from_array(points[1]) - Vec3::from_array(points[0]);
    delta.truncate().try_normalize().map(|v| v.to_array())
}

fn ordered(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

#[derive(Default)]
struct Site {
    centre: Vec2,
    exits: Vec<usize>,
}

struct Network<'a, 'b> {
    planner: &'a CartTrackPlanner<'b>,
    exits: &'a [CartTrackExit],
    entrances: &'a [Entrance],
    sites: &'a BTreeMap<usize, Site>,
    graph: Graph<'a>,
    result: CartTrackPlan,
    origin: Vec2,
}

impl Network<'_, '_> {
    fn order_by_distance(&self, sites: &mut [usize], target: Vec2) {
        // Stable ties preserve settlement order.
        sites.sort_by(|a, b| {
            self.sites[a]
                .centre
                .distance_squared(target)
                .total_cmp(&self.sites[b].centre.distance_squared(target))
        });
    }

    fn join(&mut self, site: usize, connected: &[usize]) -> bool {
        if !self.sites[&site]
            .exits
            .iter()
            .any(|&i| self.entrances[i].usable())
        {
            return false;
        }
        let sources: Vec<_> = connected
            .iter()
            .flat_map(|id| self.sites[id].exits.iter().copied())
            .collect();
        // A span may fit without room for a dry approach on both banks.
        // Retry without that directed edge for this connection.
        let mut rejected = BTreeSet::new();
        let (route, mut runs, mut bridges, mut footprints) = 'search: loop {
            let Some(route) = self.graph.route(
                &sources,
                &self.sites[&site].exits,
                self.exits,
                self.entrances,
                &rejected,
                self.origin,
            ) else {
                return false;
            };
            let Some((mut runs, mut bridges)) =
                self.planner.geometry(&route, self.result.links.len())
            else {
                return false;
            };
            let mut footprints = Vec::new();
            for (index, (bridge, crossing)) in bridges.iter_mut().zip(route.crossings()).enumerate()
            {
                let Some(layout) = self.planner.surface.bridge_layout(bridge.request(None)) else {
                    rejected.insert(crossing.nodes);
                    continue 'search;
                };
                let (a, b) = crossing.nodes;
                let footprint = BridgeFootprint::new(ordered(a, b), &layout);
                let reserved = || self.result.bridge_footprints.iter().chain(&footprints);
                if reserved().any(|other| footprint.conflicts(other)) {
                    rejected.insert(crossing.nodes);
                    continue 'search;
                }
                let (before, after) = runs.split_at_mut(index + 1);
                let start = self
                    .planner
                    .align_landing(&mut before[index], false, -layout.axis);
                let end = self.planner.align_landing(&mut after[0], true, layout.axis);
                let (Some(start), Some(end)) = (start, end) else {
                    rejected.insert(crossing.nodes);
                    continue 'search;
                };
                bridge.start_heading = Some(start);
                bridge.end_heading = Some(end);
                if !reserved().any(|other| other.nodes == footprint.nodes) {
                    footprints.push(footprint);
                }
            }
            runs.retain(|run| run.points.len() >= 2);
            break (route, runs, bridges, footprints);
        };
        if route.parent_run.is_some()
            && let Some(first) = runs.first_mut()
        {
            first.joins_track = true;
        }
        for (i, run) in runs.iter().enumerate() {
            self.graph
                .register_track(run, route.target, self.result.runs.len() + i);
        }
        self.result.links.push(CartTrackLink {
            from_settlement: self.exits[route.source].settlement_index,
            to_settlement: site,
            from_exit: route.source,
            to_exit: route.target,
            from_track: route.parent_run.is_some(),
            parent_run: route.parent_run,
        });
        self.result.runs.append(&mut runs);
        self.result.bridges.append(&mut bridges);
        self.result.bridge_footprints.append(&mut footprints);
        true
    }
}
