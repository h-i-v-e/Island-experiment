//! Bevy PBR extension for maturity-aware procedural bark optics.

use bevy::{
    asset::{Asset, Handle, load_internal_asset, uuid_handle},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::{App, Plugin, Reflect, StandardMaterial},
    render::render_resource::AsBindGroup,
    shader::{Shader, ShaderRef},
};

const BARK_MATERIAL_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("713b9e60-e834-4eaa-b940-3e17abf2a422");

pub type BarkMaterial = ExtendedMaterial<StandardMaterial, BarkMaterialExtension>;

/// Marker extension that interprets the renderer-neutral maturity channel in
/// vertex alpha while retaining Bevy's standard textures and lighting path.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct BarkMaterialExtension {}

impl MaterialExtension for BarkMaterialExtension {
    fn fragment_shader() -> ShaderRef {
        BARK_MATERIAL_SHADER_HANDLE.clone().into()
    }
}

/// Installs the embedded bark shader and its extended-material pipeline.
#[derive(Debug, Default)]
pub struct BarkMaterialPlugin;

impl Plugin for BarkMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            BARK_MATERIAL_SHADER_HANDLE,
            "bark_material.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<BarkMaterial>::default());
    }
}
