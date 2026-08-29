//! The single current procedural-material document format.
//!
//! The recipe is deliberately a plain serde model. It describes an ordered
//! stack, while the evaluator remains in Rust so Unity and other consumers do
//! not need to reproduce sampling or blend rules.

#![allow(clippy::missing_errors_doc)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::parameters::{ColourValue, LinearRgb, ParameterDefinition};

/// Maximum number of ordered material layers accepted by validation.
pub const MAX_LAYERS: usize = 64;
/// Maximum number of output profiles in one recipe.
pub const MAX_OUTPUT_PROFILES: usize = 4;
/// Maximum number of colour stops in one layer gradient.
pub const MAX_GRADIENT_STOPS: usize = 32;
/// Maximum number of scalar remap control points.
pub const MAX_REMAP_POINTS: usize = 16;
/// Largest coordinate warp supported by the cracked-stone evaluator.
pub const CRACKED_STONE_MAX_WARP_AMPLITUDE: f32 = 0.45;
/// Largest coordinate warp supported by the rounded-stone evaluator.
pub const ROUNDED_STONES_MAX_WARP_AMPLITUDE: f32 = 0.4;
/// Largest cell-relative radius supported by the rounded-stone evaluator.
pub const ROUNDED_STONES_MAX_RADIUS: f32 = 1.0;

/// Root JSON recipe for one periodic generated texture set.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextureRecipe {
    /// Human-readable and file-safe output name.
    pub name: String,
    /// Root random seed. Individual layers derive independent domains from it.
    pub seed: u64,
    /// Typed values that callers may override before evaluation.
    #[serde(default)]
    pub parameters: BTreeMap<String, ParameterDefinition>,
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Physical tile width in metres.
    pub physical_tile_width_m: f32,
    /// Physical tile height in metres.
    pub physical_tile_height_m: f32,
    /// Specialised base height/material model.
    pub material: MaterialModel,
    /// Ordered scalar layer stack.
    pub layers: Vec<MaterialLayer>,
    /// Dimensionless normal relief multiplier.
    pub normal_scale: f32,
    /// Physical height range and interpretation.
    pub displacement: DisplacementSettings,
    /// Material-local AO quality and response.
    pub occlusion: OcclusionRecipeSettings,
    /// Lighting-free base albedo controls.
    pub albedo: AlbedoSettings,
    /// Requested downstream output profiles.
    pub output_profiles: Vec<OutputProfile>,
}

/// A named, ordered layer with independent height and albedo routing.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialLayer {
    /// Stable identifier used by later-layer masks.
    pub id: String,
    /// Artist-facing label.
    pub name: String,
    /// Disabled layers remain in the document and can still be inspected.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Scalar source and source-specific controls.
    pub source: ScalarSource,
    /// Scalar remapping applied before either output binding.
    #[serde(default)]
    pub remap: ScalarRemap,
    /// Optional opacity/mask source.
    #[serde(default)]
    pub mask: Option<LayerMask>,
    /// Independent height and albedo output bindings.
    pub outputs: LayerOutputs,
}

impl Default for MaterialLayer {
    fn default() -> Self {
        Self {
            id: "layer".into(),
            name: "Layer".into(),
            enabled: true,
            source: ScalarSource::default(),
            remap: ScalarRemap::default(),
            mask: None,
            outputs: LayerOutputs::default(),
        }
    }
}

/// Scalar source and its common sampling controls.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScalarSource {
    /// Primitive source kind.
    pub kind: SourceKind,
    /// Whole-number lattice cells per tile.
    #[serde(default = "default_frequency")]
    pub frequency: u32,
    /// Fractal octave count for fBM, billow and ridged sources.
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
    /// Independent random seed domain for this source.
    #[serde(default)]
    pub seed_domain: u64,
    /// Optional explicit periodic domain warp modifier.
    #[serde(default)]
    pub domain_warp: Option<DomainWarpSettings>,
    /// Jitter for cellular source kinds.
    #[serde(default = "default_cell_jitter")]
    pub cellular_jitter: f32,
}

impl Default for ScalarSource {
    fn default() -> Self {
        Self {
            kind: SourceKind::Value,
            frequency: default_frequency(),
            octaves: default_octaves(),
            lacunarity: default_lacunarity(),
            gain: default_gain(),
            offset: [0.0, 0.0],
            seed_domain: 0,
            domain_warp: None,
            cellular_jitter: default_cell_jitter(),
        }
    }
}

/// Primitive scalar source kinds.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Interpolated periodic value noise.
    #[default]
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
}

