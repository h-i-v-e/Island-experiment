//! Bevy-side compilation of one generated tree into shared static render assets.
//!
//! The botanical model describes individual organs because that is the useful
//! authoring and review representation. An island cannot afford one entity per
//! leaf per tree, however. This compiler folds all wood into one bark draw and
//! all leaves into one transmitted-foliage draw. Many placed trees then share
//! the same two mesh and material handles, so Bevy can instance them normally.

use bevy::{
    asset::{Asset, Assets, Handle, RenderAssetUsages},
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, PrimitiveTopology},
    pbr::StandardMaterial,
    prelude::{Color, Image, Mat3, Mesh, Quat, Transform, Vec3},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use motu::Mesh as MotuMesh;

use super::{
    BarkMaterial, BarkVertex, BotanicalPrototype, BotanicalRecipe, BotanicalTexture, FoliagePad,
    LeafMaterial, LeafOrgan, bark_material::BarkMaterialExtension, generate_botanical_prototype,
    leaf_material::LeafMaterialExtension,
};

const LEAF_ARCHETYPE_TINTS: [[f32; 3]; 8] = [
    [1.00, 1.00, 1.00],
    [0.96, 1.00, 0.94],
    [1.00, 0.97, 0.88],
    [0.92, 0.94, 0.84],
    [1.00, 1.00, 1.00],
    [0.96, 1.00, 0.94],
    [1.00, 0.97, 0.88],
    [0.92, 0.94, 0.84],
];

/// One material-homogeneous draw shared by every placement of a prototype.
#[derive(Clone, Debug)]
pub struct CompiledTreePart<M: Asset> {
    pub mesh: Handle<Mesh>,
    pub material: Handle<M>,
    pub vertices: u32,
}

/// The two instanced draws that represent a full near tree on the island.
#[derive(Clone, Debug)]
pub struct CompiledTreePrototype {
    pub wood: CompiledTreePart<BarkMaterial>,
    pub foliage: CompiledTreePart<LeafMaterial>,
}

/// Generates and compiles one deterministic static tree for repeated placement.
///
/// The returned handles own no per-instance state. Placement, culling and LOD
/// remain the caller's responsibility, while this module owns the conversion
/// from botanical organs into the two renderer assets they share.
///
/// # Errors
///
/// Returns the generator's recipe error or a mesh-compilation error if organ
/// counts exceed the renderer's supported index range.
pub fn compile_static_prototype(
    seed: u64,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    bark_materials: &mut Assets<BarkMaterial>,
    leaf_materials: &mut Assets<LeafMaterial>,
) -> Result<CompiledTreePrototype, String> {
    compile_static_prototype_with_recipe(
        seed,
        BotanicalRecipe::default(),
        meshes,
        images,
        bark_materials,
        leaf_materials,
    )
}

/// Compiles a caller-selected bounded recipe through the same static path.
/// Island runtime can reduce organ density without inventing a second model.
///
/// # Errors
///
/// Returns the generator's recipe error or a mesh-compilation error if organ
/// counts exceed the renderer's supported index range.
pub fn compile_static_prototype_with_recipe(
    seed: u64,
    recipe: BotanicalRecipe,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    bark_materials: &mut Assets<BarkMaterial>,
    leaf_materials: &mut Assets<LeafMaterial>,
) -> Result<CompiledTreePrototype, String> {
    compile_static_prototype_at_lod(
        seed,
        recipe,
        StaticFoliage::Leaves,
        meshes,
        images,
        bark_materials,
        leaf_materials,
    )
}

/// Compiles the generated foliage-pad representation for landscape placement.
/// It preserves the grown crown envelope and shared leaf optics without
/// carrying every individually modelled blade into the island renderer.
///
/// # Errors
///
/// Returns the generator's recipe error or a mesh-compilation error if organ
/// counts exceed the renderer's supported index range.
pub fn compile_static_middle_prototype_with_recipe(
    seed: u64,
    recipe: BotanicalRecipe,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    bark_materials: &mut Assets<BarkMaterial>,
    leaf_materials: &mut Assets<LeafMaterial>,
) -> Result<CompiledTreePrototype, String> {
    compile_static_prototype_at_lod(
        seed,
        recipe,
        StaticFoliage::Pads,
        meshes,
        images,
        bark_materials,
        leaf_materials,
    )
}

#[derive(Clone, Copy)]
enum StaticFoliage {
    Leaves,
    Pads,
}

