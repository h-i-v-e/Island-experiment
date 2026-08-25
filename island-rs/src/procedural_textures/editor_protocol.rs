//! JSON protocol and schema metadata for editor-facing baker commands.

#![allow(
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    clippy::match_same_arms
)]

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};

use super::{RecipeValidationError, RecipeValidationErrors, recipe::TextureRecipe};

/// One machine-readable editor diagnostic.
#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub pointer: String,
    pub severity: &'static str,
    pub code: &'static str,
    pub message: String,
}

/// Common envelope returned by editor-facing commands.
#[derive(Clone, Debug, Default, Serialize)]
pub struct EditorEnvelope {
    pub success: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub recipe_hash: Option<String>,
    #[serde(default)]
    pub generated_maps: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(default)]
    pub timings_ms: BTreeMap<String, f64>,
    #[serde(default)]
    pub timings: BTreeMap<String, f64>,
}

impl EditorEnvelope {
    #[must_use]
    pub fn success() -> Self {
        Self {
            success: true,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn failure(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            success: false,
            diagnostics: vec![Diagnostic {
                pointer: String::new(),
                severity: "error",
                code,
                message: message.into(),
            }],
            ..Self::default()
        }
    }

    /// Serializes the envelope for stdout.
    ///
    /// # Errors
    ///
    /// Returns a serde error if a future envelope field cannot be serialized.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Rust-owned metadata for one editable property.
#[derive(Clone, Debug, Serialize)]
pub struct PropertyMetadata {
    pub pointer: &'static str,
    pub label: &'static str,
    pub tooltip: &'static str,
}

/// Every editor property has a label and tooltip in this table.
pub const EDITABLE_METADATA: &[PropertyMetadata] = &[
    PropertyMetadata {
        pointer: "/name",
        label: "Name",
        tooltip: "Safe generated texture name",
    },
    PropertyMetadata {
        pointer: "/seed",
        label: "Seed",
        tooltip: "Deterministic root seed",
    },
    PropertyMetadata {
        pointer: "/width",
        label: "Width",
        tooltip: "Output width in pixels",
    },
    PropertyMetadata {
        pointer: "/height",
        label: "Height",
        tooltip: "Output height in pixels",
    },
    PropertyMetadata {
        pointer: "/physical_tile_width_m",
        label: "Tile width",
        tooltip: "Physical tile width",
    },
    PropertyMetadata {
        pointer: "/physical_tile_height_m",
        label: "Tile height",
        tooltip: "Physical tile height",
    },
    PropertyMetadata {
        pointer: "/normal_scale",
        label: "Normal scale",
        tooltip: "Tangent relief multiplier",
    },
    PropertyMetadata {
        pointer: "/normal_convention",
        label: "Normal convention",
        tooltip: "Tangent-space green-channel convention",
    },
    PropertyMetadata {
        pointer: "/material",
        label: "Material",
        tooltip: "Specialised base material",
    },
    PropertyMetadata {
        pointer: "/material/kind",
        label: "Material kind",
        tooltip: "Specialised base height generator",
    },
    PropertyMetadata {
        pointer: "/material/frequency",
        label: "Base frequency",
        tooltip: "Layered base cells per tile",
    },
    PropertyMetadata {
        pointer: "/material/amplitude",
        label: "Base amplitude",
        tooltip: "Layered base height amplitude",
    },
    PropertyMetadata {
        pointer: "/material/octaves",
        label: "Base octaves",
        tooltip: "Layered base fractal octave count",
    },
    PropertyMetadata {
        pointer: "/material/lacunarity",
        label: "Base lacunarity",
        tooltip: "Layered base frequency multiplier",
    },
    PropertyMetadata {
        pointer: "/material/gain",
        label: "Base gain",
        tooltip: "Layered base amplitude multiplier",
    },
    PropertyMetadata {
        pointer: "/material/offset",
        label: "Base offset",
        tooltip: "Layered base height offset",
    },
    PropertyMetadata {
        pointer: "/material/cells_x",
        label: "Cells X",
        tooltip: "Material cell count along X",
    },
    PropertyMetadata {
        pointer: "/material/cells_y",
        label: "Cells Y",
        tooltip: "Material cell count along Y",
    },
    PropertyMetadata {
        pointer: "/material/cell_jitter",
        label: "Cell jitter",
        tooltip: "Material feature-point jitter",
    },
    PropertyMetadata {
        pointer: "/material/warp_amplitude",
        label: "Warp amplitude",
        tooltip: "Material coordinate warp amplitude",
    },
    PropertyMetadata {
        pointer: "/material/crack_width",
        label: "Crack width",
        tooltip: "Cracked-stone crack width",
    },
    PropertyMetadata {
        pointer: "/material/shoulder_width",
        label: "Shoulder width",
        tooltip: "Cracked-stone shoulder width",
    },
    PropertyMetadata {
        pointer: "/material/crack_depth",
        label: "Crack depth",
        tooltip: "Cracked-stone crack depth",
    },
    PropertyMetadata {
        pointer: "/material/slab_variation",
        label: "Slab variation",
        tooltip: "Cracked-stone slab variation",
    },
    PropertyMetadata {
        pointer: "/material/fracture_probability",
        label: "Fracture probability",
        tooltip: "Cracked-stone fracture probability",
    },
    PropertyMetadata {
        pointer: "/material/fracture_depth",
        label: "Fracture depth",
        tooltip: "Cracked-stone fracture depth",
    },
    PropertyMetadata {
        pointer: "/material/surface_amplitude",
        label: "Surface amplitude",
        tooltip: "Cracked-stone surface amplitude",
    },
    PropertyMetadata {
        pointer: "/material/broad_variation",
        label: "Broad variation",
        tooltip: "Cracked-stone broad variation",
    },
    PropertyMetadata {
        pointer: "/material/stone_radius",
        label: "Pebble size",
        tooltip: "Cell-relative pebble size; lower values leave wider gaps",
    },
    PropertyMetadata {
        pointer: "/material/anisotropy",
        label: "Dome roundness",
        tooltip: "Rounded Voronoi pebble profile exponent",
    },
    PropertyMetadata {
        pointer: "/material/stone_height",
        label: "Stone height",
        tooltip: "Rounded-stone height",
    },
    PropertyMetadata {
        pointer: "/material/stone_variation",
        label: "Stone variation",
        tooltip: "Rounded-stone height variation",
    },
    PropertyMetadata {
        pointer: "/material/gap_height",
        label: "Gap height",
        tooltip: "Rounded-stone gap height",
    },
    PropertyMetadata {
        pointer: "/material/sand_amplitude",
        label: "Sand amplitude",
        tooltip: "Rounded-stone sand variation",
    },
    PropertyMetadata {
        pointer: "/material/edge_softness",
        label: "Edge softness",
        tooltip: "Rounded-stone edge softness",
    },
    PropertyMetadata {
        pointer: "/layers",
        label: "Layers",
        tooltip: "Ordered material layer stack",
    },
    PropertyMetadata {
        pointer: "/layers/*/source",
        label: "Source",
        tooltip: "Layer scalar source",
    },
    PropertyMetadata {
        pointer: "/layers/*/remap",
        label: "Remap",
        tooltip: "Layer scalar remapping",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask",
        label: "Mask",
        tooltip: "Layer opacity mask",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs",
        label: "Outputs",
        tooltip: "Height and albedo routing",
    },
    PropertyMetadata {
        pointer: "/displacement",
        label: "Displacement",
        tooltip: "Physical height range",
    },
    PropertyMetadata {
        pointer: "/displacement/minimum_m",
        label: "Minimum displacement",
        tooltip: "Minimum represented height",
    },
    PropertyMetadata {
        pointer: "/displacement/maximum_m",
        label: "Maximum displacement",
        tooltip: "Maximum represented height",
    },
    PropertyMetadata {
        pointer: "/displacement/base_m",
        label: "Base displacement",
        tooltip: "Neutral represented height",
    },
    PropertyMetadata {
        pointer: "/displacement/displacement_map",
        label: "Displacement map",
        tooltip: "Mark height output for displacement",
    },
    PropertyMetadata {
        pointer: "/occlusion",
        label: "Occlusion",
        tooltip: "Material-local AO settings",
    },
    PropertyMetadata {
        pointer: "/occlusion/directions",
        label: "AO directions",
        tooltip: "Fixed horizon directions",
    },
    PropertyMetadata {
        pointer: "/occlusion/samples",
        label: "AO samples",
        tooltip: "Samples per direction",
    },
    PropertyMetadata {
        pointer: "/occlusion/radius",
        label: "AO radius",
        tooltip: "Occlusion lookup radius",
    },
    PropertyMetadata {
        pointer: "/occlusion/max_radius",
        label: "AO maximum radius",
        tooltip: "Maximum allowed AO lookup radius",
    },
    PropertyMetadata {
        pointer: "/occlusion/cavity_strength",
        label: "Cavity strength",
        tooltip: "Cavity openness response",
    },
    PropertyMetadata {
        pointer: "/occlusion/horizon_strength",
        label: "Horizon strength",
        tooltip: "Horizon openness response",
    },
    PropertyMetadata {
        pointer: "/occlusion/power",
        label: "AO power",
        tooltip: "Occlusion response power",
    },
    PropertyMetadata {
        pointer: "/occlusion/combine/kind",
        label: "AO combine",
        tooltip: "Cavity and horizon combination",
    },
    PropertyMetadata {
        pointer: "/occlusion/combine/cavity_weight",
        label: "Cavity weight",
        tooltip: "Cavity contribution to weighted AO",
    },
    PropertyMetadata {
        pointer: "/occlusion/combine/horizon_weight",
        label: "Horizon weight",
        tooltip: "Horizon contribution to weighted AO",
    },
    PropertyMetadata {
        pointer: "/albedo",
        label: "Albedo",
        tooltip: "Base linear colour pass",
    },
    PropertyMetadata {
        pointer: "/albedo/base_color",
        label: "Base colour",
        tooltip: "Base linear RGB colour",
    },
    PropertyMetadata {
        pointer: "/albedo/warm_color",
        label: "Warm colour",
        tooltip: "Warm linear RGB colour",
    },
    PropertyMetadata {
        pointer: "/albedo/palette",
        label: "Palette",
        tooltip: "Optional base linear RGB palette",
    },
    PropertyMetadata {
        pointer: "/albedo/variation",
        label: "Variation",
        tooltip: "Base colour variation amount",
    },
    PropertyMetadata {
        pointer: "/albedo/crack_darkening",
        label: "Crack darkening",
        tooltip: "Darkening applied at cracks",
    },
    PropertyMetadata {
        pointer: "/albedo/shoulder_variation",
        label: "Shoulder variation",
        tooltip: "Variation around crack shoulders",
    },
    PropertyMetadata {
        pointer: "/albedo/mineral_density",
        label: "Mineral density",
        tooltip: "Mineral fleck density",
    },
    PropertyMetadata {
        pointer: "/albedo/mineral_brightness",
        label: "Mineral brightness",
        tooltip: "Mineral fleck brightness",
    },
    PropertyMetadata {
        pointer: "/albedo/occlusion_influence",
        label: "AO influence",
        tooltip: "Optional AO influence on base albedo",
    },
    PropertyMetadata {
        pointer: "/output_profiles",
        label: "Output profiles",
        tooltip: "Requested output encodings",
    },
    PropertyMetadata {
        pointer: "/layers/*/id",
        label: "Layer ID",
        tooltip: "Stable mask reference ID",
    },
    PropertyMetadata {
        pointer: "/layers/*/name",
        label: "Layer name",
        tooltip: "Artist-facing layer name",
    },
    PropertyMetadata {
        pointer: "/layers/*/enabled",
        label: "Enabled",
        tooltip: "Whether this layer contributes",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/kind",
        label: "Source",
        tooltip: "Scalar noise source kind",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/frequency",
        label: "Frequency",
        tooltip: "Whole cells per tile",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/octaves",
        label: "Octaves",
        tooltip: "Fractal octave count",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/lacunarity",
        label: "Lacunarity",
        tooltip: "Fractal frequency multiplier",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/gain",
        label: "Gain",
        tooltip: "Fractal amplitude multiplier",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/offset",
        label: "Offset",
        tooltip: "Tile-space source offset",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/seed_domain",
        label: "Seed domain",
        tooltip: "Independent source domain",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/cellular_jitter",
        label: "Cell jitter",
        tooltip: "Cellular feature jitter",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/domain_warp",
        label: "Domain warp",
        tooltip: "Periodic coordinate warp",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/domain_warp/amplitude",
        label: "Warp amplitude",
        tooltip: "Coordinate displacement",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/domain_warp/frequency",
        label: "Warp frequency",
        tooltip: "Warp cells per tile",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/domain_warp/octaves",
        label: "Warp octaves",
        tooltip: "Warp fractal octave count",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/domain_warp/lacunarity",
        label: "Warp lacunarity",
        tooltip: "Warp frequency multiplier",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/domain_warp/gain",
        label: "Warp gain",
        tooltip: "Warp amplitude multiplier",
    },
    PropertyMetadata {
        pointer: "/layers/*/source/domain_warp/seed_domain",
        label: "Warp seed domain",
        tooltip: "Independent warp random domain",
    },
    PropertyMetadata {
        pointer: "/layers/*/remap/input_min",
        label: "Input minimum",
        tooltip: "Selected raw range minimum",
    },
    PropertyMetadata {
        pointer: "/layers/*/remap/input_max",
        label: "Input maximum",
        tooltip: "Selected raw range maximum",
    },
    PropertyMetadata {
        pointer: "/layers/*/remap/invert",
        label: "Invert",
        tooltip: "Invert remapped scalar",
    },
    PropertyMetadata {
        pointer: "/layers/*/remap/contrast",
        label: "Contrast",
        tooltip: "Remap contrast",
    },
    PropertyMetadata {
        pointer: "/layers/*/remap/bias",
        label: "Bias",
        tooltip: "Remap bias",
    },
    PropertyMetadata {
        pointer: "/layers/*/remap/clamp",
        label: "Clamp",
        tooltip: "Clamp remapped scalar",
    },
    PropertyMetadata {
        pointer: "/layers/*/remap/curve",
        label: "Curve",
        tooltip: "Optional monotonic remap curve",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/height/enabled",
        label: "Height enabled",
        tooltip: "Route layer into height",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/height/strength_m",
        label: "Height strength",
        tooltip: "Physical displacement strength",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/height/blend",
        label: "Height blend",
        tooltip: "Height accumulation operation",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/height/blend/kind",
        label: "Height blend kind",
        tooltip: "Height accumulation operation",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/height/blend/amount",
        label: "Height lerp amount",
        tooltip: "Interpolation amount for lerp height blending",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/enabled",
        label: "Albedo enabled",
        tooltip: "Route layer into albedo",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/strength",
        label: "Albedo strength",
        tooltip: "Albedo opacity",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/blend",
        label: "Albedo blend",
        tooltip: "Albedo accumulation operation",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/colour_map",
        label: "Colour map",
        tooltip: "Scalar to linear RGB mapping",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/colour_map/kind",
        label: "Colour map kind",
        tooltip: "Two-colour ramp or multi-stop gradient",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/colour_map/first",
        label: "Ramp first colour",
        tooltip: "First linear-RGB ramp colour",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/colour_map/second",
        label: "Ramp second colour",
        tooltip: "Second linear-RGB ramp colour",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/colour_map/stops",
        label: "Gradient stops",
        tooltip: "Ordered linear-RGB gradient stops",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/colour_map/stops/*/position",
        label: "Gradient position",
        tooltip: "Normalized gradient stop position",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/colour_map/stops/*/colour",
        label: "Gradient colour",
        tooltip: "Linear-RGB gradient stop colour",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/hue_influence",
        label: "Hue influence",
        tooltip: "Hue variation applied to mapped colour",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/saturation_influence",
        label: "Saturation influence",
        tooltip: "Saturation variation applied to mapped colour",
    },
    PropertyMetadata {
        pointer: "/layers/*/outputs/albedo/value_influence",
        label: "Value influence",
        tooltip: "Value variation applied to mapped colour",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/kind",
        label: "Mask kind",
        tooltip: "Own, inline-noise or earlier-layer opacity mask",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/layer_id",
        label: "Mask layer",
        tooltip: "Earlier layer stable ID",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/source",
        label: "Mask source",
        tooltip: "Inline scalar mask source",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/remap",
        label: "Mask remap",
        tooltip: "Mask scalar remapping",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/remap/input_min",
        label: "Mask input minimum",
        tooltip: "Mask raw range minimum",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/remap/input_max",
        label: "Mask input maximum",
        tooltip: "Mask raw range maximum",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/remap/invert",
        label: "Mask invert",
        tooltip: "Invert mask remapping",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/remap/contrast",
        label: "Mask contrast",
        tooltip: "Mask remap contrast",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/remap/bias",
        label: "Mask bias",
        tooltip: "Mask remap bias",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/remap/clamp",
        label: "Mask clamp",
        tooltip: "Clamp mask remapping",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/remap/curve",
        label: "Mask curve",
        tooltip: "Optional monotonic mask remap curve",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/source/kind",
        label: "Mask source kind",
        tooltip: "Inline mask noise source kind",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/source/frequency",
        label: "Mask frequency",
        tooltip: "Inline mask cells per tile",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/source/octaves",
        label: "Mask octaves",
        tooltip: "Inline mask fractal octave count",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/source/lacunarity",
        label: "Mask lacunarity",
        tooltip: "Inline mask frequency multiplier",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/source/gain",
        label: "Mask gain",
        tooltip: "Inline mask amplitude multiplier",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/source/offset",
        label: "Mask offset",
        tooltip: "Inline mask tile-space offset",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/source/seed_domain",
        label: "Mask seed domain",
        tooltip: "Inline mask independent seed domain",
    },
    PropertyMetadata {
        pointer: "/layers/*/mask/source/cellular_jitter",
        label: "Mask cell jitter",
        tooltip: "Inline mask cellular feature jitter",
    },
];

/// Returns the generated schema and the Rust UI metadata table.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn schema_document() -> Value {
    let metadata = EDITABLE_METADATA
        .iter()
        .map(metadata_value)
        .collect::<Vec<_>>();
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Procedural Material Recipe",
        "type": "object",
        "additionalProperties": false,
        "required": [
            "name", "seed", "width", "height", "physical_tile_width_m",
            "physical_tile_height_m", "material", "layers", "normal_convention",
            "normal_scale", "displacement", "occlusion", "albedo", "output_profiles"
        ],
        "properties": {
            "name": {"type": "string"},
            "seed": {"type": "integer", "minimum": 0},
            "width": {"type": "integer", "minimum": 1},
            "height": {"type": "integer", "minimum": 1},
            "physical_tile_width_m": {"type": "number", "exclusiveMinimum": 0},
            "physical_tile_height_m": {"type": "number", "exclusiveMinimum": 0},
            "material": {"$ref": "#/$defs/material"},
            "layers": {"type": "array", "items": {"$ref": "#/$defs/layer"}},
            "normal_convention": {"enum": ["open_gl", "direct_x"]},
            "normal_scale": {"type": "number", "minimum": 0},
            "displacement": {"$ref": "#/$defs/displacement"},
            "occlusion": {"$ref": "#/$defs/occlusion"},
            "albedo": {"$ref": "#/$defs/albedo"},
            "output_profiles": {"type": "array", "items": {"enum": ["separate", "motu_unity_terrain"]}},
        },
        "$defs": {
            "source": {
                "type": "object",
                "additionalProperties": false,
                "required": ["kind"],
                "properties": {
                    "kind": {"enum": [
                        "value", "fbm", "billow", "ridged", "cellular_distance",
                        "cellular_distance_to_edge", "cellular_value"
                    ]},
                    "frequency": {"type": "integer", "minimum": 1},
                    "octaves": {"type": "integer", "minimum": 1, "maximum": 16},
                    "lacunarity": {"type": "number", "exclusiveMinimum": 0},
                    "gain": {"type": "number"},
                    "offset": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}},
                    "seed_domain": {"type": "integer", "minimum": 0},
                    "cellular_jitter": {"type": "number", "minimum": 0, "maximum": 1},
                    "domain_warp": {"oneOf": [{"type": "null"}, {"$ref": "#/$defs/domain_warp"}]},
                },
            },
            "domain_warp": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "amplitude": {"type": "number", "minimum": 0},
                    "frequency": {"type": "integer", "minimum": 1},
                    "octaves": {"type": "integer", "minimum": 1, "maximum": 16},
                    "lacunarity": {"type": "number", "exclusiveMinimum": 0},
                    "gain": {"type": "number"},
                    "seed_domain": {"type": "integer", "minimum": 0},
                },
            },
            "remap_point": {
                "type": "object",
                "additionalProperties": false,
                "required": ["position", "value"],
                "properties": {
                    "position": {"type": "number", "minimum": 0, "maximum": 1},
                    "value": {"type": "number"},
                },
            },
            "remap": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "input_min": {"type": "number"},
                    "input_max": {"type": "number"},
                    "invert": {"type": "boolean"},
                    "contrast": {"type": "number"},
                    "bias": {"type": "number"},
                    "clamp": {"type": "boolean"},
                    "curve": {"type": "array", "maxItems": 16, "items": {"$ref": "#/$defs/remap_point"}},
                },
            },
            "mask": {
                "oneOf": [
                    {"type": "null"},
                    {"type": "object", "additionalProperties": false, "required": ["kind"], "properties": {"kind": {"const": "own"}}},
                    {"type": "object", "additionalProperties": false, "required": ["kind", "source"], "properties": {"kind": {"const": "noise"}, "source": {"$ref": "#/$defs/source"}, "remap": {"$ref": "#/$defs/remap"}}},
                    {"type": "object", "additionalProperties": false, "required": ["kind", "layer_id"], "properties": {"kind": {"const": "layer"}, "layer_id": {"type": "string"}, "remap": {"$ref": "#/$defs/remap"}}}
                ]
            },
            "height_blend": {
                "oneOf": [
                    {"type": "object", "additionalProperties": false, "required": ["kind"], "properties": {"kind": {"enum": ["replace", "add", "subtract", "multiply", "minimum", "maximum"]}}},
                    {"type": "object", "additionalProperties": false, "required": ["kind", "amount"], "properties": {"kind": {"const": "lerp"}, "amount": {"type": "number", "minimum": 0, "maximum": 1}}}
                ]
            },
            "colour": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number", "minimum": 0, "maximum": 1}},
            "gradient_stop": {
                "type": "object",
                "additionalProperties": false,
                "required": ["position", "colour"],
                "properties": {"position": {"type": "number", "minimum": 0, "maximum": 1}, "colour": {"$ref": "#/$defs/colour"}}
            },
            "colour_map": {
                "oneOf": [
                    {"type": "object", "additionalProperties": false, "required": ["kind", "first", "second"], "properties": {"kind": {"const": "ramp"}, "first": {"$ref": "#/$defs/colour"}, "second": {"$ref": "#/$defs/colour"}}},
                    {"type": "object", "additionalProperties": false, "required": ["kind", "stops"], "properties": {"kind": {"const": "gradient"}, "stops": {"type": "array", "minItems": 2, "maxItems": 32, "items": {"$ref": "#/$defs/gradient_stop"}}}}
                ]
            },
            "layer": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "name", "source", "outputs"],
                "properties": {
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "enabled": {"type": "boolean"},
                    "source": {"$ref": "#/$defs/source"},
                    "remap": {"$ref": "#/$defs/remap"},
                    "mask": {"$ref": "#/$defs/mask"},
                    "outputs": {"$ref": "#/$defs/outputs"},
                },
            },
            "outputs": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "height": {"$ref": "#/$defs/height_output"},
                    "albedo": {"$ref": "#/$defs/albedo_output"},
                },
            },
            "height_output": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "enabled": {"type": "boolean"},
                    "blend": {"$ref": "#/$defs/height_blend"},
                    "strength_m": {"type": "number"},
                },
            },
            "albedo_output": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "enabled": {"type": "boolean"},
                    "blend": {"enum": ["replace", "mix", "multiply", "add", "overlay"]},
                    "strength": {"type": "number", "minimum": 0, "maximum": 1},
                    "colour_map": {"$ref": "#/$defs/colour_map"},
                    "hue_influence": {"type": "number", "minimum": -1, "maximum": 1},
                    "saturation_influence": {"type": "number", "minimum": -1, "maximum": 1},
                    "value_influence": {"type": "number", "minimum": -1, "maximum": 1},
                },
            },
            "material": {
                "oneOf": [
                    {"$ref": "#/$defs/layered_noise_material"},
                    {"$ref": "#/$defs/cracked_stone_material"},
                    {"$ref": "#/$defs/rounded_stones_material"}
                ]
            },
            "layered_noise_material": {"type": "object", "additionalProperties": false, "required": ["kind"], "properties": {"kind": {"const": "layered_noise"}, "frequency": {"type": "number", "exclusiveMinimum": 0}, "amplitude": {"type": "number"}, "octaves": {"type": "integer", "minimum": 1, "maximum": 16}, "lacunarity": {"type": "number", "exclusiveMinimum": 0}, "gain": {"type": "number"}, "offset": {"type": "number"}}},
            "cracked_stone_material": {"type": "object", "additionalProperties": false, "required": ["kind"], "properties": {"kind": {"const": "cracked_stone"}, "cells_x": {"type": "integer", "minimum": 1}, "cells_y": {"type": "integer", "minimum": 1}, "cell_jitter": {"type": "number", "minimum": 0, "maximum": 1}, "warp_amplitude": {"type": "number", "minimum": 0}, "crack_width": {"type": "number", "minimum": 0}, "shoulder_width": {"type": "number", "minimum": 0}, "crack_depth": {"type": "number", "minimum": 0}, "slab_variation": {"type": "number", "minimum": 0}, "fracture_probability": {"type": "number", "minimum": 0, "maximum": 1}, "fracture_depth": {"type": "number", "minimum": 0}, "surface_amplitude": {"type": "number", "minimum": 0}, "broad_variation": {"type": "number", "minimum": 0}}},
            "rounded_stones_material": {"type": "object", "additionalProperties": false, "required": ["kind"], "properties": {"kind": {"const": "rounded_stones"}, "cells_x": {"type": "integer", "minimum": 1}, "cells_y": {"type": "integer", "minimum": 1}, "stone_radius": {"type": "number", "exclusiveMinimum": 0}, "cell_jitter": {"type": "number", "minimum": 0, "maximum": 1}, "warp_amplitude": {"type": "number", "minimum": 0}, "anisotropy": {"type": "number", "exclusiveMinimum": 0}, "stone_height": {"type": "number", "minimum": 0}, "stone_variation": {"type": "number", "minimum": 0}, "gap_height": {"type": "number"}, "sand_amplitude": {"type": "number", "minimum": 0}, "edge_softness": {"type": "number", "minimum": 0}}},
            "displacement": {"type": "object", "additionalProperties": false, "required": ["minimum_m", "maximum_m", "base_m"], "properties": {"minimum_m": {"type": "number"}, "maximum_m": {"type": "number"}, "base_m": {"type": "number"}, "displacement_map": {"type": "boolean"}}},
            "occlusion": {"type": "object", "additionalProperties": false, "properties": {"directions": {"type": "integer", "minimum": 1, "maximum": 32}, "samples": {"type": "integer", "minimum": 1, "maximum": 16}, "radius": {"type": "number", "minimum": 0}, "max_radius": {"type": "number", "minimum": 0}, "cavity_strength": {"type": "number", "minimum": 0}, "horizon_strength": {"type": "number", "minimum": 0}, "power": {"type": "number", "exclusiveMinimum": 0}, "combine": {"oneOf": [{"type": "object", "additionalProperties": false, "required": ["kind"], "properties": {"kind": {"const": "multiply"}}}, {"type": "object", "additionalProperties": false, "required": ["kind"], "properties": {"kind": {"const": "weighted_minimum"}, "cavity_weight": {"type": "number", "minimum": 0, "maximum": 1}, "horizon_weight": {"type": "number", "minimum": 0, "maximum": 1}}}]}}},
            "albedo": {"type": "object", "additionalProperties": false, "properties": {"base_color": {"$ref": "#/$defs/colour"}, "warm_color": {"$ref": "#/$defs/colour"}, "palette": {"type": "array", "items": {"$ref": "#/$defs/colour"}}, "variation": {"type": "number", "minimum": 0}, "crack_darkening": {"type": "number", "minimum": 0}, "shoulder_variation": {"type": "number", "minimum": 0}, "mineral_density": {"type": "number", "minimum": 0}, "mineral_brightness": {"type": "number", "minimum": 0}, "occlusion_influence": {"type": "number", "minimum": 0}}}
        },
        "metadata": metadata,
    })
}

