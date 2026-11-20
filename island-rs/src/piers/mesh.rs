//! Terrain-fitted quay and oak pile work in the island's Z-up metre coordinates.

use super::PierLayout;
use crate::{Mesh, Vec2, Vec3};
use serde::Serialize;

const FOUNDATION_EMBED: f32 = 0.45;
const VISIBLE_STONE_HEIGHT: f32 = 5.5;
const PILE_BAY: f32 = 3.35;
const PILE_INSET: f32 = 0.42;
const DECK_THICKNESS: f32 = 0.27;
const FENDER_PROJECTION: f32 = 0.19;
const STAIR_WIDTH: f32 = 1.02;
const BERTH_WALK_INSET: f32 = 0.43;
const CARGO_WALK_INSET: f32 = 1.14;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PierPropPoint {
    pub position: Vec3,
    pub heading: Vec2,
    pub kind: &'static str,
}

#[derive(Debug, Default)]
pub struct PierMeshes {
    pub timber: Mesh,
    pub stone: Mesh,
    pub metal: Mesh,
    pub coarse_timber: Mesh,
    pub coarse_stone: Mesh,
    pub collision: Mesh,
    pub navigation: Mesh,
    pub prop_points: Vec<PierPropPoint>,
}

/// Builds the fixed pier structure. The terrain callback supplies load-bearing elevations.
#[must_use]
pub fn build_pier_meshes(
    layout: &PierLayout,
    sample: &impl Fn(Vec2) -> Option<f32>,
) -> Option<PierMeshes> {
    let frame = Frame::new(layout)?;
    let mut meshes = PierMeshes::default();
    build_quay(&mut meshes, frame, sample)?;
    build_deck(&mut meshes, frame);
    build_bents(&mut meshes, frame, sample)?;
    let berth_station = (layout.berth.position.truncate() - layout.shore).dot(frame.axis);
    build_moorings(&mut meshes, frame, berth_station, layout.berth.length);
    build_walkway(&mut meshes.navigation, frame);
    meshes.prop_points = prop_points(frame, berth_station);

    append_mesh(&mut meshes.collision, &meshes.coarse_stone);
    append_mesh(&mut meshes.collision, &meshes.coarse_timber);
    for mesh in [
        &mut meshes.timber,
        &mut meshes.stone,
        &mut meshes.metal,
        &mut meshes.coarse_timber,
        &mut meshes.coarse_stone,
        &mut meshes.collision,
        &mut meshes.navigation,
    ] {
        mesh.vertices.iter_mut().for_each(|v| *v = frame.world(*v));
        mesh.normals.iter_mut().for_each(|n| *n = frame.rotate(*n));
    }
    Some(meshes)
}

#[derive(Clone, Copy)]
struct Frame {
    shore: Vec2,
    axis: Vec2,
    start: f32,
    inland_height: f32,
    tip: f32,
    quay_end: f32,
    deck: f32,
    sea: f32,
    edge: f32,
    berth_side: f32,
}

impl Frame {
    fn new(layout: &PierLayout) -> Option<Self> {
        let axis = layout.axis.try_normalize()?;
        let start = layout.start.truncate() - layout.shore;
        let end = layout.end - layout.shore;
        let inland = start.dot(axis);
        let tip = end.dot(axis);
        let sea = layout.berth.position.z;
        if !layout.start.is_finite()
            || !layout.berth.position.is_finite()
            || !(0.2..=8.).contains(&(layout.deck_height - sea))
            || !layout.berth.length.is_finite()
            || !(3. ..=12.).contains(&layout.width)
            || !(-256. ..=-0.05).contains(&inland)
            || !(8. ..=512.).contains(&tip)
            || start.dot(axis.perp()).abs() > 0.05
            || end.dot(axis.perp()).abs() > 0.05
            || !layout.berth_side.is_finite()
            || layout.berth_side.abs() <= f32::EPSILON
        {
            return None;
        }
        Some(Self {
            shore: layout.shore,
            axis,
            start: inland,
            inland_height: layout.start.z,
            tip,
            quay_end: (tip * 0.14).clamp(1.6, 3.8),
            deck: layout.deck_height,
            sea,
            edge: layout.width * 0.5,
            berth_side: layout.berth_side.signum(),
        })
    }