fn compile_static_prototype_at_lod(
    seed: u64,
    recipe: BotanicalRecipe,
    foliage_lod: StaticFoliage,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    bark_materials: &mut Assets<BarkMaterial>,
    leaf_materials: &mut Assets<LeafMaterial>,
) -> Result<CompiledTreePrototype, String> {
    let prototype = generate_botanical_prototype(seed, recipe)?;
    let BotanicalPrototype {
        mut wood,
        mut wood_bark,
        microtwigs,
        microtwig_bark,
        leaf_archetypes,
        leaves,
        foliage_pad_archetypes,
        foliage_pads,
        bark_albedo,
        bark_normal,
        bark_metallic_roughness,
        leaf_albedo,
        leaf_metallic_roughness,
        ..
    } = prototype;

    merge_wood(&mut wood, &mut wood_bark, microtwigs, microtwig_bark)?;
    let wood = bevy_wood_mesh(&wood, &wood_bark);
    let foliage = match foliage_lod {
        StaticFoliage::Leaves => compiled_leaf_mesh(&leaf_archetypes, &leaves)?,
        StaticFoliage::Pads => compiled_pad_mesh(&foliage_pad_archetypes, &foliage_pads)?,
    };
    let wood_vertices = vertex_count(&wood);
    let foliage_vertices = vertex_count(&foliage);

    let bark_albedo = images.add(texture_image(bark_albedo, true, true));
    let bark_normal = images.add(texture_image(bark_normal, true, false));
    let bark_metallic_roughness = images.add(texture_image(bark_metallic_roughness, true, false));
    let bark_material = bark_materials.add(BarkMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(bark_albedo),
            normal_map_texture: Some(bark_normal),
            metallic_roughness_texture: Some(bark_metallic_roughness),
            perceptual_roughness: 1.0,
            reflectance: 0.28,
            ..Default::default()
        },
        extension: BarkMaterialExtension::default(),
    });

    let leaf_albedo = images.add(texture_image(leaf_albedo, false, true));
    let leaf_metallic_roughness = images.add(texture_image(leaf_metallic_roughness, false, false));
    let leaf_material = leaf_materials.add(LeafMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.96, 0.97, 0.94),
            base_color_texture: Some(leaf_albedo),
            metallic_roughness_texture: Some(leaf_metallic_roughness),
            perceptual_roughness: 1.0,
            reflectance: 0.39,
            diffuse_transmission: 0.47,
            thickness: 0.000_40,
            attenuation_distance: 0.012,
            attenuation_color: Color::srgb(0.30, 0.55, 0.20),
            ior: 1.42,
            clearcoat: 0.30,
            clearcoat_perceptual_roughness: 0.50,
            double_sided: true,
            cull_mode: None,
            ..Default::default()
        },
        extension: LeafMaterialExtension::default(),
    });

    Ok(CompiledTreePrototype {
        wood: CompiledTreePart {
            mesh: meshes.add(wood),
            material: bark_material,
            vertices: wood_vertices,
        },
        foliage: CompiledTreePart {
            mesh: meshes.add(foliage),
            material: leaf_material,
            vertices: foliage_vertices,
        },
    })
}

fn merge_wood(
    wood: &mut MotuMesh,
    bark: &mut Vec<BarkVertex>,
    microtwigs: MotuMesh,
    microtwig_bark: Vec<BarkVertex>,
) -> Result<(), String> {
    if microtwigs.vertices.len() != microtwig_bark.len() {
        return Err(String::from("microtwig bark state does not match its mesh"));
    }
    let vertex_offset = u32::try_from(wood.vertices.len())
        .map_err(|_| String::from("tree mesh exceeds the supported vertex index range"))?;
    wood.vertices.extend(microtwigs.vertices);
    wood.normals.extend(microtwigs.normals);
    wood.uv.extend(microtwigs.uv);
    wood.triangles.extend(
        microtwigs
            .triangles
            .into_iter()
            .map(|index| index + vertex_offset),
    );
    bark.extend(microtwig_bark);
    Ok(())
}

