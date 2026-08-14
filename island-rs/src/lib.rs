//! Deterministic procedural island generation.
//!
//! The crate keeps terrain generation independent from its CLI and C ABI. An
//! [`Island`] owns all generated data; rendering and export methods borrow it.

mod coast;
mod ffi;
mod math;
mod mesh;
mod mesh_clipper;
mod noise;
mod png;
mod profiling;
mod raster;
mod river_emitters;
mod rivers;
mod rng;
mod terrain;

pub use glam::{Vec2, Vec3};
pub use math::BoundingBox;
pub use mesh::{Adjacency, Mesh};
pub use png::write_png;
pub use raster::Raster;
pub use river_emitters::{RiverEmitter, extract_river_emitters};
pub use rivers::{River, RiverNode};
pub use terrain::{Decoration, Decorations, Island, IslandOptions, SurfaceMaps, Terrain};
