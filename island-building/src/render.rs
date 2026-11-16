//! Shared Bevy assets. Geometry remains in native metres until this boundary.

mod arrays;
mod compiler;
pub use arrays::{BuildingArrayMaterial, BuildingArrayPlugin, compile_building_with_arrays};
mod grub;
pub use grub::{BuildingGrubPlugin, GrubMaterial};
mod spawn;

pub use compiler::{
    CompiledBuildingPart, CompiledBuildingPrototype, compile_building, compile_building_with_grub,
};
pub use spawn::spawn_building;

use bevy::prelude::Vec3;

/// Right-handed native Z-up metres to right-handed Bevy Y-up metres.
#[must_use]
pub fn native_to_bevy(point: [f32; 3]) -> Vec3 {
    Vec3::new(point[0], point[2], -point[1])
}

/// Inverse of [`native_to_bevy`].
#[must_use]
pub fn bevy_to_native(point: Vec3) -> [f32; 3] {
    [point.x, -point.z, point.y]
}

#[cfg(feature = "studio")]
pub(crate) use compiler::above_ground_bounds;
