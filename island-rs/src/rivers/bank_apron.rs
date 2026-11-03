//! Freeze uphill constraints along the dry rungs modified by bank smoothing.
use std::collections::BTreeMap;

use super::{Mesh, RiverNetwork, Vec2, WaterfallPatch, geometry::RiverMeshBuffers};

pub(super) const FULL_RINGS: u8 = 2;

fn contour_edges(terrain: &Mesh, rungs: &[u8]) -> Vec<(usize, usize)> {
    let mut edges = BTreeMap::<_, usize>::new();
    for face in terrain.triangles.chunks_exact(3) {
        let [a, b, c] = [face[0], face[1], face[2]].map(|i| i as usize);
        for (a, b, c) in [(a, b, c), (b, c, a), (c, a, b)] {
            if rungs[a] >= 2 && rungs[a] != u8::MAX && rungs[a] == rungs[b] && rungs[c] < rungs[a] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    edges
        .into_iter()
        .filter_map(|(edge, count)| (count == 1).then_some(edge))
        .collect()
}

fn flow_direction(
    source: usize,
    terrain: &Mesh,
    network: &RiverNetwork,
    buffers: &RiverMeshBuffers,
    patches: &[WaterfallPatch],
    stations: &[f32],
    neighbours: &[usize],
) -> Vec2 {
    let owner = buffers.owners[source].expect("apron source is an owned bank");
    let point = terrain.vertices[source].truncate();
    if let Some(patch) = patches
        .iter()
        .filter(|patch| {
            let (along, lateral) = patch.local_coordinates(point);
            patch.owns_local_channel(owner)
                && along >= -WaterfallPatch::face_run() - super::WATERFALL_EDGE_BLEND_RUN
                && along <= patch.downstream_extent()
                && lateral.abs() <= patch.half_width * 2.5
        })
        .min_by(|a, b| {
            a.local_coordinates(point)
                .1
                .abs()
                .total_cmp(&b.local_coordinates(point).1.abs())
        })
    {
        return patch.direction;
    }
    if let Some(river) = network.rivers.get(owner.river as usize)
        && river.nodes.get(owner.node as usize).is_some()
    {
        return super::channel::river_node_flow_direction(
            terrain,
            &river.nodes,
            owner.node as usize,
        );
    }
    // Refined contacts can outlive an authored node. Recover the local flow
    // axis from the frozen station field rather than animation side signs.
    neighbours
        .iter()
        .filter(|&&i| buffers.owners[i].is_some_and(|other| other.river == owner.river))
        .map(|&i| {
            let edge = terrain.vertices[i].truncate() - point;
            edge * ((stations[i] - stations[source]) / edge.length_squared().max(1.0e-16))
        })
        .sum::<Vec2>()
        .normalize_or_zero()
}

pub(super) fn constraints(
    terrain: &Mesh,
    network: &RiverNetwork,
    buffers: &RiverMeshBuffers,
    patches: &[WaterfallPatch],
    stations: &[f32],
    outlets: &[usize],
) -> Vec<(usize, usize)> {
    let adjacency = terrain.adjacency();
    let perimeter = terrain.perimeter_mask();
    let mut rungs = vec![u8::MAX; terrain.vertices.len()];
    let mut sources = vec![usize::MAX; terrain.vertices.len()];
    let mut frontier = Vec::new();
    for (i, &coverage) in buffers.coverage.iter().enumerate() {
        if super::is_river_boundary(coverage)
            && buffers.owners[i].is_some()
            && buffers.surfaces[i] > 0.0
            && terrain.vertices[i].z > 0.0
            && !perimeter[i]
        {
            rungs[i] = 1;
            sources[i] = i;
            frontier.push(i);
        }
    }
    let mut start = 0;
    for ring in 1..=FULL_RINGS {
        let end = frontier.len();
        for index in start..end {
            let i = frontier[index];
            let source = sources[i];
            let width = buffers.target_half_widths[source];
            for &next in &adjacency[i] {
                if buffers.coverage[next] != 0
                    || perimeter[next]
                    || terrain.vertices[next].z <= 0.0
                    || network.ocean.get(next).copied().unwrap_or(false)
                    || rungs[next] < ring + 1
                    || terrain.vertices[next]
                        .truncate()
                        .distance(terrain.vertices[source].truncate())
                        > width * 1.5
                {
                    continue;
                }
                if rungs[next] == u8::MAX {
                    frontier.push(next);
                } else {
                    let old = sources[next];
                    let distance = |s: usize| {
                        terrain.vertices[next]
                            .truncate()
                            .distance_squared(terrain.vertices[s].truncate())
                    };
                    if distance(old) <= distance(source) {
                        continue;
                    }
                }
                rungs[next] = ring + 1;
                sources[next] = source;
            }
        }
        start = end;
    }
    contour_edges(terrain, &rungs)
        .into_iter()
        .filter_map(|(a, b)| {
            let source_a = sources[a];
            let source_b = sources[b];
            let first = buffers.owners[source_a]?;
            let second = buffers.owners[source_b]?;
            if outlets.get(first.river as usize) != outlets.get(second.river as usize) {
                return None;
            }
            let station = |i: usize, source: usize| {
                stations[source]
                    + (terrain.vertices[i].truncate() - terrain.vertices[source].truncate()).dot(
                        flow_direction(
                            source,
                            terrain,
                            network,
                            buffers,
                            patches,
                            stations,
                            &adjacency[source],
                        ),
                    )
            };
            let delta = station(a, source_a) - station(b, source_b);
            if delta > 1.0e-8 {
                Some((a, b))
            } else if delta < -1.0e-8 {
                Some((b, a))
            } else {
                None
            }
        })
        .collect()
}
