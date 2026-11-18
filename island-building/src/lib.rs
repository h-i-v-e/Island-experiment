//! Deterministic building recipes, structural geometry, collision, and exterior baking.
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]
pub mod bake;
pub mod church;
pub mod collision;
pub use church::{ChurchLayout, ChurchRecipe, ChurchTowerStyle};
pub mod document;
pub mod export;
pub mod extensions;
pub mod foundation_stone;
pub use extensions::{ExtensionKind, ExtensionLayout, ExtensionRecipe};
pub(crate) mod finalize;
pub mod geometry;
pub mod grub;
pub use grub::GrubFilter;
#[cfg(feature = "bevy")]
pub mod interaction;
pub mod layout;
pub mod lod;
pub mod materials;
pub mod model;
mod native;
pub mod openings;
pub use openings::{OpeningDefaults, OpeningDimensions, OpeningGeneration, OpeningOverride};
pub mod recipe;
pub mod stairs;
pub mod terrain_support;
pub mod town;
pub use stairs::{
    StairAxis, StaircaseLayout, StaircaseRecipe, StairwellDoorway, StairwellEnclosure,
    StairwellLayout, StairwellSide,
};
#[cfg(feature = "bevy")]
pub mod render;
pub(crate) mod storeys;
#[cfg(feature = "studio")]
pub mod studio;
pub mod units;
pub mod validation;
mod window_panes;
pub use model::*;
pub use recipe::*;
pub use validation::validate as validate_recipe;
pub use validation::{BuildingError, Diagnostic, validate};
pub use window_panes::WindowPanes;

/// Resolve and generate one self-contained prototype with the built-in material set.
pub fn generate(recipe: &BuildingRecipe) -> Result<BuildingPrototype, BuildingError> {
    generate_with_base(recipe, std::path::Path::new("."))
}
/// Resolve material recipe references relative to the owning building document.
pub fn generate_with_base(
    recipe: &BuildingRecipe,
    base: &std::path::Path,
) -> Result<BuildingPrototype, BuildingError> {
    let resolved = openings::resolve(recipe)?;
    validation::validate_structure(resolved.as_ref())?;
    if resolved.church.is_some() {
        return church::generate(resolved.as_ref(), base);
    }
    if !resolved.extensions.is_empty() {
        return extensions::generate(resolved.as_ref(), base);
    }
    generate_standard(resolved, base)
}

pub(crate) fn generate_standard(
    mut resolved: std::borrow::Cow<'_, BuildingRecipe>,
    base: &std::path::Path,
) -> Result<BuildingPrototype, BuildingError> {
    let staircase = stairs::resolve_placement(resolved.as_ref(), base)?;
    if let Some(placement) = &staircase {
        openings::apply_entrance_offsets(&mut resolved, &placement.entrance_offsets);
        stairs::fit_windows(&mut resolved, &placement.layout);
    }
    let recipe = resolved.as_ref();
    validation::validate_structure(recipe)?;
    let staircase = staircase.map(|p| p.layout);
    let layout = layout::resolve(recipe, staircase);
    let roof_pitch = if recipe.roof.covering == RoofCovering::Slate {
        units::MaterialGrid::load(recipe, base, MaterialSlot::Roof)?.metres[0]
    } else {
        0.25
    };
    let mut structure = storeys::near(recipe, roof_pitch, layout.staircase.as_ref());
    validation::validate_door_swings(&structure.doors, &structure.colliders)?;
    let materials = materials::generate(recipe, base)?;
    let mut middle = lod::middle(recipe);
    let mut distant = lod::distant(recipe);
    for lod in [&mut structure.near, &mut middle, &mut distant] {
        finalize::lod(lod);
    }
    for door in &mut structure.doors {
        finalize::door(door);
    }
    let atlas = bake::bake(recipe, &materials);
    let hash = materials::prototype_hash(recipe, &materials)?;
    Ok(BuildingPrototype {
        recipe: recipe.clone(),
        layout,
        lods: [structure.near, middle, distant],
        doors: structure.doors,
        colliders: structure.colliders,
        support_colliders: structure.supports,
        stair_colliders: geometry::foundation::stair_colliders(recipe),
        atlas,
        materials,
        hash,
    })
}
