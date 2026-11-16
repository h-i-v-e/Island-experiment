//! Exact, local cleanup after geometry generation, before baking and hashing.
//!
//! A mesh part is the ownership boundary: no other material, visibility group,
//! moving door or LOD can hide its surfaces. Attribute keys deliberately retain
//! signed zero, normal and UV seams. No positional tolerance is used.
use crate::{BuildingLod, BuildingMesh, DoorDescriptor, MaterialSlot};
use std::collections::{HashMap, HashSet};

type VertexKey = [u32; 8];

pub(crate) fn lod(lod: &mut BuildingLod) {
    for part in &mut lod.parts {
        let opaque = !matches!(
            part.material,
            MaterialSlot::Glass
                | MaterialSlot::WindowProxy
                | MaterialSlot::ThatchEnd
                | MaterialSlot::ThatchShell1
                | MaterialSlot::ThatchShell2
                | MaterialSlot::ThatchShell3
                | MaterialSlot::ThatchShell4
        );
        mesh(&mut part.mesh, opaque);
    }
    lod.parts.retain(|part| !part.mesh.indices.is_empty());
}

pub(crate) fn door(door: &mut DoorDescriptor) {
    mesh(&mut door.mesh, true);
    mesh(&mut door.hardware_mesh, true);
}

/// Weld equal complete vertices and remove collapsed and identical opaque faces.
/// Alpha surfaces retain face multiplicity, which can affect blending or masking.
fn mesh(mesh: &mut BuildingMesh, deduplicate_faces: bool) {
    if !well_formed(mesh) {
        return;
    }
    let mut vertices = HashMap::with_capacity(mesh.positions.len());
    let mut faces = HashSet::new();
    let mut cleaned = BuildingMesh::default();
    for triangle in mesh.indices.chunks_exact(3) {
        let source = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if collapsed(source.map(|i| mesh.positions[i])) {
            continue;
        }
        // Collapsed generator faces may carry NaN normals from normalization.
        // Only their discarded attributes may be nonfinite: malformed retained
        // surfaces leave the entire original mesh intact for validation.
        if source.iter().any(|&index| {
            !mesh.normals[index]
                .iter()
                .chain(&mesh.uvs[index])
                .all(|v| v.is_finite())
        }) {
            return;
        }
        let mapped = source.map(|index| {
            let key = vertex_key(mesh, index);
            let next = u32::try_from(cleaned.positions.len()).expect("validated mesh budget");
            *vertices.entry(key).or_insert_with(|| {
                cleaned.positions.push(mesh.positions[index]);
                cleaned.normals.push(mesh.normals[index]);
                cleaned.uvs.push(mesh.uvs[index]);
                next
            })
        });
        if !deduplicate_faces || faces.insert(oriented_key(mapped)) {
            cleaned.indices.extend(mapped);
        }
    }
    *mesh = cleaned;
}

fn well_formed(mesh: &BuildingMesh) -> bool {
    mesh.positions.len() == mesh.normals.len()
        && mesh.positions.len() == mesh.uvs.len()
        && mesh.indices.len().is_multiple_of(3)
        && mesh
            .indices
            .iter()
            .all(|i| (*i as usize) < mesh.positions.len())
        && mesh.positions.iter().flatten().all(|v| v.is_finite())
}

fn vertex_key(mesh: &BuildingMesh, index: usize) -> VertexKey {
    let [px, py, pz] = mesh.positions[index];
    let [nx, ny, nz] = mesh.normals[index];
    let [u, v] = mesh.uvs[index];
    [px, py, pz, nx, ny, nz, u, v].map(f32::to_bits)
}

fn oriented_key(keys: [u32; 3]) -> [u32; 3] {
    let [a, b, c] = keys;
    keys.min([b, c, a]).min([c, a, b])
}

