#![allow(clippy::float_cmp)]

use std::collections::{BTreeMap, BTreeSet};

use planet_rs::{
    DVec3,
    chunk_lod::{build_lods, build_unpinned_lods},
    chunks::{ChunkMesh, ChunkVertex},
};

fn tile(offset: f64) -> ChunkMesh {
    const SIDE: u32 = 17;
    let vertices = (0..SIDE)
        .flat_map(|y| {
            (0..SIDE).map(move |x| {
                let x = f64::from(x) + offset;
                let y = f64::from(y);
                ChunkVertex {
                    position: DVec3::new(x, y, (x * 0.35).sin() * (y * 0.25).sin()),
                    normal: DVec3::Z,
                    colour: [0.2, 0.5, 0.8, 1.0],
                    // A hydraulic feature across the centre, including its bank edges.
                    protected: (7.0..=9.0).contains(&y),
                }
            })
        })
        .collect();
    let triangles = (0..SIDE - 1)
        .flat_map(|y| {
            (0..SIDE - 1).flat_map(move |x| {
                let a = y * SIDE + x;
                [[a, a + 1, a + SIDE], [a + 1, a + SIDE + 1, a + SIDE]]
            })
        })
        .collect();
    ChunkMesh {
        vertices,
        triangles,
    }
}

fn edge_counts(mesh: &ChunkMesh) -> BTreeMap<[u32; 2], usize> {
    let mut edges = BTreeMap::new();
    for &[a, b, c] in &mesh.triangles {
        for mut edge in [[a, b], [b, c], [c, a]] {
            edge.sort_unstable();
            *edges.entry(edge).or_default() += 1;
        }
    }
    edges
}

fn boundary(mesh: &ChunkMesh) -> Vec<[ChunkVertex; 2]> {
    edge_counts(mesh)
        .into_iter()
        .filter(|(_, uses)| *uses == 1)
        .map(|([a, b], _)| [mesh.vertices[a as usize], mesh.vertices[b as usize]])
        .collect()
}

fn same_edge(a: &[ChunkVertex; 2], b: &[ChunkVertex; 2]) -> bool {
    a == b || (a[0] == b[1] && a[1] == b[0])
}

#[test]
fn mixed_lods_preserve_every_border_edge_and_feature_attribute() {
    let source = tile(0.0);
    let border = boundary(&source);
    let neighbour = tile(16.0);
    let neighbour_lods = build_lods(&neighbour);
    let levels = build_lods(&source);
    assert_eq!(levels.len(), 4);
    assert_eq!(levels[0].mesh, source);
    assert_eq!(levels[0].error, 0.0);
    assert!(levels[3].mesh.triangles.len() < source.triangles.len() / 2);
    for level in &levels {
        let coarse_border = boundary(&level.mesh);
        assert_eq!(border.len(), coarse_border.len());
        for edge in &border {
            assert!(
                coarse_border
                    .iter()
                    .any(|candidate| same_edge(edge, candidate))
            );
        }
        for feature in source.vertices.iter().filter(|vertex| vertex.protected) {
            assert!(level.mesh.vertices.contains(feature));
        }
        for other in &neighbour_lods {
            let seam: Vec<_> = level
                .mesh
                .vertices
                .iter()
                .filter(|vertex| vertex.position.x == 16.0)
                .collect();
            let neighbour_seam: Vec<_> = other
                .mesh
                .vertices
                .iter()
                .filter(|vertex| vertex.position.x == 16.0)
                .collect();
            assert_eq!(seam, neighbour_seam);
        }
    }
}

#[test]
fn simplification_is_deterministic_manifold_and_outward() {
    let source = tile(0.0);
    let levels = build_lods(&source);
    assert_eq!(levels, build_lods(&source));
    for (index, level) in levels.iter().enumerate() {
        let mut keys = BTreeSet::new();
        for &face in &level.mesh.triangles {
            let [a, b, c] = face.map(|i| level.mesh.vertices[i as usize].position);
            assert!((b - a).cross(c - a).z > 0.0);
            let mut key = face;
            key.sort_unstable();
            assert!(keys.insert(key), "duplicate triangle");
        }
        let edges = edge_counts(&level.mesh);
        assert!(edges.values().all(|&uses| uses == 1 || uses == 2));
        assert_eq!(
            level.mesh.vertices.len() + level.mesh.triangles.len(),
            edges.len() + 1
        );
        if index > 0 {
            assert!(level.error >= levels[index - 1].error);
            assert!(level.mesh.triangles.len() <= levels[index - 1].mesh.triangles.len());
        }
    }
}

