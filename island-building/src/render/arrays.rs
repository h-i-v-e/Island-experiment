//! Optional GPU-only batching. CPU meshes and export material boundaries stay intact.
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::Arc,
};

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{
        AsBindGroup, ShaderType, TextureViewDescriptor, TextureViewDimension,
    },
    shader::ShaderRef,
};

use super::{
    CompiledBuildingPart, CompiledBuildingPrototype, GrubMaterial,
    compiler::{TextureKind, texture_image},
    grub::GrubUniform,
};
use crate::{BuildingPrototype, MaterialSlot, MaterialTexture};

const MAX_ROWS: usize = 32;

pub type BuildingArrayMaterial = ExtendedMaterial<StandardMaterial, BuildingArrayExtension>;

/// Registers both the array shader and the standard weathering fallback.
pub struct BuildingArrayPlugin;

impl Plugin for BuildingArrayPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<super::BuildingGrubPlugin>() {
            app.add_plugins(super::BuildingGrubPlugin);
        }
        embedded_asset!(app, "shaders/array_material.wgsl");
        embedded_asset!(app, "shaders/array_prepass.wgsl");
        app.add_plugins(MaterialPlugin::<BuildingArrayMaterial>::default());
    }
}

#[derive(Clone, Copy, Debug, Reflect, ShaderType)]
pub struct ArrayRows {
    /// Linear tint; array samples already perform sRGB conversion.
    pub colours: [Vec4; MAX_ROWS],
    /// UV scale, reflectance, exterior weathering flag.
    pub mapping: [Vec4; MAX_ROWS],
    /// Roughness, metallic, texture layer (-1 for an untextured material), reserved.
    pub surface: [Vec4; MAX_ROWS],
}

impl Default for ArrayRows {
    fn default() -> Self {
        Self {
            colours: [Vec4::ONE; MAX_ROWS],
            mapping: [Vec4::new(1., 1., 0.5, 0.); MAX_ROWS],
            surface: [Vec4::new(0.5, 0., -1., 0.); MAX_ROWS],
        }
    }
}

#[derive(Asset, AsBindGroup, Clone, Debug, Reflect)]
pub struct BuildingArrayExtension {
    #[uniform(100)]
    pub rows: ArrayRows,
    #[uniform(101)]
    pub grub: GrubUniform,
    #[texture(102, dimension = "2d_array")]
    #[sampler(105)]
    pub albedo: Handle<Image>,
    #[texture(103, dimension = "2d_array")]
    pub normal: Handle<Image>,
    #[texture(104, dimension = "2d_array")]
    pub masks: Handle<Image>,
}

impl MaterialExtension for BuildingArrayExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://island_building/render/shaders/array_material.wgsl".into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        "embedded://island_building/render/shaders/array_prepass.wgsl".into()
    }
}

struct Bank {
    slots: BTreeMap<MaterialSlot, usize>,
    textured_rows: HashSet<usize>,
    material: Handle<BuildingArrayMaterial>,
}

