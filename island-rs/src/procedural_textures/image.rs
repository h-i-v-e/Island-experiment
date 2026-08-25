//! Engine-neutral owned image buffers and texture-set contracts.

#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::uninlined_format_args
)]

use core::{fmt, ops::Range};
use serde::{Deserialize, Serialize};

/// A non-empty image extent with a checked pixel count.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureDimensions {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
}

impl TextureDimensions {
    /// Creates dimensions and verifies that their pixel count fits in a
    /// `usize` on the current target.
    pub fn new(width: u32, height: u32) -> Result<Self, ImageError> {
        if width == 0 {
            return Err(ImageError::ZeroWidth);
        }
        if height == 0 {
            return Err(ImageError::ZeroHeight);
        }
        if pixel_count(width, height).is_none() {
            return Err(ImageError::PixelCountOverflow { width, height });
        }
        Ok(Self { width, height })
    }

    /// Creates dimensions from a known-valid extent.
    pub const fn new_unchecked(width: u32, height: u32) -> Self {
        debug_assert!(width != 0 && height != 0);
        Self { width, height }
    }

    /// Returns the checked number of pixels.
    #[inline]
    pub fn pixel_count(self) -> usize {
        // `new` and all image constructors establish this invariant.
        match pixel_count(self.width, self.height) {
            Some(count) => count,
            None => unreachable!("validated dimensions must have a usize pixel count"),
        }
    }

    /// Returns the dimensions as `(width, height)`.
    #[inline]
    pub const fn as_tuple(self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// The tangent-space convention used when encoding a normal image.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalConvention {
    /// Positive tangent Y is encoded directly in the green channel.
    #[default]
    OpenGl,
    /// Tangent Y is inverted for DirectX-style consumers.
    DirectX,
}

/// Errors returned by image and texture-set constructors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ImageError {
    /// Width is zero.
    ZeroWidth,
    /// Height is zero.
    ZeroHeight,
    /// `width * height` does not fit in `usize`.
    PixelCountOverflow { width: u32, height: u32 },
    /// The owned pixel buffer does not match the image extent.
    PixelBufferLength { expected: usize, actual: usize },
    /// Two images that must be combined do not have equal extents.
    DimensionMismatch {
        expected: TextureDimensions,
        actual: TextureDimensions,
    },
    /// A pixel coordinate lies outside the image.
    CoordinateOutOfBounds {
        x: u32,
        y: u32,
        dimensions: TextureDimensions,
    },
}

impl fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWidth => formatter.write_str("image width must be greater than zero"),
            Self::ZeroHeight => formatter.write_str("image height must be greater than zero"),
            Self::PixelCountOverflow { width, height } => write!(
                formatter,
                "image dimensions {width}x{height} overflow the platform pixel count"
            ),
            Self::PixelBufferLength { expected, actual } => write!(
                formatter,
                "image pixel buffer has {actual} elements, expected {expected}"
            ),
            Self::DimensionMismatch { expected, actual } => write!(
                formatter,
                "image dimensions {:?} do not match expected {:?}",
                actual, expected
            ),
            Self::CoordinateOutOfBounds { x, y, dimensions } => write!(
                formatter,
                "pixel coordinate ({x}, {y}) is outside {}x{} image",
                dimensions.width, dimensions.height
            ),
        }
    }
}

impl std::error::Error for ImageError {}

/// Core texture-boundary error alias used by generation APIs that currently
/// only need image/extent validation. Later bake stages can wrap this error
/// with their encoding or recipe-specific variants without changing image
/// constructors.
pub type TextureError = ImageError;

/// An owned row-major image with a typed pixel representation.
#[derive(Clone, Debug, PartialEq)]
pub struct Image<T> {
    dimensions: TextureDimensions,
    pixels: Vec<T>,
}

