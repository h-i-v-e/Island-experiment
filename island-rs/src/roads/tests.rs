use super::{RoadEdge, RoadGradeOptions, grade_roads};
use crate::Vec2;

fn options() -> RoadGradeOptions {
    RoadGradeOptions {
        min_height: 2.,
        max_grade: 0.08,
        max_cut: 8.,
        shoulder: 2.,
        sample_spacing: 0.5,
    }
}

#[test]
fn strips_are_flat_across_and_respect_bounded_cut_and_grade() {
    let terrain = |p: Vec2| Some(20. + p.x * 0.14 + p.y * 0.6);
    let network = grade_roads(
        &[Vec2::ZERO, Vec2::new(24., 0.)],
        &[RoadEdge {
            start: 0,
            end: 1,
            half_width: 2.,
        }],
        options(),
        terrain,
    );
    network.validate().unwrap();
    assert_eq!(network.strips.len(), 1);
    let strip = &network.strips[0];
    for pair in strip.stations.windows(2) {
        let run = pair[0].point.distance(pair[1].point);
        assert!((pair[0].height - pair[1].height).abs() <= options().max_grade * run + 0.0001);
    }
    for x in [2., 4.25, 13.5, 22.] {
        let h = network.surface_height(Vec2::new(x, 0.)).unwrap();
        for y in [-2., 0., 2., 3., 4., 5.] {
            let point = Vec2::new(x, y);
            let natural = terrain(point).unwrap();
            let cut = network.graded_height(point, natural);
            assert!(cut <= natural);
            assert!(natural - cut <= options().max_cut);
            if y.abs() <= 2. {
                assert!((h - cut).abs() < 0.0001);
            }
            if y >= 4. {
                assert!((natural - cut).abs() < 0.0001);
            }
        }
    }
}

#[test]
fn junction_is_flat_and_mesh_sections_meet_at_the_seams() {
    let nodes = [Vec2::ZERO, Vec2::new(16., 0.), Vec2::new(0., 16.)];
    let edges = [
        RoadEdge {
            start: 0,
            end: 1,
            half_width: 2.,
        },
        RoadEdge {
            start: 0,
            end: 2,
            half_width: 2.,
        },
    ];
    let network = grade_roads(&nodes, &edges, options(), |p| Some(20. + p.x * 0.2));
    assert_eq!(network.strips.len(), 2);
    let junction = network
        .junctions
        .iter()
        .find(|junction| junction.node == 0)
        .unwrap();
    assert!((network.surface_height(Vec2::ONE).unwrap() - junction.height).abs() < 0.0001);
    for strip in &network.strips {
        assert!((strip.stations[0].height - junction.height).abs() < 0.0001);
    }

    let mesh = network.mesh();
    for face in mesh.indices.chunks_exact(3) {
        let [a, b, c] = std::array::from_fn(|i| {
            let p = mesh.positions[face[i] as usize];
            Vec2::new(p[0], p[1])
        });
        let area = (b - a).perp_dot(c - a);
        assert!(area >= -0.0001, "winding {a:?} {b:?} {c:?}");
        let centre = (a + b + c) / 3.;
        let mesh_height = face
            .iter()
            .map(|i| mesh.positions[*i as usize][2])
            .sum::<f32>()
            / 3.;
        assert!((mesh_height - network.surface_height(centre).unwrap()).abs() < 0.0001);
    }
}

