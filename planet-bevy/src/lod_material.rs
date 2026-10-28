//! Fine terrain normals and ambient occlusion for simplified render chunks.

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::{
        render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat},
        storage::ShaderBuffer,
    },
    shader::{ShaderRef, load_shader_library},
};

pub type LodMaterial = ExtendedMaterial<StandardMaterial, SurfaceExtension>;

pub struct LodMaterialPlugin;

impl Plugin for LodMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/radial_mapping.wgsl");
        load_shader_library!(app, "shaders/ground_common.wgsl");
        embedded_asset!(app, "shaders/lod_surface.wgsl");
        app.add_plugins(MaterialPlugin::<LodMaterial>::default());
    }
}

/// Diagnostic values: zero is normal rendering, one normals, two occlusion.
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct SurfaceSettings {
    pub enabled: u32,
    pub debug: u32,
    pub rivers: u32,
    /// Zero legacy/sphere, one ground, two diagnostics, three rocks, four rivers.
    pub terrain: u32,
}

#[derive(Clone, Copy, Debug, Reflect, ShaderType)]
pub struct TerrainSettings {
    pub radius: f32,
    pub snow_line: f32,
    pub detail: f32,
    pub padding: f32,
    pub colour: Vec4,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect, ShaderType)]
pub struct DetailMapping {
    pub origin: Vec4,
    pub tangent: Vec4,
    pub bitangent: Vec4,
}

impl Default for TerrainSettings {
    fn default() -> Self {
        Self {
            radius: 0.0,
            snow_line: 100.0,
            detail: 1.0,
            padding: 0.0,
            colour: Vec4::ONE,
        }
    }
}

#[derive(Asset, AsBindGroup, Clone, Debug, Default, Reflect)]
pub struct SurfaceExtension {
    #[uniform(100)]
    pub settings: SurfaceSettings,
    /// Linear RGB encodes object-space normal corrections: byte 128 is zero,
    /// and 63.5 byte steps represent one normal unit. Alpha is occlusion.
    #[texture(101)]
    #[sampler(102)]
    pub surface_map: Option<Handle<Image>>,
    #[texture(103)]
    pub river_map: Option<Handle<Image>>,
    #[uniform(105)]
    pub terrain: TerrainSettings,
    #[texture(106, dimension = "2d_array")]
    #[sampler(109)]
    pub albedo: Option<Handle<Image>>,
    #[texture(107, dimension = "2d_array")]
    pub normal: Option<Handle<Image>>,
    #[texture(108, dimension = "2d_array")]
    pub mask: Option<Handle<Image>>,
    #[texture(110, dimension = "3d")]
    #[sampler(111)]
    pub noise: Option<Handle<Image>>,
    #[storage(112, read_only)]
    pub detail_mapping: Handle<ShaderBuffer>,
    #[texture(113)]
    #[sampler(114)]
    pub detail_mask: Option<Handle<Image>>,
    #[uniform(120)]
    pub water: crate::water::WaterUniform,
    #[texture(121)]
    #[sampler(122)]
    pub water_noise: Option<Handle<Image>>,
    #[storage(123, read_only)]
    pub environment: Handle<ShaderBuffer>,
    #[texture(126, dimension = "cube")]
    pub environment_sky: Option<Handle<Image>>,
    #[texture(128)]
    pub forest_mask: Option<Handle<Image>>,
}

impl MaterialExtension for SurfaceExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://planet_bevy/shaders/lod_surface.wgsl".into()
    }
}

/// Copies a borrowed CPU atlas into a render-owned image. Only two mip levels
/// are added: the chart baker's eight-pixel padding still isolates chart edges
/// at the coarsest level. Corrections are averaged without normalization so
/// unchanged geometry retains exactly zero correction even in distant mips.
pub fn atlas_image(width: u32, height: u32, pixels: &[u8]) -> Image {
    map_image(width, height, pixels, 4, TextureFormat::Rgba8Unorm)
}

pub fn river_image(width: u32, height: u32, pixels: &[u8]) -> Image {
    map_image(width, height, pixels, 1, TextureFormat::R8Unorm)
}

