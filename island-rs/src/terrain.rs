#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashSet},
    fs::File,
    io::{self, Read},
    mem::{self, size_of},
    path::Path,
    sync::OnceLock,
    thread,
};

use rayon::prelude::*;

use crate::{
    Adjacency, BoundingBox, ISLAND_WORLD_METRES, Mesh, Raster, River, Vec2, Vec3, Vec4,
    geology::{self, GeologyField},
    mesh::{EdgeSplitStencil, NewVertexStencil, TessellationResult},
    mesh_clipper::MeshClipper,
    noise,
    profiling::StageTimer,
    rivers::{
        RiverChannelSettings, RiverNetwork, RiverShapeOptions, RiverSourceRule, WaterfallPlacement,
        append_settled_rocks, encode_bank_distance_in_uv, fix_inland_seas,
    },
    rng::Rng,
};

mod coastal_uplift;
mod decorations;
mod erosion;
mod generation;
mod generation_method;
#[cfg(feature = "gpu-generation")]
mod gpu_generation;
mod lod;
mod material;
mod sampling;
mod snapshot;
mod surface_maps;

pub(crate) use decorations::SettledRock;
pub use decorations::{Decoration, Decorations};
use erosion::{
    HydraulicScratch, barycentric, bin_coordinate, erode_mesh, hydraulic_erode_stage,
    hydraulic_erode_stage_depositing_across_sea, triangle_bin_bounds,
};
pub(crate) use erosion::{ProjectedFaceAreas, VertexFaceAdjacency, bedrock_erosion_rate};
use generation::GenerationScratch;
pub use generation::Island;
#[cfg(test)]
use generation::sharp_rock_mask;
pub use generation_method::GenerationMethod;
#[cfg(feature = "gpu-generation")]
use gpu_generation::GpuParticleErosionScratch;
pub(crate) use material::{SurfaceMaterial, projected_vertex_control_areas};
use material::{TerrainEnvironmentField, TerrainMaterialField};
pub(crate) use sampling::TerrainSupportSample;
pub use sampling::{SurfaceMaps, Terrain};
use sampling::{
    SurfaceSample, TriangleIndex, bury_river_banks, sample_mesh_surface, sample_mesh_triangle,
};
use surface_maps::bake_surface_maps;

const DETAIL_DISPLACEMENT_RATIO: f32 = 0.025;
const SHARP_ROCK_DISPLACEMENT_RATIO: f32 = 0.15;
const HYDRAULIC_EDGE_SHIFT_LIMIT: f32 = 0.08;
const HYDRAULIC_MIN_PROJECTED_AREA_RATIO: f32 = 0.2;
const MINIMUM_BEDROCK_EROSION_RATE: f32 = 0.05;
const TRIANGLE_INDEX_OFFSET_BUDGET_BYTES: usize = 32 * 1024 * 1024;
const TERRAIN_RENDER_FLOOR: f32 = -5.0 / ISLAND_WORLD_METRES;
pub(crate) const LOOSE_DEPTH_EPSILON: f32 = 1.0e-8;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct IslandOptions {
    pub max_height: f32,
    pub water_ratio: f32,
    pub slope_multiplier: f32,
    pub coastal_slope_multiplier: f32,
    /// Spatial frequency of the broad noise shared by initial elevation and hardness.
    pub continental_noise_frequency: f32,
    /// Contribution of the broad noise to initial elevation and hardness.
    pub continental_noise_strength: f32,
    /// Spatial frequency of the fine noise shared by initial elevation and hardness.
    pub detail_noise_frequency: f32,
    /// Contribution of the fine noise to initial elevation and hardness.
    pub detail_noise_strength: f32,
    /// Signed offset applied after percentile sea-level centring. Negative
    /// values submerge land and isolate high points into separate islands.
    pub land_mass_offset: f32,
    /// Multiplies the strength of every staged hydraulic erosion pass.
    /// Zero disables hydraulic erosion while preserving thermal erosion.
    pub hydraulic_erosion_strength: f32,
    /// Controls how quickly excess carried sediment settles on gentle slopes.
    pub hydraulic_deposition_strength: f32,
    /// Slope angle at which hydraulic deposition falls to zero.
    pub hydraulic_deposition_slope_degrees: f32,
    /// Minimum upstream drainage area in hectares required for a river source.
    pub river_source_catchment_hectares: f32,
    /// Multiplies the required source flow as the routed edge approaches
    /// vertical. One disables the slope penalty.
    pub river_source_steep_multiplier: f32,
    /// Additional catchment multiplier applied at sea level, fading to zero
    /// at the configured maximum elevation.
    pub river_source_elevation_boost: f32,
    /// Full channel width at the lowest represented flow, in metres.
    pub river_source_width_metres: f32,
    /// Full channel width at the greatest represented flow, in metres.
    pub river_maximum_width_metres: f32,
    /// Nominal bed depth at the lowest represented flow, in metres.
    pub river_source_depth_metres: f32,
    /// Nominal bed depth at the greatest represented flow, in metres.
    pub river_maximum_depth_metres: f32,
    /// Number of free-form XY seed points used by Delaunay triangulation.
    pub terrain_size: u32,
}

