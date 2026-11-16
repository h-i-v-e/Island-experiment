use super::*;
use crate::terrain::drainage::{
    carve_with_sources, lift_banks_with_sources, repair, repair_with_sources, route,
};

fn confluence() -> (Vec<DVec3>, SurfaceGraph, Vec<f64>) {
    let directions = (0..4)
        .map(|i| DVec3::new(f64::from(i) * 0.01, 0.0, 1.0).normalize())
        .collect();
    let graph = SurfaceGraph {
        neighbours: vec![vec![1], vec![0, 2, 3], vec![1], vec![1]],
        areas: vec![0.1, 0.2, 0.3, 0.4],
    };
    (directions, graph, vec![-0.1, 0.1, 0.2, 0.3])
}

fn qualifying_threshold() -> TerrainOptions {
    TerrainOptions {
        river_catchment: 0.01,
        ..TerrainOptions::default()
    }
}

fn impossible_threshold() -> TerrainOptions {
    TerrainOptions {
        river_catchment: 1.0e6,
        ..TerrainOptions::default()
    }
}

#[test]
fn qualifying_inherited_paths_reconverge_once_without_changing_rainfall() {
    let (directions, graph, mut heights) = confluence();
    let mut soil = vec![0.1; 4];
    let before = route(&directions, &graph, &heights);
    let anchors = [directions[3], directions[2], directions[3]];
    let (drainage, rivers, _) = carve_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut soil,
        &qualifying_threshold(),
        &anchors,
    );
    assert_eq!(
        rivers
            .iter()
            .map(|river| (river.from, river.to))
            .collect::<Vec<_>>(),
        [(1, 0), (2, 1), (3, 1)]
    );
    assert_eq!(drainage.catchment, before.catchment);
    assert!(
        rivers
            .iter()
            .all(|river| river.catchment.to_bits() == before.catchment[river.from].to_bits())
    );
    let mut retained = Vec::new();
    retain_sources(&mut retained, &directions, &rivers);
    assert_eq!(retained, [directions[2], directions[3]]);
}

#[test]
fn catchment_and_inherited_rivers_remain_uncapped_above_ninety_six() {
    let count = 120_u32;
    let mut directions = vec![DVec3::Y];
    directions.extend((0..count).map(|i| {
        let angle = f64::from(i) * std::f64::consts::TAU / f64::from(count);
        DVec3::new(angle.cos(), 0.0, angle.sin())
    }));
    let mut neighbours = vec![(1..directions.len()).collect::<Vec<_>>()];
    neighbours.extend((0..count).map(|_| vec![0]));
    let graph = SurfaceGraph {
        neighbours,
        areas: vec![0.01; directions.len()],
    };
    let mut heights = vec![0.2; directions.len()];
    heights[0] = -0.1;
    let drainage = route(&directions, &graph, &heights);
    let natural = select(
        &directions,
        &graph,
        &heights,
        &drainage,
        &TerrainOptions {
            river_catchment: 1.0e-6,
            ..TerrainOptions::default()
        },
        &[],
    );
    let inherited = select(
        &directions,
        &graph,
        &heights,
        &drainage,
        &TerrainOptions {
            river_catchment: 1.0e-6,
            ..TerrainOptions::default()
        },
        &directions[1..],
    );
    assert_eq!(natural.len(), 120);
    assert_eq!(natural, inherited);
    assert!(
        select(
            &directions,
            &graph,
            &heights,
            &drainage,
            &impossible_threshold(),
            &directions[1..]
        )
        .is_empty()
    );
}

#[test]
fn disconnected_dry_cycle_and_invalid_receiver_paths_are_rejected() {
    let (directions, mut graph, heights) = confluence();
    let options = qualifying_threshold();
    let valid = route(&directions, &graph, &heights);
    let mut broken = Drainage {
        receivers: valid.receivers.clone(),
        catchment: valid.catchment.clone(),
        order: valid.order.clone(),
    };
    broken.receivers[1] = None;
    assert!(
        select(
            &directions,
            &graph,
            &heights,
            &broken,
            &options,
            &directions[2..]
        )
        .is_empty()
    );
    broken.receivers[1] = Some(2);
    assert!(
        select(
            &directions,
            &graph,
            &heights,
            &broken,
            &options,
            &directions[2..]
        )
        .is_empty()
    );
    broken.receivers[1] = Some(0);
    graph.neighbours[2].clear();
    assert!(
        select(
            &directions,
            &graph,
            &heights,
            &broken,
            &options,
            &[directions[2]]
        )
        .iter()
        .all(|river| river.from != 2)
    );
    let dry = vec![0.1; 4];
    let drainage = route(&directions, &graph, &dry);
    assert!(
        select(
            &directions,
            &graph,
            &dry,
            &drainage,
            &options,
            &directions[2..]
        )
        .is_empty()
    );
}

#[test]
fn remapping_follows_directions_after_vertex_reordering_and_rejects_submerged_nearest() {
    let (directions, graph, heights) = confluence();
    let anchors = [directions[2]];
    let order = [2, 0, 3, 1];
    let mut inverse = [0; 4];
    for (new, &old) in order.iter().enumerate() {
        inverse[old] = new;
    }
    let directions = order.map(|old| directions[old]);
    let graph = SurfaceGraph {
        neighbours: order
            .iter()
            .map(|&old| {
                graph.neighbours[old]
                    .iter()
                    .map(|&next| inverse[next])
                    .collect()
            })
            .collect(),
        areas: order.map(|old| graph.areas[old]).to_vec(),
    };
    let mut heights = order.map(|old| heights[old]);
    assert_eq!(
        remap(&directions, &graph, &anchors).collect::<Vec<_>>(),
        [inverse[2]]
    );
    heights[inverse[2]] = -0.05;
    let drainage = route(&directions, &graph, &heights);
    let rivers = select(
        &directions,
        &graph,
        &heights,
        &drainage,
        &qualifying_threshold(),
        &anchors,
    );
    assert!(rivers.iter().all(|river| river.from != inverse[2]));
}