impl SourceKind {
    #[must_use]
    pub const fn is_cellular(self) -> bool {
        matches!(
            self,
            Self::CellularDistance | Self::CellularDistanceToEdge | Self::CellularValue
        )
    }

    #[must_use]
    pub const fn is_fractal(self) -> bool {
        matches!(self, Self::Fbm | Self::Billow | Self::Ridged)
    }
}

/// Settings for one explicit periodic coordinate warp modifier.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DomainWarpSettings {
    /// Coordinate displacement in lattice units.
    #[serde(default = "default_warp_amplitude")]
    pub amplitude: f32,
    /// Warp source frequency in cells per tile.
    #[serde(default = "default_frequency")]
    pub frequency: u32,
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

/// Scalar remapping before routing a source into outputs or masks.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScalarRemap {
    /// Lower bound selected from the raw source range.
    #[serde(default = "default_input_min")]
    pub input_min: f32,
    /// Upper bound selected from the raw source range.
    #[serde(default = "default_input_max")]
    pub input_max: f32,
    /// Reverse the mapped scalar after range selection.
    #[serde(default)]
    pub invert: bool,
    /// Monotonic contrast around the midpoint.
    #[serde(default = "default_contrast")]
    pub contrast: f32,
    /// Bias applied after contrast.
    #[serde(default)]
    pub bias: f32,
    /// Clamp the mapped scalar to `[0, 1]`.
    #[serde(default = "default_true")]
    pub clamp: bool,
    /// Optional ordered monotonic curve control points.
    #[serde(default)]
    pub curve: Option<Vec<RemapPoint>>,
}

impl Default for ScalarRemap {
    fn default() -> Self {
        Self {
            input_min: default_input_min(),
            input_max: default_input_max(),
            invert: false,
            contrast: default_contrast(),
            bias: 0.0,
            clamp: true,
            curve: None,
        }
    }
}

/// One scalar remap curve control point.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemapPoint {
    /// Input position in the normalized scalar domain.
    pub position: f32,
    /// Output value at this position.
    pub value: f32,
}

/// Layer mask source. A layer reference is valid only for an earlier layer.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LayerMask {
    /// Use the layer's own remapped scalar as opacity.
    Own,
    /// Remap the accumulated physical height immediately before this layer.
    PreviousHeight {
        /// Height at or below which the mask is zero, in metres.
        bottom_m: f32,
        /// Height at or above which the mask is one, in metres.
        top_m: f32,
        /// Select lower heights instead of higher heights.
        #[serde(default)]
        invert: bool,
    },
    /// Evaluate an inline source and remap it as opacity.
    Noise {
        source: ScalarSource,
        #[serde(default)]
        remap: ScalarRemap,
    },
    /// Use an earlier layer's remapped scalar as opacity.
    Layer {
        layer_id: String,
        #[serde(default)]
        remap: ScalarRemap,
    },
}

/// Independent routing bindings for one material layer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LayerOutputs {
    /// Physical displacement binding.
    #[serde(default)]
    pub height: HeightOutput,
    /// Lighting-free colour binding.
    #[serde(default)]
    pub albedo: AlbedoOutput,
}

/// Height output binding for one layer.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeightOutput {
    /// Whether this output is active.
    #[serde(default)]
    pub enabled: bool,
    /// Height blend operation.
    #[serde(default)]
    pub blend: HeightBlend,
    /// Physical layer strength in metres.
    #[serde(default = "default_height_strength")]
    pub strength_m: f32,
}

impl Default for HeightOutput {
    fn default() -> Self {
        Self {
            enabled: false,
            blend: HeightBlend::Add,
            strength_m: default_height_strength(),
        }
    }
}

/// Height blend operation.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HeightBlend {
    /// Replace the accumulated field with this layer's physical value.
    Replace,
    /// Add this layer's physical value.
    #[default]
    Add,
    /// Subtract this layer's physical value.
    Subtract,
    /// Multiply by this layer's physical value.
    Multiply,
    /// Keep the lower value.
    Minimum,
    /// Keep the higher value.
    Maximum,
    /// Interpolate from the accumulated value to this layer's value.
    Lerp { amount: f32 },
}

/// Albedo output binding for one layer.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AlbedoOutput {
    /// Whether this output is active.
    #[serde(default)]
    pub enabled: bool,
    /// Albedo blend operation.
    #[serde(default)]
    pub blend: AlbedoBlend,
    /// Opacity/strength in `[0, 1]`.
    #[serde(default = "default_albedo_strength")]
    pub strength: f32,
    /// Scalar-to-linear-RGB map.
    #[serde(default)]
    pub colour_map: ColourMap,
    /// Optional hue rotation influence in normalized turns.
    #[serde(default)]
    pub hue_influence: f32,
    /// Optional saturation influence.
    #[serde(default)]
    pub saturation_influence: f32,
    /// Optional value/brightness influence.
    #[serde(default)]
    pub value_influence: f32,
}

