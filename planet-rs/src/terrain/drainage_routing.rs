//! Preserve natural descent inside catchments, then connect their sinks through
//! the lowest available spill to sea. Only each sink-to-spill chain is reversed.
use super::{Drainage, State, SurfaceGraph, arc};
use crate::DVec3;
use std::collections::BinaryHeap;

pub(in crate::terrain) fn route(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &[f64],
) -> Drainage {
    let mut receivers: Vec<_> = heights
        .iter()
        .enumerate()
        .map(|(i, &height)| {
            if height < 0.0 {
                None
            } else {
                graph.steepest(directions, heights, i)
            }
        })
        .collect();
    let (basins, count) = natural_basins(heights, &receivers);
    connect_basins(directions, graph, heights, &basins, count, &mut receivers);
    let order = parent_order(&receivers);
    let mut drainage = Drainage {
        receivers,
        catchment: vec![0.0; heights.len()],
        order,
    };
    drainage.accumulate(graph, heights);
    drainage
}

/// Natural receivers strictly descend, so each receiver's basin is already
/// known in height order. All submerged roots belong to the same sea basin.
fn natural_basins(heights: &[f64], receivers: &[Option<usize>]) -> (Vec<usize>, usize) {
    let mut sorted: Vec<_> = (0..heights.len()).collect();
    sorted.sort_unstable_by(|&a, &b| heights[a].total_cmp(&heights[b]).then_with(|| a.cmp(&b)));
    let mut basins = vec![0; heights.len()];
    let mut count = 1;
    for vertex in sorted {
        if heights[vertex] < 0.0 {
            continue;
        }
        basins[vertex] = if let Some(next) = receivers[vertex] {
            basins[next]
        } else {
            count += 1;
            count - 1
        };
    }
    (basins, count)
}

/// Linked member lists avoid allocating a separate collection for every flat
/// site's basin. All scratch is owned here; sphere topology remains borrowed.
fn members(basins: &[usize], count: usize) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
    let mut heads = vec![None; count];
    let mut next = vec![None; basins.len()];
    for (vertex, &basin) in basins.iter().enumerate() {
        next[vertex] = heads[basin];
        heads[basin] = Some(vertex);
    }
    (heads, next)
}

fn reverse_to_outlet(receivers: &mut [Option<usize>], mut vertex: usize, mut outlet: usize) {
    loop {
        let next = receivers[vertex];
        receivers[vertex] = Some(outlet);
        let Some(next) = next else { break };
        outlet = vertex;
        vertex = next;
    }
}

#[allow(clippy::too_many_lines)] // One minimax traversal owns the linked basin frontier and outlet buffers.
fn connect_basins(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &[f64],
    basins: &[usize],
    count: usize,
    receivers: &mut [Option<usize>],
) {
    let (heads, next_member) = members(basins, count);
    let mut spills = vec![f64::INFINITY; count];
    let mut distances = vec![f64::INFINITY; count];
    let mut outlets = vec![None::<(usize, usize)>; count];
    let mut settled = vec![false; count];
    let mut queue = BinaryHeap::new();
    if heads[0].is_some() {
        spills[0] = 0.0;
        distances[0] = 0.0;
        queue.push(State {
            spill: 0.0,
            distance: 0.0,
            vertex: 0,
        });
    }
    while let Some(state) = queue.pop() {
        let basin = state.vertex;
        if settled[basin] {
            continue;
        }
        settled[basin] = true;
        if let Some((vertex, outlet)) = outlets[basin] {
            reverse_to_outlet(receivers, vertex, outlet);
        }
        let mut member = heads[basin];
        while let Some(i) = member {
            member = next_member[i];
            for &j in &graph.neighbours[i] {
                let target = basins[j];
                if settled[target] {
                    continue;
                }
                let spill = state.spill.max(heights[i]).max(heights[j]);
                let distance = state.distance + arc(directions[i], directions[j]);
                let boundary = (j, i);
                let better = spill
                    .total_cmp(&spills[target])
                    .then_with(|| distance.total_cmp(&distances[target]))
                    .then_with(|| Some(boundary).cmp(&outlets[target]));
                if better.is_lt() {
                    spills[target] = spill;
                    distances[target] = distance;
                    outlets[target] = Some(boundary);
                    queue.push(State {
                        spill,
                        distance,
                        vertex: target,
                    });
                }
            }
        }
    }
    // A closed dry world, or a disconnected dry component, has no sea outlet.
    for (vertex, receiver) in receivers.iter_mut().enumerate() {
        if !settled[basins[vertex]] {
            *receiver = None;
        }
    }
}

/// Derive accumulation order from the final receiver forest, rather than from
/// flood settlement: reversed spill chains can have a different terrain order.
pub(super) fn parent_order(receivers: &[Option<usize>]) -> Vec<usize> {
    let mut incoming = vec![0_usize; receivers.len()];
    for &target in receivers.iter().flatten() {
        incoming[target] += 1;
    }
    let mut order: Vec<_> = incoming
        .iter()
        .enumerate()
        .filter_map(|(i, &count)| (count == 0).then_some(i))
        .collect();
    let mut cursor = 0;
    while cursor < order.len() {
        let vertex = order[cursor];
        cursor += 1;
        if let Some(target) = receivers[vertex] {
            incoming[target] -= 1;
            if incoming[target] == 0 {
                order.push(target);
            }
        }
    }
    debug_assert_eq!(
        order.len(),
        receivers.len(),
        "basin outlets must form a forest"
    );
    order.reverse();
    order
}

#[cfg(test)]
#[path = "drainage_routing_tests.rs"]
mod tests;
