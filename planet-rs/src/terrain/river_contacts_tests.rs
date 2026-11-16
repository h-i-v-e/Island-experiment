use super::*;
use crate::terrain::{
    drainage::{connect_contacts, repair_with_sources, route},
    river_profile,
};

fn parallel() -> (Vec<DVec3>, SurfaceGraph, Vec<f64>, TerrainOptions) {
    let directions = [
        (0.006, 0.0),
        (0.004, 0.0),
        (0.002, 0.0),
        (0.006, 0.001),
        (0.004, 0.001),
        (0.002, 0.001),
    ]
    .map(|(x, y)| DVec3::new(x, y, 1.0).normalize())
    .to_vec();
    let graph = SurfaceGraph {
        neighbours: vec![
            vec![1, 3],
            vec![0, 2, 4],
            vec![1, 5],
            vec![0, 4],
            vec![3, 5, 1],
            vec![4, 2],
        ],
        areas: vec![0.0, 0.1, 0.8, 0.05, 0.05, 0.1],
    };
    let heights = vec![-0.01, 0.004, 0.006, 0.002, 0.005, 0.007];
    let settings = TerrainOptions {
        river_width: 0.0008,
        river_depth: 0.0001,
        river_taper: 0.0,
        river_catchment: 0.04 / (4.0 * std::f64::consts::PI),
        ..TerrainOptions::default()
    };
    (directions, graph, heights, settings)
}

fn selected(
    directions: &[DVec3],
    graph: &SurfaceGraph,
    heights: &[f64],
    drainage: &Drainage,
    settings: &TerrainOptions,
) -> Vec<RiverSegment> {
    super::super::river_sources::select(directions, graph, heights, drainage, settings, &[])
}

#[test]
fn a_contact_cannot_raise_the_existing_downstream_spill_head() {
    let (directions, graph, heights, settings) = parallel();
    let mut drainage = route(&directions, &graph, &heights);
    let before = drainage.receivers.clone();
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    assert!(!connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers
    ));
    assert_eq!(drainage.receivers, before);
}

#[test]
fn separate_channels_cannot_connect_through_their_shared_sea_root() {
    let (directions, mut graph, heights, settings) = parallel();
    // With crossing edges removed, the only graph connector traverses the sea.
    // Width contact must preserve the two actual ocean mouths.
    graph.neighbours[1].retain(|&v| v != 4);
    graph.neighbours[4].retain(|&v| v != 1);
    graph.neighbours[2].retain(|&v| v != 5);
    graph.neighbours[5].retain(|&v| v != 2);
    let mut drainage = route(&directions, &graph, &heights);
    let before = drainage.receivers.clone();
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    assert!(!connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers
    ));
    assert_eq!(drainage.receivers, before);
}

#[test]
fn a_second_contact_plan_does_not_paint_or_duplicate_catchment() {
    let (directions, graph, heights, settings, mut drainage) = supplied_parallel();
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers,
    );
    let total = drainage.catchment[0];
    let forest = drainage.receivers.clone();
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers,
    );
    assert_eq!(
        forest, drainage.receivers,
        "a fresh planning call must preserve accepted joins"
    );
    assert!((drainage.catchment[0] - total).abs() < 1e-12);
    for start in 0..heights.len() {
        let mut current = start;
        for _ in &heights {
            let Some(next) = drainage.receivers[current] else {
                break;
            };
            current = next;
        }
        assert!(
            heights[current] < 0.0,
            "contact forest must still reach sea"
        );
    }
}

