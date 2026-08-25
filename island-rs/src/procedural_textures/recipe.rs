//! Versioned, serde-ready texture recipes.
//!
//! The recipe model intentionally carries plain configuration values rather
//! than references to a renderer or to a field evaluator.  The generation
//! boundary can therefore convert a material variant into the appropriate
//! field program without making JSON parsing depend on Unity, Bevy or image
//! codecs.

#![allow(clippy::missing_errors_doc)]

use serde::{Deserialize, Serialize};

pub use super::image::NormalConvention;

/// Current recipe schema accepted by the Phase 1 validator.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;
/// Maximum number of recursive/stacked surface layers accepted by validation.
pub const MAX_SURFACE_LAYERS: usize = 64;
/// Maximum number of output profiles in one recipe.
pub const MAX_OUTPUT_PROFILES: usize = 4;

/// Root JSON recipe for one periodic generated texture set.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextureRecipe {
    /// Version of this JSON schema.
    pub schema_version: u32,
    /// Human-readable and file-safe output name.
    pub name: String,
    /// Root random seed. Individual layers derive independent domains from it.
    pub seed: u64,
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Physical tile width in metres.
    #[serde(alias = "tile_width_m")]
    pub physical_tile_width_m: f32,
    /// Physical tile height in metres.
    #[serde(alias = "tile_height_m")]
    pub physical_tile_height_m: f32,
    /// Height/material model to evaluate.
    pub material: MaterialModel,
    /// Additional surface-noise layers blended into the authoritative field.
    #[serde(default, alias = "surface_noise_layers")]
    pub surface_layers: Vec<NoiseLayer>,
    /// Tangent normal green-channel convention.
    pub normal_convention: NormalConvention,
    /// Dimensionless normal relief multiplier.
    pub normal_scale: f32,
    /// Physical height range and interpretation.
    pub displacement: DisplacementSettings,
    /// Material-local AO quality and response.
    pub occlusion: OcclusionRecipeSettings,
    /// Lighting-free albedo controls.
    pub albedo: AlbedoSettings,
    /// Requested downstream output profiles.
    pub output_profiles: Vec<OutputProfile>,
}

/// A material/height-field model selected by a recipe.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MaterialModel {
    /// General-purpose fBM material experimentation.
    LayeredNoise {
        #[serde(default = "default_frequency")]
        frequency: f32,
        #[serde(default = "default_amplitude")]
        amplitude: f32,
        #[serde(default = "default_octaves")]
        octaves: u8,
        #[serde(default = "default_lacunarity")]
        lacunarity: f32,
        #[serde(default = "default_gain")]
        gain: f32,
        #[serde(default)]
        offset: f32,
    },
    /// Irregular connected slabs with bevelled cracks and fractures.
    CrackedStone {
        #[serde(default = "default_cracked_cells")]
        cells_x: u32,
        #[serde(default = "default_cracked_cells")]
        cells_y: u32,
        #[serde(default = "default_cell_jitter")]
        cell_jitter: f32,
        #[serde(default = "default_crack_warp")]
        warp_amplitude: f32,
        #[serde(default = "default_crack_width")]
        crack_width: f32,
        #[serde(default = "default_shoulder_width")]
        shoulder_width: f32,
        #[serde(default = "default_crack_depth")]
        crack_depth: f32,
        #[serde(default = "default_slab_variation")]
        slab_variation: f32,
        #[serde(default = "default_fracture_probability")]
        fracture_probability: f32,
        #[serde(default = "default_fracture_depth")]
        fracture_depth: f32,
        #[serde(default = "default_surface_amplitude")]
        surface_amplitude: f32,
        #[serde(default = "default_broad_variation")]
        broad_variation: f32,
    },
    /// Smaller separated rounded stones with sand/silt gaps.
    RoundedStones {
        #[serde(default = "default_rounded_cells")]
        cells_x: u32,
        #[serde(default = "default_rounded_cells")]
        cells_y: u32,
        #[serde(default = "default_stone_radius")]
        stone_radius: f32,
        #[serde(default = "default_rounded_jitter")]
        cell_jitter: f32,
        #[serde(default = "default_stone_warp")]
        warp_amplitude: f32,
        #[serde(default = "default_anisotropy")]
        anisotropy: f32,
        #[serde(default = "default_stone_height")]
        stone_height: f32,
        #[serde(default = "default_stone_variation")]
        stone_variation: f32,
        #[serde(default = "default_gap_height")]
        gap_height: f32,
        #[serde(default = "default_sand_amplitude")]
        sand_amplitude: f32,
        #[serde(default = "default_edge_softness")]
        edge_softness: f32,
    },
}