    fn rotate(self, point: Vec3) -> Vec3 {
        (self.axis * point.x + self.axis.perp() * point.y).extend(point.z)
    }

    fn world(self, point: Vec3) -> Vec3 {
        self.shore.extend(0.) + self.rotate(point)
    }

    fn ground(self, sample: &impl Fn(Vec2) -> Option<f32>, x: f32, y: f32) -> Option<f32> {
        sample(self.world(Vec3::new(x, y, 0.)).truncate()).filter(|z| z.is_finite())
    }

    fn footing(
        self,
        sample: &impl Fn(Vec2) -> Option<f32>,
        x: f32,
        y: f32,
        half: Vec2,
    ) -> Option<f32> {
        let ground = self.ground(sample, x, y)?;
        [
            (-half.x, -half.y),
            (half.x, -half.y),
            (half.x, half.y),
            (-half.x, half.y),
        ]
        .into_iter()
        .try_fold(ground, |low, (dx, dy)| {
            Some(low.min(self.ground(sample, x + dx, y + dy)?))
        })
    }

    fn walking_height(self, x: f32) -> f32 {
        let ramp = (x / self.start).clamp(0., 1.);
        self.inland_height.mul_add(ramp, self.deck * (1. - ramp))
    }
}

fn build_quay(
    meshes: &mut PierMeshes,
    frame: Frame,
    sample: &impl Fn(Vec2) -> Option<f32>,
) -> Option<()> {
    let start = QuaySection::new(frame, sample, frame.start)?;
    let mut first = start;
    for x in stations(frame.start, 0., 1.1)
        .skip(1)
        .chain(stations(0., frame.quay_end, 1.1).skip(1))
    {
        let next = QuaySection::new(frame, sample, x)?;
        first.connect(next, frame.edge, &mut meshes.coarse_stone, false);
        first.connect(next, frame.edge, &mut meshes.stone, true);
        first = next;
    }

    // The exposed seaward face receives the timber stringers.
    for mesh in [&mut meshes.coarse_stone, &mut meshes.stone] {
        first.close(frame.edge, Vec3::X, mesh);
        start.close(frame.edge, -Vec3::X, mesh);
    }
    build_quay_steps(meshes, frame, sample)
}

#[derive(Clone, Copy)]
struct QuaySection {
    x: f32,
    top: f32,
    bearing: [f32; 2],
}

impl QuaySection {
    fn new(frame: Frame, sample: &impl Fn(Vec2) -> Option<f32>, x: f32) -> Option<Self> {
        let top = frame.walking_height(x);
        let ground = |y| Some(frame.ground(sample, x, y)?.min(top) - FOUNDATION_EMBED);
        Some(Self {
            x,
            top,
            bearing: [ground(-frame.edge)?, ground(frame.edge)?],
        })
    }

    fn top(self, y: f32) -> Vec3 {
        Vec3::new(self.x, y, self.top)
    }

    fn bottom(self, y: f32, side: usize) -> Vec3 {
        Vec3::new(self.x, y, self.bearing[side])
    }

    fn connect(self, next: Self, edge: f32, mesh: &mut Mesh, detailed: bool) {
        quad(
            mesh,
            [
                self.top(-edge),
                next.top(-edge),
                next.top(edge),
                self.top(edge),
            ],
            Vec3::Z,
            Vec3::Y,
        );
        quad_split(
            mesh,
            [
                self.bottom(-edge, 0),
                next.bottom(-edge, 0),
                next.bottom(edge, 1),
                self.bottom(edge, 1),
            ],
            -Vec3::Z,
            Vec3::Y,
        );
        for (side, y) in [-edge, edge].into_iter().enumerate() {
            let outward = Vec3::Y * y.signum();
            let corners = [
                self.bottom(y, side),
                next.bottom(y, side),
                next.top(y),
                self.top(y),
            ];
            quad(mesh, corners, outward, Vec3::X);
            if detailed {
                add_stone_courses(mesh, corners, outward, Vec3::X);
            }
        }
    }

