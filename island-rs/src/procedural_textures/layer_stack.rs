//! Ordered material-layer evaluation shared by bake and preview paths.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::too_many_arguments
)]

use super::{
    cellular,
    field_program::{self, FieldError, HeightField},
    noise,
    periodic::Period2D,
    recipe::{
        AlbedoBlend, ColourMap, HeightBlend, LayerMask, MaterialLayer, ScalarRemap, ScalarSource,
        SourceKind, TextureRecipe,
    },
};

/// Per-layer scalar diagnostics retained for editor previews.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerDiagnostic {
    /// Stable layer identifier.
    pub id: String,
    /// Source value before remapping.
    pub raw: Vec<f32>,
    /// Value after the layer remap.
    pub remapped: Vec<f32>,
    /// Effective opacity after the optional layer mask.
    pub mask: Vec<f32>,
}

/// Result of evaluating the base field and ordered height stack.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerEvaluation {
    /// Final unquantized height field.
    pub field: HeightField,
    /// Raw/remapped/mask maps for every layer in order.
    pub layers: Vec<LayerDiagnostic>,
}

impl LayerEvaluation {
    /// Applies all enabled albedo bindings in layer order to linear RGB pixels.
    pub fn apply_albedo(
        &self,
        layers: &[MaterialLayer],
        colours: &mut [[f32; 3]],
    ) -> Result<(), FieldError> {
        if layers.len() != self.layers.len() {
            return Err(FieldError::DimensionOverflow);
        }
        if colours.len() != self.field.dimensions().pixel_count() {
            return Err(FieldError::DimensionOverflow);
        }
        for (layer, diagnostic) in layers.iter().zip(&self.layers) {
            if !layer.enabled || !layer.outputs.albedo.enabled {
                continue;
            }
            apply_albedo_layer(layer, diagnostic, colours)?;
        }
        Ok(())
    }
}

/// Evaluates a complete recipe through the shared base and layer evaluator.
pub fn evaluate_recipe(recipe: &TextureRecipe) -> Result<LayerEvaluation, FieldError> {
    let base = field_program::FieldProgram::evaluate_base_recipe(recipe)?;
    evaluate_layers(base, &recipe.layers, recipe.seed)
}

/// Evaluates the ordered stack over an already-generated base field.
pub fn evaluate_layers(
    base: HeightField,
    layers: &[MaterialLayer],
    seed: u64,
) -> Result<LayerEvaluation, FieldError> {
    let dimensions = base.dimensions();
    if layers.len() > super::recipe::MAX_LAYERS {
        return Err(FieldError::NonFiniteParameter);
    }
    let pixel_count = dimensions.pixel_count();
    let mut field = base;
    let mut diagnostics = Vec::with_capacity(layers.len());

    for layer in layers {
        let raw = evaluate_source_map(&layer.source, dimensions, seed)?;
        let remapped = raw
            .iter()
            .map(|&value| apply_remap(&layer.remap, value))
            .collect::<Vec<_>>();
        let mask = evaluate_mask(layer, &remapped, &field, &diagnostics, dimensions, seed)?;
        if mask.len() != pixel_count {
            return Err(FieldError::DimensionOverflow);
        }
        if layer.enabled && layer.outputs.height.enabled {
            apply_height_output(
                field.values_mut(),
                &raw,
                &remapped,
                &mask,
                &layer.remap,
                &layer.outputs.height.blend,
                layer.outputs.height.strength_m,
            )?;
        }
        diagnostics.push(LayerDiagnostic {
            id: layer.id.clone(),
            raw,
            remapped,
            mask,
        });
    }
    if field.values().iter().any(|value| !value.is_finite()) {
        return Err(FieldError::NonFiniteParameter);
    }
    Ok(LayerEvaluation {
        field,
        layers: diagnostics,
    })
}

/// Applies only ordered height outputs, preserving the old field-program API.
pub(crate) fn apply_height_layers(
    field: &mut HeightField,
    layers: &[MaterialLayer],
    seed: u64,
) -> Result<(), FieldError> {
    let evaluated = evaluate_layers(field.clone(), layers, seed)?;
    *field = evaluated.field;
    Ok(())
}

