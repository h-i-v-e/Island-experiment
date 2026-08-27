//! Renderer-neutral procedural tree generation and botanical data.

pub(crate) mod generator;
pub(crate) mod impostor;
pub(crate) mod model;
mod random;

pub use generator::generate_botanical_prototype;
pub use impostor::{BotanicalImpostor, generate_botanical_impostor};
pub use model::{
    Axis, AxisGraph, BarkVertex, BotanicalPrototype, BotanicalRecipe, BotanicalTexture,
    FOLIAGE_PAD_ARCHETYPE_COUNT, FoliagePad, LEAF_ARCHETYPE_COUNT, LeafOrgan,
    SHOOT_TIP_ARCHETYPE_COUNT, ShootTipOrgan, ShootTipState,
};
