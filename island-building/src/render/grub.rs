use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{ShaderRef, load_shader_library},
};

pub type GrubMaterial = ExtendedMaterial<StandardMaterial, GrubExtension>;

pub struct BuildingGrubPlugin;
impl Plugin for BuildingGrubPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/grub.wgsl");
        embedded_asset!(app, "shaders/grub_material.wgsl");
        app.add_plugins(MaterialPlugin::<GrubMaterial>::default());
    }
}

#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct GrubUniform {
    pub strengths: Vec4,
    pub dirt: Vec4,
    pub moss: Vec4,
    pub offset: Vec4,
    pub noise: Vec4,
}
impl GrubUniform {
    pub(super) fn new(filter: &crate::GrubFilter, seed: u64) -> Self {
        let [strengths, dirt, moss, offset, noise] =
            filter.uniform_vectors(seed).map(Vec4::from_array);
        Self {
            strengths,
            dirt,
            moss,
            offset,
            noise,
        }
    }
}

#[derive(Asset, AsBindGroup, Clone, Debug, Default, Reflect)]
pub struct GrubExtension {
    #[uniform(100)]
    pub settings: GrubUniform,
}
impl MaterialExtension for GrubExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://island_building/render/shaders/grub_material.wgsl".into()
    }
}