// Independent closest-point oracle: project onto the triangle plane, otherwise
// take the closest edge. This does not reuse the simplifier's correspondence.
fn distance_squared(point: DVec3, triangle: [DVec3; 3]) -> f64 {
    let [a, b, c] = triangle;
    let normal = (b - a).cross(c - a);
    let projected = point - normal * (point - a).dot(normal) / normal.length_squared();
    if [(a, b), (b, c), (c, a)]
        .iter()
        .all(|&(u, v)| (v - u).cross(projected - u).dot(normal) >= -1.0e-12)
    {
        return projected.distance_squared(point);
    }
    [(a, b), (b, c), (c, a)]
        .iter()
        .map(|&(u, v)| {
            let fraction = ((point - u).dot(v - u) / (v - u).length_squared()).clamp(0.0, 1.0);
            point.distance_squared(u.lerp(v, fraction))
        })
        .fold(f64::INFINITY, f64::min)
}

fn distance_to_mesh(point: DVec3, mesh: &ChunkMesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|face| distance_squared(point, face.map(|i| mesh.vertices[i as usize].position)))
        .fold(f64::INFINITY, f64::min)
        .sqrt()
}

#[test]
fn reported_error_bounds_source_and_simplified_surfaces() {
    let source = tile(0.0);
    for level in build_lods(&source).iter().skip(1) {
        assert!(level.error.is_finite() && level.error > 0.0);
        for &face in &source.triangles {
            let [a, b, c] = face.map(|i| source.vertices[i as usize].position);
            for point in [a, (a + b + c) / 3.0, a.lerp(b, 0.5)] {
                assert!(distance_to_mesh(point, &level.mesh) <= level.error + 1.0e-10);
            }
        }
        for &face in &level.mesh.triangles {
            let [a, b, c] = face.map(|i| level.mesh.vertices[i as usize].position);
            assert!(distance_to_mesh((a + b + c) / 3.0, &source) <= level.error + 1.0e-10);
        }
    }
}

#[test]
fn fully_protected_waterfall_is_unchanged() {
    let mut source = tile(0.0);
    for vertex in &mut source.vertices {
        vertex.protected = true;
        vertex.position.z += if vertex.position.x < 8.0 { 12.0 } else { 0.0 };
    }
    for level in build_lods(&source) {
        assert_eq!(level.mesh, source);
        assert_eq!(level.error, 0.0);
    }
}

fn radial_height(direction: DVec3, mesh: &ChunkMesh) -> f64 {
    mesh.triangles
        .iter()
        .find_map(|face| {
            let [a, b, c] = face.map(|index| mesh.vertices[index as usize].position);
            let normal = (b - a).cross(c - a);
            let distance = normal.dot(a) / normal.dot(direction);
            let point = direction * distance;
            (distance > 0.0
                && [(a, b), (b, c), (c, a)].iter().all(|&(start, end)| {
                    (end - start).cross(point - start).dot(normal) >= -1.0e-14
                }))
            .then_some(distance)
        })
        .expect("radial projection covers the unchanged chunk domain")
}

#[test]
fn spherical_lods_remain_radial_graphs_with_a_radial_error_bound() {
    let mut source = tile(0.0);
    for vertex in &mut source.vertices {
        let point = vertex.position;
        vertex.position =
            DVec3::new((point.x - 8.0) * 0.05, (point.y - 8.0) * 0.05, 1.0).normalize();
        vertex.normal = vertex.position;
        vertex.protected = false;
    }
    let levels = build_lods(&source);
    assert!(levels[3].mesh.triangles.len() < source.triangles.len() / 2);
    let maximum_radius = source
        .vertices
        .iter()
        .map(|vertex| vertex.position.length())
        .fold(0.0, f64::max);
    let source_minimum = planet_rs::chunks::occluder_radius(&source);
    let mut radial_error: f64 = 0.0;
    for level in levels.iter().skip(1) {
        let minimum_radius = planet_rs::chunks::occluder_radius(&level.mesh).min(source_minimum);
        radial_error = radial_error.max(maximum_radius - minimum_radius);
        for face in &level.mesh.triangles {
            let [a, b, c] = face.map(|index| level.mesh.vertices[index as usize].position);
            assert!((b - a).cross(c - a).dot(a + b + c) > 0.0);
        }
        // Exercise the common angular domain from both triangulations. The
        // same-ray correspondence gives an independent two-way distance bound.
        for mesh in [&source, &level.mesh] {
            for face in &mesh.triangles {
                let [a, b, c] = face.map(|index| mesh.vertices[index as usize].position);
                for point in [a, (a + b + c) / 3.0, a.lerp(b, 0.5)] {
                    let direction = point.normalize();
                    let fine_height = radial_height(direction, &source);
                    let coarse_height = radial_height(direction, &level.mesh);
                    assert!((fine_height - coarse_height).abs() <= radial_error + 1.0e-10);
                }
            }
        }
    }
}