#[test]
fn spherical_remapping_crosses_meridian_and_pole_bins_without_antipodal_matches() {
    let eps = 1.0e-5;
    let directions = [
        DVec3::new(-1.0, 0.0, eps).normalize(),
        DVec3::new(eps, 1.0, 0.0).normalize(),
        DVec3::new(-1.0, 0.0, 0.01).normalize(),
        DVec3::new(0.01, 1.0, 0.0).normalize(),
    ];
    let graph = SurfaceGraph {
        neighbours: vec![vec![2], vec![3], vec![0], vec![1]],
        areas: vec![0.1; 4],
    };
    let anchors = [
        DVec3::new(-1.0, 0.0, -eps).normalize(),
        DVec3::new(-eps, 1.0, 0.0).normalize(),
    ];
    assert_eq!(
        remap(&directions, &graph, &anchors).collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(
        remap(
            &directions,
            &graph,
            &[
                DVec3::X,
                -DVec3::Y,
                DVec3::ZERO,
                DVec3::splat(2.0),
                DVec3::splat(f64::NAN)
            ]
        )
        .next()
        .is_none()
    );
}

#[test]
fn bank_lifting_keeps_qualifying_sources_and_true_area() {
    let (directions, graph, mut heights) = confluence();
    let expected = route(&directions, &graph, &heights).catchment;
    let (drainage, rivers) = lift_banks_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut [0.0; 4],
        &[],
        &qualifying_threshold(),
        &[directions[2]],
    )
    .unwrap();
    assert_eq!(rivers.len(), 3);
    assert!(
        drainage
            .catchment
            .iter()
            .zip(&expected)
            .all(|(actual, expected)| (actual - expected).abs() < 1e-12)
    );
    assert!((drainage.catchment[0] - 0.9).abs() < 1.0e-12);
}

#[test]
fn repeated_repairs_cannot_bypass_threshold_with_inherited_membership() {
    let (directions, graph, mut heights) = confluence();
    let mut drainage = route(&directions, &graph, &heights);
    let expected = drainage.catchment.clone();
    let mut soil = vec![0.0; 4];
    let options = impossible_threshold();
    let (empty, _) = repair(
        &directions,
        &graph,
        &mut heights,
        &mut soil,
        &options,
        &mut drainage,
    );
    assert!(empty.is_empty());
    for _ in 0..2 {
        let (rivers, _) = repair_with_sources(
            &directions,
            &graph,
            &mut heights,
            &mut soil,
            &options,
            &mut drainage,
            &[directions[2]],
        );
        assert!(rivers.is_empty());
        assert_eq!(drainage.catchment, expected);
    }
}

fn priority_tree() -> (Vec<DVec3>, SurfaceGraph, Vec<f64>, Drainage) {
    let directions: Vec<_> = (0..7)
        .map(|i| DVec3::new(f64::from(i) * 0.01, 0.0, 1.0).normalize())
        .collect();
    let graph = SurfaceGraph {
        neighbours: vec![
            vec![1],
            vec![0, 2, 3],
            vec![1, 4, 5],
            vec![1, 6],
            vec![2],
            vec![2],
            vec![3],
        ],
        areas: vec![0.0, 0.0, 0.0, 0.1, 0.6, 0.2, 0.1],
    };
    let heights = vec![-0.1, 0.1, 0.2, 0.2, 0.3, 0.3, 0.3];
    let drainage = route(&directions, &graph, &heights);
    (directions, graph, heights, drainage)
}

#[test]
fn largest_catchments_get_priority_and_anchors_only_break_equal_area_ties() {
    let (_, _, _, drainage) = priority_tree();
    let order = candidate_order(&drainage, &[true; 7], &[false; 7], 0.09);
    assert_eq!(order, [1, 2, 4, 3, 5, 6]);
    let mut inherited = [false; 7];
    inherited[5] = true;
    assert_eq!(
        candidate_order(&drainage, &[true; 7], &inherited, 0.09),
        [1, 2, 4, 5, 3, 6]
    );
}

#[test]
fn priority_main_stem_extends_to_its_strongest_qualifying_headwater() {
    let (_, _, _, drainage) = priority_tree();
    let order = candidate_order(&drainage, &[true; 7], &[false; 7], 0.09);
    let upstream = strongest_upstream(&order, &drainage.receivers);
    assert_eq!(upstream[1], Some(2));
    assert_eq!(upstream[2], Some(4));
    assert_eq!(upstream[4], None);
    assert_eq!(upstream[3], Some(6));
}

#[test]
fn descending_main_stem_coverage_keeps_every_lesser_branch_once() {
    let (directions, graph, heights, drainage) = priority_tree();
    let rivers = select(
        &directions,
        &graph,
        &heights,
        &drainage,
        &TerrainOptions {
            river_catchment: 0.09 / (4.0 * std::f64::consts::PI),
            ..TerrainOptions::default()
        },
        &[],
    );
    assert_eq!(
        rivers.iter().map(|river| river.from).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    let mut sources = Vec::new();
    retain_sources(&mut sources, &directions, &rivers);
    assert_eq!(sources, directions[4..]);
    assert!(
        rivers
            .iter()
            .all(|river| river.catchment.to_bits() == drainage.catchment[river.from].to_bits())
    );
}
