//! Make the clipped entrance conforming: matching positions alone do not close
//! rasterization cracks where one side of an edge has extra vertices.
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, Vec3};
use glam::{DVec3, IVec3};
use std::collections::HashMap;

const TOLERANCE: f64 = 0.0005; // Half a millimetre, in island-local metres.
const BIN: f64 = 0.5;

struct Points {
    positions: Vec<Vec3>,
    bins: [HashMap<i32, Vec<usize>>; 3],
    minimum: DVec3,
    maximum: DVec3,
}
impl Points {
    fn world(p: Vec3) -> DVec3 {
        p.as_dvec3() * f64::from(ISLAND_WORLD_METRES)
    }
    fn new(
        mesh: &mut Mesh,
        minimum: Vec3,
        maximum: Vec3,
        anchors: impl IntoIterator<Item = Vec3>,
    ) -> Self {
        let minimum = minimum.as_dvec3() - DVec3::splat(TOLERANCE);
        let maximum = maximum.as_dvec3() + DVec3::splat(TOLERANCE);
        let mut result = Self {
            positions: Vec::new(),
            bins: std::array::from_fn(|_| HashMap::new()),
            minimum,
            maximum,
        };
        let mut weld_bins: HashMap<IVec3, Vec<usize>> = HashMap::new();
        for p in anchors {
            result.intern(p, &mut weld_bins);
        }
        let mut used = vec![false; mesh.vertices.len()];
        for &i in &mesh.triangles {
            used[i as usize] = true;
        }
        for (i, p) in mesh.vertices.iter_mut().enumerate() {
            if used[i] {
                *p = result.intern(*p, &mut weld_bins);
            }
        }
        result
    }
    fn intern(&mut self, p: Vec3, weld_bins: &mut HashMap<IVec3, Vec<usize>>) -> Vec3 {
        let world = Self::world(p);
        if world.cmplt(self.minimum).any() || world.cmpgt(self.maximum).any() {
            return p;
        }
        let cell = (world / TOLERANCE).floor().as_ivec3();
        let existing = (-1..=1)
            .flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| IVec3::new(x, y, z))))
            .filter_map(|offset| weld_bins.get(&(cell + offset)))
            .flatten()
            .copied()
            .find(|&id| {
                Self::world(self.positions[id]).distance_squared(world) <= TOLERANCE * TOLERANCE
            });
        if let Some(id) = existing {
            return self.positions[id];
        }
        let id = self.positions.len();
        self.positions.push(p);
        weld_bins.entry(cell).or_default().push(id);
        let bin = (world / BIN).floor().as_ivec3();
        for axis in 0..3 {
            self.bins[axis].entry(bin[axis]).or_default().push(id);
        }
        p
    }
    fn splits(&self, a: Vec3, b: Vec3) -> Vec<(f32, Vec3)> {
        let start = Self::world(a);
        let end = Self::world(b);
        let direction = end - start;
        let length_squared = direction.length_squared();
        if length_squared <= TOLERANCE * TOLERANCE {
            return Vec::new();
        }
        let minimum = (start.min(end) - DVec3::splat(TOLERANCE)).max(self.minimum);
        let maximum = (start.max(end) + DVec3::splat(TOLERANCE)).min(self.maximum);
        if minimum.cmpgt(maximum).any() {
            return Vec::new();
        }
        let low = (minimum / BIN).floor().as_ivec3();
        let high = (maximum / BIN).floor().as_ivec3();
        let mut splits = Vec::new();
        // Search slabs along the longest edge axis, rather than every cell in
        // its 3D bounding box. A steep, long terrain edge can span many metres.
        let span = (maximum - minimum).abs();
        let axis = if span.x >= span.y && span.x >= span.z {
            0
        } else if span.y >= span.z {
            1
        } else {
            2
        };
        for bin in low[axis]..=high[axis] {
            if let Some(ids) = self.bins[axis].get(&bin) {
                for &id in ids {
                    let p = self.positions[id];
                    if p == a || p == b {
                        continue;
                    }
                    let world = Self::world(p);
                    if world.cmplt(minimum).any() || world.cmpgt(maximum).any() {
                        continue;
                    }
                    let t = (world - start).dot(direction) / length_squared;
                    if t > 0.0
                        && t < 1.0
                        && world.distance_squared(start + direction * t) <= TOLERANCE * TOLERANCE
                    {
                        splits.push((t as f32, p));
                    }
                }
            }
        }
        splits.sort_by(|a, b| a.0.total_cmp(&b.0));
        splits
    }
}

