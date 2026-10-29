use crate::terrain_material::TerrainTextures;
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::{
        render_resource::{AsBindGroup, ShaderType},
        storage::ShaderBuffer,
    },
    shader::{ShaderRef, load_shader_library},
};

pub(super) fn register(app: &mut App) {
    load_shader_library!(app, "common.wgsl");
    embedded_asset!(app, "surface.wgsl");
    embedded_asset!(app, "prepass.wgsl");
    app.add_plugins(MaterialPlugin::<ForestMaterial>::default());
}
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct ForestUniform {
    /// Physical radius, wood/fine/shell/coarse kind, shell range, reserved.
    pub shape: Vec4,
}
#[derive(Asset, AsBindGroup, Clone, Debug, Default, Reflect)]
pub struct ForestExtension {
    #[uniform(100)]
    pub settings: ForestUniform,
    #[texture(110, dimension = "3d")]
    #[sampler(111)]
    pub noise: Option<Handle<Image>>,
    #[storage(123, read_only)]
    pub environment: Handle<ShaderBuffer>,
    #[texture(126, dimension = "cube")]
    #[sampler(102)]
    pub environment_sky: Option<Handle<Image>>,
    #[texture(127, dimension = "cube")]
    pub environment_shadows: Option<Handle<Image>>,
    #[texture(129, dimension = "cube_array")]
    pub global_weather: Option<Handle<Image>>,
}
pub type ForestMaterial = ExtendedMaterial<StandardMaterial, ForestExtension>;
impl MaterialExtension for ForestExtension {
    fn vertex_shader() -> ShaderRef {
        "embedded://planet_bevy/forest/surface.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "embedded://planet_bevy/forest/prepass.wgsl".into()
    }
    fn prepass_fragment_shader() -> ShaderRef {
        Self::prepass_vertex_shader()
    }
}
pub(super) fn create_materials(
    radius: f32,
    textures: &TerrainTextures,
    materials: &mut Assets<ForestMaterial>,
) -> [Handle<ForestMaterial>; 4] {
    std::array::from_fn(|kind| {
        materials.add(ForestMaterial {
            base: StandardMaterial {
                perceptual_roughness: 0.94,
                alpha_mode: if matches!(kind, 1 | 2) {
                    AlphaMode::Mask(0.5)
                } else {
                    AlphaMode::Opaque
                },
                double_sided: kind != 0,
                cull_mode: (kind == 0).then_some(bevy::render::render_resource::Face::Back),
                ..default()
            },
            extension: ForestExtension {
                settings: ForestUniform {
                    shape: Vec4::new(radius, f32::from(u8::try_from(kind).unwrap()), 28.0, 0.0),
                },
                noise: Some(textures.noise.clone()),
                environment: textures.environment.clone(),
                environment_sky: Some(textures.environment_sky.clone()),
                environment_shadows: Some(textures.environment_shadows.clone()),
                global_weather: Some(textures.global_weather.clone()),
            },
        })
    })
}

pub(super) struct Prototype {
    pub wood: Handle<Mesh>,
    pub branches: Handle<Mesh>,
    pub leaves: Handle<Mesh>,
    pub shells: Handle<Mesh>,
    pub canopy: Handle<Mesh>,
}
impl Prototype {
    pub fn remove(self, meshes: &mut Assets<Mesh>) {
        for handle in [
            self.wood,
            self.branches,
            self.leaves,
            self.shells,
            self.canopy,
        ] {
            meshes.remove(handle.id());
        }
    }
}
pub(super) fn upload(source: motu::TreeMeshes, meshes: &mut Assets<Mesh>) -> Prototype {
    info!(
        "Tree prototype: {} fine wood / {} branch / {} fine foliage / {} coarse foliage triangles",
        source.lod0_wood.triangles.len() / 3,
        source.lod1_wood.triangles.len() / 3,
        source.lod0_foliage.triangles.len() / 3,
        source.lod1_foliage.triangles.len() / 3
    );
    let shells = mesh(&source.lod1_foliage, true);
    Prototype {
        wood: meshes.add(mesh(&source.lod0_wood, false)),
        branches: meshes.add(mesh(&source.lod1_wood, false)),
        leaves: meshes.add(mesh(&source.lod0_foliage, false)),
        canopy: meshes.add(distant_mesh(&source)),
        shells: meshes.add(shells),
    }
}
fn distant_mesh(source: &motu::TreeMeshes) -> Mesh {
    let mut canopy = mesh(&source.lod1_foliage, false);
    let mut trunk = mesh(&source.lod2_wood, false);
    let Some(VertexAttributeValues::Float32x4(colours)) =
        trunk.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    else {
        unreachable!("tree meshes always carry float vertex colours");
    };
    // G distinguishes bark from foliage within one distant-tree draw. R keeps
    // its existing shell-layer meaning; detailed wood uses its material kind.
    for colour in colours {
        colour[1] = 0.0;
    }
    canopy
        .merge(&trunk)
        .expect("tree parts share their topology and vertex attributes");
    canopy
}
fn mesh(source: &motu::Mesh, shells: bool) -> Mesh {
    let layers = if shells { 8 } else { 1 };
    let mut positions = Vec::with_capacity(source.vertices.len() * layers);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut colours = Vec::with_capacity(positions.capacity());
    let mut indices = Vec::with_capacity(source.triangles.len() * layers);
    for layer in 0..layers {
        let fraction = if shells {
            f32::from(u8::try_from(layer + 1).unwrap()) / 8.0
        } else {
            0.0
        };
        let offset = u32::try_from(positions.len()).expect("bounded tree prototype");
        for (&point, &normal) in source.vertices.iter().zip(&source.normals) {
            positions
                .push((point + normal * (0.45 * fraction / motu::ISLAND_WORLD_METRES)).to_array());
            normals.push(normal.to_array());
            colours.push([fraction, 1.0, 1.0, 1.0]);
        }
        indices.extend(source.triangles.iter().map(|i| offset + i));
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
    .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distant_tree_keeps_island_trunk_geometry_in_one_shared_mesh() {
        let source = motu::generate_tree(2018);
        let canopy_vertices = source.lod1_foliage.vertices.len();
        let canopy_indices = source.lod1_foliage.triangles.len();
        let trunk_positions: [[f32; 3]; 8] =
            std::array::from_fn(|i| source.lod2_wood.vertices[i].to_array());
        let mut assets = Assets::<Mesh>::default();
        let model = upload(source, &mut assets);
        // Four near meshes and one combined distant mesh; no extra trunk draw.
        assert_eq!(assets.len(), 5);
        let distant = assets.get(&model.canopy).unwrap();
        let positions = distant
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        assert_eq!(positions.len(), canopy_vertices + 8);
        for (actual, expected) in positions[canopy_vertices..].iter().zip(&trunk_positions) {
            assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
        }
        assert_eq!(distant.indices().unwrap().len(), canopy_indices + 36);
        let VertexAttributeValues::Float32x4(colours) =
            distant.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
        else {
            panic!("missing distant-tree material tags");
        };
        assert!(colours[..canopy_vertices].iter().all(|c| c[1] > 0.5));
        assert!(colours[canopy_vertices..].iter().all(|c| c[1] < 0.5));
        model.remove(&mut assets);
        assert!(assets.is_empty(), "regeneration must release shared meshes");
    }
}