impl Default for MaterialModel {
    fn default() -> Self {
        Self::LayeredNoise {
            frequency: default_frequency(),
            amplitude: default_amplitude(),
            octaves: default_octaves(),
            lacunarity: default_lacunarity(),
            gain: default_gain(),
            offset: 0.0,
        }
    }
}

/// Plain public parameters for converting a layered recipe into a field
/// evaluator.  It deliberately does not depend on `field_program.rs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayeredNoiseRecipeConfig {
    pub frequency: f32,
    pub amplitude: f32,
    pub octaves: u8,
    pub lacunarity: f32,
    pub gain: f32,
    pub offset: f32,
}

/// Plain public parameters for converting a cracked-stone recipe into a field
/// evaluator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrackedStoneRecipeConfig {
    pub cells_x: u32,
    pub cells_y: u32,
    pub cell_jitter: f32,
    pub warp_amplitude: f32,
    pub crack_width: f32,
    pub shoulder_width: f32,
    pub crack_depth: f32,
    pub slab_variation: f32,
    pub fracture_probability: f32,
    pub fracture_depth: f32,
    pub surface_amplitude: f32,
    pub broad_variation: f32,
}

/// Plain public parameters for converting a rounded-stones recipe into a
/// field evaluator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedStonesRecipeConfig {
    pub cells_x: u32,
    pub cells_y: u32,
    pub stone_radius: f32,
    pub cell_jitter: f32,
    pub warp_amplitude: f32,
    pub anisotropy: f32,
    pub stone_height: f32,
    pub stone_variation: f32,
    pub gap_height: f32,
    pub sand_amplitude: f32,
    pub edge_softness: f32,
}

impl MaterialModel {
    /// Returns layered-noise parameters when this is the layered variant.
    #[must_use]
    pub fn layered_noise_config(&self) -> Option<LayeredNoiseRecipeConfig> {
        let Self::LayeredNoise {
            frequency,
            amplitude,
            octaves,
            lacunarity,
            gain,
            offset,
        } = *self
        else {
            return None;
        };
        Some(LayeredNoiseRecipeConfig {
            frequency,
            amplitude,
            octaves,
            lacunarity,
            gain,
            offset,
        })
    }

    /// Returns cracked-stone parameters when this is the cracked variant.
    #[must_use]
    pub fn cracked_stone_config(&self) -> Option<CrackedStoneRecipeConfig> {
        let Self::CrackedStone {
            cells_x,
            cells_y,
            cell_jitter,
            warp_amplitude,
            crack_width,
            shoulder_width,
            crack_depth,
            slab_variation,
            fracture_probability,
            fracture_depth,
            surface_amplitude,
            broad_variation,
        } = *self
        else {
            return None;
        };
        Some(CrackedStoneRecipeConfig {
            cells_x,
            cells_y,
            cell_jitter,
            warp_amplitude,
            crack_width,
            shoulder_width,
            crack_depth,
            slab_variation,
            fracture_probability,
            fracture_depth,
            surface_amplitude,
            broad_variation,
        })
    }