/// Compiles shared texture banks and merges compatible opaque draws in every LOD.
///
/// Banks retain their native resolution. Glass, masked straw shells and the
/// baked LOD2 atlas use the existing PBR path, including alpha-tested shadow/depth
/// passes. Other LOD2 parts reuse near-detail banks when available, avoiding
/// duplicate maps. Stairs remain independently hideable; doors move separately.
#[must_use]
pub fn compile_building_with_arrays(
    source: Arc<BuildingPrototype>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    grub_materials: &mut Assets<GrubMaterial>,
    array_materials: &mut Assets<BuildingArrayMaterial>,
    images: &mut Assets<Image>,
) -> Arc<CompiledBuildingPrototype> {
    let mut owner =
        super::compile_building_with_grub(source, meshes, materials, grub_materials, images);
    let compiled = Arc::get_mut(&mut owner).expect("new prototype has one owner");
    let original_materials: Vec<_> = all_parts(compiled).map(|p| p.material.clone()).collect();
    let original_grub: Vec<_> = all_parts(compiled)
        .filter_map(|p| p.grub_material.clone())
        .collect();
    let banks = compile_banks(compiled, materials, array_materials, images);
    for level in 0..3 {
        compiled.lods[level] = merge_parts(
            &compiled.lods[level],
            compiled.source.lods[level].parts.iter().map(|p| p.material),
            &banks,
            meshes,
        );
    }
    for (leaf, hardware) in compiled.doors.iter_mut().zip(&mut compiled.door_hardware) {
        let parts = [leaf.clone(), hardware.clone()];
        let merged = merge_parts(
            &parts,
            [MaterialSlot::Door, MaterialSlot::Hardware],
            &banks,
            meshes,
        );
        if merged.len() == 1 {
            *leaf = merged.into_iter().next().expect("one merged leaf");
            hardware.triangles = 0;
        }
    }
    compiled.texture_bytes = release_replaced_maps(
        compiled,
        &original_materials,
        &original_grub,
        materials,
        grub_materials,
        images,
    );
    compiled.texture_bytes += banks
        .iter()
        .map(|bank| {
            let material = array_materials
                .get(&bank.material)
                .expect("fresh array material");
            [
                &material.extension.albedo,
                &material.extension.normal,
                &material.extension.masks,
            ]
            .into_iter()
            .map(|handle| {
                images
                    .get(handle)
                    .and_then(|image| image.data.as_ref())
                    .map_or(0, Vec::len)
            })
            .sum::<usize>()
        })
        .sum::<usize>();
    owner
}

fn compile_banks(
    compiled: &CompiledBuildingPrototype,
    materials: &Assets<StandardMaterial>,
    arrays: &mut Assets<BuildingArrayMaterial>,
    images: &mut Assets<Image>,
) -> Vec<Bank> {
    let used = used_texture_slots(&compiled.source);
    let mut grouped: BTreeMap<u32, Vec<&MaterialTexture>> = BTreeMap::new();
    for texture in &compiled.source.materials {
        if used.contains(&texture.slot)
            && !matches!(texture.slot, MaterialSlot::Glass | MaterialSlot::Atlas)
            && texture.slot.thatch_shell_layer().is_none()
        {
            grouped.entry(texture.size).or_default().push(texture);
        }
    }
    let dominant = grouped
        .iter()
        .max_by_key(|(_, textures)| textures.len())
        .map(|(&size, _)| size);
    grouped
        .into_iter()
        .filter_map(|(size, textures)| {
            let slots = bank_slots(&compiled.source, &textures, Some(size) == dominant);
            if slots.values().any(|&row| row >= MAX_ROWS) {
                return None;
            }
            let filter = &compiled.source.recipe.grub_filter;
            let extension = BuildingArrayExtension {
                rows: bank_rows(compiled, &textures, &slots, materials),
                grub: if filter.enabled {
                    GrubUniform::new(filter, compiled.source.recipe.seed)
                } else {
                    GrubUniform::default()
                },
                albedo: images.add(array_image(size, &textures, TextureKind::Srgb)),
                normal: images.add(array_image(size, &textures, TextureKind::Normal)),
                masks: images.add(array_image(size, &textures, TextureKind::Linear)),
            };
            let textured_rows = slots
                .values()
                .copied()
                .filter(|&row| extension.rows.surface[row].z >= 0.)
                .collect();
            let material = arrays.add(BuildingArrayMaterial {
                base: StandardMaterial {
                    opaque_render_method: bevy::material::OpaqueRendererMethod::Forward,
                    ..default()
                },
                extension,
            });
            Some(Bank {
                slots,
                textured_rows,
                material,
            })
        })
        .collect()
}

fn used_texture_slots(source: &BuildingPrototype) -> BTreeSet<MaterialSlot> {
    source.lods[..2]
        .iter()
        .flat_map(|lod| &lod.parts)
        .filter(|part| !part.mesh.indices.is_empty())
        .map(|part| part.material.texture_slot())
        .chain(source.doors.iter().flat_map(|door| {
            [
                (!door.mesh.indices.is_empty()).then_some(MaterialSlot::Door),
                (!door.hardware_mesh.indices.is_empty()).then_some(MaterialSlot::Hardware),
            ]
            .into_iter()
            .flatten()
        }))
        .collect()
}

