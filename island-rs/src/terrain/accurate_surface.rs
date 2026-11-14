//! Strict, allocation-free collision queries against the resident final mesh.
use super::{Island, Terrain, Vec2, Vec3};
use glam::{DVec2, DVec3};

/// A supporting final-LOD0 triangle, in native island coordinates (Z is up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainSurfaceHit {
    pub position: Vec3,
    /// Geometric face normal, oriented towards native up rather than shading.
    pub normal: Vec3,
}

impl Terrain {
    /// Highest triangle under this point. Outside points and mesh holes miss;
    /// no boundary clamping, nearest vertex or interpolated height map is used.
    #[must_use]
    pub fn sample_accurate_surface(&self, point: Vec2) -> Option<TerrainSurfaceHit> {
        if !point.is_finite() || point.min_element() < 0.0 || point.max_element() > 1.0 {
            return None;
        }
        self.raycast_accurate_surface(point.extend(0.0), -Vec3::Z)
    }

    /// Highest hit against an infinite line along `down`. An infinite line
    /// makes a height query independent of the caller's approximate altitude.
    /// The existing compressed face bins bound work to crossed cells only.
    /// This also handles a world-vertical line through a tilted island root.
    #[must_use]
    pub fn raycast_accurate_surface(&self, origin: Vec3, down: Vec3) -> Option<TerrainSurfaceHit> {
        self.raycast_surface_where(origin, down, |_| true)
    }

    fn raycast_surface_where(
        &self,
        origin: Vec3,
        down: Vec3,
        accept: impl Fn(&TerrainSurfaceHit) -> bool,
    ) -> Option<TerrainSurfaceHit> {
        if !origin.is_finite() || !down.is_finite() || down == Vec3::ZERO {
            return None;
        }
        if down.x == 0.0 && down.y == 0.0 {
            return if origin.truncate().min_element() >= 0.0
                && origin.truncate().max_element() <= 1.0
            {
                self.triangle_index
                    .candidates(origin.truncate())
                    .iter()
                    .filter_map(|&face| {
                        self.intersect_face(face, origin.as_dvec3(), down.as_dvec3())
                    })
                    .filter(accept)
                    .min_by(|a, b| {
                        (a.position.z * down.z.signum())
                            .total_cmp(&(b.position.z * down.z.signum()))
                    })
            } else {
                None
            };
        }
        let origin = origin.as_dvec3();
        let down = down.as_dvec3();
        let (start, end) = projected_interval(origin.truncate(), down.truncate())?;
        let point = (origin + down * start)
            .truncate()
            .clamp(DVec2::ZERO, DVec2::ONE);
        let dimension = self.triangle_index.dimension;
        let [mut x, mut y] = self.triangle_index.point_bin(point.as_vec2());
        let mut best: Option<(f64, TerrainSurfaceHit)> = None;
        let next_crossing = |cell: usize, axis: usize| {
            let direction = down[axis];
            if direction == 0.0 {
                f64::INFINITY
            } else {
                let edge = if direction > 0.0 { cell + 1 } else { cell };
                (edge as f64 / dimension as f64 - origin[axis]) / direction
            }
        };
        for _ in 0..=dimension * 2 {
            for &face in self.triangle_index.bin_faces(x, y) {
                if let Some(hit) = self.intersect_face(face, origin, down).filter(&accept) {
                    let distance =
                        (hit.position.as_dvec3() - origin).dot(down) / down.length_squared();
                    if best
                        .as_ref()
                        .is_none_or(|(previous, _)| distance < *previous)
                    {
                        best = Some((distance, hit));
                    }
                }
            }
            let next_x = next_crossing(x, 0);
            let next_y = next_crossing(y, 1);
            let next = next_x.min(next_y);
            if next > end || !next.is_finite() {
                break;
            }
            if next_x <= next_y && !step_cell(&mut x, down.x, dimension)
                || next_y <= next_x && !step_cell(&mut y, down.y, dimension)
            {
                break;
            }
        }
        best.map(|(_, hit)| hit)
    }