pub(crate) fn repair(
    mesh: &mut Mesh,
    minimum: Vec3,
    maximum: Vec3,
    anchors: impl IntoIterator<Item = Vec3>,
) {
    let points = Points::new(mesh, minimum, maximum, anchors);
    let triangles = std::mem::take(&mut mesh.triangles);
    for t in triangles.chunks_exact(3) {
        let mut boundary = Vec::new();
        for side in 0..3 {
            let a = t[side];
            let b = t[(side + 1) % 3];
            boundary.push(a);
            for (blend, p) in points.splits(mesh.vertices[a as usize], mesh.vertices[b as usize]) {
                let normal = mesh.normals[a as usize]
                    .lerp(mesh.normals[b as usize], blend)
                    .normalize_or_zero();
                let uv = mesh.uv[a as usize].lerp(mesh.uv[b as usize], blend);
                boundary.push(append_vertex(mesh, p, normal, uv));
            }
        }
        if boundary.len() == 3 {
            append_triangle(mesh, [t[0], t[1], t[2]]);
        } else {
            // A centre fan preserves subdivisions on all three edges; a fan
            // from a corner would silently skip its collinear edge vertices.
            let p = t
                .iter()
                .map(|&i| mesh.vertices[i as usize].as_dvec3())
                .sum::<DVec3>()
                / 3.0;
            let normal = t
                .iter()
                .map(|&i| mesh.normals[i as usize])
                .sum::<Vec3>()
                .normalize_or_zero();
            let uv = t.iter().map(|&i| mesh.uv[i as usize]).sum::<Vec2>() / 3.0;
            let centre = append_vertex(mesh, p.as_vec3(), normal, uv);
            for i in 0..boundary.len() {
                append_triangle(
                    mesh,
                    [centre, boundary[i], boundary[(i + 1) % boundary.len()]],
                );
            }
        }
    }
}
fn append_vertex(mesh: &mut Mesh, p: Vec3, normal: Vec3, uv: Vec2) -> u32 {
    let id = mesh.vertices.len() as u32;
    mesh.vertices.push(p);
    mesh.normals.push(normal);
    mesh.uv.push(uv);
    id
}
fn append_triangle(mesh: &mut Mesh, t: [u32; 3]) {
    let [a, b, c] = t.map(|i| mesh.vertices[i as usize].as_dvec3());
    if (b - a).cross(c - a).length_squared() > 0.0 {
        mesh.triangles.extend(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipped_neighbours_share_exact_subedges_without_moving_distant_vertices() {
        let origin = Vec3::new(1200.0, 1000.0, 30.0);
        let positions = [
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0002),
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0002),
        ];
        let mut mesh = Mesh {
            vertices: positions
                .map(|p| (origin + p) / ISLAND_WORLD_METRES)
                .to_vec(),
            normals: vec![Vec3::Z; 6],
            uv: vec![Vec2::ZERO; 6],
            triangles: vec![0, 1, 2, 1, 3, 4, 3, 5, 4],
        };
        let distant = mesh.vertices[2];
        repair(
            &mut mesh,
            origin - Vec3::splat(2.0),
            origin + Vec3::splat(2.0),
            [],
        );
        assert_eq!(mesh.vertices[0], mesh.vertices[5]);
        assert_eq!(mesh.vertices[2], distant);
        for (a, b) in [
            (mesh.vertices[0], mesh.vertices[3]),
            (mesh.vertices[3], mesh.vertices[1]),
        ] {
            let mut uses = [0, 0];
            for t in mesh.triangles.chunks_exact(3) {
                for i in 0..3 {
                    let p = mesh.vertices[t[i] as usize];
                    let q = mesh.vertices[t[(i + 1) % 3] as usize];
                    if p == a && q == b {
                        uses[0] += 1;
                    }
                    if p == b && q == a {
                        uses[1] += 1;
                    }
                }
            }
            assert_eq!(
                uses,
                [1, 1],
                "each shared subedge must occur in both directions"
            );
        }
    }
}
