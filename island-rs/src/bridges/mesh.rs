//! Metre-scaled stonework in the island's native XY plane, with Z up.

use super::{BridgeBay, BridgeLayout, BridgeSupport};
use crate::{Mesh, Vec2, Vec3};

const WALL_WIDTH: f32 = 0.42;
const COPING_OVERHANG: f32 = 0.05;
const WALL_HEIGHT: f32 = 0.78;
const COPING_HEIGHT: f32 = 0.92;

#[derive(Debug)]
pub struct BridgeMeshes {
    pub masonry: Mesh,
    pub dressing: Mesh,
    pub deck: Mesh,
    pub coarse_masonry: Mesh,
    pub coarse_deck: Mesh,
    pub collision: Mesh,
}

#[must_use]
pub fn build_bridge_meshes(layout: &BridgeLayout) -> BridgeMeshes {
    let fine = Stonework::build(layout, true);
    let coarse = Stonework::build(layout, false);
    let mut collision = Mesh::default();
    // PhysX receives one continuous shell, including both parapets and the
    // underside of each open barrel. The rendering LOD never controls physics.
    append_mesh(&mut collision, &coarse.masonry);
    append_mesh(&mut collision, &coarse.deck);
    BridgeMeshes {
        masonry: fine.masonry,
        dressing: fine.dressing,
        deck: fine.deck,
        coarse_masonry: coarse.masonry,
        coarse_deck: coarse.deck,
        collision,
    }
}

impl BridgeLayout {
    fn wall_edge(&self) -> f32 {
        self.width * 0.5 + WALL_WIDTH
    }

    /// Maximum plan extent from the carriageway centreline over these stations.
    pub(crate) fn clearance_half_width(&self, start: f32, end: f32) -> f32 {
        let coping = self.wall_edge() + COPING_OVERHANG;
        if self.channel.is_none() {
            return coping;
        }
        self.supports
            .iter()
            .skip(1)
            .take(self.supports.len().saturating_sub(2))
            .filter_map(|pier| {
                let cut = self.cutwater(pier)?;
                let mid = (pier.start + pier.end) * 0.5;
                // Skewed cutwater noses can project past a pier's station interval.
                let nose = cut.offset.dot(self.axis).abs();
                let first = (mid - nose).min(pier.start);
                let last = (mid + nose).max(pier.end);
                (first <= end && last >= start).then(|| cut.offset.length())
            })
            .fold(coping, f32::max)
    }

    fn cutwater(&self, pier: &BridgeSupport) -> Option<Cutwater> {
        let channel = self.channel.as_ref()?;
        let mid = Section::at(self, (pier.start + pier.end) * 0.5);
        let flow = Vec2::from_array(channel.direction).try_normalize()?;
        let left_flow = flow * flow.dot(mid.left).signum();
        if left_flow.dot(mid.left) < 0.05 {
            return None;
        }
        let shoulder_limit = self.deck_height(pier.start).min(self.deck_height(pier.end)) - 0.18;
        let height = (channel.water_height + 0.65)
            .max(pier.bottom + 0.75)
            .min(shoulder_limit);
        let depth = height - pier.bottom;
        if depth < 0.1 {
            return None;
        }
        let length = ((pier.end - pier.start) * 0.85).min(1.4);
        Some(Cutwater {
            center: mid.center,
            offset: mid.left * self.wall_edge() + left_flow * length,
            shoulder: height,
            nose: height - depth.min(0.55) * 0.6,
        })
    }
}

fn append_mesh(target: &mut Mesh, source: &Mesh) {
    let base = u32::try_from(target.vertices.len()).expect("bounded bridge mesh");
    target.vertices.extend_from_slice(&source.vertices);
    target.normals.extend_from_slice(&source.normals);
    target.uv.extend_from_slice(&source.uv);
    target
        .triangles
        .extend(source.triangles.iter().map(|i| base + i));
}

#[derive(Clone, Copy)]
struct Section {
    center: Vec2,
    left: Vec2,
    station: f32,
    deck: f32,
}

