//! Structural and numeric validation for the current material document.

#![allow(clippy::missing_errors_doc, clippy::too_many_lines)]

use core::fmt;
use std::collections::{BTreeMap, HashMap, HashSet};

use super::{
    image::{ImageError, TextureDimensions},
    parameters::{
        ColourValue, LinearRgb, MAX_PARAMETER_NAME_LEN, MAX_RECIPE_PARAMETERS, ParameterDefinition,
    },
    recipe::{
        AlbedoSettings, CRACKED_STONE_MAX_WARP_AMPLITUDE, ColourMap, DisplacementSettings,
        DomainWarpSettings, GradientStop, HeightBlend, LayerMask, MAX_GRADIENT_STOPS, MAX_LAYERS,
        MAX_OUTPUT_PROFILES, MAX_REMAP_POINTS, MaterialLayer, MaterialModel, OcclusionCombine,
        OcclusionRecipeSettings, OutputProfile, ROUNDED_STONES_MAX_RADIUS,
        ROUNDED_STONES_MAX_WARP_AMPLITUDE, RemapPoint, ScalarRemap, ScalarSource, TextureRecipe,
    },
};

/// Maximum number of fractal octaves accepted by the recipe schema.
pub const MAX_OCTAVES: u8 = 16;
/// Maximum number of fixed AO directions.
pub const MAX_AO_DIRECTIONS: u8 = 32;
/// Maximum number of horizon samples per direction.
pub const MAX_AO_SAMPLES: u8 = 16;
/// Maximum radius for one local AO lookup in output pixels.
pub const MAX_AO_RADIUS: f32 = 4096.0;

/// One precise reason a recipe cannot be evaluated.
#[derive(Clone, Debug, PartialEq)]
pub enum RecipeValidationError {
    EmptyName,
    InvalidName {
        name: String,
    },
    ZeroDimensions {
        width: u32,
        height: u32,
    },
    DimensionOverflow {
        width: u32,
        height: u32,
    },
    NegativePhysicalTileSize {
        axis: &'static str,
        value: f32,
    },
    NonPositivePhysicalTileSize {
        axis: &'static str,
        value: f32,
    },
    NonFinite {
        path: String,
    },
    InvalidFrequency {
        path: String,
        value: u32,
    },
    OctavesOutOfRange {
        path: String,
        found: u8,
        maximum: u8,
    },
    NegativeParameter {
        path: String,
        value: f32,
    },
    NormalizedParameterOutOfRange {
        path: String,
        value: f32,
    },
    NonPositiveParameter {
        path: String,
        value: f32,
    },
    ParameterBelowMinimum {
        path: String,
        value: f32,
        minimum: f32,
    },
    ParameterAboveMaximum {
        path: String,
        value: f32,
        maximum: f32,
    },
    IntegerParameterAboveMaximum {
        path: String,
        value: u32,
        maximum: u32,
    },
    InvalidDisplacementRange {
        minimum: f32,
        maximum: f32,
        base: f32,
    },
    OcclusionDirectionsOutOfRange {
        found: u8,
        minimum: u8,
        maximum: u8,
    },
    OcclusionSamplesOutOfRange {
        found: u8,
        minimum: u8,
        maximum: u8,
    },
    OcclusionRadiusOutOfRange {
        path: &'static str,
        value: f32,
    },
    MissingOutputProfile,
    TooManyOutputProfiles {
        found: usize,
        maximum: usize,
    },
    TooManyLayers {
        found: usize,
        maximum: usize,
    },
    TooManyParameters {
        found: usize,
        maximum: usize,
    },
    InvalidParameterName {
        name: String,
    },
    MissingParameterReference {
        path: String,
        name: String,
    },
    DuplicateLayerId {
        id: String,
    },
    InvalidLayerId {
        path: String,
        id: String,
    },
    MissingLayerReference {
        path: String,
        id: String,
    },
    ForwardLayerReference {
        path: String,
        id: String,
    },
    DuplicateOutputProfile {
        profile: OutputProfile,
    },
    OutputNameCollision {
        name: String,
    },
    InvalidColour {
        path: String,
        value: f32,
    },
    InvalidOcclusionWeights {
        cavity: f32,
        horizon: f32,
    },
    InvalidRemapRange {
        path: String,
        minimum: f32,
        maximum: f32,
    },
    TooManyRemapPoints {
        path: String,
        found: usize,
        maximum: usize,
    },
    InvalidRemapCurve {
        path: String,
    },
    TooManyGradientStops {
        path: String,
        found: usize,
        maximum: usize,
    },
    InvalidGradient {
        path: String,
    },
    Image(ImageError),
}

