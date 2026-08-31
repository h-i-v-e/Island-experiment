//! Deterministic procedural island generation.
//!
//! The crate keeps terrain generation independent from its CLI and C ABI. An
//! [`Island`] owns all generated data; rendering and export methods borrow it.

#![recursion_limit = "512"]

mod clouds;
mod clustered_foliage;
mod ferns;
mod ffi;
mod forest;
mod geology;
mod math;
mod mesh;
mod mesh_clipper;
mod noise;
mod png;
pub mod procedural_textures;
mod profiling;
mod raster;
mod reeds;
mod rivers;
mod rng;
mod sea_mask;
mod sky;
mod terrain;
mod trees;

pub use clouds::{CloudWeatherError, CloudWeatherMap, generate_cloud_weather_map};
pub use ferns::FernOptions;
pub use forest::ForestOptions;
pub use glam::{Vec2, Vec3, Vec4};
pub use math::BoundingBox;
pub use mesh::{Adjacency, Mesh};
pub use png::write_png;
pub use procedural_textures::{
    IslandMaterialKind, IslandMaterialTextures, LayerPreviewMaps, LinearRgb, MaterialBakeIdentity,
    MaterialEvaluation, MaterialSelection, NormalConvention, OutputManifest, OutputOptions,
    OutputProfile, ParameterDefinition, ParameterValue, PreviewMaps, PreviewSettings,
    PreviewTimings, RecipeParameterValues, RuntimeMaterialBakeError, RuntimeMaterialBakeOptions,
    RuntimeMaterialInputs, TextureError, TextureRecipe, TextureSet, bake_island_materials,
    evaluate_material, evaluate_material_with_parameters, generate_preview,
    generate_preview_with_parameters, generate_texture_set, generate_texture_set_with_parameters,
    layer_preview_maps, property_metadata, resolve_texture_recipe, texture_set_from_evaluation,
    write_texture_set,
};
pub use raster::Raster;
pub use reeds::ReedOptions;
pub use rivers::{River, RiverNode};
pub use sea_mask::SeaMask;
pub use sky::{SKY_DOME_RADIUS, generate_sky_dome};
pub use terrain::{
    Decoration, Decorations, GenerationMethod, Island, IslandOptions, SurfaceMaps, Terrain,
};
pub use trees::{TreeMeshes, generate_tree};

/// Width represented by one normalized island coordinate in Unity.
pub const ISLAND_WORLD_METRES: f32 = 2_000.0;
