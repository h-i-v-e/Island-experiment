//! Cover sea-connected qualifying catchments without a source cap or invented rainfall.
use super::{Drainage, RiverSegment, SurfaceGraph, TerrainOptions};
use crate::{DVec3, terrain::feature_index::FeatureIndex};

pub(super) fn select(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &[f64],
    drainage: &Drainage,
    options: &TerrainOptions,
    inherited_sources: &[DVec3],
) -> Vec<RiverSegment> {
    let valid = outlet_paths(graph, heights, drainage);
    let threshold = options.river_catchment * 4.0 * std::f64::consts::PI;
    let mut inherited = vec![false; heights.len()];
    for source in remap(directions, graph, inherited_sources) {
        inherited[source] = true;
    }
    let candidates = candidate_order(drainage, &valid, &inherited, threshold);
    let upstream = strongest_upstream(&candidates, &drainage.receivers);
    let mut selected = vec![false; heights.len()];
    for candidate in candidates {
        if selected[candidate] {
            continue;
        }
        let mut source = candidate;
        while let Some(next) = upstream[source].filter(|&next| !selected[next]) {
            source = next;
        }
        let mut current = source;
        while !selected[current] {
            let Some(next) = drainage.receivers[current] else {
                break;
            };
            selected[current] = true;
            current = next;
        }
    }
    drainage
        .receivers
        .iter()
        .enumerate()
        .filter_map(|(from, &receiver)| {
            let to = receiver?;
            selected[from].then_some(RiverSegment {
                from,
                to,
                catchment: drainage.catchment[from],
            })
        })
        .collect()
}

/// Prefer the largest true catchment, keeping anchors only as equal-area hints.
fn candidate_order(
    drainage: &Drainage,
    valid: &[bool],
    inherited: &[bool],
    threshold: f64,
) -> Vec<usize> {
    let mut candidates: Vec<_> = drainage
        .catchment
        .iter()
        .enumerate()
        .filter_map(|(vertex, &catchment)| {
            (valid[vertex] && catchment >= threshold && drainage.receivers[vertex].is_some())
                .then_some(vertex)
        })
        .collect();
    candidates.sort_unstable_by(|&a, &b| {
        drainage.catchment[b]
            .total_cmp(&drainage.catchment[a])
            .then_with(|| inherited[b].cmp(&inherited[a]))
            .then_with(|| a.cmp(&b))
    });
    candidates
}

/// Candidates are already in descending priority. Extending to the strongest
/// qualifying headwater makes the first accepted path a complete main stem.
fn strongest_upstream(candidates: &[usize], receivers: &[Option<usize>]) -> Vec<Option<usize>> {
    let mut upstream = vec![None; receivers.len()];
    for &source in candidates {
        if let Some(target) = receivers[source] {
            upstream[target].get_or_insert(source);
        }
    }
    upstream
}

/// A receiver must already have a valid route in the sea-seeded parent order.
/// This also rejects cycles, dry roots, missing vertices and non-adjacent links.
fn outlet_paths(graph: &SurfaceGraph, heights: &[f64], drainage: &Drainage) -> Vec<bool> {
    let mut valid = vec![false; heights.len()];
    for &vertex in &drainage.order {
        if vertex >= heights.len() || !heights[vertex].is_finite() {
            continue;
        }
        valid[vertex] = match drainage.receivers[vertex] {
            None => heights[vertex] < 0.0,
            Some(next) => {
                valid.get(next).copied().unwrap_or(false)
                    && graph.neighbours[vertex].contains(&next)
            }
        };
    }
    valid
}

fn unit(direction: DVec3) -> bool {
    direction.is_finite() && (direction.length_squared() - 1.0).abs() <= 1.0e-6
}

fn mean_edge_angle(directions: &[DVec3], graph: &SurfaceGraph) -> f64 {
    let mut total = 0.0;
    let mut count = 0_u32;
    for (vertex, neighbours) in graph.neighbours.iter().enumerate() {
        for &next in neighbours.iter().filter(|&&next| next > vertex) {
            total += super::arc(directions[vertex], directions[next]);
            count += 1;
        }
    }
    total / f64::from(count.max(1))
}

/// Index anchor caps once, then visit vertices once. Exact chord distance and
/// vertex-index ties choose the nearest current vertex before checking terrain.
fn remap(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    sources: &[DVec3],
) -> impl Iterator<Item = usize> {
    let anchors = sources
        .iter()
        .copied()
        .filter(|&source| unit(source))
        .collect::<Vec<_>>();
    let mut nearest = vec![None::<(usize, f64)>; anchors.len()];
    if !anchors.is_empty() {
        let angle = (mean_edge_angle(directions, graph) * 2.0).min(std::f64::consts::PI);
        let distance_limit = (2.0 * (angle * 0.5).sin()).powi(2);
        let index = FeatureIndex::new(anchors.iter().map(|&anchor| (anchor, angle)));
        for (vertex, &direction) in directions
            .iter()
            .enumerate()
            .filter(|(_, direction)| unit(**direction))
        {
            for &anchor in index.at(direction) {
                let distance = direction.distance_squared(anchors[anchor]);
                if distance <= distance_limit
                    && nearest[anchor].is_none_or(|(current, best)| {
                        distance < best || (distance.total_cmp(&best).is_eq() && vertex < current)
                    })
                {
                    nearest[anchor] = Some((vertex, distance));
                }
            }
        }
    }
    nearest
        .into_iter()
        .filter_map(|nearest| nearest.map(|(vertex, _)| vertex))
}

pub(in crate::terrain) fn retain_sources(
    sources: &mut Vec<DVec3>,
    directions: &[DVec3],
    rivers: &[RiverSegment],
) {
    let mut incoming = vec![false; directions.len()];
    for river in rivers {
        incoming[river.to] = true;
    }
    sources.clear();
    sources.extend(
        rivers
            .iter()
            .filter(|river| !incoming[river.from])
            .map(|river| directions[river.from]),
    );
}

#[cfg(test)]
#[path = "river_sources_tests.rs"]
mod tests;
