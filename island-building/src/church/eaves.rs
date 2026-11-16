//! Closed roof projections around the cruciform nave and transepts.
use super::{atlas, plan::Plan, windows};
use crate::{MaterialSlot, geometry::Builder};

fn boundary(p: &Plan<'_>) -> [[f32; 2]; 16] {
    let (south, north) = (p.y - p.t, p.y + p.t);
    [
        [-p.n, -p.d],
        [0., -p.d],
        [p.n, -p.d],
        [p.n, south],
        [p.w, south],
        [p.w, p.y],
        [p.w, north],
        [p.n, north],
        [p.n, p.d],
        [0., p.d],
        [-p.n, p.d],
        [-p.n, north],
        [-p.w, north],
        [-p.w, p.y],
        [-p.w, south],
        [-p.n, south],
    ]
}
fn height(p: &Plan<'_>, xy: [f32; 2]) -> f32 {
    p.r.wall_height + p.r.roof.rise
        - (p.r.roof.rise * xy[0].abs() / p.n).min(p.r.roof.rise * (xy[1] - p.y).abs() / p.t)
}
fn outward(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let length = (b[0] - a[0]).hypot(b[1] - a[1]);
    [(b[1] - a[1]) / length, (a[0] - b[0]) / length]
}
pub(super) fn generate(b: &mut Builder, p: &Plan<'_>, level: u8) {
    if p.r.roof.overhang <= 0. {
        return;
    }
    let outline = boundary(p);
    let inner = outline.map(|xy| [xy[0], xy[1], height(p, xy)]);
    let outer = std::array::from_fn::<_, 16, _>(|i| {
        let a = outward(outline[(i + 15) % 16], outline[i]);
        let c = outward(outline[i], outline[(i + 1) % 16]);
        let scale = p.r.roof.overhang / (1. + a[0] * c[0] + a[1] * c[1]);
        let xy = [
            outline[i][0] + (a[0] + c[0]) * scale,
            outline[i][1] + (a[1] + c[1]) * scale,
        ];
        [xy[0], xy[1], height(p, xy)]
    });
    let below = |mut v: [f32; 3]| {
        v[2] -= super::shell::CEILING_DROP;
        v
    };
    for i in 0..16 {
        let j = (i + 1) % 16;
        face(
            b,
            level,
            MaterialSlot::Roof,
            [inner[i], outer[i], outer[j], inner[j]],
        );
        face(
            b,
            level,
            MaterialSlot::Trim,
            [
                below(inner[j]),
                below(outer[j]),
                below(outer[i]),
                below(inner[i]),
            ],
        );
        face(
            b,
            level,
            MaterialSlot::Trim,
            [outer[i], below(outer[i]), below(outer[j]), outer[j]],
        );
    }
}
fn face(b: &mut Builder, level: u8, slot: MaterialSlot, points: [[f32; 3]; 4]) {
    if level == 2 {
        atlas::face(b, if slot == MaterialSlot::Roof { 1 } else { 4 }, points);
    } else {
        windows::surface(b, slot, points);
    }
}