fn evaluate_source_map(
    source: &ScalarSource,
    dimensions: field_program::FieldDimensions,
    seed: u64,
) -> Result<Vec<f32>, FieldError> {
    if source.frequency == 0 {
        return Err(FieldError::NonFiniteParameter);
    }
    let period = Period2D::new(source.frequency, source.frequency)
        .map_err(|_| FieldError::NonFiniteParameter)?;
    let pixel_count = dimensions.pixel_count();
    let mut values = Vec::with_capacity(pixel_count);
    for y in 0..dimensions.height {
        let v = (y as f32 + 0.5) / dimensions.height as f32;
        for x in 0..dimensions.width {
            let u = (x as f32 + 0.5) / dimensions.width as f32;
            let mut position = [
                u * source.frequency as f32 + source.offset[0] * source.frequency as f32,
                v * source.frequency as f32 + source.offset[1] * source.frequency as f32,
            ];
            let source_seed = seed ^ source.seed_domain;
            if let Some(warp) = source.domain_warp {
                if warp.frequency == 0 {
                    return Err(FieldError::NonFiniteParameter);
                }
                position = noise::domain_warp(
                    source_seed ^ warp.seed_domain,
                    position,
                    period,
                    warp.amplitude,
                    warp.frequency as f32,
                    warp.octaves,
                    warp.lacunarity,
                    warp.gain,
                );
            }
            let value = sample_source(source, source_seed, position, period)?;
            values.push(value);
        }
    }
    Ok(values)
}

fn sample_source(
    source: &ScalarSource,
    seed: u64,
    position: [f32; 2],
    period: Period2D,
) -> Result<f32, FieldError> {
    let value = match source.kind {
        SourceKind::Value => noise::value(seed, position, period),
        SourceKind::Fbm => noise::fbm(
            seed,
            position,
            period,
            source.octaves,
            source.lacunarity,
            source.gain,
        ),
        SourceKind::Billow => noise::billow(
            seed,
            position,
            period,
            source.octaves,
            source.lacunarity,
            source.gain,
        ),
        SourceKind::Ridged => noise::ridged(
            seed,
            position,
            period,
            source.octaves,
            source.lacunarity,
            source.gain,
        ),
        SourceKind::CellularDistance => {
            cellular::sample(seed, position, period, source.cellular_jitter).nearest_distance
        }
        SourceKind::CellularDistanceToEdge => {
            cellular::sample(seed, position, period, source.cellular_jitter).edge_distance
        }
        SourceKind::CellularValue => {
            cellular::sample(seed, position, period, source.cellular_jitter).cell_value
        }
    };
    value
        .is_finite()
        .then_some(value)
        .ok_or(FieldError::NonFiniteParameter)
}

fn evaluate_mask(
    layer: &MaterialLayer,
    own_remapped: &[f32],
    previous_height: &HeightField,
    previous: &[LayerDiagnostic],
    dimensions: field_program::FieldDimensions,
    seed: u64,
) -> Result<Vec<f32>, FieldError> {
    match &layer.mask {
        None => Ok(vec![1.0; dimensions.pixel_count()]),
        Some(LayerMask::Own) => Ok(own_remapped
            .iter()
            .map(|value| value.clamp(0.0, 1.0))
            .collect()),
        Some(LayerMask::PreviousHeight {
            bottom_m,
            top_m,
            invert,
        }) => Ok(previous_height
            .values()
            .iter()
            .map(|&height| {
                let mask = field_program::smoothstep(*bottom_m, *top_m, height);
                if *invert { 1.0 - mask } else { mask }
            })
            .collect()),
        Some(LayerMask::Noise { source, remap }) => {
            let raw = evaluate_source_map(source, dimensions, seed)?;
            Ok(raw
                .iter()
                .map(|&value| apply_remap(remap, value).clamp(0.0, 1.0))
                .collect())
        }
        Some(LayerMask::Layer { layer_id, remap }) => previous
            .iter()
            .find(|diagnostic| diagnostic.id == *layer_id)
            .map(|diagnostic| {
                diagnostic
                    .remapped
                    .iter()
                    .map(|&value| {
                        let mapped = if *remap == ScalarRemap::default() {
                            value
                        } else {
                            apply_remap(remap, value)
                        };
                        mapped.clamp(0.0, 1.0)
                    })
                    .collect()
            })
            .ok_or(FieldError::NonFiniteParameter),
    }
}