impl fmt::Display for RecipeValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("recipe name must not be empty"),
            Self::InvalidName { name } => {
                write!(formatter, "recipe name is not file-safe: {name:?}")
            }
            Self::ZeroDimensions { width, height } => {
                write!(
                    formatter,
                    "recipe dimensions must be non-zero ({width}x{height})"
                )
            }
            Self::DimensionOverflow { width, height } => {
                write!(
                    formatter,
                    "recipe dimensions overflow usize ({width}x{height})"
                )
            }
            Self::NegativePhysicalTileSize { axis, value } => {
                write!(formatter, "physical tile {axis} is negative ({value})")
            }
            Self::NonPositivePhysicalTileSize { axis, value } => {
                write!(formatter, "physical tile {axis} must be positive ({value})")
            }
            Self::NonFinite { path } => write!(formatter, "{path} must be finite"),
            Self::InvalidFrequency { path, value } => {
                write!(
                    formatter,
                    "{path} must be a non-zero whole-number frequency ({value})"
                )
            }
            Self::OctavesOutOfRange {
                path,
                found,
                maximum,
            } => write!(
                formatter,
                "{path} has {found} octaves; maximum is {maximum}"
            ),
            Self::NegativeParameter { path, value } => {
                write!(formatter, "{path} must not be negative ({value})")
            }
            Self::NormalizedParameterOutOfRange { path, value } => {
                write!(formatter, "{path} must be in [0, 1] ({value})")
            }
            Self::NonPositiveParameter { path, value } => {
                write!(formatter, "{path} must be positive ({value})")
            }
            Self::ParameterBelowMinimum {
                path,
                value,
                minimum,
            } => write!(formatter, "{path} must be at least {minimum} ({value})"),
            Self::ParameterAboveMaximum {
                path,
                value,
                maximum,
            } => write!(formatter, "{path} must not exceed {maximum} ({value})"),
            Self::IntegerParameterAboveMaximum {
                path,
                value,
                maximum,
            } => write!(formatter, "{path} must not exceed {maximum} ({value})"),
            Self::InvalidDisplacementRange {
                minimum,
                maximum,
                base,
            } => write!(
                formatter,
                "displacement range is invalid: minimum={minimum}, maximum={maximum}, base={base}"
            ),
            Self::OcclusionDirectionsOutOfRange {
                found,
                minimum,
                maximum,
            } => write!(
                formatter,
                "occlusion directions {found} are outside {minimum}..={maximum}"
            ),
            Self::OcclusionSamplesOutOfRange {
                found,
                minimum,
                maximum,
            } => write!(
                formatter,
                "occlusion samples {found} are outside {minimum}..={maximum}"
            ),
            Self::OcclusionRadiusOutOfRange { path, value } => {
                write!(
                    formatter,
                    "{path} is outside the safe radius range ({value})"
                )
            }
            Self::MissingOutputProfile => {
                formatter.write_str("at least one output profile is required")
            }
            Self::TooManyOutputProfiles { found, maximum } => {
                write!(
                    formatter,
                    "{found} output profiles exceed the maximum of {maximum}"
                )
            }
            Self::TooManyLayers { found, maximum } => {
                write!(formatter, "{found} layers exceed the maximum of {maximum}")
            }
            Self::TooManyParameters { found, maximum } => write!(
                formatter,
                "{found} recipe parameters exceed the maximum of {maximum}"
            ),
            Self::InvalidParameterName { name } => {
                write!(formatter, "recipe parameter name is invalid: {name:?}")
            }
            Self::MissingParameterReference { path, name } => {
                write!(formatter, "{path} references missing parameter {name:?}")
            }
            Self::DuplicateLayerId { id } => write!(formatter, "layer id {id:?} is repeated"),
            Self::InvalidLayerId { path, id } => {
                write!(formatter, "{path} has an invalid stable id {id:?}")
            }
            Self::MissingLayerReference { path, id } => {
                write!(formatter, "{path} references missing layer {id:?}")
            }
            Self::ForwardLayerReference { path, id } => {
                write!(
                    formatter,
                    "{path} must reference an earlier layer, not {id:?}"
                )
            }
            Self::DuplicateOutputProfile { profile } => {
                write!(
                    formatter,
                    "output profile {profile:?} is listed more than once"
                )
            }
            Self::OutputNameCollision { name } => {
                write!(formatter, "generated output name collides: {name:?}")
            }
            Self::InvalidColour { path, value } => {
                write!(
                    formatter,
                    "colour channel {path} is outside [0, 1] ({value})"
                )
            }
            Self::InvalidOcclusionWeights { cavity, horizon } => {
                write!(
                    formatter,
                    "occlusion weights are unusable: {cavity} + {horizon}"
                )
            }
            Self::InvalidRemapRange {
                path,
                minimum,
                maximum,
            } => write!(formatter, "{path} range is invalid ({minimum}..{maximum})"),
            Self::TooManyRemapPoints {
                path,
                found,
                maximum,
            } => write!(formatter, "{path} has {found} points; maximum is {maximum}"),
            Self::InvalidRemapCurve { path } => {
                write!(formatter, "{path} must be monotonic and finite")
            }
            Self::TooManyGradientStops {
                path,
                found,
                maximum,
            } => write!(formatter, "{path} has {found} stops; maximum is {maximum}"),
            Self::InvalidGradient { path } => {
                write!(formatter, "{path} must be ordered and finite")
            }
            Self::Image(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RecipeValidationError {}

/// Aggregate of all issues found during one deterministic validation pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeValidationErrors {
    issues: Vec<RecipeValidationError>,
}

impl RecipeValidationErrors {
    /// Returns all collected issues in traversal order.
    #[must_use]
    pub fn issues(&self) -> &[RecipeValidationError] {
        &self.issues
    }

    /// Returns whether no issues were collected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    /// Number of collected issues.
    #[must_use]
    pub fn len(&self) -> usize {
        self.issues.len()
    }

