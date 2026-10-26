//! Connect submerged components by a deterministic nearest-sea walk. Distances
//! follow mesh edges on the sphere; sea level is never moved or recalibrated.
use super::graph::{SurfaceGraph, arc};
use crate::{PlanetError, PlanetMesh};
use std::{cmp::Ordering, collections::BinaryHeap};

const LAND: usize = usize::MAX;
const CHANNEL_HALF_WIDTH: f64 = 0.0045;
const CHANNEL_DEPTH: f64 = 0.002;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OceanStage {
    Lod2,
    Lod1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanPass {
    pub stage: OceanStage,
    pub vertices: usize,
    pub seas_before: usize,
    pub seas_after: usize,
    pub channels: usize,
    /// Area-weighted height removed, treated as exported sediment.
    pub removed_material: f64,
}

struct Seas {
    labels: Vec<usize>,
    shores: Vec<Vec<usize>>,
}

impl Seas {
    fn new(graph: &SurfaceGraph, heights: &[f64]) -> Self {
        let mut labels = vec![LAND; heights.len()];
        let mut shores = Vec::new();
        let mut stack = Vec::new();
        for start in 0..heights.len() {
            if heights[start] >= 0.0 || labels[start] != LAND {
                continue;
            }
            let label = shores.len();
            let mut shore = Vec::new();
            labels[start] = label;
            stack.push(start);
            while let Some(i) = stack.pop() {
                if graph.neighbours[i].iter().any(|&j| heights[j] >= 0.0) {
                    shore.push(i);
                }
                for &j in &graph.neighbours[i] {
                    if heights[j] < 0.0 && labels[j] == LAND {
                        labels[j] = label;
                        stack.push(j);
                    }
                }
            }
            shore.sort_unstable();
            shores.push(shore);
        }
        Self { labels, shores }
    }
}

pub(super) fn count(graph: &SurfaceGraph, heights: &[f64]) -> usize {
    Seas::new(graph, heights).shores.len()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct State {
    distance: f64,
    vertex: usize,
}
impl Eq for State {}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Search stamps let successive local searches reuse buffers without clearing
/// every vertex of a finely tessellated planet for each small sea.
struct Search {
    distances: Vec<f64>,
    parents: Vec<usize>,
    stamps: Vec<usize>,
    stamp: usize,
    queue: BinaryHeap<State>,
}

impl Search {
    fn new(count: usize) -> Self {
        Self {
            distances: vec![0.0; count],
            parents: vec![LAND; count],
            stamps: vec![0; count],
            stamp: 0,
            queue: BinaryHeap::new(),
        }
    }

    fn begin(&mut self, sources: &[usize]) {
        self.stamp += 1;
        self.queue.clear();
        for &i in sources {
            self.distances[i] = 0.0;
            self.parents[i] = LAND;
            self.stamps[i] = self.stamp;
            self.queue.push(State {
                distance: 0.0,
                vertex: i,
            });
        }
    }

    fn extend(&mut self, mesh: &PlanetMesh, graph: &SurfaceGraph, state: State, limit: f64) {
        for &j in &graph.neighbours[state.vertex] {
            let distance = state.distance + arc(mesh.directions[state.vertex], mesh.directions[j]);
            if distance <= limit && (self.stamps[j] != self.stamp || distance < self.distances[j]) {
                self.distances[j] = distance;
                self.parents[j] = state.vertex;
                self.stamps[j] = self.stamp;
                self.queue.push(State {
                    distance,
                    vertex: j,
                });
            }
        }
    }

    fn nearest(
        &mut self,
        mesh: &PlanetMesh,
        graph: &SurfaceGraph,
        seas: &Seas,
        joined: &[bool],
        current: usize,
    ) -> Option<(usize, Vec<usize>)> {
        self.begin(&seas.shores[current]);
        while let Some(state) = self.queue.pop() {
            if state.distance > self.distances[state.vertex] {
                continue;
            }
            let label = seas.labels[state.vertex];
            if label != LAND && !joined[label] {
                let mut path = vec![state.vertex];
                while self.parents[*path.last()?] != LAND {
                    path.push(self.parents[*path.last()?]);
                }
                return Some((label, path));
            }
            self.extend(mesh, graph, state, f64::INFINITY);
        }
        None
    }

    fn carve(
        &mut self,
        mesh: &PlanetMesh,
        graph: &SurfaceGraph,
        heights: &mut [f64],
        soil: &mut [f64],
        path: &[usize],
    ) -> f64 {
        self.begin(path);
        let mut removed = 0.0;
        while let Some(state) = self.queue.pop() {
            let i = state.vertex;
            if state.distance > self.distances[i] {
                continue;
            }
            let x = state.distance / CHANNEL_HALF_WIDTH;
            let target = if x < 1.0 {
                -CHANNEL_DEPTH * (1.0 - x * x)
            } else {
                let t = (x - 1.0).clamp(0.0, 1.0);
                heights[i] * t * t * (3.0 - 2.0 * t)
            };
            let cut = (heights[i] - target).max(0.0);
            heights[i] -= cut;
            soil[i] = (soil[i] - cut).max(0.0);
            removed += cut * graph.areas[i];
            self.extend(mesh, graph, state, CHANNEL_HALF_WIDTH * 2.0);
        }
        removed
    }
}

/// Expand the growing ocean after each cut, including any extra seas touched by
/// banks along the way. Each wet vertex is visited once over the whole pass.
fn join(
    graph: &SurfaceGraph,
    heights: &[f64],
    seas: &Seas,
    connected: &mut [bool],
    joined: &mut [bool],
    sources: &[usize],
) {
    let mut stack = Vec::new();
    for &i in sources {
        if heights[i] < 0.0 && !connected[i] {
            connected[i] = true;
            stack.push(i);
        }
    }
    while let Some(i) = stack.pop() {
        if seas.labels[i] != LAND {
            joined[seas.labels[i]] = true;
        }
        for &j in &graph.neighbours[i] {
            if heights[j] < 0.0 && !connected[j] {
                connected[j] = true;
                stack.push(j);
            }
        }
    }
}

pub(super) fn connect(
    mesh: &PlanetMesh,
    graph: &SurfaceGraph,
    heights: &mut [f64],
    soil: &mut [f64],
    stage: OceanStage,
) -> Result<OceanPass, PlanetError> {
    let seas = Seas::new(graph, heights);
    let mut report = OceanPass {
        stage,
        vertices: heights.len(),
        seas_before: seas.shores.len(),
        seas_after: seas.shores.len(),
        channels: 0,
        removed_material: 0.0,
    };
    if seas.shores.len() <= 1 {
        return Ok(report);
    }
    let mut joined = vec![false; seas.shores.len()];
    let mut connected = vec![false; heights.len()];
    join(
        graph,
        heights,
        &seas,
        &mut connected,
        &mut joined,
        &seas.shores[0],
    );
    let mut current = 0;
    let mut search = Search::new(heights.len());
    while joined.iter().any(|&joined| !joined) {
        let (target, path) = search.nearest(mesh, graph, &seas, &joined, current).ok_or(
            PlanetError::InvalidTopology("disconnected seas have no surface path"),
        )?;
        report.removed_material += search.carve(mesh, graph, heights, soil, &path);
        join(graph, heights, &seas, &mut connected, &mut joined, &path);
        report.channels += 1;
        // Continue from the sea just reached, not the original starting sea.
        current = target;
    }
    report.seas_after = count(graph, heights);
    if report.seas_after != 1 {
        return Err(PlanetError::InvalidTopology(
            "ocean channels did not connect every sea",
        ));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DVec3, PlanetOptions, generate};

    #[test]
    fn nearest_walk_continues_from_the_sea_just_reached() {
        // A--B----D, with C on A's other side. After A reaches B,
        // B's nearest remaining sea is D; restarting at A would choose C.
        let angles: [f64; 7] = [0.0, 0.1, -0.15, 0.3, 0.05, -0.075, 0.2];
        let mesh = PlanetMesh {
            directions: angles
                .into_iter()
                .map(|a| DVec3::new(a.cos(), a.sin(), 0.0))
                .collect(),
            triangles: Vec::new(),
            radius: 1.0,
        };
        let graph = SurfaceGraph {
            neighbours: vec![
                vec![4, 5],
                vec![4, 6],
                vec![5],
                vec![6],
                vec![0, 1],
                vec![0, 2],
                vec![1, 3],
            ],
            areas: vec![1.0; 7],
        };
        let heights = [-0.01, -0.01, -0.01, -0.01, 0.1, 0.1, 0.1];
        let seas = Seas::new(&graph, &heights);
        let mut search = Search::new(7);
        let mut joined = vec![true, false, false, false];
        let (b, path) = search.nearest(&mesh, &graph, &seas, &joined, 0).unwrap();
        assert_eq!(b, 1);
        assert_eq!(path, [1, 4, 0]);
        joined[b] = true;
        let (d, path) = search.nearest(&mesh, &graph, &seas, &joined, b).unwrap();
        assert_eq!(d, 3);
        assert_eq!(path, [3, 6, 1]);
        assert_eq!(
            search.nearest(&mesh, &graph, &seas, &joined, 0).unwrap().0,
            2
        );
    }

    #[test]
    fn channels_join_real_spherical_seas_without_raising_land_or_changing_radius() {
        let mesh = generate(PlanetOptions {
            vertex_count: 128,
            radius: 6371.0,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        let mut heights = vec![0.08; 128];
        let mut seas = Vec::new();
        for (i, height) in heights.iter_mut().enumerate() {
            if seas.iter().all(|&j| !graph.neighbours[i].contains(&j)) {
                seas.push(i);
                *height = -0.02;
                if seas.len() == 4 {
                    break;
                }
            }
        }
        let original = heights.clone();
        let mut soil = vec![0.002; 128];
        let report = connect(&mesh, &graph, &mut heights, &mut soil, OceanStage::Lod2).unwrap();
        assert_eq!(report.seas_before, 4);
        assert_eq!(report.seas_after, 1);
        assert!((1..=3).contains(&report.channels));
        assert_eq!(count(&graph, &heights), 1);
        assert!(
            heights
                .iter()
                .zip(&original)
                .all(|(after, before)| after <= before)
        );
        assert!(soil.iter().all(|&s| s >= 0.0));
        let removed: f64 = heights
            .iter()
            .zip(&original)
            .zip(&graph.areas)
            .map(|((after, before), area)| (before - after) * area)
            .sum();
        assert!((removed - report.removed_material).abs() < 1.0e-12);
        assert_eq!(mesh.radius.to_bits(), 6371.0_f64.to_bits());
        let second = connect(&mesh, &graph, &mut heights, &mut soil, OceanStage::Lod1).unwrap();
        assert_eq!(second.channels, 0);
        assert!(second.removed_material.abs() < f64::EPSILON);
    }

    #[test]
    fn dry_wet_and_sea_level_spheres_need_no_channels() {
        let mesh = generate(PlanetOptions {
            vertex_count: 32,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        for height in [-0.1, 0.0, 0.1] {
            let mut heights = vec![height; 32];
            let report = connect(
                &mesh,
                &graph,
                &mut heights,
                &mut vec![0.002; 32],
                OceanStage::Lod1,
            )
            .unwrap();
            assert_eq!(report.channels, 0);
            assert!(heights.iter().all(|h| (h - height).abs() < f64::EPSILON));
        }
    }
}