    fn close(self, edge: f32, outward: Vec3, mesh: &mut Mesh) {
        quad(
            mesh,
            [
                self.bottom(-edge, 0),
                self.bottom(edge, 1),
                self.top(edge),
                self.top(-edge),
            ],
            outward,
            Vec3::Y,
        );
    }
}

fn add_stone_courses(mesh: &mut Mesh, mut face: [Vec3; 4], outward: Vec3, along: Vec3) {
    face[0].z = face[0].z.max(face[3].z - VISIBLE_STONE_HEIGHT);
    face[1].z = face[1].z.max(face[2].z - VISIBLE_STONE_HEIGHT);
    let width = (face[1] - face[0]).dot(along);
    let height = (face[3].z - face[0].z).max(face[2].z - face[1].z);
    let rows = divisions(height, 0.38);
    let columns = divisions(width, 0.59);
    let point = |horizontal, vertical| {
        let left = face[0].lerp(face[3], vertical);
        let right = face[1].lerp(face[2], vertical);
        left.lerp(right, horizontal)
    };
    for row in 0..rows {
        let bottom = (f32::from(row) + 0.05) / f32::from(rows);
        let top = (f32::from(row) + 0.95) / f32::from(rows);
        let stagger = if row % 2 == 0 { 0. } else { 0.5 };
        for column in 0..=columns {
            let station = f32::from(column) - stagger;
            let left = station.max(0.) / f32::from(columns) + 0.018;
            let right = (station + 1.).min(f32::from(columns)) / f32::from(columns) - 0.018;
            if right > left {
                let block = [
                    point(left, bottom),
                    point(right, bottom),
                    point(right, top),
                    point(left, top),
                ];
                let variant = f32::from((row * 3 + column * 5) % 7) / 6.;
                beveled_stone(mesh, block, outward, along, variant);
            }
        }
    }
}

fn beveled_stone(mesh: &mut Mesh, base: [Vec3; 4], outward: Vec3, along: Vec3, variant: f32) {
    let center = (base[0] + base[1] + base[2] + base[3]) * 0.25;
    let bevel = variant.mul_add(0.09, 0.08);
    let relief = outward * variant.mul_add(0.022, 0.023);
    let face = base.map(|v| v.lerp(center, bevel) + relief);
    quad(mesh, face, outward, along);
    for i in 0..4 {
        let j = (i + 1) % 4;
        let edge_normal = (base[i] + base[j]) * 0.5 - center + relief;
        quad(
            mesh,
            [base[i], base[j], face[j], face[i]],
            edge_normal,
            along,
        );
    }
}

fn build_quay_steps(
    meshes: &mut PierMeshes,
    frame: Frame,
    sample: &impl Fn(Vec2) -> Option<f32>,
) -> Option<()> {
    let side = -frame.berth_side;
    let y = side * (frame.edge + STAIR_WIDTH * 0.5);
    let low = frame.sea - 0.26;
    let rise = frame.deck - low;
    let steps = divisions(rise, 0.23);
    let run = 0.32;
    // The highest tread opens directly onto the quay. The lowest is within reach at low water.
    let start = frame.quay_end - 1.15;
    for step in 0..=steps {
        let x = f32::from(step).mul_add(run, start);
        let top = frame.deck - f32::from(step) * rise / f32::from(steps);
        let bottom = frame
            .footing(
                sample,
                x + run * 0.5,
                y,
                Vec2::new(run * 0.5, STAIR_WIDTH * 0.5),
            )?
            .min(low)
            - FOUNDATION_EMBED;
        for mesh in [&mut meshes.stone, &mut meshes.coarse_stone] {
            let middle = (top + bottom) * 0.5;
            beam(
                mesh,
                Vec3::new(x, y, middle),
                Vec3::new(x + run, y, middle),
                STAIR_WIDTH,
                top - bottom,
                Vec3::Y,
            );
        }
    }
    Some(())
}

