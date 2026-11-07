use meshopt::{SimplifyOptions, simplify_with_attributes_and_locks_decoder};

use super::{Mesh, TriangleIndex, Vec2, Vec3};

const NORMAL_ATTRIBUTE_WEIGHT: f32 = 0.1;
const MAXIMUM_SIMPLIFICATION_ERROR: f32 = 1.0;

/// Rebuilds both coarser render meshes from the finished LOD0 surface.
///
/// The previous staged meshes still determine the established triangle budgets,
/// but none of their positions survive into the rendered LODs. LOD2 is reduced
/// from LOD1 so every coarser vertex is also present in the next finer mesh.
/// Perimeter constraints outrank target budgets: a tiny mesh may retain more
/// triangles when the requested count cannot represent all locked edge samples.
pub(super) fn regenerate_lods(lod0: &mut Mesh, lod1: &mut Mesh, lod2: &mut Mesh) -> TriangleIndex {
    let lod1_target = refined_index_budget(lod1);
    let lod2_target = refined_index_budget(lod2);

    refresh_surface_attributes(lod0);
    *lod1 = simplify_mesh(lod0, lod1_target);
    *lod2 = simplify_mesh(lod1, lod2_target);

    TriangleIndex::new(lod0)
}

pub(super) fn regenerate_coastal_lods(
    lod0: &mut Mesh,
    lod1: &mut Mesh,
    lod2: &mut Mesh,
    field: super::coastal_strip::CoastalStripField,
) -> TriangleIndex {
    // The fully retained collar is additional geometry. Reserve the existing
    // interior budgets too, rather than spending them all on locked faces.
    let collar_indices = coastal_collar_index_count(lod0);
    let lod1_target = refined_index_budget(lod1).saturating_add(collar_indices);
    let lod2_target = refined_index_budget(lod2).saturating_add(collar_indices);
    refresh_surface_attributes(lod0);
    field.apply_boundary_normals(lod0);
    *lod1 = simplify_mesh_preserving(
        lod0,
        lod1_target,
        Some(super::coastal_strip::CoastalStripField::is_blend_vertex),
    );
    *lod2 = simplify_mesh_preserving(
        lod1,
        lod2_target,
        Some(super::coastal_strip::CoastalStripField::is_blend_vertex),
    );
    TriangleIndex::new(lod0)
}

fn coastal_collar_index_count(mesh: &Mesh) -> usize {
    mesh.triangles
        .chunks_exact(3)
        .filter(|triangle| {
            triangle.iter().any(|&index| {
                super::coastal_strip::CoastalStripField::is_blend_vertex(
                    mesh.vertices[index as usize],
                )
            })
        })
        .count()
        .saturating_mul(3)
}

/// The whole-island horizon mesh is derived on demand from finished LOD2.
/// It is deliberately not cached in snapshots or divided into streaming tiles.
pub(super) fn simplify_horizon_mesh(lod2: &Mesh) -> Mesh {
    let target = (lod2.triangles.len() / 4 / 3 * 3).max(3);
    simplify_mesh(lod2, target)
}

fn refined_index_budget(mesh: &Mesh) -> usize {
    mesh.triangles.len().saturating_mul(4)
}

fn simplify_mesh(source: &Mesh, target_index_count: usize) -> Mesh {
    simplify_mesh_preserving(source, target_index_count, None)
}

fn simplify_mesh_preserving(
    source: &Mesh,
    target_index_count: usize,
    preserve: Option<fn(Vec3) -> bool>,
) -> Mesh {
    let mut simplified = simplify_mesh_inner(source, target_index_count, false, preserve);
    repair_coastal_simplification(&mut simplified, preserve);
    if projected_triangles_valid(&simplified) {
        return simplified;
    }
    // A valid 3D collapse can invert a height-field triangle in XY. Retry in
    // the support plane with height as a weighted attribute rather than move
    // generated vertices or repair rivers after they have been built.
    let mut planar = simplify_mesh_inner(source, target_index_count, true, preserve);
    repair_coastal_simplification(&mut planar, preserve);
    if projected_triangles_valid(&planar) {
        planar
    } else {
        source.clone()
    }
}

