//! Placement state, explicit observers, collision queries and kinematic doors.
//! This plugin owns no camera, lighting, weather or movement controller.

mod collision;
mod doors;
mod lod;
mod state;
#[cfg(test)]
mod tests;

use crate::render::CompiledBuildingPrototype;
use bevy::prelude::*;
pub use state::{BuildingSceneState, DoorState, LodMode};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Component, Debug)]
pub struct BuildingInstance {
    pub prototype: Arc<CompiledBuildingPrototype>,
    pub placement_id: String,
    pub lod_mode: LodMode,
    pub current_lod: usize,
    /// Angles are offsets from each descriptor's canonical closed pose, radians.
    pub doors: BTreeMap<String, DoorState>,
}

impl BuildingInstance {
    #[must_use]
    pub fn new(prototype: Arc<CompiledBuildingPrototype>, placement_id: String) -> Self {
        let doors = prototype
            .source
            .doors
            .iter()
            .map(|door| (door.id.clone(), DoorState::default()))
            .collect();
        Self {
            prototype,
            placement_id,
            lod_mode: LodMode::Auto,
            current_lod: 0,
            doors,
        }
    }
}

/// An observer's transform is its feet position. All dimensions are world units.
/// Attach explicitly to a player or required inspection camera; ordinary cameras
/// do not implicitly acquire residency or door interaction rights.
#[derive(Component, Clone, Debug)]
pub struct BuildingObserver {
    pub view_direction: Vec3,
    pub eye_height: f32,
    pub radius: f32,
    pub height: f32,
    pub vertical_fov_radians: f32,
    pub viewport_height: f32,
    pub interaction_enabled: bool,
    /// A one-frame interaction request, consumed by the runtime (also supports E).
    pub interact: bool,
}

impl Default for BuildingObserver {
    fn default() -> Self {
        Self {
            view_direction: Vec3::NEG_Z,
            eye_height: 1.65,
            radius: 0.28,
            height: 1.8,
            vertical_fov_radians: 60.0_f32.to_radians(),
            viewport_height: 1080.0,
            interaction_enabled: true,
            interact: false,
        }
    }
}

/// Explicit occupancy survives a detached camera moving away from a building.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuildingOccupant {
    pub building: Entity,
    pub radius: f32,
    pub height: f32,
}

/// Optional nearby dynamic blocker, represented by a vertical capsule at feet.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuildingBlocker {
    pub radius: f32,
    pub height: f32,
}

#[derive(Component)]
pub struct BuildingLodRoot {
    pub building: Entity,
    pub level: usize,
}

#[derive(Component)]
pub struct DoorLeaf {
    pub building: Entity,
    pub door_id: String,
}

#[derive(Resource, Clone, Debug)]
pub struct BuildingRuntimeSettings {
    /// LOD0 / LOD1 transition thresholds, projected diameter in pixels.
    pub near_pixels: f32,
    pub far_pixels: f32,
    pub hysteresis: f32,
    /// Native metres, independent of a planet's normalized world scale.
    pub opening_inspection_metres: f32,
    pub interaction_metres: f32,
    pub door_speed_radians: f32,
}

impl Default for BuildingRuntimeSettings {
    fn default() -> Self {
        Self {
            near_pixels: 260.0,
            far_pixels: 80.0,
            hysteresis: 0.15,
            opening_inspection_metres: 3.0,
            interaction_metres: 3.0,
            door_speed_radians: 1.8,
        }
    }
}

pub struct BuildingRuntimePlugin;
/// Schedule observer/occupant synchronization before this set. Consumers may
/// also query the resulting door/collision poses after the complete set.
#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BuildingRuntimeSet;

impl Plugin for BuildingRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingRuntimeSettings>().add_systems(
            Update,
            (
                doors::interact_with_doors,
                doors::animate_doors,
                lod::select_lods,
                lod::sync_lod_roots,
                doors::sync_door_leaves,
            )
                .chain()
                .in_set(BuildingRuntimeSet),
        );
    }
}