#[allow(clippy::too_many_lines)]
fn metadata_value(item: &PropertyMetadata) -> Value {
    let (default, range, units) = match item.pointer {
        "/name" => (json!("ProceduralTexture"), None, None),
        "/seed" | "/layers/*/source/seed_domain" | "/layers/*/mask/source/seed_domain" => {
            (json!(0), Some([0.0, u64::MAX as f64]), None)
        }
        "/width" | "/height" => (json!(256), Some([1.0, u32::MAX as f64]), Some("pixels")),
        "/physical_tile_width_m" | "/physical_tile_height_m" => {
            (json!(1.0), Some([f64::EPSILON, f32::MAX as f64]), Some("m"))
        }
        "/normal_scale" => (json!(1.0), Some([0.0, f32::MAX as f64]), None),
        "/normal_convention" => (json!("open_gl"), None, None),
        "/layers/*/id" => (json!("layer"), None, None),
        "/layers/*/name" => (json!("Layer"), None, None),
        "/layers/*/enabled" => (json!(true), None, None),
        "/material/kind" => (json!("layered_noise"), None, None),
        "/material/frequency" => (
            json!(1.0),
            Some([f64::EPSILON, f32::MAX as f64]),
            Some("cells/tile"),
        ),
        "/material/amplitude" => (json!(1.0), Some([-f32::MAX as f64, f32::MAX as f64]), None),
        "/material/octaves" => (json!(4), Some([1.0, 16.0]), None),
        "/material/lacunarity" => (json!(2.0), Some([f64::EPSILON, f32::MAX as f64]), None),
        "/material/gain" => (json!(0.5), Some([-f32::MAX as f64, f32::MAX as f64]), None),
        "/material/offset" => (
            json!(0.0),
            Some([-f32::MAX as f64, f32::MAX as f64]),
            Some("m"),
        ),
        "/material/cells_x" | "/material/cells_y" => {
            (json!(8), Some([1.0, u32::MAX as f64]), Some("cells"))
        }
        "/material/cell_jitter" => (json!(0.25), Some([0.0, 1.0]), None),
        "/material/warp_amplitude" => (json!(0.15), Some([0.0, f32::MAX as f64]), None),
        "/material/crack_width" => (json!(0.035), Some([0.0, f32::MAX as f64]), None),
        "/material/shoulder_width" => (json!(0.18), Some([0.0, f32::MAX as f64]), None),
        "/material/crack_depth" => (json!(0.13), Some([0.0, f32::MAX as f64]), None),
        "/material/slab_variation" => (json!(0.035), Some([0.0, f32::MAX as f64]), None),
        "/material/fracture_probability" => (json!(0.28), Some([0.0, 1.0]), None),
        "/material/fracture_depth" => (json!(0.045), Some([0.0, f32::MAX as f64]), None),
        "/material/surface_amplitude" => (json!(0.014), Some([0.0, f32::MAX as f64]), None),
        "/material/broad_variation" => (json!(0.018), Some([0.0, f32::MAX as f64]), None),
        "/material/stone_radius" => (json!(0.36), Some([f64::EPSILON, f32::MAX as f64]), None),
        "/material/anisotropy" => (json!(1.0), Some([f64::EPSILON, f32::MAX as f64]), None),
        "/material/stone_height" => (json!(0.12), Some([0.0, f32::MAX as f64]), None),
        "/material/stone_variation" => (json!(0.045), Some([0.0, f32::MAX as f64]), None),
        "/material/gap_height" => (
            json!(-0.012),
            Some([-f32::MAX as f64, f32::MAX as f64]),
            None,
        ),
        "/material/sand_amplitude" => (json!(0.009), Some([0.0, f32::MAX as f64]), None),
        "/material/edge_softness" => (json!(0.08), Some([0.0, f32::MAX as f64]), None),
        "/layers" => (json!([]), None, None),
        "/layers/*/source/kind" | "/layers/*/mask/source/kind" => (json!("value"), None, None),
        "/layers/*/source/frequency" | "/layers/*/mask/source/frequency" => {
            (json!(1), Some([1.0, u32::MAX as f64]), Some("cells/tile"))
        }
        "/layers/*/source/octaves" | "/layers/*/mask/source/octaves" => {
            (json!(4), Some([1.0, 16.0]), None)
        }
        "/layers/*/source/lacunarity" | "/layers/*/mask/source/lacunarity" => {
            (json!(2.0), Some([f64::EPSILON, f32::MAX as f64]), None)
        }
        "/layers/*/source/gain" | "/layers/*/mask/source/gain" => {
            (json!(0.5), Some([-f32::MAX as f64, f32::MAX as f64]), None)
        }
        "/layers/*/source/offset" | "/layers/*/mask/source/offset" => {
            (json!([0.0, 0.0]), None, Some("tile"))
        }
        "/layers/*/source/cellular_jitter" | "/layers/*/mask/source/cellular_jitter" => {
            (json!(0.25), Some([0.0, 1.0]), None)
        }
        "/layers/*/source/domain_warp" => (Value::Null, None, None),
        "/layers/*/source/domain_warp/amplitude" => {
            (json!(0.15), Some([0.0, f32::MAX as f64]), None)
        }
        "/layers/*/source/domain_warp/frequency" => {
            (json!(1), Some([1.0, u32::MAX as f64]), Some("cells/tile"))
        }
        "/layers/*/source/domain_warp/octaves" => (json!(3), Some([1.0, 16.0]), None),
        "/layers/*/source/domain_warp/lacunarity" => {
            (json!(2.0), Some([f64::EPSILON, f32::MAX as f64]), None)
        }
        "/layers/*/source/domain_warp/gain" => {
            (json!(0.5), Some([-f32::MAX as f64, f32::MAX as f64]), None)
        }
        "/layers/*/source/domain_warp/seed_domain" => {
            (json!(0), Some([0.0, u64::MAX as f64]), None)
        }
        "/layers/*/remap/input_min" | "/layers/*/mask/remap/input_min" => {
            (json!(-1.0), Some([-f32::MAX as f64, f32::MAX as f64]), None)
        }
        "/layers/*/remap/input_max" | "/layers/*/mask/remap/input_max" => {
            (json!(1.0), Some([-f32::MAX as f64, f32::MAX as f64]), None)
        }
        "/layers/*/remap/invert" | "/layers/*/mask/remap/invert" => (json!(false), None, None),
        "/layers/*/remap/contrast" | "/layers/*/mask/remap/contrast" => {
            (json!(1.0), Some([0.0, f32::MAX as f64]), None)
        }
        "/layers/*/remap/bias" | "/layers/*/mask/remap/bias" => {
            (json!(0.0), Some([-f32::MAX as f64, f32::MAX as f64]), None)
        }
        "/layers/*/remap/clamp" | "/layers/*/mask/remap/clamp" => (json!(true), None, None),
        "/layers/*/remap/curve" | "/layers/*/mask/remap/curve" => (Value::Null, None, None),
        "/layers/*/outputs/height/enabled" => (json!(false), None, None),
        "/layers/*/outputs/height/strength_m" => (
            json!(0.01),
            Some([-f32::MAX as f64, f32::MAX as f64]),
            Some("m"),
        ),
        "/layers/*/outputs/height/blend" => (json!({"kind": "add"}), None, None),
        "/layers/*/outputs/height/blend/kind" => (json!("add"), None, None),
        "/layers/*/outputs/height/blend/amount" => (json!(0.5), Some([0.0, 1.0]), None),
        "/layers/*/outputs/albedo/enabled" => (json!(false), None, None),
        "/layers/*/outputs/albedo/strength" => (json!(0.3), Some([0.0, 1.0]), None),
        "/layers/*/outputs/albedo/blend" => (json!("mix"), None, None),
        "/layers/*/outputs/albedo/colour_map/kind" => (json!("ramp"), None, None),
        "/layers/*/outputs/albedo/colour_map/first" => {
            (json!([0.25, 0.27, 0.24]), Some([0.0, 1.0]), None)
        }
        "/layers/*/outputs/albedo/colour_map/second" => {
            (json!([0.42, 0.36, 0.28]), Some([0.0, 1.0]), None)
        }
        "/layers/*/outputs/albedo/colour_map/stops" => (json!([]), None, None),
        "/layers/*/outputs/albedo/colour_map/stops/*/position" => {
            (json!(0.0), Some([0.0, 1.0]), None)
        }
        "/layers/*/outputs/albedo/colour_map/stops/*/colour" => {
            (json!([0.25, 0.27, 0.24]), Some([0.0, 1.0]), None)
        }
        "/layers/*/outputs/albedo/hue_influence"
        | "/layers/*/outputs/albedo/saturation_influence"
        | "/layers/*/outputs/albedo/value_influence" => (json!(0.0), Some([-1.0, 1.0]), None),
        "/layers/*/mask/kind" => (json!("own"), None, None),
        "/layers/*/mask/layer_id" => (json!("layer"), None, None),
        "/layers/*/outputs/albedo/colour_map" => (
            json!({"kind": "ramp", "first": [0.25, 0.27, 0.24], "second": [0.42, 0.36, 0.28]}),
            Some([0.0, 1.0]),
            None,
        ),
        "/displacement/minimum_m" => (
            json!(-0.2),
            Some([-f32::MAX as f64, f32::MAX as f64]),
            Some("m"),
        ),
        "/displacement/maximum_m" => (
            json!(0.2),
            Some([-f32::MAX as f64, f32::MAX as f64]),
            Some("m"),
        ),
        "/displacement/base_m" => (
            json!(0.0),
            Some([-f32::MAX as f64, f32::MAX as f64]),
            Some("m"),
        ),
        "/displacement/displacement_map" => (json!(true), None, None),
        "/occlusion/directions" => (json!(8), Some([1.0, 32.0]), None),
        "/occlusion/samples" => (json!(6), Some([1.0, 16.0]), None),
        "/occlusion/radius" => (json!(1.0), Some([0.0, 4096.0]), Some("px")),
        "/occlusion/max_radius" => (json!(8.0), Some([0.0, 4096.0]), Some("px")),
        "/occlusion/cavity_strength" | "/occlusion/horizon_strength" => {
            (json!(1.0), Some([0.0, f32::MAX as f64]), None)
        }
        "/occlusion/power" => (json!(1.0), Some([f64::EPSILON, f32::MAX as f64]), None),
        "/occlusion/combine/kind" => (json!("multiply"), None, None),
        "/occlusion/combine/cavity_weight" | "/occlusion/combine/horizon_weight" => {
            (json!(0.5), Some([0.0, 1.0]), None)
        }
        "/albedo/base_color" => (json!([0.25, 0.27, 0.24]), Some([0.0, 1.0]), None),
        "/albedo/warm_color" => (json!([0.42, 0.36, 0.28]), Some([0.0, 1.0]), None),
        "/albedo/palette" => (json!([]), Some([0.0, 1.0]), None),
        "/albedo/variation" => (json!(0.5), Some([0.0, 1.0]), None),
        "/albedo/crack_darkening"
        | "/albedo/shoulder_variation"
        | "/albedo/mineral_density"
        | "/albedo/mineral_brightness"
        | "/albedo/occlusion_influence" => (json!(0.0), Some([0.0, 1.0]), None),
        "/output_profiles" => (json!(["separate"]), None, None),
        _ => (Value::Object(serde_json::Map::new()), None, None),
    };
    let range = range.map(|[minimum, maximum]| {
        json!({
            "minimum": minimum,
            "maximum": maximum,
        })
    });
    json!({
        "pointer": item.pointer,
        "label": item.label,
        "tooltip": item.tooltip,
        "type": metadata_type(item.pointer),
        "default": default,
        "range": range,
        "units": units,
        "enum": metadata_enum(item.pointer),
    })
}

