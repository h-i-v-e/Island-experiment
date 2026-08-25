//! Lighting-free linear colour construction and sRGB RGB8 encoding.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc
)]

use super::field_program::{HeightField, LayeredField, fbm, hash_unit, periodic_value, smoothstep};
use super::image::{ImageError, Rgb8Image, TextureDimensions};
use super::occlusion::OcclusionImage;

/// Palette and variation controls for a generated material albedo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlbedoConfig {
    pub base_color: [f32; 3],
    pub warm_color: [f32; 3],
    pub variation: f32,
    pub crack_darkening: f32,
    pub shoulder_variation: f32,
    pub mineral_density: f32,
    pub mineral_brightness: f32,
    pub occlusion_influence: f32,
}

impl Default for AlbedoConfig {
    fn default() -> Self {
        Self {
            base_color: [0.25, 0.27, 0.24],
            warm_color: [0.42, 0.36, 0.28],
            variation: 0.12,
            crack_darkening: 0.28,
            shoulder_variation: 0.06,
            mineral_density: 0.055,
            mineral_brightness: 0.25,
            occlusion_influence: 0.08,
        }
    }
}

/// An owned RGB8 sRGB albedo image.
pub type AlbedoImage = Rgb8Image;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlbedoError {
    Image(ImageError),
    InvalidConfig,
    OcclusionDimensionsMismatch,
}

impl From<ImageError> for AlbedoError {
    fn from(error: ImageError) -> Self {
        Self::Image(error)
    }
}

/// Builds an RGB8 sRGB albedo from the shared unquantized height field.
pub fn generate_albedo(
    field: &HeightField,
    config: AlbedoConfig,
    seed: u64,
) -> Result<AlbedoImage, AlbedoError> {
    generate_albedo_with_occlusion(field, config, seed, None)
}

/// Builds albedo with an optional, subtle AO influence. AO is intentionally
/// capped by the config so indirect lighting is not baked twice.
pub fn generate_albedo_with_occlusion(
    field: &HeightField,
    config: AlbedoConfig,
    seed: u64,
    occlusion: Option<&OcclusionImage>,
) -> Result<AlbedoImage, AlbedoError> {
    validate(config)?;
    let dimensions = field.dimensions();
    if let Some(occlusion) = occlusion
        && (occlusion.width() != dimensions.width || occlusion.height() != dimensions.height)
    {
        return Err(AlbedoError::OcclusionDimensionsMismatch);
    }
    let mut pixels = Vec::with_capacity(dimensions.pixel_count());
    for y in 0..dimensions.height {
        for x in 0..dimensions.width {
            let linear = albedo_at_linear(field, x, y, config, seed, occlusion);
            pixels.push(linear.map(encode_srgb));
        }
    }
    Rgb8Image::new(
        TextureDimensions::new(dimensions.width, dimensions.height)?,
        pixels,
    )
    .map_err(AlbedoError::from)
}

/// Builds the base albedo pass in linear RGB without applying occlusion.
///
/// Layered albedo bindings use this buffer so colour routing happens before
/// the optional final AO influence and before sRGB quantization.
pub fn generate_linear_albedo(
    field: &HeightField,
    config: AlbedoConfig,
    seed: u64,
) -> Result<Vec<[f32; 3]>, AlbedoError> {
    validate(config)?;
    let dimensions = field.dimensions();
    let mut pixels = Vec::with_capacity(dimensions.pixel_count());
    for y in 0..dimensions.height {
        for x in 0..dimensions.width {
            pixels.push(albedo_at_linear(field, x, y, config, seed, None));
        }
    }
    Ok(pixels)
}

/// Applies the configured subtle AO influence to an existing linear albedo
/// buffer.  This is deliberately separate from layer routing.
pub fn apply_occlusion_linear(
    pixels: &mut [[f32; 3]],
    dimensions: super::field_program::FieldDimensions,
    config: AlbedoConfig,
    occlusion: &OcclusionImage,
) -> Result<(), AlbedoError> {
    validate(config)?;
    if occlusion.width() != dimensions.width || occlusion.height() != dimensions.height {
        return Err(AlbedoError::OcclusionDimensionsMismatch);
    }
    if pixels.len() != dimensions.pixel_count() {
        return Err(AlbedoError::OcclusionDimensionsMismatch);
    }
    for (index, colour) in pixels.iter_mut().enumerate() {
        let open = f32::from(occlusion.pixels()[index]) / 255.0;
        let factor = 1.0 - config.occlusion_influence * (1.0 - open);
        for channel in colour {
            *channel = (*channel * factor).clamp(0.0, 1.0);
        }
    }
    Ok(())
}

/// Encodes a linear RGB buffer as the shared sRGB RGB8 image.
pub fn encode_linear_albedo(
    dimensions: super::field_program::FieldDimensions,
    pixels: &[[f32; 3]],
) -> Result<AlbedoImage, AlbedoError> {
    if pixels.len() != dimensions.pixel_count() {
        return Err(AlbedoError::OcclusionDimensionsMismatch);
    }
    let pixels = pixels.iter().map(|pixel| pixel.map(encode_srgb)).collect();
    Rgb8Image::new(
        TextureDimensions::new(dimensions.width, dimensions.height)?,
        pixels,
    )
    .map_err(AlbedoError::from)
}

