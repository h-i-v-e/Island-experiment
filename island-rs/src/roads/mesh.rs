use super::{RoadJunction, RoadNetwork, RoadStrip};
use crate::{Vec2, Vec3};
use std::collections::HashMap;
use std::f32::consts::TAU;

#[derive(Debug, Default, PartialEq)]
pub struct RoadMesh {
    /// Native XY metres and Z-up elevations.
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Native planar metre coordinates for continuous paving texture.
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl RoadMesh {
    fn vertex(&mut self, point: Vec2, height: f32, normal: Vec3) -> u32 {
        let index = u32::try_from(self.positions.len()).expect("bounded road geometry");
        self.positions.push([point.x, point.y, height]);
        self.normals.push(normal.to_array());
        self.uvs.push(point.to_array());
        index
    }

    fn strip(&mut self, strip: &RoadStrip, direction: Vec2) {
        let side = direction.perp() * strip.half_width;
        for span in strip.stations.windows(2) {
            let (a, b) = (span[0], span[1]);
            let slope = (b.height - a.height) / a.point.distance(b.point);
            let normal = (-direction * slope).extend(1.).normalize();
            let left_start = self.vertex(a.point + side, a.height, normal);
            let right_start = self.vertex(a.point - side, a.height, normal);
            let left_end = self.vertex(b.point + side, b.height, normal);
            let right_end = self.vertex(b.point - side, b.height, normal);
            self.indices.extend_from_slice(&[
                left_start,
                right_start,
                left_end,
                right_start,
                right_end,
                left_end,
            ]);
        }
    }

    fn junction(&mut self, junction: RoadJunction, mut seams: Vec<Seam>) {
        if junction.radius <= 0. || seams.is_empty() {
            return;
        }
        if let Some((axis, half)) = junction.rectangle() {
            self.rectangular_junction(junction, axis, half, &seams);
            return;
        }
        seams.sort_unstable_by(|a, b| a.heading.total_cmp(&b.heading));
        let centre = self.vertex(junction.point, junction.height, Vec3::Z);
        for index in 0..seams.len() {
            let seam = seams[index];
            let next = seams[(index + 1) % seams.len()];
            let a = self.vertex(seam.right, junction.height, Vec3::Z);
            let b = self.vertex(seam.left, junction.height, Vec3::Z);
            if (seam.right - junction.point).perp_dot(seam.left - junction.point) > 0.0001 {
                self.indices.extend_from_slice(&[centre, a, b]);
            }
            self.junction_arc(junction, centre, b, seam.left, next.right);
        }
    }

    fn rectangular_junction(
        &mut self,
        junction: RoadJunction,
        axis: Vec2,
        half: Vec2,
        seams: &[Seam],
    ) {
        let along = axis * half.x;
        let across = axis.perp() * half.y;
        let corners = [
            along + across,
            -along + across,
            -along - across,
            along - across,
        ];
        let mut boundary = Vec::with_capacity(4 + 2 * seams.len());
        // Include the strip seam vertices on the boundary, including where a
        // narrower strip meets the middle of an edge.
        for point in seams
            .iter()
            .flat_map(|seam| [seam.right, seam.left])
            .chain(corners.map(|corner| junction.point + corner))
        {
            if boundary
                .iter()
                .all(|&other| point.distance_squared(other) > 0.000_001)
            {
                boundary.push(point);
            }
        }
        boundary.sort_unstable_by(|a, b| {
            (a - junction.point)
                .to_angle()
                .total_cmp(&(b - junction.point).to_angle())
        });
        let centre = self.vertex(junction.point, junction.height, Vec3::Z);
        let first = self.vertex(boundary[0], junction.height, Vec3::Z);
        let mut last = first;
        for point in boundary.into_iter().skip(1) {
            let index = self.vertex(point, junction.height, Vec3::Z);
            self.indices.extend_from_slice(&[centre, last, index]);
            last = index;
        }
        self.indices.extend_from_slice(&[centre, last, first]);
    }

    // An arc is bounded by one turn, split into at most 128 spans.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn junction_arc(
        &mut self,
        junction: RoadJunction,
        centre: u32,
        start: u32,
        from: Vec2,
        to: Vec2,
    ) {
        let a = (from - junction.point).to_angle();
        let angle = ((to - junction.point).to_angle() - a).rem_euclid(TAU);
        // Sub-centimetre deviation for the default town widths.
        let segments = (angle * 128. / TAU).ceil() as u16;
        if segments == 0 {
            return;
        }
        let mut last = start;
        for index in 1..segments {
            let t = angle * (f32::from(index) / f32::from(segments));
            let point = junction.point + Vec2::from_angle(a + t) * junction.radius;
            let vertex = self.vertex(point, junction.height, Vec3::Z);
            self.indices.extend_from_slice(&[centre, last, vertex]);
            last = vertex;
        }
        let end = self.vertex(to, junction.height, Vec3::Z);
        self.indices.extend_from_slice(&[centre, last, end]);
    }
}

#[derive(Clone, Copy)]
struct Seam {
    heading: f32,
    right: Vec2,
    left: Vec2,
}

impl Seam {
    fn new(point: Vec2, direction: Vec2, half_width: f32) -> Self {
        let offset = direction.perp() * half_width;
        Self {
            heading: direction.to_angle(),
            right: point - offset,
            left: point + offset,
        }
    }
}

impl RoadNetwork {
    /// Generate the paving surface. Junction mouths end at chords shared by
    /// adjacent strips so there are no overlapping faces at a connection.
    #[must_use]
    pub fn mesh(&self) -> RoadMesh {
        let mut mesh = RoadMesh::default();
        let mut seams: HashMap<_, Vec<_>> = self
            .junctions
            .iter()
            .map(|junction| (junction.node, Vec::new()))
            .collect();
        for strip in &self.strips {
            let [first, .., last] = strip.stations.as_slice() else {
                continue;
            };
            let direction = (last.point - first.point).normalize();
            mesh.strip(strip, direction);
            if let Some(start) = seams.get_mut(&strip.start) {
                start.push(Seam::new(first.point, direction, strip.half_width));
            }
            if let Some(end) = seams.get_mut(&strip.end) {
                end.push(Seam::new(last.point, -direction, strip.half_width));
            }
        }
        for &junction in &self.junctions {
            if let Some(seams) = seams.remove(&junction.node) {
                mesh.junction(junction, seams);
            }
        }
        mesh
    }
}
