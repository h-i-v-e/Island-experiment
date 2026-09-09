//! Orient connected triangle surfaces by topology. Density gradients provide
//! the component's overall air-facing direction, never per-triangle decisions.
use crate::Mesh;
use std::collections::HashMap;

/// Cube contours have shared positions even across independently sampled chunks.
/// Work on triangle indices in place; no mesh or vertex buffers are duplicated.
pub(crate) fn orient(mesh: &mut Mesh) -> Result<(), String> {
    // Tiny corner contours can collapse when converted to stored f32 island
    // coordinates. A zero-area face would register the same edge twice and
    // falsely connect three neighbouring faces; it has no renderable surface.
    let mut written = 0;
    for read in (0..mesh.triangles.len()).step_by(3) {
        let triangle = [
            mesh.triangles[read],
            mesh.triangles[read + 1],
            mesh.triangles[read + 2],
        ];
        let points = triangle.map(|i| mesh.vertices[i as usize].as_dvec3());
        if (points[1] - points[0])
            .cross(points[2] - points[0])
            .length_squared()
            > 0.0
        {
            mesh.triangles[written..written + 3].copy_from_slice(&triangle);
            written += 3;
        }
    }
    mesh.triangles.truncate(written);
    let count = mesh.triangles.len() / 3;
    let mut neighbours = vec![[None; 3]; count];
    let mut edges = HashMap::new();
    for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
        for side in 0..3 {
            let a = mesh.vertices[triangle[side] as usize]
                .to_array()
                .map(f32::to_bits);
            let b = mesh.vertices[triangle[(side + 1) % 3] as usize]
                .to_array()
                .map(f32::to_bits);
            if a == b {
                continue;
            }
            let forward = a < b;
            let key = if forward { [a, b] } else { [b, a] };
            let previous = edges.entry(key).or_insert((face, side, forward, false));
            if previous.0 == face {
                continue;
            }
            if previous.3 {
                return Err("cave volume contains a non-manifold edge".into());
            }
            previous.3 = true;
            let opposite_flip = forward == previous.2;
            neighbours[face][side] = Some((previous.0, opposite_flip));
            neighbours[previous.0][previous.1] = Some((face, opposite_flip));
        }
    }
    let mut flips = vec![None; count];
    let mut component = Vec::new();
    for root in 0..count {
        if flips[root].is_some() {
            continue;
        }
        component.clear();
        component.push(root);
        flips[root] = Some(false);
        let mut cursor = 0;
        let mut air_facing = 0.0;
        while cursor < component.len() {
            let face = component[cursor];
            let flip = flips[face].unwrap();
            let triangle = &mesh.triangles[face * 3..face * 3 + 3];
            let points = [triangle[0], triangle[1], triangle[2]]
                .map(|i| mesh.vertices[i as usize].as_dvec3());
            let gradient = triangle
                .iter()
                .map(|&i| mesh.normals[i as usize].as_dvec3())
                .sum::<glam::DVec3>();
            let facing = (points[1] - points[0])
                .cross(points[2] - points[0])
                .dot(gradient);
            air_facing += if flip { -facing } else { facing };
            for &(next, opposite_flip) in neighbours[face].iter().flatten() {
                let expected = flip ^ opposite_flip;
                if let Some(actual) = flips[next] {
                    if actual != expected {
                        return Err("cave volume has conflicting surface orientation".into());
                    }
                } else {
                    flips[next] = Some(expected);
                    component.push(next);
                }
            }
            cursor += 1;
        }
        // Gradient votes across the whole connected surface are stable even
        // when a thin pillar or saddle makes one local gradient point backwards.
        if air_facing < 0.0 {
            for &face in &component {
                flips[face] = Some(!flips[face].unwrap());
            }
        }
    }
    for (triangle, flip) in mesh.triangles.chunks_exact_mut(3).zip(flips) {
        if flip == Some(true) {
            triangle.swap(1, 2);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec3;

    #[test]
    fn collapsed_corner_does_not_corrupt_connected_surface_facing() {
        let points = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z];
        let centre = Vec3::splat(0.25);
        let mut mesh = Mesh {
            vertices: points.to_vec(),
            normals: points.iter().map(|p| (*p - centre).normalize()).collect(),
            // The first face is reversed. The last is a collapsed corner that
            // would make an otherwise manifold edge appear to have three faces.
            triangles: vec![0, 1, 2, 0, 1, 3, 0, 3, 2, 1, 2, 3, 0, 1, 0],
            ..Mesh::default()
        };
        orient(&mut mesh).unwrap();
        assert_eq!(mesh.vertices, points);
        assert_eq!(mesh.triangles.len(), 12);
        for t in mesh.triangles.chunks_exact(3) {
            let [a, b, c] = [
                points[t[0] as usize],
                points[t[1] as usize],
                points[t[2] as usize],
            ];
            assert!((b - a).cross(c - a).dot((a + b + c) / 3.0 - centre) > 0.0);
        }
    }
}