    /// Returns rounded-stones parameters when this is the rounded variant.
    #[must_use]
    pub fn rounded_stones_config(&self) -> Option<RoundedStonesRecipeConfig> {
        let Self::RoundedStones {
            cells_x,
            cells_y,
            stone_radius,
            cell_jitter,
            warp_amplitude,
            anisotropy,
            stone_height,
            stone_variation,
            gap_height,
            sand_amplitude,
            edge_softness,
        } = *self
        else {
            return None;
        };
        Some(RoundedStonesRecipeConfig {
            cells_x,
            cells_y,
            stone_radius,
            cell_jitter,
            warp_amplitude,
            anisotropy,
            stone_height,
            stone_variation,
            gap_height,
            sand_amplitude,
            edge_softness,
        })
    }
}

/// A source field and its blend controls.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NoiseLayer {
    /// Primitive source selected by the tagged `kind` enum.
    pub kind: NoiseKind,
    /// Frequency in lattice cells per physical tile.
    #[serde(default = "default_frequency")]
    pub frequency: f32,
    /// Signed contribution to the field.
    #[serde(default = "default_amplitude")]
    pub amplitude: f32,
    /// Fractal octave count for fBM/billow/ridged kinds.
    #[serde(default = "default_octaves")]
    pub octaves: u8,
    /// Frequency multiplier between octaves.
    #[serde(default = "default_lacunarity")]
    pub lacunarity: f32,
    /// Amplitude multiplier between octaves.
    #[serde(default = "default_gain")]
    pub gain: f32,
    /// Tile-space coordinate offset.
    #[serde(default)]
    pub offset: [f32; 2],
    /// Independent random seed domain for this layer.
    #[serde(default)]
    pub seed_domain: u64,
    /// Operation used to combine this layer with the accumulated field.
    #[serde(default)]
    pub blend: BlendOperation,
    /// Optional broad domain warp applied to the layer source.
    #[serde(default)]
    pub domain_warp: Option<DomainWarpSettings>,
    /// Jitter for cellular source kinds.
    #[serde(default = "default_cell_jitter")]
    pub cellular_jitter: f32,
}

impl Default for NoiseLayer {
    fn default() -> Self {
        Self {
            kind: NoiseKind::Value,
            frequency: default_frequency(),
            amplitude: default_amplitude(),
            octaves: default_octaves(),
            lacunarity: default_lacunarity(),
            gain: default_gain(),
            offset: [0.0, 0.0],
            seed_domain: 0,
            blend: BlendOperation::Replace,
            domain_warp: None,
            cellular_jitter: default_cell_jitter(),
        }
    }
}

/// Primitive scalar source available to a [`NoiseLayer`].
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoiseKind {
    /// Interpolated periodic value noise.
    Value,
    /// Normalized fractal Brownian motion.
    Fbm,
    /// Absolute-value fBM remapped to `[-1, 1]`.
    Billow,
    /// Inverted absolute-value fBM remapped to `[-1, 1]`.
    Ridged,
    /// Cellular nearest-feature distance.
    CellularDistance,
    /// Cellular distance-to-edge approximation.
    CellularDistanceToEdge,
    /// Stable per-cell random value.
    CellularValue,
    /// A recursively selected source evaluated after a periodic coordinate
    /// warp. The warp source uses the same lattice period as the source.
    DomainWarp {
        source: Box<NoiseKind>,
        warp: Box<NoiseKind>,
        #[serde(default = "default_warp_amplitude")]
        amplitude: f32,
    },
}

/// Operation used while folding noise layers into an accumulated field.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlendOperation {
    /// Replace the accumulated field with this layer.
    #[default]
    Replace,
    /// Add this layer.
    Add,
    /// Subtract this layer.
    Subtract,
    /// Multiply by this layer.
    Multiply,
    /// Keep the lower value.
    Minimum,
    /// Keep the higher value.
    Maximum,
    /// Fixed-amount interpolation from the accumulated field to this layer.
    Lerp { amount: f32 },
    /// Generated-mask interpolation from the accumulated field to this layer.
    LerpByMask { mask: Box<NoiseLayer> },
}