    fn push(&mut self, issue: RecipeValidationError) {
        self.issues.push(issue);
    }
}

impl fmt::Display for RecipeValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, issue) in self.issues.iter().enumerate() {
            if index != 0 {
                formatter.write_str("; ")?;
            }
            issue.fmt(formatter)?;
        }
        Ok(())
    }
}

impl std::error::Error for RecipeValidationErrors {}

/// Validates all structural, numeric and reference constraints.
pub fn validate_recipe(recipe: &TextureRecipe) -> Result<(), RecipeValidationErrors> {
    let mut errors = RecipeValidationErrors::default();
    validate_root(recipe, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_root(recipe: &TextureRecipe, errors: &mut RecipeValidationErrors) {
    validate_name(&recipe.name, errors);
    validate_dimensions(recipe.width, recipe.height, errors);
    validate_tile_size("width", recipe.physical_tile_width_m, errors);
    validate_tile_size("height", recipe.physical_tile_height_m, errors);
    validate_material(&recipe.material, recipe.width, recipe.height, errors);
    validate_parameters(&recipe.parameters, errors);

    if recipe.layers.len() > MAX_LAYERS {
        errors.push(RecipeValidationError::TooManyLayers {
            found: recipe.layers.len(),
            maximum: MAX_LAYERS,
        });
    }
    let ids = collect_layer_ids(&recipe.layers, errors);
    for (index, layer) in recipe.layers.iter().enumerate() {
        validate_layer(layer, index, &ids, &recipe.parameters, errors);
    }

    validate_finite("normal_scale", recipe.normal_scale, errors);
    if recipe.normal_scale < 0.0 {
        errors.push(RecipeValidationError::NegativeParameter {
            path: "normal_scale".into(),
            value: recipe.normal_scale,
        });
    }
    validate_displacement(&recipe.displacement, errors);
    validate_occlusion(&recipe.occlusion, errors);
    validate_albedo(&recipe.albedo, &recipe.parameters, errors);
    validate_output_profiles(&recipe.name, &recipe.output_profiles, errors);
}

fn collect_layer_ids(
    layers: &[MaterialLayer],
    errors: &mut RecipeValidationErrors,
) -> HashMap<String, usize> {
    let mut ids = HashMap::with_capacity(layers.len());
    for (index, layer) in layers.iter().enumerate() {
        let path = format!("/layers/{index}/id");
        if layer.id.trim().is_empty()
            || layer.id == "."
            || layer.id == ".."
            || layer.id.contains('/')
            || layer.id.contains('\\')
            || layer.id.chars().any(char::is_control)
            || !layer
                .id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "_-.".contains(character))
        {
            errors.push(RecipeValidationError::InvalidLayerId {
                path,
                id: layer.id.clone(),
            });
        }
        if ids.insert(layer.id.clone(), index).is_some() {
            errors.push(RecipeValidationError::DuplicateLayerId {
                id: layer.id.clone(),
            });
        }
    }
    ids
}

fn validate_name(name: &str, errors: &mut RecipeValidationErrors) {
    if name.trim().is_empty() {
        errors.push(RecipeValidationError::EmptyName);
        return;
    }
    if name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
    {
        errors.push(RecipeValidationError::InvalidName {
            name: name.to_owned(),
        });
    }
}

fn validate_parameters(
    parameters: &BTreeMap<String, ParameterDefinition>,
    errors: &mut RecipeValidationErrors,
) {
    if parameters.len() > MAX_RECIPE_PARAMETERS {
        errors.push(RecipeValidationError::TooManyParameters {
            found: parameters.len(),
            maximum: MAX_RECIPE_PARAMETERS,
        });
    }
    for (name, definition) in parameters {
        if name.is_empty()
            || name.len() > MAX_PARAMETER_NAME_LEN
            || !name.chars().enumerate().all(|(index, character)| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit() && index > 0
                    || character == '_' && index > 0
            })
        {
            errors.push(RecipeValidationError::InvalidParameterName { name: name.clone() });
        }
        match definition {
            ParameterDefinition::Colour { default, .. } => {
                validate_linear_rgb(&format!("parameters.{name}.default"), *default, errors);
            }
        }
    }
}

fn validate_dimensions(width: u32, height: u32, errors: &mut RecipeValidationErrors) {
    if width == 0 || height == 0 {
        errors.push(RecipeValidationError::ZeroDimensions { width, height });
        return;
    }
    if let Err(error) = TextureDimensions::new(width, height) {
        let issue = match error {
            ImageError::PixelCountOverflow { .. } => {
                RecipeValidationError::DimensionOverflow { width, height }
            }
            other => RecipeValidationError::Image(other),
        };
        errors.push(issue);
    }
}

fn validate_tile_size(axis: &'static str, value: f32, errors: &mut RecipeValidationErrors) {
    if !value.is_finite() {
        errors.push(RecipeValidationError::NonFinite {
            path: format!("physical_tile_{axis}_m"),
        });
    } else if value < 0.0 {
        errors.push(RecipeValidationError::NegativePhysicalTileSize { axis, value });
    } else if value == 0.0 {
        errors.push(RecipeValidationError::NonPositivePhysicalTileSize { axis, value });
    }
}