fn apply_height_output(
    values: &mut [f32],
    raw: &[f32],
    remapped: &[f32],
    mask: &[f32],
    remap: &ScalarRemap,
    blend: &HeightBlend,
    strength_m: f32,
) -> Result<(), FieldError> {
    if !strength_m.is_finite() {
        return Err(FieldError::NonFiniteParameter);
    }
    for (((value, &raw_scalar), &scalar), &opacity) in
        values.iter_mut().zip(raw).zip(remapped).zip(mask)
    {
        // Scalar remaps are normalized to [0, 1] by default. Height routing
        // treats the midpoint as neutral so a signed noise source can retain
        // the old zero-centred displacement contract.
        let signed_scalar = if *remap == ScalarRemap::default() {
            raw_scalar
        } else {
            scalar * 2.0 - 1.0
        };
        let target = signed_scalar * opacity * strength_m;
        *value = match blend {
            HeightBlend::Replace => target,
            HeightBlend::Add => *value + target,
            HeightBlend::Subtract => *value - target,
            HeightBlend::Multiply => *value * target,
            HeightBlend::Minimum => (*value).min(target),
            HeightBlend::Maximum => (*value).max(target),
            HeightBlend::Lerp { amount } => {
                if !amount.is_finite() {
                    return Err(FieldError::NonFiniteParameter);
                }
                field_program::lerp(*value, target, amount.clamp(0.0, 1.0))
            }
        };
    }
    Ok(())
}

/// Applies the remap contract to one source scalar.
#[must_use]
pub fn apply_remap(remap: &ScalarRemap, value: f32) -> f32 {
    let denominator = remap.input_max - remap.input_min;
    let mut mapped = if denominator.abs() <= f32::EPSILON {
        f32::from(value >= remap.input_max)
    } else {
        (value - remap.input_min) / denominator
    };
    if remap.invert {
        mapped = 1.0 - mapped;
    }
    mapped = (mapped - 0.5) * remap.contrast + 0.5 + remap.bias;
    if let Some(points) = &remap.curve {
        mapped = sample_curve(points, mapped);
    }
    if remap.clamp {
        mapped.clamp(0.0, 1.0)
    } else {
        mapped
    }
}

fn sample_curve(points: &[super::recipe::RemapPoint], value: f32) -> f32 {
    let Some(first) = points.first() else {
        return value;
    };
    let Some(last) = points.last() else {
        return value;
    };
    if value <= first.position {
        return first.value;
    }
    if value >= last.position {
        return last.value;
    }
    for pair in points.windows(2) {
        let [left, right] = pair else {
            continue;
        };
        if value <= right.position {
            let amount = (value - left.position) / (right.position - left.position);
            return field_program::lerp(left.value, right.value, amount);
        }
    }
    value
}

