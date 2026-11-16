use super::drainage::Drainage;
use super::tracing::{calculate_flow_and_catchment, drainage_order, trace_drainage};
use super::*;

/// The excluded scaffold triangulates a prescribed graph without adding routes.
fn graph(heights: &[f32], edges: &[(usize, usize)]) -> (Mesh, Adjacency, Vec<bool>, Vec<bool>) {
    let scaffold = heights.len();
    let mut mesh = Mesh::default();
    mesh.vertices.extend(
        heights
            .iter()
            .enumerate()
            .map(|(vertex, &height)| Vec3::new(vertex as f32 * 0.01, 0.0, height)),
    );
    mesh.vertices.push(Vec3::new(0.0, 0.01, 0.0));
    for &(left, right) in edges {
        mesh.triangles
            .extend([left as u32, right as u32, scaffold as u32]);
    }
    let adjacency = mesh.adjacency();
    let mut excluded = vec![false; mesh.vertices.len()];
    excluded[scaffold] = true;
    let mut ocean = vec![false; mesh.vertices.len()];
    ocean[0] = true;
    (mesh, adjacency, ocean, excluded)
}

fn basin() -> (Mesh, Adjacency, Vec<bool>, Vec<bool>) {
    graph(
        &[0.0, 5.0, 2.0, 1.0, 3.0, 8.0],
        &[(0, 1), (1, 2), (2, 3), (3, 4), (0, 5), (5, 3)],
    )
}

#[test]
fn basin_reverses_only_the_valley_branch_to_the_lowest_spill() {
    let (mesh, adjacency, ocean, excluded) = basin();
    let drainage = Drainage::new(&mesh, &adjacency, &ocean, &excluded, 42);
    assert_eq!(drainage.downstream, [0, 0, 1, 2, 3, 3, 6]);
    assert!(drainage.reaches_ocean[..6].iter().all(|&reaches| reaches));
    assert!(!drainage.reaches_ocean[6]);
    assert_eq!(
        drainage_order(&drainage.downstream).len(),
        mesh.vertices.len()
    );
    let (flow, catchment) = calculate_flow_and_catchment(&mesh, &drainage.downstream);
    assert_eq!(flow, [6, 5, 4, 3, 1, 1, 1]);
    let land_area = crate::terrain::projected_vertex_control_areas(&mesh)
        .iter()
        .zip(&mesh.vertices)
        .filter(|(_, vertex)| vertex.z > 0.0)
        .map(|(area, _)| area * ISLAND_WORLD_METRES * ISLAND_WORLD_METRES)
        .sum::<f32>();
    assert!((catchment[0] - land_area).abs() < 0.1);
}

#[test]
fn blocked_low_saddle_uses_an_arbitrarily_higher_allowed_outlet() {
    let (mesh, adjacency, ocean, mut excluded) = basin();
    excluded[1] = true;
    let drainage = Drainage::new(&mesh, &adjacency, &ocean, &excluded, 42);
    assert_eq!(drainage.downstream[3], 5);
    assert_eq!(drainage.downstream[5], 0);
    assert!(drainage.reaches_ocean[4]);
    assert!(!drainage.reaches_ocean[1]);
}

#[test]
fn equal_height_plateau_drains_deterministically_without_cycles() {
    let (mesh, adjacency, ocean, excluded) = graph(
        &[0.0, 1.0, 1.0, 1.0, 1.0],
        &[(0, 1), (1, 2), (2, 3), (3, 4), (1, 4)],
    );
    let first = Drainage::new(&mesh, &adjacency, &ocean, &excluded, 42);
    let second = Drainage::new(&mesh, &adjacency, &ocean, &excluded, 42);
    assert_eq!(first.downstream, second.downstream);
    assert_eq!(drainage_order(&first.downstream).len(), mesh.vertices.len());
    assert!(first.reaches_ocean[..5].iter().all(|&reaches| reaches));
}

#[test]
fn selected_overflow_uses_the_spill_surface_without_excavating_the_valley() {
    let (mesh, adjacency, ocean, excluded) = basin();
    let original = mesh.vertices.clone();
    let drainage = Drainage::new(&mesh, &adjacency, &ocean, &excluded, 42);
    let (flow, catchment) = calculate_flow_and_catchment(&mesh, &drainage.downstream);
    let (rivers, joins) =
        trace_drainage(&mesh, &drainage, &flow, &catchment, &[4, 3, 2, 1], &ocean);
    assert_eq!(rivers.len(), 1);
    assert_eq!(joins, [None]);
    assert_eq!(mesh.vertices, original);
    for river in &rivers {
        assert!(
            river
                .nodes
                .windows(2)
                .all(|pair| pair[0].surface >= pair[1].surface)
        );
        assert!(
            river
                .nodes
                .iter()
                .all(|node| node.surface >= mesh.vertices[node.vertex].z)
        );
        assert!(ocean[river.nodes.last().unwrap().vertex]);
    }
    assert_eq!(mesh.vertices[1].z.to_bits(), 5.0_f32.to_bits());
}

