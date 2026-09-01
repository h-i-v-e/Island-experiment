//! Deterministic generation and Bevy rendering for New Zealand native vegetation.
//!
//! The library owns the recipe, organ graph, generated meshes and textures,
//! and static Bevy compilation. The sibling `tree-lab` binary is only a visual
//! review shell, while landscape placement and culling stay with the island
//! renderer.

mod botany;
mod render;

pub use botany::{
    Axis, AxisGraph, BarkVertex, BotanicalImpostor, BotanicalPrototype, BotanicalRecipe,
    BotanicalSpecies, BotanicalTexture, FOLIAGE_PAD_ARCHETYPE_COUNT, FoliagePad,
    LEAF_ARCHETYPE_COUNT, LeafOrgan, REPRODUCTIVE_ARCHETYPE_COUNT, ReproductiveOrgan,
    ReproductiveState, SHOOT_TIP_ARCHETYPE_COUNT, ShootTipOrgan, ShootTipState,
    generate_botanical_impostor, generate_botanical_prototype, generate_nikau_frond_prototype,
};
pub use render::{
    BarkMaterial, BarkMaterialPlugin, CompiledTreeImpostor, CompiledTreePart,
    CompiledTreePrototype, ImpostorMaterial, ImpostorMaterialPlugin, LeafMaterial,
    LeafMaterialPlugin, compile_botanical_impostor, compile_static_impostor_with_recipe,
    compile_static_middle_prototype_with_recipe, compile_static_prototype,
    compile_static_prototype_with_recipe,
};