    fn intersect_face(&self, face: u32, origin: DVec3, down: DVec3) -> Option<TerrainSurfaceHit> {
        let offset = face as usize * 3;
        let [a, b, c] = [0, 1, 2]
            .map(|i| self.mesh.vertices[self.mesh.triangles[offset + i] as usize].as_dvec3());
        let first = b - a;
        let second = c - a;
        let crossed = down.cross(second);
        let determinant = first.dot(crossed);
        if !determinant.is_finite() || determinant == 0.0 {
            return None;
        }
        let relative = origin - a;
        let b_weight = relative.dot(crossed) / determinant;
        let c_weight = down.dot(relative.cross(first)) / determinant;
        let weights = [1.0 - b_weight - c_weight, b_weight, c_weight];
        if weights
            .iter()
            .any(|weight| !weight.is_finite() || *weight < -1.0e-12)
        {
            return None;
        }
        let weights = weights.map(|weight| weight.max(0.0));
        let position =
            (a * weights[0] + b * weights[1] + c * weights[2]) / weights.iter().sum::<f64>();
        let normal = first.cross(second).try_normalize()?;
        let normal = if normal.z < 0.0 { -normal } else { normal };
        (position.is_finite() && normal.is_finite()).then_some(TerrainSurfaceHit {
            position: position.as_vec3(),
            normal: normal.as_vec3(),
        })
    }
}

impl Island {
    /// Resident final triangles, excluding the exact cave-portal volumes removed
    /// by render export. Cave interiors do not become phantom terrain supports.
    #[must_use]
    pub fn accurate_terrain_surface(&self, origin: Vec3, down: Vec3) -> Option<TerrainSurfaceHit> {
        self.terrain.raycast_surface_where(origin, down, |hit| {
            !self.caves.removes_terrain_surface(hit.position)
        })
    }
}

fn projected_interval(origin: DVec2, down: DVec2) -> Option<(f64, f64)> {
    let mut start = f64::NEG_INFINITY;
    let mut end = f64::INFINITY;
    for axis in 0..2 {
        if down[axis] == 0.0 {
            if !(0.0..=1.0).contains(&origin[axis]) {
                return None;
            }
        } else {
            let first = -origin[axis] / down[axis];
            let second = (1.0 - origin[axis]) / down[axis];
            start = start.max(first.min(second));
            end = end.min(first.max(second));
        }
    }
    (start <= end && start.is_finite() && end.is_finite()).then_some((start, end))
}

