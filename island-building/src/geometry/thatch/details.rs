//! Close-view reed ends and the hazel spars holding the ridge dressing.
use super::{Builder, Coat, RoofSection, Vertex, unit};
use std::f32::consts::TAU;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn surface_reeds(builder: &mut Builder, coat: &Coat<'_>, section: RoofSection) {
    let slope_scale = (1. + section.slope().powi(2)).sqrt();
    let area = (section.x[1] - section.x[0]) * coat.half[1] * 2. * slope_scale;
    let share = (section.x[1] - section.x[0]) / (coat.half[0] * 2.);
    let count = (area * 100.).min(12_000. * share).ceil() as u16;
    let seed = coat.recipe.seed ^ u64::from(section.flip_u);
    let min_x = section.x[0].max(-coat.half[0] + coat.detail_inset + 0.03);
    let max_x = section.x[1].min(coat.half[0] - coat.detail_inset - 0.03);
    let half_y = coat.half[1] - coat.detail_inset - 0.04;
    for index in 0..count {
        let centre = [
            min_x + (max_x - min_x) * noise(seed, index, 17),
            (noise(seed, index, 31) * 2. - 1.) * half_y,
        ];
        let length = (0.08 + noise(seed, index, 53) * 0.16) / slope_scale;
        let width = 0.0015 + noise(seed, index, 71) * 0.002;
        let direction = if section.flip_u { 1. } else { -1. };
        let drift = (noise(seed, index, 89) - 0.5) * 0.035;
        let lift = 0.002 + noise(seed, index, 101) * 0.004;
        let root_x = (centre[0] - direction * length * 0.5).clamp(min_x, max_x);
        let tip_x = (centre[0] + direction * length * 0.5).clamp(min_x, max_x);
        if coat.is_ridge(root_x) != coat.is_ridge(tip_x) {
            continue;
        }
        let stations = [
            ([root_x, centre[1] - drift * 0.5], -0.001, width * 0.3),
            ([(root_x + tip_x) * 0.5, centre[1]], lift * 0.8, width),
            ([tip_x, centre[1] + drift * 0.5], lift, width * 0.08),
        ];
        for pair in stations.windows(2) {
            let tilt = (pair[1].1 - pair[0].1) / (pair[1].0[0] - pair[0].0[0]);
            let start = reed_vertex(coat, section, pair[0], -1., tilt);
            let end = reed_vertex(coat, section, pair[1], -1., tilt);
            let mut vertices = [
                start,
                end,
                reed_vertex(coat, section, pair[1], 1., tilt),
                reed_vertex(coat, section, pair[0], 1., tilt),
            ];
            if direction < 0. {
                vertices.reverse();
            }
            coat.emit(builder, vertices, false);
        }
    }
}

fn reed_vertex(
    coat: &Coat<'_>,
    section: RoofSection,
    station: ([f32; 2], f32, f32),
    side: f32,
    tilt: f32,
) -> Vertex {
    let ([x, centre_y], lift, width) = station;
    let y = centre_y + side * width;
    let mut normal = coat.normal(x, y);
    normal[0] -= tilt * normal[2];
    normal[1] += side * 0.22;
    coat.vertex(section, [x, y, coat.height(x, y) + lift], unit(normal))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn ridge_bindings(builder: &mut Builder, coat: &Coat<'_>, section: RoofSection) {
    let high = if section.flip_u {
        section.x[0]
    } else {
        section.x[1]
    };
    let inward = if section.flip_u { 1. } else { -1. };
    let width = coat.ridge_width();
    let y0 = -coat.half[1] + coat.detail_inset + 0.04;
    let y1 = coat.half[1] - coat.detail_inset - 0.04;
    for t in [0.25, 0.72] {
        let x = high + inward * width * t;
        rod(builder, coat, section, [[x, y0], [x, y1]]);
    }
    let count = ((y1 - y0) / 0.68).floor().max(1.) as u16;
    for index in 0..count {
        let y = y0 + (f32::from(index) + 0.5) / f32::from(count) * (y1 - y0);
        let near = high + inward * width * 0.12;
        let far = high + inward * width * 0.90;
        for sign in [-1., 1.] {
            rod(
                builder,
                coat,
                section,
                [[near, y - sign * 0.14], [far, y + sign * 0.14]],
            );
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// Follow the rounded LOD0 surface with short straight hazel segments.
fn rod(builder: &mut Builder, coat: &Coat<'_>, section: RoofSection, ends: [[f32; 2]; 2]) {
    let delta = [ends[1][0] - ends[0][0], ends[1][1] - ends[0][1]];
    let length = delta[0].hypot(delta[1]);
    let count = (length / 0.16).ceil().clamp(1., 300.) as u16;
    for segment in 0..count {
        let points = [f32::from(segment), f32::from(segment + 1)].map(|s| {
            let t = s / f32::from(count);
            let [x, y] = std::array::from_fn(|i| ends[0][i] + delta[i] * t);
            [x, y, coat.height(x, y) + 0.004]
        });
        let tangent = unit(std::array::from_fn(|i| points[1][i] - points[0][i]));
        let side = unit(cross(tangent, coat.normal(points[0][0], points[0][1])));
        let up = unit(cross(side, tangent));
        let vertex = |point: [f32; 3], index: u16| {
            let angle = f32::from(index) / 6. * TAU;
            let normal = std::array::from_fn(|i| side[i] * angle.cos() + up[i] * angle.sin());
            coat.vertex(
                section,
                std::array::from_fn(|i| point[i] + normal[i] * 0.004),
                normal,
            )
        };
        for face in 0..6 {
            coat.emit(
                builder,
                [
                    vertex(points[0], face),
                    vertex(points[1], face),
                    vertex(points[1], face + 1),
                    vertex(points[0], face + 1),
                ],
                false,
            );
        }
    }
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn noise(seed: u64, index: u16, salt: u64) -> f32 {
    let mut value = seed ^ u64::from(index).wrapping_mul(0x9e37_79b9) ^ salt;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    (value & 0xffff) as f32 / 65_535.
}
