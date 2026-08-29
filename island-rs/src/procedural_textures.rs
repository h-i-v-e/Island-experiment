//! Deterministic, engine-neutral procedural material texture generation.
//!
//! Recipes are borrowed for generation and the returned [`TextureSet`] owns
//! every finished image. File output is a separate borrowed operation, so an
//! engine can upload the typed buffers directly without encoding PNG files.

pub mod albedo;
pub mod cellular;
pub mod cracked_stone;
pub mod editor_protocol;
pub mod encoding;
pub mod field_program;
pub mod image;
pub mod layer_stack;
pub mod noise;
pub mod normal;
pub mod occlusion;
pub mod packing;
pub mod parameters;
pub mod periodic;
pub mod preview;
pub mod recipe;
pub mod rounded_stones;
pub mod runtime_materials;
pub mod validation;

use std::{fmt, path::Path};

use albedo::AlbedoConfig;
use encoding::TextureSetImages;
use occlusion::{OcclusionCombine, OcclusionSettings};
use packing::HeightRange;

pub use editor_protocol::{
    Diagnostic, EDITABLE_METADATA, EditorEnvelope, PropertyMetadata, metadata_coverage,
    property_metadata, schema_document,
};
pub use encoding::{ManifestMap, OutputManifest, OutputOptions, OutputProfile, PixelFormat};
pub use image::{
    FloatImage, Gray8Image, Gray16Image, Image, ImageError, NormalConvention, Rgb8Image,
    Rgba8Image, TextureDimensions, TextureMetadata, TextureSet,
};
pub use parameters::{
    ColourParameterReference, ColourValue, LinearRgb, ParameterDefinition, ParameterValue,
    RecipeParameterError, RecipeParameterErrors, RecipeParameterValues, ResolvedTextureRecipe,
};
pub use preview::{
    LayerPreviewMaps, PreviewMaps, PreviewSettings, PreviewTimings, generate_preview,
    generate_preview_with_parameters, layer_preview_maps,
};
pub use recipe::{
    AlbedoBlend, AlbedoOutput, AlbedoSettings, ColourMap, DisplacementSettings, DomainWarpSettings,
    GradientStop, HeightBlend, HeightOutput, LayerMask, LayerOutputs, MaterialLayer, MaterialModel,
    OcclusionRecipeSettings, RemapPoint, ScalarRemap, ScalarSource, SourceKind, TextureRecipe,
};
pub use runtime_materials::{
    IslandMaterialKind, IslandMaterialTextures, MaterialBakeIdentity, MaterialSelection,
    RuntimeMaterialBakeError, RuntimeMaterialBakeOptions, RuntimeMaterialInputs,
    bake_island_materials,
};
pub use validation::{RecipeValidationError, RecipeValidationErrors, validate_recipe};

/// Version of the deterministic texture-generation algorithm.
pub const TEXTURE_ALGORITHM_VERSION: u32 = 1;

/// Errors returned by the high-level generation and file-output boundary.
#[derive(Debug)]
pub enum TextureError {
    Validation(RecipeValidationErrors),
    Parameters(RecipeParameterErrors),
    Field(field_program::FieldError),
    Normal(normal::NormalError),
    Occlusion(occlusion::OcclusionError),
    Albedo(albedo::AlbedoError),
    Packing(packing::PackingError),
    Image(ImageError),
    Output(encoding::OutputError),
}

impl fmt::Display for TextureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => write!(formatter, "invalid texture recipe: {error}"),
            Self::Parameters(error) => write!(formatter, "invalid texture parameters: {error}"),
            Self::Field(error) => write!(formatter, "height-field generation failed: {error:?}"),
            Self::Normal(error) => write!(formatter, "normal generation failed: {error:?}"),
            Self::Occlusion(error) => write!(formatter, "occlusion generation failed: {error:?}"),
            Self::Albedo(error) => write!(formatter, "albedo generation failed: {error:?}"),
            Self::Packing(error) => write!(formatter, "height packing failed: {error:?}"),
            Self::Image(error) => write!(formatter, "texture image is invalid: {error}"),
            Self::Output(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TextureError {}

impl From<RecipeValidationErrors> for TextureError {
    fn from(error: RecipeValidationErrors) -> Self {
        Self::Validation(error)
    }
}

impl From<RecipeParameterErrors> for TextureError {
    fn from(error: RecipeParameterErrors) -> Self {
        Self::Parameters(error)
    }
}

macro_rules! texture_error_from {
    ($source:ty, $variant:ident) => {
        impl From<$source> for TextureError {
            fn from(error: $source) -> Self {
                Self::$variant(error)
            }
        }
    };
}

texture_error_from!(field_program::FieldError, Field);
texture_error_from!(normal::NormalError, Normal);
texture_error_from!(occlusion::OcclusionError, Occlusion);
texture_error_from!(albedo::AlbedoError, Albedo);
texture_error_from!(packing::PackingError, Packing);
texture_error_from!(ImageError, Image);
texture_error_from!(encoding::OutputError, Output);

/// Shared unquantized material evaluation used by both final bake and editor
/// preview. Keeping the layer diagnostics here lets a preview inspect raw,
/// remapped and masked maps without a second noise implementation.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialEvaluation {
    /// Final height and per-layer scalar diagnostics.
    pub layers: layer_stack::LayerEvaluation,
    /// Final material-local occlusion.
    pub occlusion: occlusion::OcclusionImage,
    /// Final linear-RGB albedo before sRGB encoding.
    pub albedo_linear: Vec<[f32; 3]>,
}

