use super::{ISLAND_WORLD_METRES, Mesh, SurfaceMaterial};

const MAXIMUM_UPLIFT: f32 = 2.0 / ISLAND_WORLD_METRES;
const COASTAL_RING_HEIGHT: f32 = 0.01 / ISLAND_WORLD_METRES;

/// Applies the experimental hardness-weighted uplift and inserts a shared
/// coastal ring before the final adaptive LOD0 pass.
///
/// Only existing land is uplifted. The height-plane split then replaces every
/// triangle crossing the slightly raised coast with conforming land and sea
/// triangles that share newly inserted ring vertices.
pub(super) fn prepare(terrain: &mut Mesh, material: &mut SurfaceMaterial) -> usize {
    debug_assert_eq!(terrain.vertices.len(), material.hardnesses().len());
    terrain
        .vertices
        .iter_mut()
        .zip(material.hardnesses())
        .filter(|(vertex, _)| vertex.z > 0.0)
        .for_each(|(vertex, &hardness)| {
            vertex.z += hardness.clamp(0.0, 1.0) * MAXIMUM_UPLIFT;
        });

    let constrained = terrain.constrain_height_plane(COASTAL_RING_HEIGHT);
    material.extend_after_edge_splits(&constrained.splits);
    terrain.calculate_normals();
    constrained.splits.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Vec2, Vec3};

    #[test]
    fn uplift_is_linear_in_hardness_and_does_not_raise_seabed() {
        let mut terrain = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.1),
                Vec3::new(1.0, 0.0, 0.1),
                Vec3::new(2.0, 0.0, -0.1),
            ],
            ..Mesh::default()
        };
        let mut material = SurfaceMaterial::empty(terrain.vertices.len());
        material.bedrock_hardness = vec![0.0, 1.0, 1.0];

        assert_eq!(prepare(&mut terrain, &mut material), 0);
        assert!((terrain.vertices[0].z - 0.1).abs() <= f32::EPSILON);
        assert!((terrain.vertices[1].z - (0.1 + MAXIMUM_UPLIFT)).abs() <= f32::EPSILON);
        assert!((terrain.vertices[2].z + 0.1).abs() <= f32::EPSILON);
    }

    #[test]
    fn coastal_ring_is_watertight_and_extends_material_fields() {
        let mut terrain = Mesh {
            vertices: vec![
                Vec3::new(-1.0, -1.0, -0.1),
                Vec3::new(1.0, -1.0, -0.1),
                Vec3::new(1.0, 1.0, -0.1),
                Vec3::new(-1.0, 1.0, -0.1),
                Vec3::new(0.0, 0.0, 0.1),
            ],
            triangles: vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4],
            uv: vec![Vec2::ZERO; 5],
            ..Mesh::default()
        };
        let original_vertex_count = terrain.vertices.len();
        let mut material = SurfaceMaterial::empty(original_vertex_count);
        material.bedrock_hardness = vec![0.0, 0.25, 0.5, 0.75, 1.0];

        assert_eq!(prepare(&mut terrain, &mut material), 4);
        assert_eq!(terrain.vertices.len(), original_vertex_count + 4);
        assert_eq!(terrain.vertices.len(), terrain.normals.len());
        assert_eq!(terrain.vertices.len(), terrain.uv.len());
        assert_eq!(terrain.vertices.len(), material.hardnesses().len());
        assert_eq!(terrain.vertices.len(), material.depths().len());
        assert!(
            terrain.vertices[original_vertex_count..]
                .iter()
                .all(|vertex| (vertex.z - COASTAL_RING_HEIGHT).abs() <= f32::EPSILON)
        );
        assert!(terrain.triangles.chunks_exact(3).all(|triangle| {
            let heights = [
                terrain.vertices[triangle[0] as usize].z,
                terrain.vertices[triangle[1] as usize].z,
                terrain.vertices[triangle[2] as usize].z,
            ];
            !(heights.iter().any(|height| *height > COASTAL_RING_HEIGHT)
                && heights.iter().any(|height| *height < COASTAL_RING_HEIGHT))
        }));
    }
}
