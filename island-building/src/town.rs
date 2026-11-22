//! Deterministic, terrain-aware coastal settlements in native Z-up coordinates.
//!
//! Inputs are borrowed throughout placement. Settlements share generated shells
//! for equal building recipes; terrain-fitted supports stay with each instance.
mod input;
mod placement;
mod prototypes;
mod rivers;
mod roads;
mod settlements;
mod wheat;

pub use input::*;
pub(crate) use settlements::capital_mouth;
pub(crate) use settlements::generate as generate_with_piers;

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
    #[serde(default)]
    pub entry_id: String,
    #[serde(default)]
    pub settlement_index: usize,
    /// World floor origin, native Z-up metres.
    pub position: [f32; 3],
    /// Radians about native +Z.
    pub yaw: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontage: Option<TownFrontage>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TownFrontage {
    pub door_id: String,
    /// Native Z-up metres at the walking surface of the adjoining street.
    pub street_edge: [f32; 3],
}

/// Graded road ends on opposing riverbanks, in native Z-up metres.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BridgeCandidate {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub width: f32,
    /// Unit directions of travel from `from` to `to` at the respective road ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_heading: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_heading: Option<[f32; 2]>,
}

/// Outward cart-track attachment on a graded street end or the outer ring curb.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TownRoadExit {
    pub settlement_index: usize,
    /// Point on the paved surface, native Z-up metres.
    pub position: [f32; 3],
    /// Unit vector pointing out of the settlement in native XY.
    pub direction: [f32; 2],
    pub half_width: f32,
    /// A departure from the outer ring, alongside any cross-street terminals.
    #[serde(default)]
    pub curb: bool,
}

/// Oriented horizontal rectangle in native XY metres.
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
    /// The terminal line for a distinct outlet, or the source of the longest
    /// head-to-sea route in legacy single-town mode.
    pub river_index: usize,
    pub position: [f32; 2],
    pub upstream: [f32; 2],
    pub river_length: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TownSettlementKind {
    Capital,
    Village,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TownSettlement {
    pub kind: TownSettlementKind,
    pub river_index: usize,
    pub mouth: [f32; 2],
    pub centre: Option<[f32; 2]>,
    pub building_count: usize,
    pub skipped_count: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TownOutput {
    pub tile_seed: u64,
    pub mouth: Option<TownMouth>,
    pub centre: Option<[f32; 2]>,
    #[serde(default)]
    pub settlements: Vec<TownSettlement>,
    pub prototypes: Vec<TownPrototype>,
    pub instances: Vec<TownInstance>,
    /// Combined world-space, Z-up mesh, all using the shared foundation material.
    pub supports: BuildingMesh,
    /// Original per-building local positions for support stone weathering.
    pub supports_grub_positions: Vec<[f32; 3]>,
    pub support_material: Option<MaterialTexture>,
    pub support_colliders: Vec<BoxCollider>,
    pub stair_colliders: Vec<BoxCollider>,
    /// Fitted exterior stair approaches and the foundation footprints they avoid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stair_feet: Vec<motu::TownStairFoot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footpath_blockers: Vec<ClearanceRect>,
    pub clearance: Vec<ClearanceRect>,
    /// Vacant plots awaiting final terrain fitting. Local X follows the wheat rows.
    #[serde(default)]
    pub wheat_fields: Vec<ClearanceRect>,
    /// Accepted building, support and entrance reservations for cart-track routing.
    #[serde(default)]
    pub routing_blockers: Vec<ClearanceRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roads: Option<motu::roads::RoadNetwork>,
    #[serde(default)]
    pub road_exits: Vec<TownRoadExit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridge_candidates: Vec<BridgeCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pier_layout: Option<motu::piers::PierLayout>,
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
    settlements::generate(recipe, None, input, &[], base, cancelled)
}

/// Plan a capital at the middlemost distinct sea outlet and a village at every other outlet.
pub fn generate_settlements(
    capital: &TownRecipe,
    village: &TownRecipe,
    input: &TownInput,
) -> Result<TownOutput, TownError> {
    generate_settlements_with_base(capital, village, input, Path::new("."), &|| false)
}

/// Resolve both catalogues using the same material base and cancellation callback.
pub fn generate_settlements_with_base(
    capital: &TownRecipe,
    village: &TownRecipe,
    input: &TownInput,
    base: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<TownOutput, TownError> {
    settlements::generate(capital, Some(village), input, &[], base, cancelled)
}