#[test]
fn rotated_rectangular_junction_shares_its_paved_boundary_with_terrain_and_strips() {
    let centre = Vec2::new(1264.5, 1419.75);
    let axis = Vec2::from_angle(0.61);
    let point = |x, y| centre + x * axis + y * axis.perp();
    let nodes = [centre, point(16., 0.), point(0., 16.), point(0., -16.)];
    let edges = [1, 2, 3].map(|end| RoadEdge {
        start: 0,
        end,
        half_width: if end == 1 { 3. } else { 2. },
    });
    let network = grade_roads(&nodes, &edges, options(), |p| {
        Some(20. + (p - centre).dot(axis) * 0.07)
    });
    network.validate().unwrap();
    let [junction] = network.junctions.as_slice() else {
        panic!("one crossing with three straight ends");
    };
    assert_eq!(junction.rectangle().unwrap().1, Vec2::new(2., 3.));
    assert_eq!(network.strips.len(), 3);
    assert!(network.surface_height(point(-2.05, 0.)).is_none());
    assert!(network.surface_height(point(0., 16.05)).is_none());
    assert!(
        (junction.ray_entry(point(-6., 1.), axis, 0.02).unwrap() - point(-1.98, 1.)).length()
            < 0.001
    );
    let mesh = network.mesh();
    for x in [-2., 2.] {
        for y in [-3., 3.] {
            assert!(
                mesh.positions
                    .iter()
                    .any(|p| Vec2::new(p[0], p[1]).distance(point(x, y)) < 0.001)
            );
            let near_corner = point(x * 0.999, y * 0.999);
            assert!(
                (network.surface_height(near_corner).unwrap() - junction.height).abs() < 0.0001
            );
            assert!((network.graded_height(near_corner, 21.) - junction.height).abs() < 0.0001);
        }
    }
}

#[test]
fn selectively_rejects_edges_that_cross_protection_or_need_excessive_cut() {
    let nodes = [
        Vec2::ZERO,
        Vec2::new(12., 0.),
        Vec2::new(24., 0.),
        Vec2::new(12., 12.),
    ];
    let edges = [
        RoadEdge {
            start: 0,
            end: 1,
            half_width: 1.5,
        },
        RoadEdge {
            start: 1,
            end: 2,
            half_width: 1.5,
        },
        RoadEdge {
            start: 1,
            end: 3,
            half_width: 1.5,
        },
    ];
    let network = grade_roads(&nodes, &edges, options(), |p| {
        if p.x > 16. && p.y > 2. {
            return None;
        }
        Some(20. + (p.y - 4.).max(0.) * 3.)
    });
    assert_eq!(network.strips.len(), 1);
    assert_eq!(network.strips[0].source, 0);
}

#[test]
fn optional_support_raise_is_bounded_and_fades_into_the_shoulder() {
    let nodes = [Vec2::ZERO, Vec2::new(12., 0.)];
    let edges = [RoadEdge {
        start: 0,
        end: 1,
        half_width: 2.,
    }];
    let network = grade_roads(&nodes, &edges, options(), |_| Some(10.));
    let h = |y| network.terrain_support_height(Vec2::new(6., y), 9.8, 0.1);
    assert!((h(2.) - 9.9).abs() < 0.00001);
    assert!((h(3.) - 9.85).abs() < 0.00001);
    assert!((h(4.) - 9.8).abs() < 0.00001);
}

#[test]
fn overlapping_shoulders_preserve_the_ramp_at_the_pavement_edge() {
    let offset = Vec2::new(1264.5, 1419.75);
    let along = Vec2::from_angle(0.61);
    let across = along.perp();
    let place = |x, y| offset + x * along + y * across;
    let network = grade_roads(
        &[offset, place(16., 0.), place(0., 16.)],
        &[
            RoadEdge {
                start: 0,
                end: 1,
                half_width: 2.,
            },
            RoadEdge {
                start: 0,
                end: 2,
                half_width: 3.,
            },
        ],
        options(),
        |point| Some(20. + (point - offset).dot(along) * 0.12),
    );
    network.validate().unwrap();
    assert_eq!(network.strips.len(), 2);
    let grade = |distance: f32| network.graded_height(place(5., 2. + distance), 22.);
    let edge = grade(0.);
    for distance in [-0.002, 0.002] {
        assert!((grade(distance) - edge).abs() < 0.0005);
    }
    assert!(grade(0.5) > edge);

    let mesh = network.mesh();
    for face in mesh.indices.chunks_exact(3) {
        let [a, b, c] = std::array::from_fn(|i| {
            let p = mesh.positions[face[i] as usize];
            Vec2::new(p[0], p[1])
        });
        assert!(
            (b - a).perp_dot(c - a) > 0.,
            "nonpositive face {a:?} {b:?} {c:?}"
        );
        let centre = (a + b + c) / 3.;
        let mesh_height = face
            .iter()
            .map(|i| mesh.positions[*i as usize][2])
            .sum::<f32>()
            / 3.;
        assert!((mesh_height - network.surface_height(centre).unwrap()).abs() < 0.0001);
    }
}
