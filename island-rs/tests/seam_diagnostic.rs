#![allow(clippy::cast_precision_loss)]

use motu::{BoundingBox, Island, IslandOptions, Mesh, Vec3};

fn assert_boundaries_match(left: &[(u32, u32)], right: &[(u32, u32)]) {
    const EPSILON: f32 = 1.0e-5;

    for (source, target) in [(left, right), (right, left)] {
        let maximum_error = source
            .iter()
            .map(|&(coordinate, height)| {
                let coordinate = f32::from_bits(coordinate);
                let height = f32::from_bits(height);
                target
                    .iter()
                    .map(|&(other_coordinate, other_height)| {
                        (coordinate - f32::from_bits(other_coordinate))
                            .abs()
                            .max((height - f32::from_bits(other_height)).abs())
                    })
                    .fold(f32::INFINITY, f32::min)
            })
            .fold(0.0_f32, f32::max);
        assert!(
            maximum_error <= EPSILON,
            "sibling boundary error was {maximum_error}"
        );
    }
}

const TOP: u8 = 1;
const LEFT: u8 = 2;
const BOTTOM: u8 = 4;
const RIGHT: u8 = 8;

fn on_side(vertex: Vec3, bounds: BoundingBox, side: u8) -> bool {
    let epsilon =
        ((bounds.max.x - bounds.min.x).abs() + (bounds.max.y - bounds.min.y).abs()) * 1.0e-6;
    match side {
        TOP => (vertex.y - bounds.max.y).abs() <= epsilon,
        LEFT => (vertex.x - bounds.min.x).abs() <= epsilon,
        BOTTOM => (vertex.y - bounds.min.y).abs() <= epsilon,
        RIGHT => (vertex.x - bounds.max.x).abs() <= epsilon,
        _ => false,
    }
}

fn coordinate(vertex: Vec3, side: u8) -> f32 {
    if side == TOP || side == BOTTOM {
        vertex.x
    } else {
        vertex.y
    }
}

fn profile(mesh: &Mesh, bounds: BoundingBox, side: u8) -> Vec<(f32, f32)> {
    let epsilon =
        ((bounds.max.x - bounds.min.x).abs() + (bounds.max.y - bounds.min.y).abs()) * 1.0e-6;
    let mut samples: Vec<_> = mesh
        .vertices
        .iter()
        .copied()
        .filter(|vertex| on_side(*vertex, bounds, side))
        .map(|vertex| (coordinate(vertex, side), vertex.z))
        .collect();
    samples.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    samples.dedup_by(|a, b| (a.0 - b.0).abs() <= epsilon);
    samples
}

fn sample(samples: &[(f32, f32)], at: f32) -> f32 {
    let upper = samples.partition_point(|sample| sample.0 < at);
    if upper == 0 {
        return samples[0].1;
    }
    if upper == samples.len() {
        return samples[upper - 1].1;
    }
    let lower = samples[upper - 1];
    let upper = samples[upper];
    lower.1 + (upper.1 - lower.1) * (at - lower.0) / (upper.0 - lower.0)
}

#[test]
fn clamped_edges_include_coarse_slope_changes_between_fine_vertices() {
    let mut fine = Mesh::delaunay(&[
        motu::Vec2::ZERO,
        motu::Vec2::X,
        motu::Vec2::ONE,
        motu::Vec2::Y,
    ]);
    fine.calculate_normals();
    let mut coarse = Mesh::delaunay(&[
        motu::Vec2::ZERO,
        motu::Vec2::X,
        motu::Vec2::ONE,
        motu::Vec2::Y,
        motu::Vec2::new(0.3, 1.0),
        motu::Vec2::new(0.7, 1.0),
    ]);
    for vertex in &mut coarse.vertices {
        if vertex.x > 0.0 && vertex.x < 1.0 {
            vertex.z = 0.2;
        }
    }
    coarse.calculate_normals();
    let bounds = BoundingBox::default();
    let coarse_profile = profile(&coarse, bounds, TOP);
    let fine_tile = fine.sliced_grid_clamped(bounds, 1, &coarse, TOP).remove(0);
    let fine_profile = profile(&fine_tile, bounds, TOP);
    for &(at, height) in &coarse_profile {
        assert!(
            (sample(&fine_profile, at) - height).abs() < 1.0e-6,
            "fine edge skips coarse slope change at {at}"
        );
    }
}

