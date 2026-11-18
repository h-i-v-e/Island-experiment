//! Closed roof projections around the cruciform nave and transepts.
use super::{atlas, plan::Plan, windows};
use crate::{ChurchTowerStyle, MaterialSlot, geometry::Builder};

fn boundary(p: &Plan<'_>) -> [[f32; 2]; 16] {
    let (south, north) = (p.y - p.t, p.y + p.t);
    [
        [-p.n, p.nave_front()],
        [0., p.nave_front()],
        [p.n, p.nave_front()],
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
        let (mut a, mut c, mut d, mut e) = (inner[i], outer[i], outer[j], inner[j]);
        if p.c.tower_style == ChurchTowerStyle::SingleFront && i < 2 {
            // The front roof meets the tower; only its exposed shoulders have eaves.
            let x = if i == 0 {
                -p.c.tower_width * 0.5
            } else {
                p.c.tower_width * 0.5
            };
            let clip = |u: [f32; 3], v: [f32; 3]| {
                let t = (x - u[0]) / (v[0] - u[0]);
                std::array::from_fn(|axis| u[axis] + (v[axis] - u[axis]) * t)
            };
            if i == 0 {
                d = clip(c, d);
                e = clip(a, e);
            } else {
                a = clip(a, e);
                c = clip(c, d);
            }
        }
        face(b, level, MaterialSlot::Roof, [a, c, d, e]);
        face(
            b,
            level,
            MaterialSlot::Trim,
            [below(e), below(d), below(c), below(a)],
        );
        face(b, level, MaterialSlot::Trim, [c, below(c), below(d), d]);
    }
}
fn face(b: &mut Builder, level: u8, slot: MaterialSlot, points: [[f32; 3]; 4]) {
    if level == 2 {
        atlas::face(b, if slot == MaterialSlot::Roof { 1 } else { 4 }, points);
    } else {
        windows::surface(b, slot, points);
    }
}