impl Section {
    fn at(layout: &BridgeLayout, station: f32) -> Self {
        Self {
            center: layout.point(station, 0.0),
            left: layout.axis.perp(),
            station,
            deck: layout.deck_height(station),
        }
    }

    fn point(self, side: f32, height: f32) -> Vec3 {
        (self.center + self.left * side).extend(height)
    }

    fn above(self, side: f32, offset: f32) -> Vec3 {
        self.point(side, self.deck + offset)
    }
}

struct Stonework<'a> {
    layout: &'a BridgeLayout,
    detailed: bool,
    masonry: Mesh,
    dressing: Mesh,
    deck: Mesh,
}

impl<'a> Stonework<'a> {
    fn build(layout: &'a BridgeLayout, detailed: bool) -> Self {
        let mut stone = Self {
            layout,
            detailed,
            masonry: Mesh::default(),
            dressing: Mesh::default(),
            deck: Mesh::default(),
        };
        for (index, support) in layout.supports.iter().enumerate() {
            stone.support(support, index);
        }
        for bay in &layout.bays {
            stone.barrel(bay);
        }
        stone
    }

    fn barrel(&mut self, bay: &BridgeBay) {
        let span = bay.end - bay.start;
        let segments = 2 * divisions(span * 0.5, if self.detailed { 0.52 } else { 1.0 }).max(2);
        let arc = Arch::new(bay);
        let mut left = arc.section(self.layout, 0, segments, self.detailed);
        for step in 1..=segments {
            let right = arc.section(self.layout, step, segments, self.detailed);
            self.barrel_segment(left, right);
            left = right;
        }
    }

    fn barrel_segment(&mut self, a: ArchSection, b: ArchSection) {
        let edge = self.layout.wall_edge();
        let soffit = [
            a.inner.point(-edge, a.soffit),
            b.inner.point(-edge, b.soffit),
            b.inner.point(edge, b.soffit),
            a.inner.point(edge, a.soffit),
        ];
        // Unroll the underside in metre coordinates along the curve.
        let barrel_uv = [
            Vec2::new(a.arc_length, -edge),
            Vec2::new(b.arc_length, -edge),
            Vec2::new(b.arc_length, edge),
            Vec2::new(a.arc_length, edge),
        ];
        face_uv(&mut self.masonry, soffit, -Vec3::Z, barrel_uv);
        for side in [-1.0, 1.0] {
            let x = edge * side;
            let outward = (a.outer.left + b.outer.left).normalize().extend(0.0) * side;
            let face = [
                a.outer.point(x, a.extrados),
                b.outer.point(x, b.extrados),
                b.outer.above(x, 0.0),
                a.outer.above(x, 0.0),
            ];
            surface(&mut self.masonry, face, outward, self.layout.axis);
            if self.detailed {
                let block = [
                    a.inner.point(x, a.soffit),
                    b.inner.point(x, b.soffit),
                    b.outer.point(x, b.extrados),
                    a.outer.point(x, a.extrados),
                ];
                voussoir(&mut self.dressing, block, outward);
            }
        }
        self.roadway(a.outer, b.outer);
    }

    fn support(&mut self, pier: &BridgeSupport, index: usize) {
        let first = Section::at(self.layout, pier.start);
        let last = Section::at(self.layout, pier.end);
        let previous = index.checked_sub(1).and_then(|i| self.layout.bays.get(i));
        let next = self.layout.bays.get(index);
        let internal = previous.is_some() && next.is_some();
        let cutwater = internal.then(|| self.layout.cutwater(pier)).flatten();

        for (end, bay, sign) in [(first, previous, -1.0), (last, next, 1.0)] {
            let top = bay.map_or(end.deck, |bay| bay.spring);
            self.support_return(end, pier.bottom, top, sign);
            if bay.is_none() {
                self.parapet_ends(end, sign);
            }
        }

        let divisions = divisions(
            pier.end - pier.start,
            if self.detailed { 0.78 } else { 1.0 },
        );
        let mut a = first;
        for step in 1..=divisions {
            let b = if step == divisions {
                last
            } else {
                let fraction = f32::from(step) / f32::from(divisions);
                Section::at(self.layout, pier.start + (pier.end - pier.start) * fraction)
            };
            self.support_segment(a, b, pier.bottom, cutwater, step == 1, step == divisions);
            self.roadway(a, b);
            a = b;
        }
    }

