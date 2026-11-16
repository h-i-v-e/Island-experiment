//! Natural catchments joined at their lowest sea-connected spill saddles.
//!
//! Reversing just the sink-to-saddle branch preserves the rest of each valley's
//! natural drainage. The resulting forest is also the graph used for rainfall
//! accumulation and river tracing.

use super::tracing::{RiverMeanderScale, river_effective_height};
use super::{Adjacency, BinaryHeap, Mesh, Ordering};

pub(super) struct Drainage {
    pub(super) downstream: Vec<usize>,
    pub(super) reaches_ocean: Vec<bool>,
}

#[derive(Clone, Copy)]
struct Spill {
    height: f32,
    distance: f32,
    from: usize,
    to: usize,
}

impl PartialEq for Spill {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Spill {}

impl PartialOrd for Spill {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Spill {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .height
            .total_cmp(&self.height)
            .then_with(|| other.distance.total_cmp(&self.distance))
            .then_with(|| other.from.cmp(&self.from))
            .then_with(|| other.to.cmp(&self.to))
    }
}

impl Drainage {
    pub(super) fn new(
        mesh: &Mesh,
        adjacency: &Adjacency,
        ocean: &[bool],
        excluded: &[bool],
        seed: u64,
    ) -> Self {
        let count = mesh.vertices.len();
        let sea = count;
        let mut downstream = natural_downstream(mesh, adjacency, ocean, excluded, seed);
        let mut order: Vec<_> = (0..count).collect();
        order.sort_unstable_by(|&left, &right| {
            mesh.vertices[left]
                .z
                .total_cmp(&mesh.vertices[right].z)
                .then_with(|| left.cmp(&right))
        });

        let mut basin = vec![sea; count];
        let mut heads = vec![usize::MAX; count + 1];
        let mut next_member = vec![usize::MAX; count];
        for vertex in order {
            let receiver = downstream[vertex];
            basin[vertex] = if ocean[vertex] {
                sea
            } else if receiver == vertex {
                vertex
            } else {
                basin[receiver]
            };
            next_member[vertex] = heads[basin[vertex]];
            heads[basin[vertex]] = vertex;
        }

        let mut settled = vec![false; count + 1];
        settled[sea] = true;
        let mut frontier = BinaryHeap::new();
        let mut flood = BasinFlood {
            mesh,
            adjacency,
            excluded,
            basin: &basin,
            heads: &heads,
            next_member: &next_member,
            settled: &mut settled,
            frontier: &mut frontier,
        };
        flood.expand(sea, 0.0, 0.0);
        while let Some(spill) = flood.frontier.pop() {
            let incoming = basin[spill.from];
            if flood.settled[incoming] {
                continue;
            }
            // Reverse the original tree branch, then attach it to an already
            // drained basin. All other natural receivers remain untouched.
            let mut vertex = spill.from;
            let mut receiver = spill.to;
            loop {
                let original = downstream[vertex];
                downstream[vertex] = receiver;
                if original == vertex {
                    break;
                }
                receiver = vertex;
                vertex = original;
            }
            flood.settled[incoming] = true;
            flood.expand(incoming, spill.height, spill.distance);
        }

        let reaches_ocean = basin
            .iter()
            .enumerate()
            .map(|(vertex, &basin)| settled[basin] && !is_excluded(excluded, vertex))
            .collect();
        Self {
            downstream,
            reaches_ocean,
        }
    }
}

struct BasinFlood<'a> {
    mesh: &'a Mesh,
    adjacency: &'a Adjacency,
    excluded: &'a [bool],
    basin: &'a [usize],
    heads: &'a [usize],
    next_member: &'a [usize],
    settled: &'a mut [bool],
    frontier: &'a mut BinaryHeap<Spill>,
}

impl BasinFlood<'_> {
    fn expand(&mut self, basin: usize, height: f32, distance: f32) {
        let mut vertex = self.heads[basin];
        while vertex != usize::MAX {
            if !is_excluded(self.excluded, vertex) {
                for &neighbour in &self.adjacency[vertex] {
                    if self.settled[self.basin[neighbour]] || is_excluded(self.excluded, neighbour)
                    {
                        continue;
                    }
                    self.frontier.push(Spill {
                        height: height
                            .max(self.mesh.vertices[vertex].z)
                            .max(self.mesh.vertices[neighbour].z),
                        distance: distance
                            + self.mesh.vertices[vertex]
                                .truncate()
                                .distance(self.mesh.vertices[neighbour].truncate()),
                        from: neighbour,
                        to: vertex,
                    });
                }
            }
            vertex = self.next_member[vertex];
        }
    }
}

fn is_excluded(excluded: &[bool], vertex: usize) -> bool {
    excluded.get(vertex).copied().unwrap_or(false)
}

fn natural_downstream(
    mesh: &Mesh,
    adjacency: &Adjacency,
    ocean: &[bool],
    excluded: &[bool],
    seed: u64,
) -> Vec<usize> {
    let scale = RiverMeanderScale::from_mesh(mesh, adjacency);
    let effective_heights: Vec<_> = (0..mesh.vertices.len())
        .map(|vertex| river_effective_height(mesh, vertex, seed, scale))
        .collect();
    mesh.vertices
        .iter()
        .enumerate()
        .map(|(vertex, position)| {
            if ocean[vertex] || is_excluded(excluded, vertex) {
                return vertex;
            }
            let score = |next: usize| {
                let distance = position
                    .truncate()
                    .distance(mesh.vertices[next].truncate())
                    .max(f32::EPSILON);
                (effective_heights[vertex] - effective_heights[next]) / distance
            };
            adjacency[vertex]
                .iter()
                .copied()
                .filter(|&next| mesh.vertices[next].z < position.z && !is_excluded(excluded, next))
                .max_by(|&left, &right| {
                    score(left)
                        .total_cmp(&score(right))
                        .then_with(|| right.cmp(&left))
                })
                .unwrap_or(vertex)
        })
        .collect()
}
