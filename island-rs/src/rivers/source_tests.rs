use super::tracing::{find_sources, prioritize_inherited_sources, trace_rivers_avoiding};
use super::*;

/// Separate triangulated channels prevent confluences from hiding source counts.
fn separate_channels(count: usize) -> (Mesh, Adjacency, Vec<bool>) {
    let mut mesh = Mesh::default();
    let mut ocean = Vec::with_capacity(count * 4);
    for channel in 0..count {
        let x = channel as f32 * 0.1;
        let start = mesh.vertices.len() as u32;
        mesh.vertices.extend([
            Vec3::new(x, 0.0, 0.2),
            Vec3::new(x + 0.01, 0.0, 0.1),
            Vec3::new(x + 0.02, 0.0, 0.0),
            Vec3::new(x + 0.01, -0.01, 0.3),
        ]);
        mesh.triangles
            .extend([start, start + 1, start + 3, start + 1, start + 2, start + 3]);
        ocean.extend([false, false, true, false]);
    }
    let adjacency = mesh.adjacency();
    (mesh, adjacency, ocean)
}

#[test]
fn excluded_candidates_do_not_starve_later_valid_sources() {
    let (mut mesh, adjacency, ocean) = separate_channels(101);
    let downstream = tracing::map_downstream(&mesh, &adjacency);
    let catchments = (0..mesh.vertices.len())
        .map(|vertex| if vertex % 4 == 0 { 20_000.0 } else { 0.0 })
        .collect::<Vec<_>>();
    let sources = find_sources(
        &mesh,
        &adjacency,
        &downstream,
        &catchments,
        RiverSourceRule::new(1.0, 1.0, 0.0, 0.2),
    );
    assert_eq!(sources.len(), 101);
    assert_eq!(
        sources,
        (0..101).map(|channel| channel * 4).collect::<Vec<_>>()
    );
    let flow = vec![1; mesh.vertices.len()];
    let mut excluded = vec![false; mesh.vertices.len()];
    excluded[..400].fill(true);

    let (rivers, _) =
        trace_rivers_avoiding(&mut mesh, &adjacency, &flow, &sources, &ocean, 0, &excluded);

    assert_eq!(rivers.len(), 1);
    assert_eq!(rivers[0].nodes[0].vertex, 400);
}

#[test]
fn failed_candidates_do_not_consume_the_accepted_river_budget() {
    let (mut mesh, adjacency, mut ocean) = separate_channels(101);
    ocean[..400].fill(false);
    let flow = vec![1; mesh.vertices.len()];
    let sources = (0..101).map(|channel| channel * 4).collect::<Vec<_>>();
    let (rivers, _) = trace_rivers_avoiding(&mut mesh, &adjacency, &flow, &sources, &ocean, 0, &[]);

    assert_eq!(rivers.len(), 1);
    assert_eq!(rivers[0].nodes[0].vertex, 400);
}

#[test]
fn tracing_caps_accepted_rivers_deterministically() {
    let (mut mesh, adjacency, ocean) = separate_channels(101);
    let flow = vec![1; mesh.vertices.len()];
    let sources = (0..101).map(|channel| channel * 4).collect::<Vec<_>>();
    let (first, _) = trace_rivers_avoiding(&mut mesh, &adjacency, &flow, &sources, &ocean, 7, &[]);
    let (second, _) = trace_rivers_avoiding(&mut mesh, &adjacency, &flow, &sources, &ocean, 7, &[]);
    let starts = |rivers: &[River]| {
        rivers
            .iter()
            .map(|river| river.nodes[0].vertex)
            .collect::<Vec<_>>()
    };

    assert_eq!(first.len(), 96);
    assert_eq!(
        starts(&first),
        (0..96).map(|channel| channel * 4).collect::<Vec<_>>()
    );
    assert_eq!(starts(&first), starts(&second));
}

#[test]
fn inherited_sources_are_prioritized_and_deduplicated() {
    let (mesh, adjacency, _) = separate_channels(3);
    let anchor = mesh.vertices[8].truncate() + Vec2::new(0.001, 0.0);

    let sources =
        prioritize_inherited_sources(&mesh, &adjacency, &[0, 4, 8], &[anchor, anchor], &[]);

    assert_eq!(sources, [8, 0, 4]);
}

#[test]
fn invalid_inherited_anchors_do_not_relocate_to_other_channels() {
    let (mesh, adjacency, _) = separate_channels(3);
    let mut excluded = vec![false; mesh.vertices.len()];
    excluded[4] = true;
    let anchors = [
        mesh.vertices[4].truncate(),
        mesh.vertices[2].truncate(),
        Vec2::splat(10.0),
        Vec2::new(f32::NAN, 0.0),
    ];

    let sources = prioritize_inherited_sources(&mesh, &adjacency, &[], &anchors, &excluded);

    assert!(sources.is_empty());
}

#[test]
fn inherited_generation_requires_a_current_outlet_route() {
    let (mut mesh, adjacency, mut ocean) = separate_channels(3);
    ocean[6] = false;
    let anchors = [8, 4, 0].map(|vertex| mesh.vertices[vertex].truncate());
    let excluded = vec![false; mesh.vertices.len()];
    let network = RiverNetwork::generate_with_ocean_avoiding_sources(
        &mut mesh,
        &adjacency,
        RiverSourceRule::new(1.0e9, 1.0, 0.0, 0.2),
        ocean,
        0,
        &excluded,
        &anchors,
    );

    assert_eq!(
        network.source_positions().collect::<Vec<_>>(),
        [anchors[0], anchors[2]]
    );
    assert!(network.rivers.iter().all(|river| {
        river.join.is_some()
            || river
                .nodes
                .last()
                .is_some_and(|node| network.ocean[node.vertex])
    }));
}
