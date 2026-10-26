//! Owned spatial index over the displayed terrain; the generator retains its
//! topology. Traversals borrow that topology and allocate no per-query buffers.
use crate::{Scene, command::BedView, surface};
use bevy::prelude::*;
use planet_rs::{DVec3, PlanetTerrain};

pub(super) struct Ground {
    points: Vec<DVec3>,
    faces: Vec<usize>,
    nodes: Vec<Node>,
    pub outer_radius: f32,
    rocks: Vec<RockCollider>,
}

#[derive(Clone, Copy, Debug)]
pub struct RockCollider {
    pub centre: Vec3,
    pub radius: f32,
}

struct Node {
    min: Vec3,
    max: Vec3,
    contents: Contents,
}

enum Contents {
    Leaf { start: usize, end: usize },
    Branch { left: usize, right: usize },
}

impl Ground {
    pub fn new(scene: &Scene) -> Self {
        let points: Vec<_> = (0..scene.terrain.elevations.len())
            .map(|i| surface::displayed_position(&scene.terrain, i, scene.bed == BedView::Original))
            .collect();
        let outer_radius = points
            .iter()
            .map(|p| surface::vector(*p).length())
            .fold(1.0, f32::max)
            + 0.01;
        let mut ground = Self {
            points,
            faces: (0..scene.terrain.topology.triangles.len()).collect(),
            nodes: Vec::new(),
            outer_radius,
            rocks: Vec::new(),
        };
        ground.build(&scene.terrain, 0, ground.faces.len());
        ground
    }

    fn build(&mut self, terrain: &PlanetTerrain, start: usize, end: usize) -> usize {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for &face in &self.faces[start..end] {
            for i in terrain.topology.triangles[face] {
                let point = surface::vector(self.points[i as usize]);
                min = min.min(point);
                max = max.max(point);
            }
        }
        let node = self.nodes.len();
        self.nodes.push(Node {
            min: min - Vec3::splat(1.0e-6),
            max: max + Vec3::splat(1.0e-6),
            contents: Contents::Leaf { start, end },
        });
        if end - start > 16 {
            let size = max - min;
            let axis = if size.x >= size.y && size.x >= size.z {
                0
            } else if size.y >= size.z {
                1
            } else {
                2
            };
            let middle = start + (end - start) / 2;
            let center = |face: usize| {
                terrain.topology.triangles[face]
                    .iter()
                    .map(|&i| self.points[i as usize][axis])
                    .sum::<f64>()
            };
            self.faces[start..end].select_nth_unstable_by(middle - start, |&a, &b| {
                center(a).total_cmp(&center(b)).then(a.cmp(&b))
            });
            let left = self.build(terrain, start, middle);
            let right = self.build(terrain, middle, end);
            self.nodes[node].contents = Contents::Branch { left, right };
        }
        node
    }

    pub fn hit(
        &self,
        terrain: &PlanetTerrain,
        origin: Vec3,
        direction: Vec3,
        limit: f32,
    ) -> Option<Vec3> {
        let terrain_hit = self
            .intersect(terrain, 0, origin, direction, limit)
            .map(|(distance, _)| distance);
        let distance = self
            .rocks
            .iter()
            .filter_map(|rock| {
                let delta = origin - rock.centre;
                let b = delta.dot(direction);
                let discriminant = b * b - (delta.length_squared() - rock.radius * rock.radius);
                if discriminant < 0.0 {
                    return None;
                }
                let hit = -b - discriminant.sqrt();
                (hit >= 0.0 && hit <= limit).then_some(hit)
            })
            .chain(terrain_hit)
            .min_by(f32::total_cmp)?;
        Some(origin + direction * distance)
    }

    pub fn set_rocks(&mut self, rocks: Vec<RockCollider>) {
        self.rocks = rocks;
    }

