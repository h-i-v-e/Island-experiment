//! One persistent cubemap is a celestial sphere at infinity: camera translation
//! has no parallax, while the solar frame rotates it consistently with the day.
use crate::{
    Orbit, Scene,
    environment::EnvironmentSettings,
    solar::{SolarControls, SolarFrame},
};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::Skybox,
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct StarSettings {
    pub enabled: bool,
    pub brightness: f32,
}
impl Default for StarSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            brightness: 1.0,
        }
    }
}
impl StarSettings {
    pub fn validate(self) -> Result<(), String> {
        if !self.brightness.is_finite() || !(0.0..=3.0).contains(&self.brightness) {
            return Err("star brightness must be 0..3".into());
        }
        Ok(())
    }
}
#[derive(Resource, Default)]
struct StarImage(Handle<Image>);
pub struct StarPlugin;
impl Plugin for StarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StarImage>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                update
                    .after(crate::environment::FrameUpdate)
                    .after(crate::solar::SolarUpdate)
                    .after(crate::explore::update),
            );
    }
}
fn setup(mut image: ResMut<StarImage>, mut images: ResMut<Assets<Image>>) {
    image.0 = images.add(star_image(1024));
}

// A fixed independent stream keeps the sky unchanged when terrain is regenerated.
struct Random(u64);
impl Random {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        #[allow(clippy::cast_precision_loss)]
        let value = ((x ^ (x >> 31)) >> 11) as f64 / 9_007_199_254_740_992.0;
        value
    }
}
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Bounded cube texels and display colour.
fn star_image(size: u32) -> Image {
    let mut pixels = vec![0_u8; size as usize * size as usize * 6 * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    let mut random = Random(0x25ba_806d_a97c_1743);
    let pole = planet_rs::DVec3::new(0.28, 0.71, 0.64).normalize();
    let axis = pole.any_orthonormal_vector();
    for i in 0..6500 {
        let longitude = random.unit() * std::f64::consts::TAU;
        let height = (random.unit() * 2.0 - 1.0) * if i < 2000 { 0.14 } else { 1.0 };
        let horizontal = (1.0 - height * height).sqrt();
        let direction = (axis * longitude.cos() + pole.cross(axis) * longitude.sin()) * horizontal
            + pole * height;
        let tangent = direction.any_orthonormal_vector();
        let bitangent = direction.cross(tangent);
        let luminosity = 0.18 + 0.82 * random.unit().powf(3.0);
        let tint = match (random.unit() * 10.0) as u32 {
            0 => [1.0, 0.65, 0.42],
            1 | 2 => [0.68, 0.82, 1.0],
            _ => [1.0, 0.96, 0.88],
        };
        let sigma = 0.38 + luminosity * 0.35;
        for dy in -3..=3 {
            for dx in -3..=3 {
                let weight =
                    (-(f64::from(dx * dx + dy * dy)) / (2.0 * sigma * sigma)).exp() * luminosity;
                if weight < 0.015 {
                    continue;
                }
                // Direction-space splats wrap naturally across cube faces/poles.
                let offset = (direction
                    + (tangent * f64::from(dx) + bitangent * f64::from(dy))
                        * (1.5 / f64::from(size)))
                .normalize();
                let (face, pixel_x, pixel_y) = planet_rs::ocean_surface::cube_pixel(offset, size);
                let index = ((face * size as usize + pixel_y as usize) * size as usize
                    + pixel_x as usize)
                    * 4;
                for channel in 0..3 {
                    pixels[index + channel] =
                        pixels[index + channel].max((255.0 * weight * tint[channel]) as u8);
                }
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}
#[allow(clippy::cast_possible_truncation)] // Planet metres to render-space units.
fn visibility(settings: &EnvironmentSettings, scene: &Scene, eye: Vec3, sun: Vec3) -> f32 {
    if !settings.atmosphere || scene.terrain.is_moon() {
        return 1.0;
    }
    let altitude = (eye.length() - 1.0) * scene.terrain.topology.radius as f32;
    let space = ((altitude / settings.atmosphere_height.max(1.0) - 0.85) / 0.15).clamp(0.0, 1.0);
    let daylight = ((eye.normalize_or_zero().dot(sun) + 0.12) / 0.24).clamp(0.0, 1.0);
    let night = 1.0 - daylight * daylight * (3.0 - 2.0 * daylight);
    night + (1.0 - night) * space * space * (3.0 - 2.0 * space)
}
fn update(
    mut commands: Commands,
    scene: Res<Scene>,
    solar: Res<SolarControls>,
    frame: Res<SolarFrame>,
    atmosphere: (
        Res<EnvironmentSettings>,
        Res<crate::environment::EnvironmentFrame>,
    ),
    image: Res<StarImage>,
    mut cameras: Query<(Entity, &Transform, Option<&mut Skybox>), With<Orbit>>,
) {
    let (environment, air) = atmosphere;
    let effective = EnvironmentSettings {
        atmosphere_height: environment.atmosphere_height.max(air.uniform.air.x),
        ..*environment
    };
    let sun = frame.state.map_or_else(
        || environment.sun_direction(),
        |s| crate::surface::vector(s.sun_direction),
    );
    let rotation = frame.state.map_or(Quat::IDENTITY, |s| {
        Quat::from_array(s.local_to_inertial.conjugate().as_quat().to_array())
    });
    for (id, eye, sky) in &mut cameras {
        let brightness = if solar.stars.enabled {
            1600.0 * solar.stars.brightness * visibility(&effective, &scene, eye.translation, sun)
        } else {
            0.0
        };
        if let Some(mut sky) = sky {
            sky.brightness = brightness;
            sky.rotation = rotation;
        } else {
            commands.entity(id).insert(Skybox {
                image: Some(image.0.clone()),
                brightness,
                rotation,
            });
        }
    }
}
pub fn controls(ui: &mut bevy_egui::egui::Ui, settings: &mut StarSettings) {
    ui.checkbox(&mut settings.enabled, "Surrounding star sphere");
    ui.add(
        bevy_egui::egui::Slider::new(&mut settings.brightness, 0.0..=3.0).text("Star brightness"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn star_cube_is_seeded_nonempty_and_populates_all_six_faces() {
        let a = star_image(64);
        let b = star_image(64);
        assert_eq!(a.data, b.data);
        let data = a.data.as_ref().unwrap();
        for face in data.chunks_exact(64 * 64 * 4) {
            assert!(face.chunks_exact(4).filter(|p| p[0] > 0).count() > 20);
        }
        assert_eq!(
            a.texture_view_descriptor.unwrap().dimension,
            Some(TextureViewDimension::Cube)
        );
    }
    #[test]
    fn stars_fade_in_daylight_and_remain_visible_from_space() {
        let scene = crate::collision::test_scene();
        let settings = EnvironmentSettings::default();
        assert!(visibility(&settings, &scene, Vec3::Y * 1.001, Vec3::Y) < 0.01);
        assert!(visibility(&settings, &scene, -Vec3::Y * 1.001, Vec3::Y) > 0.99);
        assert!(visibility(&settings, &scene, Vec3::Y * 3.0, Vec3::Y) > 0.99);
    }
}