/// Settings for an optional periodic coordinate warp.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct DomainWarpSettings {
    /// Coordinate displacement in lattice units.
    #[serde(default = "default_warp_amplitude")]
    pub amplitude: f32,
    /// Warp source frequency in cells per tile.
    #[serde(default = "default_frequency")]
    pub frequency: f32,
    /// Warp fBM octave count.
    #[serde(default = "default_warp_octaves")]
    pub octaves: u8,
    /// Warp octave frequency multiplier.
    #[serde(default = "default_lacunarity")]
    pub lacunarity: f32,
    /// Warp octave amplitude multiplier.
    #[serde(default = "default_gain")]
    pub gain: f32,
    /// Independent random domain for the warp.
    #[serde(default)]
    pub seed_domain: u64,
}

impl Default for DomainWarpSettings {
    fn default() -> Self {
        Self {
            amplitude: default_warp_amplitude(),
            frequency: default_frequency(),
            octaves: default_warp_octaves(),
            lacunarity: default_lacunarity(),
            gain: default_gain(),
            seed_domain: 0,
        }
    }
}

/// Physical height range stored in metadata and used by quantization.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct DisplacementSettings {
    /// Minimum represented linear height in metres.
    pub minimum_m: f32,
    /// Maximum represented linear height in metres.
    pub maximum_m: f32,
    /// Neutral/base height in metres.
    pub base_m: f32,
    /// Whether consumers should interpret the map as displacement.
    #[serde(default = "default_true")]
    pub displacement_map: bool,
}

impl Default for DisplacementSettings {
    fn default() -> Self {
        Self {
            minimum_m: -0.2,
            maximum_m: 0.2,
            base_m: 0.0,
            displacement_map: true,
        }
    }
}

/// Material-local occlusion controls.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct OcclusionRecipeSettings {
    /// Number of fixed horizon directions.
    #[serde(default = "default_ao_directions")]
    pub directions: u8,
    /// Samples per horizon direction.
    #[serde(default = "default_ao_samples")]
    pub samples: u8,
    /// Initial sample radius in pixels.
    #[serde(default = "default_ao_radius")]
    pub radius: f32,
    /// Largest radius multiplier relative to `radius`.
    #[serde(default = "default_ao_max_radius")]
    pub max_radius: f32,
    /// Cavity response strength.
    #[serde(default = "default_cavity_strength")]
    pub cavity_strength: f32,
    /// Horizon response strength.
    #[serde(default = "default_horizon_strength")]
    pub horizon_strength: f32,
    /// Final response power.
    #[serde(default = "default_ao_power")]
    pub power: f32,
    /// How cavity and horizon terms are combined.
    #[serde(default)]
    pub combine: OcclusionCombine,
}

impl Default for OcclusionRecipeSettings {
    fn default() -> Self {
        Self {
            directions: default_ao_directions(),
            samples: default_ao_samples(),
            radius: default_ao_radius(),
            max_radius: default_ao_max_radius(),
            cavity_strength: default_cavity_strength(),
            horizon_strength: default_horizon_strength(),
            power: default_ao_power(),
            combine: OcclusionCombine::Multiply,
        }
    }
}

/// Cavity/horizon combination policy.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OcclusionCombine {
    /// Multiply independent openness terms.
    #[default]
    Multiply,
    /// Use a weighted minimum of the two openness terms.
    WeightedMinimum {
        #[serde(default = "default_half")]
        cavity_weight: f32,
        #[serde(default = "default_half")]
        horizon_weight: f32,
    },
}

/// Lighting-free linear albedo controls.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AlbedoSettings {
    /// Primary cool/warm base colour in linear RGB.
    #[serde(default = "default_base_color")]
    pub base_color: [f32; 3],
    /// Secondary palette colour in linear RGB.
    #[serde(default = "default_warm_color")]
    pub warm_color: [f32; 3],
    /// Optional larger palette. Empty means use `base_color` and `warm_color`.
    #[serde(default)]
    pub palette: Vec<[f32; 3]>,
    /// Broad colour variation.
    #[serde(default = "default_albedo_variation")]
    pub variation: f32,
    /// Darkening inside deep cracks.
    #[serde(default = "default_crack_darkening")]
    pub crack_darkening: f32,
    /// Shoulder variation amount.
    #[serde(default = "default_shoulder_variation")]
    pub shoulder_variation: f32,
    /// Sparse mineral-fleck density.
    #[serde(default = "default_mineral_density")]
    pub mineral_density: f32,
    /// Mineral-fleck brightness.
    #[serde(default = "default_mineral_brightness")]
    pub mineral_brightness: f32,
    /// Optional indirect-occlusion influence, deliberately kept subtle.
    #[serde(default = "default_occlusion_influence")]
    pub occlusion_influence: f32,
}