fn apply_albedo_layer(
    layer: &MaterialLayer,
    diagnostic: &LayerDiagnostic,
    colours: &mut [[f32; 3]],
) -> Result<(), FieldError> {
    let output = &layer.outputs.albedo;
    if !output.strength.is_finite()
        || !output.hue_influence.is_finite()
        || !output.saturation_influence.is_finite()
        || !output.value_influence.is_finite()
    {
        return Err(FieldError::NonFiniteParameter);
    }
    for ((colour, &scalar), &opacity) in colours
        .iter_mut()
        .zip(&diagnostic.remapped)
        .zip(&diagnostic.mask)
    {
        let mut mapped = colour_map_sample(&output.colour_map, scalar);
        apply_hsv_influence(
            &mut mapped,
            output.hue_influence,
            output.saturation_influence,
            output.value_influence,
        );
        let amount = (output.strength * opacity).clamp(0.0, 1.0);
        let current = *colour;
        *colour = match output.blend {
            AlbedoBlend::Replace | AlbedoBlend::Mix => std::array::from_fn(|index| {
                current[index] * (1.0 - amount) + mapped[index] * amount
            }),
            AlbedoBlend::Multiply => std::array::from_fn(|index| {
                current[index] * field_program::lerp(1.0, mapped[index], amount)
            }),
            AlbedoBlend::Add => {
                std::array::from_fn(|index| current[index] + mapped[index] * amount)
            }
            AlbedoBlend::Overlay => std::array::from_fn(|index| {
                let channel = current[index];
                let overlay = if channel <= 0.5 {
                    2.0 * channel * mapped[index]
                } else {
                    1.0 - 2.0 * (1.0 - channel) * (1.0 - mapped[index])
                };
                field_program::lerp(channel, overlay, amount)
            }),
        };
    }
    Ok(())
}

fn colour_map_sample(map: &ColourMap, value: f32) -> [f32; 3] {
    let value = value.clamp(0.0, 1.0);
    match map {
        ColourMap::Ramp { first, second } => {
            let first = resolved_colour(first);
            let second = resolved_colour(second);
            std::array::from_fn(|index| field_program::lerp(first[index], second[index], value))
        }
        ColourMap::Gradient { stops } => {
            let Some(first) = stops.first() else {
                return [0.0; 3];
            };
            let Some(last) = stops.last() else {
                return resolved_colour(&first.colour);
            };
            if value <= first.position {
                return resolved_colour(&first.colour);
            }
            if value >= last.position {
                return resolved_colour(&last.colour);
            }
            for pair in stops.windows(2) {
                let [left, right] = pair else {
                    continue;
                };
                if value <= right.position {
                    let amount = (value - left.position) / (right.position - left.position);
                    let left_colour = resolved_colour(&left.colour);
                    let right_colour = resolved_colour(&right.colour);
                    return std::array::from_fn(|index| {
                        field_program::lerp(left_colour[index], right_colour[index], amount)
                    });
                }
            }
            resolved_colour(&last.colour)
        }
    }
}

fn resolved_colour(colour: &super::parameters::ColourValue) -> [f32; 3] {
    colour
        .as_resolved()
        .expect("parameter references must be resolved before layer evaluation")
        .channels()
}

fn apply_hsv_influence(colour: &mut [f32; 3], hue: f32, saturation: f32, value: f32) {
    let (mut h, mut s, mut v) = rgb_to_hsv(*colour);
    h = (h + hue).rem_euclid(1.0);
    s = (s + saturation).clamp(0.0, 1.0);
    v = (v + value).clamp(0.0, 1.0);
    *colour = hsv_to_rgb(h, s, v);
}

