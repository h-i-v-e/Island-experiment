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

fn impossible_threshold() -> TerrainOptions {
    TerrainOptions {
        river_catchment: 1.0e6,
        ..TerrainOptions::default()
    }
}

#[test]
fn inherited_paths_survive_impossible_threshold_and_reconverge_once() {
    let (directions, graph, mut heights) = confluence();
    let mut soil = vec![0.1; 4];
    let before = route(&directions, &graph, &heights);
    let anchors = [directions[3], directions[2], directions[3]];
    let (drainage, rivers, _) = carve_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut soil,
        &impossible_threshold(),
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
        &impossible_threshold(),
        &directions[1..],
    );
    assert_eq!(natural.len(), 120);
    assert_eq!(natural, inherited);
}

#[test]
fn disconnected_dry_cycle_and_invalid_receiver_paths_are_rejected() {
    let (directions, mut graph, heights) = confluence();
    let options = impossible_threshold();
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
        .is_empty()
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
    let drainage = route(&directions, &graph, &heights);
    let rivers = select(
        &directions,
        &graph,
        &heights,
        &drainage,
        &impossible_threshold(),
        &anchors,
    );
    let mut retained = Vec::new();
    retain_sources(&mut retained, &directions, &rivers);
    assert_eq!(retained, anchors);
    heights[inverse[2]] = -0.05;
    let drainage = route(&directions, &graph, &heights);
    assert!(
        select(
            &directions,
            &graph,
            &heights,
            &drainage,
            &impossible_threshold(),
            &anchors
        )
        .is_empty()
    );
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
fn bank_lifting_keeps_inherited_sources_and_true_area() {
    let (directions, graph, mut heights) = confluence();
    let expected = route(&directions, &graph, &heights).catchment;
    let (drainage, rivers) = lift_banks_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut [0.0; 4],
        &[],
        &impossible_threshold(),
        &[directions[2]],
    )
    .unwrap();
    assert_eq!(rivers.len(), 2);
    assert_eq!(drainage.catchment, expected);
    assert!((drainage.catchment[0] - 0.9).abs() < 1.0e-12);
}

#[test]
fn repeated_repairs_keep_inherited_membership_without_changing_catchment() {
    let (directions, graph, mut heights) = confluence();
    let mut drainage = route(&directions, &graph, &heights);
    let expected = drainage.catchment.clone();
    let mut soil = vec![0.0; 4];
    let mut depths = vec![0.0; 4];
    let options = impossible_threshold();
    let (empty, _) = repair(
        &directions,
        &graph,
        &mut heights,
        &mut soil,
        &options,
        &mut drainage,
        &mut depths,
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
            &mut depths,
            &[directions[2]],
        );
        assert_eq!(rivers.len(), 2);
        assert_eq!(drainage.catchment, expected);
    }
}
