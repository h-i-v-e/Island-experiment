//! Engine-neutral material preview generation.
//!
//! Preview generation deliberately shares the final evaluator and texture-set
//! conversion. The result owns all CPU maps so a renderer can move it to a
//! background task without borrowing document state or pulling in an engine.

use std::time::Instant;

use super::{
    MaterialEvaluation, TextureError, TextureRecipe, TextureSet,
    encoding::normalized_recipe_hash,
    image::{FloatImage, Rgba8Image, TextureDimensions},
    layer_stack::LayerDiagnostic,
    texture_set_from_evaluation,
};

/// Settings that affect one generated preview without changing the recipe.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewSettings {
    /// Stable layer identifier whose raw, remapped, and mask maps should be
    /// returned for inspection.
    pub selected_layer_id: Option<String>,
}

/// Timings for the CPU stages of one preview generation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewTimings {
    /// Time spent evaluating the base field, layers, occlusion, and albedo.
    pub evaluate_ms: f64,
    /// Time spent converting the evaluation into texture maps and preview
    /// diagnostics.
    pub texture_set_ms: f64,
    /// End-to-end time spent by [`generate_preview`].
    pub total_ms: f64,
}

/// Typed CPU maps produced for one selected layer.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerPreviewMaps {
    /// Source value before the layer remap.
    pub raw: FloatImage,
    /// Value after the layer remap.
    pub remapped: FloatImage,
    /// Effective opacity after the layer mask.
    pub mask: FloatImage,
}

/// Complete, coherent CPU preview output.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewMaps {
    /// Final albedo, height, normal, and occlusion maps.
    pub textures: TextureSet,
    /// Unity terrain packed mask, when a packed map is requested by the
    /// preview contract. The current preview always requests this map.
    pub packed_mask: Option<Rgba8Image>,
    /// Diagnostics for the selected layer, if one was requested and found.
    pub selected_layer: Option<LayerPreviewMaps>,
    /// SHA-256 hash of the normalized effective recipe.
    pub recipe_hash: String,
    /// CPU generation timings in milliseconds.
    pub timings_ms: PreviewTimings,
}

/// Generates one complete preview from the effective recipe.
///
/// The caller owns any requested preview-resolution override and should pass
/// that effective recipe directly. This function validates and evaluates the
/// same recipe path used by final baking, then builds all preview maps before
/// returning so consumers cannot observe a partially replaced map set.
///
/// # Errors
///
/// Returns [`TextureError`] when recipe validation or any shared map
/// conversion fails.
pub fn generate_preview(
    effective_recipe: &TextureRecipe,
    settings: &PreviewSettings,
) -> Result<PreviewMaps, TextureError> {
    let started = Instant::now();
    let evaluation_started = Instant::now();
    let evaluation = super::evaluate_material(effective_recipe)?;
    let evaluate_ms = elapsed_ms(evaluation_started);

    let texture_started = Instant::now();
    let textures = texture_set_from_evaluation(effective_recipe, &evaluation)?;
    let packed_mask = Some(packed_mask_from_texture_set(&textures)?);
    let dimensions = textures.dimensions;
    let selected_layer = selected_layer_maps(
        &evaluation,
        dimensions,
        settings.selected_layer_id.as_deref(),
    )?;
    let texture_set_ms = elapsed_ms(texture_started);
    let timings_ms = PreviewTimings {
        evaluate_ms,
        texture_set_ms,
        total_ms: elapsed_ms(started),
    };

    // `texture_set_from_evaluation` already computes this hash after the
    // shared validation boundary. Keep the explicit normalized-hash call out
    // of the hot path and use the metadata that all maps carry.
    let recipe_hash = textures.metadata.recipe_hash.clone();
    debug_assert_eq!(
        normalized_recipe_hash(effective_recipe).ok().as_deref(),
        Some(recipe_hash.as_str())
    );

    Ok(PreviewMaps {
        textures,
        packed_mask,
        selected_layer,
        recipe_hash,
        timings_ms,
    })
}

/// Builds typed diagnostics for every evaluated layer.
///
/// This adapter is useful to file protocols that retain the historical
/// all-layer diagnostic output while the in-process preview API only keeps
/// the selected layer in its result.
///
/// # Errors
///
/// Returns [`TextureError::Image`] when an evaluated diagnostic has an
/// inconsistent pixel buffer.
pub fn layer_preview_maps(
    evaluation: &MaterialEvaluation,
) -> Result<Vec<(String, LayerPreviewMaps)>, TextureError> {
    let dimensions = dimensions_from_evaluation(evaluation);
    evaluation
        .layers
        .layers
        .iter()
        .map(|diagnostic| {
            Ok((
                diagnostic.id.clone(),
                layer_preview_maps_for_diagnostic(diagnostic, dimensions)?,
            ))
        })
        .collect()
}