impl Default for IslandOptions {
    fn default() -> Self {
        Self {
            max_height: 0.2,
            water_ratio: 0.6,
            slope_multiplier: 1.3,
            coastal_slope_multiplier: 1.0,
            continental_noise_frequency: 2.2,
            continental_noise_strength: 0.78,
            detail_noise_frequency: 12.0,
            detail_noise_strength: 0.22,
            land_mass_offset: 0.0,
            hydraulic_erosion_strength: 1.0,
            hydraulic_deposition_strength: 1.5,
            hydraulic_deposition_slope_degrees: 12.0,
            river_source_catchment_hectares: 0.05,
            river_source_steep_multiplier: 4.0,
            river_source_elevation_boost: 9.0,
            river_source_width_metres: 2.0,
            river_maximum_width_metres: 14.0,
            river_source_depth_metres: 0.35,
            river_maximum_depth_metres: 2.0,
            terrain_size: 1024,
        }
    }
}

impl IslandOptions {
    const fn river_source_rule(self) -> RiverSourceRule {
        RiverSourceRule::new(
            self.river_source_catchment_hectares,
            self.river_source_steep_multiplier,
            self.river_source_elevation_boost,
            self.max_height,
        )
    }

    const fn river_channel_settings(self) -> RiverChannelSettings {
        RiverChannelSettings {
            source_width: self.river_source_width_metres / ISLAND_WORLD_METRES,
            maximum_width: self.river_maximum_width_metres / ISLAND_WORLD_METRES,
            source_depth: self.river_source_depth_metres / ISLAND_WORLD_METRES,
            maximum_depth: self.river_maximum_depth_metres / ISLAND_WORLD_METRES,
        }
    }

    pub(super) fn validate(self) -> Result<Self, String> {
        if !self.max_height.is_finite() || self.max_height <= 0.0 {
            return Err("max_height must be finite and greater than zero".into());
        }
        if !self.continental_noise_frequency.is_finite()
            || !(0.1..=128.0).contains(&self.continental_noise_frequency)
            || !self.detail_noise_frequency.is_finite()
            || !(0.1..=128.0).contains(&self.detail_noise_frequency)
        {
            return Err("terrain noise frequencies must be between 0.1 and 128".into());
        }
        if !self.continental_noise_strength.is_finite()
            || !(0.0..=4.0).contains(&self.continental_noise_strength)
            || !self.detail_noise_strength.is_finite()
            || !(0.0..=4.0).contains(&self.detail_noise_strength)
        {
            return Err("terrain noise strengths must be between 0 and 4".into());
        }
        if !self.land_mass_offset.is_finite() || !(-2.0..=2.0).contains(&self.land_mass_offset) {
            return Err("land_mass_offset must be between -2 and 2".into());
        }
        if !self.hydraulic_erosion_strength.is_finite()
            || !(0.0..=8.0).contains(&self.hydraulic_erosion_strength)
        {
            return Err("hydraulic_erosion_strength must be between 0 and 8".into());
        }
        if !self.hydraulic_deposition_strength.is_finite()
            || !(0.0..=4.0).contains(&self.hydraulic_deposition_strength)
        {
            return Err("hydraulic_deposition_strength must be between 0 and 4".into());
        }
        if !self.hydraulic_deposition_slope_degrees.is_finite()
            || !(1.0..=45.0).contains(&self.hydraulic_deposition_slope_degrees)
        {
            return Err("hydraulic_deposition_slope_degrees must be between 1 and 45".into());
        }
        if self.terrain_size < 16 || self.terrain_size > 4096 {
            return Err("terrain_size must contain between 16 and 4096 seed points".into());
        }
        if !self.river_source_width_metres.is_finite()
            || !self.river_maximum_width_metres.is_finite()
            || self.river_source_width_metres <= 0.0
            || self.river_maximum_width_metres < self.river_source_width_metres
        {
            return Err(
                "river widths must be finite and maximum width must be at least source width"
                    .into(),
            );
        }
        if !self.river_source_depth_metres.is_finite()
            || !self.river_maximum_depth_metres.is_finite()
            || self.river_source_depth_metres <= 0.0
            || self.river_maximum_depth_metres < self.river_source_depth_metres
        {
            return Err(
                "river depths must be finite and maximum depth must be at least source depth"
                    .into(),
            );
        }
        Ok(self)
    }
}

fn sample_grid(width: u32, height: u32, mut sample: impl FnMut(f32, f32) -> f32) -> Vec<f32> {
    let mut output = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        let v = y as f32 / height.saturating_sub(1).max(1) as f32;
        for x in 0..width {
            let u = x as f32 / width.saturating_sub(1).max(1) as f32;
            output.push(sample(u, v));
        }
    }
    output
}

fn legacy_catchment_hectares(fraction: f32, water_ratio: f32) -> f32 {
    const SQUARE_METRES_PER_HECTARE: f32 = 10_000.0;

    let estimated_land_fraction = (1.0 - water_ratio).clamp(0.0, 1.0);
    fraction * estimated_land_fraction * ISLAND_WORLD_METRES * ISLAND_WORLD_METRES
        / SQUARE_METRES_PER_HECTARE
}
