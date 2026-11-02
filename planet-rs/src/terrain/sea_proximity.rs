//! Island's 2–20 m sea-proximity mask, frozen before the final river carve.
use super::graph::{SurfaceGraph, arc};
use crate::PlanetMesh;
use std::{cmp::Ordering, collections::BinaryHeap};

const FULL_METRES: f64 = 2.0;
const FADE_METRES: f64 = 20.0;

#[derive(Clone, Copy, PartialEq)]
struct Entry {
    distance: f64,
    vertex: usize,
}
impl Eq for Entry {}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(super) fn build(mesh: &PlanetMesh, graph: &SurfaceGraph, heights: &[f64]) -> Vec<f64> {
    let mut distances = vec![f64::INFINITY; heights.len()];
    let mut queue = BinaryHeap::new();
    // Seed only dry coastal neighbours, keeping the heap independent of the
    // millions of deep-ocean vertices that cannot reach the 20 m dry fringe.
    for (i, &height) in heights.iter().enumerate() {
        if height <= 0.0 {
            distances[i] = 0.0;
            continue;
        }
        let distance = graph.neighbours[i]
            .iter()
            .filter(|&&j| heights[j] <= 0.0)
            .map(|&j| arc(mesh.directions[i], mesh.directions[j]) * mesh.radius)
            .fold(f64::INFINITY, f64::min);
        if distance <= FADE_METRES {
            distances[i] = distance;
            queue.push(Entry {
                distance,
                vertex: i,
            });
        }
    }
    while let Some(Entry { distance, vertex }) = queue.pop() {
        if distance > distances[vertex] {
            continue;
        }
        for &next in &graph.neighbours[vertex] {
            if heights[next] <= 0.0 {
                continue;
            }
            let candidate =
                distance + arc(mesh.directions[vertex], mesh.directions[next]) * mesh.radius;
            if candidate <= FADE_METRES && candidate < distances[next] {
                distances[next] = candidate;
                queue.push(Entry {
                    distance: candidate,
                    vertex: next,
                });
            }
        }
    }
    for distance in &mut distances {
        *distance = (1.0 - (*distance - FULL_METRES) / (FADE_METRES - FULL_METRES)).clamp(0.0, 1.0);
    }
    distances
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DVec3;

    #[test]
    fn preserved_sea_mask_fades_in_metres_and_does_not_classify_later_river_excavation_as_ocean() {
        for radius in [1000.0, 2000.0] {
            let mesh = PlanetMesh {
                directions: (0..=25)
                    .map(|i| {
                        let angle = f64::from(i) / radius;
                        DVec3::new(angle.sin(), 0.0, angle.cos())
                    })
                    .collect(),
                triangles: Vec::new(),
                radius,
            };
            let graph = SurfaceGraph {
                neighbours: (0_usize..=25)
                    .map(|i| {
                        [i.checked_sub(1), (i < 25).then_some(i + 1)]
                            .into_iter()
                            .flatten()
                            .collect()
                    })
                    .collect(),
                areas: vec![0.0; 26],
            };
            let mut heights = vec![0.01; 26];
            heights[0] = -0.01;
            let proximity = build(&mesh, &graph, &heights);
            assert!((proximity[2] - 1.0).abs() < 1e-12);
            assert!((proximity[11] - 0.5).abs() < 1e-12);
            assert!(proximity[20] < 1e-12 && proximity[25] < 1e-12);
            heights[25] = -0.001;
            assert!(proximity[25] < 1e-12);
            assert!((build(&mesh, &graph, &heights)[25] - 1.0).abs() < 1e-12);
        }
    }
}
