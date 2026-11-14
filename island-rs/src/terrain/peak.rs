//! Lazy callers cache this borrowed, allocation-free final-terrain reduction.
use super::{Island, Mesh, Vec3};

impl Island {
    /// Highest referenced final terrain vertex along the caller's world-up axis.
    /// Unreferenced vertices and vertices removed by cave portals are excluded.
    #[must_use]
    pub fn terrain_peak(&self, up: Vec3) -> Option<Vec3> {
        peak_where(&self.terrain.mesh, up, |point| {
            !self.caves.removes_terrain_surface(point)
        })
    }
}

fn peak_where(mesh: &Mesh, up: Vec3, keep: impl Fn(Vec3) -> bool) -> Option<Vec3> {
    if !up.is_finite() || up == Vec3::ZERO {
        return None;
    }
    mesh.triangles
        .iter()
        .filter_map(|&index| mesh.vertices.get(index as usize).copied())
        .filter(|point| point.is_finite() && keep(*point))
        .max_by(|first, second| first.dot(up).total_cmp(&second.dot(up)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_referenced_retained_vertices_contribute_to_the_peak() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::ZERO,
                Vec3::new(1., 0., 2.),
                Vec3::new(0., 1., 3.),
                Vec3::splat(99.),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        assert_eq!(peak_where(&mesh, Vec3::Z, |_| true), Some(mesh.vertices[2]));
        assert_eq!(
            peak_where(&mesh, Vec3::Z, |point| point.z < 3.),
            Some(mesh.vertices[1])
        );
        assert_eq!(peak_where(&mesh, Vec3::X, |_| true), Some(mesh.vertices[1]));
    }

    #[test]
    fn unavailable_mesh_or_invalid_up_misses() {
        assert_eq!(peak_where(&Mesh::default(), Vec3::Z, |_| true), None);
        assert_eq!(peak_where(&Mesh::default(), Vec3::ZERO, |_| true), None);
        assert_eq!(
            peak_where(&Mesh::default(), Vec3::splat(f32::NAN), |_| true),
            None
        );
    }
}
