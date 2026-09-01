use meshopt::{SimplifyOptions, simplify_with_attributes_and_locks_decoder};

use super::{Mesh, TriangleIndex, Vec2, Vec3};

const NORMAL_ATTRIBUTE_WEIGHT: f32 = 0.1;
const MAXIMUM_SIMPLIFICATION_ERROR: f32 = 1.0;

/// Rebuilds both coarser render meshes from the finished LOD0 surface.
///
/// The previous staged meshes still determine the established triangle budgets,
/// but none of their positions survive into the rendered LODs. LOD2 is reduced
/// from LOD1 so every coarser vertex is also present in the next finer mesh.
pub(super) fn regenerate_lods(lod0: &mut Mesh, lod1: &mut Mesh, lod2: &mut Mesh) -> TriangleIndex {
    let lod1_target = refined_index_budget(lod1);
    let lod2_target = refined_index_budget(lod2);

    refresh_surface_attributes(lod0);
    *lod1 = simplify_mesh(lod0, lod1_target);
    *lod2 = simplify_mesh(lod1, lod2_target);

    TriangleIndex::new(lod0)
}

fn refined_index_budget(mesh: &Mesh) -> usize {
    mesh.triangles.len().saturating_mul(4)
}

fn simplify_mesh(source: &Mesh, target_index_count: usize) -> Mesh {
    let target_index_count = target_index_count
        .min(source.triangles.len())
        .saturating_sub(target_index_count.min(source.triangles.len()) % 3);
    if target_index_count < 3 || target_index_count == source.triangles.len() {
        return source.clone();
    }

    let positions = source
        .vertices
        .iter()
        .map(Vec3::to_array)
        .collect::<Vec<_>>();
    let normal_attributes = source
        .normals
        .iter()
        .flat_map(Vec3::to_array)
        .collect::<Vec<_>>();
    let vertex_locks = domain_corner_locks(source);
    let triangles = simplify_with_attributes_and_locks_decoder(
        &source.triangles,
        &positions,
        &normal_attributes,
        &[NORMAL_ATTRIBUTE_WEIGHT; 3],
        size_of::<[f32; 3]>(),
        &vertex_locks,
        target_index_count,
        MAXIMUM_SIMPLIFICATION_ERROR,
        SimplifyOptions::Regularize,
        None,
    );

    compact_mesh(source, triangles)
}

fn domain_corner_locks(mesh: &Mesh) -> Vec<bool> {
    let bounds = mesh.vertices.iter().fold(
        (f32::MAX, f32::MIN, f32::MAX, f32::MIN),
        |(minimum_x, maximum_x, minimum_y, maximum_y), vertex| {
            (
                minimum_x.min(vertex.x),
                maximum_x.max(vertex.x),
                minimum_y.min(vertex.y),
                maximum_y.max(vertex.y),
            )
        },
    );
    mesh.vertices
        .iter()
        .map(|vertex| {
            let x = vertex.x.to_bits();
            let y = vertex.y.to_bits();
            (x == bounds.0.to_bits() || x == bounds.1.to_bits())
                && (y == bounds.2.to_bits() || y == bounds.3.to_bits())
        })
        .collect()
}

fn compact_mesh(source: &Mesh, triangles: Vec<u32>) -> Mesh {
    let mut source_to_compact = vec![u32::MAX; source.vertices.len()];
    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    let mut uv = Vec::new();
    let triangles = triangles
        .into_iter()
        .map(|source_index| {
            let source_index = source_index as usize;
            let mapped = &mut source_to_compact[source_index];
            if *mapped == u32::MAX {
                *mapped = vertices.len() as u32;
                let vertex = source.vertices[source_index];
                vertices.push(vertex);
                normals.push(source.normals.get(source_index).copied().unwrap_or(Vec3::Z));
                uv.push(
                    source
                        .uv
                        .get(source_index)
                        .copied()
                        .unwrap_or_else(|| vertex.truncate()),
                );
            }
            *mapped
        })
        .collect();
    Mesh {
        vertices,
        normals,
        triangles,
        uv,
    }
}

fn refresh_surface_attributes(mesh: &mut Mesh) {
    mesh.uv = mesh
        .vertices
        .iter()
        .map(|vertex| Vec2::new(vertex.x, vertex.y))
        .collect();
    mesh.calculate_normals();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Vec2, Vec3, terrain::sample_mesh_surface};

    #[test]
    fn coarser_lods_are_deterministic_subsets_of_the_finished_surface() {
        let mut lod2 = Mesh::delaunay(&[Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]);
        let mut lod1 = lod2.tessellated();
        let mut lod0 = lod1.tessellated().tessellated();
        lod0.vertices.iter_mut().for_each(|vertex| {
            vertex.z = vertex.x.mul_add(vertex.x * 0.2, vertex.y * vertex.y * 0.1);
        });
        lod0.calculate_normals();
        let source = lod0.clone();
        let source_corners = domain_corner_locks(&source);
        let expected_lod1_maximum = lod1.triangles.len() * 4;
        let expected_lod2_maximum = lod2.triangles.len() * 4;

        let index = regenerate_lods(&mut lod0, &mut lod1, &mut lod2);
        let mut repeated_lod1 = Mesh::default();
        let mut repeated_lod2 = Mesh::default();
        let mut repeated_lod0 = source.clone();
        // Supply the same established budgets as the staged inputs.
        repeated_lod1.triangles.resize(expected_lod1_maximum / 4, 0);
        repeated_lod2.triangles.resize(expected_lod2_maximum / 4, 0);
        regenerate_lods(&mut repeated_lod0, &mut repeated_lod1, &mut repeated_lod2);

        assert!(lod1.triangles.len() <= expected_lod1_maximum);
        assert!(lod2.triangles.len() <= expected_lod2_maximum);
        assert!(lod2.triangles.len() < lod1.triangles.len());
        assert_eq!(lod1, repeated_lod1);
        assert_eq!(lod2, repeated_lod2);
        for mesh in [&lod1, &lod2] {
            assert!(
                mesh.triangles
                    .iter()
                    .all(|&vertex| { (vertex as usize) < mesh.vertices.len() })
            );
            assert!(mesh.vertices.iter().all(|vertex| {
                source.vertices.contains(vertex)
                    && (sample_mesh_surface(&lod0, &index, vertex.x, vertex.y).0 - vertex.z).abs()
                        < 1.0e-6
            }));
        }
        for (corner, &locked) in source_corners.iter().enumerate() {
            if locked {
                assert!(lod1.vertices.contains(&source.vertices[corner]));
                assert!(lod2.vertices.contains(&source.vertices[corner]));
            }
        }
    }

    #[test]
    fn compact_mesh_preserves_selected_vertex_attributes() {
        let mut source = Mesh {
            vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::ONE],
            normals: Vec::new(),
            triangles: vec![0, 1, 2, 1, 3, 2],
            uv: vec![Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE],
        };
        source.calculate_normals();

        let compact = compact_mesh(&source, vec![1, 3, 2]);

        assert_eq!(compact.vertices, vec![Vec3::X, Vec3::ONE, Vec3::Y]);
        assert_eq!(
            compact.normals,
            vec![source.normals[1], source.normals[3], source.normals[2]]
        );
        assert_eq!(compact.uv, vec![Vec2::X, Vec2::ONE, Vec2::Y]);
        assert_eq!(compact.triangles, vec![0, 1, 2]);
    }
}