fn map_image(
    width: u32,
    height: u32,
    pixels: &[u8],
    channels: u32,
    format: TextureFormat,
) -> Image {
    assert!(width > 0 && height > 0, "surface atlas must be nonempty");
    assert_eq!(
        pixels.len(),
        byte_len(width, height) / 4 * channels as usize
    );
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels.to_vec(),
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    let levels = width.min(height).ilog2().min(2) + 1;
    let data = image.data.as_mut().expect("new image has pixel data");
    let mut start = 0;
    let (mut mip_width, mut mip_height) = (width, height);
    for _ in 1..levels {
        let next = downsample(&data[start..], mip_width, mip_height, channels as usize);
        start = data.len();
        data.extend_from_slice(&next);
        mip_width = (mip_width / 2).max(1);
        mip_height = (mip_height / 2).max(1);
    }
    image.texture_descriptor.mip_level_count = levels;
    let max_lod = u8::try_from(levels - 1).expect("surface atlas has at most three mips");
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        lod_max_clamp: f32::from(max_lod),
        ..default()
    });
    image
}

fn byte_len(width: u32, height: u32) -> usize {
    usize::try_from(u64::from(width) * u64::from(height) * 4)
        .expect("surface atlas dimensions fit in memory")
}

fn downsample(pixels: &[u8], width: u32, height: u32, channels: usize) -> Vec<u8> {
    let next_width = (width / 2).max(1);
    let next_height = (height / 2).max(1);
    let mut next = Vec::with_capacity(byte_len(next_width, next_height) / 4 * channels);
    for y in 0..next_height {
        for x in 0..next_width {
            let mut sums = [0_u16; 4];
            for dy in 0..2 {
                for dx in 0..2 {
                    let sx = (x * 2 + dx).min(width - 1);
                    let sy = (y * 2 + dy).min(height - 1);
                    let offset = (sx + sy * width) as usize * channels;
                    let pixel = &pixels[offset..offset + channels];
                    for (sum, &value) in sums.iter_mut().zip(pixel) {
                        *sum += u16::from(value);
                    }
                }
            }
            next.extend(sums[..channels].iter().map(|sum| {
                u8::try_from((sum + 2) / 4).expect("average of four bytes fits in a byte")
            }));
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn river_mask_uses_one_byte_per_texel_and_averages_coverage_in_mips() {
        let image = river_image(2, 2, &[0, 0, 255, 255]);
        assert_eq!(image.texture_descriptor.format, TextureFormat::R8Unorm);
        assert_eq!(image.texture_descriptor.mip_level_count, 2);
        assert_eq!(image.data.unwrap(), &[0, 0, 255, 255, 128]);
    }

    #[test]
    fn atlas_is_linear_and_chart_padding_survives_the_mip_chain() {
        let image = atlas_image(16, 16, &[128, 128, 128, 192].repeat(256));
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        assert_eq!(image.texture_descriptor.mip_level_count, 3);
        assert_eq!(image.data.as_ref().unwrap().len(), (256 + 64 + 16) * 4);
        let ImageSampler::Descriptor(sampler) = image.sampler else {
            panic!("atlas needs an explicit sampler");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::ClampToEdge);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::ClampToEdge);
        assert!((sampler.lod_max_clamp - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn mip_corrections_average_magnitude_without_normalization() {
        let pixels = [
            192, 128, 128, 0, 128, 160, 128, 64, 192, 128, 128, 128, 128, 160, 128, 255,
        ];
        let image = atlas_image(2, 2, &pixels);
        let mip = &image.data.as_ref().unwrap()[16..];
        assert_eq!(mip, &[160, 144, 128, 112]);
    }

    #[test]
    fn unchanged_geometry_keeps_exactly_zero_correction_at_every_mip() {
        let image = atlas_image(16, 16, &[128, 128, 128, 255].repeat(256));
        for pixel in image.data.unwrap().chunks_exact(4) {
            assert_eq!(pixel, &[128, 128, 128, 255]);
        }
        let opposing = [192, 128, 128, 255, 64, 128, 128, 255].repeat(2);
        let image = atlas_image(2, 2, &opposing);
        assert_eq!(&image.data.unwrap()[16..], &[128, 128, 128, 255]);
    }

    #[test]
    fn tiny_atlas_has_no_invalid_mips_and_material_defaults_to_standard_shading() {
        let image = atlas_image(1, 1, &[128, 128, 128, 255]);
        assert_eq!(image.texture_descriptor.mip_level_count, 1);
        assert_eq!(image.data.unwrap().len(), 4);
        let extension = SurfaceExtension::default();
        assert_eq!(extension.settings.enabled, 0);
        assert!(extension.surface_map.is_none());
    }
}