fn repair_coastal_simplification(mesh: &mut Mesh, preserve: Option<fn(Vec3) -> bool>) {
    if preserve.is_some() && !projected_triangles_valid(mesh) {
        // Repair a reduced mesh by flipping its shared diagonals. Positions,
        // including every locked collar sample, remain unchanged. Avoid
        // retaining the entire fine mesh for a few projected foldovers.
        let normals = std::mem::take(&mut mesh.normals);
        mesh.repair_projected_foldovers_preserving(|_, _| false);
        mesh.normals = normals;
        orient_coastal_slivers(mesh);
    }
}

/// River finishing can leave sub-centimetre folds which remain after diagonal
/// repairs. Correct only their winding; keep the exact sampled positions and
/// normals. Larger folds still require the planar retry or fine-mesh fallback.
fn orient_coastal_slivers(mesh: &mut Mesh) {
    let maximum_area = 1.0e-4 / f64::from(super::ISLAND_WORLD_METRES).powi(2);
    for triangle in mesh.triangles.chunks_exact_mut(3) {
        let area = crate::mesh::projected_area_twice(
            &mesh.vertices,
            [triangle[0], triangle[1], triangle[2]],
        );
        if area < 0.0 && area >= -maximum_area {
            triangle.swap(1, 2);
        }
    }
}

fn projected_triangles_valid(mesh: &Mesh) -> bool {
    mesh.triangles.chunks_exact(3).all(|triangle| {
        crate::mesh::projected_area_twice(&mesh.vertices, [triangle[0], triangle[1], triangle[2]])
            > f64::EPSILON
    })
}

fn simplify_mesh_inner(
    source: &Mesh,
    target_index_count: usize,
    planar: bool,
    preserve: Option<fn(Vec3) -> bool>,
) -> Mesh {
    let target_index_count = target_index_count
        .min(source.triangles.len())
        .saturating_sub(target_index_count.min(source.triangles.len()) % 3);
    if target_index_count < 3 || target_index_count == source.triangles.len() {
        return source.clone();
    }

    let positions = source
        .vertices
        .iter()
        .map(|position| {
            if planar {
                [position.x, position.y, 0.0]
            } else {
                position.to_array()
            }
        })
        .collect::<Vec<_>>();
    let normal_attributes = source
        .normals
        .iter()
        .zip(&source.vertices)
        .flat_map(|(normal, position)| [normal.x, normal.y, normal.z, position.z])
        .collect::<Vec<_>>();
    let mut vertex_locks = source.perimeter_mask();
    if let Some(preserve) = preserve {
        for (locked, &point) in vertex_locks.iter_mut().zip(&source.vertices) {
            *locked |= preserve(point);
        }
    }
    let triangles = simplify_with_attributes_and_locks_decoder(
        &source.triangles,
        &positions,
        &normal_attributes,
        &[
            NORMAL_ATTRIBUTE_WEIGHT,
            NORMAL_ATTRIBUTE_WEIGHT,
            NORMAL_ATTRIBUTE_WEIGHT,
            if planar { 1.0 } else { 0.0 },
        ],
        size_of::<[f32; 4]>(),
        &vertex_locks,
        target_index_count,
        MAXIMUM_SIMPLIFICATION_ERROR,
        SimplifyOptions::Regularize,
        None,
    );

    compact_mesh(source, triangles)
}

