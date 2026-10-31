//! Shared Unity water optics; river attributes are derived without changing geometry.
mod flow;
mod impacts;
mod mist;
mod underwater;
use crate::{
    Scene,
    lod_material::LodMaterial,
    ocean::{OceanMaterial, OceanSettings},
};
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, ShaderType, TextureDimension, TextureFormat},
    shader::load_shader_library,
};
pub use flow::FlowField;
pub use impacts::ImpactUniform;

#[derive(Resource, Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WaterSettings {
    pub distortion: f32,
    pub absorption: f32,
    pub reflections: f32,
    pub river_ripples: f32,
    pub whitewater: f32,
    pub waterfall_speed: f32,
    pub mist: f32,
    pub underwater: bool,
    pub foam_lifetime: f32,
    pub foam_strength: f32,
}
impl Default for WaterSettings {
    fn default() -> Self {
        Self {
            distortion: 1.0,
            absorption: 1.0,
            reflections: 1.0,
            river_ripples: 0.1,
            whitewater: 0.9,
            waterfall_speed: 9.0,
            mist: 1.0,
            underwater: true,
            foam_lifetime: 6.0,
            foam_strength: 0.8,
        }
    }
}
impl WaterSettings {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, min, max) in [
            ("distortion", self.distortion, 0.0, 1.5),
            ("absorption", self.absorption, 0.0, 4.0),
            ("reflections", self.reflections, 0.0, 1.0),
            ("river ripples", self.river_ripples, 0.0, 0.5),
            ("whitewater", self.whitewater, 0.0, 1.0),
            ("waterfall speed", self.waterfall_speed, 0.0, 20.0),
            ("mist", self.mist, 0.0, 2.0),
            ("foam lifetime", self.foam_lifetime, 0.1, 30.0),
            ("foam strength", self.foam_strength, 0.0, 2.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!("water {name} must be {min}..{max}"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct WaterUniform {
    pub shape: Vec4,
    pub optics: Vec4,
    pub absorption: Vec4,
    pub river: Vec4,
    pub effects: Vec4,
}
impl WaterUniform {
    fn new(radius: f32, seconds: f32, options: &WaterSettings, sea: bool) -> Self {
        Self {
            shape: Vec4::new(radius, seconds, if sea { 0.02 } else { 0.012 }, 0.6),
            optics: Vec4::new(
                if sea { 0.65 } else { 0.45 },
                if sea { 0.8 } else { 0.55 },
                options.distortion,
                options.reflections,
            ),
            absorption: if sea {
                Vec4::new(0.45, 0.12, 0.055, options.absorption)
            } else {
                Vec4::new(0.32, 0.09, 0.16, options.absorption)
            },
            river: Vec4::new(
                options.river_ripples,
                options.whitewater,
                options.waterfall_speed,
                0.18,
            ),
            effects: Vec4::new(options.foam_strength, options.foam_lifetime, 2.0, 0.16),
        }
    }
}
#[derive(Resource, Default)]
pub struct WaterAssets {
    pub noise: Handle<Image>,
}
#[derive(Component)]
pub struct RiverChunk;
pub struct WaterPlugin;
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WaterPost;
impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "optics.wgsl");
        load_shader_library!(app, "river.wgsl");
        embedded_asset!(app, "mist.wgsl");
        app.init_resource::<WaterSettings>()
            .init_resource::<WaterAssets>()
            .init_resource::<impacts::WaterImpacts>()
            .add_plugins((mist::MistPlugin, underwater::UnderwaterPlugin))
            .add_systems(Startup, setup)
            .add_systems(Update, impacts::update.after(update));
    }
}
fn setup(mut assets: ResMut<WaterAssets>, mut images: ResMut<Assets<Image>>) {
    assets.noise = images.add(noise_image());
}
fn noise_image() -> Image {
    let hash = |x: u32, y: u32, seed: u32| {
        let mut n = (x % 32).wrapping_mul(1_664_525) ^ (y % 32).wrapping_mul(1_013_904_223) ^ seed;
        n = (n ^ (n >> 16)).wrapping_mul(2_246_822_519);
        f32::from(u16::try_from(n >> 16).unwrap()) / 65535.0
    };
    let mut data = Vec::with_capacity(256 * 256 * 4);
    for y in 0..256 {
        for x in 0..256 {
            let (cx, cy) = (x / 8, y / 8);
            let f = |n: u32| {
                let t = (f32::from(u8::try_from(n % 8).unwrap()) + 0.5) / 8.0;
                t * t * (3.0 - 2.0 * t)
            };
            for seed in [0xa341_316c, 0xc801_3ea4, 0xad90_777d] {
                let a = hash(cx, cy, seed) * (1.0 - f(x)) + hash(cx + 1, cy, seed) * f(x);
                let b = hash(cx, cy + 1, seed) * (1.0 - f(x)) + hash(cx + 1, cy + 1, seed) * f(x);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                data.push(((a * (1.0 - f(y)) + b * f(y)) * 255.0).round() as u8);
            }
            data.push(255);
        }
    }
    let mut size = 256_usize;
    let mut offset = 0;
    while size > 1 {
        let next = size / 2;
        let mut mip = Vec::with_capacity(next * next * 4);
        for y in 0..next {
            for x in 0..next {
                for c in 0..4 {
                    let sum: u16 = (0..2)
                        .flat_map(|dy| (0..2).map(move |dx| (x * 2 + dx, y * 2 + dy)))
                        .map(|(sx, sy)| u16::from(data[offset + (sy * size + sx) * 4 + c]))
                        .sum();
                    mip.push(u8::try_from((sum + 2) / 4).unwrap());
                }
            }
        }
        offset = data.len();
        data.extend(mip);
        size = next;
    }
    let mips = data.split_off(256 * 256 * 4);
    let mut image = Image::new(
        Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data.as_mut().unwrap().extend(mips);
    image.texture_descriptor.mip_level_count = 9;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}
#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
pub fn update(
    scene: Res<Scene>,
    time: Res<Time>,
    settings: Res<WaterSettings>,
    ocean: Res<OceanSettings>,
    assets: Res<WaterAssets>,
    mut rivers: ResMut<Assets<LodMaterial>>,
    mut seas: ResMut<Assets<OceanMaterial>>,
    mut seconds: Local<f32>,
    visible_rivers: Query<(&MeshMaterial3d<LodMaterial>, &Visibility), With<RiverChunk>>,
) {
    if !ocean.paused {
        *seconds += time.delta_secs() * ocean.speed;
    }
    let radius = scene.terrain.topology.radius as f32;
    for (handle, visibility) in &visible_rivers {
        if *visibility == Visibility::Visible
            && let Some(mut material) = rivers.get_mut(&handle.0)
        {
            material.extension.water = WaterUniform::new(radius, *seconds, &settings, false);
            material.extension.water_noise = Some(assets.noise.clone());
        }
    }
    for (_, material) in seas.iter_mut() {
        material.extension.optics = WaterUniform::new(radius, *seconds, &settings, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn river_noise_upload_has_the_complete_linear_mip_chain() {
        let image = noise_image();
        assert_eq!(image.texture_descriptor.mip_level_count, 9);
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        assert_eq!(image.data.as_ref().unwrap().len(), 349_524);
        assert!(WaterSettings::default().validate().is_ok());
    }
    #[test]
    fn only_visible_river_materials_receive_live_water_controls() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::collision::test_scene())
            .init_resource::<WaterSettings>()
            .init_resource::<OceanSettings>()
            .init_resource::<WaterAssets>()
            .init_resource::<Assets<LodMaterial>>()
            .init_resource::<Assets<OceanMaterial>>()
            .add_systems(Update, update);
        let visible = app
            .world_mut()
            .resource_mut::<Assets<LodMaterial>>()
            .add(LodMaterial::default());
        let hidden = app
            .world_mut()
            .resource_mut::<Assets<LodMaterial>>()
            .add(LodMaterial::default());
        app.world_mut().spawn((
            RiverChunk,
            MeshMaterial3d(visible.clone()),
            Visibility::Visible,
        ));
        let entity = app
            .world_mut()
            .spawn((
                RiverChunk,
                MeshMaterial3d(hidden.clone()),
                Visibility::Hidden,
            ))
            .id();
        app.update();
        let materials = app.world().resource::<Assets<LodMaterial>>();
        assert!(materials.get(&visible).unwrap().extension.water.shape.x > 0.0);
        assert!(
            materials
                .get(&hidden)
                .unwrap()
                .extension
                .water
                .shape
                .x
                .abs()
                < f32::EPSILON
        );
        app.world_mut()
            .entity_mut(entity)
            .insert(Visibility::Visible);
        app.world_mut().resource_mut::<WaterSettings>().whitewater = 0.4;
        app.update();
        let settings = &app
            .world()
            .resource::<Assets<LodMaterial>>()
            .get(&hidden)
            .unwrap()
            .extension
            .water;
        assert!(settings.shape.x > 0.0 && (settings.river.y - 0.4).abs() < f32::EPSILON);
    }
}
