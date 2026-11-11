use super::BuildingInstance;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LodMode {
    #[default]
    Auto,
    Lod0,
    Lod1,
    Lod2,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct DoorState {
    pub angle: f32,
    pub target: f32,
}

/// Stable placement identity and door IDs; independent of the shared prototype hash.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BuildingSceneState {
    pub version: u32,
    pub placement_id: String,
    pub doors: BTreeMap<String, DoorState>,
    #[serde(default = "stairs_visible_default")]
    pub stairs_visible: bool,
}

const fn stairs_visible_default() -> bool {
    true
}

impl BuildingInstance {
    pub fn toggle_door(&mut self, id: &str) -> bool {
        let Some(descriptor) = self
            .prototype
            .source
            .doors
            .iter()
            .find(|door| door.id == id)
        else {
            return false;
        };
        let Some(state) = self.doors.get_mut(id) else {
            return false;
        };
        state.target = if state.target.abs() > 0.01 {
            0.0
        } else {
            descriptor.swing_angle
        };
        true
    }

    #[must_use]
    pub fn snapshot(&self) -> BuildingSceneState {
        BuildingSceneState {
            version: 1,
            placement_id: self.placement_id.clone(),
            doors: self.doors.clone(),
            stairs_visible: self.stairs_visible,
        }
    }

    /// Unknown/removed doors are ignored; new doors start closed. Invalid states
    /// or a mismatched placement/version are rejected without modifying state.
    pub fn restore_state(&mut self, saved: &BuildingSceneState) -> bool {
        if saved.version != 1
            || saved.placement_id != self.placement_id
            || saved
                .doors
                .values()
                .any(|state| !state.angle.is_finite() || !state.target.is_finite())
        {
            return false;
        }
        self.stairs_visible = saved.stairs_visible;
        for descriptor in &self.prototype.source.doors {
            let mut state = saved.doors.get(&descriptor.id).copied().unwrap_or_default();
            let min = descriptor.swing_angle.min(0.0);
            let max = descriptor.swing_angle.max(0.0);
            state.angle = state.angle.clamp(min, max);
            state.target = state.target.clamp(min, max);
            self.doors.insert(descriptor.id.clone(), state);
        }
        true
    }
}
