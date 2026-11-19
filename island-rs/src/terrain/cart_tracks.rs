//! Terrain-following links between outward settlement road terminals, in native XY metres.
mod search;
mod surface;
#[cfg(test)]
mod tests;

use super::{Island, town::DecorationClearance};
use crate::{Mesh, Vec2, Vec3};
use search::{Entrance, Graph, Route};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use surface::Surface;

pub(crate) type CartTrackBlocker = DecorationClearance;
const HALF_WIDTH: f32 = 0.9;
const WHEEL_OFFSET: f32 = 0.6;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct CartTrackExit {
    pub(crate) settlement_index: usize,
    pub(crate) position: [f32; 3],
    pub(crate) direction: [f32; 2],
    pub(crate) half_width: f32,
}

impl CartTrackExit {
    fn xy(&self) -> Vec2 {
        Vec3::from_array(self.position).truncate()
    }
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct CartTrackPlan {
    pub(crate) runs: Vec<CartTrackRun>,
    pub(crate) bridges: Vec<CartTrackBridge>,
    pub(crate) links: Vec<CartTrackLink>,
    pub(crate) diagnostics: Vec<CartTrackDiagnostic>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CartTrackRun {
    pub(crate) link_index: usize,
    pub(crate) points: Vec<[f32; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Crossing {
    River,
    Ravine,
}

#[derive(Debug, Serialize)]
pub(crate) struct CartTrackBridge {
    pub(crate) link_index: usize,
    pub(crate) start: [f32; 3],
    pub(crate) end: [f32; 3],
    pub(crate) kind: Crossing,
}

#[derive(Debug, Serialize)]
pub(crate) struct CartTrackLink {
    pub(crate) from_settlement: usize,
    pub(crate) to_settlement: usize,
    pub(crate) from_exit: usize,
    pub(crate) to_exit: usize,
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
            mesh: &island.coarser_lods[0],
            surface: Surface::new(island, blockers),
        }
    }

    pub(crate) fn portal_approach(&self, boundary: Vec2, inward: Vec2) -> Option<Vec<[f32; 3]>> {
        let end = boundary + inward.try_normalize()? * 8.;
        self.surface.dry_cost(boundary, end)?;
        self.ground_points(&[boundary, end])
    }

    pub(crate) fn plan(&self, exits: &[CartTrackExit]) -> CartTrackPlan {
        let graph = Graph::new(self.mesh, &self.surface);
        let entrances = exits
            .iter()
            .map(|exit| graph.entrance(exit))
            .collect::<Vec<_>>();
        let mut sites = BTreeMap::<usize, Vec<usize>>::new();
        for (i, exit) in exits.iter().enumerate() {
            sites.entry(exit.settlement_index).or_default().push(i);
        }
        let mut network = Network {
            planner: self,
            exits,
            entrances: &entrances,
            sites: &sites,
            graph,
            result: CartTrackPlan::default(),
            pairs: BTreeSet::new(),
            attempted: BTreeSet::new(),
        };
        for (source, exit) in exits.iter().enumerate() {
            if !entrances[source].usable()
                || network
                    .result
                    .links
                    .iter()
                    .any(|link| link.to_exit == source)
            {
                continue;
            }
            for (_, _, site) in ranked_sites(exit, exits, &sites) {
                if network
                    .pairs
                    .contains(&ordered(exit.settlement_index, site))
                    || network.try_link(source, site)
                {
                    break;
                }
            }
        }
        // Directional choices can form separate groups. Try the nearest unjoined
        // pairs, including alternate terminals, until every feasible group is joined.
        let mut candidates = Vec::new();
        for (source, exit) in exits
            .iter()
            .enumerate()
            .filter(|(i, _)| entrances[*i].usable())
        {
            for (_, distance, site) in ranked_sites(exit, exits, &sites) {
                candidates.push((distance, source, site));
            }
        }
        candidates.sort_unstable_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        for (_, source, site) in candidates {
            if !network.connected(exits[source].settlement_index, site) {
                network.try_link(source, site);
            }
        }
        for (&site, indices) in &sites {
            let unjoined = sites.keys().any(|other| !network.connected(site, *other));
            let reason = if !indices.iter().any(|&i| entrances[i].usable()) {
                Some("no_dry_outward_approach")
            } else if unjoined {
                Some("disconnected_settlement_component")
            } else {
                None
            };
            if let Some(reason) = reason {
                network.result.diagnostics.push(CartTrackDiagnostic {
                    settlement_index: site,
                    reason,
                });
            }
        }
        network.result
    }

    fn ground_points(&self, polyline: &[Vec2]) -> Option<Vec<[f32; 3]>> {
        let &first = polyline.first()?;
        let mut points = vec![first.extend(self.surface.height(first)?).to_array()];
        for edge in polyline.windows(2) {
            let count = (edge[0].distance(edge[1]) / 2.).ceil() as usize;
            for i in 1..=count {
                let p = edge[0].lerp(edge[1], i as f32 / count as f32);
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
        })
    }

    fn geometry(
        &self,
        route: &Route,
        link_index: usize,
    ) -> Option<(Vec<CartTrackRun>, Vec<CartTrackBridge>)> {
        let mut runs = Vec::new();
        let mut bridges = Vec::new();
        let mut run = Vec::new();
        for &(p, crossing) in &route.stations {
            if let Some(kind) = crossing {
                let a = *run.last()?;
                runs.push(self.finish_run(&run, link_index)?);
                let start = a.extend(self.surface.height(a)?).to_array();
                let end = p.extend(self.surface.height(p)?).to_array();
                bridges.push(CartTrackBridge {
                    link_index,
                    start,
                    end,
                    kind,
                });
                run.clear();
            }
            if run.last() != Some(&p) {
                run.push(p);
            }
        }
        runs.push(self.finish_run(&run, link_index)?);
        runs.retain(|r| r.points.len() >= 2);
        Some((runs, bridges))
    }
}

fn ordered(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

fn ranked_sites(
    exit: &CartTrackExit,
    exits: &[CartTrackExit],
    sites: &BTreeMap<usize, Vec<usize>>,
) -> Vec<(bool, f32, usize)> {
    let direction = Vec2::from_array(exit.direction);
    let mut ranks = sites
        .iter()
        .filter(|(id, _)| **id != exit.settlement_index)
        .map(|(&site, indices)| {
            let centre =
                indices.iter().map(|&i| exits[i].xy()).sum::<Vec2>() / indices.len() as f32;
            let delta = centre - exit.xy();
            (direction.dot(delta) <= 0., delta.length_squared(), site)
        })
        .collect::<Vec<_>>();
    ranks.sort_unstable_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.total_cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    ranks
}

struct Network<'a, 'b> {
    planner: &'a CartTrackPlanner<'b>,
    exits: &'a [CartTrackExit],
    entrances: &'a [Entrance],
    sites: &'a BTreeMap<usize, Vec<usize>>,
    graph: Graph<'a>,
    result: CartTrackPlan,
    pairs: BTreeSet<(usize, usize)>,
    attempted: BTreeSet<(usize, usize)>,
}

impl Network<'_, '_> {
    fn try_link(&mut self, source: usize, site: usize) -> bool {
        if !self.attempted.insert((source, site)) {
            return false;
        }
        let Some(route) = self
            .graph
            .route(source, &self.sites[&site], self.exits, self.entrances)
        else {
            return false;
        };
        let Some((mut runs, mut bridges)) = self.planner.geometry(&route, self.result.links.len())
        else {
            return false;
        };
        let origin = self.exits[source].settlement_index;
        self.pairs.insert(ordered(origin, site));
        self.result.links.push(CartTrackLink {
            from_settlement: origin,
            to_settlement: site,
            from_exit: source,
            to_exit: route.target,
        });
        self.result.runs.append(&mut runs);
        self.result.bridges.append(&mut bridges);
        true
    }

    fn connected(&self, from: usize, to: usize) -> bool {
        let mut visited = BTreeSet::from([from]);
        let mut queue = vec![from];
        while let Some(site) = queue.pop() {
            if site == to {
                return true;
            }
            for &(a, b) in &self.pairs {
                let next = if a == site {
                    b
                } else if b == site {
                    a
                } else {
                    continue;
                };
                if visited.insert(next) {
                    queue.push(next);
                }
            }
        }
        false
    }
}
