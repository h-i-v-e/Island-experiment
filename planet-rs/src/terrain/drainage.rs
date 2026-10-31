//! Sea-seeded minimax flood routing, followed by downhill incision. Unlike a
//! planar DEM, this graph has no outside border: every submerged site is a root.
//! See [Barnes et al.](https://arxiv.org/abs/1511.04463) (adapted, not copied).
use super::{
    RiverSegment, TerrainOptions,
    graph::{SurfaceGraph, arc},
};
use crate::{DVec3, PlanetError};
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

#[derive(Debug, PartialEq)]
pub struct Drainage {
    /// A land site's next adjacent site. Submerged sites always have no receiver.
    pub receivers: Vec<Option<usize>>,
    /// Accumulated rainfall catchment, in unit-sphere steradians.
    pub catchment: Vec<f64>,
    /// Parent-before-child order used for linear-time accumulation/validation.
    pub(super) order: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct State {
    spill: f64,
    distance: f64,
    vertex: usize,
}
impl Eq for State {}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .spill
            .total_cmp(&self.spill)
            .then_with(|| other.distance.total_cmp(&self.distance))
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn route(directions: &[DVec3], graph: &SurfaceGraph, heights: &[f64]) -> Drainage {
    let count = heights.len();
    let mut receivers = vec![None; count];
    let mut spills = vec![f64::INFINITY; count];
    let mut distances = vec![f64::INFINITY; count];
    let mut ranks = vec![usize::MAX; count];
    let mut order = Vec::with_capacity(count);
    let mut queue = BinaryHeap::new();
    for (vertex, &height) in heights.iter().enumerate() {
        if height < 0.0 {
            spills[vertex] = 0.0;
            distances[vertex] = 0.0;
            queue.push(State {
                spill: 0.0,
                distance: 0.0,
                vertex,
            });
        }
    }
    while let Some(state) = queue.pop() {
        let i = state.vertex;
        if ranks[i] != usize::MAX {
            continue;
        }
        ranks[i] = order.len();
        order.push(i);
        for &neighbour in &graph.neighbours[i] {
            if ranks[neighbour] != usize::MAX || heights[neighbour] < 0.0 {
                continue;
            }
            let spill = state.spill.max(heights[neighbour]);
            let distance = state.distance + arc(directions[i], directions[neighbour]);
            if spill < spills[neighbour]
                || (spill.total_cmp(&spills[neighbour]).is_eq() && distance < distances[neighbour])
            {
                spills[neighbour] = spill;
                distances[neighbour] = distance;
                receivers[neighbour] = Some(i);
                queue.push(State {
                    spill,
                    distance,
                    vertex: neighbour,
                });
            }
        }
    }
    prefer_downhill(directions, graph, heights, &ranks, &order, &mut receivers);
    let mut drainage = Drainage {
        receivers,
        catchment: vec![0.0; count],
        order,
    };
    drainage.accumulate(graph, heights);
    drainage
}

fn prefer_downhill(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &[f64],
    ranks: &[usize],
    order: &[usize],
    receivers: &mut [Option<usize>],
) {
    // Prefer the steepest already-connected downhill neighbour where there is
    // one. The flood parent supplies a deterministic route out of a dry pit.
    for &i in order {
        if heights[i] < 0.0 {
            continue;
        }
        let natural = graph.neighbours[i]
            .iter()
            .copied()
            .filter(|&j| ranks[j] < ranks[i] && heights[j] < heights[i])
            .max_by(|&a, &b| {
                let slope = |j| (heights[i] - heights[j]) / arc(directions[i], directions[j]);
                slope(a).total_cmp(&slope(b)).then_with(|| b.cmp(&a))
            });
        if natural.is_some() {
            receivers[i] = natural;
        }
    }
}

impl Drainage {
    fn accumulate(&mut self, graph: &SurfaceGraph, heights: &[f64]) {
        for (i, catchment) in self.catchment.iter_mut().enumerate() {
            *catchment = if heights[i] >= 0.0 {
                graph.areas[i]
            } else {
                0.0
            };
        }
        for &i in self.order.iter().rev() {
            if let Some(target) = self.receivers[i] {
                self.catchment[target] += self.catchment[i];
            }
        }
    }

