//! Closed-leaf charts retain the selected joinery and oriented material grain.
use super::{Sample, plain, texture};
use crate::{BuildingRecipe, DoorStyle, HingeSide, MaterialSlot, MaterialTexture, OpeningRecipe};

pub(super) fn sample(
    recipe: &BuildingRecipe,
    materials: &[MaterialTexture],
    opening: &OpeningRecipe,
    point: [f32; 2],
) -> Sample {
    let panels = (opening.door_style == DoorStyle::FrameAndPanel)
        .then(|| crate::geometry::door_panels::sample(opening.width, opening.height, point));
    let uv = panels.as_ref().map_or(point, |p| p.uv);
    let mut surface = if recipe.materials.door_recipe.is_some() {
        texture(
            materials,
            MaterialSlot::Door,
            uv[0],
            uv[1],
            recipe.materials.door,
        )
    } else {
        let groove =
            panels.is_none() && ((point[0] - opening.width * 0.5) * 9.).rem_euclid(1.) < 0.045;
        plain(
            recipe
                .materials
                .door
                .map(|v| v * if groove { 0.7 } else { 1. }),
            0.,
            205,
        )
    };
    surface.height = -0.012;
    if let Some(panel) = panels {
        if panel.horizontal {
            surface.normal.swap(0, 1);
        }
        surface.height -= panel.recess;
        surface.ao =
            (f32::from(surface.ao) * (1. - 0.2 * (panel.recess / 0.013).clamp(0., 1.))) as u8;
    }
    let knob_x = if opening.hinge == HingeSide::Left {
        opening.width - 0.078
    } else {
        0.078
    };
    let knob_z = 1.02_f32.min((opening.height - 0.009) * 0.55) + 0.006;
    if (point[0] - knob_x).hypot(point[1] - knob_z) < 0.025 {
        surface = plain([0.48, 0.44, 0.34], 0.02, 61);
        surface.metallic = 230;
    }
    surface
}
