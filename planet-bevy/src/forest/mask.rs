//! One generation-owned, seamless globe mask for forest floor and grass removal.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
use super::placement::Plant;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use std::f32::consts::{PI, TAU};
const WIDTH: usize = 1024;
const HEIGHT: usize = 512;
pub(crate) const PIXELS: usize = WIDTH * HEIGHT;
pub(super) fn build(plants: &[Plant], radius: f32) -> Vec<u8> {
    let mut pixels = vec![0; PIXELS];
    for plant in plants {
        stamp(&mut pixels, plant.root.normalize(), radius);
    }
    pixels
}
fn stamp(pixels: &mut [u8], direction: Vec3, radius: f32) {
    let lat = direction.y.asin();
    let lon = direction.z.atan2(direction.x);
    let extent = 8.0 / radius;
    let min_y = (((lat - extent) / PI + 0.5) * HEIGHT as f32)
        .floor()
        .max(0.0) as usize;
    let max_y = (((lat + extent) / PI + 0.5) * HEIGHT as f32)
        .ceil()
        .min((HEIGHT - 1) as f32) as usize;
    let x = (lon / TAU + 0.5) * WIDTH as f32;
    let span = (extent / (lat.abs() + extent).cos().max(0.001) / TAU * WIDTH as f32)
        .ceil()
        .min(WIDTH as f32 / 2.0) as i32
        + 1;
    for py in min_y..=max_y {
        let latitude = ((py as f32 + 0.5) / HEIGHT as f32 - 0.5) * PI;
        for px in x.floor() as i32 - span..=x.ceil() as i32 + span {
            let column = px.rem_euclid(i32::try_from(WIDTH).unwrap()) as usize;
            let longitude = ((column as f32 + 0.5) / WIDTH as f32 - 0.5) * TAU;
            let sample = Vec3::new(
                latitude.cos() * longitude.cos(),
                latitude.sin(),
                latitude.cos() * longitude.sin(),
            );
            let distance = sample.distance(direction) * radius;
            let cover = ((8.0 - distance) / 3.0).clamp(0.0, 1.0);
            let offset = py * WIDTH + column;
            pixels[offset] = pixels[offset].max((cover * 255.0).round() as u8);
        }
    }
}
pub(crate) fn image(pixels: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: WIDTH as u32,
            height: HEIGHT as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mask_wraps_the_seam_and_reaches_both_poles_without_a_gap() {
        for direction in [-Vec3::X, Vec3::Y, -Vec3::Y] {
            let mut pixels = vec![0; PIXELS];
            stamp(&mut pixels, direction, 1000.0);
            if direction == -Vec3::X {
                assert!(pixels[HEIGHT / 2 * WIDTH] > 0);
                assert!(pixels[HEIGHT / 2 * WIDTH + WIDTH - 1] > 0);
            } else {
                let row = if direction.y > 0.0 { HEIGHT - 1 } else { 0 };
                assert!(
                    pixels[row * WIDTH..(row + 1) * WIDTH]
                        .iter()
                        .all(|&x| x > 0)
                );
            }
        }
    }
}