    fn support_return(&mut self, s: Section, bottom: f32, top: f32, sign: f32) {
        let edge = self.layout.wall_edge();
        let corners = [
            s.point(-edge, bottom),
            s.point(edge, bottom),
            s.point(edge, top),
            s.point(-edge, top),
        ];
        surface(
            &mut self.masonry,
            corners,
            (-s.left.perp() * sign).extend(0.0),
            s.left,
        );
    }

    fn support_segment(
        &mut self,
        a: Section,
        b: Section,
        bottom: f32,
        cut: Option<Cutwater>,
        first: bool,
        last: bool,
    ) {
        let edge = self.layout.wall_edge();
        let base = [
            a.point(-edge, bottom),
            a.point(edge, bottom),
            b.point(edge, bottom),
            b.point(-edge, bottom),
        ];
        surface(&mut self.masonry, base, -Vec3::Z, self.layout.axis);
        for sign in [-1.0, 1.0] {
            let side = sign * edge;
            let out = ((a.left + b.left) * sign).extend(0.0);
            let low = cut.map_or(bottom, |cut| cut.shoulder);
            let wall = [
                a.point(side, low),
                b.point(side, low),
                b.above(side, 0.0),
                a.above(side, 0.0),
            ];
            surface(&mut self.masonry, wall, out, self.layout.axis);
            if let Some(cut) = cut {
                let tip = cut.center + cut.offset * sign;
                let nose = tip.extend(cut.nose);
                surface(
                    &mut self.masonry,
                    [a.point(side, low), b.point(side, low), nose],
                    Vec3::Z,
                    self.layout.axis,
                );
                surface(
                    &mut self.masonry,
                    [
                        a.point(side, bottom),
                        b.point(side, bottom),
                        tip.extend(bottom),
                    ],
                    -Vec3::Z,
                    self.layout.axis,
                );
                for (station, end, direction) in [(a, first, -1.0), (b, last, 1.0)] {
                    if end {
                        let shoulder = station.point(side, low);
                        let tangent = (tip - shoulder.truncate()).normalize();
                        let outward = (tangent.perp() * sign * -direction).extend(0.0);
                        surface(
                            &mut self.masonry,
                            [
                                shoulder,
                                nose,
                                tip.extend(bottom),
                                station.point(side, bottom),
                            ],
                            outward,
                            tangent,
                        );
                    }
                }
            }
        }
    }

    fn roadway(&mut self, a: Section, b: Section) {
        let half = self.layout.width * 0.5;
        let surface_points = [
            a.above(-half, 0.0),
            b.above(-half, 0.0),
            b.above(half, 0.0),
            a.above(half, 0.0),
        ];
        let deck_uv = strip_uv(a.station, b.station, -half, half);
        face_uv(&mut self.deck, surface_points, Vec3::Z, deck_uv);
        let edge = self.layout.wall_edge();
        let top = if self.detailed {
            WALL_HEIGHT
        } else {
            COPING_HEIGHT
        };
        for side in [-1.0, 1.0] {
            let out = ((a.left + b.left) * side).extend(0.0);
            for (x, direction) in [(side * half, -out), (side * edge, out)] {
                let wall = [
                    a.above(x, 0.0),
                    b.above(x, 0.0),
                    b.above(x, top),
                    a.above(x, top),
                ];
                surface(&mut self.masonry, wall, direction, self.layout.axis);
            }
            if self.detailed {
                self.coping(a, b, side);
            } else {
                let top_face = [
                    a.above(side * half, top),
                    b.above(side * half, top),
                    b.above(side * edge, top),
                    a.above(side * edge, top),
                ];
                face_uv(
                    &mut self.masonry,
                    top_face,
                    Vec3::Z,
                    strip_uv(a.station, b.station, side * half, side * edge),
                );
            }
        }
    }