impl Default for AlbedoOutput {
    fn default() -> Self {
        Self {
            enabled: false,
            blend: AlbedoBlend::Mix,
            strength: default_albedo_strength(),
            colour_map: ColourMap::default(),
            hue_influence: 0.0,
            saturation_influence: 0.0,
            value_influence: 0.0,
        }
    }
}

/// Albedo blend operation.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlbedoBlend {
    /// Replace the accumulated colour, respecting output strength.
    Replace,
    /// Interpolate to the mapped colour.
    #[default]
    Mix,
    /// Multiply by the mapped colour.
    Multiply,
    /// Add the mapped colour.
    Add,
    /// Apply a standard overlay response.
    Overlay,
}

/// Scalar-to-colour map.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ColourMap {
    /// Two-colour linear ramp.
    Ramp {
        first: ColourValue,
        second: ColourValue,
    },
    /// Ordered multi-stop linear gradient.
    Gradient { stops: Vec<GradientStop> },
}

impl Default for ColourMap {
    fn default() -> Self {
        Self::Ramp {
            first: default_base_colour_value(),
            second: default_warm_colour_value(),
        }
    }
}

/// One linear-RGB gradient stop.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GradientStop {
    /// Position in the normalized scalar domain.
    pub position: f32,
    /// Linear RGB colour.
    pub colour: ColourValue,
}

/// Base material/height-field model selected by a recipe.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaterialModel {
    /// General-purpose fBM material experimentation.
    LayeredNoise {
        #[serde(default = "default_frequency_f32")]
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
            frequency: default_frequency_f32(),
            amplitude: default_amplitude(),
            octaves: default_octaves(),
            lacunarity: default_lacunarity(),
            gain: default_gain(),
            offset: 0.0,
        }
    }
}

/// Plain parameters for the layered base evaluator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayeredNoiseRecipeConfig {
    pub frequency: f32,
    pub amplitude: f32,
    pub octaves: u8,
    pub lacunarity: f32,
    pub gain: f32,
    pub offset: f32,
}

/// Plain parameters for the cracked-stone evaluator.
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

/// Plain parameters for the rounded-stones evaluator.
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

/// Physical height range stored in metadata and used by quantization.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct OcclusionRecipeSettings {
    #[serde(default = "default_ao_directions")]
    pub directions: u8,
    #[serde(default = "default_ao_samples")]
    pub samples: u8,
    #[serde(default = "default_ao_radius")]
    pub radius: f32,
    #[serde(default = "default_ao_max_radius")]
    pub max_radius: f32,
    #[serde(default = "default_cavity_strength")]
    pub cavity_strength: f32,
    #[serde(default = "default_horizon_strength")]
    pub horizon_strength: f32,
    #[serde(default = "default_ao_power")]
    pub power: f32,
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
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct AlbedoSettings {
    #[serde(default = "default_base_colour_value")]
    pub base_color: ColourValue,
    #[serde(default = "default_warm_colour_value")]
    pub warm_color: ColourValue,
    #[serde(default)]
    pub palette: Vec<ColourValue>,
    #[serde(default = "default_albedo_variation")]
    pub variation: f32,
    #[serde(default = "default_crack_darkening")]
    pub crack_darkening: f32,
    #[serde(default = "default_shoulder_variation")]
    pub shoulder_variation: f32,
    #[serde(default = "default_mineral_density")]
    pub mineral_density: f32,
    #[serde(default = "default_mineral_brightness")]
    pub mineral_brightness: f32,
    #[serde(default = "default_occlusion_influence")]
    pub occlusion_influence: f32,
}

impl Default for AlbedoSettings {
    fn default() -> Self {
        Self {
            base_color: default_base_colour_value(),
            warm_color: default_warm_colour_value(),
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

fn default_frequency() -> u32 {
    1
}

fn default_frequency_f32() -> f32 {
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

fn default_input_min() -> f32 {
    -1.0
}

fn default_input_max() -> f32 {
    1.0
}

fn default_contrast() -> f32 {
    1.0
}

fn default_height_strength() -> f32 {
    0.01
}

fn default_albedo_strength() -> f32 {
    0.3
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

fn default_base_colour_value() -> ColourValue {
    ColourValue::Literal(LinearRgb(default_base_color()))
}

fn default_warm_colour_value() -> ColourValue {
    ColourValue::Literal(LinearRgb(default_warm_color()))
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