fn validate_material(
    material: &MaterialModel,
    width: u32,
    height: u32,
    errors: &mut RecipeValidationErrors,
) {
    match material {
        MaterialModel::LayeredNoise {
            frequency,
            amplitude,
            octaves,
            lacunarity,
            gain,
            offset,
        } => {
            validate_finite("material.frequency", *frequency, errors);
            validate_positive("material.frequency", *frequency, errors);
            validate_finite("material.amplitude", *amplitude, errors);
            validate_octaves("material.octaves", *octaves, errors);
            validate_positive("material.lacunarity", *lacunarity, errors);
            validate_finite("material.gain", *gain, errors);
            validate_finite("material.offset", *offset, errors);
        }
        MaterialModel::CrackedStone {
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
        } => {
            validate_nonzero("material.cells_x", *cells_x, errors);
            validate_nonzero("material.cells_y", *cells_y, errors);
            validate_normalized("material.cell_jitter", *cell_jitter, errors);
            validate_nonnegative("material.warp_amplitude", *warp_amplitude, errors);
            validate_maximum(
                "material.warp_amplitude",
                *warp_amplitude,
                CRACKED_STONE_MAX_WARP_AMPLITUDE,
                errors,
            );
            validate_positive("material.crack_width", *crack_width, errors);
            validate_nonnegative("material.shoulder_width", *shoulder_width, errors);
            validate_minimum(
                "material.shoulder_width",
                *shoulder_width,
                *crack_width,
                errors,
            );
            validate_nonnegative("material.crack_depth", *crack_depth, errors);
            validate_nonnegative("material.slab_variation", *slab_variation, errors);
            validate_normalized(
                "material.fracture_probability",
                *fracture_probability,
                errors,
            );
            validate_nonnegative("material.fracture_depth", *fracture_depth, errors);
            validate_nonnegative("material.surface_amplitude", *surface_amplitude, errors);
            validate_nonnegative("material.broad_variation", *broad_variation, errors);
        }
        MaterialModel::RoundedStones {
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
        } => {
            validate_nonzero("material.cells_x", *cells_x, errors);
            validate_nonzero("material.cells_y", *cells_y, errors);
            validate_integer_maximum("material.cells_x", *cells_x, width, errors);
            validate_integer_maximum("material.cells_y", *cells_y, height, errors);
            validate_positive("material.stone_radius", *stone_radius, errors);
            validate_maximum(
                "material.stone_radius",
                *stone_radius,
                ROUNDED_STONES_MAX_RADIUS,
                errors,
            );
            validate_normalized("material.cell_jitter", *cell_jitter, errors);
            validate_nonnegative("material.warp_amplitude", *warp_amplitude, errors);
            validate_maximum(
                "material.warp_amplitude",
                *warp_amplitude,
                ROUNDED_STONES_MAX_WARP_AMPLITUDE,
                errors,
            );
            validate_positive("material.anisotropy", *anisotropy, errors);
            validate_nonnegative("material.stone_height", *stone_height, errors);
            validate_nonnegative("material.stone_variation", *stone_variation, errors);
            validate_finite("material.gap_height", *gap_height, errors);
            validate_maximum("material.gap_height", *gap_height, *stone_height, errors);
            validate_nonnegative("material.sand_amplitude", *sand_amplitude, errors);
            validate_positive("material.edge_softness", *edge_softness, errors);
        }
    }
}

fn validate_layer(
    layer: &MaterialLayer,
    index: usize,
    ids: &HashMap<String, usize>,
    parameters: &BTreeMap<String, ParameterDefinition>,
    errors: &mut RecipeValidationErrors,
) {
    let path = format!("/layers/{index}");
    validate_source(&layer.source, &format!("{path}/source"), errors);
    validate_remap(&layer.remap, &format!("{path}/remap"), errors);
    validate_mask(
        layer.mask.as_ref(),
        index,
        ids,
        &format!("{path}/mask"),
        errors,
    );
    validate_outputs(
        &layer.outputs,
        &format!("{path}/outputs"),
        parameters,
        errors,
    );
}

fn validate_source(source: &ScalarSource, path: &str, errors: &mut RecipeValidationErrors) {
    if source.frequency == 0 {
        errors.push(RecipeValidationError::InvalidFrequency {
            path: format!("{path}/frequency"),
            value: source.frequency,
        });
    }
    if source.kind.is_fractal() {
        validate_octaves(&format!("{path}/octaves"), source.octaves, errors);
        validate_positive(&format!("{path}/lacunarity"), source.lacunarity, errors);
        validate_finite(&format!("{path}/gain"), source.gain, errors);
    }
    for (axis, value) in [("x", source.offset[0]), ("y", source.offset[1])] {
        validate_finite(&format!("{path}/offset/{axis}"), value, errors);
    }
    if source.kind.is_cellular() {
        validate_normalized(
            &format!("{path}/cellular_jitter"),
            source.cellular_jitter,
            errors,
        );
    }
    if let Some(warp) = source.domain_warp {
        validate_domain_warp(warp, &format!("{path}/domain_warp"), errors);
    }
}