#[test]
fn upstream_extension_keeps_the_spill_grade_without_excavating_the_receiver() {
    let (mesh, adjacency, ocean, excluded) = graph(
        &[0.0, 8.0, 4.0, 2.0, 1.0],
        &[(0, 1), (1, 2), (2, 3), (3, 4)],
    );
    let drainage = Drainage::new(&mesh, &adjacency, &ocean, &excluded, 42);
    let original = mesh.vertices.clone();
    let flow = vec![1; mesh.vertices.len()];
    let (_, catchment) = calculate_flow_and_catchment(&mesh, &drainage.downstream);
    let (rivers, joins) = trace_drainage(&mesh, &drainage, &flow, &catchment, &[2, 4], &ocean);
    assert_eq!(rivers.len(), 1);
    assert_eq!(joins, [None]);
    assert_eq!(rivers[0].nodes.first().unwrap().vertex, 4);
    assert!(rivers.iter().all(|river| {
        river
            .nodes
            .windows(2)
            .all(|pair| pair[0].surface >= pair[1].surface)
    }));
    assert_eq!(mesh.vertices, original);
    assert_eq!(mesh.vertices[1].z.to_bits(), 8.0_f32.to_bits());
}

#[test]
fn absent_or_blocked_ocean_keeps_terrain_unchanged() {
    let (mesh, adjacency, mut ocean, excluded) = basin();
    ocean.fill(false);
    let original = mesh.vertices.clone();
    let drainage = Drainage::new(&mesh, &adjacency, &ocean, &excluded, 42);
    let (rivers, _) = trace_drainage(&mesh, &drainage, &[1; 7], &[0.0; 7], &[4, 3, 2, 1], &ocean);
    assert!(rivers.is_empty());
    assert_eq!(mesh.vertices, original);
}

#[test]
fn largest_true_catchment_owns_the_main_stem_before_smaller_tributaries() {
    let (mesh, _, ocean, _) = graph(
        &[0.0, 1.0, 2.0, 3.0, 4.0, 3.0, 4.0],
        &[(0, 1), (1, 2), (2, 3), (3, 4), (2, 5), (5, 6)],
    );
    let drainage = Drainage {
        downstream: vec![0, 0, 1, 2, 3, 2, 5, 7],
        reaches_ocean: vec![true, true, true, true, true, true, true, false],
    };
    // Vertex counts deliberately favour the smaller-area branch. Vertex 3 is
    // a connecting span, not a qualifying source, and must remain connected.
    let flow = [1000, 900, 800, 10, 1, 700, 699, 1];
    let catchment = [1000.0, 100.0, 90.0, 60.0, 50.0, 30.0, 20.0, 0.0];
    let original = mesh.vertices.clone();
    let (rivers, joins) = trace_drainage(
        &mesh,
        &drainage,
        &flow,
        &catchment,
        &[1, 2, 4, 5, 6],
        &ocean,
    );
    let vertices = |river: &River| {
        river
            .nodes
            .iter()
            .map(|node| node.vertex)
            .collect::<Vec<_>>()
    };

    assert_eq!(rivers.len(), 2);
    assert_eq!(vertices(&rivers[0]), [4, 3, 2, 1, 0]);
    assert_eq!(vertices(&rivers[1]), [6, 5, 2]);
    assert_eq!(rivers[0].join, None);
    assert_eq!(rivers[1].join, Some(0));
    assert_eq!(joins, [None, Some(2)]);
    assert_eq!(mesh.vertices, original);
    assert!(
        rivers
            .iter()
            .flat_map(|river| &river.nodes)
            .all(|node| node.flow == flow[node.vertex])
    );
}

#[test]
fn mouth_first_candidates_form_one_complete_valley_without_two_node_fragments() {
    const COUNT: usize = 64;
    let heights: Vec<_> = (0..COUNT).map(|i| i as f32).collect();
    let edges: Vec<_> = (1..COUNT).map(|i| (i - 1, i)).collect();
    let (mesh, _, ocean, _) = graph(&heights, &edges);
    let drainage = Drainage {
        downstream: (0..=COUNT)
            .map(|i| if i == COUNT { i } else { i.saturating_sub(1) })
            .collect(),
        reaches_ocean: (0..=COUNT).map(|i| i < COUNT).collect(),
    };
    let flow: Vec<_> = (0..=COUNT)
        .map(|i| COUNT.saturating_sub(i) as u32)
        .collect();
    let catchment: Vec<_> = flow.iter().map(|&q| q as f32).collect();
    let candidates: Vec<_> = (1..COUNT).collect();
    let (rivers, joins) = trace_drainage(&mesh, &drainage, &flow, &catchment, &candidates, &ocean);

    assert_eq!(rivers.len(), 1);
    assert_eq!(joins, [None]);
    assert_eq!(rivers[0].nodes.len(), COUNT);
    assert_eq!(rivers[0].nodes[0].vertex, COUNT - 1);
    assert_eq!(rivers[0].nodes.last().unwrap().vertex, 0);
}
