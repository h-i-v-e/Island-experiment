//! An arched opening cut directly into each original render LOD. Only triangles
//! crossing the entrance are split; the connecting lip is built from those exact
//! cut edges, so it follows that LOD without copying the roof over the passage.
use super::{CaveOptions, passage, stitch};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Portal {
    pub origin: Vec3,
    pub inward: Vec2,
    pub length: f32,
    pub floor_clearance: f32,
    pub section: Vec<Vec2>,
}
#[derive(Clone, Copy)]
struct Vertex {
    p: Vec3,
    n: Vec3,
    uv: Vec2,
}
impl Vertex {
    fn lerp(self, other: Self, t: f32) -> Self {
        if t <= 0.0 {
            return self;
        }
        if t >= 1.0 {
            return other;
        }
        Self {
            p: self.p.lerp(other.p, t),
            n: self.n.lerp(other.n, t).normalize_or_zero(),
            uv: self.uv.lerp(other.uv, t),
        }
    }
}
#[derive(Clone, Copy)]
struct Edge {
    a: Vertex,
    b: Vertex,
    floor: bool,
}

#[derive(Clone, Copy)]
struct Plane {
    normal: Vec3,
    offset: f32,
}
impl Plane {
    fn distance(self, p: Vec3) -> f32 {
        self.normal.dot(p) - self.offset
    }
}

