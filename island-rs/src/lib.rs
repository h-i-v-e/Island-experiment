//! Deterministic procedural island generation.
//!
//! The crate keeps terrain generation independent from its CLI and C ABI. An
//! [`Island`] owns all generated data; rendering and export methods borrow it.

mod coast;
mod ffi;
mod math;
mod mesh;
mod noise;
mod png;
mod raster;
mod rivers;
mod rng;
mod terrain;

pub use glam::{Vec2, Vec3};
pub use math::BoundingBox;
pub use mesh::{Adjacency, Mesh};
pub use png::write_png;
pub use raster::Raster;
pub use rivers::{River, RiverNode};
pub use terrain::{Decoration, Decorations, Island, IslandOptions, SurfaceMaps, Terrain};