/// Computes one linear-RGB albedo sample before colour-space conversion.
#[must_use]
pub fn albedo_at_linear(
    field: &HeightField,
    x: u32,
    y: u32,
    config: AlbedoConfig,
    seed: u64,
    occlusion: Option<&OcclusionImage>,
) -> [f32; 3] {
    let dimensions = field.dimensions();
    let current = field.at(x, y);
    let broad = fbm(
        seed.wrapping_add(31),
        (x as f32 + 0.5) / dimensions.width as f32,
        (y as f32 + 0.5) / dimensions.height as f32,
        LayeredField {
            frequency: 2.0,
            amplitude: 1.0,
            octaves: 3,
            lacunarity: 2.1,
            gain: 0.5,
            offset: 0.0,
        },
    );
    let medium = periodic_value(
        seed.wrapping_add(37),
        (x as f32 + 0.5) / dimensions.width as f32 * 7.0,
        (y as f32 + 0.5) / dimensions.height as f32 * 7.0,
        7,
        7,
        0x414c_4245_444f_5f4d,
    );
    let average = cardinal_average(field, x, y);
    let cavity = smoothstep(0.0, 0.08, average - current);
    let shoulder = smoothstep(0.0, 0.06, (average - current).abs());
    let blend = ((broad * 0.5 + 0.5) * 0.65 + (medium * 0.5 + 0.5) * 0.35).clamp(0.0, 1.0);
    let mut colour = [0.0; 3];
    for (channel, value) in colour.iter_mut().enumerate() {
        *value = config.base_color[channel]
            + (config.warm_color[channel] - config.base_color[channel]) * blend * config.variation
            + medium * config.shoulder_variation * shoulder;
        *value *= 1.0 - config.crack_darkening * cavity;
    }

    let fleck = hash_unit(
        seed,
        x.cast_signed(),
        y.cast_signed(),
        0x464c_4543_4b5f_4841,
    );
    if fleck < config.mineral_density {
        let amount =
            config.mineral_brightness * (1.0 - fleck / config.mineral_density.max(f32::EPSILON));
        for channel in &mut colour {
            *channel += amount;
        }
    }
    if let Some(occlusion) = occlusion {
        let byte = occlusion.pixels()[(y as usize * dimensions.width as usize) + x as usize];
        let open = f32::from(byte) / 255.0;
        let factor = 1.0 - config.occlusion_influence * (1.0 - open);
        for channel in &mut colour {
            *channel *= factor;
        }
    }
    colour.map(|channel| channel.clamp(0.0, 1.0))
}

fn cardinal_average(field: &HeightField, x: u32, y: u32) -> f32 {
    let x = i32::try_from(x).unwrap_or(i32::MAX);
    let y = i32::try_from(y).unwrap_or(i32::MAX);
    [
        field.sample_wrapped(x - 1, y),
        field.sample_wrapped(x + 1, y),
        field.sample_wrapped(x, y - 1),
        field.sample_wrapped(x, y + 1),
    ]
    .iter()
    .sum::<f32>()
        / 4.0
}

fn validate(config: AlbedoConfig) -> Result<(), AlbedoError> {
    if !config
        .base_color
        .iter()
        .chain(config.warm_color.iter())
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        || config.variation < 0.0
        || config.crack_darkening < 0.0
        || config.shoulder_variation < 0.0
        || !(0.0..=1.0).contains(&config.mineral_density)
        || config.mineral_brightness < 0.0
        || !(0.0..=1.0).contains(&config.occlusion_influence)
        || ![
            config.variation,
            config.crack_darkening,
            config.shoulder_variation,
            config.mineral_density,
            config.mineral_brightness,
            config.occlusion_influence,
        ]
        .iter()
        .all(|value| value.is_finite())
    {
        return Err(AlbedoError::InvalidConfig);
    }
    Ok(())
}

/// Converts one linear channel to the sRGB transfer curve.
#[must_use]
pub fn encode_srgb(linear: f32) -> u8 {
    let linear = linear.clamp(0.0, 1.0);
    let srgb = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u8
}

#[cfg(test)]
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
    fn srgb_encoding_has_expected_endpoints() {
        assert_eq!(encode_srgb(0.0), 0);
        assert_eq!(encode_srgb(1.0), 255);
        assert!(encode_srgb(0.18) > 100);
    }

    #[test]
    fn albedo_is_deterministic_and_rgb() {
        let field = field(12, 12, vec![0.0; 144]);
        let a = generate_albedo(&field, AlbedoConfig::default(), 44).expect("albedo");
        let b = generate_albedo(&field, AlbedoConfig::default(), 44).expect("albedo");
        assert_eq!(a, b);
        assert_eq!(a.pixels().len(), 144);
        assert!(
            a.pixels()
                .iter()
                .all(|pixel| pixel.iter().any(|&value| value > 0))
        );
    }

    #[test]
    fn lower_trench_is_darker_than_flat_plateau() {
        let mut values = vec![0.0; 16 * 16];
        values[8 * 16 + 8] = -0.5;
        let field = field(16, 16, values);
        let config = AlbedoConfig {
            mineral_density: 0.0,
            ..AlbedoConfig::default()
        };
        let trench = albedo_at_linear(&field, 8, 8, config, 4, None);
        let plateau = albedo_at_linear(&field, 2, 2, config, 4, None);
        assert!(trench.iter().sum::<f32>() < plateau.iter().sum::<f32>());
    }

    #[test]
    fn invalid_palette_is_rejected() {
        let field = field(2, 2, vec![0.0; 4]);
        assert_eq!(
            generate_albedo(
                &field,
                AlbedoConfig {
                    base_color: [f32::NAN; 3],
                    ..AlbedoConfig::default()
                },
                1,
            ),
            Err(AlbedoError::InvalidConfig)
        );
    }
}
