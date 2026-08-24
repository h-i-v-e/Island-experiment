//! Material-local ambient occlusion derived from the unquantized height field.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc
)]

use std::f32::consts::TAU;

use super::field_program::HeightField;
use super::image::{Gray8Image, ImageError, TextureDimensions};

/// Deterministic quality and response controls for local relief occlusion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OcclusionSettings {
    /// Number of evenly spaced horizon directions. Eight is the final preset.
    pub directions: u8,
    /// Number of exponentially spaced samples in each direction.
    pub samples: u8,
    /// Initial horizon radius in pixels.
    pub radius: f32,
    /// Largest radius multiplier relative to `radius`.
    pub max_radius: f32,
    /// Strength of the cavity term.
    pub cavity_strength: f32,
    /// Strength of the horizon term.
    pub horizon_strength: f32,
    /// Response curve applied after combining the terms.
    pub power: f32,
    /// Combination policy for cavity and horizon openness.
    pub combine: OcclusionCombine,
}

/// Policies for combining cavity and horizon openness terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OcclusionCombine {
    /// Multiply independent terms.
    Multiply,
    /// Weighted minimum, useful for a softer preview quality preset.
    WeightedMinimum {
        cavity_weight: f32,
        horizon_weight: f32,
    },
}

impl Default for OcclusionSettings {
    fn default() -> Self {
        Self {
            directions: 8,
            samples: 6,
            radius: 1.0,
            max_radius: 8.0,
            cavity_strength: 1.5,
            horizon_strength: 0.85,
            power: 1.0,
            combine: OcclusionCombine::Multiply,
        }
    }
}

/// An owned linear Gray8 occlusion image. Byte 255 means fully open.
pub type OcclusionImage = Gray8Image;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcclusionError {
    Image(ImageError),
    InvalidSettings,
}

impl From<ImageError> for OcclusionError {
    fn from(error: ImageError) -> Self {
        Self::Image(error)
    }
}

/// Derives local cavity plus wrapped horizon occlusion from `field`.
pub fn derive_occlusion(
    field: &HeightField,
    settings: OcclusionSettings,
) -> Result<OcclusionImage, OcclusionError> {
    validate_settings(settings)?;
    let dimensions = field.dimensions();
    let mut pixels = Vec::with_capacity(dimensions.pixel_count());
    for y in 0..dimensions.height {
        for x in 0..dimensions.width {
            let value = occlusion_at(field, x, y, settings);
            pixels.push(quantize_occlusion(value));
        }
    }
    Gray8Image::new(
        TextureDimensions::new(dimensions.width, dimensions.height)?,
        pixels,
    )
    .map_err(OcclusionError::from)
}

/// Computes one unquantized occlusion value in `[0, 1]`.
#[must_use]
pub fn occlusion_at(field: &HeightField, x: u32, y: u32, settings: OcclusionSettings) -> f32 {
    let dimensions = field.dimensions();
    let current = field.at(x, y);
    let (pixel_width, pixel_height) = dimensions.pixel_size();

    // A three-radius local average captures narrow crack floors without
    // requiring a second scratch image.  The fixed order is part of the
    // deterministic output contract.
    let cavity_offsets = [1.0_f32, 2.0, 4.0];
    let mut cavity = 0.0;
    let mut cavity_weight = 0.0;
    for &radius in &cavity_offsets {
        let count = 8.0;
        let mut average = 0.0;
        for index in 0..8 {
            let angle = TAU * index as f32 / count;
            let sample = field.sample_bilinear_wrapped(
                x as f32 + angle.cos() * radius,
                y as f32 + angle.sin() * radius,
            );
            average += sample;
        }
        average /= count;
        // Only material below its neighbourhood is cavity; exposed peaks are
        // left open by this term.
        let positive_difference = (average - current).max(0.0);
        let scale = (radius * pixel_width.min(pixel_height)).max(f32::EPSILON);
        cavity += (positive_difference / scale).min(1.0) / radius;
        cavity_weight += 1.0 / radius;
    }
    cavity = (cavity / cavity_weight) * settings.cavity_strength;
    let cavity_factor = (1.0 - cavity).clamp(0.0, 1.0);

    let mut horizon = 0.0;
    for direction in 0..settings.directions {
        let angle = TAU * f32::from(direction) / f32::from(settings.directions);
        let direction_x = angle.cos();
        let direction_y = angle.sin();
        let mut direction_horizon: f32 = 0.0;
        for sample_index in 0..settings.samples {
            let t = if settings.samples == 1 {
                0.0
            } else {
                f32::from(sample_index) / f32::from(settings.samples - 1)
            };
            let radius = settings.radius * settings.max_radius.powf(t);
            let sample = field.sample_bilinear_wrapped(
                x as f32 + direction_x * radius,
                y as f32 + direction_y * radius,
            );
            let distance_metres = radius
                * (pixel_width * direction_x)
                    .hypot(pixel_height * direction_y)
                    .max(f32::EPSILON);
            direction_horizon = direction_horizon.max((sample - current) / distance_metres);
        }
        horizon += direction_horizon.max(0.0);
    }
    horizon /= f32::from(settings.directions);
    let horizon_factor = (1.0 - (horizon * settings.horizon_strength).max(0.0)).clamp(0.0, 1.0);
    let openness = match settings.combine {
        OcclusionCombine::Multiply => cavity_factor * horizon_factor,
        OcclusionCombine::WeightedMinimum {
            cavity_weight,
            horizon_weight,
        } => {
            let cavity_weight = cavity_weight.clamp(0.0, 1.0);
            let horizon_weight = horizon_weight.clamp(0.0, 1.0);
            cavity_factor
                .powf(cavity_weight)
                .min(horizon_factor.powf(horizon_weight))
        }
    };
    openness.powf(settings.power).clamp(0.0, 1.0)
}

