use super::TownError;
use crate::BuildingRecipe;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

pub const TOWN_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeightGrid {
    pub origin: [f32; 2],
    pub spacing: f32,
    pub width: usize,
    pub height: usize,
    /// Row-major Z heights, indexed by y * width + x.
    pub heights: Vec<f32>,
}
impl HeightGrid {
    /// Bilinear interpolation; outside-grid positions never extrapolate.
    #[must_use]
    // Dimensions are limited to 2^24 samples; indices remain exactly representable.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn sample(&self, x: f32, y: f32) -> Option<f32> {
        if self.width.checked_mul(self.height)? != self.heights.len()
            || self.heights.len() > 16_777_216
        {
            return None;
        }
        if self.width < 2 || self.height < 2 || !self.spacing.is_finite() || self.spacing <= 0. {
            return None;
        }
        let px = (x - self.origin[0]) / self.spacing;
        let py = (y - self.origin[1]) / self.spacing;
        if !px.is_finite()
            || !py.is_finite()
            || px < 0.
            || py < 0.
            || px > (self.width - 1) as f32
            || py > (self.height - 1) as f32
        {
            return None;
        }
        let ix = (px.floor() as usize).min(self.width - 2);
        let iy = (py.floor() as usize).min(self.height - 2);
        let fx = px - ix as f32;
        let fy = py - iy as f32;
        let at = |dx, dy| self.heights.get((iy + dy) * self.width + ix + dx).copied();
        let lower = at(0, 0)? * (1. - fx) + at(1, 0)? * fx;
        let upper = at(0, 1)? * (1. - fx) + at(1, 1)? * fx;
        let result = lower * (1. - fy) + upper * fy;
        result.is_finite().then_some(result)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiverPolyline {
    /// Source-to-mouth native XY positions. Tributaries may end on another line.
    pub points: Vec<[f32; 2]>,
    /// Full channel widths, one per point. Empty uses a conservative 3m width.
    #[serde(default)]
    pub widths: Vec<f32>,
    /// Optional authoritative downstream river index (for channel-edge joins).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<usize>,
}
impl RiverPolyline {
    pub(crate) fn width(&self, index: usize) -> f32 {
        self.widths.get(index).copied().unwrap_or(3.)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TownInput {
    pub tile_seed: u64,
    pub sea_level: f32,
    pub terrain: HeightGrid,
    pub rivers: Vec<RiverPolyline>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected_terrain: Vec<motu::TownProtectedTerrain>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TownPlacement {
    Centre,
    Other,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TownCatalogueEntry {
    pub id: String,
    pub building: BuildingRecipe,
    pub min_count: u32,
    pub max_count: u32,
    pub placement: TownPlacement,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TownRecipe {
    pub schema_version: u32,
    pub catalogue: Vec<TownCatalogueEntry>,
    #[serde(default)]
    pub settings: TownSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TownSettings {
    /// Furthest town site search from the selected mouth.
    pub site_search_radius: f32,
    /// Maximum placement radius when roads are disabled.
    pub town_radius: f32,
    pub building_gap: f32,
    pub river_margin: f32,
    pub clearance_margin: f32,
    pub approach_length: f32,
    pub max_slope_degrees: f32,
    /// Maximum actual masonry depth, including burial, across occupied plinth regions.
    pub max_support_height: f32,
    pub min_dry_height: f32,
    pub sample_spacing: f32,
    pub attempts_per_copy: u32,
    pub max_total_attempts: u32,
    /// Enable the terrain-graded street layout for this settlement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roads: Option<TownRoadSettings>,
}
impl Default for TownSettings {
    fn default() -> Self {
        Self {
            site_search_radius: 100.,
            town_radius: 85.,
            building_gap: 3.,
            river_margin: 3.,
            clearance_margin: 2.,
            approach_length: 3.,
            max_slope_degrees: 14.,
            max_support_height: 4.,
            min_dry_height: 0.35,
            sample_spacing: 0.75,
            attempts_per_copy: 96,
            max_total_attempts: 4096,
            roads: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TownRoadSettings {
    /// Legacy recipe field. The planner sizes road networks to the catalogue.
    #[serde(skip_serializing)]
    pub ring_count: u8,
    pub ring_spacing: f32,
    pub half_width: f32,
    pub access_half_width: f32,
    /// A combined network uses the capital's shoulder when its roads are enabled.
    pub shoulder: f32,
    /// Rise over horizontal run.
    pub max_grade: f32,
    /// Maximum terrain change in metres.
    pub max_cut: f32,
    /// Maximum ground-floor door distance from a road edge in metres.
    pub max_frontage_distance: f32,
}

impl Default for TownRoadSettings {
    fn default() -> Self {
        Self {
            ring_count: 2,
            ring_spacing: 24.,
            half_width: 2.,
            access_half_width: 3.,
            shoulder: 2.,
            max_grade: 0.22,
            max_cut: 3.,
            max_frontage_distance: 8.,
        }
    }
}

impl TownRoadSettings {
    fn valid(self) -> bool {
        [
            (self.ring_spacing, 12., 60.),
            (self.half_width, 1., 6.),
            (self.access_half_width, 1., 6.),
            (self.shoulder, 0.5, 5.),
            (self.max_grade, 0.03, 0.25),
            (self.max_cut, 0.1, 3.),
            (self.max_frontage_distance, 4., 12.),
        ]
        .into_iter()
        .all(|(value, lower, upper)| (lower..=upper).contains(&value))
    }
}

pub(super) fn validate_terrain(input: &TownInput) -> Result<(), TownError> {
    let fail = |message: &str| Err(TownError::InvalidInput(message.into()));
    let grid = &input.terrain;
    if grid.width < 2
        || grid.height < 2
        || grid.width.checked_mul(grid.height) != Some(grid.heights.len())
        || grid.heights.len() > 16_777_216
    {
        return fail("terrain dimensions must match 2..16777216 finite height samples");
    }
    if !grid.spacing.is_finite()
        || grid.spacing < 0.05
        || !grid.origin.iter().all(|v| v.is_finite())
        || !grid.heights.iter().all(|v| v.is_finite())
        || !input.sea_level.is_finite()
    {
        return fail(
            "terrain origin, heights and sea level must be finite; spacing must be >= 0.05m",
        );
    }
    if input.protected_terrain.len() > 64
        || input.protected_terrain.iter().any(|area| !area.is_valid())
    {
        return fail("protected terrain requires up to 64 finite, ordered rectangles");
    }
    validate_rivers(input)
}

pub(super) fn validate_recipe(
    recipe: &TownRecipe,
) -> Result<Vec<Cow<'_, BuildingRecipe>>, TownError> {
    let fail = |message: &str| Err(TownError::InvalidInput(message.into()));
    if recipe.schema_version != TOWN_SCHEMA_VERSION {
        return fail("schema_version must be 1");
    }
    if recipe.catalogue.len() > 64
        || recipe
            .catalogue
            .iter()
            .map(|e| u64::from(e.max_count))
            .sum::<u64>()
            > 256
    {
        return fail("catalogue is limited to 64 entries and 256 total maximum copies");
    }
    let mut buildings = Vec::with_capacity(recipe.catalogue.len());
    for (index, entry) in recipe.catalogue.iter().enumerate() {
        if entry.id.trim().is_empty()
            || recipe.catalogue[..index]
                .iter()
                .any(|other| other.id == entry.id)
        {
            return fail("catalogue ids must be nonempty and unique");
        }
        if entry.min_count > entry.max_count {
            return fail("catalogue min_count must not exceed max_count");
        }
        let resolved =
            crate::openings::resolve(&entry.building).map_err(|error| TownError::Building {
                entry: entry.id.clone(),
                reason: error.to_string(),
            })?;
        crate::validation::validate_structure(resolved.as_ref()).map_err(|error| {
            TownError::Building {
                entry: entry.id.clone(),
                reason: error.to_string(),
            }
        })?;
        buildings.push(resolved);
    }
    let s = &recipe.settings;
    let bounded = [
        (s.site_search_radius, 1., 1000.),
        (s.town_radius, 1., 1000.),
        (s.building_gap, 0., 100.),
        (s.river_margin, 0., 100.),
        (s.clearance_margin, 0., 100.),
        (s.approach_length, 0., 30.),
        (s.max_slope_degrees, 0.1, 40.),
        (s.max_support_height, 0.05, 10.),
        (s.min_dry_height, 0.01, 10.),
        (s.sample_spacing, 0.25, 4.),
    ];
    if bounded
        .iter()
        .any(|(v, lo, hi)| !v.is_finite() || v < lo || v > hi)
        || s.attempts_per_copy == 0
        || s.attempts_per_copy > 512
        || s.max_total_attempts == 0
        || s.max_total_attempts > 16_384
    {
        return fail(
            "settings outside supported finite bounds: radii 1..1000m, slope 0.1..40deg, sample spacing 0.25..4m, attempts/copy 1..512, total attempts 1..16384",
        );
    }
    if s.roads.is_some_and(|roads| !roads.valid()) {
        return fail("road settings outside supported bounds");
    }
    Ok(buildings)
}

fn validate_rivers(input: &TownInput) -> Result<(), TownError> {
    let fail = |message: &str| Err(TownError::InvalidInput(message.into()));
    if input.rivers.len() > 256
        || input.rivers.iter().map(|r| r.points.len()).sum::<usize>() > 32_768
    {
        return fail("river input exceeds 256 lines or 32768 total points");
    }
    for (index, river) in input.rivers.iter().enumerate() {
        if river
            .join
            .is_some_and(|join| join >= input.rivers.len() || join == index)
        {
            return fail("river join must reference a different valid river index");
        }
        if river.points.len() < 2
            || !river.points.iter().flatten().all(|v| v.is_finite())
            || (!river.widths.is_empty() && river.widths.len() != river.points.len())
            || !river
                .widths
                .iter()
                .all(|v| v.is_finite() && *v > 0. && *v <= 1000.)
        {
            return fail(
                "rivers need at least two finite XY points and positive widths matching points (or empty widths)",
            );
        }
    }
    for index in 0..input.rivers.len() {
        let mut current = Some(index);
        for step in 0..=input.rivers.len() {
            let Some(i) = current else {
                break;
            };
            if step == input.rivers.len() {
                return fail("river join graph must not contain a downstream cycle");
            }
            current = input.rivers[i].join;
        }
    }
    Ok(())
}
