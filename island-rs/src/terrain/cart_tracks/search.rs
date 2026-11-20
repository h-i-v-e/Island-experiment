use super::{CartTrackExit, Crossing, Surface, surface::SpatialBins};
use crate::{Adjacency, ISLAND_WORLD_METRES, Mesh, Vec2, bridges::MIN_HEADING_DOT};
use std::{
    cmp::Ordering,
    collections::{BTreeSet, BinaryHeap, HashMap},
};

#[derive(Clone, Copy)]
struct Edge {
    to: usize,
    cost: f32,
    crossing: Option<Crossing>,
}

#[derive(Clone, Copy)]
struct Attachment {
    node: usize,
    cost: f32,
}

pub(super) struct Entrance {
    lead: Vec2,
    anchors: Vec<Attachment>,
}

impl Entrance {
    pub(super) fn usable(&self) -> bool {
        !self.anchors.is_empty()
    }
}

pub(super) struct Graph<'a> {
    surface: &'a Surface<'a>,
    positions: Vec<Vec2>,
    bins: SpatialBins<usize>,
    adjacency: Adjacency,
    edges: Vec<Option<Vec<Edge>>>,
    costs: HashMap<(usize, usize), Option<f32>>,
}

impl<'a> Graph<'a> {
    pub(super) fn new(mesh: &Mesh, surface: &'a Surface<'a>) -> Self {
        let positions: Vec<_> = mesh
            .vertices
            .iter()
            .map(|p| p.truncate() * ISLAND_WORLD_METRES)
            .collect();
        let mut bins = SpatialBins::new();
        for (i, &point) in positions.iter().enumerate() {
            bins.insert(point, point, i);
        }
        Self {
            edges: (0..positions.len()).map(|_| None).collect(),
            positions,
            bins,
            adjacency: mesh.adjacency(),
            surface,
            costs: HashMap::new(),
        }
    }

    pub(super) fn entrance(&self, exit: &CartTrackExit) -> Entrance {
        let origin = exit.xy();
        let dir = Vec2::from_array(exit.direction).normalize_or_zero();
        let mut entrance = Entrance {
            lead: origin,
            anchors: Vec::new(),
        };
        if dir == Vec2::ZERO || !origin.is_finite() {
            return entrance;
        }
        for distance in [3.0, 1.0] {
            let lead = origin + distance * dir;
            let Some(initial) = self.surface.dry_cost(origin, lead) else {
                continue;
            };
            entrance.lead = lead;
            let mut nearby = self
                .bins
                .nearby(lead - 36., lead + 36.)
                .filter_map(|&node| {
                    let delta = self.positions[node] - lead;
                    let dist = delta.length();
                    let bearing = dir.dot(delta.normalize_or_zero());
                    (dist <= 36. && bearing >= -0.15).then_some((node, dist + (1. - bearing) * 4.))
                })
                .collect::<Vec<_>>();
            nearby.sort_unstable_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            for (node, _) in nearby {
                let point = self.positions[node];
                if entrance
                    .anchors
                    .iter()
                    .any(|entry| self.positions[entry.node].distance_squared(point) < 4.)
                {
                    continue;
                }
                if let Some(cost) = self.surface.dry_cost(lead, point) {
                    entrance.anchors.push(Attachment {
                        node,
                        cost: initial + cost,
                    });
                    if entrance.anchors.len() >= 8 {
                        break;
                    }
                }
            }
            if entrance.usable() {
                break;
            }
        }
        entrance
    }