fn validate_domain_warp(warp: DomainWarpSettings, path: &str, errors: &mut RecipeValidationErrors) {
    validate_nonnegative(&format!("{path}/amplitude"), warp.amplitude, errors);
    if warp.frequency == 0 {
        errors.push(RecipeValidationError::InvalidFrequency {
            path: format!("{path}/frequency"),
            value: warp.frequency,
        });
    }
    validate_octaves(&format!("{path}/octaves"), warp.octaves, errors);
    validate_positive(&format!("{path}/lacunarity"), warp.lacunarity, errors);
    validate_finite(&format!("{path}/gain"), warp.gain, errors);
}

fn validate_remap(remap: &ScalarRemap, path: &str, errors: &mut RecipeValidationErrors) {
    validate_finite(&format!("{path}/input_min"), remap.input_min, errors);
    validate_finite(&format!("{path}/input_max"), remap.input_max, errors);
    if remap.input_max <= remap.input_min {
        errors.push(RecipeValidationError::InvalidRemapRange {
            path: path.into(),
            minimum: remap.input_min,
            maximum: remap.input_max,
        });
    }
    validate_finite(&format!("{path}/contrast"), remap.contrast, errors);
    validate_finite(&format!("{path}/bias"), remap.bias, errors);
    if let Some(points) = &remap.curve {
        if points.len() > MAX_REMAP_POINTS {
            errors.push(RecipeValidationError::TooManyRemapPoints {
                path: format!("{path}/curve"),
                found: points.len(),
                maximum: MAX_REMAP_POINTS,
            });
        }
        validate_curve(points, &format!("{path}/curve"), errors);
    }
}

fn validate_curve(points: &[RemapPoint], path: &str, errors: &mut RecipeValidationErrors) {
    let mut previous_position = None;
    let mut previous_value = None;
    for point in points {
        let valid = point.position.is_finite()
            && point.value.is_finite()
            && (0.0..=1.0).contains(&point.position)
            && previous_position.is_none_or(|value| point.position > value)
            && previous_value.is_none_or(|value| point.value >= value);
        if !valid {
            errors.push(RecipeValidationError::InvalidRemapCurve { path: path.into() });
            break;
        }
        previous_position = Some(point.position);
        previous_value = Some(point.value);
    }
}

fn validate_mask(
    mask: Option<&LayerMask>,
    index: usize,
    ids: &HashMap<String, usize>,
    path: &str,
    errors: &mut RecipeValidationErrors,
) {
    match mask {
        Some(LayerMask::PreviousHeight {
            bottom_m, top_m, ..
        }) => {
            validate_finite(&format!("{path}/bottom_m"), *bottom_m, errors);
            validate_finite(&format!("{path}/top_m"), *top_m, errors);
            if top_m <= bottom_m {
                errors.push(RecipeValidationError::InvalidRemapRange {
                    path: path.into(),
                    minimum: *bottom_m,
                    maximum: *top_m,
                });
            }
        }
        Some(LayerMask::Noise { source, remap }) => {
            validate_source(source, &format!("{path}/source"), errors);
            validate_remap(remap, &format!("{path}/remap"), errors);
        }
        Some(LayerMask::Layer { layer_id, remap }) => {
            validate_remap(remap, &format!("{path}/remap"), errors);
            match ids.get(layer_id) {
                None => errors.push(RecipeValidationError::MissingLayerReference {
                    path: format!("{path}/layer_id"),
                    id: layer_id.clone(),
                }),
                Some(&target) if target >= index => {
                    errors.push(RecipeValidationError::ForwardLayerReference {
                        path: format!("{path}/layer_id"),
                        id: layer_id.clone(),
                    });
                }
                Some(_) => {}
            }
        }
        Some(LayerMask::Own) | None => {}
    }
}

fn validate_outputs(
    outputs: &super::recipe::LayerOutputs,
    path: &str,
    parameters: &BTreeMap<String, ParameterDefinition>,
    errors: &mut RecipeValidationErrors,
) {
    let height = &outputs.height;
    validate_finite(
        &format!("{path}/height/strength_m"),
        height.strength_m,
        errors,
    );
    if let HeightBlend::Lerp { amount } = height.blend {
        validate_normalized(&format!("{path}/height/blend/amount"), amount, errors);
    }
    let albedo = &outputs.albedo;
    validate_normalized(&format!("{path}/albedo/strength"), albedo.strength, errors);
    for (suffix, value) in [
        ("hue_influence", albedo.hue_influence),
        ("saturation_influence", albedo.saturation_influence),
        ("value_influence", albedo.value_influence),
    ] {
        validate_finite(&format!("{path}/albedo/{suffix}"), value, errors);
        if !(-1.0..=1.0).contains(&value) {
            errors.push(RecipeValidationError::NormalizedParameterOutOfRange {
                path: format!("{path}/albedo/{suffix}"),
                value,
            });
        }
    }
    validate_colour_map(
        &albedo.colour_map,
        &format!("{path}/albedo/colour_map"),
        parameters,
        errors,
    );
}