fn metadata_type(pointer: &str) -> &'static str {
    match pointer {
        "/name"
        | "/layers/*/id"
        | "/layers/*/name"
        | "/layers/*/source/kind"
        | "/layers/*/mask/source/kind"
        | "/layers/*/outputs/albedo/blend"
        | "/normal_convention"
        | "/material/kind"
        | "/layers/*/mask/kind"
        | "/layers/*/mask/layer_id"
        | "/layers/*/outputs/height/blend/kind" => "string",
        "/seed"
        | "/width"
        | "/height"
        | "/layers/*/source/frequency"
        | "/layers/*/mask/source/frequency"
        | "/layers/*/source/octaves"
        | "/layers/*/mask/source/octaves"
        | "/layers/*/source/seed_domain"
        | "/layers/*/mask/source/seed_domain"
        | "/layers/*/source/domain_warp/frequency"
        | "/layers/*/source/domain_warp/octaves"
        | "/layers/*/source/domain_warp/seed_domain"
        | "/material/cells_x"
        | "/material/cells_y" => "integer",
        "/layers" | "/output_profiles" => "array",
        "/layers/*/enabled"
        | "/layers/*/remap/invert"
        | "/layers/*/remap/clamp"
        | "/layers/*/mask/remap/invert"
        | "/layers/*/mask/remap/clamp"
        | "/layers/*/outputs/height/enabled"
        | "/layers/*/outputs/albedo/enabled" => "boolean",
        "/layers/*/source/offset" | "/layers/*/mask/source/offset" => "array",
        "/layers/*/source"
        | "/layers/*/remap"
        | "/layers/*/mask"
        | "/layers/*/mask/remap"
        | "/layers/*/outputs"
        | "/layers/*/outputs/height/blend"
        | "/layers/*/outputs/albedo/colour_map"
        | "/layers/*/source/domain_warp"
        | "/layers/*/mask/source"
        | "/material"
        | "/displacement"
        | "/occlusion"
        | "/occlusion/combine"
        | "/albedo" => "object",
        "/layers/*/remap/curve" | "/layers/*/mask/remap/curve" => "array",
        "/layers/*/outputs/albedo/colour_map/first"
        | "/layers/*/outputs/albedo/colour_map/second" => "array",
        "/layers/*/outputs/albedo/colour_map/stops"
        | "/layers/*/outputs/albedo/colour_map/stops/*/colour"
        | "/albedo/base_color"
        | "/albedo/warm_color"
        | "/albedo/palette" => "array",
        "/displacement/displacement_map" => "boolean",
        "/occlusion/directions" | "/occlusion/samples" => "integer",
        _ => "number",
    }
}