impl Portal {
    pub(crate) fn new(entrance: Vec3, inward: Vec2, o: &CaveOptions) -> Self {
        Self {
            origin: entrance - inward.extend(0.0) * 2.0,
            inward,
            length: o.transition_length + 2.0,
            floor_clearance: o.approach_step + 2.0 * o.approach_slope.to_radians().tan() + 0.1,
            section: passage::section(o.entrance_width, o.entrance_height),
        }
    }
    pub(crate) fn validate(&self) -> bool {
        self.origin.is_finite()
            && self.inward.is_finite()
            && (self.inward.length() - 1.0).abs() < 0.001
            && self.length.is_finite()
            && self.length > 2.0
            && self.length < 128.0
            && self.floor_clearance.is_finite()
            && self.floor_clearance >= 0.0
            && self.section.len() >= 3
            && self.section.len() <= 64
            && self.section.iter().all(|p| p.is_finite())
            && (0..self.section.len()).all(|i| {
                self.section[i].distance_squared(self.section[(i + 1) % self.section.len()])
                    > 1.0e-8
            })
    }
    pub(crate) fn throat(&self) -> Vec3 {
        self.origin + self.inward.extend(0.0) * self.length
    }
    fn throat_ring(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.section
            .iter()
            .map(|q| (self.throat() + self.right() * q.x + Vec3::Z * q.y) / ISLAND_WORLD_METRES)
    }
    pub(crate) fn throat_anchors<'a>(
        &'a self,
        collar: &'a [Vec3],
    ) -> impl Iterator<Item = Vec3> + 'a {
        self.throat_ring().chain(collar.iter().copied().filter(|p| {
            (*p * ISLAND_WORLD_METRES - self.throat())
                .truncate()
                .dot(self.inward)
                .abs()
                < 0.002
        }))
    }
    pub(crate) fn throat_bounds(&self) -> (Vec3, Vec3) {
        self.throat_ring()
            .fold((self.throat(), self.throat()), |(min, max), p| {
                let p = p * ISLAND_WORLD_METRES;
                (min.min(p), max.max(p))
            })
    }
    fn right(&self) -> Vec3 {
        Vec3::new(-self.inward.y, self.inward.x, 0.0)
    }
    fn planes(&self) -> Vec<Plane> {
        let along = self.inward.extend(0.0);
        let mut planes = vec![
            Plane {
                normal: along,
                offset: along.dot(self.origin),
            },
            Plane {
                normal: -along,
                offset: (-along).dot(self.throat()),
            },
        ];
        for i in 0..self.section.len() {
            let a = Self::mouth_section(self.section[i]);
            let b = Self::mouth_section(self.section[(i + 1) % self.section.len()]);
            let edge = b - a;
            let n = Vec2::new(-edge.y, edge.x).normalize();
            let normal = self.right() * n.x + Vec3::Z * n.y;
            let offset = normal.dot(self.origin) + n.dot(a)
                - if normal.z > 0.999 {
                    self.floor_clearance
                } else {
                    0.0
                };
            if !planes.iter().any(|p| {
                p.normal.distance_squared(normal) < 1.0e-8 && (p.offset - offset).abs() < 0.001
            }) {
                planes.push(Plane { normal, offset });
            }
        }
        planes
    }
    #[cfg(test)]
    pub(crate) fn contains(&self, p: Vec3) -> bool {
        self.planes()
            .iter()
            .all(|plane| plane.distance(p) >= -0.001)
    }
    pub(crate) fn bounds(&self) -> (Vec3, Vec3) {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for origin in [self.origin, self.throat()] {
            for q in &self.section {
                let q = Self::mouth_section(*q);
                let p = origin + self.right() * q.x + Vec3::Z * q.y;
                min = min.min(p);
                max = max.max(p);
            }
        }
        min.z -= self.floor_clearance;
        (min, max)
    }
    fn triangle(mesh: &Mesh, indices: &[u32]) -> [Vertex; 3] {
        [indices[0], indices[1], indices[2]].map(|i| {
            let i = i as usize;
            Vertex {
                p: mesh.vertices[i] * ISLAND_WORLD_METRES,
                n: mesh.normals[i],
                uv: mesh
                    .uv
                    .get(i)
                    .copied()
                    .unwrap_or(mesh.vertices[i].truncate()),
            }
        })
    }
    fn overlaps_triangle(mesh: &Mesh, t: &[u32], min: Vec3, max: Vec3) -> bool {
        let a = mesh.vertices[t[0] as usize] * ISLAND_WORLD_METRES;
        let b = mesh.vertices[t[1] as usize] * ISLAND_WORLD_METRES;
        let c = mesh.vertices[t[2] as usize] * ISLAND_WORLD_METRES;
        a.min(b).min(c).cmple(max + Vec3::splat(0.001)).all()
            && a.max(b).max(c).cmpge(min - Vec3::splat(0.001)).all()
    }
    /// Append only the lip to a collider, using the authoritative exterior mesh.
    pub(crate) fn collider(&self, mesh: &Mesh) -> Mesh {
        let planes = self.planes();
        let (min, max) = self.bounds();
        let mut collar = Mesh::default();
        let mut edges = Vec::new();
        for t in mesh.triangles.chunks_exact(3) {
            if !Self::overlaps_triangle(mesh, t, min, max) {
                continue;
            }
            let original = Self::triangle(mesh, t);
            let inside = planes
                .iter()
                .fold(original.to_vec(), |p, &plane| clip(&p, plane, true));
            Self::collect_edges(&mut edges, &inside, &original, &planes);
        }
        self.append_lip(&mut collar, &edges);
        let (min, max) = expanded_bounds(&collar.vertices, min, max);
        stitch::repair(&mut collar, min, max, self.throat_ring());
        collar
    }
    pub(crate) fn cut(&self, mut mesh: Mesh, collar: &[Vec3]) -> Mesh {
        let planes = self.planes();
        let (min, max) = self.bounds();
        let first_new_vertex = mesh.vertices.len();
        let mut repair_min = min;
        let mut repair_max = max;
        let triangles = std::mem::take(&mut mesh.triangles);
        let mut edges = Vec::new();
        for t in triangles.chunks_exact(3) {
            if !Self::overlaps_triangle(&mesh, t, min, max) {
                mesh.triangles.extend_from_slice(t);
                continue;
            }
            let original = Self::triangle(&mesh, t);
            let inside = planes
                .iter()
                .fold(original.to_vec(), |p, &plane| clip(&p, plane, true));
            if inside.len() < 3 {
                mesh.triangles.extend_from_slice(t);
                continue;
            }
            for v in &original {
                repair_min = repair_min.min(v.p);
                repair_max = repair_max.max(v.p);
            }
            let mut remainder = original.to_vec();
            for &plane in &planes {
                append_polygon(&mut mesh, &clip(&remainder, plane, false));
                remainder = clip(&remainder, plane, true);
                if remainder.len() < 3 {
                    break;
                }
            }
            Self::collect_edges(&mut edges, &inside, &original, &planes);
        }
        self.append_lip(&mut mesh, &edges);
        let (min, max) =
            expanded_bounds(&mesh.vertices[first_new_vertex..], repair_min, repair_max);
        stitch::repair(&mut mesh, min, max, self.throat_anchors(collar));
        mesh
    }
    fn collect_edges(
        output: &mut Vec<Edge>,
        inside: &[Vertex],
        original: &[Vertex],
        planes: &[Plane],
    ) {
        if inside.len() < 3 {
            return;
        }
        for i in 0..inside.len() {
            let a = inside[i];
            let b = inside[(i + 1) % inside.len()];
            let Some((side, _)) = planes.iter().enumerate().find(|(_, p)| {
                p.distance(a.p).abs() < 0.001
                    && p.distance(b.p).abs() < 0.001
                    && !original.iter().all(|v| p.distance(v.p).abs() < 0.001)
            }) else {
                continue;
            };
            if side == 1 {
                continue;
            } // No lip across the buried end of the opening.
            output.push(Edge {
                a,
                b,
                floor: side == 0 || planes[side].normal.z > 0.999,
            });
        }
    }
    /// Resolve each shared endpoint once, including across source normal seams
    /// and the floor/wall corner. Curves must not depend on which strip owns it.
    fn append_lip(&self, output: &mut Mesh, edges: &[Edge]) {
        let mut points: Vec<(Vertex, bool)> = Vec::new();
        let indices: Vec<[usize; 2]> = edges
            .iter()
            .map(|edge| {
                [edge.a, edge.b].map(|v| {
                    if let Some(i) = points
                        .iter()
                        .position(|(p, _)| p.p.distance_squared(v.p) < 1.0e-6)
                    {
                        points[i].0.n += v.n;
                        points[i].1 |= edge.floor;
                        i
                    } else {
                        points.push((v, edge.floor));
                        points.len() - 1
                    }
                })
            })
            .collect();
        for (v, _) in &mut points {
            v.n = v.n.normalize_or_zero();
        }
        for [a, b] in indices {
            let (a, floor_a) = points[a];
            let (b, floor_b) = points[b];
            self.append_curved_edge(
                output,
                [a, b],
                [self.target(a, floor_a), self.target(b, floor_b)],
            );
        }
    }
    fn target(&self, source: Vertex, floor: bool) -> Vertex {
        // Classify shared corners from position as well, so a tile containing
        // only the wall strip agrees with the neighbouring tile's floor strip.
        let floor = floor
            || (source.p - self.origin).truncate().dot(self.inward).abs() < 0.002
            || source.p.z <= self.origin.z - self.floor_clearance + 0.002;
        let p = self.project_to_throat(source.p, floor);
        Vertex {
            p,
            n: self.passage_normal(p, floor),
            uv: p.truncate() / ISLAND_WORLD_METRES,
        }
    }
    fn mouth_section(q: Vec2) -> Vec2 {
        q * Vec2::new(1.18, 1.30)
    }

    fn project_to_throat(&self, p: Vec3, floor: bool) -> Vec3 {
        let delta = p - self.origin;
        self.throat()
            + self.right() * (delta.dot(self.right()) / 1.18)
            + Vec3::Z
                * if floor {
                    0.0
                } else {
                    (delta.z / 1.30).max(0.0)
                }
    }

    pub(crate) fn passage_normal(&self, p: Vec3, floor: bool) -> Vec3 {
        if floor {
            return Vec3::Z;
        }
        let delta = p - self.throat();
        let x = delta.dot(self.right());
        let width = self.section.iter().map(|q| q.x.abs()).fold(0.0, f32::max);
        let height = self.section.iter().map(|q| q.y).fold(0.0, f32::max);
        if delta.z <= height * 0.35 {
            -self.right() * x.signum()
        } else {
            (-self.right() * (x / width.powi(2))
                - Vec3::Z * ((delta.z - height * 0.35) / (height * 0.65).powi(2)))
            .normalize_or_zero()
        }
    }

    /// A cubic transition starts tangent to the original terrain and finishes
    /// along the passage. Its end positions are exact on both shared boundaries.
    fn lip_vertex(&self, source: Vertex, target: Vertex, t: f32) -> Vertex {
        if t <= 0.0 {
            return source;
        }
        if t >= 1.0 {
            return target;
        }
        let end = target.p;
        let delta = end - source.p;
        let tangent = delta - source.n * delta.dot(source.n);
        let control_a = tangent * 0.65;
        let control_b = delta - self.inward.extend(0.0) * delta.dot(self.inward.extend(0.0)) * 0.4;
        let u = 1.0 - t;
        let p = source.p
            + control_a * (3.0 * u * u * t)
            + control_b * (3.0 * u * t * t)
            + delta * t.powi(3);
        Vertex {
            p,
            n: source
                .n
                .lerp(target.n, t * t * (3.0 - 2.0 * t))
                .normalize_or_zero(),
            uv: p.truncate() / ISLAND_WORLD_METRES,
        }
    }

    fn append_curved_edge(&self, output: &mut Mesh, [a, b]: [Vertex; 2], targets: [Vertex; 2]) {
        // Subdivide across as well as along the lip so a large source triangle
        // cannot leave a visibly faceted arch. No intermediate mesh is needed.
        const RINGS: usize = 12;
        let across = (a.p.distance(b.p) / 0.5).ceil().max(1.0) as usize;
        for i in 0..across {
            let start = a.lerp(b, i as f32 / across as f32);
            let end = a.lerp(b, (i + 1) as f32 / across as f32);
            let target_a = targets[0].lerp(targets[1], i as f32 / across as f32);
            let target_b = targets[0].lerp(targets[1], (i + 1) as f32 / across as f32);
            for j in 0..RINGS {
                let t0 = j as f32 / RINGS as f32;
                let t1 = (j + 1) as f32 / RINGS as f32;
                let corners = [
                    self.lip_vertex(start, target_a, t0),
                    self.lip_vertex(end, target_b, t0),
                    self.lip_vertex(end, target_b, t1),
                    self.lip_vertex(start, target_a, t1),
                ];
                for triangle in [
                    [corners[0], corners[1], corners[2]],
                    [corners[0], corners[2], corners[3]],
                ] {
                    // Inherit the removed polygon's boundary orientation.
                    // Lighting normals can disagree with geometric facing near
                    // the tangent; using them here flips triangles into holes.
                    append_polygon(output, &triangle);
                }
            }
        }
    }
}
fn expanded_bounds(vertices: &[Vec3], minimum: Vec3, maximum: Vec3) -> (Vec3, Vec3) {
    vertices.iter().fold((minimum, maximum), |(min, max), &p| {
        let p = p * ISLAND_WORLD_METRES;
        (min.min(p), max.max(p))
    })
}

