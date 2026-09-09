//! Round the actual joined surface, sharing positions and normals across blocks.
//! The floor and the portion used by the entrance join retain their exact shape.
use super::{Cave, CaveOptions, volume::ENTRANCE_BLEND_LENGTH};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec3};
use std::collections::HashMap;

pub(super) fn round_interior(mesh: &mut Mesh, cave: &Cave, options: &CaveOptions) {
    let throat = cave.portal.throat();
    smooth(mesh, 0.6 / ISLAND_WORLD_METRES, |position| {
        let p = position * ISLAND_WORLD_METRES;
        let floor =
            ((p.z - cave.entrance.z - options.floor_roughness - 0.15) / 0.8).clamp(0.0, 1.0);
        let entrance =
            ((p - throat).truncate().dot(cave.inward) - ENTRANCE_BLEND_LENGTH - options.voxel_size)
                .clamp(0.0, 1.0);
        let weight = floor.min(entrance);
        weight * weight * (3.0 - 2.0 * weight)
    });
}

fn smooth(mesh: &mut Mesh, limit: f32, mobility: impl Fn(Vec3) -> f32) {
    let mut unique = HashMap::new();
    let mut original = Vec::new();
    let indices: Vec<_> = mesh
        .vertices
        .iter()
        .map(|&p| {
            *unique
                .entry(p.to_array().map(f32::to_bits))
                .or_insert_with(|| {
                    original.push(p);
                    original.len() - 1
                })
        })
        .collect();
    let faces: Vec<_> = mesh
        .triangles
        .chunks_exact(3)
        .map(|t| {
            [
                indices[t[0] as usize],
                indices[t[1] as usize],
                indices[t[2] as usize],
            ]
        })
        .collect();
    let mut adjacent = vec![Vec::new(); original.len()];
    for &[a, b, c] in &faces {
        adjacent[a].extend([b, c]);
        adjacent[b].extend([c, a]);
        adjacent[c].extend([a, b]);
    }
    for neighbours in &mut adjacent {
        neighbours.sort_unstable();
        neighbours.dedup();
    }
    let mut weights: Vec<_> = original.iter().map(|&p| mobility(p)).collect();
    // Tiny contours can still have precision seams awaiting the existing weld
    // pass. Keep their endpoints fixed so relaxation cannot widen those seams.
    let mut edges = HashMap::new();
    for &[a, b, c] in &faces {
        for (start, end) in [(a, b), (b, c), (c, a)] {
            *edges.entry((start.min(end), start.max(end))).or_insert(0) += 1;
        }
    }
    for ((a, b), count) in edges {
        if count != 2 {
            weights[a] = 0.0;
            weights[b] = 0.0;
        }
    }
    // Two working buffers are needed for simultaneous, order-independent steps.
    let mut positions = original.clone();
    let mut next = original.clone();
    for _ in 0..12 {
        for (i, neighbours) in adjacent.iter().enumerate() {
            if neighbours.is_empty() || weights[i] == 0.0 {
                continue;
            }
            let delta = neighbours
                .iter()
                .map(|&j| positions[j] - positions[i])
                .sum::<Vec3>()
                / neighbours.len() as f32;
            let offset = positions[i] + delta * (0.5 * weights[i]) - original[i];
            next[i] = original[i] + offset.clamp_length_max(limit * weights[i]);
        }
        prevent_foldovers(&positions, &mut next, &faces);
        std::mem::swap(&mut positions, &mut next);
        next.copy_from_slice(&positions);
    }
    // The old analytic normals came from the unsmoothed hard union and could
    // point through a neighbouring face on a narrow pillar. Use the actual mesh.
    let mut normals = vec![glam::DVec3::ZERO; positions.len()];
    for &face in &faces {
        let [a, b, c] = face.map(|i| positions[i].as_dvec3());
        let normal = (b - a).cross(c - a);
        for i in face {
            normals[i] += normal;
        }
    }
    for (i, &shared) in indices.iter().enumerate() {
        mesh.vertices[i] = positions[shared];
        mesh.uv[i] = positions[shared].truncate();
        if weights[shared] > 0.0 {
            let normal = normals[shared].normalize_or_zero().as_vec3();
            mesh.normals[i] = mesh.normals[i]
                .lerp(normal, weights[shared])
                .normalize_or_zero();
        }
    }
}

// Reject local moves that collapse or turn a face inside out. Recheck neighbours
// after a rejection; each iteration fixes at least one additional moved vertex.
fn prevent_foldovers(previous: &[Vec3], next: &mut [Vec3], faces: &[[usize; 3]]) {
    loop {
        let mut rejected = false;
        for &face in faces {
            let old = face.map(|i| previous[i].as_dvec3());
            let new = face.map(|i| next[i].as_dvec3());
            let before = (old[1] - old[0]).cross(old[2] - old[0]);
            let after = (new[1] - new[0]).cross(new[2] - new[0]);
            if before.dot(after) <= before.length_squared() * 0.05 {
                for i in face {
                    rejected |= next[i] != previous[i];
                    next[i] = previous[i];
                }
            }
        }
        if !rejected {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_crease_without_splitting_shared_vertices_or_moving_anchors() {
        // A 90-degree crease, deliberately stored with separate vertices per
        // triangle as happens where independently sampled blocks meet.
        let mut mesh = Mesh::default();
        for y in -4..4 {
            for x in -4..4 {
                let corners = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)]
                    .map(|(x, y)| Vec3::new(x as f32, y as f32, (x as f32).abs()) * 0.5);
                for triangle in [[0, 1, 2], [2, 1, 3]] {
                    for index in triangle {
                        mesh.triangles.push(mesh.vertices.len() as u32);
                        mesh.vertices.push(corners[index]);
                        mesh.uv.push(corners[index].truncate());
                    }
                }
            }
        }
        mesh.normals = mesh
            .vertices
            .iter()
            .map(|p| Vec3::new(-p.x.signum(), 0.0, 1.0).normalize())
            .collect();
        let before = mesh.clone();
        smooth(&mut mesh, 0.6, |_| 1.0);
        assert_eq!(mesh.triangles, before.triangles);
        let mut shared = HashMap::new();
        let mut rounded = false;
        for (i, &p) in before.vertices.iter().enumerate() {
            let actual = (mesh.vertices[i], mesh.normals[i]);
            assert_eq!(
                *shared
                    .entry(p.to_array().map(f32::to_bits))
                    .or_insert(actual),
                actual
            );
            assert!(actual.0.distance(p) <= 0.60001);
            if p.x.abs() >= 1.9 || p.y.abs() >= 1.9 {
                assert_eq!(actual.0, p);
            }
            if p == Vec3::ZERO {
                assert!(actual.0.z > 0.2, "The crease must actually become rounded.");
                assert!(
                    actual.1.z > 0.95,
                    "The normal must follow the rounded crest."
                );
                rounded = true;
            }
        }
        assert!(rounded);
        for t in mesh.triangles.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize]);
            assert!((b - a).cross(c - a).z > 0.0);
        }
    }
}
