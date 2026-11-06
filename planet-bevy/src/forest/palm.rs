//! One shared frond cutout, with coverage-preserving alpha mips.
use bevy::{
    asset::RenderAssetUsages,
    image::{
        CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler,
        ImageSamplerDescriptor, ImageType,
    },
    prelude::*,
};

const FROND: &[u8] =
    include_bytes!("../../../packages/com.motu.runtime/Runtime/Resources/Motu/PalmFrond.png");

pub(super) fn image() -> Image {
    let mut image = Image::from_buffer(
        FROND,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        false,
        ImageSampler::Default,
        RenderAssetUsages::RENDER_WORLD,
    )
    .expect("shared palm frond RGBA PNG");
    let (mut width, mut height) = (image.width() as usize, image.height() as usize);
    let mut pixels = image.data.take().expect("decoded frond pixels");
    assert_eq!(pixels.len(), width * height * 4);
    let coverage = alpha_coverage(&pixels);
    let mut data = Vec::with_capacity(pixels.len() * 4 / 3 + 64);
    let mut levels = 0;
    loop {
        data.extend_from_slice(&pixels);
        levels += 1;
        if width == 1 && height == 1 {
            break;
        }
        pixels = downsample(&pixels, width, height);
        preserve_coverage(&mut pixels, coverage);
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

fn alpha_coverage(pixels: &[u8]) -> f32 {
    pixels.chunks_exact(4).filter(|p| p[3] >= 128).count() as f32 / (pixels.len() / 4) as f32
}

fn downsample(pixels: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut next = Vec::with_capacity((width / 2).max(1) * (height / 2).max(1) * 4);
    for y in 0..(height / 2).max(1) {
        for x in 0..(width / 2).max(1) {
            for channel in 0..4 {
                let sum: u16 = (0..2)
                    .flat_map(|dy| (0..2).map(move |dx| (dx, dy)))
                    .map(|(dx, dy)| {
                        u16::from(
                            pixels[(((y * 2 + dy).min(height - 1) * width
                                + (x * 2 + dx).min(width - 1))
                                * 4)
                                + channel],
                        )
                    })
                    .sum();
                next.push((sum / 4) as u8);
            }
        }
    }
    next
}

#[allow(clippy::cast_sign_loss)] // Alpha is rounded and clamped to the u8 range.
fn preserve_coverage(pixels: &mut [u8], target: f32) {
    let (mut low, mut high) = (0.0, 8.0);
    for _ in 0..14 {
        let scale = (low + high) * 0.5;
        let coverage = pixels
            .chunks_exact(4)
            .filter(|p| f32::from(p[3]) * scale >= 127.5)
            .count() as f32
            / (pixels.len() / 4) as f32;
        if coverage < target {
            low = scale;
        } else {
            high = scale;
        }
    }
    let error = |scale: f32| {
        let coverage = pixels
            .chunks_exact(4)
            .filter(|p| f32::from(p[3]) * scale >= 127.5)
            .count() as f32
            / (pixels.len() / 4) as f32;
        (coverage - target).abs()
    };
    let scale = if error(low) < error(high) { low } else { high };
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = (f32::from(pixel[3]) * scale).round().clamp(0.0, 255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_frond_has_rgba_alpha_and_covered_mips() {
        let image = image();
        assert_eq!(
            image.texture_descriptor.format,
            bevy::render::render_resource::TextureFormat::Rgba8Unorm
        );
        assert!(image.texture_descriptor.mip_level_count > 1);
        let data = image.data.as_ref().unwrap();
        let mut offset = 0;
        let (mut width, mut height) = (image.width() as usize, image.height() as usize);
        // PNG rows start at the tip margin. The WGSL chart flips native V so
        // the narrow root stem remains attached to the tree at native V=0.
        assert_eq!(data[(width / 2) * 4 + 3], 0);
        assert!(data[((height - 1) * width + width / 2) * 4 + 3] >= 128);
        let target = alpha_coverage(&data[..width * height * 4]);
        assert!(
            target > 0.05 && target < 0.6,
            "fronds retain open gaps: {target}"
        );
        loop {
            let size = width * height * 4;
            if width * height >= 1024 {
                assert!((alpha_coverage(&data[offset..offset + size]) - target).abs() < 0.04);
            }
            offset += size;
            if width == 1 && height == 1 {
                break;
            }
            width = (width / 2).max(1);
            height = (height / 2).max(1);
        }
        assert_eq!(offset, data.len());
    }
}
