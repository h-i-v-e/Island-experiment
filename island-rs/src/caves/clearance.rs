//! Bounded triangle lookup used for geometric (rather than density-only) checks.
use super::{Cave, CaveOptions, Node};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, Vec3};
use std::collections::HashMap;

const BIN: f32 = 2.0;

pub(crate) struct Triangles {
    faces: Vec<[Vec3; 3]>,
    bins: HashMap<(i32, i32), Vec<usize>>,
}

impl Triangles {
    pub(crate) fn new<'a>(
        meshes: impl IntoIterator<Item = &'a Mesh>,
        minimum: Vec2,
        maximum: Vec2,
    ) -> Self {
        let mut result = Self {
            faces: Vec::new(),
            bins: HashMap::new(),
        };
        for mesh in meshes {
            for face in mesh.triangles.chunks_exact(3) {
                let triangle = [face[0], face[1], face[2]]
                    .map(|i| mesh.vertices[i as usize] * ISLAND_WORLD_METRES);
                let low = triangle[0].min(triangle[1]).min(triangle[2]).truncate();
                let high = triangle[0].max(triangle[1]).max(triangle[2]).truncate();
                if low.cmpgt(maximum).any() || high.cmplt(minimum).any() {
                    continue;
                }
                let index = result.faces.len();
                result.faces.push(triangle);
                let low = (low.max(minimum) / BIN).floor().as_ivec2();
                let high = (high.min(maximum) / BIN).floor().as_ivec2();
                for y in low.y..=high.y {
                    for x in low.x..=high.x {
                        result.bins.entry((x, y)).or_default().push(index);
                    }
                }
            }
        }
        result
    }

    fn nearby(&self, point: Vec2, radius: f32) -> impl Iterator<Item = &[Vec3; 3]> {
        let low = ((point - Vec2::splat(radius)) / BIN).floor().as_ivec2();
        let high = ((point + Vec2::splat(radius)) / BIN).floor().as_ivec2();
        (low.y..=high.y)
            .flat_map(move |y| (low.x..=high.x).map(move |x| (x, y)))
            .filter_map(|key| self.bins.get(&key))
            .flatten()
            .map(|&i| &self.faces[i])
    }

    fn clear_sphere(&self, centre: Vec3, radius: f32) -> bool {
        self.nearby(centre.truncate(), radius)
            .all(|t| distance_squared(centre, *t) >= radius * radius)
    }

    pub(super) fn floor(&self, near: Vec3) -> Option<f32> {
        self.nearby(near.truncate(), 0.0)
            .filter_map(|&t| {
                let normal = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
                if normal.z < 0.7 {
                    return None;
                }
                let height =
                    t[0].z - normal.truncate().dot(near.truncate() - t[0].truncate()) / normal.z;
                let point = near.truncate().extend(height);
                ((height - near.z).abs() <= 0.4 && projected_inside(point, t)).then_some(height)
            })
            .max_by(f32::total_cmp)
    }

    pub(super) fn wall_distance_squared(&self, point: Vec3, radius: f32) -> Option<f32> {
        self.nearby(point.truncate(), radius)
            .filter(|t| (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero().z.abs() < 0.7)
            .map(|&t| distance_squared(point, t))
            .filter(|&distance| distance <= radius * radius)
            .min_by(f32::total_cmp)
    }

    pub(crate) fn rock_cover(&self, cave: &Cave, options: &CaveOptions) -> bool {
        cave.paths()
            .all(|path| self.rock_cover_path(path, cave.entrance, cave.inward, options))
    }

    pub(crate) fn rock_cover_path(
        &self,
        nodes: &[Node],
        entrance: Vec3,
        inward: Vec2,
        options: &CaveOptions,
    ) -> bool {
        nodes.windows(2).all(|pair| {
            let count = (pair[0].floor.distance(pair[1].floor) / 0.5).ceil() as usize;
            (0..=count).all(|i| {
                let blend = i as f32 / count.max(1) as f32;
                let floor = pair[0].floor.lerp(pair[1].floor, blend);
                let progress = (floor - entrance).truncate().dot(inward);
                let transition = ((progress - options.transition_length)
                    / options.roof_cover.max(options.side_cover))
                .clamp(0.0, 1.0);
                if transition == 0.0 {
                    return true;
                }
                let radius = (pair[0].width + (pair[1].width - pair[0].width) * blend) * 0.5;
                let height = pair[0].height + (pair[1].height - pair[0].height) * blend;
                let roughness = options.broad_amplitude + options.fine_amplitude;
                (0..16).all(|side| {
                    let angle = side as f32 * std::f32::consts::TAU / 16.0;
                    let point = floor
                        + Vec3::new(
                            angle.cos() * (radius + roughness),
                            angle.sin() * (radius + roughness),
                            height + roughness,
                        );
                    self.clear_sphere(
                        point,
                        options.roof_cover.min(options.side_cover) * transition,
                    )
                })
            })
        })
    }
}