#[cfg(test)]
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
    fn validation_accepts_positive_nearly_collinear_faces() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.16026135, 0.20683776, 0.0),
                Vec3::new(0.68624616, 0.43698743, 0.0),
                Vec3::new(0.4153148, 0.31843883, 0.0),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        assert_eq!(mesh.projected_foldover_count(), 0);
        assert!(
            projected_triangles_valid(&mesh),
            "f32 cancellation must not force a full-detail fallback"
        );
    }

    #[test]
    fn coastal_diagonal_repairs_retain_fine_surface_normals() {
        let normal = (Vec3::X + Vec3::Z).normalize();
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, -1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(2.0, 0.0, 0.0),
            ],
            triangles: vec![0, 1, 2, 0, 2, 3],
            normals: vec![normal; 4],
            ..Mesh::default()
        };
        repair_coastal_simplification(&mut mesh, Some(|_| true));
        assert!(projected_triangles_valid(&mesh));
        assert!(mesh.normals.iter().all(|&value| value == normal));
    }

    #[test]
    fn coastal_slivers_keep_their_positions_and_normals_when_reoriented() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.7317489, 0.5421017, 0.017574841),
                Vec3::new(0.7317458, 0.54210496, 0.017569836),
                Vec3::new(0.7317487, 0.5421022, 0.01757404),
            ],
            triangles: vec![0, 1, 2],
            normals: vec![Vec3::Z; 3],
            ..Mesh::default()
        };
        let positions = mesh.vertices.clone();
        assert!(!projected_triangles_valid(&mesh));
        repair_coastal_simplification(&mut mesh, Some(|_| false));
        assert!(projected_triangles_valid(&mesh));
        assert_eq!(mesh.vertices, positions);
        assert_eq!(mesh.normals, vec![Vec3::Z; 3]);
    }

    #[test]
    fn coastal_sliver_repair_rejects_larger_foldovers() {
        let mut mesh = Mesh {
            vertices: vec![Vec3::ZERO, Vec3::Y, Vec3::X],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        orient_coastal_slivers(&mut mesh);
        assert!(!projected_triangles_valid(&mesh));
        assert_eq!(mesh.triangles, vec![0, 1, 2]);
    }

    #[test]
    fn coarser_lods_are_deterministic_subsets_of_the_finished_surface() {
        let mut lod2 = Mesh::delaunay(&super::super::generation::create_seed_points(42, 32));
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
    fn locked_perimeter_outweighs_an_impossible_tiny_triangle_budget() {
        let mut source = Mesh::delaunay(&[Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]);
        for _ in 0..3 {
            source = source.tessellated();
        }
        let boundary: Vec<_> = source
            .perimeter_vertices()
            .into_iter()
            .map(|i| source.vertices[i])
            .collect();
        let simplified = simplify_mesh(&source, 6);
        assert!(
            simplified.triangles.len() > 6,
            "two triangles cannot retain this perimeter"
        );
        assert!(boundary.iter().all(|p| simplified.vertices.contains(p)));
        assert!(projected_triangles_valid(&simplified));
    }

    #[test]
    fn horizon_lod_reduces_lod2_without_moving_vertices_or_domain_corners() {
        let mut lod2 = Mesh::delaunay(&[Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]);
        for _ in 0..4 {
            lod2 = lod2.tessellated();
        }
        for vertex in &mut lod2.vertices {
            vertex.z = (vertex.x * 3.0).sin() * (vertex.y * 3.0).sin() * 0.2;
        }
        lod2.calculate_normals();
        let horizon = simplify_horizon_mesh(&lod2);
        assert_eq!(horizon, simplify_horizon_mesh(&lod2));
        assert!(!horizon.triangles.is_empty());
        assert!(horizon.triangles.len() <= lod2.triangles.len() / 4);
        assert!(
            horizon
                .vertices
                .iter()
                .all(|vertex| lod2.vertices.contains(vertex))
        );
        assert!(
            horizon
                .triangles
                .iter()
                .all(|&index| (index as usize) < horizon.vertices.len())
        );
        for (vertex, locked) in lod2.vertices.iter().zip(domain_corner_locks(&lod2)) {
            if locked {
                assert!(horizon.vertices.contains(vertex));
            }
        }
        assert!(horizon.vertices.iter().map(|v| v.z).fold(0.0_f32, f32::max) > 0.15);
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