impl Default for AlbedoSettings {
    fn default() -> Self {
        Self {
            base_color: default_base_color(),
            warm_color: default_warm_color(),
            palette: Vec::new(),
            variation: default_albedo_variation(),
            crack_darkening: default_crack_darkening(),
            shoulder_variation: default_shoulder_variation(),
            mineral_density: default_mineral_density(),
            mineral_brightness: default_mineral_brightness(),
            occlusion_influence: default_occlusion_influence(),
        }
    }
}

/// A file-output profile requested by a recipe.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputProfile {
    /// Separate albedo, height, normal and occlusion files.
    Separate,
    /// Separate files plus the packed Unity terrain mask.
    MotuUnityTerrain,
}

fn default_true() -> bool {
    true
}

fn default_frequency() -> f32 {
    1.0
}

fn default_amplitude() -> f32 {
    1.0
}

fn default_octaves() -> u8 {
    4
}

fn default_warp_octaves() -> u8 {
    3
}

fn default_lacunarity() -> f32 {
    2.0
}

fn default_gain() -> f32 {
    0.5
}

fn default_cell_jitter() -> f32 {
    0.25
}

fn default_cracked_cells() -> u32 {
    8
}

fn default_rounded_cells() -> u32 {
    14
}

fn default_rounded_jitter() -> f32 {
    0.23
}

fn default_crack_warp() -> f32 {
    0.16
}

fn default_stone_warp() -> f32 {
    0.08
}

fn default_crack_width() -> f32 {
    0.035
}

fn default_shoulder_width() -> f32 {
    0.18
}

fn default_crack_depth() -> f32 {
    0.13
}

fn default_slab_variation() -> f32 {
    0.035
}

fn default_fracture_probability() -> f32 {
    0.28
}

fn default_fracture_depth() -> f32 {
    0.045
}

fn default_surface_amplitude() -> f32 {
    0.014
}

fn default_broad_variation() -> f32 {
    0.018
}

fn default_stone_radius() -> f32 {
    0.36
}

fn default_anisotropy() -> f32 {
    1.0
}

fn default_stone_height() -> f32 {
    0.12
}

fn default_stone_variation() -> f32 {
    0.045
}

fn default_gap_height() -> f32 {
    -0.012
}

fn default_sand_amplitude() -> f32 {
    0.009
}

fn default_edge_softness() -> f32 {
    0.08
}

fn default_warp_amplitude() -> f32 {
    0.15
}

fn default_ao_directions() -> u8 {
    8
}

fn default_ao_samples() -> u8 {
    6
}

fn default_ao_radius() -> f32 {
    1.0
}

fn default_ao_max_radius() -> f32 {
    8.0
}

fn default_cavity_strength() -> f32 {
    1.5
}

fn default_horizon_strength() -> f32 {
    0.85
}

fn default_ao_power() -> f32 {
    1.0
}

fn default_half() -> f32 {
    0.5
}

fn default_base_color() -> [f32; 3] {
    [0.25, 0.27, 0.24]
}

fn default_warm_color() -> [f32; 3] {
    [0.42, 0.36, 0.28]
}

fn default_albedo_variation() -> f32 {
    0.12
}

fn default_crack_darkening() -> f32 {
    0.28
}

fn default_shoulder_variation() -> f32 {
    0.06
}

fn default_mineral_density() -> f32 {
    0.055
}

fn default_mineral_brightness() -> f32 {
    0.25
}

fn default_occlusion_influence() -> f32 {
    0.08
}
