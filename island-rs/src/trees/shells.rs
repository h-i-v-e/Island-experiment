//! Engine-neutral foliage detail, shared by Planet and Unity.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
use crate::{Mesh, Vec2, Vec3};
use std::f32::consts::{PI, TAU};

/// Shared shell geometry and its per-vertex normalized layer coordinate.
#[derive(Debug, Default)]
pub struct TreeShells {
    pub mesh: Mesh,
    pub layers: Vec<f32>,
}

/// Planet's four branch-following needle layers, or eight broadleaf layers.
/// Input and output use native XY/Z-up coordinates normalized to island size.
/// Conifer input must be the fine crown returned by `generate_conifer`.
#[must_use]
pub fn tree_shells(source: &Mesh, conifer: bool) -> TreeShells {
    let mut out = TreeShells::default();
    let radii = if conifer {
        component_radii(source)
    } else {
        Vec::new()
    };
    let count = if conifer { 4 } else { 8 };
    for layer in 1..=count {
        let fraction = layer as f32 / count as f32;
        if conifer {
            for face in source.triangles.chunks_exact(3) {
                let angles: [f32; 3] = std::array::from_fn(|i| {
                    let p = source.vertices[face[i] as usize];
                    p.y.atan2(p.x)
                });
                let wrap = angles.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                    - angles.iter().copied().fold(f32::INFINITY, f32::min)
                    > PI;
                // Derive physical UV scale from the connected tier, so extra
                // subdivision cannot turn vertex indices into the wrong cone.
                let radius = radii[face[0] as usize];
                for (corner, &id) in face.iter().enumerate() {
                    let p = source.vertices[id as usize];
                    let normal = source.normals[id as usize];
                    let radial = Vec3::new(p.x, p.y, 0.0).normalize_or_zero();
                    let direction = (normal * 0.60 + radial * 0.85 - Vec3::Z * 0.35).normalize();
                    out.mesh.triangles.push(out.mesh.vertices.len() as u32);
                    out.mesh
                        .vertices
                        .push(p + direction * (fraction * 0.12 / crate::ISLAND_WORLD_METRES));
                    out.mesh.normals.push(normal);
                    let angle = angles[corner]
                        + if wrap && angles[corner] < 0.0 {
                            TAU
                        } else {
                            0.0
                        };
                    out.mesh
                        .uv
                        .push(Vec2::new(angle * radius, p.z * crate::ISLAND_WORLD_METRES));
                    out.layers.push(fraction);
                }
            }
        } else {
            let offset = out.mesh.vertices.len() as u32;
            for (&p, &normal) in source.vertices.iter().zip(&source.normals) {
                out.mesh
                    .vertices
                    .push(p + normal * (fraction * 0.45 / crate::ISLAND_WORLD_METRES));
                out.mesh.normals.push(normal);
                out.mesh.uv.push(Vec2::ZERO);
                out.layers.push(fraction);
            }
            out.mesh
                .triangles
                .extend(source.triangles.iter().map(|&i| i + offset));
        }
    }
    out
}

fn component_radii(source: &Mesh) -> Vec<f32> {
    let adjacency = source.adjacency();
    let mut components = vec![usize::MAX; source.vertices.len()];
    let mut radii = Vec::new();
    let mut pending = Vec::new();
    for start in 0..source.vertices.len() {
        if components[start] != usize::MAX {
            continue;
        }
        let component = radii.len();
        let mut radius = 0.0_f32;
        components[start] = component;
        pending.push(start);
        while let Some(index) = pending.pop() {
            radius = radius.max(source.vertices[index].truncate().length());
            for &neighbour in &adjacency[index] {
                if components[neighbour] == usize::MAX {
                    components[neighbour] = component;
                    pending.push(neighbour);
                }
            }
        }
        radii.push(radius * crate::ISLAND_WORLD_METRES);
    }
    components
        .into_iter()
        .map(|component| radii[component])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pine_needle_uvs_follow_the_tier_instead_of_vertex_numbering() {
        let source = crate::generate_conifer(3).lod0_foliage;
        let expected = tree_shells(&source, true);
        let mut reordered = source.clone();
        reordered.vertices.reverse();
        reordered.normals.reverse();
        reordered.uv.reverse();
        let last = u32::try_from(reordered.vertices.len() - 1).unwrap();
        for index in &mut reordered.triangles {
            *index = last - *index;
        }
        let actual = tree_shells(&reordered, true);
        assert_eq!(actual.mesh, expected.mesh);
        assert_eq!(actual.layers, expected.layers);
    }

    #[test]
    fn shared_shells_retain_planet_layers_and_physical_needle_lengths() {
        let tree = crate::generate_conifer(0);
        let shells = tree_shells(&tree.lod0_foliage, true);
        assert_eq!(shells.mesh.triangles.len() / 3, 3072);
        let per_layer = tree.lod0_foliage.triangles.len();
        for layer in 0..4 {
            for (corner, &id) in tree.lod0_foliage.triangles.iter().enumerate() {
                let index = layer * per_layer + corner;
                let distance = (shells.mesh.vertices[index]
                    - tree.lod0_foliage.vertices[id as usize])
                    .length()
                    * crate::ISLAND_WORLD_METRES;
                assert!((distance - (layer + 1) as f32 * 0.03).abs() < 0.001);
                assert!(shells.mesh.uv[index].is_finite());
            }
        }
        let broadleaf = tree_shells(&tree.lod1_foliage, false);
        assert_eq!(
            broadleaf.mesh.vertices.len(),
            tree.lod1_foliage.vertices.len() * 8
        );
        assert_eq!(broadleaf.layers.len(), broadleaf.mesh.vertices.len());
    }
}
