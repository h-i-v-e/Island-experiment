//! A small segment BVH supplies consistent metre coordinates to clipped river chunks.
use planet_rs::{DVec3, PlanetTerrain};
use std::collections::HashMap;
struct Segment {
    a: DVec3,
    b: DVec3,
    mouth: f64,
    length: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_coordinates_follow_the_network_and_survive_edge_clipping() {
        let mut terrain = crate::collision::test_scene().terrain;
        for (i, (p, h)) in [
            (DVec3::new(0.0, 1.0, 0.02), 0.02),
            (DVec3::Y, 0.01),
            (DVec3::new(0.0, 1.0, -0.02), 0.0),
            (DVec3::new(0.02, 1.0, 0.01), 0.015),
        ]
        .into_iter()
        .enumerate()
        {
            terrain.topology.directions[i] = p.normalize();
            terrain.water_elevations[i] = h;
            terrain.elevations[i] = h - 0.005;
        }
        terrain.rivers = [(0, 1), (1, 2), (3, 1)]
            .map(|(from, to)| planet_rs::RiverSegment {
                from,
                to,
                catchment: 1.0,
            })
            .into();
        let field = FlowField::new(&terrain);
        let point = |i: usize| terrain.topology.directions[i] * (1.0 + terrain.water_elevations[i]);
        let upstream = field.uv(point(0));
        let junction = field.uv(point(1));
        let downstream = field.uv(point(2));
        assert!(upstream[1] < junction[1] && junction[1] < downstream[1]);
        assert!(field.uv(point(3))[1] < junction[1]);
        assert!(upstream[0].abs() < 0.001 && junction[0].abs() < 0.001);
        let clipped = field.uv(point(0) * 0.7 + point(1) * 0.3);
        assert!((clipped[1] - (upstream[1] * 0.7 + junction[1] * 0.3)).abs() < 0.001);
        let flow = (point(1) - point(0)).normalize();
        let right = flow.cross(point(0).normalize()).normalize();
        let bank = field.uv(point(0) + right / terrain.topology.radius);
        assert!((bank[0] - 1.0).abs() < 0.005);
        assert_eq!(FlowField::default().uv(DVec3::Y).map(f32::to_bits), [0; 2]);
    }
}
struct Node {
    lo: DVec3,
    hi: DVec3,
    range: std::ops::Range<usize>,
    children: Option<[usize; 2]>,
}
#[derive(Default)]
pub struct FlowField {
    segments: Vec<Segment>,
    indices: Vec<usize>,
    nodes: Vec<Node>,
    radius: f64,
}
impl FlowField {
    pub fn new(terrain: &PlanetTerrain) -> Self {
        let outgoing: HashMap<_, _> = terrain
            .rivers
            .iter()
            .enumerate()
            .map(|(i, r)| (r.from, i))
            .collect();
        let mut distances = vec![None; terrain.rivers.len()];
        let positions = |i: usize| {
            terrain.topology.directions[i]
                * (1.0 + terrain.water_elevations[i].max(terrain.elevations[i]))
        };
        for start in 0..terrain.rivers.len() {
            if distances[start].is_some() {
                continue;
            }
            let mut path = Vec::new();
            let mut current = start;
            let mut distance = loop {
                if let Some(d) = distances[current] {
                    break d;
                }
                if path.len() > terrain.rivers.len() {
                    break 0.0;
                }
                path.push(current);
                let to = terrain.rivers[current].to;
                if let Some(&next) = outgoing.get(&to) {
                    current = next;
                } else {
                    break 0.0;
                }
            };
            for i in path.into_iter().rev() {
                let r = terrain.rivers[i];
                distance += positions(r.from).distance(positions(r.to)) * terrain.topology.radius;
                distances[i] = Some(distance);
            }
        }
        let mut field = Self {
            radius: terrain.topology.radius,
            ..Self::default()
        };
        field.segments = terrain
            .rivers
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let (a, b) = (positions(r.from), positions(r.to));
                let length = a.distance(b) * field.radius;
                Segment {
                    a,
                    b,
                    mouth: distances[i].unwrap_or(length) - length,
                    length,
                }
            })
            .collect();
        field.indices = (0..field.segments.len()).collect();
        if !field.segments.is_empty() {
            field.build(0..field.segments.len());
        }
        field
    }
    fn build(&mut self, range: std::ops::Range<usize>) -> usize {
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = -lo;
        for &i in &self.indices[range.clone()] {
            let s = &self.segments[i];
            lo = lo.min(s.a).min(s.b);
            hi = hi.max(s.a).max(s.b);
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            lo,
            hi,
            range: range.clone(),
            children: None,
        });
        if range.len() > 8 {
            let span = hi - lo;
            let axis = if span.x > span.y && span.x > span.z {
                0
            } else if span.y > span.z {
                1
            } else {
                2
            };
            let middle = range.start + range.len() / 2;
            self.indices[range.clone()].select_nth_unstable_by(range.len() / 2, |&a, &b| {
                let centre = |i: usize| self.segments[i].a[axis] + self.segments[i].b[axis];
                centre(a).total_cmp(&centre(b))
            });
            let children = [
                self.build(range.start..middle),
                self.build(middle..range.end),
            ];
            self.nodes[index].children = Some(children);
        }
        index
    }
    #[allow(clippy::cast_possible_truncation)]
    pub fn uv(&self, p: DVec3) -> [f32; 2] {
        if self.nodes.is_empty() {
            return [0.0; 2];
        }
        let mut best = (f64::INFINITY, 0_usize, 0.0);
        self.nearest(0, p, &mut best);
        let s = &self.segments[best.1];
        let flow = (s.b - s.a).normalize_or_zero();
        let right = flow.cross(p.normalize()).normalize_or_zero();
        [
            ((p - s.a).dot(right) * self.radius) as f32,
            (-(s.mouth + s.length * (1.0 - best.2))) as f32,
        ]
    }
    fn nearest(&self, index: usize, p: DVec3, best: &mut (f64, usize, f64)) {
        let node = &self.nodes[index];
        if p.distance_squared(p.clamp(node.lo, node.hi)) > best.0 {
            return;
        }
        if let Some(children) = node.children {
            let distance =
                |i: usize| p.distance_squared(p.clamp(self.nodes[i].lo, self.nodes[i].hi));
            let [a, b] = if distance(children[0]) < distance(children[1]) {
                children
            } else {
                [children[1], children[0]]
            };
            self.nearest(a, p, best);
            self.nearest(b, p, best);
        } else {
            for &i in &self.indices[node.range.clone()] {
                let s = &self.segments[i];
                let edge = s.b - s.a;
                let t = ((p - s.a).dot(edge) / edge.length_squared().max(1e-20)).clamp(0.0, 1.0);
                let distance = p.distance_squared(s.a + edge * t);
                if distance < best.0 {
                    *best = (distance, i, t);
                }
            }
        }
    }
}