fn metadata_enum(pointer: &str) -> Option<Value> {
    match pointer {
        "/normal_convention" => Some(json!(["open_gl", "direct_x"])),
        "/material/kind" => Some(json!(["layered_noise", "cracked_stone", "rounded_stones"])),
        "/output_profiles" => Some(json!(["separate", "motu_unity_terrain"])),
        "/layers/*/source/kind" | "/layers/*/mask/source/kind" => Some(json!([
            "value",
            "fbm",
            "billow",
            "ridged",
            "cellular_distance",
            "cellular_distance_to_edge",
            "cellular_value"
        ])),
        "/layers/*/outputs/height/blend/kind" => Some(json!([
            "replace", "add", "subtract", "multiply", "minimum", "maximum", "lerp"
        ])),
        "/layers/*/outputs/albedo/blend" => {
            Some(json!(["replace", "mix", "multiply", "add", "overlay"]))
        }
        "/layers/*/outputs/albedo/colour_map/kind" => Some(json!(["ramp", "gradient"])),
        "/layers/*/mask/kind" => Some(json!(["own", "noise", "layer"])),
        _ => None,
    }
}

/// Verifies metadata coverage for every root and layer editor section.
///
/// # Errors
///
/// Returns the JSON pointers that do not have a Rust-owned metadata entry.
pub fn metadata_coverage() -> Result<(), Vec<&'static str>> {
    let required = [
        "/name",
        "/seed",
        "/width",
        "/height",
        "/material",
        "/layers",
        "/layers/*/source",
        "/layers/*/remap",
        "/layers/*/mask",
        "/layers/*/outputs",
        "/normal_scale",
        "/displacement",
        "/occlusion",
        "/albedo",
        "/output_profiles",
    ];
    let missing = required
        .into_iter()
        .filter(|pointer| {
            !EDITABLE_METADATA
                .iter()
                .any(|item| item.pointer == *pointer)
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

/// Converts validation issues to stable JSON-pointer diagnostics.
#[must_use]
pub fn diagnostics_from_validation(errors: &RecipeValidationErrors) -> Vec<Diagnostic> {
    errors
        .issues()
        .iter()
        .map(|issue| {
            let (pointer, code) = issue_pointer_code(issue);
            Diagnostic {
                pointer,
                severity: "error",
                code,
                message: issue.to_string(),
            }
        })
        .collect()
}

/// Validates one parsed document and returns editor diagnostics.
#[must_use]
pub fn validate_diagnostics(recipe: &TextureRecipe) -> Vec<Diagnostic> {
    match super::validate_recipe(recipe) {
        Ok(()) => Vec::new(),
        Err(errors) => diagnostics_from_validation(&errors),
    }
}

fn issue_pointer_code(issue: &RecipeValidationError) -> (String, &'static str) {
    match issue {
        RecipeValidationError::EmptyName => ("/name".into(), "name.empty"),
        RecipeValidationError::InvalidName { .. } => ("/name".into(), "name.invalid"),
        RecipeValidationError::ZeroDimensions { .. } => ("/width".into(), "dimensions.zero"),
        RecipeValidationError::DimensionOverflow { .. } => ("/width".into(), "dimensions.overflow"),
        RecipeValidationError::NegativePhysicalTileSize { axis, .. }
        | RecipeValidationError::NonPositivePhysicalTileSize { axis, .. } => {
            (format!("/physical_tile_{axis}_m"), "tile_size.invalid")
        }
        RecipeValidationError::NonFinite { path } => (json_pointer(path), "number.non_finite"),
        RecipeValidationError::InvalidFrequency { path, .. } => {
            (json_pointer(path), "source.frequency")
        }
        RecipeValidationError::OctavesOutOfRange { path, .. } => {
            (json_pointer(path), "source.octaves")
        }
        RecipeValidationError::NegativeParameter { path, .. }
        | RecipeValidationError::NormalizedParameterOutOfRange { path, .. }
        | RecipeValidationError::NonPositiveParameter { path, .. } => {
            (json_pointer(path), "number.out_of_range")
        }
        RecipeValidationError::InvalidDisplacementRange { .. } => {
            ("/displacement".into(), "displacement.range")
        }
        RecipeValidationError::OcclusionDirectionsOutOfRange { .. } => {
            ("/occlusion/directions".into(), "occlusion.directions")
        }
        RecipeValidationError::OcclusionSamplesOutOfRange { .. } => {
            ("/occlusion/samples".into(), "occlusion.samples")
        }
        RecipeValidationError::OcclusionRadiusOutOfRange { path, .. } => {
            (json_pointer(path), "occlusion.radius")
        }
        RecipeValidationError::MissingOutputProfile => {
            ("/output_profiles".into(), "output_profiles.empty")
        }
        RecipeValidationError::TooManyOutputProfiles { .. } => {
            ("/output_profiles".into(), "output_profiles.too_many")
        }
        RecipeValidationError::TooManyLayers { .. } => ("/layers".into(), "layers.too_many"),
        RecipeValidationError::DuplicateLayerId { .. } => ("/layers".into(), "layers.duplicate_id"),
        RecipeValidationError::InvalidLayerId { path, .. }
        | RecipeValidationError::MissingLayerReference { path, .. }
        | RecipeValidationError::ForwardLayerReference { path, .. } => {
            (path.clone(), "layers.reference")
        }
        RecipeValidationError::DuplicateOutputProfile { .. } => {
            ("/output_profiles".into(), "output_profiles.duplicate")
        }
        RecipeValidationError::OutputNameCollision { .. } => ("/name".into(), "name.collision"),
        RecipeValidationError::InvalidColour { path, .. } => (json_pointer(path), "colour.invalid"),
        RecipeValidationError::InvalidOcclusionWeights { .. } => {
            ("/occlusion/combine".into(), "occlusion.weights")
        }
        RecipeValidationError::InvalidRemapRange { path, .. } => {
            (json_pointer(path), "remap.range")
        }
        RecipeValidationError::TooManyRemapPoints { path, .. }
        | RecipeValidationError::InvalidRemapCurve { path } => (json_pointer(path), "remap.curve"),
        RecipeValidationError::TooManyGradientStops { path, .. }
        | RecipeValidationError::InvalidGradient { path } => {
            (json_pointer(path), "colour.gradient")
        }
        RecipeValidationError::Image(_) => (String::new(), "image.invalid"),
    }
}

fn json_pointer(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    if path.starts_with('/') {
        return path.to_owned();
    }
    let mut pointer = String::new();
    let mut segment = String::new();
    let flush = |pointer: &mut String, segment: &mut String| {
        if !segment.is_empty() {
            pointer.push('/');
            pointer.push_str(segment);
            segment.clear();
        }
    };
    for character in path.chars() {
        match character {
            '.' => flush(&mut pointer, &mut segment),
            '[' => flush(&mut pointer, &mut segment),
            ']' => flush(&mut pointer, &mut segment),
            _ => segment.push(character),
        }
    }
    flush(&mut pointer, &mut segment);
    pointer
}

/// Computes a normalized recipe hash after validation.
///
/// # Errors
///
/// Returns validation text or a serialization error when hashing fails.
pub fn recipe_hash(recipe: &TextureRecipe) -> Result<String, String> {
    super::validate_recipe(recipe).map_err(|errors| errors.to_string())?;
    super::encoding::normalized_recipe_hash(recipe).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_metadata_has_required_entries() {
        assert!(metadata_coverage().is_ok());
        let schema = schema_document();
        assert_eq!(schema["properties"]["layers"]["type"], "array");
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|name| name == "layers"))
        );
        assert!(schema["metadata"][0].get("default").is_some());
    }

    #[test]
    fn diagnostics_use_json_pointers() {
        let (pointer, _) = issue_pointer_code(&RecipeValidationError::NonFinite {
            path: "material.frequency".into(),
        });
        assert_eq!(pointer, "/material/frequency");
        assert_eq!(json_pointer("albedo.palette[2]"), "/albedo/palette/2");
    }
}
