use std::collections::HashMap;

use planet_rs::{
    DVec3, PlanetOptions,
    chunks::{ChunkBounds, ChunkMesh, ChunkVertex, occluder_radius, partition},
    generate,
};

fn sphere() -> ChunkMesh {
    let planet = generate(PlanetOptions {
        vertex_count: 96,
        ..PlanetOptions::default()
    })
    .unwrap();
    ChunkMesh {
        vertices: planet
            .directions
            .iter()
            .map(|&direction| ChunkVertex {
                position: direction * 1000.0,
                normal: direction,
                colour: [0.3, 0.5, 0.8, 1.0],
                protected: false,
            })
            .collect(),
        triangles: planet.triangles,
    }
}

#[test]
fn detail_ownership_matches_the_terrain_partition() {
    use planet_rs::chunks::CellLocator;
    let source = sphere();
    for level in 0..=3 {
        let locator = CellLocator::new(level).unwrap();
        for chunk in partition(&source, level).unwrap() {
            for face in &chunk.mesh.triangles {
                let centre = face
                    .iter()
                    .map(|&i| chunk.mesh.vertices[i as usize].position)
                    .sum::<DVec3>()
                    / 3.0;
                assert_eq!(locator.owner(centre), Some(chunk.id));
            }
        }
        for pole in [
            DVec3::X,
            -DVec3::X,
            DVec3::Y,
            -DVec3::Y,
            DVec3::Z,
            -DVec3::Z,
        ] {
            assert_eq!(locator.owner(pole), locator.owner(pole * 1000.0));
            assert!(locator.owner(pole).is_some());
        }
        assert_eq!(locator.owner(DVec3::ZERO), None);
        assert_eq!(locator.owner(DVec3::splat(f64::NAN)), None);
    }
    assert!(CellLocator::new(7).is_err());
}

fn area(mesh: &ChunkMesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|face| {
            let [a, b, c] = face.map(|i| mesh.vertices[i as usize].position);
            (b - a).cross(c - a).length() * 0.5
        })
        .sum()
}

fn bits(point: DVec3) -> [u64; 3] {
    point.to_array().map(|coordinate| {
        if coordinate == 0.0 {
            0
        } else {
            coordinate.to_bits()
        }
    })
}

#[test]
fn partition_preserves_area_exact_shared_edges_and_determinism() {
    let source = sphere();
    let chunks = partition(&source, 2).unwrap();
    assert_eq!(chunks.len(), 320);
    assert_eq!(chunks, partition(&source, 2).unwrap());
    let total_area: f64 = chunks.iter().map(|chunk| area(&chunk.mesh)).sum();
    assert!(
        (total_area - area(&source)).abs() / area(&source) < 1.0e-11,
        "area ratio {}",
        total_area / area(&source)
    );
    let mut edges = HashMap::new();
    let mut vertices = HashMap::new();
    for chunk in &chunks {
        for vertex in &chunk.mesh.vertices {
            let key = bits(vertex.position);
            if let Some(previous) = vertices.insert(key, *vertex) {
                assert_eq!(
                    vertex.normal, previous.normal,
                    "shared normals must be identical"
                );
                assert_eq!(
                    vertex.colour.map(f32::to_bits),
                    previous.colour.map(f32::to_bits)
                );
            }
            assert!(vertex.position.distance(chunk.bounds.centre) <= chunk.bounds.radius);
            assert!(vertex.position.length() <= chunk.bounds.max_radius);
        }
        for face in &chunk.mesh.triangles {
            let [a, b, c] = face.map(|i| chunk.mesh.vertices[i as usize].position);
            assert!(
                (b - a).cross(c - a).dot(a) > 0.0,
                "outward winding must survive cuts"
            );
            for (a, b) in [(a, b), (b, c), (c, a)] {
                let a = bits(a);
                let b = bits(b);
                *edges
                    .entry(if a <= b { (a, b) } else { (b, a) })
                    .or_insert(0) += 1;
            }
        }
    }
    let unpaired = edges.values().filter(|&&count| count != 2).count();
    assert_eq!(
        unpaired, 0,
        "all terrain edges must pair across chunk seams"
    );
}

#[test]
fn poles_cone_corners_and_boundary_vertices_do_not_lose_triangles() {
    let directions = vec![
        DVec3::X,
        DVec3::Y,
        DVec3::Z,
        -DVec3::X,
        -DVec3::Y,
        -DVec3::Z,
    ];
    let mesh = planet_rs::triangulate(directions, 1000.0, DVec3::Y).unwrap();
    let source = ChunkMesh {
        vertices: mesh
            .directions
            .into_iter()
            .map(|direction| ChunkVertex {
                position: direction * 1000.0,
                normal: direction,
                colour: [1.0; 4],
                protected: false,
            })
            .collect(),
        triangles: mesh.triangles,
    };
    for level in 0..=3 {
        let chunks = partition(&source, level).unwrap();
        let total: f64 = chunks.iter().map(|chunk| area(&chunk.mesh)).sum();
        assert!((total - area(&source)).abs() < 1.0e-7);
        assert!(chunks.iter().all(|chunk| {
            chunk
                .mesh
                .vertices
                .iter()
                .all(|vertex| vertex.position.is_finite())
        }));
    }
}

