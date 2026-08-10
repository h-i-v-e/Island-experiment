#![allow(clippy::cast_precision_loss)]

use motu::{BoundingBox, Island, IslandOptions, Mesh, Vec3};

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
fn two_side_clamps_share_one_coarse_corner_sample() {
    let island = Island::generate(
        2018,
        IslandOptions {
            terrain_size: 65,
            noise_multiplier: 0.005,
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
