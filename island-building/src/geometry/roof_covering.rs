//! Bounded roof relief. Every band is cut around the complete chimney shaft.
use super::{Builder, RoofSection, chimney};
use crate::{BuildingRecipe, MaterialSlot, RoofCovering};

/// Slate course lips provide depth while the material supplies individual joints.
/// Large buildings retain a bounded number of broad bands.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn courses(b: &mut Builder, recipe: &BuildingRecipe, section: RoofSection) {
    let length = (section.x[1] - section.x[0]) * (1. + section.slope().powi(2)).sqrt();
    let pitch: f32 = match recipe.roof.covering {
        RoofCovering::Slate => 0.25,
        RoofCovering::StandingSeam | RoofCovering::Thatch => return,
    };
    let pitch = pitch.max(length / 128.);
    let count = (length / pitch).ceil().clamp(1., 128.) as u16;
    let step = pitch / (1. + section.slope().powi(2)).sqrt();
    for index in 0..count {
        let near = f32::from(index) * step;
        let far = f32::from(index + 1) * step;
        let (x0, x1) = if section.flip_u {
            ((section.x[1] - far).max(section.x[0]), section.x[1] - near)
        } else {
            (section.x[0] + near, (section.x[0] + far).min(section.x[1]))
        };
        if x1 - x0 <= 0.00001 {
            continue;
        }
        // Keep the final partial course's original gradient at the ridge.
        let high_relief = 0.025 - 0.023 * ((x1 - x0) / step);
        let relief = if section.flip_u {
            [high_relief, 0.025]
        } else {
            [0.025, high_relief]
        };
        band(b, recipe, section, [x0, x1], relief);
    }
}

fn band(b: &mut Builder, recipe: &BuildingRecipe, s: RoofSection, x: [f32; 2], relief: [f32; 2]) {
    for (min, max) in chimney::roof_cells(recipe, [x[0], -s.depth], [x[1], s.depth]) {
        let height =
            |point: f32| relief[0] + (relief[1] - relief[0]) * (point - x[0]) / (x[1] - x[0]);
        wedge(b, s, min, max, [height(min[0]), height(max[0])]);
    }
}

fn wedge(b: &mut Builder, s: RoofSection, min: [f32; 2], max: [f32; 2], relief: [f32; 2]) {
    let [x0, y0] = min;
    let [x1, y1] = max;
    let z0 = s.height(x0) + s.thickness;
    let z1 = s.height(x1) + s.thickness;
    let h0 = z0 + relief[0];
    let h1 = z1 + relief[1];
    for p in [
        [[x0, y0, h0], [x1, y0, h1], [x1, y1, h1], [x0, y1, h0]],
        [[x0, y0, z0], [x1, y0, z1], [x1, y0, h1], [x0, y0, h0]],
        [[x1, y1, z1], [x0, y1, z0], [x0, y1, h0], [x1, y1, h1]],
        [[x0, y1, z0], [x0, y0, z0], [x0, y0, h0], [x0, y1, h0]],
        [[x1, y0, z1], [x1, y1, z1], [x1, y1, h1], [x1, y0, h1]],
    ] {
        b.quad(MaterialSlot::Roof, p, s.face_uvs(p));
    }
}