#[test]
fn horizon_culling_keeps_tall_peaks_and_nearby_patches() {
    let camera = DVec3::Z * 1010.0;
    let angular_distance = 0.5_f64;
    let direction = DVec3::new(angular_distance.sin(), 0.0, angular_distance.cos());
    let sea_patch = ChunkBounds {
        centre: direction * 1000.0,
        radius: 1.0,
        direction,
        half_angle: 0.001,
        max_radius: 1000.0,
    };
    assert!(!sea_patch.horizon_visible(camera, 1000.0));
    let mountain = ChunkBounds {
        max_radius: 1100.0,
        ..sea_patch
    };
    assert!(mountain.horizon_visible(camera, 1000.0));
    assert!(sea_patch.horizon_visible(DVec3::Z * 900.0, 1000.0));
    assert!(sea_patch.horizon_visible(camera, 0.0));
    let behind = ChunkBounds {
        direction: -DVec3::Z,
        ..sea_patch
    };
    assert!(!behind.horizon_visible(camera, 1000.0));
    let facing = ChunkBounds {
        direction: DVec3::Z,
        ..sea_patch
    };
    assert!(facing.horizon_visible(camera, 1000.0));
}

#[test]
fn occluder_uses_triangle_interior_instead_of_vertex_radius() {
    let mesh = ChunkMesh {
        vertices: [
            DVec3::new(1.0, -2.0, -2.0),
            DVec3::new(1.0, 2.0, -2.0),
            DVec3::new(1.0, 0.0, 2.0),
        ]
        .map(|position| ChunkVertex {
            position,
            normal: DVec3::X,
            colour: [1.0; 4],
            protected: false,
        })
        .to_vec(),
        triangles: vec![[0, 1, 2]],
    };
    assert!((occluder_radius(&mesh) - 1.0).abs() < 1.0e-11);
    assert!(
        occluder_radius(&mesh)
            < mesh
                .vertices
                .iter()
                .map(|vertex| vertex.position.length())
                .fold(f64::INFINITY, f64::min)
    );
}

#[test]
fn invalid_partition_inputs_return_errors() {
    let source = sphere();
    assert!(partition(&source, 7).is_err());
    let mut invalid = source;
    invalid.triangles.push([u32::MAX, 0, 1]);
    assert!(partition(&invalid, 2).is_err());
    assert!(occluder_radius(&ChunkMesh::default()).abs() < f64::EPSILON);
}

#[test]
fn radial_waterfall_on_partition_boundary_has_one_owner() {
    let phi = f64::midpoint(1.0, 5.0_f64.sqrt());
    let a = DVec3::new(-1.0, phi, 0.0).normalize();
    let b = DVec3::new(-phi, 0.0, 1.0).normalize();
    let direction = (a + b).normalize();
    let mesh = ChunkMesh {
        vertices: [a * 1000.0, direction * 1000.0, direction * 1010.0]
            .map(|position| ChunkVertex {
                position,
                normal: DVec3::Y,
                colour: [1.0; 4],
                protected: false,
            })
            .to_vec(),
        triangles: vec![[0, 1, 2]],
    };
    for level in 0..=3 {
        let chunks = partition(&mesh, level).unwrap();
        let cut_area: f64 = chunks.iter().map(|chunk| area(&chunk.mesh)).sum();
        assert!(
            (cut_area - area(&mesh)).abs() / area(&mesh) < 1.0e-11,
            "level {level}: {cut_area} versus {}",
            area(&mesh)
        );
    }
}

#[test]
fn a_culled_patch_has_no_sample_visible_past_the_occluder() {
    let source = sphere();
    let occluder = occluder_radius(&source);
    let chunks = partition(&source, 2).unwrap();
    for direction in [DVec3::Z, DVec3::X, DVec3::new(1.0, 2.0, 3.0).normalize()] {
        for radius in [occluder + 1.0, 1100.0, 4000.0] {
            let camera = direction * radius;
            for chunk in &chunks {
                if chunk.bounds.horizon_visible(camera, occluder) {
                    continue;
                }
                for vertex in &chunk.mesh.vertices {
                    let ray = vertex.position - camera;
                    let t = (-camera.dot(ray) / ray.length_squared()).clamp(0.0, 1.0);
                    let closest = camera + ray * t;
                    assert!(
                        t < 1.0 && closest.length() <= occluder + 1.0e-7,
                        "a visible vertex was culled"
                    );
                }
            }
        }
    }
}