#[allow(clippy::float_cmp)] // Exact collapse only; near-coincident geometry is retained.
fn collapsed([a, b, c]: [[f32; 3]; 3]) -> bool {
    a == b || b == c || c == a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MeshPart, geometry::Builder};

    fn triangle() -> BuildingMesh {
        let mut builder = Builder::default();
        builder.triangle(
            MaterialSlot::Wall,
            [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            [[0., 0.], [1., 0.], [0., 1.]],
        );
        builder.finish().parts.remove(0).mesh
    }

    fn append(mesh: &mut BuildingMesh, other: &BuildingMesh, order: [u32; 3]) {
        let start = u32::try_from(mesh.positions.len()).unwrap();
        mesh.positions.extend(&other.positions);
        mesh.normals.extend(&other.normals);
        mesh.uvs.extend(&other.uvs);
        mesh.indices.extend(order.map(|i| start + i));
    }

    #[test]
    fn exact_duplicate_faces_weld_but_opposing_faces_and_seams_survive() {
        let original = triangle();
        let mut combined = original.clone();
        append(&mut combined, &original, [1, 2, 0]);
        append(&mut combined, &original, [0, 2, 1]);
        let mut seam = original.clone();
        seam.uvs[0][0] = 0.25;
        append(&mut combined, &seam, [0, 1, 2]);
        let mut normal_seam = original;
        normal_seam.normals[0] = [0., 1., 0.];
        append(&mut combined, &normal_seam, [0, 1, 2]);
        mesh(&mut combined, true);
        assert_eq!(combined.triangle_count(), 4);
        assert_eq!(combined.positions.len(), 5);
        let once = combined.clone();
        mesh(&mut combined, true);
        assert_eq!(combined, once, "cleanup is stable and idempotent");
    }

    #[test]
    fn collapsed_faces_and_unused_vertices_are_removed_without_area_threshold() {
        let mut combined = triangle();
        combined.indices.extend([0, 0, 1]);
        let mut tiny = triangle();
        tiny.positions[1][0] = f32::MIN_POSITIVE;
        tiny.positions[2][1] = f32::MIN_POSITIVE;
        append(&mut combined, &tiny, [0, 1, 2]);
        combined.positions.push([5.; 3]);
        combined.normals.push([0.; 3]);
        combined.uvs.push([0.; 2]);
        mesh(&mut combined, true);
        assert_eq!(combined.triangle_count(), 2);
        assert_eq!(combined.positions.len(), 5);
    }

    #[test]
    fn collapsed_generator_face_drops_its_nonfinite_normal_before_attribute_validation() {
        let original = triangle();
        let mut builder = Builder::default();
        builder.triangle(
            MaterialSlot::Wall,
            [[0., 0., 0.], [0., 0., 0.], [1., 0., 0.]],
            [[0., 0.]; 3],
        );
        let collapsed = builder.finish().parts.remove(0).mesh;
        assert!(collapsed.normals.iter().flatten().any(|v| !v.is_finite()));
        let mut combined = original.clone();
        append(&mut combined, &collapsed, [0, 1, 2]);
        mesh(&mut combined, true);
        assert_eq!(combined, original);
    }

    #[test]
    fn nonfinite_attributes_on_retained_faces_leave_the_original_mesh_intact() {
        for invalid_uv in [false, true] {
            let mut malformed = triangle();
            // An earlier removable triangle must not partially mutate the mesh
            // before a later retained triangle fails attribute validation.
            drop(malformed.indices.splice(0..0, [0, 0, 1]));
            if invalid_uv {
                malformed.uvs[1][0] = f32::INFINITY;
            } else {
                malformed.normals[1][0] = f32::INFINITY;
            }
            let original = malformed.clone();
            mesh(&mut malformed, true);
            assert_eq!(malformed, original);
        }
    }

    #[test]
    fn masked_alpha_and_independent_parts_keep_face_multiplicity() {
        for material in [
            MaterialSlot::Glass,
            MaterialSlot::WindowProxy,
            MaterialSlot::ThatchEnd,
            MaterialSlot::ThatchShell1,
            MaterialSlot::ThatchShell2,
            MaterialSlot::ThatchShell3,
            MaterialSlot::ThatchShell4,
        ] {
            let original = triangle();
            let mut duplicate = original.clone();
            append(&mut duplicate, &original, [0, 1, 2]);
            let mut grouped = BuildingLod {
                parts: vec![
                    MeshPart {
                        material,
                        mesh: duplicate,
                    },
                    MeshPart {
                        material,
                        mesh: original,
                    },
                ],
            };
            lod(&mut grouped);
            assert_eq!(grouped.parts.len(), 2);
            assert_eq!(grouped.triangle_count(), 3);
        }
    }

    #[test]
    fn malformed_meshes_are_left_for_validation() {
        let mut malformed = triangle();
        malformed.uvs.clear();
        let original = malformed.clone();
        mesh(&mut malformed, true);
        assert_eq!(malformed, original);
    }
}