    fn rivers(&self, options: &TerrainOptions) -> Vec<RiverSegment> {
        let threshold = options.river_catchment * 4.0 * std::f64::consts::PI;
        self.receivers
            .iter()
            .enumerate()
            .filter_map(|(from, &receiver)| {
                let to = receiver?;
                (self.catchment[from] >= threshold).then_some(RiverSegment {
                    from,
                    to,
                    catchment: self.catchment[from],
                })
            })
            .collect()
    }
}

/// Finish banks by lifting, never incising. A bank can be finalized only after
/// all its downstream bank neighbours, so an upstream dip is raised to their
/// maximum height. The same sea-seeded flood raises any newly trapped feeding
/// terrain and builds valid downhill receivers on the finished surface.
pub(super) fn lift_banks(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &mut [f64],
    soil: &mut [f64],
    upstream_banks: &[(usize, usize)],
    options: &TerrainOptions,
) -> Result<(Drainage, Vec<RiverSegment>), PlanetError> {
    let count = heights.len();
    let mut upstream = HashMap::<usize, Vec<usize>>::new();
    let mut pending = vec![0_usize; count];
    for &(down, up) in upstream_banks {
        upstream.entry(down).or_default().push(up);
        pending[up] += 1;
    }
    let mut floors = vec![f64::NEG_INFINITY; count];
    let mut candidates = vec![f64::INFINITY; count];
    let mut distances = vec![f64::INFINITY; count];
    let mut receivers = vec![None; count];
    let mut ranks = vec![usize::MAX; count];
    let mut order = Vec::with_capacity(count);
    let mut queue = BinaryHeap::new();
    for (vertex, &height) in heights.iter().enumerate() {
        if height < 0.0 {
            queue.push(State {
                spill: height,
                distance: 0.0,
                vertex,
            });
        }
    }
    while let Some(state) = queue.pop() {
        let i = state.vertex;
        if ranks[i] != usize::MAX {
            continue;
        }
        ranks[i] = order.len();
        order.push(i);
        soil[i] += state.spill - heights[i];
        heights[i] = state.spill;
        for &j in &graph.neighbours[i] {
            if ranks[j] != usize::MAX || heights[j] < 0.0 {
                continue;
            }
            let floor = heights[i].next_up();
            let distance = state.distance + arc(directions[i], directions[j]);
            if floor < candidates[j]
                || (floor.total_cmp(&candidates[j]).is_eq() && distance < distances[j])
            {
                candidates[j] = floor;
                distances[j] = distance;
                receivers[j] = Some(i);
                if pending[j] == 0 {
                    queue.push(State {
                        spill: heights[j].max(floors[j]).max(floor),
                        distance,
                        vertex: j,
                    });
                }
            }
        }
        for &j in upstream.get(&i).into_iter().flatten() {
            floors[j] = floors[j].max(heights[i]);
            pending[j] -= 1;
            if pending[j] == 0 && candidates[j].is_finite() {
                queue.push(State {
                    spill: heights[j].max(floors[j]).max(candidates[j]),
                    distance: distances[j],
                    vertex: j,
                });
            }
        }
    }
    if order.len() != count {
        return Err(PlanetError::InvalidTopology(
            "river bank order cannot reach sea",
        ));
    }
    prefer_downhill(directions, graph, heights, &ranks, &order, &mut receivers);
    let mut drainage = Drainage {
        receivers,
        catchment: vec![0.0; count],
        order,
    };
    drainage.accumulate(graph, heights);
    let rivers = drainage.rivers(options);
    Ok((drainage, rivers))
}

pub(super) fn carve(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &mut [f64],
    soil: &mut [f64],
    options: &TerrainOptions,
) -> (Drainage, Vec<RiverSegment>, f64) {
    let mut drainage = route(directions, graph, heights);
    let threshold = options.river_catchment * 4.0 * std::f64::consts::PI;
    let mut removed = 0.0;
    for &i in &drainage.order {
        if heights[i] >= 0.0 && drainage.catchment[i] >= threshold {
            let depth = options.river_depth * (drainage.catchment[i] / threshold).sqrt().min(4.0);
            heights[i] -= depth;
            soil[i] = (soil[i] - depth).max(0.0);
            removed += depth * graph.areas[i];
        }
    }
    let mut depths = vec![0.0; heights.len()];
    let (rivers, repaired) = repair(
        directions,
        graph,
        heights,
        soil,
        options,
        &mut drainage,
        &mut depths,
    );
    (drainage, rivers, removed + repaired)
}

pub(super) fn repair(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &mut [f64],
    soil: &mut [f64],
    options: &TerrainOptions,
    drainage: &mut Drainage,
    depths: &mut [f64],
) -> (Vec<RiverSegment>, f64) {
    let mut removed = 0.0;
    // Incise downstream saddles in upstream-to-outlet order. Never raise sea
    // level or fill all land to a spill plane; no coastline constraint is used.
    for &i in drainage.order.iter().rev() {
        if heights[i] < 0.0 {
            continue;
        }
        if let Some(target) = drainage.receivers[i] {
            let ceiling = heights[i] - 1.0e-6 * arc(directions[i], directions[target]);
            let carved = (heights[target] - ceiling).max(0.0);
            heights[target] -= carved;
            depths[target] += carved;
            soil[target] = (soil[target] - carved).max(0.0);
            removed += carved * graph.areas[target];
        }
    }
    // Carving can create new below-sea roots. Terminate at the first such site.
    for (i, receiver) in drainage.receivers.iter_mut().enumerate() {
        if heights[i] < 0.0 {
            *receiver = None;
        }
    }
    drainage.accumulate(graph, heights);
    let rivers = drainage.rivers(options);
    (rivers, removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlanetOptions, generate};

    #[test]
    fn bank_dips_lift_upstream_independently_and_feeding_pits_lift_without_new_incision() {
        let directions: Vec<_> = (0..10)
            .map(|i| DVec3::new(f64::from(i) * 0.001, 0.0, 1.0).normalize())
            .collect();
        let graph = SurfaceGraph {
            neighbours: vec![
                vec![1, 3, 6],
                vec![0, 2, 3, 4, 6, 7],
                vec![1, 4, 5, 7, 8],
                vec![0, 1, 4],
                vec![1, 2, 3, 5, 9],
                vec![2, 4],
                vec![0, 1, 7],
                vec![1, 2, 6, 8],
                vec![2, 7],
                vec![4],
            ],
            areas: vec![1.0; 10],
        };
        let before = [
            -0.01, 0.01, 0.02, 0.04, 0.01, 0.03, 0.02, 0.005, 0.015, 0.001,
        ];
        let mut heights = before;
        let mut soil = [0.0; 10];
        let edges = [(3, 4), (4, 5), (6, 7), (7, 8)];
        let (drains, _) = lift_banks(
            &directions,
            &graph,
            &mut heights,
            &mut soil,
            &edges,
            &TerrainOptions::default(),
        )
        .unwrap();
        for (down, up) in edges {
            assert!(heights[up] >= heights[down]);
        }
        assert!((heights[5] - 0.04).abs() < 1.0e-12);
        assert!(
            (heights[8] - 0.02).abs() < 1.0e-12,
            "the opposite bank has its own floor"
        );
        assert!(
            heights[9] > heights[4],
            "lift the trapped feeder instead of cutting the bank"
        );
        assert_eq!(
            &heights[..3],
            &before[..3],
            "sea and channel bed stay unchanged"
        );
        for i in 0..heights.len() {
            assert!(heights[i] >= before[i]);
            assert!((soil[i] - (heights[i] - before[i])).abs() < 1.0e-12);
            let mut current = i;
            for _ in 0..heights.len() {
                let Some(next) = drains.receivers[current] else {
                    break;
                };
                assert!(graph.neighbours[current].contains(&next));
                assert!(heights[current] > heights[next]);
                current = next;
            }
            assert!(heights[current] < 0.0);
        }
    }

    #[test]
    fn closed_land_pits_breach_to_any_submerged_site() {
        let mesh = generate(PlanetOptions {
            vertex_count: 64,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        let mut heights = vec![0.08; 64];
        let separate_basin = (1..64).find(|i| !graph.neighbours[0].contains(i)).unwrap();
        let dry_pit = (1..64).find(|&i| i != separate_basin).unwrap();
        heights[0] = -0.03;
        heights[separate_basin] = -0.01;
        heights[dry_pit] = 0.001; // Dry local depression behind the high rim.
        let (drains, _, _) = carve(
            &mesh.directions,
            &graph,
            &mut heights,
            &mut vec![0.002; 64],
            &TerrainOptions::default(),
        );
        for start in 0..64 {
            let mut current = start;
            for _ in 0..64 {
                let Some(next) = drains.receivers[current] else {
                    break;
                };
                assert!(graph.neighbours[current].contains(&next));
                assert!(heights[next] < heights[current]);
                current = next;
            }
            assert!(heights[current] < 0.0, "site {start} must reach water");
        }
        assert!(drains.receivers[0].is_none() && drains.receivers[separate_basin].is_none());
    }

    #[test]
    fn dry_and_fully_submerged_worlds_are_valid_without_forced_outlets() {
        let mesh = generate(PlanetOptions {
            vertex_count: 32,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        for height in [0.1, -0.1, 0.0] {
            let mut heights = vec![height; 32];
            let (drains, rivers, removed) = carve(
                &mesh.directions,
                &graph,
                &mut heights,
                &mut vec![0.002; 32],
                &TerrainOptions::default(),
            );
            assert!(drains.receivers.iter().all(Option::is_none));
            assert!(rivers.is_empty());
            assert!(removed.abs() < f64::EPSILON);
            assert!(heights.iter().all(|&h| (h - height).abs() < f64::EPSILON));
        }
    }
}