impl<T> Image<T> {
    /// Creates an image after checking the pixel-buffer length.
    pub fn new(dimensions: TextureDimensions, pixels: Vec<T>) -> Result<Self, ImageError> {
        let expected = dimensions.pixel_count();
        if pixels.len() != expected {
            return Err(ImageError::PixelBufferLength {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self { dimensions, pixels })
    }

    /// Creates an image from dimensions and a row-major pixel buffer.
    pub fn from_dimensions(width: u32, height: u32, pixels: Vec<T>) -> Result<Self, ImageError> {
        Self::new(TextureDimensions::new(width, height)?, pixels)
    }

    /// Creates a filled image without exposing its internal buffer.
    pub fn filled(dimensions: TextureDimensions, pixel: T) -> Self
    where
        T: Clone,
    {
        Self {
            dimensions,
            pixels: vec![pixel; dimensions.pixel_count()],
        }
    }

    /// Returns the image extent.
    #[inline]
    pub const fn dimensions(&self) -> TextureDimensions {
        self.dimensions
    }

    /// Returns the width in pixels.
    #[inline]
    pub const fn width(&self) -> u32 {
        self.dimensions.width
    }

    /// Returns the height in pixels.
    #[inline]
    pub const fn height(&self) -> u32 {
        self.dimensions.height
    }

    /// Returns the number of pixels.
    #[inline]
    pub fn len(&self) -> usize {
        self.pixels.len()
    }

    /// Returns whether the image has no pixels.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }

    /// Borrows the row-major pixel buffer.
    #[inline]
    pub fn pixels(&self) -> &[T] {
        &self.pixels
    }

    /// Mutably borrows the row-major pixel buffer.
    #[inline]
    pub fn pixels_mut(&mut self) -> &mut [T] {
        &mut self.pixels
    }

    /// Consumes the image and returns its owned pixels.
    #[inline]
    pub fn into_pixels(self) -> Vec<T> {
        self.pixels
    }

    /// Returns one pixel if the coordinate is in bounds.
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> Option<&T> {
        self.index_of(x, y).and_then(|index| self.pixels.get(index))
    }

    /// Mutably returns one pixel if the coordinate is in bounds.
    #[inline]
    pub fn get_mut(&mut self, x: u32, y: u32) -> Option<&mut T> {
        let index = self.index_of(x, y)?;
        self.pixels.get_mut(index)
    }

    /// Returns a borrowed row if `y` is in bounds.
    #[inline]
    pub fn row(&self, y: u32) -> Option<&[T]> {
        if y >= self.height() {
            return None;
        }
        let start = y as usize * self.width() as usize;
        Some(&self.pixels[start..start + self.width() as usize])
    }

    /// Mutably returns a row if `y` is in bounds.
    #[inline]
    pub fn row_mut(&mut self, y: u32) -> Option<&mut [T]> {
        if y >= self.height() {
            return None;
        }
        let width = self.width() as usize;
        let start = y as usize * width;
        Some(&mut self.pixels[start..start + width])
    }

    /// Returns the half-open index range for one row.
    #[inline]
    pub fn row_range(&self, y: u32) -> Option<Range<usize>> {
        if y >= self.height() {
            return None;
        }
        let start = y as usize * self.width() as usize;
        Some(start..start + self.width() as usize)
    }

    /// Maps pixels into another typed image while retaining dimensions.
    pub fn map<U, F>(self, mut map_pixel: F) -> Image<U>
    where
        F: FnMut(T) -> U,
    {
        let dimensions = self.dimensions;
        let pixels = self.pixels.into_iter().map(&mut map_pixel).collect();
        Image { dimensions, pixels }
    }

    fn index_of(&self, x: u32, y: u32) -> Option<usize> {
        (x < self.width() && y < self.height())
            .then_some(y as usize * self.width() as usize + x as usize)
    }
}

/// RGB 8-bit image, suitable for albedo or encoded tangent normals.
pub type Rgb8Image = Image<[u8; 3]>;
/// RGBA 8-bit image, suitable for packed engine masks.
pub type Rgba8Image = Image<[u8; 4]>;
/// Linear unsigned 8-bit image, suitable for occlusion.
pub type Gray8Image = Image<u8>;
/// Linear unsigned 16-bit image, suitable for source height.
pub type Gray16Image = Image<u16>;
/// Linear floating-point scalar image, suitable for an unquantized field.
pub type FloatImage = Image<f32>;

/// Metadata shared by all maps in a generated set.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureMetadata {
    /// Safe output stem carried from the validated recipe.
    pub name: String,
    /// SHA-256 of the normalized effective recipe.
    pub recipe_hash: String,
    /// Generator algorithm version used for this output.
    pub algorithm_version: u32,
    /// Seed used by all source fields.
    pub seed: u64,
    /// Physical tile size in metres.
    pub physical_tile_size_m: [f32; 2],
    /// Minimum represented height in metres.
    pub minimum_height_m: f32,
    /// Maximum represented height in metres.
    pub maximum_height_m: f32,
    /// Neutral/base height in metres.
    pub base_height_m: f32,
    /// Whether a consumer should treat height as displacement rather than a
    /// blend-height signal.
    pub displacement: bool,
    /// Convention used by the encoded normal map.
    pub normal_convention: NormalConvention,
}

impl Default for TextureMetadata {
    fn default() -> Self {
        Self {
            name: "ProceduralTexture".into(),
            recipe_hash: String::new(),
            algorithm_version: 1,
            seed: 0,
            physical_tile_size_m: [1.0, 1.0],
            minimum_height_m: 0.0,
            maximum_height_m: 1.0,
            base_height_m: 0.0,
            displacement: true,
            normal_convention: NormalConvention::OpenGl,
        }
    }
}

/// A complete set of maps sharing one extent and height-field metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureSet {
    /// Shared map dimensions.
    pub dimensions: TextureDimensions,
    /// RGB albedo in encoded sRGB bytes.
    pub albedo: Rgb8Image,
    /// Linear source height in unsigned 16-bit metres-range encoding.
    pub height: Gray16Image,
    /// RGB tangent-space normal bytes.
    pub normal: Rgb8Image,
    /// Linear material-local occlusion.
    pub occlusion: Gray8Image,
    /// Shared physical and algorithm metadata.
    pub metadata: TextureMetadata,
}