/// Evaluates the shared unquantized field and albedo passes.
///
/// # Errors
///
/// Returns validation, field, occlusion or albedo errors before a map is
/// allocated for output.
pub fn evaluate_material(recipe: &TextureRecipe) -> Result<MaterialEvaluation, TextureError> {
    validate_recipe(recipe)?;
    let resolved = parameters::resolve_validated_recipe(recipe, &RecipeParameterValues::new())?;
    evaluate_resolved_material(&resolved)
}

/// Resolves explicit caller values and evaluates the shared unquantized maps.
///
/// # Errors
///
/// Returns recipe, parameter-resolution, field, occlusion, or albedo errors.
pub fn evaluate_material_with_parameters(
    recipe: &TextureRecipe,
    parameters: &RecipeParameterValues,
) -> Result<MaterialEvaluation, TextureError> {
    let resolved = resolve_texture_recipe(recipe, parameters)?;
    evaluate_resolved_material(&resolved)
}

/// Resolves and validates one recipe without evaluating any pixels.
///
/// # Errors
///
/// Returns recipe validation or strict parameter-resolution errors.
pub fn resolve_texture_recipe(
    recipe: &TextureRecipe,
    parameters: &RecipeParameterValues,
) -> Result<ResolvedTextureRecipe, TextureError> {
    validate_recipe(recipe)?;
    parameters::resolve_validated_recipe(recipe, parameters).map_err(TextureError::from)
}

/// Evaluates a recipe whose parameter references have already been resolved.
///
/// # Errors
///
/// Returns field, occlusion, or albedo evaluation errors.
pub fn evaluate_resolved_material(
    resolved: &ResolvedTextureRecipe,
) -> Result<MaterialEvaluation, TextureError> {
    let recipe = resolved.recipe();
    let layers = layer_stack::evaluate_recipe(recipe)?;
    let field = &layers.field;
    let occlusion = occlusion::derive_occlusion(field, occlusion_settings(recipe))?;
    let config = albedo_config(recipe);
    let mut albedo_linear = albedo::generate_linear_albedo(field, config, recipe.seed)?;
    layers.apply_albedo(&recipe.layers, &mut albedo_linear)?;
    albedo::apply_occlusion_linear(&mut albedo_linear, field.dimensions(), config, &occlusion)?;
    Ok(MaterialEvaluation {
        layers,
        occlusion,
        albedo_linear,
    })
}

/// Generates a complete owned texture set from one validated recipe.
///
/// # Errors
///
/// Returns an error when validation fails or any field, map, quantization, or
/// metadata pass cannot produce a valid texture set.
pub fn generate_texture_set(
    recipe: &TextureRecipe,
    normal_convention: NormalConvention,
) -> Result<TextureSet, TextureError> {
    generate_texture_set_with_parameters(recipe, &RecipeParameterValues::new(), normal_convention)
}

/// Generates a complete owned texture set with explicit caller parameter
/// values. The input maps are borrowed and the returned images are owned.
///
/// # Errors
///
/// Returns validation, parameter-resolution, evaluation, or map errors.
pub fn generate_texture_set_with_parameters(
    recipe: &TextureRecipe,
    parameters: &RecipeParameterValues,
    normal_convention: NormalConvention,
) -> Result<TextureSet, TextureError> {
    let resolved = resolve_texture_recipe(recipe, parameters)?;
    let evaluated = evaluate_resolved_material(&resolved)?;
    texture_set_from_resolved_evaluation(&resolved, &evaluated, normal_convention)
}

/// Encodes one shared material evaluation into the final map set.
///
/// # Errors
///
/// Returns normal, packing, image or serialization errors at the output
/// boundary.
pub fn texture_set_from_evaluation(
    recipe: &TextureRecipe,
    evaluated: &MaterialEvaluation,
    normal_convention: NormalConvention,
) -> Result<TextureSet, TextureError> {
    let resolved = resolve_texture_recipe(recipe, &RecipeParameterValues::new())?;
    texture_set_from_resolved_evaluation(&resolved, evaluated, normal_convention)
}