pub(crate) fn walkable(cave: &Cave, options: &CaveOptions) -> bool {
    let triangles = Triangles::new(
        cave.chunks
            .iter()
            .chain(std::iter::once(&cave.entrance_collider)),
        cave.minimum,
        cave.maximum,
    );
    walkable_paths(&triangles, cave.paths(), options)
}

pub(crate) fn walkable_paths<'a>(
    triangles: &Triangles,
    paths: impl IntoIterator<Item = &'a [Node]>,
    options: &CaveOptions,
) -> bool {
    for path in paths {
        let mut previous = None::<(Vec3, f32)>;
        for pair in path.windows(2) {
            let count = (pair[0].floor.distance(pair[1].floor) / 0.25).ceil() as usize;
            for i in 0..=count {
                let near = pair[0]
                    .floor
                    .lerp(pair[1].floor, i as f32 / count.max(1) as f32);
                let Some(height) = triangles.floor(near) else {
                    return false;
                };
                if let Some((last, last_height)) = previous {
                    let run = last.truncate().distance(near.truncate());
                    if (height - last_height).abs()
                        > options.approach_step.min(0.2)
                            + run * options.floor_slope.to_radians().tan()
                    {
                        return false;
                    }
                }
                // Overlapping spheres conservatively enclose the demo's 0.3 m radius,
                // 1.6 m tall standing capsule, with a small floor/skin allowance.
                for level in 0..=5 {
                    if !triangles.clear_sphere(
                        near.truncate().extend(height + 0.4 + level as f32 * 0.2),
                        0.34,
                    ) {
                        return false;
                    }
                }
                previous = Some((near, height));
            }
        }
    }
    true
}

fn projected_inside(point: Vec3, triangle: [Vec3; 3]) -> bool {
    let normal = (triangle[1] - triangle[0]).cross(triangle[2] - triangle[0]);
    (0..3).all(|i| {
        (triangle[(i + 1) % 3] - triangle[i])
            .cross(point - triangle[i])
            .dot(normal)
            >= -1.0e-6 * normal.length_squared()
    })
}

fn distance_squared(point: Vec3, triangle: [Vec3; 3]) -> f32 {
    let normal = (triangle[1] - triangle[0])
        .cross(triangle[2] - triangle[0])
        .normalize_or_zero();
    let distance = (point - triangle[0]).dot(normal);
    let projection = point - normal * distance;
    if normal != Vec3::ZERO && projected_inside(projection, triangle) {
        return distance * distance;
    }
    (0..3)
        .map(|i| {
            let origin = triangle[i];
            let edge = triangle[(i + 1) % 3] - origin;
            let blend =
                ((point - origin).dot(edge) / edge.length_squared().max(1.0e-12)).clamp(0.0, 1.0);
            point.distance_squared(origin + edge * blend)
        })
        .fold(f32::INFINITY, f32::min)
}