    fn coping(&mut self, a: Section, b: Section, side: f32) {
        let profile = coping_profile(self.layout.width * 0.5);
        // The top arris dips into a shallow joint at both ends of every cap.
        // Interpolate on this wall segment so the joints share the masonry edges.
        let rim_a = profile.map(|(x, z)| a.above(side * x, z));
        let rim_b = profile.map(|(x, z)| b.above(side * x, z));
        let fraction = (0.018 / (b.station - a.station)).min(0.15);
        let left = std::array::from_fn::<_, 7, _>(|i| rim_a[i].lerp(rim_b[i], fraction));
        let right = std::array::from_fn::<_, 7, _>(|i| rim_b[i].lerp(rim_a[i], fraction));
        let joint = |mut rim: [Vec3; 7]| {
            for vertex in &mut rim[1..=4] {
                vertex.z -= 0.012;
            }
            rim
        };
        let outward = ((a.left + b.left) * (0.5 * side)).extend(0.0);
        let first_joint = a.station + (b.station - a.station) * fraction;
        let last_joint = b.station - (b.station - a.station) * fraction;
        let strips = [
            (joint(rim_a), left, a.station, first_joint),
            (left, right, first_joint, last_joint),
            (right, joint(rim_b), last_joint, b.station),
        ];
        for (start, end, start_station, end_station) in strips {
            // Unroll the coping profile across V so the same span-aligned grain
            // continues over the flat top, chamfers and vertical edges.
            let mut across = profile[0].0;
            for (i, pair) in profile.windows(2).enumerate() {
                let (dx, dz) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
                let next = across + dx.hypot(dz);
                let normal = -outward * dz + Vec3::Z * dx;
                face_uv(
                    &mut self.dressing,
                    [start[i], end[i], end[i + 1], start[i + 1]],
                    normal,
                    strip_uv(start_station, end_station, side * across, side * next),
                );
                across = next;
            }
        }
    }

    fn parapet_ends(&mut self, s: Section, sign: f32) {
        let half = self.layout.width * 0.5;
        let edge = self.layout.wall_edge();
        let out = (-s.left.perp() * sign).extend(0.0);
        let top = if self.detailed {
            WALL_HEIGHT
        } else {
            COPING_HEIGHT
        };
        for side in [-1.0, 1.0] {
            let wall = [
                s.above(side * half, 0.0),
                s.above(side * edge, 0.0),
                s.above(side * edge, top),
                s.above(side * half, top),
            ];
            surface(&mut self.masonry, wall, out, s.left);
            if self.detailed {
                let rim = coping_profile(half).map(|(x, z)| s.above(side * x, z));
                let middle = s.above(side * (half + WALL_WIDTH * 0.5), WALL_HEIGHT + 0.06);
                let mut cap = rim;
                for vertex in &mut cap[1..=4] {
                    vertex.z -= 0.012;
                }
                for i in 0..cap.len() {
                    surface(
                        &mut self.dressing,
                        [middle, cap[i], cap[(i + 1) % cap.len()]],
                        out,
                        s.left,
                    );
                }
            }
        }
    }
}

fn coping_profile(half: f32) -> [(f32, f32); 7] {
    let edge = half + WALL_WIDTH;
    [
        (half, WALL_HEIGHT),
        (half, COPING_HEIGHT - 0.035),
        (half + 0.035, COPING_HEIGHT),
        (edge + 0.015, COPING_HEIGHT),
        (edge + COPING_OVERHANG, COPING_HEIGHT - 0.035),
        (edge + COPING_OVERHANG, WALL_HEIGHT + 0.035),
        (edge, WALL_HEIGHT),
    ]
}

#[derive(Clone, Copy)]
struct Cutwater {
    center: Vec2,
    offset: Vec2,
    shoulder: f32,
    nose: f32,
}