/// Two separate centre lines with a common sea root and one physical crossing.
/// Deliberately supply the sea-connected forest: routing must not be allowed to
/// choose the prospective contact before the contact planner is exercised.
fn supplied_parallel() -> (Vec<DVec3>, SurfaceGraph, Vec<f64>, TerrainOptions, Drainage) {
    let directions = [
        (0., 0.),
        (0.002, 0.),
        (0.004, 0.),
        (0.006, 0.),
        (0.002, 0.001),
        (0.004, 0.001),
        (0.006, 0.001),
    ]
    .map(|(x, y)| DVec3::new(x, y, 1.).normalize())
    .to_vec();
    let graph = SurfaceGraph {
        neighbours: vec![
            vec![1, 4],
            vec![0, 2],
            vec![1, 3, 5],
            vec![2],
            vec![0, 5],
            vec![4, 6, 2],
            vec![5],
        ],
        areas: vec![0., 0.001, 0., 0.7, 0., 0., 0.1],
    };
    let heights = vec![-0.01, 0.004, 0.006, 0.008, 0.002, 0.003, 0.005];
    let options = TerrainOptions {
        river_width: 0.0008,
        river_depth: 0.0001,
        river_taper: 0.,
        river_catchment: 0.04 / (4. * std::f64::consts::PI),
        ..TerrainOptions::default()
    };
    let mut drainage = Drainage {
        receivers: vec![None, Some(0), Some(1), Some(2), Some(0), Some(4), Some(5)],
        catchment: vec![0.; 7],
        order: vec![0, 1, 2, 3, 4, 5, 6],
    };
    drainage.accumulate(&graph, &heights);
    (directions, graph, heights, options, drainage)
}

fn assert_sea_forest(drainage: &Drainage, graph: &SurfaceGraph, heights: &[f64]) {
    for start in 0..heights.len() {
        let mut current = start;
        let mut visited = vec![false; heights.len()];
        while let Some(next) = drainage.receivers[current] {
            assert!(!visited[current], "cycle starting at {start}");
            visited[current] = true;
            assert!(graph.neighbours[current].contains(&next));
            current = next;
        }
        assert!(heights[current] < 0., "lost outlet at {current}");
    }
    let area: f64 = graph
        .areas
        .iter()
        .zip(heights)
        .filter(|(_, h)| **h >= 0.)
        .map(|(a, _)| a)
        .sum();
    assert!((drainage.catchment[0] - area).abs() < 1e-12);
}

#[test]
fn higher_main_captures_the_lower_touching_channel_without_lowering_its_upstream_grade() {
    let (directions, graph, mut heights, settings, mut drainage) = supplied_parallel();
    let original_heights = heights.clone();
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    assert!(connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers
    ));
    assert_eq!(
        drainage.receivers[2],
        Some(5),
        "the higher main must run downhill into the touching lower channel"
    );
    assert_eq!(drainage.receivers[3], Some(2));
    assert_eq!(drainage.receivers[5], Some(4));
    assert_sea_forest(&drainage, &graph, &heights);
    let mut soil = vec![0.; heights.len()];
    let rivers = repair_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut soil,
        &settings,
        &mut drainage,
        &[],
    )
    .0;
    assert_eq!(heights[3].to_bits(), original_heights[3].to_bits());
    let mesh = crate::PlanetMesh {
        directions,
        triangles: vec![],
        radius: 2000.,
    };
    let profile = river_profile::build(&mesh, &heights, &drainage, &rivers, &settings);
    assert!(profile.levels[2] >= profile.levels[5]);
    assert!(profile.levels[6] >= profile.levels[5]);
    assert!((drainage.catchment[5] - 0.8).abs() < 1e-12);
    assert!(
        !rivers.iter().any(|river| river.from == 1),
        "the abandoned suffix has only below-threshold real runoff"
    );
}

#[test]
fn a_true_three_edge_contact_uses_each_land_connector_and_conserves_flow() {
    let (mut directions, mut graph, mut heights, settings, mut drainage) = supplied_parallel();
    graph.neighbours[2].retain(|&v| v != 5);
    graph.neighbours[5].retain(|&v| v != 2);
    for (y, height, to) in [(0.0003, 0.0038, 2), (0.0007, 0.0035, 5)] {
        directions.push(DVec3::new(0.004, y, 1.).normalize());
        graph.neighbours.push(vec![to]);
        graph.areas.push(0.);
        heights.push(height);
        drainage.receivers.push(Some(to));
        drainage.catchment.push(0.);
    }
    graph.neighbours[2].push(7);
    graph.neighbours[7].push(8);
    graph.neighbours[8].push(7);
    graph.neighbours[5].push(8);
    drainage.order = super::super::routing::parent_order(&drainage.receivers);
    drainage.accumulate(&graph, &heights);
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    assert!(connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers
    ));
    assert_eq!(
        [
            drainage.receivers[2],
            drainage.receivers[7],
            drainage.receivers[8]
        ],
        [Some(7), Some(8), Some(5)]
    );
    assert_sea_forest(&drainage, &graph, &heights);
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    assert!(!connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers
    ));
}