fn step_cell(cell: &mut usize, direction: f64, dimension: usize) -> bool {
    if direction > 0.0 && *cell + 1 < dimension {
        *cell += 1;
        true
    } else if direction < 0.0 && *cell > 0 {
        *cell -= 1;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mesh;

    fn make_terrain(vertices: &[Vec3], triangles: &[u32]) -> Terrain {
        Terrain::new(Mesh {
            vertices: vertices.to_vec(),
            triangles: triangles.to_vec(),
            ..Mesh::default()
        })
    }

    #[test]
    fn exact_sloping_face_returns_geometric_normal_and_submerged_height() {
        let terrain = make_terrain(
            &[
                Vec3::new(0., 0., -3.),
                Vec3::new(1., 0., -2.),
                Vec3::new(0., 1., -3.),
            ],
            &[0, 1, 2],
        );
        let hit = terrain.sample_accurate_surface(Vec2::splat(0.25)).unwrap();
        assert!((hit.position.z + 2.75).abs() < 1e-6);
        assert!(
            hit.normal
                .abs_diff_eq(Vec3::new(-1., 0., 1.).normalize(), 1e-6)
        );
    }

    #[test]
    fn holes_outside_nonfinite_and_degenerate_faces_miss() {
        let terrain = make_terrain(&[Vec3::ZERO, Vec3::X, Vec3::Y], &[0, 1, 2]);
        for point in [
            Vec2::splat(0.75),
            Vec2::new(-0.001, 0.2),
            Vec2::splat(1.001),
            Vec2::splat(f32::NAN),
        ] {
            assert!(terrain.sample_accurate_surface(point).is_none());
        }
        let empty = Terrain::new(Mesh::default());
        assert!(empty.sample_accurate_surface(Vec2::splat(0.5)).is_none());
        let degenerate = make_terrain(&[Vec3::ZERO, Vec3::X, Vec3::X * 0.5], &[0, 1, 2]);
        assert!(
            degenerate
                .sample_accurate_surface(Vec2::new(0.25, 0.))
                .is_none()
        );
    }

    #[test]
    fn tiny_faces_and_domain_edges_are_not_replaced_by_vertices() {
        let point = Vec2::new(0.25, 0.5);
        let side = 1. / 8192.;
        let terrain = make_terrain(
            &[
                (point - Vec2::splat(side)).extend(0.),
                (point + Vec2::new(side, -side)).extend(6.),
                (point + Vec2::Y * side).extend(0.),
            ],
            &[0, 1, 2],
        );
        let hit = terrain.sample_accurate_surface(point).unwrap();
        assert!((hit.position.z - 1.5).abs() < 1e-6);
        let boundary = make_terrain(
            &[
                Vec3::new(1., 0., 2.),
                Vec3::new(1., 1., 6.),
                Vec3::new(1. - f32::EPSILON, 0.5, 100.),
            ],
            &[0, 1, 2],
        );
        assert!(
            (boundary
                .sample_accurate_surface(Vec2::new(1., 0.25))
                .unwrap()
                .position
                .z
                - 3.)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn overlapping_faces_choose_highest_independent_of_face_order_or_winding() {
        let vertices = [
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
            Vec3::Z * 2.,
            Vec3::X + Vec3::Z * 2.,
            Vec3::Y + Vec3::Z * 2.,
        ];
        for triangles in [[0, 1, 2, 5, 4, 3], [5, 4, 3, 0, 1, 2]] {
            let hit = make_terrain(&vertices, &triangles)
                .sample_accurate_surface(Vec2::splat(0.25))
                .unwrap();
            assert!((hit.position.z - 2.).abs() < 1e-6);
            assert_eq!(hit.normal, Vec3::Z);
        }
    }

    #[test]
    fn removed_top_face_does_not_hide_real_support_below_or_fill_a_hole() {
        let vertices = [
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
            Vec3::Z * 2.,
            Vec3::X + Vec3::Z * 2.,
            Vec3::Y + Vec3::Z * 2.,
        ];
        let terrain = make_terrain(&vertices, &[0, 1, 2, 3, 4, 5]);
        let origin = Vec3::new(0.25, 0.25, 10.);
        let filtered = terrain
            .raycast_surface_where(origin, -Vec3::Z, |hit| hit.position.z < 1.)
            .unwrap();
        assert!(filtered.position.z.abs() < 1e-6);
        assert!(
            terrain
                .raycast_surface_where(origin, -Vec3::Z, |_| false)
                .is_none()
        );
    }

    #[test]
    fn tilted_root_line_crosses_bins_and_chooses_nearest_along_down() {
        let vertices = [
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
            Vec3::Z * 2.,
            Vec3::X + Vec3::Z * 2.,
            Vec3::Y + Vec3::Z * 2.,
        ];
        let terrain = make_terrain(&vertices, &[0, 1, 2, 3, 4, 5]);
        let hit = terrain
            .raycast_accurate_surface(Vec3::new(-0.5, 0.2, 3.), Vec3::new(0.25, 0., -1.))
            .unwrap();
        // The higher plane is outside at x=-0.25; only the lower face supports this vertical world line.
        assert!(hit.position.abs_diff_eq(Vec3::new(0.25, 0.2, 0.), 1e-6));
        assert!(
            terrain
                .raycast_accurate_surface(Vec3::new(2., 2., 1.), Vec3::new(0.25, 0., -1.))
                .is_none()
        );
    }

    #[test]
    fn snapshot_cache_retains_exact_query_geometry_and_index() {
        let island = crate::Island::shared_forest_fixture(crate::ForestOptions::default());
        let path = std::env::temp_dir().join(format!(
            "motu-accurate-surface-{}.snapshot",
            std::process::id()
        ));
        super::super::snapshot::save(&island, &path).unwrap();
        let restored = super::super::snapshot::load(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        for point in [Vec2::splat(0.32), Vec2::new(0.37, 0.39), Vec2::splat(0.9)] {
            assert_eq!(
                island.terrain().sample_accurate_surface(point),
                restored.terrain().sample_accurate_surface(point)
            );
        }
        assert!(
            restored
                .terrain()
                .sample_accurate_surface(Vec2::splat(0.32))
                .is_some()
        );
    }
}