struct Arch<'a> {
    bay: &'a BridgeBay,
    radius: f32,
    angle: f32,
    thickness: f32,
}

#[derive(Clone, Copy)]
struct ArchSection {
    inner: Section,
    outer: Section,
    soffit: f32,
    extrados: f32,
    arc_length: f32,
}

impl<'a> Arch<'a> {
    fn new(bay: &'a BridgeBay) -> Self {
        let half = (bay.end - bay.start) * 0.5;
        let radius = 0.5 * (half * half / bay.rise + bay.rise);
        Self {
            bay,
            radius,
            angle: half.atan2(radius - bay.rise),
            thickness: (half * 0.15).clamp(0.16, 0.38),
        }
    }

    fn section(&self, layout: &BridgeLayout, step: u16, count: u16, dressed: bool) -> ArchSection {
        let middle = (self.bay.start + self.bay.end) * 0.5;
        let angle = (2.0 * f32::from(step) / f32::from(count) - 1.0) * self.angle;
        let sine = angle.sin();
        let (station, soffit) = if step == 0 {
            (self.bay.start, self.bay.spring)
        } else if step == count {
            (self.bay.end, self.bay.spring)
        } else {
            let station = middle + self.radius * sine;
            (station, self.bay.intrados(station))
        };
        let inner = Section::at(layout, station);
        let (outer, extrados) = if dressed {
            // Interior bed joints fan from the centre of the arch. At each
            // spring the ring terminates flush against the neighbouring pier.
            let bearing = (station - self.bay.start).min(self.bay.end - station);
            let outer_station = station + sine * bearing.min(self.thickness);
            let outer = Section::at(layout, outer_station);
            let radius = self.radius + self.thickness;
            let x = outer_station - middle;
            // Stable difference sqrt(radius^2 - x^2) - radius for shallow arcs.
            let sag = x * x / (radius + (radius * radius - x * x).max(0.0).sqrt());
            let height = self.bay.spring + self.bay.rise + self.thickness - sag;
            (outer, height.min(outer.deck - 0.025))
        } else {
            (inner, soffit)
        };
        ArchSection {
            inner,
            outer,
            soffit,
            extrados,
            arc_length: (self.angle + angle) * self.radius,
        }
    }
}

