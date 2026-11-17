//! Deterministic, terrain-aware coastal settlements in native Z-up coordinates.
//!
//! Inputs are borrowed throughout placement. Every catalogue entry owns at most one
//! generated shell, shared by all its instances; fitted supports are combined once.
mod input;
mod placement;
mod rivers;

pub use input::*;

use crate::{BuildingMesh, BuildingPrototype, MaterialTexture, collision::BoxCollider};
use serde::{Deserialize, Serialize};
use std::{fmt, path::Path};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TownPrototype {
    pub entry_id: String,
    pub building: BuildingPrototype,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TownInstance {
    pub prototype_index: usize,
    /// World floor origin, native Z-up metres.
    pub position: [f32; 3],
    /// Radians about native +Z.
    pub yaw: f32,
}

/// Horizontal rectangle for removal of trees, rocks and other terrain decorations.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClearanceRect {
    pub centre: [f32; 2],
    pub half_extents: [f32; 2],
    pub yaw: f32,
}

impl ClearanceRect {
    #[must_use]
    pub fn contains(&self, point: [f32; 2], margin: f32) -> bool {
        let p = self.local(point);
        (0..2).all(|i| p[i].abs() <= self.half_extents[i] + margin)
    }

    pub(crate) fn local(&self, point: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.yaw.sin_cos();
        let delta = [point[0] - self.centre[0], point[1] - self.centre[1]];
        [c * delta[0] + s * delta[1], -s * delta[0] + c * delta[1]]
    }

    pub(crate) fn world(&self, point: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.yaw.sin_cos();
        [
            self.centre[0] + c * point[0] - s * point[1],
            self.centre[1] + s * point[0] + c * point[1],
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TownMouth {
    /// The source branch of the longest head-to-sea route. Shared tails count once.
    pub river_index: usize,
    pub position: [f32; 2],
    pub upstream: [f32; 2],
    pub river_length: f32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TownOutput {
    pub tile_seed: u64,
    pub mouth: Option<TownMouth>,
    pub centre: Option<[f32; 2]>,
    pub prototypes: Vec<TownPrototype>,
    pub instances: Vec<TownInstance>,
    /// Combined world-space, Z-up mesh, all using the shared foundation material.
    pub supports: BuildingMesh,
    /// Original per-building local positions for support stone weathering.
    pub supports_grub_positions: Vec<[f32; 3]>,
    pub support_material: Option<MaterialTexture>,
    pub support_colliders: Vec<BoxCollider>,
    pub stair_colliders: Vec<BoxCollider>,
    pub clearance: Vec<ClearanceRect>,
    /// Requested copies can be skipped when no safe placement exists.
    pub skipped_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TownError {
    InvalidInput(String),
    Building { entry: String, reason: String },
    Support(String),
    Cancelled,
}

impl fmt::Display for TownError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid town input: {message}"),
            Self::Building { entry, reason } => {
                write!(f, "town catalogue entry '{entry}': {reason}")
            }
            Self::Support(message) => write!(f, "town terrain support: {message}"),
            Self::Cancelled => f.write_str("town generation cancelled"),
        }
    }
}
impl std::error::Error for TownError {}

/// Generate using material references relative to the current directory.
pub fn generate_town(recipe: &TownRecipe, input: &TownInput) -> Result<TownOutput, TownError> {
    generate_town_with_base(recipe, input, Path::new("."), &|| false)
}

/// Bounded search with cooperative cancellation between attempts and prototype builds.
pub fn generate_town_with_base(
    recipe: &TownRecipe,
    input: &TownInput,
    base: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<TownOutput, TownError> {
    input::validate(recipe, input)?;
    if cancelled() {
        return Err(TownError::Cancelled);
    }
    let Some(mouth) = rivers::longest_mouth(input) else {
        return Ok(TownOutput {
            tile_seed: input.tile_seed,
            ..TownOutput::default()
        });
    };
    placement::generate(recipe, input, mouth, base, cancelled)
}