#[test]
fn a_real_high_ridge_blocks_a_width_contact_instead_of_becoming_a_channel() {
    let (mut directions, mut graph, mut heights, settings, mut drainage) = supplied_parallel();
    graph.neighbours[2].retain(|&v| v != 5);
    graph.neighbours[5].retain(|&v| v != 2);
    directions.push(DVec3::new(0.004, 0.0005, 1.).normalize());
    heights.push(0.02);
    graph.neighbours.push(vec![2, 5]);
    graph.areas.push(0.);
    graph.neighbours[2].push(7);
    graph.neighbours[5].push(7);
    drainage.receivers.push(Some(2));
    drainage.catchment.push(0.);
    drainage.order = super::super::routing::parent_order(&drainage.receivers);
    drainage.accumulate(&graph, &heights);
    let before = drainage.receivers.clone();
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    assert!(!connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers
    ));
    assert_eq!(before, drainage.receivers);
    assert_sea_forest(&drainage, &graph, &heights);

    let mesh = crate::PlanetMesh {
        directions,
        triangles: vec![],
        radius: 2000.0,
    };
    let profile = river_profile::build(&mesh, &heights, &drainage, &rivers, &settings);
    let widths = river_widths::build(&mesh, &drainage, &rivers, &settings);
    let footprints = crate::terrain::channels::Footprints::with_widths(
        &mesh,
        &rivers,
        &profile.levels,
        &settings,
        0.0,
        &widths,
    );
    let (target, core) = footprints.bed_target(mesh.directions[7], heights[7]);
    assert_eq!(
        target.to_bits(),
        heights[7].to_bits(),
        "overlapping foreign widths must not cut the rejected ridge"
    );
    assert!(!core);
    let mut shaped = heights.clone();
    let mut soil = vec![0.0; heights.len()];
    footprints.carve(&mesh, &mut shaped, &mut soil);
    let pins = footprints.bank_pins(&mesh, &graph, &shaped);
    assert!(!pins.core[7]);
    pins.lift(&mesh, &graph, &heights, &mut shaped, &mut soil);
    footprints.relax_bed(&mesh, &graph, &mut shaped, &mut soil, &pins);
    pins.relax_river_support(&graph, &drainage, &mut shaped, &mut soil);
    assert!(
        shaped[7] >= heights[7],
        "bank shaping must preserve the divide"
    );
    let water = pins.into_water(&mesh, &shaped);
    assert!(
        water[7] < shaped[7],
        "the ridge must remain dry in the final clip fields"
    );
}

#[test]
fn narrow_channels_do_not_join_merely_because_their_graph_vertices_are_adjacent() {
    let (directions, graph, heights, mut settings, mut drainage) = supplied_parallel();
    settings.river_width = 0.00001;
    let before = drainage.receivers.clone();
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    assert!(!connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers
    ));
    assert_eq!(drainage.receivers, before);
}

fn planner<'a>(
    directions: &'a [DVec3],
    graph: &'a SurfaceGraph,
    heights: &'a [f64],
    receivers: &'a mut [Option<usize>],
    plan: &'a mut ContactPlan,
) -> Planner<'a> {
    let count = directions.len();
    Planner {
        directions,
        graph,
        heights,
        receivers,
        plan,
        grades: heights.to_vec(),
        widths: vec![0.001; count],
        offsets: vec![0.; count],
        owner: vec![UNOWNED; count],
        segments: vec![],
        index: FeatureIndex::new(std::iter::empty()),
        search: Search::new(count),
        maximum_width: 0.001,
        registered: vec![None; count],
    }
}

#[test]
fn a_capture_cannot_close_a_loop_through_the_recipient_suffix() {
    let (directions, graph, heights, _, mut drainage) = supplied_parallel();
    drainage.receivers[5] = Some(2);
    let mut plan = ContactPlan::new(heights.len());
    let mut planner = planner(
        &directions,
        &graph,
        &heights,
        &mut drainage.receivers,
        &mut plan,
    );
    assert!(planner.would_cycle(&[2, 5]));
    assert!(planner.would_cycle(&[3, 2, 6]));
    assert!(!planner.would_cycle(&[2, 4]));
}

