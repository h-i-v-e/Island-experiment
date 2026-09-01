//! Renderer-neutral procedural tree generation and botanical data.

pub(crate) mod generator;
mod harakeke;
pub(crate) mod impostor;
mod kauri;
mod manuka;
pub(crate) mod model;
mod nikau;
mod random;
mod rimu;

pub use generator::generate_botanical_prototype;
pub use impostor::{BotanicalImpostor, generate_botanical_impostor};
pub use model::{
    Axis, AxisGraph, BarkVertex, BotanicalPrototype, BotanicalRecipe, BotanicalSpecies,
    BotanicalTexture, FOLIAGE_PAD_ARCHETYPE_COUNT, FoliagePad, LEAF_ARCHETYPE_COUNT, LeafOrgan,
    REPRODUCTIVE_ARCHETYPE_COUNT, ReproductiveOrgan, ReproductiveState, SHOOT_TIP_ARCHETYPE_COUNT,
    ShootTipOrgan, ShootTipState,
};

/// Builds one mature nīkau frond as a standalone renderer-neutral prototype.
///
/// # Errors
///
/// Returns an error when the recipe is invalid or does not select nīkau.
pub fn generate_nikau_frond_prototype(
    seed: u64,
    recipe: BotanicalRecipe,
) -> Result<BotanicalPrototype, String> {
    let recipe = recipe.validate()?;
    if recipe.species != BotanicalSpecies::Nikau {
        return Err("standalone frond generation requires the nīkau species".into());
    }
    nikau::generate_nikau_frond_prototype(seed, recipe)
}
