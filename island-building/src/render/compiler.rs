use std::{collections::BTreeMap, sync::Arc};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    math::Affine2,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::native_to_bevy;
use crate::{BuildingMesh, BuildingPrototype, MaterialSlot};

/// One material-homogeneous immutable draw, shared by every placement.
#[derive(Clone, Debug)]
pub struct CompiledBuildingPart {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    pub triangles: usize,
    pub glass: bool,
}

/// Retaining this value keeps all assets and collision metadata resident.
#[derive(Debug)]
pub struct CompiledBuildingPrototype {
    pub source: Arc<BuildingPrototype>,
    pub lods: [Vec<CompiledBuildingPart>; 3],
    pub doors: Vec<CompiledBuildingPart>,
    /// Separate metal hardware keeps a textured door leaf from texturing its knobs.
    pub door_hardware: Vec<CompiledBuildingPart>,
    /// Close inspection points in Bevy-local metres, including all windows.
    pub inspection_points: Vec<Vec3>,
    pub radius: f32,
    pub centre: Vec3,
    /// Native Z-up bounds including every possible moving-leaf pose.
    pub native_collision_bounds: [Vec3; 2],
    pub texture_bytes: usize,
}

/// Compiles a complete immutable prototype once; placements only clone handles.
#[must_use]
pub fn compile_building(
    source: Arc<BuildingPrototype>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Arc<CompiledBuildingPrototype> {
    let material_handles = compile_materials(&source, materials, images);
    let compile =
        |slot: MaterialSlot, mesh: &BuildingMesh, meshes: &mut Assets<Mesh>| CompiledBuildingPart {
            mesh: meshes.add(convert_mesh(mesh)),
            material: material_handles[&slot].clone(),
            triangles: mesh.triangle_count(),
            glass: slot == MaterialSlot::Glass,
        };
    let lods = std::array::from_fn(|level| {
        source.lods[level]
            .parts
            .iter()
            .map(|part| compile(part.material, &part.mesh, meshes))
            .collect()
    });
    let doors = source
        .doors
        .iter()
        .map(|door| compile(MaterialSlot::Door, &door.mesh, meshes))
        .collect();
    let door_hardware = source
        .doors
        .iter()
        .map(|door| compile(MaterialSlot::Hardware, &door.hardware_mesh, meshes))
        .collect();
    let inspection_points = inspection_points(&source);
    let native_collision_bounds = collision_bounds(&source);
    let min = native_to_bevy(source.layout.bounds_min);
    let max = native_to_bevy(source.layout.bounds_max);
    let texture_bytes = source
        .materials
        .iter()
        .map(|texture| texture.albedo.len() + texture.normal.len() + texture.masks.len())
        .sum::<usize>()
        + source.atlas.albedo.len() * 3;
    Arc::new(CompiledBuildingPrototype {
        source,
        lods,
        doors,
        door_hardware,
        inspection_points,
        radius: min.distance(max) * 0.5,
        centre: (min + max) * 0.5,
        native_collision_bounds,
        texture_bytes: texture_bytes * 4 / 3,
    })
}

fn collision_bounds(source: &BuildingPrototype) -> [Vec3; 2] {
    let mut min = Vec3::from_array(source.layout.bounds_min);
    let mut max = Vec3::from_array(source.layout.bounds_max);
    for door in &source.doors {
        let hinge = Vec3::from_array(door.hinge);
        let reach = door.width + door.thickness;
        min = min.min(hinge - Vec3::new(reach, reach, 0.0));
        max = max.max(hinge + Vec3::new(reach, reach, door.height));
    }
    [min, max]
}

fn inspection_points(source: &BuildingPrototype) -> Vec<Vec3> {
    let mut points: Vec<_> = source
        .doors
        .iter()
        .map(|door| native_to_bevy(door.hinge) + Vec3::Y * (door.height * 0.5))
        .collect();
    // Keep every window vertex: a merged glass mesh can span opposite facades.
    points.extend(
        source.lods[0]
            .parts
            .iter()
            .filter(|part| part.material == MaterialSlot::Glass)
            .flat_map(|part| part.mesh.positions.iter().copied().map(native_to_bevy)),
    );
    points
}

fn convert_mesh(source: &BuildingMesh) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        source
            .positions
            .iter()
            .copied()
            .map(|point| native_to_bevy(point).to_array())
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        source
            .normals
            .iter()
            .copied()
            .map(|normal| native_to_bevy(normal).to_array())
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, source.uvs.clone());
    mesh.insert_indices(Indices::U32(source.indices.clone()));
    // Normal maps require tangent space; all generated structural charts carry UVs.
    let _ = mesh.generate_tangents();
    mesh
}

