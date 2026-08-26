//! Converts engine-neutral procedural maps into Bevy images.
//!
//! Every map passes through this module without changing row order. Keeping the
//! conversions together prevents the 2D and lit previews from disagreeing
//! about UV orientation, colour space, or height polarity.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use bevy::{
    asset::RenderAssetUsages,
    image::{Image, ImageSampler},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use motu::procedural_textures::{
    FloatImage, PreviewMaps, Rgba8Image, TextureDimensions, TextureSet,
};

/// CPU display images prepared as one coherent preview replacement.
#[derive(Debug)]
pub struct ConvertedPreviewImages {
    pub albedo: Image,
    pub height: Image,
    pub normal: Image,
    pub occlusion: Image,
    pub packed_mask: Image,
    pub depth: Image,
    pub layer_raw: Option<Image>,
    pub layer_remapped: Option<Image>,
    pub layer_mask: Option<Image>,
}

/// Optional scalar diagnostics for the selected layer.
#[derive(Clone, Copy, Debug)]
pub struct LayerImageSet<'a> {
    pub raw: &'a FloatImage,
    pub remapped: &'a FloatImage,
    pub mask: &'a FloatImage,
}

/// Converts a complete map set atomically.
#[must_use]
pub fn convert_preview(
    textures: &TextureSet,
    selected_layer: Option<LayerImageSet<'_>>,
    nearest: bool,
) -> ConvertedPreviewImages {
    convert_preview_with_packed_mask(textures, None, selected_layer, nearest)
}

/// Converts a complete preview result, retaining the engine-neutral packed
/// mask produced by `island-rs` instead of reconstructing it in the renderer.
#[must_use]
pub fn convert_preview_maps(
    preview: &PreviewMaps,
    selected_layer: Option<LayerImageSet<'_>>,
    nearest: bool,
) -> ConvertedPreviewImages {
    convert_preview_with_packed_mask(
        &preview.textures,
        preview.packed_mask.as_ref(),
        selected_layer,
        nearest,
    )
}

fn convert_preview_with_packed_mask(
    textures: &TextureSet,
    packed_mask: Option<&Rgba8Image>,
    selected_layer: Option<LayerImageSet<'_>>,
    nearest: bool,
) -> ConvertedPreviewImages {
    let dimensions = textures.dimensions();
    let sampler = if nearest {
        ImageSampler::nearest()
    } else {
        ImageSampler::linear()
    };
    let albedo = rgba_image(
        dimensions,
        textures
            .albedo
            .pixels()
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], u8::MAX])
            .collect(),
        TextureFormat::Rgba8UnormSrgb,
        sampler.clone(),
    );
    let normal = rgba_image(
        dimensions,
        textures
            .normal
            .pixels()
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], u8::MAX])
            .collect(),
        TextureFormat::Rgba8Unorm,
        sampler.clone(),
    );
    let height_bytes = textures
        .height
        .pixels()
        .iter()
        .map(|value| value.to_be_bytes()[0])
        .collect::<Vec<_>>();
    let height = grayscale_rgba(dimensions, &height_bytes, sampler.clone());
    let occlusion = grayscale_rgba(dimensions, textures.occlusion.pixels(), sampler.clone());
    let packed_pixels = packed_mask
        .filter(|mask| mask.dimensions() == dimensions)
        .map_or_else(
            || {
                height_bytes
                    .iter()
                    .zip(textures.occlusion.pixels())
                    .flat_map(|(&height, &occlusion)| [height, occlusion, 0, u8::MAX])
                    .collect()
            },
            |mask| {
                mask.pixels()
                    .iter()
                    .flat_map(|pixel| pixel.iter().copied())
                    .collect()
            },
        );
    let packed_mask = rgba_image(
        dimensions,
        packed_pixels,
        TextureFormat::Rgba8Unorm,
        sampler.clone(),
    );
    // Bevy's parallax contract is white=bottom and black=top.
    let depth = scalar_image(
        dimensions,
        height_bytes.iter().map(|height| u8::MAX - height).collect(),
        TextureFormat::R8Unorm,
        sampler.clone(),
    );
    let (layer_raw, layer_remapped, layer_mask) =
        selected_layer.map_or((None, None, None), |layer| {
            (
                Some(float_display(
                    layer.raw,
                    ScalarDisplayRange::Signed,
                    sampler.clone(),
                )),
                Some(float_display(
                    layer.remapped,
                    ScalarDisplayRange::Unit,
                    sampler.clone(),
                )),
                Some(float_display(layer.mask, ScalarDisplayRange::Unit, sampler)),
            )
        });
    ConvertedPreviewImages {
        albedo,
        height,
        normal,
        occlusion,
        packed_mask,
        depth,
        layer_raw,
        layer_remapped,
        layer_mask,
    }
}

#[derive(Clone, Copy)]
enum ScalarDisplayRange {
    Signed,
    Unit,
}