    pub fn nearby_faces(&self, terrain: &PlanetTerrain, centre: Vec3, radius: f32) -> Vec<usize> {
        fn visit(
            ground: &Ground,
            terrain: &PlanetTerrain,
            node: usize,
            centre: Vec3,
            squared: f32,
            out: &mut Vec<usize>,
        ) {
            let node = &ground.nodes[node];
            if centre.distance_squared(centre.clamp(node.min, node.max)) > squared {
                return;
            }
            match node.contents {
                Contents::Leaf { start, end } => {
                    for &face in &ground.faces[start..end] {
                        let points = terrain.topology.triangles[face]
                            .map(|i| surface::vector(ground.points[i as usize]));
                        let low = points[0].min(points[1]).min(points[2]);
                        let high = points[0].max(points[1]).max(points[2]);
                        if centre.distance_squared(centre.clamp(low, high)) <= squared {
                            out.push(face);
                        }
                    }
                }
                Contents::Branch { left, right } => {
                    visit(ground, terrain, left, centre, squared, out);
                    visit(ground, terrain, right, centre, squared, out);
                }
            }
        }
        let mut faces = Vec::new();
        visit(self, terrain, 0, centre, radius * radius, &mut faces);
        faces.sort_unstable();
        faces
    }

    pub fn radius(&self, terrain: &PlanetTerrain, direction: Vec3) -> Option<f32> {
        self.hit(
            terrain,
            direction * self.outer_radius,
            -direction,
            self.outer_radius,
        )
        .map(|point| point.dot(direction))
    }

    /// Final terrain and clipped river radii, excluding decorative rocks.
    pub fn water_support(
        &self,
        terrain: &PlanetTerrain,
        direction: Vec3,
    ) -> Option<(f32, Option<f32>)> {
        let origin = direction * self.outer_radius;
        let ray = -direction;
        let (distance, face) = self.intersect(terrain, 0, origin, ray, self.outer_radius)?;
        let surface = &terrain.river_surface;
        // Water triangles are emitted in supporting terrain-face order.
        let start = surface.terrain_faces.partition_point(|&id| id < face);
        let end = surface.terrain_faces.partition_point(|&id| id <= face);
        let water = surface.triangles[start..end]
            .iter()
            .filter_map(|triangle| {
                triangle_hit(triangle.map(|i| surface.positions[i as usize]), origin, ray)
            })
            .min_by(f32::total_cmp)
            .map(|distance| (origin + ray * distance).dot(direction));
        Some(((origin + ray * distance).dot(direction), water))
    }

    fn intersect(
        &self,
        terrain: &PlanetTerrain,
        node: usize,
        origin: Vec3,
        direction: Vec3,
        mut limit: f32,
    ) -> Option<(f32, usize)> {
        let node = &self.nodes[node];
        if !bounds_hit(node.min, node.max, origin, direction, limit) {
            return None;
        }
        match node.contents {
            Contents::Leaf { start, end } => {
                let mut hit = None;
                for &face in &self.faces[start..end] {
                    let points = terrain.topology.triangles[face].map(|i| self.points[i as usize]);
                    if let Some(distance) = triangle_hit(points, origin, direction)
                        && distance < limit
                    {
                        limit = distance;
                        hit = Some((distance, face));
                    }
                }
                hit
            }
            Contents::Branch { left, right } => {
                let first = self.intersect(terrain, left, origin, direction, limit);
                let second = self.intersect(
                    terrain,
                    right,
                    origin,
                    direction,
                    first.map_or(limit, |(distance, _)| distance),
                );
                second.or(first)
            }
        }
    }
}

fn bounds_hit(min: Vec3, max: Vec3, origin: Vec3, direction: Vec3, mut far: f32) -> bool {
    let mut near: f32 = 0.0;
    for axis in 0..3 {
        if direction[axis].abs() < 1.0e-12 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return false;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return false;
            }
        }
    }
    true
}

#[allow(clippy::cast_possible_truncation)]
fn triangle_hit([a, b, c]: [DVec3; 3], origin: Vec3, direction: Vec3) -> Option<f32> {
    // Double precision matters for the tiny, skinny coastal faces. Pick and
    // collision must agree on shared edges, including hits on a triangle vertex.
    let origin = DVec3::from_array(origin.to_array().map(f64::from));
    let direction = DVec3::from_array(direction.to_array().map(f64::from));
    let ab = b - a;
    let ac = c - a;
    let cross = direction.cross(ac);
    let determinant = ab.dot(cross);
    if determinant.abs() < 1.0e-24 {
        return None;
    }
    let offset = origin - a;
    let barycentric_b = offset.dot(cross) / determinant;
    let barycentric_c = direction.dot(offset.cross(ab)) / determinant;
    let distance = ac.dot(offset.cross(ab)) / determinant;
    (barycentric_b >= -1.0e-8
        && barycentric_c >= -1.0e-8
        && barycentric_b + barycentric_c <= 1.0 + 1.0e-8
        && distance >= 0.0)
        .then_some(distance as f32)
}