fn build_deck(meshes: &mut PierMeshes, frame: Frame) {
    let width = frame.tip - frame.quay_end;
    let middle = f32::midpoint(frame.quay_end, frame.tip);
    for (mesh, recess) in [(&mut meshes.timber, 0.09), (&mut meshes.coarse_timber, 0.)] {
        let depth = DECK_THICKNESS - recess;
        let z = frame.deck - recess - depth * 0.5;
        beam(
            mesh,
            Vec3::new(middle, -frame.edge, z),
            Vec3::new(middle, frame.edge, z),
            width,
            depth,
            Vec3::X,
        );
    }

    // Adjoining decks share the exact ramp/wood seam. The thin seams reveal the lining below.
    let count = divisions(frame.tip - frame.quay_end, 0.31);
    let pitch = (frame.tip - frame.quay_end) / f32::from(count);
    for plank in 0..count {
        let x = f32::from(plank).mul_add(pitch, frame.quay_end);
        let before = if plank == 0 { 0. } else { 0.006 };
        let last = if plank + 1 == count {
            frame.tip
        } else {
            x + pitch - 0.006
        };
        let center = (x + before + last) * 0.5;
        let z = frame.deck - 0.045;
        beam(
            &mut meshes.timber,
            Vec3::new(center, -frame.edge, z),
            Vec3::new(center, frame.edge, z),
            last - x - before,
            0.09,
            Vec3::X,
        );
    }

    let z = frame.deck - 0.37;
    for lateral in [-0.76, -0.26, 0.26, 0.76] {
        let y = frame.edge * lateral;
        for mesh in [&mut meshes.timber, &mut meshes.collision] {
            beam(
                mesh,
                Vec3::new(frame.quay_end - 0.08, y, z),
                Vec3::new(frame.tip, y, z),
                0.22,
                0.25,
                Vec3::Y,
            );
        }
    }
}

#[derive(Clone, Copy)]
struct Bent {
    station: f32,
    bottom: [Vec3; 2],
    top: [Vec3; 2],
}

impl Bent {
    fn new(frame: Frame, sample: &impl Fn(Vec2) -> Option<f32>, station: f32) -> Option<Self> {
        let inset = frame.edge - PILE_INSET;
        let top = [-inset, inset].map(|y| Vec3::new(station, y, frame.deck - 0.34));
        let bottom = |side: f32| {
            let y = (inset + 0.13) * side;
            let x = side.mul_add(0.06, station);
            let ground = frame
                .footing(sample, x, y, Vec2::splat(0.18))?
                .min(frame.deck - 0.82);
            Some(Vec3::new(x, y, ground - FOUNDATION_EMBED))
        };
        Some(Self {
            station,
            bottom: [bottom(-1.)?, bottom(1.)?],
            top,
        })
    }

    fn add_to(self, mesh: &mut Mesh, frame: Frame) {
        for (bottom, top) in self.bottom.into_iter().zip(self.top) {
            beam(mesh, bottom, top, 0.34, 0.31, Vec3::X);
        }

        let z = frame.deck - 0.49;
        beam(
            mesh,
            Vec3::new(self.station, -frame.edge, z),
            Vec3::new(self.station, frame.edge, z),
            0.31,
            0.30,
            Vec3::X,
        );

        let y = frame.berth_side * (frame.edge + FENDER_PROJECTION - 0.13);
        let bottom = frame.sea - 0.84;
        if bottom < frame.deck - 0.05 {
            beam(
                mesh,
                Vec3::new(self.station, y, bottom),
                Vec3::new(self.station, y, frame.deck - 0.05),
                0.24,
                0.26,
                Vec3::X,
            );
        }
    }

    fn brace_end(self, index: usize, low_z: f32) -> Vec3 {
        let a = self.bottom[index];
        let b = self.top[index];
        a.lerp(b, ((low_z - a.z) / (b.z - a.z)).max(0.14))
    }

    fn cross_brace(self, mesh: &mut Mesh, frame: Frame) {
        let low_z = frame.sea - 0.84;
        for (low, high) in [(0, 1), (1, 0)] {
            let a = self.brace_end(low, low_z);
            let b = self.top[high] - Vec3::Z * 0.21;
            if b.z - a.z > 0.44 {
                beam(mesh, a, b, 0.15, 0.18, Vec3::X);
            }
        }
    }
}