fn selected_layer_maps(
    evaluation: &MaterialEvaluation,
    dimensions: TextureDimensions,
    selected_layer_id: Option<&str>,
) -> Result<Option<LayerPreviewMaps>, TextureError> {
    selected_layer_id
        .and_then(|selected_id| {
            evaluation
                .layers
                .layers
                .iter()
                .find(|diagnostic| diagnostic.id == selected_id)
        })
        .map(|diagnostic| layer_preview_maps_for_diagnostic(diagnostic, dimensions))
        .transpose()
}

fn layer_preview_maps_for_diagnostic(
    diagnostic: &LayerDiagnostic,
    dimensions: TextureDimensions,
) -> Result<LayerPreviewMaps, TextureError> {
    Ok(LayerPreviewMaps {
        raw: FloatImage::new(dimensions, diagnostic.raw.clone())?,
        remapped: FloatImage::new(dimensions, diagnostic.remapped.clone())?,
        mask: FloatImage::new(dimensions, diagnostic.mask.clone())?,
    })
}

fn dimensions_from_evaluation(evaluation: &MaterialEvaluation) -> TextureDimensions {
    let dimensions = evaluation.layers.field.dimensions();
    TextureDimensions::new_unchecked(dimensions.width, dimensions.height)
}

fn packed_mask_from_texture_set(textures: &TextureSet) -> Result<Rgba8Image, TextureError> {
    let pixels = textures
        .height
        .pixels()
        .iter()
        .zip(textures.occlusion.pixels())
        .map(|(&height, &occlusion)| [(height >> 8) as u8, occlusion, 0, u8::MAX])
        .collect();
    Rgba8Image::new(textures.dimensions, pixels).map_err(TextureError::from)
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedural_textures::{TextureDimensions, generate_texture_set};

    fn recipe() -> TextureRecipe {
        serde_json::from_value(serde_json::json!({
            "name": "preview_test",
            "seed": 42,
            "width": 8,
            "height": 6,
            "physical_tile_width_m": 4.0,
            "physical_tile_height_m": 3.0,
            "material": { "kind": "layered_noise" },
            "layers": [{
                "id": "detail",
                "name": "Detail",
                "source": { "kind": "value", "frequency": 2 },
                "outputs": {
                    "height": { "enabled": true, "strength_m": 0.01 }
                }
            }],
            "normal_convention": "open_gl",
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
    fn preview_matches_final_texture_set_and_builds_selected_maps() {
        let recipe = recipe();
        let preview = generate_preview(
            &recipe,
            &PreviewSettings {
                selected_layer_id: Some("detail".into()),
            },
        )
        .expect("preview");
        let final_set = generate_texture_set(&recipe).expect("final texture set");
        assert_eq!(preview.textures, final_set);
        assert_eq!(preview.recipe_hash, final_set.metadata.recipe_hash);
        assert_eq!(
            preview.textures.dimensions,
            TextureDimensions::new(8, 6).unwrap()
        );
        assert_eq!(
            preview.packed_mask.as_ref().map(Rgba8Image::dimensions),
            Some(preview.textures.dimensions)
        );
        let selected = preview.selected_layer.expect("selected layer");
        assert_eq!(selected.raw.dimensions(), preview.textures.dimensions);
        assert_eq!(selected.remapped.dimensions(), preview.textures.dimensions);
        assert_eq!(selected.mask.dimensions(), preview.textures.dimensions);
        assert!(preview.timings_ms.total_ms >= preview.timings_ms.evaluate_ms);
    }

    #[test]
    fn unknown_selected_layer_is_omitted_without_affecting_maps() {
        let recipe = recipe();
        let preview = generate_preview(
            &recipe,
            &PreviewSettings {
                selected_layer_id: Some("missing".into()),
            },
        )
        .expect("preview");
        assert!(preview.selected_layer.is_none());
        assert_eq!(preview.textures.metadata.name, recipe.name);
    }

    #[test]
    fn all_layer_diagnostics_preserve_stable_ids_and_dimensions() {
        let recipe = recipe();
        let evaluation = crate::procedural_textures::evaluate_material(&recipe).expect("evaluate");
        let maps = layer_preview_maps(&evaluation).expect("layer maps");
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].0, "detail");
        assert_eq!(
            maps[0].1.raw.dimensions(),
            TextureDimensions::new(8, 6).unwrap()
        );
    }

    #[test]
    fn packed_mask_uses_final_height_bytes() {
        let recipe = recipe();
        let preview = generate_preview(&recipe, &PreviewSettings::default()).expect("preview");
        let packed = preview.packed_mask.expect("packed mask");
        for ((&height, &occlusion), pixel) in preview
            .textures
            .height
            .pixels()
            .iter()
            .zip(preview.textures.occlusion.pixels())
            .zip(packed.pixels())
        {
            assert_eq!(*pixel, [(height >> 8) as u8, occlusion, 0, 255]);
        }
    }

    #[test]
    fn preview_settings_default_has_no_selected_layer() {
        assert_eq!(PreviewSettings::default().selected_layer_id, None);
        assert_eq!(recipe().output_profiles.len(), 1);
    }
}
