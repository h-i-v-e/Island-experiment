//! Building-local weathering settings. Base maps remain lighting-free and reusable.
use serde::{Deserialize, Serialize};

pub const SHADER_SOURCE: &str = include_str!("render/shaders/grub.wgsl");

/// Foundation surfaces share a fixed enabled profile; other exteriors retain
/// their authored building-specific weathering settings.
#[must_use]
pub fn material_filter(
    recipe: &crate::BuildingRecipe,
    slot: crate::MaterialSlot,
) -> Option<(GrubFilter, u64)> {
    if slot.is_foundation() {
        Some((
            crate::foundation_stone::grub_filter(),
            crate::foundation_stone::SEED,
        ))
    } else {
        (recipe.grub_filter.enabled && exterior_material(recipe, slot))
            .then_some((recipe.grub_filter, recipe.seed))
    }
}

/// Only dedicated outdoor groups are weathered. Mixed geometry and the distant
/// facade atlas stay clean so weathering cannot leak into timber or interiors.
#[must_use]
pub fn exterior_material(recipe: &crate::BuildingRecipe, slot: crate::MaterialSlot) -> bool {
    use crate::{MaterialSlot, RoofCovering, WallStyle};
    match slot {
        MaterialSlot::Foundation | MaterialSlot::Stairs | MaterialSlot::StairTreads => true,
        MaterialSlot::Wall => {
            recipe.church.is_some() || matches!(recipe.walls.style, WallStyle::WattleAndDaub)
        }
        MaterialSlot::Roof
        | MaterialSlot::ThatchEnd
        | MaterialSlot::ThatchShell1
        | MaterialSlot::ThatchShell2
        | MaterialSlot::ThatchShell3
        | MaterialSlot::ThatchShell4 => matches!(recipe.roof.covering, RoofCovering::Thatch),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GrubFilter {
    pub enabled: bool,
    pub grime: f32,
    pub moss: f32,
    /// Metres over which the floor-relative influence halves its odds.
    pub height_falloff: f32,
    /// Broad noise scale in building-local metres.
    pub patch_size: f32,
    pub fine_patch_size: f32,
    /// Contribution of fine noise to the combined mask; the rest is broad noise.
    pub fine_mix: f32,
    pub dirt_colour: [f32; 3],
    pub moss_colour: [f32; 3],
}

impl Default for GrubFilter {
    fn default() -> Self {
        Self {
            enabled: false,
            grime: 1.,
            moss: 0.7,
            height_falloff: 1.5,
            patch_size: 1.25,
            fine_patch_size: 0.18,
            fine_mix: 0.35,
            dirt_colour: [0.075, 0.055, 0.035],
            moss_colour: [0.07, 0.115, 0.025],
        }
    }
}

impl GrubFilter {
    #[must_use]
    pub fn height_influence(&self, height: f32) -> f32 {
        1. / (1. + (height / self.height_falloff).exp2())
    }

    /// Five aligned vec4 values, shared by the Bevy material and portable shader.
    #[must_use]
    pub fn uniform_vectors(&self, seed: u64) -> [[f32; 4]; 5] {
        let active = f32::from(u8::from(self.enabled));
        let folded = seed ^ (seed >> 32) ^ (seed >> 24);
        let mut offset = [0.; 4];
        for (i, value) in offset.iter_mut().take(3).enumerate() {
            *value = f32::from(u8::try_from((folded >> (i * 8)) & 255).unwrap());
        }
        [
            [
                self.grime * active,
                self.moss * active,
                self.height_falloff,
                self.patch_size,
            ],
            [
                self.dirt_colour[0],
                self.dirt_colour[1],
                self.dirt_colour[2],
                1.,
            ],
            [
                self.moss_colour[0],
                self.moss_colour[1],
                self.moss_colour[2],
                1.,
            ],
            offset,
            [self.fine_patch_size, self.fine_mix, 0., 0.],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foundation_profile_is_enabled_and_shared_without_weathering_the_hearth() {
        let mut a = crate::BuildingRecipe::hut();
        let mut b = crate::BuildingRecipe::gothic_church();
        a.grub_filter.enabled = false;
        b.seed = 987_654;
        b.grub_filter.height_falloff = 9.;
        for slot in [
            crate::MaterialSlot::Foundation,
            crate::MaterialSlot::Stairs,
            crate::MaterialSlot::StairTreads,
        ] {
            let first = material_filter(&a, slot).unwrap();
            assert!(first.0.enabled);
            assert_eq!(first, material_filter(&b, slot).unwrap());
        }
        assert!(material_filter(&a, crate::MaterialSlot::StoneSlab).is_none());
        assert_eq!(
            material_filter(&b, crate::MaterialSlot::Wall),
            Some((b.grub_filter, b.seed))
        );
    }

    #[test]
    fn weathering_is_limited_to_the_requested_dedicated_outdoor_groups() {
        use crate::{MaterialSlot, RoofCovering, WallStyle};
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.walls.style = WallStyle::WattleAndDaub;
        recipe.roof.covering = RoofCovering::Thatch;
        for slot in [
            MaterialSlot::Wall,
            MaterialSlot::Roof,
            MaterialSlot::ThatchEnd,
            MaterialSlot::Foundation,
            MaterialSlot::Stairs,
            MaterialSlot::StairTreads,
            MaterialSlot::ThatchShell1,
            MaterialSlot::ThatchShell4,
        ] {
            assert!(exterior_material(&recipe, slot), "{slot:?}");
        }
        for slot in [
            MaterialSlot::Floor,
            MaterialSlot::Ceiling,
            MaterialSlot::Interior,
            MaterialSlot::Timber,
            MaterialSlot::Trim,
            MaterialSlot::Door,
            MaterialSlot::Chimney,
            MaterialSlot::Firebox,
            MaterialSlot::StoneSlab,
            MaterialSlot::Glass,
            MaterialSlot::Hardware,
            MaterialSlot::WindowProxy,
            MaterialSlot::Atlas,
        ] {
            assert!(!exterior_material(&recipe, slot), "{slot:?}");
        }
        recipe.walls.style = WallStyle::Plain;
        recipe.roof.covering = RoofCovering::Slate;
        assert!(!exterior_material(&recipe, MaterialSlot::Wall));
        assert!(!exterior_material(&recipe, MaterialSlot::Roof));
    }

    #[test]
    fn influence_is_half_at_floor_and_increases_monotonically_downward() {
        let filter = GrubFilter::default();
        assert!((filter.height_influence(0.) - 0.5).abs() < f32::EPSILON);
        let values = [-12., -3., -1., 0., 1., 3., 12.].map(|h| filter.height_influence(h));
        assert!(values.windows(2).all(|pair| pair[0] > pair[1]));
        assert!(values[0] > 0.99 && values[6] < 0.01);
    }

    #[test]
    fn shared_shader_parses_and_validates_without_the_engine_adapter() {
        let source = SHADER_SOURCE
            .lines()
            .filter(|line| !line.starts_with("#define_import_path"))
            .collect::<Vec<_>>()
            .join("\n");
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
