//! Deterministic building recipes, structural geometry, collision, and exterior baking.
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]
pub mod bake;
pub mod collision;
pub mod document;
pub mod export;
pub mod geometry;
#[cfg(feature = "bevy")]
pub mod interaction;
pub mod layout;
pub mod lod;
pub mod materials;
pub mod model;
pub mod recipe;
#[cfg(feature = "bevy")]
pub mod render;
#[cfg(feature = "studio")]
pub mod studio;
pub mod validation;
pub use model::*;
pub use recipe::*;
pub use validation::validate as validate_recipe;
pub use validation::{BuildingError, Diagnostic, validate};

/// Resolve and generate one self-contained prototype with the built-in material set.
pub fn generate(recipe: &BuildingRecipe) -> Result<BuildingPrototype, BuildingError> {
    generate_with_base(recipe, std::path::Path::new("."))
}
/// Resolve material recipe references relative to the owning building document.
pub fn generate_with_base(
    recipe: &BuildingRecipe,
    base: &std::path::Path,
) -> Result<BuildingPrototype, BuildingError> {
    validation::validate_structure(recipe)?;
    let layout = layout::resolve(recipe);
    let (near, doors, colliders) = geometry::generate(recipe);
    validation::validate_door_swings(&doors, &colliders)?;
    let materials = materials::generate(recipe, base)?;
    let middle = lod::middle(recipe);
    let distant = lod::distant(recipe);
    let atlas = bake::bake(recipe, &materials);
    let hash = materials::prototype_hash(recipe, &materials)?;
    Ok(BuildingPrototype {
        recipe: recipe.clone(),
        layout,
        lods: [near, middle, distant],
        doors,
        colliders,
        stair_colliders: geometry::foundation::stair_colliders(recipe),
        atlas,
        materials,
        hash,
    })
}
