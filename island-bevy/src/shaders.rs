//! Embedded shader assets, registered beside the WGSL sources they expose.

use bevy::{app::App, asset::embedded_asset, shader::load_shader_library};

pub(crate) fn load_surface(app: &mut App) {
    load_shader_library!(app, "shaders/noise.wgsl");
    load_shader_library!(app, "shaders/debug.wgsl");
    embedded_asset!(app, "shaders/terrain.wgsl");
    embedded_asset!(app, "shaders/rock.wgsl");
    embedded_asset!(app, "shaders/ocean.wgsl");
    embedded_asset!(app, "shaders/river.wgsl");
    embedded_asset!(app, "shaders/spray.wgsl");
}

pub(crate) fn load_cloud(app: &mut App) {
    embedded_asset!(app, "shaders/cloud.wgsl");
}