fn bank_slots(
    source: &BuildingPrototype,
    textures: &[&MaterialTexture],
    include_solids: bool,
) -> BTreeMap<MaterialSlot, usize> {
    let mut slots: BTreeMap<_, _> = textures
        .iter()
        .enumerate()
        .map(|(row, t)| (t.slot, row))
        .collect();
    // Aliases share pixels but need independent weathering policies.
    for slot in [MaterialSlot::Stairs, MaterialSlot::StairTreads] {
        if slots.contains_key(&slot.texture_slot()) {
            slots.insert(slot, slots.len());
        }
    }
    // Solid hardware can join a leaf at any texture resolution without adding
    // pixel layers or forcing that door to use the dominant structural bank.
    if !source
        .materials
        .iter()
        .any(|texture| texture.slot == MaterialSlot::Hardware)
    {
        let row = slots.len();
        slots.entry(MaterialSlot::Hardware).or_insert(row);
    }
    if include_solids {
        let candidates = source
            .lods
            .iter()
            .flat_map(|lod| lod.parts.iter().map(|part| part.material))
            .chain([MaterialSlot::Door, MaterialSlot::Hardware]);
        for slot in candidates {
            if !matches!(slot, MaterialSlot::Glass | MaterialSlot::Atlas)
                && slot.thatch_shell_layer().is_none()
                && !source
                    .materials
                    .iter()
                    .any(|t| t.slot == slot.texture_slot())
            {
                let row = slots.len();
                slots.entry(slot).or_insert(row);
            }
        }
    }
    slots
}

fn bank_rows(
    compiled: &CompiledBuildingPrototype,
    textures: &[&MaterialTexture],
    slots: &BTreeMap<MaterialSlot, usize>,
    materials: &Assets<StandardMaterial>,
) -> ArrayRows {
    let mut rows = ArrayRows::default();
    for (&slot, &row) in slots {
        let Some(base) = standard_material(compiled, slot, materials) else {
            continue;
        };
        rows.colours[row] = Vec4::from_array(base.base_color.to_linear().to_f32_array());
        let scale = base.uv_transform.matrix2;
        let exterior = crate::grub::exterior_material(&compiled.source.recipe, slot);
        rows.mapping[row] = Vec4::new(
            scale.x_axis.x,
            scale.y_axis.y,
            base.reflectance,
            f32::from(u8::from(exterior)),
        );
        let layer = textures.iter().position(|t| t.slot == slot.texture_slot());
        rows.surface[row] = Vec4::new(
            base.perceptual_roughness,
            base.metallic,
            layer.map_or(-1., |i| {
                f32::from(u16::try_from(i).expect("small material bank"))
            }),
            0.,
        );
    }
    rows
}

fn all_parts(compiled: &CompiledBuildingPrototype) -> impl Iterator<Item = &CompiledBuildingPart> {
    compiled
        .lods
        .iter()
        .flatten()
        .chain(&compiled.doors)
        .chain(&compiled.door_hardware)
}

fn material_maps(material: &StandardMaterial) -> impl Iterator<Item = &Handle<Image>> {
    [
        material.base_color_texture.as_ref(),
        material.normal_map_texture.as_ref(),
        material.metallic_roughness_texture.as_ref(),
        material.occlusion_texture.as_ref(),
    ]
    .into_iter()
    .flatten()
}