#[test]
fn two_side_clamps_share_one_coarse_corner_sample() {
    let island = Island::generate(
        2018,
        IslandOptions {
            terrain_size: 65,
            ..IslandOptions::default()
        },
    )
    .unwrap();
    let mut maximum_error = 0.0_f32;
    let mut compared = 0_usize;

    for (fine_lod, resolution, coordinates) in
        [(0, 64, &[1, 16, 31, 47, 62][..]), (1, 8, &[1, 3, 4, 6][..])]
    {
        let fine = island.lod(fine_lod).unwrap();
        let coarse = island.lod(fine_lod + 1).unwrap();
        for &y in coordinates {
            for &x in coordinates {
                let bounds = BoundingBox::new(
                    Vec3::new(
                        x as f32 / resolution as f32,
                        y as f32 / resolution as f32,
                        f32::MIN,
                    ),
                    Vec3::new(
                        (x + 1) as f32 / resolution as f32,
                        (y + 1) as f32 / resolution as f32,
                        f32::MAX,
                    ),
                );
                let coarse_patch = coarse.sliced(bounds);
                for mask in [TOP | LEFT, TOP | RIGHT, BOTTOM | LEFT, BOTTOM | RIGHT] {
                    let tiles = fine.sliced_grid_clamped(bounds, 8, coarse, mask);
                    for side in [TOP, LEFT, BOTTOM, RIGHT]
                        .into_iter()
                        .filter(|side| mask & side != 0)
                    {
                        let coarse_profile = profile(&coarse_patch, bounds, side);
                        assert!(
                            !coarse_profile.is_empty(),
                            "empty coarse profile at {x},{y} side {side}"
                        );
                        for vertex in tiles
                            .iter()
                            .flat_map(|mesh| &mesh.vertices)
                            .copied()
                            .filter(|vertex| on_side(*vertex, bounds, side))
                        {
                            assert!(
                                vertex.is_finite(),
                                "non-finite fine vertex at {x},{y} side {side}"
                            );
                            let expected = sample(&coarse_profile, coordinate(vertex, side));
                            maximum_error = maximum_error.max((vertex.z - expected).abs());
                            compared += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(compared > 0);
    assert!(
        maximum_error < 1.0e-5,
        "maximum seam error was {maximum_error}"
    );
}

#[test]
fn render_grid_preserves_sibling_boundaries() {
    let island = Island::generate(
        23,
        IslandOptions {
            terrain_size: 65,
            hydraulic_erosion_strength: 8.0,
            ..IslandOptions::default()
        },
    )
    .unwrap();
    let tiles = island
        .render_mesh_grid(0, BoundingBox::default(), 2, 0)
        .unwrap();
    let vertical = |mesh: &Mesh, minimum_y: f32, maximum_y: f32| {
        let mut points: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|vertex| {
                (vertex.x - 0.5).abs() < 1.0e-6 && vertex.y >= minimum_y && vertex.y <= maximum_y
            })
            .map(|vertex| (vertex.y.to_bits(), vertex.z.to_bits()))
            .collect();
        points.sort_unstable();
        points.dedup();
        points
    };
    let horizontal = |mesh: &Mesh, minimum_x: f32, maximum_x: f32| {
        let mut points: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|vertex| {
                (vertex.y - 0.5).abs() < 1.0e-6 && vertex.x >= minimum_x && vertex.x <= maximum_x
            })
            .map(|vertex| (vertex.x.to_bits(), vertex.z.to_bits()))
            .collect();
        points.sort_unstable();
        points.dedup();
        points
    };

    assert_boundaries_match(
        &vertical(&tiles[0], 0.0, 0.5),
        &vertical(&tiles[1], 0.0, 0.5),
    );
    assert_boundaries_match(
        &horizontal(&tiles[0], 0.0, 0.5),
        &horizontal(&tiles[2], 0.0, 0.5),
    );
}

#[test]
fn rendered_lod_transitions_follow_complete_coarse_edges_on_visible_land() {
    let island = Island::generate(
        23,
        IslandOptions {
            terrain_size: 65,
            hydraulic_erosion_strength: 8.0,
            ..IslandOptions::default()
        },
    )
    .unwrap();
    for (lod, resolution) in [(0, 64.0), (1, 8.0)] {
        // Exercise visible land; the old corner tile was entirely removed by the
        // ocean-floor cutoff, so its vertex-only assertions passed vacuously.
        let peak = island
            .lod(0)
            .unwrap()
            .vertices
            .iter()
            .max_by(|a, b| a.z.total_cmp(&b.z))
            .unwrap();
        let x = (peak.x * resolution).floor().min(resolution - 1.0);
        let y = (peak.y * resolution).floor().min(resolution - 1.0);
        let bounds = BoundingBox::new(
            Vec3::new(x / resolution, y / resolution, f32::MIN),
            Vec3::new((x + 1.0) / resolution, (y + 1.0) / resolution, f32::MAX),
        );
        let coarse = island.lod(lod + 1).unwrap().sliced(bounds);
        assert_neighbouring_render_groups_match(&island, lod, bounds);
        for mask in [TOP | LEFT, TOP | RIGHT, BOTTOM | LEFT, BOTTOM | RIGHT] {
            let tiles = island.render_mesh_grid(lod, bounds, 8, mask).unwrap();

            for side in [TOP, LEFT, BOTTOM, RIGHT]
                .into_iter()
                .filter(|side| mask & side != 0)
            {
                let coarse_profile = profile(&coarse, bounds, side);
                assert!(!coarse_profile.is_empty());
                let mut fine_profile: Vec<_> = tiles
                    .iter()
                    .flat_map(|tile| profile(tile, bounds, side))
                    .collect();
                fine_profile.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                fine_profile.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0e-7);
                for &(at, height) in &coarse_profile {
                    let actual = sample(&fine_profile, at);
                    assert!(
                        (actual - height).abs() < 1.0e-5,
                        "fine edge bridges a coarse bend: side={side} at={at} error={}",
                        (actual - height).abs()
                    );
                }
                for vertex in tiles
                    .iter()
                    .flat_map(|mesh| &mesh.vertices)
                    .copied()
                    .filter(|vertex| on_side(*vertex, bounds, side))
                {
                    let expected = sample(&coarse_profile, coordinate(vertex, side));
                    assert!((vertex.z - expected).abs() < 1.0e-5);
                }
            }
        }
    }
}

fn assert_neighbouring_render_groups_match(island: &Island, lod: usize, bounds: BoundingBox) {
    let width = bounds.max.x - bounds.min.x;
    let neighbour = BoundingBox::new(bounds.min + Vec3::X * width, bounds.max + Vec3::X * width);
    let left = island.render_mesh_grid(lod, bounds, 8, TOP).unwrap();
    let right = island
        .render_mesh_grid(lod, neighbour, 8, TOP | RIGHT)
        .unwrap();
    let edge_profile = |tiles: &[Mesh], bounds, side| {
        let mut samples: Vec<_> = tiles
            .iter()
            .flat_map(|tile| profile(tile, bounds, side))
            .collect();
        samples.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        samples.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0e-7);
        samples
    };
    let left = edge_profile(&left, bounds, RIGHT);
    let right = edge_profile(&right, neighbour, LEFT);
    assert!(!left.is_empty() && !right.is_empty());
    for &(at, _) in left.iter().chain(&right) {
        let error = (sample(&left, at) - sample(&right, at)).abs();
        assert!(error < 1.0e-5, "LOD{lod} group seam at {at}: error={error}");
    }
}