fn compiled_leaf_mesh(archetypes: &[MotuMesh; 8], leaves: &[LeafOrgan]) -> Result<Mesh, String> {
    let vertices = leaves.iter().try_fold(0_usize, |count, leaf| {
        count
            .checked_add(archetypes[usize::from(leaf.archetype)].vertices.len())
            .ok_or_else(|| String::from("compiled foliage vertex count overflowed"))
    })?;
    let indices = leaves.iter().try_fold(0_usize, |count, leaf| {
        count
            .checked_add(archetypes[usize::from(leaf.archetype)].triangles.len())
            .ok_or_else(|| String::from("compiled foliage index count overflowed"))
    })?;
    let mut positions = Vec::with_capacity(vertices);
    let mut normals = Vec::with_capacity(vertices);
    let mut uv = Vec::with_capacity(vertices);
    let mut colours = Vec::with_capacity(vertices);
    let mut triangles = Vec::with_capacity(indices);

    for leaf in leaves {
        let archetype = &archetypes[usize::from(leaf.archetype)];
        let transform = leaf_transform(*leaf);
        let index_offset = u32::try_from(positions.len())
            .map_err(|_| String::from("compiled foliage exceeds the supported index range"))?;
        let tint = leaf_tint(*leaf);
        positions.extend(archetype.vertices.iter().map(|vertex| {
            let local = Vec3::new(vertex.x, vertex.z, vertex.y);
            transform.transform_point(local).to_array()
        }));
        normals.extend(archetype.normals.iter().map(|normal| {
            let local = Vec3::new(normal.x, normal.z, normal.y);
            transform_normal(transform, local).to_array()
        }));
        uv.extend(archetype.uv.iter().map(|point| [point.x, point.y]));
        colours.extend(std::iter::repeat_n(tint, archetype.vertices.len()));
        triangles.extend(
            archetype
                .triangles
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|triangle| {
                    [
                        triangle[0] + index_offset,
                        triangle[2] + index_offset,
                        triangle[1] + index_offset,
                    ]
                }),
        );
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    mesh.insert_indices(Indices::U32(triangles));
    mesh.generate_tangents()
        .map_err(|error| format!("could not generate foliage tangents: {error}"))?;
    Ok(mesh)
}

fn compiled_pad_mesh(archetypes: &[MotuMesh; 2], pads: &[FoliagePad]) -> Result<Mesh, String> {
    let vertices = pads.iter().try_fold(0_usize, |count, pad| {
        count
            .checked_add(archetypes[usize::from(pad.archetype)].vertices.len())
            .ok_or_else(|| String::from("compiled foliage-pad vertex count overflowed"))
    })?;
    let indices = pads.iter().try_fold(0_usize, |count, pad| {
        count
            .checked_add(archetypes[usize::from(pad.archetype)].triangles.len())
            .ok_or_else(|| String::from("compiled foliage-pad index count overflowed"))
    })?;
    let mut positions = Vec::with_capacity(vertices);
    let mut normals = Vec::with_capacity(vertices);
    let mut uv = Vec::with_capacity(vertices);
    let mut colours = Vec::with_capacity(vertices);
    let mut triangles = Vec::with_capacity(indices);

    for pad in pads {
        let archetype = &archetypes[usize::from(pad.archetype)];
        let transform = pad_transform(*pad);
        let index_offset = u32::try_from(positions.len())
            .map_err(|_| String::from("compiled foliage pads exceed the supported index range"))?;
        let tint = pad_tint(*pad);
        positions.extend(archetype.vertices.iter().map(|vertex| {
            let local = Vec3::new(vertex.x, vertex.z, vertex.y);
            transform.transform_point(local).to_array()
        }));
        normals.extend(archetype.normals.iter().map(|normal| {
            let local = Vec3::new(normal.x, normal.z, normal.y);
            transform_normal(transform, local).to_array()
        }));
        uv.extend(archetype.uv.iter().map(|point| [point.x, point.y]));
        colours.extend(std::iter::repeat_n(tint, archetype.vertices.len()));
        triangles.extend(
            archetype
                .triangles
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|triangle| {
                    [
                        triangle[0] + index_offset,
                        triangle[2] + index_offset,
                        triangle[1] + index_offset,
                    ]
                }),
        );
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    mesh.insert_indices(Indices::U32(triangles));
    mesh.generate_tangents()
        .map_err(|error| format!("could not generate foliage-pad tangents: {error}"))?;
    Ok(mesh)
}

fn leaf_transform(leaf: LeafOrgan) -> Transform {
    let direction = convert(leaf.direction).normalize_or(Vec3::X);
    let normal = convert(leaf.normal).normalize_or(Vec3::Y);
    let transverse = normal.cross(direction).normalize_or(Vec3::Z);
    Transform {
        translation: convert(leaf.blade_base_metres),
        rotation: Quat::from_mat3(&Mat3::from_cols(direction, transverse, normal)),
        scale: Vec3::new(leaf.length_metres, leaf.width_metres, leaf.length_metres),
    }
}

fn pad_transform(pad: FoliagePad) -> Transform {
    let direction = convert(pad.direction).normalize_or(Vec3::X);
    let normal = convert(pad.normal).normalize_or(Vec3::Y);
    let transverse = direction.cross(normal).normalize_or(Vec3::Z);
    let extents = Vec3::new(
        pad.half_extents_metres.x,
        pad.half_extents_metres.y,
        pad.half_extents_metres.z,
    );
    Transform {
        translation: convert(pad.centre_metres),
        rotation: Quat::from_mat3(&Mat3::from_cols(direction, normal, transverse)),
        scale: Vec3::new(
            extents.x.max(0.35),
            extents.y.max(0.24),
            extents.z.max(0.30),
        ),
    }
}