#[cfg(test)]
pub(super) fn test_scene() -> Scene {
    use crate::command::{GenerationMode, RiverDisplay};
    use planet_rs::{PlanetOptions, TerrainOptions, generate_terrain};
    Scene {
        terrain: generate_terrain(
            PlanetOptions {
                vertex_count: 256,
                ..PlanetOptions::default()
            },
            TerrainOptions {
                relief: 0.0,
                hydraulic_passes: 0,
                thermal_passes: 0,
                refinement_passes: 1,
                river_refinement_passes: 0,
                river_depth: 0.0,
                connect_seas: false,
                ..TerrainOptions::default()
            },
        )
        .unwrap(),
        seed: 42,
        generation: GenerationMode::Terrain,
        bed: BedView::Carved,
        river_display: RiverDisplay::Show,
        no_ocean: true,
        wireframe: false,
        smooth: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_support_samples_clipped_render_geometry_and_ignores_rocks() {
        let mut scene = test_scene();
        let points = scene.terrain.topology.triangles[0]
            .map(|i| surface::displayed_position(&scene.terrain, i as usize, false));
        let middle = (points[0] + points[1] + points[2]) / 3.0;
        scene.terrain.river_surface.positions = points.map(|p| middle.lerp(p, 0.2) * 1.003).into();
        scene.terrain.river_surface.triangles = vec![[0, 1, 2]];
        scene.terrain.river_surface.terrain_faces = vec![0];
        let mut ground = Ground::new(&scene);
        let up = surface::vector(middle.normalize());
        let (floor, water) = ground.water_support(&scene.terrain, up).unwrap();
        assert!((water.unwrap() - floor * 1.003).abs() < 1.0e-6);
        let outside = surface::vector(middle.lerp(points[0], 0.8).normalize());
        assert!(
            ground
                .water_support(&scene.terrain, outside)
                .unwrap()
                .1
                .is_none()
        );
        ground.set_rocks(vec![RockCollider {
            centre: up * (floor + 0.003),
            radius: 0.001,
        }]);
        assert!(ground.radius(&scene.terrain, up).unwrap() > floor + 0.002);
        assert_eq!(
            ground.water_support(&scene.terrain, up),
            Some((floor, water))
        );
    }

    #[test]
    fn boulders_support_radial_queries_and_clearing_restores_terrain() {
        let scene = test_scene();
        let mut ground = Ground::new(&scene);
        for up in [Vec3::Y, -Vec3::Y] {
            let floor = ground.radius(&scene.terrain, up).unwrap();
            ground.set_rocks(vec![RockCollider {
                centre: up * floor,
                radius: 0.003,
            }]);
            let supported = ground.radius(&scene.terrain, up).unwrap();
            assert!((supported - floor - 0.003).abs() < 0.0001);
            ground.set_rocks(Vec::new());
            assert!((ground.radius(&scene.terrain, up).unwrap() - floor).abs() < 1.0e-6);
        }
    }

    #[test]
    fn spatial_queries_match_all_triangles_including_both_poles_and_misses() {
        let scene = test_scene();
        let ground = Ground::new(&scene);
        let mut directions = vec![Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        directions.extend(
            scene
                .terrain
                .topology
                .directions
                .iter()
                .take(100)
                .map(|&p| surface::vector(p)),
        );
        for direction in directions {
            let origin = direction * 3.0;
            let expected = scene
                .terrain
                .topology
                .triangles
                .iter()
                .filter_map(|face| {
                    triangle_hit(face.map(|i| ground.points[i as usize]), origin, -direction)
                })
                .min_by(f32::total_cmp)
                .unwrap();
            let hit = ground
                .hit(&scene.terrain, origin, -direction, 50.0)
                .unwrap();
            assert!((hit.distance(origin) - expected).abs() < 1.0e-5);
            assert!(
                (hit.length() - ground.radius(&scene.terrain, direction).unwrap()).abs() < 1.0e-5
            );
        }
        assert!(
            ground
                .hit(&scene.terrain, Vec3::Z * 3.0, Vec3::X, 50.0)
                .is_none()
        );
        assert!(
            ground
                .hit(&scene.terrain, Vec3::Z * 3.0, -Vec3::Z, 0.1)
                .is_none()
        );
    }
}
