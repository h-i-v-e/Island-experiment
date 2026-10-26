use planet_rs::{
    DVec3, PlanetOptions,
    chunks::{ChunkMesh, ChunkVertex},
    generate,
    ocean_surface::{CoastMap, OceanGrid, OceanGridOptions, cube_direction, cube_pixel},
    surface_atlas::SurfaceSampler,
};
use std::collections::HashMap;

fn terrain(height: impl Fn(DVec3) -> f64) -> ChunkMesh {
    let sphere = generate(PlanetOptions {
        vertex_count: 512,
        ..PlanetOptions::default()
    })
    .unwrap();
    ChunkMesh {
        vertices: sphere
            .directions
            .into_iter()
            .map(|n| ChunkVertex {
                position: n * (1.0 + height(n)),
                normal: n,
                colour: [0.5, 1.0, 0.5, 0.0],
                protected: false,
            })
            .collect(),
        triangles: sphere.triangles,
    }
}
#[test]
fn graded_ocean_is_closed_outward_and_has_no_polar_or_ring_cracks() {
    let grid = OceanGrid::build(
        400.0,
        OceanGridOptions {
            spacing: 1.0,
            fine_radius: 16.0,
            rows_per_level: 4,
        },
    )
    .unwrap();
    let mut edges = HashMap::<(u32, u32), u32>::new();
    let mut worst = 0.0_f64;
    for &[a, b, c] in &grid.triangles {
        let [pa, pb, pc] = [a, b, c].map(|i| grid.positions[i as usize]);
        let twice_area = (pb - pa).cross(pc - pa).length();
        assert!((pb - pa).cross(pc - pa).dot(pa + pb + pc) > 0.0);
        let lengths = [pa.distance(pb), pb.distance(pc), pc.distance(pa)];
        worst = worst.max(lengths.into_iter().fold(0.0, f64::max).powi(2) / twice_area);
        for (a, b) in [(a, b), (b, c), (c, a)] {
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    assert!(edges.values().all(|&count| count == 2));
    assert_eq!(grid.positions.len() + grid.triangles.len(), edges.len() + 2);
    assert!(worst < 12.0, "thin ocean triangles: {worst}");
    assert!(
        grid.positions
            .iter()
            .all(|p| (p.length() - 1.0).abs() < 1.0e-12)
    );
    assert!(grid.sampling.iter().all(|p| p[0].is_finite() && p[1] > 0.0));
}
#[test]
fn unity_default_detail_fits_the_global_runtime_budget_and_grades_outward() {
    let grid = OceanGrid::build(1000.0, OceanGridOptions::default()).unwrap();
    assert!(grid.positions.len() < 500_000 && grid.triangles.len() < 1_000_000);
    assert!(grid.positions.len() > 100_000 && grid.rings > 256);
    assert!(
        grid.sampling
            .iter()
            .filter(|s| s[0] < 192.0)
            .all(|s| (s[1] - 1.0).abs() < f32::EPSILON)
    );
    assert!(grid.sampling.iter().any(|s| s[1] >= 8.0));
    assert!(
        OceanGrid::build(
            1000.0,
            OceanGridOptions {
                spacing: 0.5,
                ..OceanGridOptions::default()
            }
        )
        .is_err()
    );
    assert!(OceanGrid::build(f64::NAN, OceanGridOptions::default()).is_err());
}
#[test]
fn cube_coordinates_round_trip_at_both_poles_and_all_six_faces() {
    for face in 0..6 {
        for y in 0..32 {
            for x in 0..32 {
                let n = cube_direction(
                    face,
                    (f64::from(x) + 0.5) / 16.0 - 1.0,
                    (f64::from(y) + 0.5) / 16.0 - 1.0,
                );
                assert_eq!(cube_pixel(n, 32), (face, x, y));
            }
        }
    }
    assert_eq!(cube_pixel(DVec3::Y, 32).0, 2);
    assert_eq!(cube_pixel(-DVec3::Y, 32).0, 3);
}
#[test]
fn coastal_fields_use_the_actual_displayed_mesh_and_cross_cube_seams() {
    let source = terrain(|n| n.y * 0.08);
    let sampler = SurfaceSampler::new(&source).unwrap();
    let map = CoastMap::bake(&sampler, 1000.0, 64).unwrap();
    for face in 0..6 {
        for (x, y) in [(8_u32, 8_u32), (32, 32), (48, 48)] {
            let n = cube_direction(
                face,
                (f64::from(x) + 0.5) / 32.0 - 1.0,
                (f64::from(y) + 0.5) / 32.0 - 1.0,
            );
            let (radius, _) = sampler.radial_surface(n).unwrap();
            let depth = ((1.0 - radius) * 1000.0).clamp(0.0, 10.0);
            let index = (face * 64 * 64 + y as usize * 64 + x as usize) * 4;
            let encoded = f64::from(map.pixels[index]) / 25.5;
            assert!((encoded - depth).abs() < 0.041);
            assert_eq!(map.pixels[index + 2], 128);
        }
    }
    let distance = |n: DVec3| {
        let (f, x, y) = cube_pixel(n, 64);
        f64::from(map.pixels[(f * 64 * 64 + y as usize * 64 + x as usize) * 4 + 1]) * 128.0 / 255.0
    };
    for x in [-1.0, 1.0] {
        for z in [-1.0, 1.0] {
            // Locate the actual planar mesh/sea intersection, rather than
            // assuming the sparse inscribed source intersects at latitude zero.
            let (mut low, mut high) = (-0.5, 0.5);
            for _ in 0..40 {
                let middle = f64::midpoint(low, high);
                let n = DVec3::new(x, middle, z).normalize();
                if sampler.radial_surface(n).unwrap().0 > 1.0 {
                    high = middle;
                } else {
                    low = middle;
                }
            }
            let latitude = f64::midpoint(low, high) - 0.02;
            let first = DVec3::new(x, latitude, z * 0.999).normalize();
            let second = DVec3::new(x * 0.999, latitude, z).normalize();
            assert!((distance(first) - distance(second)).abs() < 40.0);
            assert!(
                distance(first) < 128.0,
                "shore propagation stopped at a cube edge"
            );
        }
    }
}
#[test]
fn fully_wet_and_dry_maps_have_finite_unambiguous_fallbacks() {
    for (height, depth, distance) in [(-0.1, 255, 255), (0.1, 0, 0)] {
        let source = terrain(|_| height);
        let sampler = SurfaceSampler::new(&source).unwrap();
        let map = CoastMap::bake(&sampler, 1000.0, 32).unwrap();
        assert!(
            map.pixels
                .chunks_exact(4)
                .all(|p| p[0] == depth && p[1] == distance)
        );
    }
}