#[test]
fn a_connector_edge_cannot_cross_actual_ocean_between_dry_endpoints() {
    let directions = [
        DVec3::new(-0.01, 0., 1.).normalize(),
        DVec3::new(0.01, 0., 1.).normalize(),
    ];
    let graph = SurfaceGraph {
        neighbours: vec![vec![1], vec![0]],
        areas: vec![1.; 2],
    };
    let mut receivers = [Some(1), None];
    let mut plan = ContactPlan::new(2);
    let shallow = [0.00001; 2];
    let low = planner(&directions, &graph, &shallow, &mut receivers, &mut plan);
    assert!(!low.edge_is_land(0, 1));
    let dry = [0.001; 2];
    let high = planner(&directions, &graph, &dry, &mut receivers, &mut plan);
    assert!(high.edge_is_land(0, 1));
}

fn width_feedback() -> (Vec<DVec3>, SurfaceGraph, Vec<f64>, TerrainOptions, Drainage) {
    let directions = [
        (0.009, 0.0003),
        (0.003, 0.),
        (0.006, 0.),
        (0.003, 0.0003),
        (0.006, 0.0003),
        (0.003, 0.00064),
        (0.006, 0.00064),
    ]
    .map(|(x, y)| DVec3::new(x, y, 1.).normalize())
    .to_vec();
    let graph = SurfaceGraph {
        neighbours: vec![
            vec![2, 4, 6],
            vec![2, 3],
            vec![1, 0],
            vec![4, 1, 5],
            vec![3, 0],
            vec![6, 3],
            vec![5, 0],
        ],
        areas: vec![0., 0.003, 0., 0.002, 0., 0.002, 0.],
    };
    let heights = vec![-0.01, 0.02, 0.019, 0.015, 0.014, 0.01, 0.009];
    let settings = TerrainOptions {
        river_width: 0.0001,
        river_depth: 0.00001,
        river_taper: 0.,
        river_catchment: 0.001 / (4. * std::f64::consts::PI),
        ..TerrainOptions::default()
    };
    let mut drainage = Drainage {
        receivers: vec![None, Some(2), Some(0), Some(4), Some(0), Some(6), Some(0)],
        catchment: vec![0.; 7],
        order: vec![0, 2, 1, 4, 3, 6, 5],
    };
    drainage.accumulate(&graph, &heights);
    (directions, graph, heights, settings, drainage)
}

#[test]
fn catchment_width_feedback_discovers_a_second_contact_and_then_is_idempotent() {
    let (directions, graph, heights, settings, mut drainage) = width_feedback();
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    let mut plan = ContactPlan::new(heights.len());
    assert_eq!(
        join(
            &directions,
            &graph,
            &heights,
            &settings,
            &mut drainage,
            &rivers,
            &mut plan,
        ),
        1
    );
    assert_eq!(drainage.receivers[1], Some(3));
    assert_eq!(
        drainage.receivers[3],
        Some(4),
        "third channel starts outside the initial widths"
    );
    drainage.order = super::super::routing::parent_order(&drainage.receivers);
    drainage.accumulate(&graph, &heights);
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    assert_eq!(
        join(
            &directions,
            &graph,
            &heights,
            &settings,
            &mut drainage,
            &rivers,
            &mut plan
        ),
        1
    );
    drainage.order = super::super::routing::parent_order(&drainage.receivers);
    drainage.accumulate(&graph, &heights);
    assert_eq!(
        drainage.receivers[3],
        Some(5),
        "the enlarged true catchment reveals a new physical touch"
    );
    assert_sea_forest(&drainage, &graph, &heights);
    let final_forest = drainage.receivers.clone();
    let rivers = selected(&directions, &graph, &heights, &drainage, &settings);
    assert_eq!(
        join(
            &directions,
            &graph,
            &heights,
            &settings,
            &mut drainage,
            &rivers,
            &mut plan
        ),
        0
    );
    assert!(!connect_contacts(
        &directions,
        &graph,
        &heights,
        &settings,
        &mut drainage,
        &rivers
    ));
    assert_eq!(drainage.receivers, final_forest);
}