fn transform_normal(transform: Transform, normal: Vec3) -> Vec3 {
    (transform.rotation * (normal / transform.scale)).normalize_or(Vec3::Y)
}

fn leaf_tint(leaf: LeafOrgan) -> [f32; 4] {
    let archetype = LEAF_ARCHETYPE_TINTS[usize::from(leaf.archetype)];
    let exposure = leaf.light_exposure.clamp(0.0, 1.0);
    let exposure_tint = Vec3::new(
        0.90 + exposure * 0.10,
        0.94 + exposure * 0.03,
        1.00 - exposure * 0.13,
    );
    let variation = (leaf.variation / std::f32::consts::TAU).rem_euclid(1.0);
    let pigment = if variation < 0.26 {
        Vec3::new(0.88, 0.93, 0.90)
    } else if variation > 0.74 && leaf.age > 0.42 {
        Vec3::new(1.00, 0.94, 0.84)
    } else {
        Vec3::ONE
    };
    let tint = Vec3::from_array(archetype) * exposure_tint * pigment;
    [tint.x, tint.y, tint.z, 1.0]
}

fn pad_tint(pad: FoliagePad) -> [f32; 4] {
    let archetype = if pad.archetype == 0 {
        Vec3::new(0.92, 1.00, 0.87)
    } else {
        Vec3::new(0.82, 0.94, 0.77)
    };
    let exposure = pad.light_exposure.clamp(0.0, 1.0);
    let exposure_tint = Vec3::new(
        0.90 + exposure * 0.10,
        0.94 + exposure * 0.03,
        1.00 - exposure * 0.13,
    );
    let variation = (pad.variation / std::f32::consts::TAU).rem_euclid(1.0);
    let pigment = if variation < 0.26 {
        Vec3::new(0.88, 0.93, 0.90)
    } else if variation > 0.74 && pad.mean_age > 0.42 {
        Vec3::new(1.00, 0.94, 0.84)
    } else {
        Vec3::ONE
    };
    let density = 0.90 + pad.density.clamp(0.0, 1.0) * 0.10;
    let tint = archetype * exposure_tint * pigment * density;
    [tint.x, tint.y, tint.z, 1.0]
}

fn bevy_wood_mesh(source: &MotuMesh, bark: &[BarkVertex]) -> Mesh {
    assert_eq!(source.vertices.len(), bark.len());
    let positions: Vec<[f32; 3]> = source
        .vertices
        .iter()
        .map(|vertex| [vertex.x, vertex.z, vertex.y])
        .collect();
    let normals: Vec<[f32; 3]> = source
        .normals
        .iter()
        .map(|normal| [normal.x, normal.z, normal.y])
        .collect();
    let uv: Vec<[f32; 2]> = source.uv.iter().map(|point| [point.x, point.y]).collect();
    let indices = source
        .triangles
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|triangle| [triangle[0], triangle[2], triangle[1]])
        .collect();
    let colours: Vec<[f32; 4]> = bark.iter().copied().map(bark_vertex_colour).collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    mesh.insert_indices(Indices::U32(indices));
    mesh.generate_tangents()
        .expect("generated tree wood has valid positions, normals, and UVs");
    mesh
}

fn bark_vertex_colour(vertex: BarkVertex) -> [f32; 4] {
    let maturity = smoothstep(vertex.maturity);
    let colour = Vec3::new(1.03, 1.04, 1.01).lerp(Vec3::new(0.99, 0.98, 0.95), maturity);
    [colour.x, colour.y, colour.z, vertex.maturity]
}

fn texture_image(texture: BotanicalTexture, repeat: bool, srgb: bool) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        texture.rgba,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        },
        address_mode_v: if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        },
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..Default::default()
    });
    image
}

fn vertex_count(mesh: &Mesh) -> u32 {
    u32::try_from(mesh.count_vertices()).unwrap_or(u32::MAX)
}

fn convert(vector: motu::Vec3) -> Vec3 {
    Vec3::new(vector.x, vector.z, vector.y)
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::{BotanicalRecipe, compiled_leaf_mesh, generate_botanical_prototype};

    #[test]
    fn static_foliage_compiles_every_leaf_into_one_mesh() {
        let prototype = generate_botanical_prototype(42, BotanicalRecipe::default())
            .expect("the reference tree should generate");
        let expected = prototype
            .leaves
            .iter()
            .map(|leaf| {
                prototype.leaf_archetypes[usize::from(leaf.archetype)]
                    .vertices
                    .len()
            })
            .sum::<usize>();
        let mesh = compiled_leaf_mesh(&prototype.leaf_archetypes, &prototype.leaves)
            .expect("the reference tree foliage should compile");
        assert_eq!(mesh.count_vertices(), expected);
    }
}
