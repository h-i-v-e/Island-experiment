//! Unity Island's ground recipe arrays, shared by every streamed terrain chunk.
//! CPU assets are decoded once; entities only retain inexpensive image handles.
use bevy::{
    asset::RenderAssetUsages,
    image::{
        CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler,
        ImageSamplerDescriptor, ImageType,
    },
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        storage::ShaderBuffer,
    },
};

#[derive(Resource, Default)]
pub struct TerrainTextures {
    pub albedo: Handle<Image>,
    pub normal: Handle<Image>,
    pub mask: Handle<Image>,
    pub noise: Handle<Image>,
    pub detail_mask: Handle<Image>,
    pub detail_mapping: crate::lod_material::DetailMapping,
    pub detail_frame: Handle<ShaderBuffer>,
    pub environment: Handle<ShaderBuffer>,
    pub environment_sky: Handle<Image>,
    pub environment_shadows: Handle<Image>,
    pub global_weather: Handle<Image>,
    pub forest_mask: Handle<Image>,
}

impl TerrainTextures {
    pub fn set_detail_mapping(
        &mut self,
        mapping: crate::lod_material::DetailMapping,
        buffers: &mut Assets<ShaderBuffer>,
    ) {
        if self.detail_mapping != mapping {
            if let Some(mut buffer) = buffers.get_mut(&self.detail_frame) {
                buffer.set_data(mapping);
            }
            self.detail_mapping = mapping;
        }
    }

    pub fn extension(&self, radius: f32) -> crate::lod_material::SurfaceExtension {
        crate::lod_material::SurfaceExtension {
            terrain: crate::lod_material::TerrainSettings {
                radius,
                ..default()
            },
            albedo: Some(self.albedo.clone()),
            normal: Some(self.normal.clone()),
            mask: Some(self.mask.clone()),
            noise: Some(self.noise.clone()),
            detail_mask: Some(self.detail_mask.clone()),
            detail_mapping: self.detail_frame.clone(),
            environment: self.environment.clone(),
            environment_sky: Some(self.environment_sky.clone()),
            environment_shadows: Some(self.environment_shadows.clone()),
            global_weather: Some(self.global_weather.clone()),
            forest_mask: Some(self.forest_mask.clone()),
            ..default()
        }
    }
}

pub fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    environment: Res<crate::environment::settings::EnvironmentAssets>,
    reflection: Res<crate::environment::reflections::SkyReflectionAssets>,
    weather: Res<crate::weather::WeatherFrame>,
) {
    commands.insert_resource(TerrainTextures {
        albedo: images.add(array_image(
            include_bytes!("../assets/terrain/albedo.png"),
            true,
        )),
        normal: images.add(array_image(
            include_bytes!("../assets/terrain/normal.png"),
            false,
        )),
        mask: images.add(array_image(
            include_bytes!("../assets/terrain/mask.png"),
            false,
        )),
        noise: images.add(cliff_noise()),
        detail_mask: images.add(crate::details::mask_image(vec![0; 512 * 512 * 4])),
        environment: environment.uniform.clone(),
        environment_sky: reflection.cube.clone(),
        environment_shadows: reflection.shadows.clone(),
        global_weather: weather.texture.clone(),
        forest_mask: images.add(crate::forest::mask::image(vec![
            0;
            crate::forest::mask::PIXELS
        ])),
        detail_mapping: default(),
        detail_frame: buffers.add(ShaderBuffer::from(
            crate::lod_material::DetailMapping::default(),
        )),
    });
}

fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    })
}

