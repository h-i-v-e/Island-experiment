//! Small off-screen lit material preview using Bevy's standard parallax map.

#![allow(clippy::struct_excessive_bools)]

use bevy::{
    camera::{RenderTarget, visibility::RenderLayers},
    image::Image,
    math::Affine2,
    pbr::{ParallaxMappingMethod, StandardMaterial},
    prelude::*,
    render::render_resource::TextureFormat,
};
use bevy_egui::{EguiTextureHandle, EguiUserTextures, egui};
use motu::procedural_textures::NormalConvention;

use crate::preview::{PreviewAssets, RegisteredImage};

const PREVIEW_LAYER: usize = 1;
const TARGET_SIZE: u32 = 640;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreviewShape {
    Sphere,
    #[default]
    Plane,
}

/// Artist controls applied to the lit preview scene.
#[derive(Resource, Clone, Debug)]
pub struct LitPreviewControls {
    pub shape: PreviewShape,
    pub albedo: bool,
    pub normal: bool,
    pub occlusion: bool,
    pub height: bool,
    pub tiling: f32,
    pub roughness: f32,
    pub light_azimuth_degrees: f32,
    pub light_elevation_degrees: f32,
    pub light_intensity: f32,
    pub ambient_strength: f32,
    pub height_scale: f32,
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub camera_distance: f32,
}

impl Default for LitPreviewControls {
    fn default() -> Self {
        Self {
            shape: PreviewShape::Plane,
            albedo: true,
            normal: true,
            occlusion: true,
            height: true,
            tiling: 1.0,
            roughness: 0.55,
            light_azimuth_degrees: 35.0,
            light_elevation_degrees: 50.0,
            light_intensity: 8_000.0,
            ambient_strength: 250.0,
            height_scale: 1.0,
            orbit_yaw: 0.55,
            orbit_pitch: 0.85,
            camera_distance: 3.1,
        }
    }
}

impl LitPreviewControls {
    pub fn reset_view(&mut self) {
        self.orbit_yaw = 0.55;
        self.orbit_pitch = 0.85;
        self.camera_distance = 3.1;
    }
}

#[derive(Resource)]
pub struct LitPreviewTarget {
    pub image: RegisteredImage,
    material: Handle<StandardMaterial>,
}

#[derive(Component)]
struct PreviewSphere;

#[derive(Component)]
struct PreviewPlane;

#[derive(Component)]
struct PreviewCamera;

#[derive(Component)]
struct PreviewLight;

pub struct LitPreviewPlugin;

impl Plugin for LitPreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LitPreviewControls>()
            .add_systems(Startup, setup)
            .add_systems(Update, sync_scene);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut egui_textures: ResMut<EguiUserTextures>,
) {
    let target = images.add(Image::new_target_texture(
        TARGET_SIZE,
        TARGET_SIZE,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    ));
    let texture_id = egui_textures.add_image(EguiTextureHandle::Strong(target.clone()));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.55, 0.55),
        perceptual_roughness: 0.55,
        reflectance: 0.3,
        parallax_mapping_method: ParallaxMappingMethod::Relief { max_steps: 8 },
        max_parallax_layer_count: 32.0,
        ..default()
    });
    let layer = RenderLayers::layer(PREVIEW_LAYER);
    let sphere = Sphere::new(1.0)
        .mesh()
        .uv(96, 64)
        .with_generated_tangents()
        .expect("UV sphere has valid tangents");
    let plane = Plane3d::default()
        .mesh()
        .size(2.4, 2.4)
        .subdivisions(64)
        .build()
        .with_generated_tangents()
        .expect("preview plane has valid tangents");
    commands.spawn((
        Mesh3d(meshes.add(sphere)),
        MeshMaterial3d(material.clone()),
        PreviewSphere,
        layer.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(plane)),
        MeshMaterial3d(material.clone()),
        Visibility::Hidden,
        PreviewPlane,
        layer.clone(),
    ));
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -1,
            clear_color: Color::srgb(0.035, 0.045, 0.055).into(),
            ..default()
        },
        RenderTarget::Image(target.clone().into()),
        Transform::from_xyz(1.5, 0.9, 2.5).looking_at(Vec3::ZERO, Vec3::Y),
        PreviewCamera,
        layer.clone(),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(2.0, 3.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
        PreviewLight,
        layer,
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 250.0,
        ..default()
    });
    commands.insert_resource(LitPreviewTarget {
        image: RegisteredImage {
            handle: target,
            texture_id,
        },
        material,
    });
}