/// Discard maps replaced by array layers before the render world uploads them.
/// The baked LOD2 atlas, glass and cutouts keep their original handles.
fn release_replaced_maps(
    compiled: &CompiledBuildingPrototype,
    originals: &[Handle<StandardMaterial>],
    original_grub: &[Handle<GrubMaterial>],
    materials: &mut Assets<StandardMaterial>,
    grub: &mut Assets<GrubMaterial>,
    images: &mut Assets<Image>,
) -> usize {
    let used: HashSet<_> = all_parts(compiled)
        .filter(|p| p.array_material.is_none() && p.triangles > 0)
        .map(|p| p.material.id())
        .collect();
    let used_grub: HashSet<_> = all_parts(compiled)
        .filter_map(|p| p.grub_material.as_ref().map(Handle::id))
        .collect();
    let mut original_images = HashSet::new();
    for handle in originals {
        if let Some(mut material) = materials.get_mut(handle) {
            original_images.extend(material_maps(&material).map(Handle::id));
            if !used.contains(&handle.id()) {
                material.base_color_texture = None;
                material.normal_map_texture = None;
                material.metallic_roughness_texture = None;
                material.occlusion_texture = None;
            }
        }
    }
    for handle in original_grub {
        if !used_grub.contains(&handle.id()) {
            grub.remove(handle);
        }
    }
    let retained: HashSet<_> = used
        .iter()
        .filter_map(|id| materials.get(*id))
        .flat_map(material_maps)
        .map(Handle::id)
        .collect();
    for id in original_images.difference(&retained) {
        images.remove(*id);
    }
    retained
        .into_iter()
        .filter_map(|id| images.get(id))
        .filter_map(|image| image.data.as_ref())
        .map(Vec::len)
        .sum()
}

fn standard_material<'a>(
    compiled: &CompiledBuildingPrototype,
    slot: MaterialSlot,
    materials: &'a Assets<StandardMaterial>,
) -> Option<&'a StandardMaterial> {
    let part = compiled
        .source
        .lods
        .iter()
        .zip(&compiled.lods)
        .find_map(|(sources, parts)| {
            sources
                .parts
                .iter()
                .zip(parts)
                .find_map(|(s, p)| (s.material == slot).then_some(p))
        })
        .or_else(|| match slot {
            MaterialSlot::Door => compiled.doors.first(),
            MaterialSlot::Hardware => compiled.door_hardware.first(),
            _ => None,
        })?;
    materials.get(&part.material)
}

fn array_image(size: u32, textures: &[&MaterialTexture], kind: TextureKind) -> Image {
    let mut layers = textures.iter().map(|texture| {
        let data = match kind {
            TextureKind::Srgb => &texture.albedo,
            TextureKind::Normal => &texture.normal,
            TextureKind::Linear => &texture.masks,
        };
        texture_image(size, data, kind, true)
    });
    let mut image = layers.next().expect("nonempty texture bank");
    for layer in layers {
        image
            .data
            .as_mut()
            .expect("CPU texture")
            .extend(layer.data.expect("CPU layer"));
    }
    image.texture_descriptor.size.depth_or_array_layers =
        u32::try_from(textures.len()).expect("small material bank");
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    image
}