fn clip(poly: &[Vertex], plane: Plane, inside: bool) -> Vec<Vertex> {
    let mut output = Vec::new();
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let da = plane.distance(a.p);
        let db = plane.distance(b.p);
        let ina = if inside { da >= 0.0 } else { da <= 0.0 };
        let inb = if inside { db >= 0.0 } else { db <= 0.0 };
        if ina {
            output.push(a);
        }
        if ina != inb {
            output.push(a.lerp(b, (da / (da - db)).clamp(0.0, 1.0)));
        }
    }
    output
}
fn append_polygon(mesh: &mut Mesh, polygon: &[Vertex]) {
    if polygon.len() < 3 {
        return;
    }
    let base = mesh.vertices.len() as u32;
    for v in polygon {
        mesh.vertices.push(v.p / ISLAND_WORLD_METRES);
        mesh.normals.push(v.n);
        mesh.uv.push(v.uv);
    }
    for i in 1..polygon.len() - 1 {
        if (polygon[i].p - polygon[0].p)
            .cross(polygon[i + 1].p - polygon[0].p)
            .length_squared()
            > 0.0
        {
            mesh.triangles
                .extend([base, base + i as u32, base + i as u32 + 1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_lip_matches_the_cliff_and_passage_tangents() {
        let portal = Portal::new(
            Vec3::new(999.0, 1000.0, 10.0),
            Vec2::X,
            &CaveOptions::default(),
        );
        let width = portal.section.iter().map(|q| q.x.abs()).fold(0.0, f32::max);
        let source = Vertex {
            p: Vec3::new(1000.0, 1000.0 + width * 1.18, 11.0),
            n: -Vec3::X,
            uv: Vec2::ZERO,
        };
        let start = portal.lip_vertex(source, portal.target(source, false), 0.0);
        let end = portal.lip_vertex(source, portal.target(source, false), 1.0);
        assert_eq!(start.p, source.p);
        assert_eq!(start.n, source.n);
        assert!((end.p - portal.project_to_throat(source.p, false)).length() < 0.0001);
        let start_tangent = (portal
            .lip_vertex(source, portal.target(source, false), 0.001)
            .p
            - start.p)
            .normalize();
        let end_tangent = (end.p
            - portal
                .lip_vertex(source, portal.target(source, false), 0.999)
                .p)
            .normalize();
        assert!(start_tangent.dot(source.n).abs() < 0.1);
        assert!(end_tangent.dot(Vec3::X) > 0.999);
        for i in 0..=100 {
            let v = portal.lip_vertex(source, portal.target(source, false), i as f32 / 100.0);
            assert!(v.p.is_finite() && v.n.is_finite());
            assert!((v.n.length() - 1.0).abs() < 0.0001);
            assert!(v.p.y >= end.p.y - 0.0001 && v.p.y <= start.p.y + 0.0001);
        }
    }
    #[test]
    fn floor_and_wall_share_every_edge_of_the_rounded_corner() {
        let portal = Portal::new(Vec3::ZERO, Vec2::X, &CaveOptions::default());
        let vertex = |p, n| Vertex {
            p,
            n,
            uv: Vec2::ZERO,
        };
        let corner = Vec3::new(-2.0, 2.36, 0.2);
        let edges = [
            Edge {
                a: vertex(Vec3::new(-2.0, 0.0, 0.2), Vec3::Z),
                b: vertex(corner, Vec3::Z),
                floor: true,
            },
            Edge {
                a: vertex(corner, -Vec3::X),
                b: vertex(Vec3::new(0.0, 2.36, 1.1), -Vec3::X),
                floor: false,
            },
        ];
        let mut mesh = Mesh::default();
        portal.append_lip(&mut mesh, &edges);
        let source = vertex(corner, (Vec3::Z - Vec3::X).normalize());
        let target = portal.target(source, true);
        assert_eq!(
            target.p,
            portal.target(source, false).p,
            "tile-local floor classification split the shared corner"
        );
        for ring in 0..12 {
            let a = portal.lip_vertex(source, target, ring as f32 / 12.0).p;
            let b = portal
                .lip_vertex(source, target, (ring + 1) as f32 / 12.0)
                .p;
            let mut uses = [0, 0];
            for triangle in mesh.triangles.chunks_exact(3) {
                for i in 0..3 {
                    let p = mesh.vertices[triangle[i] as usize] * ISLAND_WORLD_METRES;
                    let q = mesh.vertices[triangle[(i + 1) % 3] as usize] * ISLAND_WORLD_METRES;
                    if p.distance(a) < 0.0001 && q.distance(b) < 0.0001 {
                        uses[0] += 1;
                    }
                    if q.distance(a) < 0.0001 && p.distance(b) < 0.0001 {
                        uses[1] += 1;
                    }
                }
            }
            assert_eq!(uses, [1, 1], "open or reversed seam at ring {ring}");
        }
    }
}