fn compile_materials(
    source: &BuildingPrototype,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> BTreeMap<MaterialSlot, Handle<StandardMaterial>> {
    let mut handles = BTreeMap::new();
    for texture in &source.materials {
        let mut material = textured_material(
            texture.size,
            &texture.albedo,
            &texture.normal,
            &texture.masks,
            true,
            images,
        );
        // Structural UVs use a two-metre chart; recipe dependencies retain their
        // authored physical tile dimensions rather than inheriting that default.
        material.uv_transform = Affine2::from_scale(Vec2::new(
            2.0 / texture.physical_size[0].max(0.001),
            2.0 / texture.physical_size[1].max(0.001),
        ));
        handles.insert(texture.slot, materials.add(material));
    }
    handles.insert(
        MaterialSlot::Atlas,
        materials.add(textured_material(
            source.atlas.size,
            &source.atlas.albedo,
            &source.atlas.normal,
            &source.atlas.masks,
            false,
            images,
        )),
    );
    let metal = crate::materials::colour(&source.recipe, MaterialSlot::Hardware);
    handles.insert(
        MaterialSlot::Hardware,
        materials.add(StandardMaterial {
            base_color: Color::linear_rgb(metal[0], metal[1], metal[2]),
            metallic: 0.9,
            perceptual_roughness: 0.24,
            ..default()
        }),
    );
    let glass = crate::materials::colour(&source.recipe, MaterialSlot::Glass);
    handles.insert(
        MaterialSlot::Glass,
        materials.add(StandardMaterial {
            base_color: Color::linear_rgba(glass[0], glass[1], glass[2], glass[3]),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.08,
            reflectance: 0.12,
            double_sided: true,
            cull_mode: None,
            ..default()
        }),
    );
    for slot in [
        MaterialSlot::Wall,
        MaterialSlot::Roof,
        MaterialSlot::Floor,
        MaterialSlot::Interior,
        MaterialSlot::Trim,
        MaterialSlot::Door,
        MaterialSlot::WindowProxy,
    ] {
        handles.entry(slot).or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: {
                    let colour = crate::materials::colour(&source.recipe, slot);
                    Color::linear_rgb(colour[0], colour[1], colour[2])
                },
                perceptual_roughness: 0.85,
                ..default()
            })
        });
    }
    handles
}

fn textured_material(
    size: u32,
    albedo: &[u8],
    normal: &[u8],
    masks: &[u8],
    repeating: bool,
    images: &mut Assets<Image>,
) -> StandardMaterial {
    let mask = images.add(texture_image(size, masks, TextureKind::Linear, repeating));
    StandardMaterial {
        base_color_texture: Some(images.add(texture_image(
            size,
            albedo,
            TextureKind::Srgb,
            repeating,
        ))),
        normal_map_texture: Some(images.add(texture_image(
            size,
            normal,
            TextureKind::Normal,
            repeating,
        ))),
        occlusion_texture: Some(mask.clone()),
        metallic_roughness_texture: Some(mask),
        perceptual_roughness: 1.0,
        metallic: 1.0,
        ..default()
    }
}

#[derive(Clone, Copy)]
enum TextureKind {
    Srgb,
    Linear,
    Normal,
}