    fn prepare_edges(&mut self, from: usize) {
        if self.edges[from].is_some() {
            return;
        }
        let a = self.positions[from];
        let mut edges = Vec::with_capacity(self.adjacency[from].len());
        for &to in &self.adjacency[from] {
            let b = self.positions[to];
            let key = (from.min(to), from.max(to));
            let cost = self
                .costs
                .entry(key)
                .or_insert_with(|| self.surface.dry_cost(a, b));
            if let Some(cost) = *cost {
                edges.push(Edge {
                    to,
                    cost,
                    crossing: None,
                });
            }
        }
        // When an adjacent mesh edge leads into water or down a steep bank, look
        // ahead on that same bearing for an actual opposite bank vertex. This
        // adds explicit spans without connecting arbitrary nearby mesh vertices.
        let mut extra = Vec::new();
        for &blocked in &self.adjacency[from] {
            if edges.iter().any(|edge| edge.to == blocked) {
                continue;
            }
            let b = self.positions[blocked];
            let Some(dir) = (b - a).try_normalize() else {
                continue;
            };
            if !self.surface.near_river(a, 32.) {
                let Some(bank) = self.surface.height(a) else {
                    continue;
                };
                if ![5., 10., 18.].into_iter().any(|d| {
                    self.surface
                        .height(a + dir * d)
                        .is_some_and(|z| z > 0. && z < bank - 1.8)
                }) {
                    continue;
                }
            }
            let mut previous = usize::MAX;
            for distance in [a.distance(b).clamp(5., 50.), 10., 17., 25., 34., 43., 49.] {
                let target = a + distance * dir;
                let radius = 6.;
                let closest = self
                    .bins
                    .nearby(target - radius, target + radius)
                    .copied()
                    .filter(|&i| {
                        i != from && self.positions[i].distance_squared(target) <= radius * radius
                    })
                    .min_by(|&i, &j| {
                        self.positions[i]
                            .distance_squared(target)
                            .total_cmp(&self.positions[j].distance_squared(target))
                            .then(i.cmp(&j))
                    });
                let Some(to) = closest else {
                    continue;
                };
                if to == previous {
                    continue;
                }
                previous = to;
                if extra.iter().any(|edge: &Edge| edge.to == to) {
                    break;
                }
                let end = self.positions[to];
                if let Some(kind) = self.surface.crossing(a, end) {
                    extra.push(Edge {
                        to,
                        cost: 75. + 3. * a.distance(end),
                        crossing: Some(kind),
                    });
                    break;
                }
            }
        }
        edges.extend(extra);
        self.edges[from] = Some(edges);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Predecessor {
    Vertex(usize),
    Exit(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct State {
    previous: Predecessor,
    node: usize,
}

#[derive(Clone, Copy)]
struct Visit {
    state: State,
    cost: f32,
    estimate: f32,
}

impl Ord for Visit {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| self.cost.total_cmp(&other.cost))
            .then_with(|| self.state.node.cmp(&other.state.node))
            .then_with(|| self.state.previous.cmp(&other.state.previous))
    }
}
impl PartialOrd for Visit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Visit {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Visit {}

#[derive(Clone, Copy)]
struct Arrival {
    cost: f32,
    parent: Option<State>,
    crossing: Option<Crossing>,
}

pub(super) struct Route {
    pub(super) source: usize,
    pub(super) target: usize,
    pub(super) stations: Vec<(Vec2, Option<RouteCrossing>)>,
}

#[derive(Clone, Copy)]
pub(super) struct RouteCrossing {
    pub(super) kind: Crossing,
    pub(super) nodes: (usize, usize),
}

impl Route {
    pub(super) fn crossings(&self) -> impl Iterator<Item = RouteCrossing> + '_ {
        self.stations.iter().filter_map(|(_, crossing)| *crossing)
    }
}

fn target_anchors(targets: &[usize], entrances: &[Entrance]) -> HashMap<usize, Vec<(usize, f32)>> {
    let mut ends = HashMap::<_, Vec<_>>::new();
    for &target in targets {
        for anchor in &entrances[target].anchors {
            ends.entry(anchor.node)
                .or_default()
                .push((target, anchor.cost));
        }
    }
    ends
}

fn turn_cost(incoming: Vec2, outward: Vec2, at_bridge_landing: bool) -> Option<f32> {
    let outward = outward.normalize_or_zero();
    let alignment = incoming.dot(outward).clamp(-1., 1.);
    if at_bridge_landing
        && incoming != Vec2::ZERO
        && outward != Vec2::ZERO
        && alignment < MIN_HEADING_DOT
    {
        return None;
    }
    Some(4.0 * (1.0 - alignment).powi(2))
}

impl Graph<'_> {
    pub(super) fn route(
        &mut self,
        sources: &[usize],
        targets: &[usize],
        exits: &[CartTrackExit],
        entrances: &[Entrance],
        rejected_crossings: &BTreeSet<(usize, usize)>,
    ) -> Option<Route> {
        let ends = target_anchors(targets, entrances);
        if ends.is_empty() || sources.is_empty() {
            return None;
        }
        let heuristic = |p: Vec2| {
            targets
                .iter()
                .map(|&i| exits[i].xy().distance(p))
                .fold(f32::INFINITY, f32::min)
        };
        let mut queue = BinaryHeap::new();
        let mut visited = HashMap::new();
        for &source in sources {
            for anchor in &entrances[source].anchors {
                // Two exits can attach to the same vertex with different
                // approach headings. Both arrivals must enter the search.
                let state = State {
                    previous: Predecessor::Exit(source),
                    node: anchor.node,
                };
                visited.insert(
                    state,
                    Arrival {
                        cost: anchor.cost,
                        parent: None,
                        crossing: None,
                    },
                );
                queue.push(Visit {
                    state,
                    cost: anchor.cost,
                    estimate: anchor.cost + heuristic(self.positions[state.node]),
                });
            }
        }
        let mut best: Option<(f32, State, usize)> = None;
        while let Some(Visit {
            state,
            cost,
            estimate,
        }) = queue.pop()
        {
            if best.is_some_and(|(total, _, _)| estimate >= total) {
                break;
            }
            if cost > visited[&state].cost {
                continue;
            }
            let at = self.positions[state.node];
            let prior = match state.previous {
                Predecessor::Vertex(index) => self.positions[index],
                Predecessor::Exit(index) => entrances[index].lead,
            };
            let incoming = (at - prior).normalize_or_zero();
            let arrived_by_bridge = visited[&state].crossing.is_some();
            if let Some(options) = ends.get(&state.node) {
                for &(target, connector) in options {
                    let Some(turn_cost) =
                        turn_cost(incoming, entrances[target].lead - at, arrived_by_bridge)
                    else {
                        continue;
                    };
                    let total = cost + connector + turn_cost;
                    if best.is_none_or(|(existing, _, _)| total < existing) {
                        best = Some((total, state, target));
                    }
                }
            }
            self.prepare_edges(state.node);
            for edge in self.edges[state.node].as_deref().unwrap_or_default() {
                if state.previous == Predecessor::Vertex(edge.to)
                    || (edge.crossing.is_some()
                        && rejected_crossings.contains(&(state.node, edge.to)))
                {
                    continue;
                }
                let p = self.positions[edge.to];
                let Some(turn_cost) = turn_cost(
                    incoming,
                    p - at,
                    arrived_by_bridge != edge.crossing.is_some(),
                ) else {
                    continue;
                };
                let next = State {
                    previous: Predecessor::Vertex(state.node),
                    node: edge.to,
                };
                let cost = cost + edge.cost + turn_cost;
                if visited.get(&next).is_some_and(|entry| entry.cost <= cost) {
                    continue;
                }
                visited.insert(
                    next,
                    Arrival {
                        cost,
                        parent: Some(state),
                        crossing: edge.crossing,
                    },
                );
                queue.push(Visit {
                    state: next,
                    cost,
                    estimate: cost + heuristic(p),
                });
            }
        }
        let (_, last, target) = best?;
        Some(self.reconstruct(last, &visited, exits, entrances, target))
    }

    fn reconstruct(
        &self,
        mut state: State,
        visited: &HashMap<State, Arrival>,
        exits: &[CartTrackExit],
        entrances: &[Entrance],
        target: usize,
    ) -> Route {
        let mut stations = vec![(exits[target].xy(), None), (entrances[target].lead, None)];
        let source = loop {
            let step = visited[&state];
            let crossing = step
                .crossing
                .zip(step.parent)
                .map(|(kind, parent)| RouteCrossing {
                    kind,
                    nodes: (parent.node, state.node),
                });
            stations.push((self.positions[state.node], crossing));
            if let Predecessor::Exit(source) = state.previous {
                break source;
            }
            state = step.parent.expect("a terrain arrival has a parent");
        };
        stations.extend([(entrances[source].lead, None), (exits[source].xy(), None)]);
        stations.reverse();
        Route {
            source,
            target,
            stations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec3;

    #[test]
    fn exits_sharing_an_anchor_keep_separate_approach_headings() {
        let island = super::super::tests::fixture(|_| 10.);
        let surface = Surface::new(&island, &[]);
        let mut graph = Graph::new(&island.coarser_lods[0], &surface);
        let y = 140.;
        graph.positions = vec![Vec2::new(150., y), Vec2::new(170., y)];
        graph.edges = vec![
            Some(vec![Edge {
                to: 1,
                cost: 20.,
                crossing: None,
            }]),
            Some(Vec::new()),
        ];
        let exits = [(155., -1.), (143., 1.), (176., -1.)].map(|(x, dx)| CartTrackExit {
            settlement_index: usize::from(x > 170.),
            position: Vec3::new(x, y, 10.).to_array(),
            direction: [dx, 0.],
            half_width: 2.,
            curb: false,
        });
        let entrances =
            [(152., 0, 5.), (146., 0, 7.), (173., 1, 6.)].map(|(x, node, cost)| Entrance {
                lead: Vec2::new(x, y),
                anchors: vec![Attachment { node, cost }],
            });
        // Exit 0 reaches the shared anchor sooner, but would need a U-turn.
        for sources in [[0, 1], [1, 0]] {
            let route = graph
                .route(&sources, &[2], &exits, &entrances, &BTreeSet::new())
                .unwrap();
            assert_eq!(route.source, 1);
            assert_eq!(route.stations[0].0, exits[1].xy());
            assert_eq!(route.stations[1].0, entrances[1].lead);
        }
    }
}
