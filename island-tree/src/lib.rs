//! Deterministic pōhutukawa generation and Bevy rendering.
//!
//! The library owns the recipe, organ graph, generated meshes and textures,
//! and static Bevy compilation. The sibling `tree-lab` binary is only a visual
//! review shell, while landscape placement and culling stay with the island
//! renderer.

mod bark_material;
mod generator;
mod leaf_material;
mod model;
mod random;
mod renderer;

pub use bark_material::{BarkMaterial, BarkMaterialPlugin};
pub use generator::generate_botanical_prototype;
pub use leaf_material::{LeafMaterial, LeafMaterialPlugin};
pub use model::{
    Axis, AxisGraph, BarkVertex, BotanicalPrototype, BotanicalRecipe, BotanicalTexture,
    FOLIAGE_PAD_ARCHETYPE_COUNT, FoliagePad, LEAF_ARCHETYPE_COUNT, LeafOrgan,
    SHOOT_TIP_ARCHETYPE_COUNT, ShootTipOrgan, ShootTipState,
};
pub use renderer::{
    CompiledTreePart, CompiledTreePrototype, compile_static_middle_prototype_with_recipe,
    compile_static_prototype, compile_static_prototype_with_recipe,
};