fn texture_image(size: u32, data: &[u8], kind: TextureKind, repeating: bool) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data.to_vec(),
        if matches!(kind, TextureKind::Srgb) {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::default(),
    );
    let mut sampler = ImageSamplerDescriptor::linear();
    sampler.set_address_mode(if repeating {
        ImageAddressMode::Repeat
    } else {
        ImageAddressMode::ClampToEdge
    });
    image.sampler = ImageSampler::Descriptor(sampler);
    let (mips, levels) = mip_chain(size, data, kind);
    image.data = Some(mips);
    image.texture_descriptor.mip_level_count = levels;
    image
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped RGBA8 quantization.
fn mip_chain(mut size: u32, data: &[u8], kind: TextureKind) -> (Vec<u8>, u32) {
    let mut pixels = data.to_vec();
    let mut offset = 0;
    let mut levels = 1;
    while size > 1 {
        let next_size = (size / 2).max(1);
        let mut next = Vec::with_capacity((next_size * next_size * 4) as usize);
        for y in 0..next_size {
            for x in 0..next_size {
                let mut average = [0.0; 4];
                for dy in 0..2 {
                    for dx in 0..2 {
                        let index = offset
                            + (((y * 2 + dy).min(size - 1) * size + (x * 2 + dx).min(size - 1)) * 4)
                                as usize;
                        for channel in 0..4 {
                            let value = f32::from(pixels[index + channel]) / 255.0;
                            average[channel] += if channel < 3 && matches!(kind, TextureKind::Srgb)
                            {
                                if value <= 0.04045 {
                                    value / 12.92
                                } else {
                                    ((value + 0.055) / 1.055).powf(2.4)
                                }
                            } else {
                                value
                            } * 0.25;
                        }
                    }
                }
                if matches!(kind, TextureKind::Normal) {
                    let normal = (Vec3::from_array([average[0], average[1], average[2]]) * 2.0
                        - Vec3::ONE)
                        .try_normalize()
                        .unwrap_or(Vec3::Z)
                        * 0.5
                        + Vec3::splat(0.5);
                    average[..3].copy_from_slice(&normal.to_array());
                }
                for (channel, value) in average.into_iter().enumerate() {
                    let value = if channel < 3 && matches!(kind, TextureKind::Srgb) {
                        if value <= 0.003_130_8 {
                            value * 12.92
                        } else {
                            1.055 * value.powf(1.0 / 2.4) - 0.055
                        }
                    } else {
                        value
                    };
                    next.push((value.clamp(0.0, 1.0) * 255.0).round() as u8);
                }
            }
        }
        offset = pixels.len();
        pixels.extend(next);
        size = next_size;
        levels += 1;
    }
    (pixels, levels)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn axis_conversion_preserves_winding_and_normal_map_tangents() {
        let mesh = convert_mesh(&BuildingMesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            indices: vec![0, 1, 2],
        });
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let normal = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .unwrap()
            .as_float3()
            .unwrap();
        let geometric_normal = (Vec3::from_array(positions[1]) - Vec3::from_array(positions[0]))
            .cross(Vec3::from_array(positions[2]) - Vec3::from_array(positions[0]))
            .normalize();
        assert_eq!(geometric_normal, Vec3::Y);
        assert!(Vec3::from_array(normal[0]).abs_diff_eq(Vec3::Y, f32::EPSILON));
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }
    #[test]
    fn mipmaps_filter_colour_in_linear_space_and_renormalize_normals() {
        let (colour, levels) = mip_chain(
            2,
            &[
                0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255,
            ],
            TextureKind::Srgb,
        );
        assert_eq!(levels, 2);
        assert!(colour[16] > 180 && colour[16] < 190);
        let (normals, _) = mip_chain(2, &[128, 128, 255, 255].repeat(4), TextureKind::Normal);
        assert_eq!(&normals[16..], &[128, 128, 255, 255]);
    }
}
