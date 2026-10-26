use std::collections::BTreeSet;

use planet_rs::{DVec3, PlanetError, PlanetOptions, generate, triangulate};

fn canonical(mesh: &planet_rs::PlanetMesh) -> BTreeSet<[u32; 3]> {
    mesh.triangles
        .iter()
        .map(|face| {
            let mut key = *face;
            key.sort_unstable();
            key
        })
        .collect()
}

#[test]
fn tetrahedron_and_octahedron_are_closed_and_outward() {
    let tetrahedron = vec![
        DVec3::new(1.0, 1.0, 1.0).normalize(),
        DVec3::new(-1.0, -1.0, 1.0).normalize(),
        DVec3::new(-1.0, 1.0, -1.0).normalize(),
        DVec3::new(1.0, -1.0, -1.0).normalize(),
    ];
    let mesh = triangulate(tetrahedron, 5.0, DVec3::Y).unwrap();
    assert_eq!(mesh.validate().unwrap().faces, 4);
    assert_eq!(mesh.radius.to_bits(), 5.0_f64.to_bits());
    let mesh = triangulate(
        vec![
            DVec3::X,
            -DVec3::X,
            DVec3::Y,
            -DVec3::Y,
            DVec3::Z,
            -DVec3::Z,
        ],
        1.0,
        DVec3::Y,
    )
    .unwrap();
    assert_eq!(mesh.validate().unwrap().edges, 12);
    assert_eq!(mesh.triangles.len(), 8);
}

#[test]
fn exact_cocircular_cube_sites_and_equal_projection_events_are_supported() {
    for axis in [DVec3::X, DVec3::Y, DVec3::Z, DVec3::ONE] {
        let mut points = Vec::new();
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    points.push(DVec3::new(x, y, z).normalize());
                }
            }
        }
        assert_eq!(
            triangulate(points, 1.0, axis)
                .unwrap()
                .validate()
                .unwrap()
                .faces,
            12
        );
    }
}

#[test]
fn seeded_meshes_are_repeatable_and_independent_of_sweep_direction() {
    let options = PlanetOptions {
        vertex_count: 128,
        ..PlanetOptions::default()
    };
    let first = generate(options).unwrap();
    assert_eq!(first, generate(options).unwrap());
    for axis in [DVec3::X, -DVec3::Y, DVec3::Z, DVec3::new(1.0, 2.0, 3.0)] {
        let other = generate(PlanetOptions {
            sweep_direction: axis,
            ..options
        })
        .unwrap();
        assert_eq!(first.directions, other.directions);
        assert_eq!(canonical(&first), canonical(&other));
    }
    let scaled = generate(PlanetOptions {
        radius: 6_371_000.0,
        ..options
    })
    .unwrap();
    assert_eq!(canonical(&first), canonical(&scaled));
    assert_eq!(first.directions, scaled.directions);
}

#[test]
fn one_hundred_seeds_pass_full_geometry_validation() {
    for seed in 0..100 {
        let mesh = generate(PlanetOptions {
            seed,
            vertex_count: 128,
            ..PlanetOptions::default()
        })
        .unwrap();
        let stats = mesh.validate().unwrap();
        assert_eq!(stats.vertices + stats.faces, stats.edges + 2, "seed {seed}");
    }
}

#[test]
fn small_random_mesh_matches_an_independent_exhaustive_supporting_plane_oracle() {
    let mesh = generate(PlanetOptions {
        vertex_count: 32,
        ..PlanetOptions::default()
    })
    .unwrap();
    let mut expected = BTreeSet::new();
    let points = &mesh.directions;
    for a in 0..points.len() {
        for b in a + 1..points.len() {
            for c in b + 1..points.len() {
                let normal = (points[b] - points[a]).cross(points[c] - points[a]);
                let distances: Vec<_> = points
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| ![a, b, c].contains(i))
                    .map(|(_, point)| normal.dot(*point - points[a]))
                    .collect();
                if distances.iter().all(|&d| d > 1.0e-12) || distances.iter().all(|&d| d < -1.0e-12)
                {
                    expected.insert([
                        u32::try_from(a).unwrap(),
                        u32::try_from(b).unwrap(),
                        u32::try_from(c).unwrap(),
                    ]);
                }
            }
        }
    }
    assert_eq!(canonical(&mesh), expected);
}

#[test]
fn separated_close_sites_and_nearly_coplanar_events_preserve_every_vertex() {
    let points = vec![
        DVec3::X,
        -DVec3::X,
        DVec3::Y,
        -DVec3::Y,
        DVec3::Z,
        -DVec3::Z,
        DVec3::new(1.0, 1.0e-6, 0.0).normalize(),
        DVec3::new(1.0, 1.0, 1.0e-12).normalize(),
    ];
    let mesh = triangulate(points, 1.0, DVec3::Y).unwrap();
    assert_eq!(mesh.validate().unwrap().vertices, 8);
    assert_eq!(mesh.triangles.len(), 12);
}

#[test]
fn invalid_options_and_sites_are_rejected() {
    for radius in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            generate(PlanetOptions {
                radius,
                ..PlanetOptions::default()
            })
            .is_err()
        );
    }
    for vertex_count in [0, 3, 8_193] {
        assert!(
            generate(PlanetOptions {
                vertex_count,
                ..PlanetOptions::default()
            })
            .is_err()
        );
    }
    for sweep_direction in [DVec3::ZERO, DVec3::splat(f64::NAN), DVec3::splat(f64::MAX)] {
        assert!(
            generate(PlanetOptions {
                sweep_direction,
                ..PlanetOptions::default()
            })
            .is_err()
        );
    }
    for bad in [
        DVec3::X,
        DVec3::new(1.0, 1.0e-9, 0.0).normalize(),
        DVec3::ZERO,
        DVec3::splat(f64::NAN),
    ] {
        assert!(triangulate(vec![DVec3::X, DVec3::Y, DVec3::Z, bad], 1.0, DVec3::Y).is_err());
    }
}

#[test]
fn coplanar_and_hemisphere_only_sites_are_rejected() {
    assert_eq!(
        triangulate(
            vec![DVec3::X, -DVec3::X, DVec3::Y, -DVec3::Y],
            1.0,
            DVec3::Y
        ),
        Err(PlanetError::DegenerateInput)
    );
    let points = vec![
        DVec3::Z,
        DVec3::new(1.0, 0.0, 1.0).normalize(),
        DVec3::new(0.0, 1.0, 1.0).normalize(),
        DVec3::new(-1.0, -1.0, 1.0).normalize(),
    ];
    assert_eq!(
        triangulate(points, 1.0, DVec3::Y),
        Err(PlanetError::HemisphereCoverage)
    );
}

#[test]
fn validator_detects_holes_duplicates_bad_indices_and_inverted_faces() {
    let options = PlanetOptions {
        vertex_count: 32,
        ..PlanetOptions::default()
    };
    let mut mesh = generate(options).unwrap();
    mesh.triangles.pop();
    assert!(mesh.validate().is_err());
    let mut mesh = generate(options).unwrap();
    mesh.triangles[0] = mesh.triangles[1];
    assert!(mesh.validate().is_err());
    let mut mesh = generate(options).unwrap();
    mesh.triangles[0][0] = u32::MAX;
    assert!(mesh.validate().is_err());
    let mut mesh = generate(options).unwrap();
    mesh.triangles[0].swap(1, 2);
    assert!(mesh.validate().is_err());
}