fn validate_colour_map(
    map: &ColourMap,
    path: &str,
    parameters: &BTreeMap<String, ParameterDefinition>,
    errors: &mut RecipeValidationErrors,
) {
    match map {
        ColourMap::Ramp { first, second } => {
            validate_colour_value(&format!("{path}/first"), first, parameters, errors);
            validate_colour_value(&format!("{path}/second"), second, parameters, errors);
        }
        ColourMap::Gradient { stops } => {
            if stops.len() < 2 {
                errors.push(RecipeValidationError::InvalidGradient { path: path.into() });
            }
            if stops.len() > MAX_GRADIENT_STOPS {
                errors.push(RecipeValidationError::TooManyGradientStops {
                    path: format!("{path}/stops"),
                    found: stops.len(),
                    maximum: MAX_GRADIENT_STOPS,
                });
            }
            validate_gradient_stops(stops, path, parameters, errors);
        }
    }
}

fn validate_gradient_stops(
    stops: &[GradientStop],
    path: &str,
    parameters: &BTreeMap<String, ParameterDefinition>,
    errors: &mut RecipeValidationErrors,
) {
    let mut previous = None;
    for stop in stops {
        if !stop.position.is_finite()
            || !(0.0..=1.0).contains(&stop.position)
            || previous.is_some_and(|position| stop.position <= position)
        {
            errors.push(RecipeValidationError::InvalidGradient { path: path.into() });
        }
        validate_colour_value(
            &format!("{path}/stops/colour"),
            &stop.colour,
            parameters,
            errors,
        );
        previous = Some(stop.position);
    }
}

fn validate_displacement(settings: &DisplacementSettings, errors: &mut RecipeValidationErrors) {
    validate_finite("displacement.minimum_m", settings.minimum_m, errors);
    validate_finite("displacement.maximum_m", settings.maximum_m, errors);
    validate_finite("displacement.base_m", settings.base_m, errors);
    if settings.minimum_m >= settings.maximum_m
        || settings.base_m < settings.minimum_m
        || settings.base_m > settings.maximum_m
    {
        errors.push(RecipeValidationError::InvalidDisplacementRange {
            minimum: settings.minimum_m,
            maximum: settings.maximum_m,
            base: settings.base_m,
        });
    }
}

fn validate_occlusion(settings: &OcclusionRecipeSettings, errors: &mut RecipeValidationErrors) {
    if !(1..=MAX_AO_DIRECTIONS).contains(&settings.directions) {
        errors.push(RecipeValidationError::OcclusionDirectionsOutOfRange {
            found: settings.directions,
            minimum: 1,
            maximum: MAX_AO_DIRECTIONS,
        });
    }
    if !(1..=MAX_AO_SAMPLES).contains(&settings.samples) {
        errors.push(RecipeValidationError::OcclusionSamplesOutOfRange {
            found: settings.samples,
            minimum: 1,
            maximum: MAX_AO_SAMPLES,
        });
    }
    for (path, value) in [
        ("occlusion.radius", settings.radius),
        ("occlusion.max_radius", settings.max_radius),
    ] {
        if !value.is_finite() || value <= 0.0 || value > MAX_AO_RADIUS {
            errors.push(RecipeValidationError::OcclusionRadiusOutOfRange { path, value });
        }
    }
    if settings.max_radius < settings.radius {
        errors.push(RecipeValidationError::OcclusionRadiusOutOfRange {
            path: "occlusion.max_radius",
            value: settings.max_radius,
        });
    }
    for (path, value) in [
        ("occlusion.cavity_strength", settings.cavity_strength),
        ("occlusion.horizon_strength", settings.horizon_strength),
    ] {
        validate_nonnegative(path, value, errors);
    }
    validate_positive("occlusion.power", settings.power, errors);
    if let OcclusionCombine::WeightedMinimum {
        cavity_weight,
        horizon_weight,
    } = settings.combine
    {
        validate_nonnegative("occlusion.combine.cavity_weight", cavity_weight, errors);
        validate_nonnegative("occlusion.combine.horizon_weight", horizon_weight, errors);
        if cavity_weight + horizon_weight <= 0.0 {
            errors.push(RecipeValidationError::InvalidOcclusionWeights {
                cavity: cavity_weight,
                horizon: horizon_weight,
            });
        }
    }
}

fn validate_albedo(
    settings: &AlbedoSettings,
    parameters: &BTreeMap<String, ParameterDefinition>,
    errors: &mut RecipeValidationErrors,
) {
    for (name, colour) in [
        ("albedo.base_color", &settings.base_color),
        ("albedo.warm_color", &settings.warm_color),
    ] {
        validate_colour_value(name, colour, parameters, errors);
    }
    if settings.palette.len() > 8 {
        errors.push(RecipeValidationError::OutputNameCollision {
            name: "albedo.palette has more than 8 entries".into(),
        });
    }
    for (index, colour) in settings.palette.iter().enumerate() {
        validate_colour_value(
            &format!("albedo.palette[{index}]"),
            colour,
            parameters,
            errors,
        );
    }
    for (path, value) in [
        ("albedo.variation", settings.variation),
        ("albedo.crack_darkening", settings.crack_darkening),
        ("albedo.shoulder_variation", settings.shoulder_variation),
        ("albedo.mineral_density", settings.mineral_density),
        ("albedo.mineral_brightness", settings.mineral_brightness),
        ("albedo.occlusion_influence", settings.occlusion_influence),
    ] {
        validate_nonnegative(path, value, errors);
    }
    validate_normalized("albedo.mineral_density", settings.mineral_density, errors);
    validate_normalized(
        "albedo.occlusion_influence",
        settings.occlusion_influence,
        errors,
    );
}