fn float_display(image: &FloatImage, range: ScalarDisplayRange, sampler: ImageSampler) -> Image {
    let bytes = image
        .pixels()
        .iter()
        .map(|&value| match range {
            ScalarDisplayRange::Signed => (value.mul_add(0.5, 0.5).clamp(0.0, 1.0) * 255.0) as u8,
            ScalarDisplayRange::Unit => (value.clamp(0.0, 1.0) * 255.0) as u8,
        })
        .collect::<Vec<_>>();
    grayscale_rgba(image.dimensions(), &bytes, sampler)
}

fn grayscale_rgba(dimensions: TextureDimensions, pixels: &[u8], sampler: ImageSampler) -> Image {
    rgba_image(
        dimensions,
        pixels
            .iter()
            .flat_map(|&value| [value, value, value, u8::MAX])
            .collect(),
        TextureFormat::Rgba8Unorm,
        sampler,
    )
}

fn rgba_image(
    dimensions: TextureDimensions,
    pixels: Vec<u8>,
    format: TextureFormat,
    sampler: ImageSampler,
) -> Image {
    scalar_image(dimensions, pixels, format, sampler)
}

fn scalar_image(
    dimensions: TextureDimensions,
    pixels: Vec<u8>,
    format: TextureFormat,
    sampler: ImageSampler,
) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: dimensions.width,
            height: dimensions.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        format,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = sampler;
    image
}

#[cfg(test)]
mod tests {
    use motu::procedural_textures::{
        FloatImage, Gray8Image, Gray16Image, NormalConvention, PreviewMaps, PreviewTimings,
        Rgb8Image, Rgba8Image, TextureDimensions, TextureMetadata, TextureSet,
    };

    use super::*;

    fn image_bytes(image: &Image) -> &[u8] {
        image.data.as_deref().expect("CPU image data")
    }

    #[test]
    fn every_map_preserves_the_same_asymmetric_pixel_address() {
        let dimensions = TextureDimensions::new(2, 2).unwrap();
        let textures = TextureSet::new(
            Rgb8Image::new(
                dimensions,
                vec![[1, 2, 3], [10, 20, 30], [4, 5, 6], [7, 8, 9]],
            )
            .unwrap(),
            Gray16Image::new(dimensions, vec![0, 0x7f00, 0xffff, 0x3300]).unwrap(),
            Rgb8Image::new(
                dimensions,
                vec![[11, 12, 13], [110, 120, 130], [14, 15, 16], [17, 18, 19]],
            )
            .unwrap(),
            Gray8Image::new(dimensions, vec![21, 210, 22, 23]).unwrap(),
            TextureMetadata {
                normal_convention: NormalConvention::OpenGl,
                ..TextureMetadata::default()
            },
        )
        .unwrap();
        let raw = FloatImage::new(dimensions, vec![-1.0, 1.0, -0.5, 0.5]).unwrap();
        let remapped = FloatImage::new(dimensions, vec![0.0, 0.75, 0.25, 0.5]).unwrap();
        let mask = FloatImage::new(dimensions, vec![0.0, 0.8, 0.2, 0.4]).unwrap();

        let converted = convert_preview(
            &textures,
            Some(LayerImageSet {
                raw: &raw,
                remapped: &remapped,
                mask: &mask,
            }),
            true,
        );
        let pixel = 1_usize;
        let rgba = pixel * 4;
        assert_eq!(
            &image_bytes(&converted.albedo)[rgba..rgba + 4],
            &[10, 20, 30, 255]
        );
        assert_eq!(
            &image_bytes(&converted.normal)[rgba..rgba + 4],
            &[110, 120, 130, 255]
        );
        assert_eq!(
            &image_bytes(&converted.height)[rgba..rgba + 4],
            &[127, 127, 127, 255]
        );
        assert_eq!(
            &image_bytes(&converted.occlusion)[rgba..rgba + 4],
            &[210, 210, 210, 255]
        );
        assert_eq!(
            &image_bytes(&converted.packed_mask)[rgba..rgba + 4],
            &[127, 210, 0, 255]
        );
        assert_eq!(image_bytes(&converted.depth)[pixel], 128);
        assert_eq!(
            image_bytes(converted.layer_raw.as_ref().unwrap())[rgba],
            255
        );
        assert_eq!(
            image_bytes(converted.layer_remapped.as_ref().unwrap())[rgba],
            191
        );
        assert_eq!(
            image_bytes(converted.layer_mask.as_ref().unwrap())[rgba],
            204
        );

        let preview = PreviewMaps {
            textures,
            packed_mask: Some(
                Rgba8Image::new(
                    dimensions,
                    vec![
                        [200, 201, 202, 203],
                        [204, 205, 206, 207],
                        [208, 209, 210, 211],
                        [212, 213, 214, 215],
                    ],
                )
                .unwrap(),
            ),
            selected_layer: None,
            recipe_hash: String::new(),
            timings_ms: PreviewTimings::default(),
        };
        let converted = convert_preview_maps(&preview, None, true);
        assert_eq!(
            &image_bytes(&converted.packed_mask)[..4],
            &[200, 201, 202, 203]
        );
    }
}
