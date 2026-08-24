//! Wrapped tangent-space normal derivation from the authoritative height pass.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc
)]

use glam::Vec3;

use super::field_program::HeightField;
use super::image::{ImageError, Rgb8Image, TextureDimensions};

/// Re-export the shared image-layer normal convention from the material API.
pub use super::image::NormalConvention;

/// An owned RGB8 normal image. The image module carries the common dimensions
/// and buffer validation shared by all generated maps.
pub type NormalImage = Rgb8Image;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalError {
    Image(ImageError),
    InvalidScale,
}

impl From<ImageError> for NormalError {
    fn from(error: ImageError) -> Self {
        Self::Image(error)
    }
}

/// Computes one wrapped, unit-length tangent-space normal in linear form.
///
/// `vertical_scale` is a dimensionless multiplier applied to the source
/// height.  A value of one uses the physical metre heights directly.  The
/// central difference uses the recipe's physical tile spacing, so changing
/// resolution alone does not change the perceived relief.
#[must_use]
pub fn normal_at(
    field: &HeightField,
    x: u32,
    y: u32,
    vertical_scale: f32,
    convention: NormalConvention,
) -> Vec3 {
    let dimensions = field.dimensions();
    let (pixel_width, pixel_height) = dimensions.pixel_size();
    let x = x % dimensions.width;
    let y = y % dimensions.height;
    let x = i32::try_from(x).unwrap_or(i32::MAX);
    let y = i32::try_from(y).unwrap_or(i32::MAX);
    let dx =
        (field.sample_wrapped(x + 1, y) - field.sample_wrapped(x - 1, y)) / (2.0 * pixel_width);
    let dy =
        (field.sample_wrapped(x, y + 1) - field.sample_wrapped(x, y - 1)) / (2.0 * pixel_height);
    let mut normal = Vec3::new(-dx * vertical_scale, -dy * vertical_scale, 1.0);
    if convention == NormalConvention::DirectX {
        normal.y = -normal.y;
    }
    normal.normalize_or_zero()
}

/// Derives wrapped normals and encodes them to RGB8 tangent-space data.
pub fn derive_normals(
    field: &HeightField,
    vertical_scale: f32,
    convention: NormalConvention,
) -> Result<NormalImage, NormalError> {
    if !vertical_scale.is_finite() || vertical_scale < 0.0 {
        return Err(NormalError::InvalidScale);
    }
    let dimensions = field.dimensions();
    let mut pixels = Vec::with_capacity(dimensions.pixel_count());
    for y in 0..dimensions.height {
        for x in 0..dimensions.width {
            pixels.push(encode_normal(normal_at(
                field,
                x,
                y,
                vertical_scale,
                convention,
            )));
        }
    }
    Rgb8Image::new(
        TextureDimensions::new(dimensions.width, dimensions.height)?,
        pixels,
    )
    .map_err(NormalError::from)
}

/// Returns an RGB8 pixel from a unit tangent-space normal.
#[must_use]
pub fn encode_normal(normal: Vec3) -> [u8; 3] {
    let normal = normal.normalize_or_zero();
    [
        quantize_unit(normal.x),
        quantize_unit(normal.y),
        quantize_unit(normal.z),
    ]
}

#[must_use]
fn quantize_unit(value: f32) -> u8 {
    (((value.clamp(-1.0, 1.0) * 0.5) + 0.5) * 255.0).round() as u8
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
    fn flat_field_encodes_upward_normal() {
        let field = field(4, 4, vec![0.0; 16]);
        let normal = normal_at(&field, 2, 2, 1.0, NormalConvention::OpenGl);
        assert!((normal - Vec3::Z).length() < 1.0e-6);
        let image = derive_normals(&field, 1.0, NormalConvention::OpenGl).expect("normals");
        assert!(image.pixels().iter().all(|pixel| *pixel == [128, 128, 255]));
    }

    #[test]
    fn wrapped_central_difference_handles_periodic_ramp() {
        let mut values = Vec::new();
        for _y in 0..4 {
            values.extend([0.0, 1.0, 2.0, 3.0]);
        }
        let field = field(4, 4, values);
        // At x=1 the wrapped central slope is one metre across two metres of
        // horizontal distance.  The normal leans down in tangent X.
        let normal = normal_at(&field, 1, 1, 1.0, NormalConvention::OpenGl);
        assert!(normal.x < -0.4);
        assert!(normal.z > 0.7);
    }

    #[test]
    fn direct_x_only_inverts_green_channel() {
        let mut values = vec![0.0; 16];
        for y in 0..4 {
            for x in 0..4 {
                values[y * 4 + x] = y as f32;
            }
        }
        let field = field(4, 4, values);
        let open_gl = derive_normals(&field, 1.0, NormalConvention::OpenGl)
            .expect("normals")
            .pixels()[0];
        let direct_x = derive_normals(&field, 1.0, NormalConvention::DirectX)
            .expect("normals")
            .pixels()[0];
        assert_eq!(open_gl[0], direct_x[0]);
        assert_eq!(open_gl[2], direct_x[2]);
        assert_ne!(open_gl[1], direct_x[1]);
    }
}
