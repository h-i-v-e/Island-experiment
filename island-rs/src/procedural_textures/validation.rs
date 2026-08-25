//! Full recipe validation before any bake allocates output files.

#![allow(clippy::missing_errors_doc, clippy::too_many_lines)]

use core::fmt;
use std::collections::HashSet;

use super::{
    image::{ImageError, TextureDimensions},
    recipe::{
        AlbedoSettings, BlendOperation, CURRENT_SCHEMA_VERSION, DomainWarpSettings,
        MAX_OUTPUT_PROFILES, MAX_SURFACE_LAYERS, MaterialModel, NoiseKind, NoiseLayer,
        OcclusionCombine, OcclusionRecipeSettings, OutputProfile, TextureRecipe,
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
/// Maximum recursive depth of a domain-warp or mask source.
pub const MAX_NOISE_DEPTH: usize = 16;

/// One precise reason a recipe cannot be baked.
#[derive(Clone, Debug, PartialEq)]
pub enum RecipeValidationError {
    /// The schema version is not supported by this generator.
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    /// The output name is empty or consists only of whitespace.
    EmptyName,
    /// The output name contains path/control characters.
    InvalidName { name: String },
    /// An image axis is zero.
    ZeroDimensions { width: u32, height: u32 },
    /// The image pixel count cannot fit in `usize`.
    DimensionOverflow { width: u32, height: u32 },
    /// A physical tile axis is negative.
    NegativePhysicalTileSize { axis: &'static str, value: f32 },
    /// A physical tile axis is zero or otherwise unusable.
    NonPositivePhysicalTileSize { axis: &'static str, value: f32 },
    /// A named numeric recipe value is NaN or infinite.
    NonFinite { path: String },
    /// A frequency must be strictly positive.
    InvalidFrequency { path: String, value: f32 },
    /// A cellular field was configured with zero frequency.
    ZeroCellularFrequency { path: String },
    /// An octave count is outside the documented safety range.
    OctavesOutOfRange {
        path: String,
        found: u8,
        maximum: u8,
    },
    /// A parameter expected to be non-negative is negative.
    NegativeParameter { path: String, value: f32 },
    /// A parameter expected to be in a normalized interval is outside it.
    NormalizedParameterOutOfRange { path: String, value: f32 },
    /// A positive parameter is zero or negative.
    NonPositiveParameter { path: String, value: f32 },
    /// The displacement range is empty or has its bounds reversed.
    InvalidDisplacementRange {
        minimum: f32,
        maximum: f32,
        base: f32,
    },
    /// A fixed AO direction count is outside safe limits.
    OcclusionDirectionsOutOfRange { found: u8, minimum: u8, maximum: u8 },
    /// A per-direction AO sample count is outside safe limits.
    OcclusionSamplesOutOfRange { found: u8, minimum: u8, maximum: u8 },
    /// An AO radius is outside safe limits.
    OcclusionRadiusOutOfRange { path: &'static str, value: f32 },
    /// No output profile was requested.
    MissingOutputProfile,
    /// Too many profiles make one recipe ambiguous/unsafe.
    TooManyOutputProfiles { found: usize, maximum: usize },
    /// Too many surface layers make one recipe unsafe to evaluate.
    TooManySurfaceLayers { found: usize, maximum: usize },
    /// The same profile was listed more than once.
    DuplicateOutputProfile { profile: OutputProfile },
    /// A blend mask or generated output would reuse a path/name.
    OutputNameCollision { name: String },
    /// A palette or colour channel is malformed.
    InvalidColour { path: String, value: f32 },
    /// A weighted AO combination has no usable weight.
    InvalidOcclusionWeights { cavity: f32, horizon: f32 },
    /// A nested source exceeded the recursion safety limit.
    NoiseNestingTooDeep { path: String, maximum: usize },
    /// An image constructor returned a more specific dimension failure.
    Image(ImageError),
}

impl fmt::Display for RecipeValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { found, supported } => {
                write!(
                    formatter,
                    "schema version {found} is unsupported (expected {supported})"
                )
            }
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
            Self::InvalidFrequency { path, value } | Self::NonPositiveParameter { path, value } => {
                write!(formatter, "{path} must be positive ({value})")
            }
            Self::ZeroCellularFrequency { path } => {
                write!(formatter, "cellular frequency at {path} must not be zero")
            }
            Self::OctavesOutOfRange {
                path,
                found,
                maximum,
            } => {
                write!(
                    formatter,
                    "{path} has {found} octaves; maximum is {maximum}"
                )
            }
            Self::NegativeParameter { path, value } => {
                write!(formatter, "{path} must not be negative ({value})")
            }
            Self::NormalizedParameterOutOfRange { path, value } => {
                write!(formatter, "{path} must be in [0, 1] ({value})")
            }
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
            Self::TooManySurfaceLayers { found, maximum } => {
                write!(
                    formatter,
                    "{found} surface layers exceed the maximum of {maximum}"
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
            Self::NoiseNestingTooDeep { path, maximum } => {
                write!(formatter, "noise nesting at {path} exceeds depth {maximum}")
            }
            Self::Image(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RecipeValidationError {}

/// An aggregate of all recipe issues found in one validation pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeValidationErrors {
    issues: Vec<RecipeValidationError>,
}

impl RecipeValidationErrors {
    /// Returns all collected issues in deterministic traversal order.
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

/// Validates all structural, numeric and output-safety constraints.
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
    if recipe.schema_version != CURRENT_SCHEMA_VERSION {
        errors.push(RecipeValidationError::UnsupportedSchemaVersion {
            found: recipe.schema_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }

    validate_name(&recipe.name, errors);
    validate_dimensions(recipe.width, recipe.height, errors);
    validate_tile_size("width", recipe.physical_tile_width_m, errors);
    validate_tile_size("height", recipe.physical_tile_height_m, errors);
    validate_material(&recipe.material, errors);

    if recipe.surface_layers.len() > MAX_SURFACE_LAYERS {
        errors.push(RecipeValidationError::TooManySurfaceLayers {
            found: recipe.surface_layers.len(),
            maximum: MAX_SURFACE_LAYERS,
        });
    }
    for (index, layer) in recipe.surface_layers.iter().enumerate() {
        validate_layer(layer, &format!("surface_layers[{index}]"), errors, 0);
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
    validate_albedo(&recipe.albedo, errors);
    validate_output_profiles(&recipe.name, &recipe.output_profiles, errors);
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

fn validate_material(material: &MaterialModel, errors: &mut RecipeValidationErrors) {
    match material {
        MaterialModel::LayeredNoise {
            frequency,
            amplitude,
            octaves,
            lacunarity,
            gain,
            offset,
        } => {
            validate_frequency("material.frequency", *frequency, errors, false);
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
            validate_nonnegative("material.crack_width", *crack_width, errors);
            validate_nonnegative("material.shoulder_width", *shoulder_width, errors);
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
            validate_positive("material.stone_radius", *stone_radius, errors);
            validate_normalized("material.cell_jitter", *cell_jitter, errors);
            validate_nonnegative("material.warp_amplitude", *warp_amplitude, errors);
            validate_positive("material.anisotropy", *anisotropy, errors);
            validate_nonnegative("material.stone_height", *stone_height, errors);
            validate_nonnegative("material.stone_variation", *stone_variation, errors);
            validate_finite("material.gap_height", *gap_height, errors);
            validate_nonnegative("material.sand_amplitude", *sand_amplitude, errors);
            validate_nonnegative("material.edge_softness", *edge_softness, errors);
        }
    }
}

fn validate_layer(
    layer: &NoiseLayer,
    path: &str,
    errors: &mut RecipeValidationErrors,
    depth: usize,
) {
    validate_frequency(
        &format!("{path}.frequency"),
        layer.frequency,
        errors,
        layer.kind.is_cellular(),
    );
    validate_finite(&format!("{path}.amplitude"), layer.amplitude, errors);
    validate_octaves(&format!("{path}.octaves"), layer.octaves, errors);
    validate_positive(&format!("{path}.lacunarity"), layer.lacunarity, errors);
    validate_finite(&format!("{path}.gain"), layer.gain, errors);
    for (axis, value) in [("x", layer.offset[0]), ("y", layer.offset[1])] {
        validate_finite(&format!("{path}.offset.{axis}"), value, errors);
    }
    validate_normalized(
        &format!("{path}.cellular_jitter"),
        layer.cellular_jitter,
        errors,
    );

    if let Some(warp) = layer.domain_warp {
        validate_domain_warp(warp, &format!("{path}.domain_warp"), errors);
    }
    validate_noise_kind(&layer.kind, &format!("{path}.kind"), errors, depth);
    validate_blend(&layer.blend, &format!("{path}.blend"), errors, depth);
}

fn validate_noise_kind(
    kind: &NoiseKind,
    path: &str,
    errors: &mut RecipeValidationErrors,
    depth: usize,
) {
    if depth > MAX_NOISE_DEPTH {
        errors.push(RecipeValidationError::NoiseNestingTooDeep {
            path: path.into(),
            maximum: MAX_NOISE_DEPTH,
        });
        return;
    }
    if let NoiseKind::DomainWarp {
        source,
        warp,
        amplitude,
    } = kind
    {
        validate_nonnegative(&format!("{path}.amplitude"), *amplitude, errors);
        validate_noise_kind(source, &format!("{path}.source"), errors, depth + 1);
        validate_noise_kind(warp, &format!("{path}.warp"), errors, depth + 1);
    }
}

fn validate_blend(
    blend: &BlendOperation,
    path: &str,
    errors: &mut RecipeValidationErrors,
    depth: usize,
) {
    match blend {
        BlendOperation::Lerp { amount } => {
            validate_normalized(path, *amount, errors);
        }
        BlendOperation::LerpByMask { mask } => {
            validate_layer(mask, &format!("{path}.mask"), errors, depth + 1);
        }
        _ => {}
    }
}

fn validate_domain_warp(warp: DomainWarpSettings, path: &str, errors: &mut RecipeValidationErrors) {
    validate_nonnegative(&format!("{path}.amplitude"), warp.amplitude, errors);
    validate_frequency(&format!("{path}.frequency"), warp.frequency, errors, false);
    validate_octaves(&format!("{path}.octaves"), warp.octaves, errors);
    validate_positive(&format!("{path}.lacunarity"), warp.lacunarity, errors);
    validate_finite(&format!("{path}.gain"), warp.gain, errors);
}

fn validate_displacement(
    displacement: &super::recipe::DisplacementSettings,
    errors: &mut RecipeValidationErrors,
) {
    validate_finite("displacement.minimum_m", displacement.minimum_m, errors);
    validate_finite("displacement.maximum_m", displacement.maximum_m, errors);
    validate_finite("displacement.base_m", displacement.base_m, errors);
    if displacement.minimum_m >= displacement.maximum_m
        || displacement.base_m < displacement.minimum_m
        || displacement.base_m > displacement.maximum_m
    {
        errors.push(RecipeValidationError::InvalidDisplacementRange {
            minimum: displacement.minimum_m,
            maximum: displacement.maximum_m,
            base: displacement.base_m,
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

fn validate_albedo(settings: &AlbedoSettings, errors: &mut RecipeValidationErrors) {
    for (name, colour) in [
        ("base_color", settings.base_color),
        ("warm_color", settings.warm_color),
    ] {
        validate_colour(name, colour, errors);
    }
    if settings.palette.len() > 8 {
        errors.push(RecipeValidationError::OutputNameCollision {
            name: "albedo.palette has more than 8 entries".into(),
        });
    }
    for (index, colour) in settings.palette.iter().enumerate() {
        validate_colour(&format!("albedo.palette[{index}]"), *colour, errors);
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
        // A trailing dot is accepted by some filesystems but aliases to the
        // same stem on others, so reject the collision-prone name early.
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

fn validate_frequency(path: &str, value: f32, errors: &mut RecipeValidationErrors, cellular: bool) {
    if !value.is_finite() {
        errors.push(RecipeValidationError::NonFinite { path: path.into() });
    } else if value == 0.0 && cellular {
        errors.push(RecipeValidationError::ZeroCellularFrequency { path: path.into() });
    } else if value <= 0.0 {
        errors.push(RecipeValidationError::InvalidFrequency {
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

impl NoiseKind {
    fn is_cellular(&self) -> bool {
        matches!(
            self,
            Self::CellularDistance | Self::CellularDistanceToEdge | Self::CellularValue
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{RecipeValidationError, validate_recipe};
    use crate::procedural_textures::image::NormalConvention;
    use crate::procedural_textures::recipe::{
        AlbedoSettings, DisplacementSettings, MaterialModel, NoiseKind, NoiseLayer,
        OcclusionRecipeSettings, OutputProfile, TextureRecipe,
    };

    fn valid_recipe() -> TextureRecipe {
        TextureRecipe {
            schema_version: 1,
            name: "test-texture".into(),
            seed: 42,
            width: 32,
            height: 16,
            physical_tile_width_m: 4.0,
            physical_tile_height_m: 4.0,
            material: MaterialModel::default(),
            surface_layers: Vec::new(),
            normal_convention: NormalConvention::OpenGl,
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
    fn invalid_dimensions_and_schema_are_rejected() {
        let mut recipe = valid_recipe();
        recipe.schema_version = 99;
        recipe.width = 0;
        let errors = validate_recipe(&recipe).expect_err("invalid recipe");
        assert!(errors.issues().iter().any(|issue| matches!(
            issue,
            RecipeValidationError::UnsupportedSchemaVersion { .. }
        )));
        assert!(
            errors
                .issues()
                .iter()
                .any(|issue| matches!(issue, RecipeValidationError::ZeroDimensions { .. }))
        );
    }

    #[test]
    fn cellular_zero_frequency_and_octave_limit_are_rejected() {
        let mut recipe = valid_recipe();
        recipe.surface_layers.push(NoiseLayer {
            kind: NoiseKind::CellularDistance,
            frequency: 0.0,
            octaves: 17,
            ..NoiseLayer::default()
        });
        let errors = validate_recipe(&recipe).expect_err("invalid source layer");
        assert!(
            errors
                .issues()
                .iter()
                .any(|issue| matches!(issue, RecipeValidationError::ZeroCellularFrequency { .. }))
        );
        assert!(
            errors
                .issues()
                .iter()
                .any(|issue| matches!(issue, RecipeValidationError::OctavesOutOfRange { .. }))
        );
    }

    #[test]
    fn unsafe_occlusion_settings_are_rejected() {
        let mut recipe = valid_recipe();
        recipe.occlusion.samples = 0;
        recipe.occlusion.radius = f32::INFINITY;
        let errors = validate_recipe(&recipe).expect_err("invalid AO");
        assert!(errors.issues().iter().any(|issue| matches!(
            issue,
            RecipeValidationError::OcclusionSamplesOutOfRange { .. }
        )));
        assert!(errors.issues().iter().any(|issue| matches!(
            issue,
            RecipeValidationError::OcclusionRadiusOutOfRange { .. }
        )));
    }
}