#[allow(clippy::too_many_arguments)]
fn sync_scene(
    controls: Res<LitPreviewControls>,
    previews: Res<PreviewAssets>,
    target: Res<LitPreviewTarget>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sphere: Single<&mut Visibility, (With<PreviewSphere>, Without<PreviewPlane>)>,
    mut plane: Single<&mut Visibility, (With<PreviewPlane>, Without<PreviewSphere>)>,
    mut camera: Single<&mut Transform, (With<PreviewCamera>, Without<PreviewLight>)>,
    mut light: Single<(&mut Transform, &mut DirectionalLight), With<PreviewLight>>,
) {
    if controls.is_changed() {
        **sphere = if controls.shape == PreviewShape::Sphere {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        **plane = if controls.shape == PreviewShape::Plane {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        let horizontal = controls.camera_distance * controls.orbit_pitch.cos();
        camera.translation = Vec3::new(
            horizontal * controls.orbit_yaw.sin(),
            controls.camera_distance * controls.orbit_pitch.sin(),
            horizontal * controls.orbit_yaw.cos(),
        );
        camera.look_at(Vec3::ZERO, Vec3::Y);
        let azimuth = controls.light_azimuth_degrees.to_radians();
        let elevation = controls.light_elevation_degrees.to_radians();
        light.0.translation = Vec3::new(
            elevation.cos() * azimuth.sin(),
            elevation.sin(),
            elevation.cos() * azimuth.cos(),
        ) * 4.0;
        light.0.look_at(Vec3::ZERO, Vec3::Y);
        light.1.illuminance = controls.light_intensity;
        ambient.brightness = controls.ambient_strength;
    }
    if !(controls.is_changed() || previews.is_changed()) {
        return;
    }
    let Some(mut material) = materials.get_mut(&target.material) else {
        return;
    };
    material.perceptual_roughness = controls.roughness;
    material.uv_transform = Affine2::from_scale(Vec2::splat(controls.tiling));
    material.base_color_texture = controls
        .albedo
        .then(|| previews.albedo.as_ref().map(|image| image.handle.clone()))
        .flatten();
    material.normal_map_texture = controls
        .normal
        .then(|| previews.normal.as_ref().map(|image| image.handle.clone()))
        .flatten();
    material.occlusion_texture = controls
        .occlusion
        .then(|| {
            previews
                .occlusion
                .as_ref()
                .map(|image| image.handle.clone())
        })
        .flatten();
    material.depth_map = controls.height.then(|| previews.depth.clone()).flatten();
    if let Some(maps) = &previews.maps {
        material.flip_normal_map_y =
            maps.textures.metadata.normal_convention == NormalConvention::DirectX;
        let displacement_range =
            maps.textures.metadata.maximum_height_m - maps.textures.metadata.minimum_height_m;
        let tile_width = maps.textures.metadata.physical_tile_size_m[0].max(f32::EPSILON);
        material.parallax_depth_scale =
            displacement_range / tile_width * controls.height_scale * controls.tiling;
    }
}

/// Handles orbit/zoom input over the egui lit-preview image.
pub fn interact(response: &egui::Response, controls: &mut LitPreviewControls) {
    if response.dragged_by(egui::PointerButton::Primary) {
        let delta = response.drag_delta();
        controls.orbit_yaw -= delta.x * 0.01;
        controls.orbit_pitch = (controls.orbit_pitch + delta.y * 0.01).clamp(-1.2, 1.2);
    }
    if response.hovered() {
        let scroll = response.ctx.input(|input| input.smooth_scroll_delta.y);
        controls.camera_distance =
            (controls.camera_distance * (-scroll * 0.002).exp()).clamp(1.6, 7.0);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parallax_depth_scale_is_a_physical_tile_ratio() {
        let minimum = -0.05_f32;
        let maximum = 0.15_f32;
        let tile_width = 2.0_f32;
        let artist_scale = 0.5_f32;
        let tiling = 2.0_f32;
        let scale = (maximum - minimum) / tile_width * artist_scale * tiling;
        assert!((scale - 0.1).abs() < f32::EPSILON);
    }
}