fn build_bents(
    meshes: &mut PierMeshes,
    frame: Frame,
    sample: &impl Fn(Vec2) -> Option<f32>,
) -> Option<()> {
    let first = frame.quay_end + 0.84;
    let last = frame.tip - 0.43;
    let mut previous: Option<Bent> = None;
    for (index, x) in stations(first, last, PILE_BAY).enumerate() {
        let bent = Bent::new(frame, sample, x)?;
        bent.add_to(&mut meshes.timber, frame);
        bent.add_to(&mut meshes.coarse_timber, frame);
        bent.cross_brace(&mut meshes.timber, frame);
        if index % 2 == 0 {
            bent.cross_brace(&mut meshes.coarse_timber, frame);
        } else {
            bent.cross_brace(&mut meshes.collision, frame);
        }
        if let Some(prior) = previous {
            for side in 0..2 {
                let (low, high) = if index % 2 == side {
                    (prior, bent)
                } else {
                    (bent, prior)
                };
                let a = low.brace_end(side, frame.sea - 1.04);
                let b = high.top[side] - Vec3::Z * 0.22;
                if b.z - a.z > 0.55 {
                    for mesh in [&mut meshes.timber, &mut meshes.collision] {
                        beam(mesh, a, b, 0.15, 0.18, Vec3::Y);
                    }
                }
            }
        }
        previous = Some(bent);
    }
    Some(())
}

fn build_moorings(meshes: &mut PierMeshes, frame: Frame, center: f32, length: f32) {
    let reach = (length * 0.48).max(2.25);
    let end = frame.tip - 0.47;
    let start = frame.quay_end + 0.56;
    // Leave space at midships for a gangway, paired with fittings toward the bow and stern.
    for x in [(center - reach).max(start), (center + reach).min(end)] {
        if !(start..=end).contains(&x) || (x - center).abs() < 1.65 {
            continue;
        }
        let base = Vec3::new(x, frame.berth_side * (frame.edge - 0.23), frame.deck - 0.47);
        let top = base + Vec3::Z * 1.13;
        for mesh in [&mut meshes.timber, &mut meshes.coarse_timber] {
            beam(mesh, base, top, 0.32, 0.32, Vec3::X);
        }
        beam(
            &mut meshes.metal,
            top - Vec3::Z * 0.13,
            top - Vec3::Z * 0.065,
            0.336,
            0.336,
            Vec3::X,
        );
        let ring_center = top - Vec3::Z * 0.26 + Vec3::Y * frame.berth_side * 0.177;
        iron_ring(&mut meshes.metal, ring_center);
    }
}

fn iron_ring(mesh: &mut Mesh, center: Vec3) {
    const AROUND: u16 = 12;
    const TUBE: u16 = 5;
    let radial = |angle| {
        let (sin, cos) = circle(angle, AROUND).sin_cos();
        Vec3::new(cos, 0., sin)
    };
    let outward_at = |ray, side| {
        let (sin, cos) = circle(side, TUBE).sin_cos();
        ray * cos + Vec3::Y * sin
    };
    let position = |ray, side| center + ray * 0.10 + outward_at(ray, side) * 0.023;
    for angle in 0..AROUND {
        let a = radial(angle);
        let b = radial(angle + 1);
        let middle = (a + b).normalize();
        for tube in 0..TUBE {
            let out = outward_at(middle, tube) + outward_at(middle, tube + 1);
            let corners = [
                position(a, tube),
                position(b, tube),
                position(b, tube + 1),
                position(a, tube + 1),
            ];
            quad_split(mesh, corners, out, b - a);
        }
    }
}

fn circle(index: u16, count: u16) -> f32 {
    f32::from(index % count) / f32::from(count) * std::f32::consts::TAU
}

fn build_walkway(mesh: &mut Mesh, frame: Frame) {
    let berth = frame.berth_side * (frame.edge - BERTH_WALK_INSET);
    let offside = -frame.berth_side * (frame.edge - CARGO_WALK_INSET);
    let low = berth.min(offside);
    let high = berth.max(offside);
    for [a, b] in [[frame.start, 0.], [0., frame.tip]] {
        let start = frame.walking_height(a);
        let end = frame.walking_height(b);
        quad(
            mesh,
            [
                Vec3::new(a, low, start),
                Vec3::new(b, low, end),
                Vec3::new(b, high, end),
                Vec3::new(a, high, start),
            ],
            Vec3::Z,
            Vec3::X,
        );
    }
}

