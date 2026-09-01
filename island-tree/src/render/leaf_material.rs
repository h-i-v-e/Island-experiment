//! Bevy PBR extension for optically distinct broadleaf undersides.

use bevy::{
    asset::{Asset, Handle, load_internal_asset, uuid_handle},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::{App, Plugin, Reflect, StandardMaterial},
    render::render_resource::AsBindGroup,
    shader::{Shader, ShaderRef},
};

const LEAF_MATERIAL_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("6ad69b13-6fbf-4f0e-a65c-8186c401e087");

pub type LeafMaterial = ExtendedMaterial<StandardMaterial, LeafMaterialExtension>;

/// Marker extension whose embedded shader distinguishes the two optical faces
/// of a leaf while retaining Bevy's standard PBR and shadow paths.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct LeafMaterialExtension {}

impl MaterialExtension for LeafMaterialExtension {
    fn fragment_shader() -> ShaderRef {
        LEAF_MATERIAL_SHADER_HANDLE.clone().into()
    }
}

/// Installs the embedded leaf shader and its extended-material pipeline.
#[derive(Debug, Default)]
pub struct LeafMaterialPlugin;

impl Plugin for LeafMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            LEAF_MATERIAL_SHADER_HANDLE,
            "shaders/leaf_material.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<LeafMaterial>::default());
    }
}