fn rgb_to_hsv(colour: [f32; 3]) -> (f32, f32, f32) {
    let max = colour[0].max(colour[1]).max(colour[2]);
    let min = colour[0].min(colour[1]).min(colour[2]);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if (max - colour[0]).abs() <= f32::EPSILON {
        ((colour[1] - colour[2]) / delta / 6.0).rem_euclid(1.0)
    } else if (max - colour[1]).abs() <= f32::EPSILON {
        ((colour[2] - colour[0]) / delta + 2.0) / 6.0
    } else {
        ((colour[0] - colour[1]) / delta + 4.0) / 6.0
    };
    (
        hue,
        if max <= f32::EPSILON {
            0.0
        } else {
            delta / max
        },
        max,
    )
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    if saturation <= f32::EPSILON {
        return [value; 3];
    }
    let scaled = hue.rem_euclid(1.0) * 6.0;
    let sector = scaled.floor() as u32;
    let fraction = scaled - sector as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));
    match sector {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedural_textures::{
        field_program::{FieldDimensions, HeightField},
        recipe::{AlbedoOutput, HeightOutput, LayerOutputs},
    };

    const EPSILON: f32 = 1.0e-6;

    fn base() -> HeightField {
        HeightField::new(
            FieldDimensions::new(16, 16, 4.0, 4.0).expect("dimensions"),
            vec![0.0; 256],
        )
        .expect("field")
    }

    #[test]
    fn remap_supports_invert_contrast_bias_and_curve() {
        let remap = ScalarRemap {
            invert: true,
            contrast: 2.0,
            bias: 0.1,
            curve: Some(vec![
                super::super::recipe::RemapPoint {
                    position: 0.0,
                    value: 0.0,
                },
                super::super::recipe::RemapPoint {
                    position: 1.0,
                    value: 1.0,
                },
            ]),
            ..ScalarRemap::default()
        };
        assert!(apply_remap(&remap, 0.0) > 0.5);
    }

    #[test]
    fn layer_evaluation_is_periodic_and_deterministic() {
        let layer = MaterialLayer {
            id: "noise".into(),
            source: ScalarSource {
                frequency: 4,
                ..ScalarSource::default()
            },
            outputs: LayerOutputs {
                height: HeightOutput {
                    enabled: true,
                    ..HeightOutput::default()
                },
                ..LayerOutputs::default()
            },
            ..MaterialLayer::default()
        };
        let first = evaluate_layers(base(), std::slice::from_ref(&layer), 42).expect("first");
        let second = evaluate_layers(base(), std::slice::from_ref(&layer), 42).expect("second");
        assert_eq!(first, second);
        assert!(first.field.values().iter().all(|value| value.is_finite()));
    }

    #[test]
    fn earlier_layer_mask_uses_the_remapped_scalar_domain() {
        let first = MaterialLayer {
            id: "first".into(),
            ..MaterialLayer::default()
        };
        let second = MaterialLayer {
            id: "second".into(),
            mask: Some(LayerMask::Layer {
                layer_id: "first".into(),
                remap: ScalarRemap::default(),
            }),
            ..MaterialLayer::default()
        };
        let evaluation = evaluate_layers(base(), &[first, second], 42).expect("layers");
        assert_eq!(evaluation.layers[0].remapped, evaluation.layers[1].mask);
    }

    #[test]
    fn previous_height_mask_reads_the_incoming_physical_height_field() {
        let dimensions = FieldDimensions::new(4, 1, 4.0, 1.0).expect("dimensions");
        let incoming =
            HeightField::new(dimensions, vec![-0.02, 0.0, 0.01, 0.02]).expect("incoming height");
        let high = MaterialLayer {
            id: "high".into(),
            mask: Some(LayerMask::PreviousHeight {
                bottom_m: 0.0,
                top_m: 0.02,
                invert: false,
            }),
            ..MaterialLayer::default()
        };
        let low = MaterialLayer {
            id: "low".into(),
            mask: Some(LayerMask::PreviousHeight {
                bottom_m: 0.0,
                top_m: 0.02,
                invert: true,
            }),
            ..MaterialLayer::default()
        };

        let evaluation = evaluate_layers(incoming, &[high, low], 42).expect("layers");
        assert_eq!(evaluation.layers[0].mask, [0.0, 0.0, 0.5, 1.0]);
        assert_eq!(evaluation.layers[1].mask, [1.0, 1.0, 0.5, 0.0]);
    }

    #[test]
    fn every_scalar_source_is_finite_and_deterministic() {
        let kinds = [
            SourceKind::Value,
            SourceKind::Fbm,
            SourceKind::Billow,
            SourceKind::Ridged,
            SourceKind::CellularDistance,
            SourceKind::CellularDistanceToEdge,
            SourceKind::CellularValue,
        ];
        for kind in kinds {
            let layer = MaterialLayer {
                id: format!("{kind:?}"),
                source: ScalarSource {
                    kind,
                    frequency: 4,
                    ..ScalarSource::default()
                },
                ..MaterialLayer::default()
            };
            let first = evaluate_layers(base(), std::slice::from_ref(&layer), 73)
                .expect("source should evaluate");
            let second = evaluate_layers(base(), std::slice::from_ref(&layer), 73)
                .expect("source should be repeatable");
            assert_eq!(first, second, "{kind:?} was not deterministic");
            assert!(
                first.layers[0].raw.iter().all(|value| value.is_finite()),
                "{kind:?} produced a non-finite scalar"
            );
        }
    }

    #[test]
    fn every_height_blend_has_the_documented_numerical_response() {
        let cases = [
            (HeightBlend::Replace, 0.5),
            (HeightBlend::Add, 2.5),
            (HeightBlend::Subtract, 1.5),
            (HeightBlend::Multiply, 1.0),
            (HeightBlend::Minimum, 0.5),
            (HeightBlend::Maximum, 2.0),
            (HeightBlend::Lerp { amount: 0.5 }, 1.25),
        ];
        for (blend, expected) in cases {
            let mut values = [2.0];
            apply_height_output(
                &mut values,
                &[0.5],
                &[0.75],
                &[0.5],
                &ScalarRemap::default(),
                &blend,
                2.0,
            )
            .expect("height blend should evaluate");
            assert!(
                (values[0] - expected).abs() <= EPSILON,
                "{blend:?} produced {}, expected {expected}",
                values[0]
            );
        }
    }

    #[test]
    fn every_albedo_blend_has_the_documented_numerical_response() {
        let cases = [
            (AlbedoBlend::Replace, [0.52, 0.24, 0.36]),
            (AlbedoBlend::Mix, [0.52, 0.24, 0.36]),
            (AlbedoBlend::Multiply, [0.2, 0.24, 0.36]),
            (AlbedoBlend::Add, [0.6, 0.4, 0.6]),
            (AlbedoBlend::Overlay, [0.28, 0.24, 0.44]),
        ];
        for (blend, expected) in cases {
            let layer = MaterialLayer {
                outputs: LayerOutputs {
                    albedo: AlbedoOutput {
                        enabled: true,
                        blend,
                        strength: 0.8,
                        colour_map: ColourMap::Ramp {
                            first: [1.0, 0.0, 0.0].into(),
                            second: [1.0, 0.0, 0.0].into(),
                        },
                        ..AlbedoOutput::default()
                    },
                    ..LayerOutputs::default()
                },
                ..MaterialLayer::default()
            };
            let diagnostic = LayerDiagnostic {
                id: layer.id.clone(),
                raw: vec![0.0],
                remapped: vec![0.25],
                mask: vec![0.5],
            };
            let mut colour = [[0.2, 0.4, 0.6]];
            apply_albedo_layer(&layer, &diagnostic, &mut colour)
                .expect("albedo blend should evaluate");
            for (actual, expected) in colour[0].iter().zip(expected) {
                assert!((actual - expected).abs() <= EPSILON);
            }
        }
    }

    #[test]
    fn layers_route_to_height_albedo_both_or_neither_independently() {
        for (height_enabled, albedo_enabled) in
            [(true, false), (false, true), (true, true), (false, false)]
        {
            let layer = MaterialLayer {
                outputs: LayerOutputs {
                    height: HeightOutput {
                        enabled: height_enabled,
                        strength_m: 0.1,
                        ..HeightOutput::default()
                    },
                    albedo: AlbedoOutput {
                        enabled: albedo_enabled,
                        strength: 1.0,
                        colour_map: ColourMap::Ramp {
                            first: [0.0, 0.0, 0.0].into(),
                            second: [1.0, 1.0, 1.0].into(),
                        },
                        ..AlbedoOutput::default()
                    },
                },
                ..MaterialLayer::default()
            };
            let evaluation = evaluate_layers(base(), std::slice::from_ref(&layer), 91)
                .expect("layer should evaluate");
            let height_changed = evaluation
                .field
                .values()
                .iter()
                .any(|value| value.abs() > EPSILON);
            let mut colours = vec![[0.25, 0.25, 0.25]; 256];
            evaluation
                .apply_albedo(std::slice::from_ref(&layer), &mut colours)
                .expect("albedo routing should evaluate");
            let albedo_changed = colours
                .iter()
                .any(|colour| colour.iter().any(|value| (*value - 0.25).abs() > EPSILON));
            assert_eq!(height_changed, height_enabled);
            assert_eq!(albedo_changed, albedo_enabled);
        }
    }
}