fn validate_colour(path: &str, colour: [f32; 3], errors: &mut RecipeValidationErrors) {
    for (channel, value) in colour.into_iter().enumerate() {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            errors.push(RecipeValidationError::InvalidColour {
                path: format!("{path}[{channel}]"),
                value,
            });
        }
    }
}

fn validate_linear_rgb(path: &str, colour: LinearRgb, errors: &mut RecipeValidationErrors) {
    validate_colour(path, colour.channels(), errors);
}

fn validate_colour_value(
    path: &str,
    colour: &ColourValue,
    parameters: &BTreeMap<String, ParameterDefinition>,
    errors: &mut RecipeValidationErrors,
) {
    match colour {
        ColourValue::Literal(colour) => validate_linear_rgb(path, *colour, errors),
        ColourValue::Parameter(reference) => {
            if !parameters.contains_key(&reference.parameter) {
                errors.push(RecipeValidationError::MissingParameterReference {
                    path: path.into(),
                    name: reference.parameter.clone(),
                });
            }
            if let Some(base) = reference.base {
                validate_linear_rgb(&format!("{path}.base"), base, errors);
            }
        }
    }
}

fn validate_output_profiles(
    recipe_name: &str,
    profiles: &[OutputProfile],
    errors: &mut RecipeValidationErrors,
) {
    if profiles.is_empty() {
        errors.push(RecipeValidationError::MissingOutputProfile);
        return;
    }
    if profiles.len() > MAX_OUTPUT_PROFILES {
        errors.push(RecipeValidationError::TooManyOutputProfiles {
            found: profiles.len(),
            maximum: MAX_OUTPUT_PROFILES,
        });
    }
    let mut seen = HashSet::new();
    for profile in profiles {
        if !seen.insert(*profile) {
            errors.push(RecipeValidationError::DuplicateOutputProfile { profile: *profile });
        }
    }
    if recipe_name.ends_with('.') {
        errors.push(RecipeValidationError::OutputNameCollision {
            name: recipe_name.into(),
        });
    }
}

fn validate_finite(path: &str, value: f32, errors: &mut RecipeValidationErrors) {
    if !value.is_finite() {
        errors.push(RecipeValidationError::NonFinite { path: path.into() });
    }
}

fn validate_positive(path: &str, value: f32, errors: &mut RecipeValidationErrors) {
    if !value.is_finite() {
        errors.push(RecipeValidationError::NonFinite { path: path.into() });
    } else if value <= 0.0 {
        errors.push(RecipeValidationError::NonPositiveParameter {
            path: path.into(),
            value,
        });
    }
}

fn validate_nonnegative(path: &str, value: f32, errors: &mut RecipeValidationErrors) {
    if !value.is_finite() {
        errors.push(RecipeValidationError::NonFinite { path: path.into() });
    } else if value < 0.0 {
        errors.push(RecipeValidationError::NegativeParameter {
            path: path.into(),
            value,
        });
    }
}

fn validate_minimum(path: &str, value: f32, minimum: f32, errors: &mut RecipeValidationErrors) {
    if value.is_finite() && minimum.is_finite() && value < minimum {
        errors.push(RecipeValidationError::ParameterBelowMinimum {
            path: path.into(),
            value,
            minimum,
        });
    }
}

fn validate_maximum(path: &str, value: f32, maximum: f32, errors: &mut RecipeValidationErrors) {
    if value.is_finite() && maximum.is_finite() && value > maximum {
        errors.push(RecipeValidationError::ParameterAboveMaximum {
            path: path.into(),
            value,
            maximum,
        });
    }
}

fn validate_integer_maximum(
    path: &str,
    value: u32,
    maximum: u32,
    errors: &mut RecipeValidationErrors,
) {
    if value > maximum {
        errors.push(RecipeValidationError::IntegerParameterAboveMaximum {
            path: path.into(),
            value,
            maximum,
        });
    }
}

fn validate_normalized(path: &str, value: f32, errors: &mut RecipeValidationErrors) {
    if !value.is_finite() {
        errors.push(RecipeValidationError::NonFinite { path: path.into() });
    } else if !(0.0..=1.0).contains(&value) {
        errors.push(RecipeValidationError::NormalizedParameterOutOfRange {
            path: path.into(),
            value,
        });
    }
}

fn validate_octaves(path: &str, value: u8, errors: &mut RecipeValidationErrors) {
    if value == 0 || value > MAX_OCTAVES {
        errors.push(RecipeValidationError::OctavesOutOfRange {
            path: path.into(),
            found: value,
            maximum: MAX_OCTAVES,
        });
    }
}

