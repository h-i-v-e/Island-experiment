use std::{collections::BTreeMap, sync::Arc};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    math::Affine2,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::{
    GrubMaterial,
    grub::{GrubExtension, GrubUniform},
    native_to_bevy,
};
use crate::{BuildingMesh, BuildingPrototype, MaterialSlot};

/// One material-homogeneous immutable draw, shared by every placement.
#[derive(Clone, Debug)]
pub struct CompiledBuildingPart {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    pub grub_material: Option<Handle<GrubMaterial>>,
    /// Optional GPU-only material batching; source parts remain available for export.
    pub array_material: Option<Handle<super::BuildingArrayMaterial>>,
    pub triangles: usize,
    pub glass: bool,
    pub stairs: bool,
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
    Arc::new(compile_prototype(source, meshes, materials, images))
}

fn compile_prototype(
    source: Arc<BuildingPrototype>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> CompiledBuildingPrototype {
    let material_handles = compile_materials(&source, materials, images);
    let compile = |slot: MaterialSlot,
                   mesh: &BuildingMesh,
                   height: f32,
                   meshes: &mut Assets<Mesh>| CompiledBuildingPart {
        mesh: meshes.add(convert_mesh(mesh, height)),
        material: material_handles[&slot].clone(),
        grub_material: None,
        array_material: None,
        triangles: mesh.triangle_count(),
        glass: slot == MaterialSlot::Glass,
        stairs: slot.is_stairs(),
    };
    let lods = std::array::from_fn(|level| {
        source.lods[level]
            .parts
            .iter()
            .map(|part| compile(part.material, &part.mesh, 0., meshes))
            .collect()
    });
    let doors = source
        .doors
        .iter()
        .map(|door| compile(MaterialSlot::Door, &door.mesh, door.hinge[2], meshes))
        .collect();
    let door_hardware = source
        .doors
        .iter()
        .map(|door| {
            compile(
                MaterialSlot::Hardware,
                &door.hardware_mesh,
                door.hinge[2],
                meshes,
            )
        })
        .collect();
    let inspection_points = inspection_points(&source);
    let native_collision_bounds = collision_bounds(&source);
    let [min, max] = above_ground_bounds(&source).map(native_to_bevy);
    let texture_bytes = source
        .materials
        .iter()
        .map(|texture| texture.albedo.len() + texture.normal.len() + texture.masks.len())
        .sum::<usize>()
        + source.atlas.albedo.len() * 3;
    CompiledBuildingPrototype {
        source,
        lods,
        doors,
        door_hardware,
        inspection_points,
        radius: min.distance(max) * 0.5,
        centre: (min + max) * 0.5,
        native_collision_bounds,
        texture_bytes: texture_bytes * 4 / 3,
    }
}

/// Compile the optional floor-relative shader while sharing the existing image assets.
/// Register `BuildingGrubPlugin` before spawning these materials.
#[must_use]
pub fn compile_building_with_grub(
    source: Arc<BuildingPrototype>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    grub_materials: &mut Assets<GrubMaterial>,
    images: &mut Assets<Image>,
) -> Arc<CompiledBuildingPrototype> {
    let mut compiled = compile_prototype(source, meshes, materials, images);
    let filter = &compiled.source.recipe.grub_filter;
    if filter.enabled {
        let settings = GrubUniform::new(filter, compiled.source.recipe.seed);
        let mut shared = BTreeMap::new();
        for (parts, source) in compiled.lods.iter_mut().zip(&compiled.source.lods) {
            for (part, source) in parts.iter_mut().zip(&source.parts) {
                if !crate::grub::exterior_material(&compiled.source.recipe, source.material) {
                    continue;
                }
                let handle = shared.entry(part.material.id()).or_insert_with(|| {
                    // ExtendedMaterial owns a base snapshot. Only image handles are
                    // cloned here: pixels and generated geometry remain shared.
                    let mut base = materials
                        .get(&part.material)
                        .expect("fresh material")
                        .clone();
                    base.opaque_render_method = bevy::material::OpaqueRendererMethod::Forward;
                    grub_materials.add(GrubMaterial {
                        base,
                        extension: GrubExtension { settings },
                    })
                });
                part.grub_material = Some(handle.clone());
            }
        }
    }
    Arc::new(compiled)
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

pub(super) fn convert_mesh(source: &BuildingMesh, height_offset: f32) -> Mesh {
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
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_1,
        source
            .positions
            .iter()
            .map(|point| [point[2] + height_offset, 0.])
            .collect::<Vec<_>>(),
    );
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
        if texture.slot == MaterialSlot::ThatchEnd {
            material.reflectance = 0.1;
        }
        if texture.slot.thatch_shell_layer().is_some() {
            // The standard masked PBR path clips the same strand holes in
            // colour, depth and shadow passes. Shell geometry supplies depth.
            material.alpha_mode = AlphaMode::Mask(0.5);
            material.double_sided = true;
            material.cull_mode = None;
            material.reflectance = 0.1;
        }
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
        MaterialSlot::Ceiling,
        MaterialSlot::Interior,
        MaterialSlot::Trim,
        MaterialSlot::Door,
        MaterialSlot::WindowProxy,
    ]
    .into_iter()
    .chain(
        source
            .recipe
            .chimney
            .iter()
            .flat_map(|_| [MaterialSlot::Chimney, MaterialSlot::Firebox]),
    ) {
        handles.entry(slot).or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: {
                    let colour = crate::materials::colour(&source.recipe, slot);
                    Color::linear_rgb(colour[0], colour[1], colour[2])
                },
                perceptual_roughness: if slot == MaterialSlot::Firebox {
                    0.98
                } else {
                    0.85
                },
                ..default()
            })
        });
    }
    for alias in [MaterialSlot::Stairs, MaterialSlot::StairTreads] {
        if let Some(material) = handles.get(&alias.texture_slot()).cloned() {
            handles.insert(alias, material);
        }
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
pub(super) enum TextureKind {
    Srgb,
    Linear,
    Normal,
}

pub(super) fn texture_image(size: u32, data: &[u8], kind: TextureKind, repeating: bool) -> Image {
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

pub(crate) fn above_ground_bounds(source: &BuildingPrototype) -> [[f32; 3]; 2] {
    source.lods[0]
        .parts
        .iter()
        .filter(|part| {
            !matches!(
                part.material,
                MaterialSlot::Foundation | MaterialSlot::Stairs | MaterialSlot::StairTreads
            )
        })
        .flat_map(|part| &part.mesh.positions)
        .fold(
            [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]],
            |mut bounds, point| {
                for axis in 0..3 {
                    bounds[0][axis] = bounds[0][axis].min(point[axis]);
                    bounds[1][axis] = bounds[1][axis].max(point[axis]);
                }
                bounds
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn straw_shells_use_masked_pbr_with_cutout_maps() {
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.roof.covering = crate::RoofCovering::Thatch;
        let source = Arc::new(crate::generate(&recipe).unwrap());
        let mut meshes = Assets::default();
        let mut materials = Assets::default();
        let mut images = Assets::default();
        let compiled = compile_building(source, &mut meshes, &mut materials, &mut images);
        let end_index = compiled.source.lods[0]
            .parts
            .iter()
            .position(|part| part.material == MaterialSlot::ThatchEnd)
            .unwrap();
        let end_material = materials
            .get(&compiled.lods[0][end_index].material)
            .unwrap();
        assert_eq!(end_material.alpha_mode, AlphaMode::Opaque);
        assert!(end_material.normal_map_texture.is_some());
        for slot in MaterialSlot::THATCH_SHELLS {
            let index = compiled.source.lods[0]
                .parts
                .iter()
                .position(|part| part.material == slot)
                .expect("four shell draws");
            let part = &compiled.lods[0][index];
            assert!(!part.glass);
            let material = materials.get(&part.material).unwrap();
            assert_eq!(material.alpha_mode, AlphaMode::Mask(0.5));
            assert!(material.double_sided && material.cull_mode.is_none());
            let texture = images
                .get(material.base_color_texture.as_ref().unwrap())
                .unwrap();
            let data = texture.data.as_ref().unwrap();
            assert!(data.chunks_exact(4).any(|p| p[3] == 0));
            assert!(data.chunks_exact(4).any(|p| p[3] >= 128));
        }
    }
    #[test]
    fn axis_conversion_preserves_winding_and_normal_map_tangents() {
        let mesh = convert_mesh(
            &BuildingMesh {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                indices: vec![0, 1, 2],
            },
            0.,
        );
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

#[cfg(test)]
mod grub_tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn church_stone_weathers_outside_while_its_interior_and_joinery_remain_clean() {
        let mut recipe = crate::BuildingRecipe::gothic_church();
        recipe.atlas_size = 128;
        let compiled = compile_building_with_grub(
            Arc::new(crate::generate(&recipe).unwrap()),
            &mut Assets::default(),
            &mut Assets::default(),
            &mut Assets::default(),
            &mut Assets::default(),
        );
        for (parts, source) in compiled.lods.iter().zip(&compiled.source.lods) {
            for (part, source) in parts.iter().zip(&source.parts) {
                let exterior = matches!(
                    source.material,
                    MaterialSlot::Wall
                        | MaterialSlot::Foundation
                        | MaterialSlot::Stairs
                        | MaterialSlot::StairTreads
                );
                assert_eq!(
                    part.grub_material.is_some(),
                    exterior,
                    "{:?}",
                    source.material
                );
            }
        }
        assert!(
            compiled
                .doors
                .iter()
                .all(|part| part.grub_material.is_none())
        );
    }

    #[test]
    fn shared_timber_joinery_chimney_and_doors_keep_their_original_materials() {
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.walls.style = crate::WallStyle::WattleAndDaub;
        recipe.roof.covering = crate::RoofCovering::Thatch;
        recipe.grub_filter.enabled = true;
        let compiled = compile_building_with_grub(
            Arc::new(crate::generate(&recipe).unwrap()),
            &mut Assets::default(),
            &mut Assets::default(),
            &mut Assets::default(),
            &mut Assets::default(),
        );
        for slot in [
            MaterialSlot::Timber,
            MaterialSlot::Trim,
            MaterialSlot::Chimney,
            MaterialSlot::Floor,
            MaterialSlot::Ceiling,
            MaterialSlot::Interior,
            MaterialSlot::StoneSlab,
        ] {
            let index = compiled.source.lods[0]
                .parts
                .iter()
                .position(|p| p.material == slot)
                .unwrap();
            assert!(compiled.lods[0][index].grub_material.is_none(), "{slot:?}");
        }
        for slot in [
            MaterialSlot::Wall,
            MaterialSlot::Roof,
            MaterialSlot::Foundation,
            MaterialSlot::Stairs,
            MaterialSlot::StairTreads,
        ] {
            let index = compiled.source.lods[0]
                .parts
                .iter()
                .position(|p| p.material == slot)
                .unwrap();
            assert!(compiled.lods[0][index].grub_material.is_some(), "{slot:?}");
        }
        assert!(compiled.doors.iter().all(|p| p.grub_material.is_none()));
    }

    fn assert_building_floor(source: &BuildingPrototype) {
        let floor = source.lods[0]
            .parts
            .iter()
            .find(|part| part.material == MaterialSlot::Floor)
            .unwrap();
        assert!(
            floor
                .mesh
                .positions
                .iter()
                .any(|point| point[2].abs() < 0.00001)
        );
        assert!(floor.mesh.positions.iter().all(|point| point[2] <= 0.00001));
    }

    fn assert_fine_uniform(material: &GrubMaterial, filter: &crate::GrubFilter) {
        assert_eq!(
            material.extension.settings.noise,
            Vec4::new(filter.fine_patch_size, filter.fine_mix, 0., 0.)
        );
    }

    #[test]
    fn filter_shares_images_keeps_alpha_and_carries_floor_height_on_all_lods_and_doors() {
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.roof.covering = crate::RoofCovering::Thatch;
        let mut meshes = Assets::default();
        let mut materials = Assets::default();
        let mut grub_materials = Assets::default();
        let mut images = Assets::default();
        let clean = compile_building_with_grub(
            Arc::new(crate::generate(&recipe).unwrap()),
            &mut meshes,
            &mut materials,
            &mut grub_materials,
            &mut images,
        );
        assert!(grub_materials.is_empty());
        assert!(
            clean
                .lods
                .iter()
                .flatten()
                .all(|p| p.grub_material.is_none())
        );
        let image_count = images.len();
        recipe.grub_filter.enabled = true;
        let compiled = compile_building_with_grub(
            Arc::new(crate::generate(&recipe).unwrap()),
            &mut meshes,
            &mut materials,
            &mut grub_materials,
            &mut images,
        );
        assert_eq!(
            images.len(),
            image_count * 2,
            "the extension allocates no images"
        );
        assert_building_floor(&compiled.source);
        let mut shared = BTreeMap::new();
        for (level, parts) in compiled.lods.iter().enumerate() {
            for (source, part) in compiled.source.lods[level].parts.iter().zip(parts) {
                if !crate::grub::exterior_material(&recipe, source.material) {
                    assert!(part.grub_material.is_none());
                    continue;
                }
                let handle = part.grub_material.as_ref().unwrap();
                assert_eq!(
                    shared.entry(part.material.id()).or_insert(handle.id()),
                    &handle.id()
                );
                let extended = grub_materials.get(handle).unwrap();
                assert_fine_uniform(extended, &recipe.grub_filter);
                let base = materials.get(&part.material).unwrap();
                assert_eq!(extended.base.alpha_mode, base.alpha_mode);
                assert_eq!(extended.base.base_color_texture, base.base_color_texture);
                assert_eq!(extended.base.normal_map_texture, base.normal_map_texture);
                assert_eq!(extended.base.uv_transform, base.uv_transform);
                let VertexAttributeValues::Float32x2(heights) = meshes
                    .get(&part.mesh)
                    .unwrap()
                    .attribute(Mesh::ATTRIBUTE_UV_1)
                    .unwrap()
                else {
                    panic!("UV1 heights")
                };
                assert!(
                    heights
                        .iter()
                        .zip(&source.mesh.positions)
                        .all(|(h, p)| (h[0] - p[2]).abs() < 0.00001)
                );
            }
        }
        assert!(
            compiled
                .door_hardware
                .iter()
                .all(|p| p.grub_material.is_none())
        );
        for (source, part) in compiled.source.doors.iter().zip(&compiled.doors) {
            assert!(part.grub_material.is_none());
            let VertexAttributeValues::Float32x2(heights) = meshes
                .get(&part.mesh)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_UV_1)
                .unwrap()
            else {
                panic!("leaf heights")
            };
            assert!(
                heights
                    .iter()
                    .zip(&source.mesh.positions)
                    .all(|(h, p)| (h[0] - p[2] - source.hinge[2]).abs() < 0.00001)
            );
        }
    }
}