fn merge_parts(
    parts: &[CompiledBuildingPart],
    slots: impl IntoIterator<Item = MaterialSlot>,
    banks: &[Bank],
    meshes: &mut Assets<Mesh>,
) -> Vec<CompiledBuildingPart> {
    let mut groups: BTreeMap<(usize, bool), Vec<(&CompiledBuildingPart, usize)>> = BTreeMap::new();
    let mut fallback = Vec::new();
    let mut preferred = None;
    for (part, slot) in parts.iter().zip(slots) {
        let preferred_row =
            preferred.and_then(|i: usize| banks[i].slots.get(&slot).map(|&row| (i, row)));
        let bank = preferred_row.or_else(|| {
            banks
                .iter()
                .enumerate()
                .find_map(|(i, bank)| bank.slots.get(&slot).map(|&row| (i, row)))
        });
        if part.triangles == 0 {
            continue;
        }
        let tangents = meshes
            .get(&part.mesh)
            .is_some_and(|mesh| mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
        if let Some((bank, row)) =
            bank.filter(|(bank, row)| tangents || !banks[*bank].textured_rows.contains(row))
        {
            preferred.get_or_insert(bank);
            groups
                .entry((bank, part.stairs))
                .or_default()
                .push((part, row));
        } else {
            fallback.push(part.clone());
        }
    }
    for ((bank, stairs), group) in groups {
        let mut merged = MergedMesh::default();
        for (part, row) in &group {
            append_mesh(
                &mut merged,
                meshes.get(&part.mesh).expect("fresh source mesh"),
                *row,
            );
        }
        let triangles = merged.indices.len() / 3;
        let mesh = merged.finish();
        fallback.push(CompiledBuildingPart {
            mesh: meshes.add(mesh),
            material: group[0].0.material.clone(),
            grub_material: None,
            array_material: Some(banks[bank].material.clone()),
            triangles,
            glass: false,
            stairs,
        });
    }
    fallback
}

#[derive(Default)]
struct MergedMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    heights: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl MergedMesh {
    fn finish(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.heights);
        // Copy the original tangent frames rather than welding across material boundaries.
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, self.tangents);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

fn append_mesh(target: &mut MergedMesh, mesh: &Mesh, row: usize) {
    let offset = u32::try_from(target.positions.len()).expect("building vertex count fits u32");
    let points = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(VertexAttributeValues::as_float3)
        .expect("positions");
    let normals = mesh
        .attribute(Mesh::ATTRIBUTE_NORMAL)
        .and_then(VertexAttributeValues::as_float3)
        .expect("normals");
    let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("UV0")
    else {
        unreachable!("UV0 vec2")
    };
    let VertexAttributeValues::Float32x2(floor) =
        mesh.attribute(Mesh::ATTRIBUTE_UV_1).expect("UV1")
    else {
        unreachable!("UV1 vec2")
    };
    target.positions.extend_from_slice(points);
    target.normals.extend_from_slice(normals);
    target.uvs.extend_from_slice(uvs);
    if let Some(VertexAttributeValues::Float32x4(tangents)) =
        mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
    {
        target.tangents.extend_from_slice(tangents);
    } else {
        // Only untextured rows can reach this branch; their shaders use the
        // geometric normal and ignore these placeholder tangent frames.
        target
            .tangents
            .extend(std::iter::repeat_n([1., 0., 0., 1.], points.len()));
    }
    let row = f32::from(u16::try_from(row).expect("bounded material row"));
    target
        .heights
        .extend(floor.iter().map(|height| [height[0], row]));
    match mesh.indices().expect("indexed building mesh") {
        Indices::U16(indices) => target
            .indices
            .extend(indices.iter().map(|&i| offset + u32::from(i))),
        Indices::U32(indices) => target.indices.extend(indices.iter().map(|&i| offset + i)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::{TextureDataOrder, TextureFormat};

    fn texture(slot: MaterialSlot, value: u8) -> MaterialTexture {
        MaterialTexture {
            slot,
            size: 2,
            albedo: [value, value, value, 255].repeat(4),
            normal: [128, 128, 255, 255].repeat(4),
            masks: [255, 128, 0, 255].repeat(4),
            physical_size: [2., 2.],
            source_hash: String::new(),
        }
    }

    #[test]
    fn banks_keep_layer_major_linear_filtered_mips_and_native_resolution() {
        let a = texture(MaterialSlot::Wall, 20);
        let b = texture(MaterialSlot::Roof, 220);
        let image = array_image(2, &[&a, &b], TextureKind::Srgb);
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8UnormSrgb
        );
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 2);
        assert_eq!(image.texture_descriptor.mip_level_count, 2);
        assert_eq!(image.data_order, TextureDataOrder::LayerMajor);
        assert_eq!(
            image.texture_view_descriptor.unwrap().dimension,
            Some(TextureViewDimension::D2Array)
        );
        let pixels = image.data.unwrap();
        assert_eq!(&pixels[16..20], &[20, 20, 20, 255]);
        assert_eq!(&pixels[20..24], &[220, 220, 220, 255]);
    }

    fn small_two_bank_source() -> Arc<BuildingPrototype> {
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.roof.covering = crate::RoofCovering::Thatch;
        recipe.grub_filter.enabled = true;
        recipe.chimney = None;
        let mut source = crate::generate(&recipe).unwrap();
        for image in &mut source.materials {
            let slot = image.slot;
            *image = texture(slot, 100);
        }
        source.materials.push(texture(MaterialSlot::Door, 100));
        // A second resolution stays in its own bank.
        for end in source
            .materials
            .iter_mut()
            .filter(|t| matches!(t.slot, MaterialSlot::ThatchEnd | MaterialSlot::Door))
        {
            end.size = 4;
            end.albedo = [120, 120, 120, 255].repeat(16);
            end.normal = [128, 128, 255, 255].repeat(16);
            end.masks = [255, 128, 0, 255].repeat(16);
        }
        // A valid generated map with no matching draw must not occupy a live
        // array layer merely because export retains its CPU pixels.
        source.materials.push(texture(MaterialSlot::Chimney, 222));
        Arc::new(source)
    }

    #[test]
    fn gpu_batching_preserves_geometry_visibility_weathering_and_moving_leaf_heights() {
        let source = small_two_bank_source();
        let mut meshes = Assets::default();
        let mut materials = Assets::default();
        let mut grub = Assets::default();
        let mut arrays = Assets::default();
        let mut images = Assets::default();
        let compiled = compile_building_with_arrays(
            source.clone(),
            &mut meshes,
            &mut materials,
            &mut grub,
            &mut arrays,
            &mut images,
        );
        assert!(Arc::ptr_eq(&source, &compiled.source));
        assert_eq!(arrays.len(), 2);
        assert_geometry_and_visibility(&compiled, &materials);
        assert_far_atlas_preserved(&compiled, &meshes, &materials, &images);
        assert_moving_leaf_heights(&compiled, &meshes);
        assert_material_rows(&compiled.source, &arrays, &images);
        assert_constant_triangle_ids(&compiled, &meshes);
    }

    fn assert_geometry_and_visibility(
        compiled: &CompiledBuildingPrototype,
        materials: &Assets<StandardMaterial>,
    ) {
        let source = &compiled.source;
        for level in 0..3 {
            assert_eq!(
                source.lods[level].triangle_count(),
                compiled.lods[level]
                    .iter()
                    .map(|p| p.triangles)
                    .sum::<usize>()
            );
            assert_eq!(
                source.lods[level]
                    .parts
                    .iter()
                    .filter(|p| p.material.is_stairs())
                    .map(|p| p.mesh.triangle_count())
                    .sum::<usize>(),
                compiled.lods[level]
                    .iter()
                    .filter(|p| p.stairs)
                    .map(|p| p.triangles)
                    .sum::<usize>()
            );
        }
        assert!(compiled.lods[0].len() < source.lods[0].parts.len());
        for slot in MaterialSlot::THATCH_SHELLS {
            let source = source.lods[0]
                .parts
                .iter()
                .find(|p| p.material == slot)
                .unwrap();
            assert!(compiled.lods[0].iter().any(|p| p.array_material.is_none()
                && p.triangles == source.mesh.triangle_count()
                && materials.get(&p.material).unwrap().alpha_mode == AlphaMode::Mask(0.5)));
        }
    }

    fn assert_moving_leaf_heights(compiled: &CompiledBuildingPrototype, meshes: &Assets<Mesh>) {
        let source = &compiled.source;
        for (descriptor, leaf) in source.doors.iter().zip(&compiled.doors) {
            assert!(leaf.array_material.is_some());
            assert_eq!(
                leaf.triangles,
                descriptor.mesh.triangle_count() + descriptor.hardware_mesh.triangle_count()
            );
            let VertexAttributeValues::Float32x2(heights) = meshes
                .get(&leaf.mesh)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_UV_1)
                .unwrap()
            else {
                panic!("floor heights")
            };
            assert!(
                heights
                    .iter()
                    .take(descriptor.mesh.positions.len())
                    .zip(&descriptor.mesh.positions)
                    .all(|(uv, p)| (uv[0] - p[2] - descriptor.hinge[2]).abs() < 0.00001)
            );
        }
    }

    fn assert_far_atlas_preserved(
        compiled: &CompiledBuildingPrototype,
        meshes: &Assets<Mesh>,
        materials: &Assets<StandardMaterial>,
        images: &Assets<Image>,
    ) {
        let source = compiled.source.lods[2]
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Atlas)
            .unwrap();
        let atlas = compiled.lods[2]
            .iter()
            .find(|p| p.array_material.is_none() && !p.stairs)
            .unwrap();
        assert!(atlas.grub_material.is_none());
        assert_eq!(atlas.triangles, source.mesh.triangle_count());
        let mesh = meshes.get(&atlas.mesh).unwrap();
        let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
        else {
            panic!("atlas UV0")
        };
        assert_eq!(uvs, &source.mesh.uvs);
        let material = materials.get(&atlas.material).unwrap();
        let image = images
            .get(material.base_color_texture.as_ref().unwrap())
            .unwrap();
        assert_eq!(
            image.texture_descriptor.size.width,
            compiled.source.atlas.size
        );
        let bevy::image::ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("atlas sampler")
        };
        assert_eq!(
            sampler.address_mode_u,
            bevy::image::ImageAddressMode::ClampToEdge
        );
        assert!(
            compiled.lods[2]
                .iter()
                .any(|p| p.stairs && p.array_material.is_some())
        );
        assert!(
            compiled.lods[2]
                .iter()
                .any(|p| !p.stairs && p.array_material.is_some())
        );
    }

    fn assert_material_rows(
        source: &BuildingPrototype,
        arrays: &Assets<BuildingArrayMaterial>,
        images: &Assets<Image>,
    ) {
        let bank = arrays
            .iter()
            .find(|(_, m)| {
                images
                    .get(&m.extension.albedo)
                    .unwrap()
                    .texture_descriptor
                    .size
                    .width
                    == 2
            })
            .unwrap()
            .1;
        let used = used_texture_slots(source);
        assert!(!used.contains(&MaterialSlot::Chimney));
        let textures: Vec<_> = source
            .materials
            .iter()
            .filter(|t| {
                t.size == 2 && t.slot.thatch_shell_layer().is_none() && used.contains(&t.slot)
            })
            .collect();
        let layers = images
            .get(&bank.extension.albedo)
            .unwrap()
            .texture_descriptor
            .size
            .depth_or_array_layers;
        assert_eq!(usize::try_from(layers).unwrap(), textures.len());
        let rows = bank_slots(source, &textures, true);
        for (&slot, &row) in &rows {
            let exterior = crate::grub::exterior_material(&source.recipe, slot);
            assert_eq!(
                bank.extension.rows.mapping[row].w > 0.5,
                exterior,
                "{slot:?}"
            );
        }
        let hardware = rows[&MaterialSlot::Hardware];
        assert!(bank.extension.rows.surface[hardware].z < 0.);
        assert_eq!(
            bank.extension.rows.surface[hardware].y.to_bits(),
            0.9_f32.to_bits()
        );
        assert_eq!(
            bank.extension.rows.surface[hardware].x.to_bits(),
            0.24_f32.to_bits()
        );
    }

    fn assert_constant_triangle_ids(compiled: &CompiledBuildingPrototype, meshes: &Assets<Mesh>) {
        for part in compiled
            .lods
            .iter()
            .flatten()
            .filter(|p| p.array_material.is_some())
        {
            let mesh = meshes.get(&part.mesh).unwrap();
            assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
            let VertexAttributeValues::Float32x2(uv) =
                mesh.attribute(Mesh::ATTRIBUTE_UV_1).unwrap()
            else {
                panic!("material rows")
            };
            let Indices::U32(indices) = mesh.indices().unwrap() else {
                panic!("u32")
            };
            assert!(
                indices
                    .chunks_exact(3)
                    .all(
                        |t| uv[t[0] as usize][1].to_bits() == uv[t[1] as usize][1].to_bits()
                            && uv[t[0] as usize][1].to_bits() == uv[t[2] as usize][1].to_bits()
                    )
            );
        }
    }
}
