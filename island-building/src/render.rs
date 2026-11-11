//! Shared Bevy assets. Geometry remains in native metres until this boundary.

mod compiler;
mod spawn;

pub use compiler::{CompiledBuildingPart, CompiledBuildingPrototype, compile_building};
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

pub(crate) use compiler::above_ground_bounds;