impl TextureSet {
    /// Constructs a set after verifying every image extent.
    pub fn new(
        albedo: Rgb8Image,
        height: Gray16Image,
        normal: Rgb8Image,
        occlusion: Gray8Image,
        metadata: TextureMetadata,
    ) -> Result<Self, ImageError> {
        let dimensions = albedo.dimensions();
        for image_dimensions in [
            height.dimensions(),
            normal.dimensions(),
            occlusion.dimensions(),
        ] {
            if image_dimensions != dimensions {
                return Err(ImageError::DimensionMismatch {
                    expected: dimensions,
                    actual: image_dimensions,
                });
            }
        }
        Ok(Self {
            dimensions,
            albedo,
            height,
            normal,
            occlusion,
            metadata,
        })
    }

    /// Returns all image dimensions as one value.
    #[inline]
    pub const fn dimensions(&self) -> TextureDimensions {
        self.dimensions
    }
}

fn pixel_count(width: u32, height: u32) -> Option<usize> {
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    width.checked_mul(height)
}

#[cfg(test)]
mod tests {
    use super::{
        Gray8Image, Gray16Image, Image, ImageError, NormalConvention, Rgb8Image, TextureDimensions,
        TextureMetadata, TextureSet,
    };

    #[test]
    fn constructors_validate_dimensions_and_buffers() {
        assert!(matches!(
            TextureDimensions::new(0, 4),
            Err(ImageError::ZeroWidth)
        ));
        let dimensions = TextureDimensions::new(3, 2).expect("valid dimensions");
        assert!(matches!(
            Image::<u8>::new(dimensions, vec![0; 5]),
            Err(ImageError::PixelBufferLength {
                expected: 6,
                actual: 5
            })
        ));
    }

    #[test]
    fn rows_and_coordinates_are_borrowed_without_cloning() {
        let dimensions = TextureDimensions::new(3, 2).expect("valid dimensions");
        let mut image = Image::new(dimensions, vec![0, 1, 2, 3, 4, 5]).expect("matching pixels");
        assert_eq!(image.row(1), Some(&[3, 4, 5][..]));
        *image.get_mut(2, 0).expect("in bounds") = 22;
        assert_eq!(image.get(2, 0), Some(&22));
        assert_eq!(image.get(3, 0), None);
    }

    #[test]
    fn texture_set_rejects_mismatched_map_extents() {
        let two = TextureDimensions::new(2, 2).expect("valid dimensions");
        let three = TextureDimensions::new(3, 2).expect("valid dimensions");
        let result = TextureSet::new(
            Rgb8Image::filled(two, [0, 0, 0]),
            Gray16Image::filled(three, 0),
            Rgb8Image::filled(two, [128, 128, 255]),
            Gray8Image::filled(two, 255),
            TextureMetadata {
                normal_convention: NormalConvention::OpenGl,
                ..TextureMetadata::default()
            },
        );
        assert!(matches!(result, Err(ImageError::DimensionMismatch { .. })));
    }
}
