//! Upload adapter for the engine-neutral runtime material bake.

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use motu::{IslandMaterialKind, IslandMaterialTextures, RuntimeMaterialInputs, TextureSet};

/// Three 3x2 atlases keep the complete six-material set to three texture and
/// three sampler bindings, which stays comfortably below Metal's limits.
pub struct MaterialAtlases {
    pub albedo: Handle<Image>,
    pub normal: Handle<Image>,
    pub mask: Handle<Image>,
    pub dirt_colour: Vec3,
    pub stone_colour: Vec3,
    pub sand_colour: Vec3,
}

pub fn upload(
    images: &mut Assets<Image>,
    inputs: RuntimeMaterialInputs,
    textures: IslandMaterialTextures,
) -> Result<MaterialAtlases, String> {
    let first = textures
        .materials
        .get(&IslandMaterialKind::Dirt)
        .ok_or_else(|| String::from("runtime material group has no dirt texture"))?;
    let dimensions = first.dimensions();
    for kind in IslandMaterialKind::ALL {
        let texture = textures
            .materials
            .get(&kind)
            .ok_or_else(|| format!("runtime material group has no {} texture", kind.name()))?;
        if texture.dimensions() != dimensions {
            return Err(format!(
                "{} material extent differs from the atlas",
                kind.name()
            ));
        }
    }

    let width = dimensions
        .width
        .checked_mul(3)
        .ok_or_else(|| String::from("material atlas width overflow"))?;
    let height = dimensions
        .height
        .checked_mul(2)
        .ok_or_else(|| String::from("material atlas height overflow"))?;
    let pixels = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| String::from("material atlas pixel count overflow"))?;
    let mut albedo = vec![255; pixels * 4];
    let mut normal = vec![0; pixels * 4];
    let mut mask = vec![0; pixels * 4];
    for alpha in normal.iter_mut().skip(3).step_by(4) {
        *alpha = 255;
    }
    for alpha in mask.iter_mut().skip(3).step_by(4) {
        *alpha = 255;
    }

    for kind in IslandMaterialKind::ALL {
        let texture = &textures.materials[&kind];
        write_slot(&mut albedo, width, kind, texture, SlotMap::Albedo);
        write_slot(&mut normal, width, kind, texture, SlotMap::Normal);
        write_slot(&mut mask, width, kind, texture, SlotMap::Mask);
    }

    let dirt = inputs.dirt_colour.channels();
    let stone = inputs.stone_colour.channels();
    let sand = inputs.sand_colour.channels();
    Ok(MaterialAtlases {
        albedo: images.add(image(width, height, albedo, TextureFormat::Rgba8UnormSrgb)),
        normal: images.add(image(width, height, normal, TextureFormat::Rgba8Unorm)),
        mask: images.add(image(width, height, mask, TextureFormat::Rgba8Unorm)),
        dirt_colour: Vec3::from_array(dirt),
        stone_colour: Vec3::from_array(stone),
        sand_colour: Vec3::from_array(sand),
    })
}

#[derive(Clone, Copy)]
enum SlotMap {
    Albedo,
    Normal,
    Mask,
}

fn write_slot(
    atlas: &mut [u8],
    atlas_width: u32,
    kind: IslandMaterialKind,
    texture: &TextureSet,
    map: SlotMap,
) {
    let (slot_x, slot_y) = match kind {
        IslandMaterialKind::Dirt => (0, 0),
        IslandMaterialKind::ForestFloor => (1, 0),
        IslandMaterialKind::Rock => (2, 0),
        IslandMaterialKind::RiverBed => (0, 1),
        IslandMaterialKind::Beach => (1, 1),
        IslandMaterialKind::FallenStones => (2, 1),
    };
    let width = texture.dimensions.width;
    let height = texture.dimensions.height;
    for y in 0..height {
        for x in 0..width {
            let source = (y * width + x) as usize;
            let destination =
                (((slot_y * height + y) * atlas_width + slot_x * width + x) * 4) as usize;
            let rgba = match map {
                SlotMap::Albedo => {
                    let [red, green, blue] = texture.albedo.pixels()[source];
                    [red, green, blue, 255]
                }
                SlotMap::Normal => {
                    let [red, green, blue] = texture.normal.pixels()[source];
                    [red, green, blue, 255]
                }
                SlotMap::Mask => {
                    let height = texture.height.pixels()[source];
                    [
                        u8::try_from(height >> 8).unwrap_or(255),
                        texture.occlusion.pixels()[source],
                        0,
                        255,
                    ]
                }
            };
            atlas[destination..destination + 4].copy_from_slice(&rgba);
        }
    }
}

fn image(width: u32, height: u32, data: Vec<u8>, format: TextureFormat) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
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

#[cfg(test)]
mod tests {
    use motu::{
        LinearRgb, MaterialSelection, NormalConvention, RuntimeMaterialBakeOptions,
        RuntimeMaterialInputs, bake_island_materials,
    };

    use super::{SlotMap, write_slot};

    #[test]
    fn atlas_slots_do_not_overlap() {
        let textures = bake_island_materials(
            &RuntimeMaterialInputs::new(
                LinearRgb::new(0.1, 0.06, 0.03),
                LinearRgb::new(0.25, 0.27, 0.24),
                LinearRgb::new(0.54, 0.45, 0.18),
            ),
            &RuntimeMaterialBakeOptions {
                width: Some(64),
                height: Some(64),
                normal_convention: NormalConvention::OpenGl,
                materials: MaterialSelection::ALL,
            },
        )
        .unwrap();
        let mut atlas = vec![0; 192 * 128 * 4];
        for (kind, texture) in &textures.materials {
            write_slot(&mut atlas, 192, *kind, texture, SlotMap::Albedo);
        }
        assert!(atlas.chunks_exact(4).all(|pixel| pixel[3] == 255));
    }
}