#[test]
fn cone_cuts_preserve_planar_surfaces_and_interpolated_colours() {
    let positions = [
        DVec3::new(-500.0, -200.0, 800.0),
        DVec3::new(700.0, -100.0, 900.0),
        DVec3::new(-100.0, 900.0, 1000.0),
    ];
    let face_normal = (positions[1] - positions[0])
        .cross(positions[2] - positions[0])
        .normalize();
    let mesh = ChunkMesh {
        vertices: positions
            .into_iter()
            .zip([0.25, 0.85, 0.45])
            .map(|(position, red)| ChunkVertex {
                position,
                normal: face_normal,
                colour: [red, 0.5, 0.25, 1.0],
                protected: false,
            })
            .collect(),
        triangles: vec![[0, 1, 2]],
    };
    for chunk in partition(&mesh, 2).unwrap() {
        for vertex in chunk.mesh.vertices {
            assert!((vertex.position - positions[0]).dot(face_normal).abs() < 1.0e-9);
            assert!(
                (f64::from(vertex.colour[0]) - (vertex.position.x * 0.0005 + 0.5)).abs() < 1.0e-7
            );
            assert!(vertex.normal.distance(face_normal) < 1.0e-12);
        }
    }
}

fn boundary_length(mesh: &ChunkMesh) -> f64 {
    let mut edges = HashMap::new();
    for face in &mesh.triangles {
        for (start, end) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
            let start = bits(mesh.vertices[start as usize].position);
            let end = bits(mesh.vertices[end as usize].position);
            let key = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            *edges.entry(key).or_insert(0_usize) += 1;
        }
    }
    edges
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|((start, end), _)| {
            DVec3::from_array(start.map(f64::from_bits))
                .distance(DVec3::from_array(end.map(f64::from_bits)))
        })
        .sum()
}

fn assert_generated_partition(source: &ChunkMesh, closed: bool, label: &str) {
    let chunks = partition(source, 2).unwrap_or_else(|error| panic!("{label}: {error}"));
    let total_area: f64 = chunks.iter().map(|chunk| area(&chunk.mesh)).sum();
    let original_area = area(source);
    assert!(
        (total_area - original_area).abs() <= original_area * 1.0e-10,
        "{label}: area changed from {original_area} to {total_area}"
    );
    // Combining chunk-local vertices without welding still lets this geometric
    // edge oracle identify cracks: shared cut positions must match bit for bit.
    let mut assembled = ChunkMesh::default();
    for chunk in chunks {
        let offset = u32::try_from(assembled.vertices.len()).unwrap();
        assert!(
            chunk
                .mesh
                .vertices
                .iter()
                .all(|vertex| vertex.position.is_finite()
                    && vertex.normal.is_finite()
                    && vertex.position.distance(chunk.bounds.centre) <= chunk.bounds.radius)
        );
        assembled.vertices.extend(chunk.mesh.vertices);
        assembled.triangles.extend(
            chunk
                .mesh
                .triangles
                .into_iter()
                .map(|face| face.map(|index| index + offset)),
        );
    }
    let original_boundary = boundary_length(source);
    let cut_boundary = boundary_length(&assembled);
    if closed {
        assert!(
            original_boundary < 1.0e-10,
            "{label}: original terrain must be closed"
        );
        assert!(
            cut_boundary < 1.0e-10,
            "{label}: terrain acquired cracks totalling {cut_boundary}"
        );
    } else {
        assert!(
            (cut_boundary - original_boundary).abs() < 1.0e-9,
            "{label}: river boundary changed from {original_boundary} to {cut_boundary}"
        );
    }
}

#[test]
fn generated_terrain_and_waterfall_rivers_preserve_area_and_seams() {
    for seed in [17, 42, 666] {
        let terrain = planet_rs::generate_terrain(
            PlanetOptions {
                seed,
                vertex_count: 256,
                ..PlanetOptions::default()
            },
            planet_rs::TerrainOptions {
                refinement_passes: 2,
                river_refinement_passes: 3,
                ..planet_rs::TerrainOptions::default()
            },
        )
        .unwrap();
        let surface = ChunkMesh {
            vertices: terrain
                .normals
                .iter()
                .enumerate()
                .map(|(index, &normal)| ChunkVertex {
                    position: terrain.position(index) / terrain.topology.radius,
                    normal,
                    colour: [0.5; 4],
                    protected: false,
                })
                .collect(),
            triangles: terrain.topology.triangles,
        };
        assert_generated_partition(&surface, true, &format!("seed {seed} terrain"));
        let rivers = ChunkMesh {
            vertices: terrain
                .river_surface
                .positions
                .iter()
                .zip(&terrain.river_surface.normals)
                .map(|(&position, &normal)| ChunkVertex {
                    position,
                    normal,
                    colour: [0.5; 4],
                    protected: true,
                })
                .collect(),
            triangles: terrain.river_surface.triangles,
        };
        assert!(
            !rivers.triangles.is_empty(),
            "seed {seed} should exercise actual water geometry"
        );
        assert!(
            !terrain.waterfalls.is_empty(),
            "seed {seed} should exercise waterfalls"
        );
        assert_generated_partition(&rivers, false, &format!("seed {seed} rivers"));
    }
}
