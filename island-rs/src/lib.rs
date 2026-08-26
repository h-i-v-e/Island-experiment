//! Deterministic procedural island generation.
//!
//! The crate keeps terrain generation independent from its CLI and C ABI. An
//! [`Island`] owns all generated data; rendering and export methods borrow it.

#![recursion_limit = "512"]

mod clustered_foliage;
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
mod river_emitters;
mod rivers;
mod rng;
mod sea_mask;
mod terrain;
mod trees;

pub use forest::ForestOptions;
pub use glam::{Vec2, Vec3, Vec4};
pub use math::BoundingBox;
pub use mesh::{Adjacency, Mesh};
pub use png::write_png;
pub use procedural_textures::{
    LayerPreviewMaps, MaterialEvaluation, OutputManifest, OutputOptions, OutputProfile,
    PreviewMaps, PreviewSettings, PreviewTimings, TextureError, TextureRecipe, TextureSet,
    evaluate_material, generate_preview, generate_texture_set, layer_preview_maps,
    property_metadata, texture_set_from_evaluation, write_texture_set,
};
pub use raster::Raster;
pub use river_emitters::{RiverEmitter, extract_river_emitters};
pub use rivers::{River, RiverNode};
pub use sea_mask::SeaMask;
pub use terrain::{
    Decoration, Decorations, GenerationMethod, Island, IslandOptions, SurfaceMaps, Terrain,
};
pub use trees::{TreeMeshes, generate_tree};

/// Width represented by one normalized island coordinate in Unity.
pub const ISLAND_WORLD_METRES: f32 = 2_000.0;
