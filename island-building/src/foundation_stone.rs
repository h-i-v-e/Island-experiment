//! One portable default for foundation, entrance-stair and tread surfaces.
use crate::{BuildingRecipe, GrubFilter};

pub const MATERIAL_ID: &str = "foundation_stone";
pub const TEXTURE_SIZE: u32 = 512;
pub const SEED: u64 = 64013;
pub const COLOUR: [f32; 3] = [0.24, 0.25, 0.245];

#[must_use]
pub fn uses_shared_default(recipe: &BuildingRecipe) -> bool {
    recipe.materials.foundation_recipe.is_none()
}

/// Independent of building seeds and the optional weathering on other surfaces.
#[must_use]
pub fn grub_filter() -> GrubFilter {
    GrubFilter {
        enabled: true,
        ..GrubFilter::default()
    }
}