#[test]
fn textured_lods_release_hydraulic_features_but_keep_exact_borders_and_lod_zero() {
    let mut source = tile(0.0);
    source
        .vertices
        .iter_mut()
        .for_each(|vertex| vertex.protected = true);
    let constrained = build_lods(&source);
    assert_eq!(constrained[3].mesh.triangles.len(), source.triangles.len());
    let levels = build_unpinned_lods(&source);
    assert_eq!(levels[0].mesh, source);
    assert!(levels[3].mesh.triangles.len() < source.triangles.len() / 2);
    for level in &levels {
        let border = boundary(&level.mesh);
        for edge in boundary(&source) {
            assert!(border.iter().any(|candidate| same_edge(&edge, candidate)));
        }
    }
}

fn triangle_quality(mesh: &ChunkMesh, face: [u32; 3]) -> f64 {
    let [a, b, c] = face.map(|index| mesh.vertices[index as usize].position);
    let longest_squared = a
        .distance_squared(b)
        .max(b.distance_squared(c))
        .max(c.distance_squared(a));
    (b - a).cross(c - a).length() / longest_squared
}

#[test]
fn coarse_lods_do_not_turn_regular_terrain_into_thin_triangle_fans() {
    let mut source = tile(0.0);
    for vertex in &mut source.vertices {
        vertex.protected = false;
        vertex.position.z = 1.0;
    }
    let border = boundary(&source);
    for level in build_unpinned_lods(&source).iter().skip(1) {
        assert!(level.mesh.triangles.len() < source.triangles.len());
        for &face in &level.mesh.triangles {
            assert!(
                triangle_quality(&level.mesh, face) >= 0.1 - 1.0e-12,
                "LOD simplification generated a triangle with aspect ratio above 10:1"
            );
        }
        let mut degree = vec![0; level.mesh.vertices.len()];
        for &vertex in level.mesh.triangles.iter().flatten() {
            degree[vertex as usize] += 1;
        }
        assert!(
            degree.into_iter().all(|count| count <= 16),
            "oversized triangle fan"
        );
        let coarse_border = boundary(&level.mesh);
        assert_eq!(border.len(), coarse_border.len());
        assert!(
            border
                .iter()
                .all(|edge| coarse_border.iter().any(|other| same_edge(edge, other)))
        );
    }
}

#[test]
fn dense_border_slivers_survive_without_becoming_longer_and_thinner() {
    let mut source = tile(0.0);
    for vertex in &mut source.vertices {
        vertex.protected = false;
        vertex.position.z = 1.0;
        let x = vertex.position.x;
        vertex.position.x = if x <= 4.0 { x * 0.02 } else { 0.08 + x - 4.0 };
    }
    let minimum_quality = source
        .triangles
        .iter()
        .map(|&face| triangle_quality(&source, face))
        .fold(f64::INFINITY, f64::min);
    let border = boundary(&source);
    let levels = build_unpinned_lods(&source);
    assert!(levels[3].mesh.triangles.len() < source.triangles.len() / 2);
    for level in levels.iter().skip(1) {
        assert!(
            level
                .mesh
                .triangles
                .iter()
                .all(|&face| triangle_quality(&level.mesh, face) >= minimum_quality - 1.0e-12)
        );
        let coarse_border = boundary(&level.mesh);
        assert_eq!(border.len(), coarse_border.len());
        assert!(
            border
                .iter()
                .all(|edge| coarse_border.iter().any(|other| same_edge(edge, other)))
        );
    }
}