fn array_image(png: &[u8], srgb: bool) -> Image {
    let mut image = Image::from_buffer(
        png,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        srgb,
        sampler(),
        RenderAssetUsages::RENDER_WORLD,
    )
    .expect("embedded Unity ground texture PNG");
    image
        .reinterpret_stacked_2d_as_array(6)
        .expect("six ground recipe layers");
    let size = image.width() as usize;
    assert_eq!(image.height() as usize, size);
    let base = image.data.take().expect("decoded texture pixels");
    let mut data = Vec::with_capacity(base.len() * 4 / 3 + 64);
    // WGPU's default LayerMajor order: all mips of one layer precede the next.
    // Downsample layers independently; filtering never mixes material recipes.
    for layer in base.chunks_exact(size * size * 4) {
        let mut pixels = layer.to_vec();
        let mut width = size;
        loop {
            data.extend_from_slice(&pixels);
            if width == 1 {
                break;
            }
            let mut next = Vec::with_capacity(width * width);
            for y in 0..width / 2 {
                for x in 0..width / 2 {
                    for channel in 0..4 {
                        let values = [0, 1, width, width + 1].map(|offset| {
                            let byte = pixels[((y * 2 * width + x * 2 + offset) * 4) + channel];
                            let value = f32::from(byte) / 255.0;
                            if srgb && channel < 3 {
                                srgb_to_linear(value)
                            } else {
                                value
                            }
                        });
                        let mut value = values.iter().sum::<f32>() * 0.25;
                        if srgb && channel < 3 {
                            value = linear_to_srgb(value);
                        }
                        next.push(byte(value));
                    }
                }
            }
            pixels = next;
            width /= 2;
        }
    }
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = size.ilog2() + 1;
    image
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        v.powf(1.0 / 2.4).mul_add(1.055, -0.055)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

// Same periodic lattice, seeds and quintic fade as ProceduralNoiseTextures.cs.
#[allow(clippy::cast_precision_loss)]
fn lattice([x, y, z]: [u32; 3], seed: u32) -> f32 {
    let mut value = (x % 16).wrapping_mul(0x8DA6_B343)
        ^ (y % 16).wrapping_mul(0xD816_3841)
        ^ (z % 16).wrapping_mul(0xCB1A_B31F)
        ^ seed;
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    (value & 0x00FF_FFFF) as f32 / 16_777_215.0
}

fn coherent(cell: [u32; 3], fractions: [f32; 3], seed: u32) -> f32 {
    let f = fractions.map(|v| v * v * v * v.mul_add(v.mul_add(6.0, -15.0), 10.0));
    let mut values = [0.0; 4];
    for z in 0..2 {
        for y in 0..2 {
            let a = lattice([cell[0], cell[1] + y, cell[2] + z], seed);
            let b = lattice([cell[0] + 1, cell[1] + y, cell[2] + z], seed);
            values[(z * 2 + y) as usize] = (b - a).mul_add(f[0], a);
        }
    }
    let near = (values[1] - values[0]).mul_add(f[1], values[0]);
    let far = (values[3] - values[2]).mul_add(f[1], values[2]);
    (far - near).mul_add(f[2], near)
}

fn cliff_noise() -> Image {
    let mut pixels = Vec::with_capacity(64 * 64 * 64 * 4);
    for z in 0..64 {
        for y in 0..64 {
            for x in 0..64 {
                let p = [x, y, z];
                let cell = p.map(|v| v / 4);
                let fractions = p.map(|v| (f32::from(u8::try_from(v % 4).unwrap()) + 0.5) * 0.25);
                for seed in [0xA341_316C, 0xC801_3EA4, 0xAD90_777D] {
                    pixels.push(byte(coherent(cell, fractions, seed)));
                }
                pixels.push(255);
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 64,
        },
        TextureDimension::D3,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    let mut mip_size = 64_usize;
    let mut offset = 0;
    let data = image.data.as_mut().expect("new cliff noise");
    while mip_size > 1 {
        let next_size = mip_size / 2;
        let mut mip = Vec::with_capacity(next_size.pow(3) * 4);
        for z in 0..next_size {
            for y in 0..next_size {
                for x in 0..next_size {
                    let origin = x * 2 + mip_size * (y * 2 + mip_size * z * 2);
                    for channel in 0..4 {
                        let mut sum = 0_u16;
                        for dz in 0..2 {
                            for dy in 0..2 {
                                for dx in 0..2 {
                                    sum += u16::from(
                                        data[offset
                                            + (origin + dx + mip_size * (dy + mip_size * dz)) * 4
                                            + channel],
                                    );
                                }
                            }
                        }
                        mip.push(u8::try_from((sum + 4) / 8).expect("average byte"));
                    }
                }
            }
        }
        offset = data.len();
        data.extend(mip);
        mip_size = next_size;
    }
    image.texture_descriptor.mip_level_count = 7;
    image.sampler = sampler();
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_detail_frame_writes_one_shared_buffer_without_dirtying_materials() {
        use crate::{
            details::GrassMaterial,
            lod_material::{DetailMapping, LodMaterial},
        };
        use bevy::asset::{AssetApp, AssetEvent, AssetPlugin};
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<ShaderBuffer>()
            .init_asset::<LodMaterial>()
            .init_asset::<GrassMaterial>();
        let handle = app
            .world_mut()
            .resource_mut::<Assets<ShaderBuffer>>()
            .add(ShaderBuffer::from(DetailMapping::default()));
        let mut textures = TerrainTextures {
            detail_frame: handle.clone(),
            ..default()
        };
        let ground = || LodMaterial {
            extension: textures.extension(1000.0),
            ..default()
        };
        // Retain the material handles throughout the frame update.
        let _terrain: Vec<_> = (0..400)
            .map(|_| {
                app.world_mut()
                    .resource_mut::<Assets<LodMaterial>>()
                    .add(ground())
            })
            .collect();
        let _grass: Vec<_> = (0..16)
            .map(|_| {
                app.world_mut()
                    .resource_mut::<Assets<GrassMaterial>>()
                    .add(GrassMaterial {
                        base: ground(),
                        ..default()
                    })
            })
            .collect();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<ShaderBuffer>>>()
            .clear();
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<LodMaterial>>>()
            .clear();
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<GrassMaterial>>>()
            .clear();
        let mapping = DetailMapping {
            origin: Vec4::new(50.0, 1000.0, 25.0, 160.0),
            tangent: Vec4::X,
            bitangent: Vec4::Z,
        };
        textures.set_detail_mapping(
            mapping,
            &mut app.world_mut().resource_mut::<Assets<ShaderBuffer>>(),
        );
        app.update();
        let writes = app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<ShaderBuffer>>>()
            .drain()
            .filter(|e| e.is_modified(handle.id()))
            .count();
        assert_eq!(writes, 1);
        assert!(
            app.world()
                .resource::<Messages<AssetEvent<LodMaterial>>>()
                .is_empty()
        );
        assert!(
            app.world()
                .resource::<Messages<AssetEvent<GrassMaterial>>>()
                .is_empty()
        );
        let expected = ShaderBuffer::from(mapping);
        let buffers = app.world().resource::<Assets<ShaderBuffer>>();
        let data = buffers.get(&handle).unwrap().data.as_ref().unwrap();
        assert_eq!(data.len(), 48);
        assert_eq!(Some(data), expected.data.as_ref());
        textures.set_detail_mapping(
            mapping,
            &mut app.world_mut().resource_mut::<Assets<ShaderBuffer>>(),
        );
        app.update();
        assert!(
            app.world()
                .resource::<Messages<AssetEvent<ShaderBuffer>>>()
                .is_empty()
        );
    }

    #[test]
    fn baked_arrays_have_isolated_full_mip_chains_and_correct_colour_spaces() {
        for (png, srgb) in [
            (
                include_bytes!("../assets/terrain/albedo.png").as_slice(),
                true,
            ),
            (
                include_bytes!("../assets/terrain/normal.png").as_slice(),
                false,
            ),
            (
                include_bytes!("../assets/terrain/mask.png").as_slice(),
                false,
            ),
        ] {
            let image = array_image(png, srgb);
            assert_eq!(
                image.texture_descriptor.size,
                Extent3d {
                    width: 512,
                    height: 512,
                    depth_or_array_layers: 6
                }
            );
            assert_eq!(image.texture_descriptor.mip_level_count, 10);
            assert_eq!(image.texture_descriptor.format.is_srgb(), srgb);
            assert_eq!(
                image.data.unwrap().len(),
                (0..10)
                    .map(|level| (512_usize >> level).max(1).pow(2) * 6 * 4)
                    .sum::<usize>()
            );
        }
    }

    #[test]
    fn cliff_lattice_repeats_continuously_in_all_three_axes() {
        let first = coherent([3, 5, 7], [0.2, 0.4, 0.8], 0xA341_316C);
        for cell in [[19, 5, 7], [3, 21, 7], [3, 5, 23]] {
            assert_eq!(
                first.to_bits(),
                coherent(cell, [0.2, 0.4, 0.8], 0xA341_316C).to_bits()
            );
        }
    }

    #[test]
    fn cliff_volume_mips_filter_fine_noise_at_planet_view_distances() {
        let image = cliff_noise();
        assert_eq!(image.texture_descriptor.dimension, TextureDimension::D3);
        assert_eq!(image.texture_descriptor.mip_level_count, 7);
        let pixels = image.data.unwrap();
        assert_eq!(
            pixels.len(),
            (0..7)
                .map(|level| (64_usize >> level).pow(3) * 4)
                .sum::<usize>()
        );
        let last = &pixels[pixels.len() - 4..];
        assert!(last[..3].iter().all(|&v| (120..=135).contains(&v)));
        assert_eq!(last[3], 255);
    }
}