// One dressed face per wedge with four chamfered arrises. Neighbouring
// bevels meet at the recessed joint with no coplanar face underneath.
fn voussoir(mesh: &mut Mesh, frame: [Vec3; 4], outward: Vec3) {
    let tangent = (frame[1] - frame[0]).normalize();
    let radial = (frame[3] - frame[0])
        .reject_from_normalized(tangent)
        .normalize();
    let uv = |p: Vec3| Vec2::new((p - frame[0]).dot(tangent), (p - frame[0]).dot(radial));
    let bilinear = |u, v| {
        frame[0]
            .lerp(frame[1], u)
            .lerp(frame[3].lerp(frame[2], u), v)
    };
    let inset = [
        (0.025, 0.045),
        (0.975, 0.045),
        (0.975, 0.955),
        (0.025, 0.955),
    ]
    .map(|(u, v)| bilinear(u, v) + outward * 0.026);
    face_uv(mesh, inset, outward, inset.map(uv));
    for i in 0..4 {
        let j = (i + 1) % 4;
        let bevel = [frame[i], frame[j], inset[j], inset[i]];
        face_uv(mesh, bevel, outward, bevel.map(uv));
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn divisions(length: f32, target: f32) -> u16 {
    (length / target).ceil().clamp(1.0, 2048.0) as u16
}

fn strip_uv(start: f32, end: f32, near: f32, far: f32) -> [Vec2; 4] {
    [
        Vec2::new(start, near),
        Vec2::new(end, near),
        Vec2::new(end, far),
        Vec2::new(start, far),
    ]
}

// Horizontal surfaces tile in world XY; walls stay upright and metrically
// scaled along the supplied horizontal direction.
fn surface<const N: usize>(mesh: &mut Mesh, p: [Vec3; N], out: Vec3, along: Vec2) {
    let uv = if out.z.abs() > out.truncate().length() {
        p.map(Vec3::truncate)
    } else {
        p.map(|v| Vec2::new(v.truncate().dot(along), v.z))
    };
    face_uv(mesh, p, out, uv);
}

fn face_uv<const N: usize>(mesh: &mut Mesh, p: [Vec3; N], out: Vec3, uv: [Vec2; N]) {
    let cross = (p[1] - p[0]).cross(p[2] - p[0]);
    let Some(mut normal) = cross.try_normalize() else {
        return;
    };
    let reverse = normal.dot(out) < 0.0;
    if reverse {
        normal = -normal;
    }
    let base = u32::try_from(mesh.vertices.len()).expect("bounded bridge mesh");
    mesh.vertices.extend_from_slice(&p);
    mesh.uv.extend_from_slice(&uv);
    mesh.normals.extend([normal; N]);
    for index in 2..N {
        let i = u32::try_from(index).expect("triangle or quad");
        let triangle = if reverse {
            [base, base + i, base + i - 1]
        } else {
            [base, base + i - 1, base + i]
        };
        mesh.triangles.extend(triangle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridges::{BridgeChannel, BridgeKind, BridgeRequest, BridgeSurface, fit_bridge};

    fn river() -> BridgeLayout {
        let request = BridgeRequest {
            start: [512., -380., 5.],
            end: [550., -380., 5.4],
            start_heading: Some([1., 0.12]),
            end_heading: Some([1., -0.1]),
            width: 4.,
            kind: BridgeKind::River,
            surface: BridgeSurface::Paved,
            channel: Some(BridgeChannel {
                center: [531., -379.],
                direction: [0.3, 1.],
                half_width: 3.,
                water_height: 2.5,
            }),
        };
        fit_bridge(&request, |point| Some(1. + 0.01 * (point.x - 512.))).unwrap()
    }

    #[test]
    fn both_lods_have_consistent_outward_faces_and_exact_deck_edges() {
        let layout = river();
        let m = build_bridge_meshes(&layout);
        for mesh in [
            &m.masonry,
            &m.dressing,
            &m.deck,
            &m.coarse_masonry,
            &m.coarse_deck,
            &m.collision,
        ] {
            assert_eq!(mesh.vertices.len(), mesh.normals.len());
            assert_eq!(mesh.vertices.len(), mesh.uv.len());
            assert!(mesh.uv.iter().all(|uv| uv.is_finite()));
            for ix in mesh.triangles.chunks_exact(3) {
                let (i, j, k) = (ix[0] as usize, ix[1] as usize, ix[2] as usize);
                let (a, b, c) = (mesh.vertices[i], mesh.vertices[j], mesh.vertices[k]);
                let cross = (b - a).cross(c - a);
                assert!(
                    cross.dot(mesh.normals[i]) > 1e-9,
                    "inverted or collapsed face at {a}, {b}, {c}"
                );
                assert!(mesh.normals[i].is_normalized());
            }
        }
        for deck in [&m.deck, &m.coarse_deck] {
            for (position, uv) in deck.vertices.iter().zip(&deck.uv) {
                assert_eq!(uv.y.abs(), layout.width * 0.5);
                assert!(
                    position.distance(layout.point(uv.x, uv.y).extend(layout.deck_height(uv.x)))
                        < 1e-4
                );
            }
            for station in [0., layout.length] {
                for edge in [-layout.width * 0.5, layout.width * 0.5] {
                    assert!(deck.uv.contains(&Vec2::new(station, edge)));
                }
            }
        }
        assert!(
            m.coarse_masonry.triangles.len()
                < (m.masonry.triangles.len() + m.dressing.triangles.len()) / 3
        );
    }

    #[test]
    fn coping_texture_runs_along_a_diagonal_span_at_both_lods() {
        let layout = fit_bridge(
            &BridgeRequest {
                start: [60., 100., 5.],
                end: [68., 106., 5.4],
                start_heading: None,
                end_heading: None,
                width: 3.8,
                kind: BridgeKind::Ravine,
                surface: BridgeSurface::Paved,
                channel: None,
            },
            |_| Some(1.),
        )
        .unwrap();
        let bridge = build_bridge_meshes(&layout);
        for mesh in [&bridge.dressing, &bridge.coarse_masonry] {
            let mut samples = 0;
            for ((point, normal), uv) in mesh.vertices.iter().zip(&mesh.normals).zip(&mesh.uv) {
                let offset = (*point - layout.start).truncate();
                let station = offset.dot(layout.axis);
                let lateral = offset.dot(layout.axis.perp());
                if normal.z > 0.95
                    && lateral.abs() > layout.width * 0.5 - 0.0001
                    && point.z >= layout.deck_height(station) + COPING_HEIGHT - 0.014
                {
                    assert!((uv.x - station).abs() < 0.0001, "{point:?} {uv:?}");
                    assert!((uv.y - lateral).abs() < 0.15, "{point:?} {uv:?}");
                    samples += 1;
                }
            }
            assert!(samples >= 8, "both parapets should have textured top faces");
        }
    }

    // Moller-Trumbore intersection with a finite segment, two-sided.
    fn intersects(mesh: &Mesh, origin: Vec3, delta: Vec3) -> bool {
        mesh.triangles.chunks_exact(3).any(|ix| {
            let (a, b, c) = (
                mesh.vertices[ix[0] as usize],
                mesh.vertices[ix[1] as usize],
                mesh.vertices[ix[2] as usize],
            );
            let edge = b - a;
            let side = c - a;
            let p = delta.cross(side);
            let det = edge.dot(p);
            if det.abs() < 1e-7 {
                return false;
            }
            let offset = origin - a;
            let u = offset.dot(p) / det;
            let q = offset.cross(edge);
            let v = delta.dot(q) / det;
            let t = side.dot(q) / det;
            u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001 && (0. ..=1.).contains(&t)
        })
    }

    #[test]
    fn collision_has_open_barrels_solid_piers_rails_and_walkable_deck() {
        let layout = river();
        let mesh = build_bridge_meshes(&layout).collision;
        for bay in &layout.bays {
            let station = (bay.start + bay.end) * 0.5;
            let left = layout.axis.perp().extend(0.);
            let center = layout.point(station, 0.);
            let deck = center.extend(layout.deck_height(station));
            let water = center.extend(bay.intrados(station) - 0.25);
            assert!(!intersects(&mesh, water - left * 8., left * 16.));
            assert!(intersects(&mesh, water, Vec3::Z));
            assert!(intersects(&mesh, deck + Vec3::Z, -2. * Vec3::Z));
            for side in [-1., 1.] {
                assert!(intersects(&mesh, deck + 0.65 * Vec3::Z, side * left * 5.));
            }
        }
        let flow = Vec2::from_array(layout.channel.unwrap().direction).normalize();
        assert_eq!(
            layout.clearance_half_width(0., 0.25),
            layout.wall_edge() + COPING_OVERHANG
        );
        for pier in layout
            .supports
            .iter()
            .skip(1)
            .take(layout.supports.len() - 2)
        {
            let station = (pier.start + pier.end) * 0.5;
            let half = layout.width * 0.5 + WALL_WIDTH;
            let projection = (0.85 * (pier.end - pier.start)).min(1.4);
            for side in [-1., 1.] {
                let tip = layout.point(station, side * half) + side * flow * projection;
                assert!(
                    layout.clearance_half_width(pier.start, pier.end) + 1e-4
                        >= tip.distance(layout.point(station, 0.0))
                );
                assert!(
                    mesh.vertices
                        .iter()
                        .any(|p| p.truncate().distance(tip) < 1e-4)
                );
            }
            let start = layout.point(station, -8.).extend(pier.bottom + 0.35);
            let ray = 16. * layout.axis.perp().extend(0.);
            assert!(intersects(&mesh, start, ray));
        }
    }
}
