//! View-selecting material for generated multi-angle tree impostors.

use bevy::{
    asset::{Asset, Handle, load_internal_asset, uuid_handle},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::{App, Plugin, Reflect, StandardMaterial},
    render::render_resource::AsBindGroup,
    shader::{Shader, ShaderRef},
};

const IMPOSTOR_MATERIAL_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("b67ca8c7-a380-4fce-a563-7e37476ad903");

pub type ImpostorMaterial = ExtendedMaterial<StandardMaterial, ImpostorMaterialExtension>;

/// Marker extension that selects and dithers the two atlas views nearest to
/// the camera while retaining Bevy's standard alpha-mask and fog path.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct ImpostorMaterialExtension {}

impl MaterialExtension for ImpostorMaterialExtension {
    fn vertex_shader() -> ShaderRef {
        IMPOSTOR_MATERIAL_SHADER_HANDLE.clone().into()
    }

    fn fragment_shader() -> ShaderRef {
        IMPOSTOR_MATERIAL_SHADER_HANDLE.clone().into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// Installs the embedded impostor shader and its extended-material pipeline.
#[derive(Debug, Default)]
pub struct ImpostorMaterialPlugin;

impl Plugin for ImpostorMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            IMPOSTOR_MATERIAL_SHADER_HANDLE,
            "shaders/impostor_material.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<ImpostorMaterial>::default());
    }
}