/// Encodes an evaluation produced from the same resolved recipe.
///
/// # Errors
///
/// Returns normal, packing, image, or metadata errors.
pub fn texture_set_from_resolved_evaluation(
    resolved: &ResolvedTextureRecipe,
    evaluated: &MaterialEvaluation,
    normal_convention: NormalConvention,
) -> Result<TextureSet, TextureError> {
    let recipe = resolved.recipe();
    let field = &evaluated.layers.field;
    let occlusion = &evaluated.occlusion;
    let normal = normal::derive_normals(field, recipe.normal_scale, normal_convention)?;
    let albedo = albedo::encode_linear_albedo(field.dimensions(), &evaluated.albedo_linear)?;
    let range = HeightRange::new(
        recipe.displacement.minimum_m,
        recipe.displacement.maximum_m,
        recipe.displacement.base_m,
    )?;
    let height = packing::quantize_height(field, range)?;
    let recipe_hash = encoding::normalized_recipe_hash(recipe)?;
    let metadata = TextureMetadata {
        name: recipe.name.clone(),
        recipe_hash,
        parameter_hash: resolved.parameter_hash().to_owned(),
        algorithm_version: TEXTURE_ALGORITHM_VERSION,
        seed: recipe.seed,
        physical_tile_size_m: [recipe.physical_tile_width_m, recipe.physical_tile_height_m],
        minimum_height_m: range.minimum,
        maximum_height_m: range.maximum,
        base_height_m: range.neutral,
        displacement: recipe.displacement.displacement_map,
        normal_convention,
    };
    TextureSet::new(albedo, height, normal, occlusion.clone(), metadata).map_err(TextureError::from)
}

fn albedo_config(recipe: &TextureRecipe) -> AlbedoConfig {
    AlbedoConfig {
        base_color: resolved_colour(&recipe.albedo.base_color),
        warm_color: resolved_colour(&recipe.albedo.warm_color),
        variation: recipe.albedo.variation,
        crack_darkening: recipe.albedo.crack_darkening,
        shoulder_variation: recipe.albedo.shoulder_variation,
        mineral_density: recipe.albedo.mineral_density,
        mineral_brightness: recipe.albedo.mineral_brightness,
        occlusion_influence: recipe.albedo.occlusion_influence,
    }
}

fn resolved_colour(colour: &ColourValue) -> [f32; 3] {
    colour
        .as_resolved()
        .expect("parameter references must be resolved before material evaluation")
        .channels()
}

fn occlusion_settings(recipe: &TextureRecipe) -> OcclusionSettings {
    OcclusionSettings {
        directions: recipe.occlusion.directions,
        samples: recipe.occlusion.samples,
        radius: recipe.occlusion.radius,
        max_radius: recipe.occlusion.max_radius,
        cavity_strength: recipe.occlusion.cavity_strength,
        horizon_strength: recipe.occlusion.horizon_strength,
        power: recipe.occlusion.power,
        combine: match recipe.occlusion.combine {
            recipe::OcclusionCombine::Multiply => OcclusionCombine::Multiply,
            recipe::OcclusionCombine::WeightedMinimum {
                cavity_weight,
                horizon_weight,
            } => OcclusionCombine::WeightedMinimum {
                cavity_weight,
                horizon_weight,
            },
        },
    }
}

/// Writes a complete texture set as portable PNG files and a final manifest.
///
/// # Errors
///
/// Returns an error when the destination is unsafe to replace or a map cannot
/// be encoded or written atomically.
pub fn write_texture_set(
    textures: &TextureSet,
    destination: &Path,
    options: &OutputOptions,
) -> Result<OutputManifest, TextureError> {
    let images = TextureSetImages::from_texture_set(textures);
    encoding::write_texture_set(&images, destination, options).map_err(TextureError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe() -> TextureRecipe {
        serde_json::from_value(serde_json::json!({
            "name": "test_stone",
            "seed": 42,
            "width": 32,
            "height": 32,
            "physical_tile_width_m": 4.0,
            "physical_tile_height_m": 4.0,
            "material": { "kind": "cracked_stone" },
            "layers": [],
            "normal_scale": 1.0,
            "displacement": {
                "minimum_m": -0.2,
                "maximum_m": 0.2,
                "base_m": 0.0,
                "displacement_map": true
            },
            "occlusion": {},
            "albedo": {},
            "output_profiles": ["motu_unity_terrain"]
        }))
        .expect("recipe")
    }

    #[test]
    fn generation_is_deterministic_and_maps_share_dimensions() {
        let recipe = recipe();
        let first = generate_texture_set(&recipe, NormalConvention::OpenGl).expect("first bake");
        let second = generate_texture_set(&recipe, NormalConvention::OpenGl).expect("second bake");
        assert_eq!(first, second);
        assert_eq!(first.dimensions, TextureDimensions::new(32, 32).unwrap());
    }

    #[test]
    fn caller_selects_normal_convention_without_changing_recipe_content() {
        let recipe = recipe();
        let open_gl = generate_texture_set(&recipe, NormalConvention::OpenGl).expect("OpenGL");
        let direct_x = generate_texture_set(&recipe, NormalConvention::DirectX).expect("DirectX");

        assert_eq!(open_gl.albedo, direct_x.albedo);
        assert_eq!(open_gl.height, direct_x.height);
        assert_eq!(open_gl.occlusion, direct_x.occlusion);
        assert_eq!(open_gl.metadata.recipe_hash, direct_x.metadata.recipe_hash);
        assert_eq!(open_gl.metadata.normal_convention, NormalConvention::OpenGl);
        assert_eq!(
            direct_x.metadata.normal_convention,
            NormalConvention::DirectX
        );
        assert_ne!(open_gl.normal, direct_x.normal);
    }
}