fn prop_points(frame: Frame, berth_station: f32) -> Vec<PierPropPoint> {
    let first = frame.quay_end + 1.15;
    let last = (berth_station - 3.).min(frame.tip * 0.48);
    let y = -frame.berth_side * (frame.edge - 0.62);
    [first, first + 2.25, first + 4.5]
        .into_iter()
        .take_while(|x| *x < last)
        .map(|x| PierPropPoint {
            position: frame.world(Vec3::new(x, y, frame.deck)),
            heading: frame.axis,
            kind: "cargo",
        })
        .collect()
}

fn beam(mesh: &mut Mesh, start: Vec3, end: Vec3, width: f32, depth: f32, width_axis: Vec3) {
    let Some(axis) = (end - start).try_normalize() else {
        return;
    };
    let lateral = (width_axis - axis * width_axis.dot(axis)).normalize();
    let u = lateral * (width * 0.5);
    let v = axis.cross(lateral) * (depth * 0.5);
    let corners = [-u - v, u - v, u + v, -u + v];
    quad(mesh, corners.map(|offset| start + offset), -axis, lateral);
    quad(mesh, corners.map(|offset| end + offset), axis, lateral);
    for i in 0..4 {
        let j = (i + 1) % 4;
        let outside = corners[i] + corners[j];
        quad(
            mesh,
            [
                start + corners[i],
                end + corners[i],
                end + corners[j],
                start + corners[j],
            ],
            outside,
            axis,
        );
    }
}

fn quad(mesh: &mut Mesh, points: [Vec3; 4], outward: Vec3, along: Vec3) {
    face(mesh, points, outward, along);
}

fn quad_split(mesh: &mut Mesh, [a, b, c, d]: [Vec3; 4], outward: Vec3, along: Vec3) {
    face(mesh, [a, b, c], outward, along);
    face(mesh, [a, c, d], outward, along);
}

fn face<const N: usize>(mesh: &mut Mesh, points: [Vec3; N], outward: Vec3, along: Vec3) {
    let Some(mut normal) = (points[1] - points[0])
        .cross(points[2] - points[0])
        .try_normalize()
    else {
        return;
    };
    let reverse = normal.dot(outward) < 0.;
    if reverse {
        normal = -normal;
    }
    let tangent = (along - normal * along.dot(normal)).normalize_or_zero();
    let bitangent = normal.cross(tangent);
    let base = u32::try_from(mesh.vertices.len()).expect("bounded pier mesh");
    mesh.vertices.extend(points);
    mesh.normals.extend([normal; N]);
    mesh.uv
        .extend(points.map(|p| Vec2::new(p.dot(tangent), p.dot(bitangent))));
    for i in 2..u32::try_from(N).expect("triangle or quad") {
        let (b, c) = if reverse { (i, i - 1) } else { (i - 1, i) };
        mesh.triangles.extend([base, base + b, base + c]);
    }
}

fn append_mesh(target: &mut Mesh, source: &Mesh) {
    let base = u32::try_from(target.vertices.len()).expect("bounded pier mesh");
    target.vertices.extend_from_slice(&source.vertices);
    target.normals.extend_from_slice(&source.normals);
    target.uv.extend_from_slice(&source.uv);
    target
        .triangles
        .extend(source.triangles.iter().map(|i| base + i));
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn divisions(length: f32, pitch: f32) -> u16 {
    (length / pitch).ceil().clamp(1., 2048.) as u16
}

fn stations(first: f32, last: f32, pitch: f32) -> impl Iterator<Item = f32> {
    let count = divisions(last - first, pitch);
    (0..=count).map(move |i| {
        if i == count {
            last
        } else {
            (last - first).mul_add(f32::from(i) / f32::from(count), first)
        }
    })
}

#[cfg(test)]
#[path = "mesh_tests.rs"]
mod tests;
