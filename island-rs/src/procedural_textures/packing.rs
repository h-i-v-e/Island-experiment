//! Height quantization and Unity terrain mask packing.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc
)]

use super::{
    field_program::HeightField,
    image::{Gray16Image, ImageError, Rgba8Image, TextureDimensions},
    occlusion::OcclusionImage,
};

/// Physical range represented by a stored 16-bit height image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightRange {
    pub minimum: f32,
    pub maximum: f32,
    pub neutral: f32,
}

impl HeightRange {
    pub fn new(minimum: f32, maximum: f32, neutral: f32) -> Result<Self, PackingError> {
        if !minimum.is_finite()
            || !maximum.is_finite()
            || !neutral.is_finite()
            || maximum <= minimum
            || neutral < minimum
            || neutral > maximum
        {
            return Err(PackingError::InvalidHeightRange);
        }
        Ok(Self {
            minimum,
            maximum,
            neutral,
        })
    }

    #[must_use]
    pub fn normalized(self, value: f32) -> f32 {
        ((value.clamp(self.minimum, self.maximum) - self.minimum) / (self.maximum - self.minimum))
            .clamp(0.0, 1.0)
    }
}

/// Owned linear Gray16 height data.
pub type HeightImage = Gray16Image;

/// Integer-safe representation of the f32 height range metadata.
///
/// The generator retains the exact f32 values in [`HeightRange`] at the API
/// boundary; this compact copy makes the image itself `Eq` and convenient for
/// engine adapters that serialize metadata separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeightRangeBits {
    pub minimum_bits: u32,
    pub maximum_bits: u32,
    pub neutral_bits: u32,
}

impl HeightRangeBits {
    #[must_use]
    pub const fn from_range(range: HeightRange) -> Self {
        Self {
            minimum_bits: range.minimum.to_bits(),
            maximum_bits: range.maximum.to_bits(),
            neutral_bits: range.neutral.to_bits(),
        }
    }

    #[must_use]
    pub fn as_range(self) -> HeightRange {
        HeightRange {
            minimum: f32::from_bits(self.minimum_bits),
            maximum: f32::from_bits(self.maximum_bits),
            neutral: f32::from_bits(self.neutral_bits),
        }
    }
}

/// Owned Unity terrain mask: R height, G occlusion, B spare/zero, A one.
pub type UnityMaskImage = Rgba8Image;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackingError {
    Image(ImageError),
    InvalidHeightRange,
    DimensionsMismatch,
}

impl From<ImageError> for PackingError {
    fn from(error: ImageError) -> Self {
        Self::Image(error)
    }
}

/// Quantizes the unquantized f32 height field to a Gray16 image.
pub fn quantize_height(
    field: &HeightField,
    range: HeightRange,
) -> Result<HeightImage, PackingError> {
    HeightRange::new(range.minimum, range.maximum, range.neutral)?;
    let pixels = field
        .values()
        .iter()
        .map(|&value| quantize_height_value(value, range))
        .collect();
    Gray16Image::new(
        TextureDimensions::new(field.dimensions().width, field.dimensions().height)?,
        pixels,
    )
    .map_err(PackingError::from)
}

/// Packs 8-bit height and occlusion into the Unity terrain contract.
pub fn pack_unity_mask(
    field: &HeightField,
    range: HeightRange,
    occlusion: &OcclusionImage,
) -> Result<UnityMaskImage, PackingError> {
    let dimensions = field.dimensions();
    if occlusion.width() != dimensions.width || occlusion.height() != dimensions.height {
        return Err(PackingError::DimensionsMismatch);
    }
    HeightRange::new(range.minimum, range.maximum, range.neutral)?;
    let pixels = field
        .values()
        .iter()
        .zip(occlusion.pixels())
        .map(|(&height, &ao)| [quantize_height_byte(height, range), ao, 0, 255])
        .collect();
    Rgba8Image::new(
        TextureDimensions::new(dimensions.width, dimensions.height)?,
        pixels,
    )
    .map_err(PackingError::from)
}

/// Quantizes one physical height to the full unsigned 16-bit range.
#[must_use]
pub fn quantize_height_value(value: f32, range: HeightRange) -> u16 {
    (range.normalized(value) * 65_535.0).round() as u16
}

/// Quantizes one physical height to the Unity mask's red channel.
#[must_use]
pub fn quantize_height_byte(value: f32, range: HeightRange) -> u8 {
    (range.normalized(value) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedural_textures::field_program::{FieldDimensions, HeightField};
    use crate::procedural_textures::image::Gray8Image;

    fn field() -> HeightField {
        HeightField::new(
            FieldDimensions::new(2, 2, 4.0, 4.0).expect("dimensions"),
            vec![-1.0, 0.0, 0.5, 1.0],
        )
        .expect("field")
    }

    #[test]
    fn quantization_clamps_and_preserves_endpoints() {
        let range = HeightRange::new(-1.0, 1.0, 0.0).expect("range");
        assert_eq!(quantize_height_value(-100.0, range), 0);
        assert_eq!(quantize_height_value(-1.0, range), 0);
        assert_eq!(quantize_height_value(0.0, range), 32_768);
        assert_eq!(quantize_height_value(1.0, range), 65_535);
        assert_eq!(quantize_height_byte(0.0, range), 128);
    }

    #[test]
    fn unity_mask_obeys_channel_contract() {
        let field = field();
        let dimensions = field.dimensions();
        let occlusion = Gray8Image::new(
            TextureDimensions::new(dimensions.width, dimensions.height).expect("dimensions"),
            vec![0, 64, 192, 255],
        )
        .expect("occlusion");
        let mask = pack_unity_mask(
            &field,
            HeightRange::new(-1.0, 1.0, 0.0).expect("range"),
            &occlusion,
        )
        .expect("mask");
        assert_eq!(
            mask.pixels(),
            &[
                [0, 0, 0, 255],
                [128, 64, 0, 255],
                [191, 192, 0, 255],
                [255, 255, 0, 255]
            ]
        );
    }

    #[test]
    fn mask_rejects_mismatched_occlusion() {
        let field = field();
        let occlusion =
            Gray8Image::new(TextureDimensions::new(1, 1).expect("dimensions"), vec![255])
                .expect("occlusion");
        assert_eq!(
            pack_unity_mask(
                &field,
                HeightRange::new(-1.0, 1.0, 0.0).expect("range"),
                &occlusion,
            ),
            Err(PackingError::DimensionsMismatch)
        );
    }

    #[test]
    fn invalid_range_is_rejected() {
        assert_eq!(
            HeightRange::new(1.0, 1.0, 1.0),
            Err(PackingError::InvalidHeightRange)
        );
    }
}
