//! Bevy compilation and procedural tree material pipelines.

pub(crate) mod bark_material;
pub(crate) mod compiler;
pub(crate) mod impostor_material;
pub(crate) mod leaf_material;

pub use bark_material::{BarkMaterial, BarkMaterialPlugin};
pub use compiler::{
    CompiledTreeImpostor, CompiledTreePart, CompiledTreePrototype, compile_botanical_impostor,
    compile_static_impostor_with_recipe, compile_static_middle_prototype_with_recipe,
    compile_static_prototype, compile_static_prototype_with_recipe,
};
pub use impostor_material::{ImpostorMaterial, ImpostorMaterialPlugin};
pub use leaf_material::{LeafMaterial, LeafMaterialPlugin};
