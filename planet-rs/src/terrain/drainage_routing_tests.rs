use super::*;
use crate::terrain::{
    TerrainOptions,
    drainage::{carve_with_sources, repair_with_sources},
};

fn basin() -> (Vec<DVec3>, SurfaceGraph, Vec<f64>) {
    let directions = (0..6)
        .map(|i| DVec3::new(f64::from(i) * 0.01, 0.0, 1.0).normalize())
        .collect();
    let graph = SurfaceGraph {
        neighbours: vec![
            vec![1, 5],
            vec![0, 2],
            vec![1, 3],
            vec![2, 4],
            vec![3, 5],
            vec![0, 4],
        ],
        areas: vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5],
    };
    (directions, graph, vec![-0.1, 0.3, 0.02, 0.15, 0.2, 0.5])
}

fn options(catchment: f64) -> TerrainOptions {
    TerrainOptions {
        river_catchment: catchment / (4.0 * std::f64::consts::PI),
        river_depth: 0.0,
        ..TerrainOptions::default()
    }
}

fn reaches_sea(drainage: &Drainage, heights: &[f64]) {
    for start in 0..heights.len() {
        let mut current = start;
        for _ in heights {
            let Some(next) = drainage.receivers[current] else {
                break;
            };
            current = next;
        }
        assert!(
            heights[current] < 0.0,
            "site {start} must reach sea without cycling"
        );
    }
}

#[test]
fn basin_preserves_natural_valley_and_accumulates_through_the_lowest_spill() {
    let (directions, graph, heights) = basin();
    let drainage = route(&directions, &graph, &heights);
    assert_eq!(
        drainage.receivers,
        [None, Some(0), Some(1), Some(2), Some(3), Some(4)]
    );
    assert!((drainage.catchment[2] - 1.4).abs() < 1e-12);
    assert!((drainage.catchment[0] - 1.5).abs() < 1e-12);
    reaches_sea(&drainage, &heights);
}

#[test]
fn larger_threshold_uses_combined_basin_rainfall_and_fills_only_its_corridor() {
    let (directions, graph, mut heights) = basin();
    let before = heights.clone();
    let (drainage, rivers, removed) = carve_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut [0.0; 6],
        &options(1.3),
        &[],
    );
    assert_eq!(
        rivers.iter().map(|r| (r.from, r.to)).collect::<Vec<_>>(),
        [(1, 0), (2, 1)]
    );
    assert_eq!(removed.to_bits(), 0.0_f64.to_bits());
    assert!(heights[1] < heights[2]);
    assert_eq!(heights[1].to_bits(), before[1].to_bits());
    assert!(heights[2] > before[2]);
    assert_eq!(&heights[3..], &before[3..]);
    assert!((drainage.catchment[2] - 1.4).abs() < 1e-12);
}

#[test]
fn excessive_threshold_preserves_unselected_pits_even_with_old_source_anchors() {
    let (directions, graph, mut heights) = basin();
    let before = heights.clone();
    let (_, rivers, removed) = carve_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut [0.0; 6],
        &options(2.0),
        &[directions[2], directions[5]],
    );
    assert!(rivers.is_empty());
    assert_eq!(removed.to_bits(), 0.0_f64.to_bits());
    assert_eq!(heights, before);
}

#[test]
fn flat_world_has_deterministic_conservative_paths_and_no_artificial_source_limit() {
    let count = 240;
    let directions: Vec<_> = (0..count)
        .map(|i| DVec3::new(f64::from(i) * 0.001, 0.0, 1.0).normalize())
        .collect();
    let mut neighbours = vec![Vec::new(); directions.len()];
    for i in 1..directions.len() {
        neighbours[i - 1].push(i);
        neighbours[i].push(i - 1);
    }
    let graph = SurfaceGraph {
        neighbours,
        areas: vec![0.01; directions.len()],
    };
    let mut heights = vec![0.1; directions.len()];
    heights[0] = -0.1;
    let drainage = route(&directions, &graph, &heights);
    assert_eq!(drainage, route(&directions, &graph, &heights));
    assert!((drainage.catchment[0] - 2.39).abs() < 1e-12);
    reaches_sea(&drainage, &heights);
    let (_, rivers, _) = carve_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut vec![0.0; directions.len()],
        &options(0.005),
        &[],
    );
    assert_eq!(rivers.len(), 239);
    assert!(
        rivers
            .iter()
            .all(|river| heights[river.from] > heights[river.to])
    );
}

#[test]
fn downstream_candidates_are_covered_once_without_losing_either_confluence_branch() {
    let directions: Vec<_> = (0..5)
        .map(|i| DVec3::new(f64::from(i) * 0.01, 0.0, 1.0).normalize())
        .collect();
    let graph = SurfaceGraph {
        neighbours: vec![vec![1], vec![0, 2], vec![1, 3, 4], vec![2], vec![2]],
        areas: vec![0.1; 5],
    };
    let mut heights = [-0.1, 0.1, 0.2, 0.3, 0.4];
    let (_, rivers, _) = carve_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut [0.0; 5],
        &options(0.09),
        &[],
    );
    assert_eq!(
        rivers.iter().map(|r| (r.from, r.to)).collect::<Vec<_>>(),
        [(1, 0), (2, 1), (3, 2), (4, 2)]
    );
}

#[test]
fn inland_bed_crossing_zero_keeps_its_original_outlet_and_rainfall() {
    let (directions, graph, mut heights) = basin();
    let mut drainage = route(&directions, &graph, &heights);
    // A prior corridor cut crosses zero inland. It remains part of its river,
    // rather than becoming an artificial sea root that discards downstream flow.
    let catchment = drainage.catchment.clone();
    heights[1] = -0.001;
    heights[3] = 0.3;
    heights[4] = 0.02;
    let (rivers, _) = repair_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut [0.0; 6],
        &options(0.3),
        &mut drainage,
        &[],
    );
    assert_eq!(drainage.receivers[1], Some(0));
    assert!(drainage.receivers[0].is_none());
    assert_eq!(drainage.catchment, catchment);
    assert!(heights[1] >= 0.0);
    assert!(rivers.iter().any(|river| river.from == 4));
    assert!(
        rivers
            .iter()
            .all(|river| heights[river.from] > heights[river.to])
    );
}

#[test]
fn low_tributary_is_filled_without_cutting_or_flooding_its_receiving_trunk() {
    let directions: Vec<_> = (0..4)
        .map(|i| DVec3::new(f64::from(i) * 0.01, 0.0, 1.0).normalize())
        .collect();
    let graph = SurfaceGraph {
        neighbours: vec![vec![1], vec![0, 2, 3], vec![1], vec![1]],
        areas: vec![1.0; 4],
    };
    let mut drainage = Drainage {
        receivers: vec![None, Some(0), Some(1), Some(1)],
        catchment: vec![0.0; 4],
        order: vec![0, 1, 2, 3],
    };
    let mut heights = [-0.1, 0.2, 0.3, 0.02];
    let before = heights;
    let mut soil = [0.0; 4];
    let (rivers, added) = repair_with_sources(
        &directions,
        &graph,
        &mut heights,
        &mut soil,
        &options(0.5),
        &mut drainage,
        &[],
    );
    assert_eq!(&heights[..3], &before[..3]);
    assert!(heights[3] > heights[1]);
    assert!((added - (heights[3] - before[3])).abs() < 1e-12);
    assert!((soil[3] - added).abs() < 1e-12);
    assert_eq!(rivers.len(), 3);
    assert!((drainage.catchment[0] - 3.0).abs() < 1e-12);
}