fn validate_nonzero(path: &str, value: u32, errors: &mut RecipeValidationErrors) {
    if value == 0 {
        errors.push(RecipeValidationError::NonPositiveParameter {
            path: path.into(),
            value: 0.0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedural_textures::recipe::{
        AlbedoSettings, DisplacementSettings, HeightOutput, LayerOutputs, MaterialLayer,
        OcclusionRecipeSettings, ScalarSource,
    };

    fn valid_recipe() -> TextureRecipe {
        TextureRecipe {
            name: "test-texture".into(),
            seed: 42,
            parameters: BTreeMap::new(),
            width: 32,
            height: 16,
            physical_tile_width_m: 4.0,
            physical_tile_height_m: 4.0,
            material: MaterialModel::default(),
            layers: Vec::new(),
            normal_scale: 1.0,
            displacement: DisplacementSettings::default(),
            occlusion: OcclusionRecipeSettings::default(),
            albedo: AlbedoSettings::default(),
            output_profiles: vec![OutputProfile::Separate],
        }
    }

    #[test]
    fn valid_recipe_passes() {
        assert!(validate_recipe(&valid_recipe()).is_ok());
    }

    #[test]
    fn duplicate_and_forward_layer_references_are_rejected() {
        let mut recipe = valid_recipe();
        let mut first = MaterialLayer {
            id: "first".into(),
            source: ScalarSource::default(),
            outputs: LayerOutputs {
                height: HeightOutput {
                    enabled: true,
                    ..HeightOutput::default()
                },
                ..LayerOutputs::default()
            },
            ..MaterialLayer::default()
        };
        first.mask = Some(LayerMask::Layer {
            layer_id: "later".into(),
            remap: ScalarRemap::default(),
        });
        recipe.layers = vec![
            first,
            MaterialLayer {
                id: "later".into(),
                ..MaterialLayer::default()
            },
        ];
        let errors = validate_recipe(&recipe).expect_err("forward mask");
        assert!(
            errors
                .issues()
                .iter()
                .any(|issue| matches!(issue, RecipeValidationError::ForwardLayerReference { .. }))
        );
        recipe.layers[1].id = "first".into();
        assert!(
            validate_recipe(&recipe)
                .expect_err("duplicate id")
                .issues()
                .iter()
                .any(|issue| matches!(issue, RecipeValidationError::DuplicateLayerId { .. }))
        );
    }

    #[test]
    fn previous_height_mask_requires_an_ordered_finite_range() {
        let mut recipe = valid_recipe();
        recipe.layers.push(MaterialLayer {
            mask: Some(LayerMask::PreviousHeight {
                bottom_m: 0.02,
                top_m: 0.01,
                invert: false,
            }),
            ..MaterialLayer::default()
        });

        let errors = validate_recipe(&recipe).expect_err("reversed height mask range");
        assert!(errors.issues().iter().any(|issue| matches!(
            issue,
            RecipeValidationError::InvalidRemapRange { path, .. }
                if path == "/layers/0/mask"
        )));
    }

    #[test]
    fn remap_curves_must_be_monotonic() {
        let mut recipe = valid_recipe();
        recipe.layers.push(MaterialLayer {
            remap: ScalarRemap {
                curve: Some(vec![
                    RemapPoint {
                        position: 0.0,
                        value: 0.8,
                    },
                    RemapPoint {
                        position: 0.5,
                        value: 0.2,
                    },
                ]),
                ..ScalarRemap::default()
            },
            ..MaterialLayer::default()
        });
        assert!(
            validate_recipe(&recipe)
                .expect_err("non-monotonic curve")
                .issues()
                .iter()
                .any(|issue| matches!(issue, RecipeValidationError::InvalidRemapCurve { .. }))
        );
    }

    #[test]
    fn rounded_stone_evaluator_limits_are_reported_by_validation() {
        let mut recipe = valid_recipe();
        recipe.material = serde_json::from_value(serde_json::json!({
            "kind": "rounded_stones",
            "cells_x": 33,
            "stone_radius": 1.1,
            "warp_amplitude": 0.41,
            "stone_height": 0.1,
            "gap_height": 0.2,
            "edge_softness": 0.0
        }))
        .expect("rounded-stone fixture");

        let errors = validate_recipe(&recipe).expect_err("unsafe rounded-stone parameters");
        assert!(errors.issues().iter().any(|issue| matches!(
            issue,
            RecipeValidationError::IntegerParameterAboveMaximum { path, .. }
                if path == "material.cells_x"
        )));
        for path in [
            "material.stone_radius",
            "material.warp_amplitude",
            "material.gap_height",
        ] {
            assert!(errors.issues().iter().any(|issue| matches!(
                issue,
                RecipeValidationError::ParameterAboveMaximum {
                    path: issue_path,
                    ..
                } if issue_path == path
            )));
        }
        assert!(errors.issues().iter().any(|issue| matches!(
            issue,
            RecipeValidationError::NonPositiveParameter { path, .. }
                if path == "material.edge_softness"
        )));
    }

    #[test]
    fn cracked_stone_evaluator_limits_are_reported_by_validation() {
        let mut recipe = valid_recipe();
        recipe.material = serde_json::from_value(serde_json::json!({
            "kind": "cracked_stone",
            "warp_amplitude": 0.46,
            "crack_width": 0.2,
            "shoulder_width": 0.1
        }))
        .expect("cracked-stone fixture");

        let errors = validate_recipe(&recipe).expect_err("unsafe cracked-stone parameters");
        assert!(errors.issues().iter().any(|issue| matches!(
            issue,
            RecipeValidationError::ParameterAboveMaximum { path, .. }
                if path == "material.warp_amplitude"
        )));
        assert!(errors.issues().iter().any(|issue| matches!(
            issue,
            RecipeValidationError::ParameterBelowMinimum { path, .. }
                if path == "material.shoulder_width"
        )));
    }
}