fn validate_settings(settings: OcclusionSettings) -> Result<(), OcclusionError> {
    if settings.directions == 0
        || settings.directions > 32
        || settings.samples == 0
        || settings.samples > 16
        || !settings.radius.is_finite()
        || !settings.max_radius.is_finite()
        || !settings.cavity_strength.is_finite()
        || !settings.horizon_strength.is_finite()
        || !settings.power.is_finite()
        || settings.radius <= 0.0
        || settings.max_radius < 1.0
        || settings.cavity_strength < 0.0
        || settings.horizon_strength < 0.0
        || settings.power <= 0.0
        || matches!(
            settings.combine,
            OcclusionCombine::WeightedMinimum {
                cavity_weight,
                horizon_weight
            } if !cavity_weight.is_finite()
                || !horizon_weight.is_finite()
                || cavity_weight < 0.0
                || horizon_weight < 0.0
                || (cavity_weight + horizon_weight) <= 0.0
        )
    {
        return Err(OcclusionError::InvalidSettings);
    }
    Ok(())
}

#[must_use]
fn quantize_occlusion(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::procedural_textures::field_program::{FieldDimensions, HeightField};

    fn field(width: u32, height: u32, values: Vec<f32>) -> HeightField {
        HeightField::new(
            FieldDimensions::new(width, height, 4.0, 4.0).expect("dimensions"),
            values,
        )
        .expect("field")
    }

    #[test]
    fn flat_height_is_fully_open() {
        let field = field(16, 16, vec![0.0; 256]);
        let image = derive_occlusion(&field, OcclusionSettings::default()).expect("occlusion");
        assert!(image.pixels().iter().all(|&value| value == 255));
    }

    #[test]
    fn trench_floor_is_darker_than_plateau() {
        let mut values = vec![0.0; 32 * 32];
        for y in 12..20 {
            for x in 12..20 {
                values[y * 32 + x] = -0.4;
            }
        }
        let field = field(32, 32, values);
        let settings = OcclusionSettings {
            radius: 0.75,
            max_radius: 8.0,
            ..OcclusionSettings::default()
        };
        let image = derive_occlusion(&field, settings).expect("occlusion");
        let floor = image.pixels()[16 * 32 + 16];
        let plateau = image.pixels()[4 * 32 + 4];
        assert!(floor < plateau);
        assert!(plateau > 220);
    }

    #[test]
    fn wrapped_occlusion_is_deterministic_at_edges() {
        let mut values = vec![0.0; 8 * 8];
        values[0] = -1.0;
        let field = field(8, 8, values);
        let settings = OcclusionSettings::default();
        let a = occlusion_at(&field, 0, 0, settings);
        let b = occlusion_at(&field, 8, 8, settings);
        assert_eq!(a, b);
    }

    #[test]
    fn unsafe_quality_is_rejected() {
        assert_eq!(
            derive_occlusion(
                &field(2, 2, vec![0.0; 4]),
                OcclusionSettings {
                    directions: 0,
                    ..OcclusionSettings::default()
                }
            ),
            Err(OcclusionError::InvalidSettings)
        );
    }
}
