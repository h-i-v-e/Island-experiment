//! A render-independent local terrain snapshot owned by a detail worker.
use crate::{Scene, command::BedView, surface};
use planet_rs::DVec3;

pub struct Triangle {
    pub id: usize,
    pub points: [DVec3; 3],
    pub material: [[f32; 4]; 3],
}

impl Triangle {
    pub fn normal(&self) -> DVec3 {
        (self.points[1] - self.points[0])
            .cross(self.points[2] - self.points[0])
            .normalize_or_zero()
    }
    pub fn area(&self) -> f64 {
        (self.points[1] - self.points[0])
            .cross(self.points[2] - self.points[0])
            .length()
            * 0.5
    }
    pub fn sample(&self, weights: [f64; 3]) -> Sample {
        let point = self
            .points
            .into_iter()
            .zip(weights)
            .map(|(p, w)| p * w)
            .sum();
        Sample {
            point,
            normal: self.normal(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Sample {
    pub point: DVec3,
    pub normal: DVec3,
}

pub struct Snapshot {
    pub triangles: Vec<Triangle>,
    order: Vec<usize>,
    nodes: Vec<Node>,
    outer: f64,
}
struct Node {
    min: DVec3,
    max: DVec3,
    left: usize,
    right: usize,
    start: usize,
    end: usize,
}

impl Snapshot {
    pub fn new(scene: &Scene, faces: &[usize]) -> Self {
        let original = scene.bed == BedView::Original;
        let triangles = faces
            .iter()
            .map(|&face| {
                let vertices = scene.terrain.topology.triangles[face];
                Triangle {
                    id: face,
                    points: vertices
                        .map(|i| surface::displayed_position(&scene.terrain, i as usize, original)),
                    material: vertices
                        .map(|i| surface::terrain_material(&scene.terrain, i as usize, original)),
                }
            })
            .collect();
        Self::from_triangles(triangles)
    }
    pub(super) fn from_triangles(triangles: Vec<Triangle>) -> Self {
        Self {
            triangles,
            order: Vec::new(),
            nodes: Vec::new(),
            outer: 0.0,
        }
    }
    pub fn prepare(&mut self) {
        self.nodes.clear();
        self.order = (0..self.triangles.len()).collect();
        self.outer = self
            .triangles
            .iter()
            .flat_map(|t| t.points)
            .map(DVec3::length)
            .fold(1.0, f64::max)
            + 0.01;
        if !self.order.is_empty() {
            self.build(0, self.order.len());
        }
    }
    fn build(&mut self, start: usize, end: usize) -> usize {
        let mut min = DVec3::splat(f64::INFINITY);
        let mut max = DVec3::splat(f64::NEG_INFINITY);
        for &i in &self.order[start..end] {
            for p in self.triangles[i].points {
                min = min.min(p);
                max = max.max(p);
            }
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            left: 0,
            right: 0,
            start,
            end,
        });
        if end - start > 12 {
            let size = max - min;
            let axis = if size.x >= size.y && size.x >= size.z {
                0
            } else if size.y >= size.z {
                1
            } else {
                2
            };
            let middle = usize::midpoint(start, end);
            let triangles = &self.triangles;
            let centre = |id: usize| triangles[id].points.iter().map(|p| p[axis]).sum::<f64>();
            self.order[start..end].select_nth_unstable_by(middle - start, |&a, &b| {
                centre(a).total_cmp(&centre(b)).then(a.cmp(&b))
            });
            self.nodes[index].left = self.build(start, middle);
            self.nodes[index].right = self.build(middle, end);
        }
        index
    }
    pub fn sample(&self, direction: DVec3) -> Option<Sample> {
        if self.nodes.is_empty() {
            return None;
        }
        let up = direction.normalize();
        self.intersect(0, up * self.outer, -up, self.outer)
            .map(|(_, face, weights)| self.triangles[face].sample(weights))
    }
    #[allow(clippy::many_single_char_names)] // Möller–Trumbore barycentric coordinates.
    fn intersect(
        &self,
        index: usize,
        origin: DVec3,
        ray: DVec3,
        mut limit: f64,
    ) -> Option<(f64, usize, [f64; 3])> {
        let node = &self.nodes[index];
        let mut near: f64 = 0.0;
        for axis in 0..3 {
            if ray[axis].abs() < 1.0e-15 {
                if origin[axis] < node.min[axis] - 1.0e-12
                    || origin[axis] > node.max[axis] + 1.0e-12
                {
                    return None;
                }
            } else {
                let a = (node.min[axis] - origin[axis]) / ray[axis];
                let b = (node.max[axis] - origin[axis]) / ray[axis];
                near = near.max(a.min(b));
                limit = limit.min(a.max(b));
                if near > limit + 1.0e-12 {
                    return None;
                }
            }
        }
        if node.left != 0 {
            let first = self.intersect(node.left, origin, ray, limit);
            return self
                .intersect(
                    node.right,
                    origin,
                    ray,
                    first.as_ref().map_or(limit, |hit| hit.0),
                )
                .or(first);
        }
        let mut hit = None;
        for &face in &self.order[node.start..node.end] {
            let [a, b, c] = self.triangles[face].points;
            let ab = b - a;
            let ac = c - a;
            let cross = ray.cross(ac);
            let determinant = ab.dot(cross);
            if determinant.abs() < 1.0e-24 {
                continue;
            }
            let relative = origin - a;
            let u = relative.dot(cross) / determinant;
            let q = relative.cross(ab);
            let v = ray.dot(q) / determinant;
            let distance = ac.dot(q) / determinant;
            if u >= -1.0e-9
                && v >= -1.0e-9
                && u + v <= 1.0 + 1.0e-9
                && distance >= 0.0
                && distance <= limit + 1.0e-12
            {
                limit = distance;
                hit = Some((distance, face, [1.0 - u - v, u, v]));
            }
        }
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_snapshot_matches_displayed_collision_surface_at_both_poles() {
        let scene = crate::collision::test_scene();
        let ground = crate::collision::Ground::new(&scene);
        for up in [bevy::prelude::Vec3::Y, -bevy::prelude::Vec3::Y] {
            let faces = ground.nearby_faces(&scene.terrain, up, 0.3);
            assert!(!faces.is_empty());
            let mut snapshot = Snapshot::new(&scene, &faces);
            snapshot.prepare();
            // Preparing again must replace the index, including its root.
            snapshot.prepare();
            let sample = snapshot
                .sample(DVec3::from_array(up.to_array().map(f64::from)))
                .unwrap();
            let expected = ground.radius(&scene.terrain, up).unwrap();
            assert!((sample.point.length() - f64::from(expected)).abs() < 1.0e-6);
            assert!(sample.normal.dot(sample.point.normalize()) > 0.95);
            assert!(
                snapshot
                    .sample(DVec3::from_array((-up).to_array().map(f64::from)))
                    .is_none()
            );
        }
    }
}
