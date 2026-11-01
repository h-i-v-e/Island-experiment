//! A phase-driven reflected light, aimed back from the Moon onto the planet.
//! One shadow projection covers the receiving body. No mesh/material rebuilds.
use crate::{
    Scene,
    environment::EnvironmentSettings,
    lighting::{LightingSettings, Sun},
    moon::{MoonUpdate, MoonWorld},
    solar::{SolarControls, SolarFrame},
};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MoonlightSettings {
    pub albedo: f64,
    pub strength: f64,
    pub shadows: bool,
}
impl Default for MoonlightSettings {
    fn default() -> Self {
        Self {
            albedo: 0.12,
            strength: 8.0,
            shadows: false,
        }
    }
}
impl MoonlightSettings {
    pub fn validate(self) -> Result<(), String> {
        if !self.albedo.is_finite()
            || !(0.0..=1.0).contains(&self.albedo)
            || !self.strength.is_finite()
            || !(0.0..=20.0).contains(&self.strength)
        {
            return Err("moon reflectivity must be 0..1 and moonlight strength 0..20".into());
        }
        Ok(())
    }
}
#[derive(Component)]
struct Moonlight;
#[derive(Resource, Default)]
pub struct MoonlightFrame {
    pub lux: f64,
    pub phase: f64,
    pub sunlight: f64,
}
pub struct MoonlightPlugin;
impl Plugin for MoonlightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoonlightFrame>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                update.after(MoonUpdate).after(crate::lighting::update),
            );
    }
}
fn setup(mut commands: Commands) {
    commands.spawn((
        Moonlight,
        SpotLight {
            intensity: 0.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::default(),
    ));
}

/// GPU light intensity is in render units. Calibrate the centre illuminance
/// against Bevy's 4pi lumens conversion and smooth range falloff.
#[allow(clippy::cast_possible_truncation)]
fn light_sample(
    settings: &SolarControls,
    frame: &SolarFrame,
    radius: f64,
    sun: planet_rs::DVec3,
    angular_radius: f64,
    solar_lux: f64,
    shadows: bool,
) -> (SpotLight, Transform, MoonlightFrame) {
    let moon =
        settings
            .companion
            .orbit
            .state(frame.orbital_seconds, radius, settings.model, frame.state);
    let reflected =
        moon.reflected_light(radius, sun, angular_radius, settings.companion.light.albedo);
    let lux = solar_lux * reflected.irradiance_ratio * settings.companion.light.strength;
    // Start outside the lunar surface, so its own shell cannot occlude every
    // shadow ray leaving the source. The cone includes the whole receiving body.
    let source = (moon.position - moon.position.normalize() * moon.radius * 1.15) / radius;
    let distance = source.length();
    let range = distance * 4.0 + 1.5;
    let falloff = (1.0 - (distance / range).powi(4)).powi(2);
    let outer = ((1.45 / distance).clamp(0.0, 1.0).asin() + 0.1).min(1.5);
    let light = SpotLight {
        color: Color::linear_rgb(0.90, 0.94, 1.0),
        intensity: (lux * 4.0 * std::f64::consts::PI * distance * distance / falloff) as f32,
        range: range as f32,
        radius: (moon.radius / radius * 0.4) as f32,
        inner_angle: (outer - 0.06) as f32,
        outer_angle: outer as f32,
        shadow_maps_enabled: shadows && settings.companion.light.shadows && lux > 1e-6,
        shadow_depth_bias: (0.02 / radius) as f32,
        shadow_normal_bias: 1.8,
        shadow_map_near_z: 0.02,
        ..default()
    };
    let source = crate::surface::vector(source);
    let up = if source.normalize().dot(Vec3::Y).abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    (
        light,
        Transform::from_translation(source).looking_at(Vec3::ZERO, up),
        MoonlightFrame {
            lux,
            phase: reflected.phase,
            sunlight: reflected.sunlight,
        },
    )
}
#[allow(clippy::too_many_arguments)] // Independent shared render state, one light entity.
fn update(
    scene: Res<Scene>,
    settings: Res<SolarControls>,
    frame: Res<SolarFrame>,
    environment: Res<EnvironmentSettings>,
    lighting: Res<LightingSettings>,
    moon: Res<MoonWorld>,
    sun: Query<&DirectionalLight, With<Sun>>,
    mut lights: Query<(&mut SpotLight, &mut Transform), With<Moonlight>>,
    mut report: ResMut<MoonlightFrame>,
) {
    let active = settings.companion.enabled && !scene.terrain.is_moon() && moon.visible_body();
    if !active {
        *report = MoonlightFrame::default();
        for (mut light, _) in &mut lights {
            light.intensity = 0.0;
            light.shadow_maps_enabled = false;
        }
        return;
    }
    let direction = frame.state.map_or_else(
        || planet_rs::DVec3::from_array(environment.sun_direction().to_array().map(f64::from)),
        |s| s.sun_direction,
    );
    let angular_radius = frame.state.map_or(0.00465, |s| s.angular_radius);
    let sun_lux = sun
        .single()
        .map_or(12000.0, |light| f64::from(light.illuminance));
    let (next, transform, state) = light_sample(
        &settings,
        &frame,
        scene.terrain.topology.radius,
        direction,
        angular_radius,
        sun_lux,
        lighting.shadows,
    );
    *report = state;
    for (mut light, mut position) in &mut lights {
        if light.intensity.to_bits() != next.intensity.to_bits()
            || light.range.to_bits() != next.range.to_bits()
            || light.radius.to_bits() != next.radius.to_bits()
            || light.outer_angle.to_bits() != next.outer_angle.to_bits()
            || light.shadow_depth_bias.to_bits() != next.shadow_depth_bias.to_bits()
            || light.shadow_maps_enabled != next.shadow_maps_enabled
        {
            *light = next;
        }
        if *position != transform {
            *position = transform;
        }
    }
}
pub fn controls(ui: &mut bevy_egui::egui::Ui, settings: &mut MoonlightSettings) {
    use bevy_egui::egui;
    ui.add(egui::Slider::new(&mut settings.albedo, 0.0..=1.0).text("Reflectivity (albedo)"));
    ui.add(egui::Slider::new(&mut settings.strength, 0.0..=20.0).text("Moonlight strength"));
    ui.checkbox(&mut settings.shadows, "Cast moonlight shadows");
    ui.small("Phase, distance and eclipses affect the light. Strength 1 uses the reflectivity model; 0 disables moonlight.");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendering_preserves_physical_scale_calibrates_lux_and_avoids_the_lunar_shadow_shell() {
        let mut settings = SolarControls::default();
        settings.companion.orbit.initial_angle = 40_f64.to_radians();
        let mut previous: Option<f64> = None;
        for radius in [1000.0, 2000.0] {
            let state = planet_rs::solar::SolarSystem::new(settings.model, radius)
                .unwrap()
                .state(0.0);
            let mut frame = SolarFrame::default();
            frame.state = Some(state);
            let (light, transform, report) = light_sample(
                &settings,
                &frame,
                radius,
                state.sun_direction,
                state.angular_radius,
                12000.0,
                true,
            );
            assert!(report.lux > 0.0 && report.sunlight > 0.99);
            let distance = f64::from(transform.translation.length());
            let range = f64::from(light.range);
            let lux = f64::from(light.intensity)
                / (4.0 * std::f64::consts::PI * distance * distance)
                * (1.0 - (distance / range).powi(4)).powi(2);
            assert!((lux / report.lux - 1.0).abs() < 1e-6);
            let moon = settings
                .companion
                .orbit
                .state(0.0, radius, settings.model, Some(state));

            assert!(
                (f64::from(transform.translation.length()) - moon.position.length() / radius).abs()
                    > moon.radius / radius
            );
            if let Some(previous) = previous {
                assert!((report.lux - previous).abs() < 1e-9);
            }
            previous = Some(report.lux);
        }
    }
    #[test]
    fn zero_strength_or_reflectivity_disables_light_and_shadow_updates() {
        let mut settings = SolarControls::default();
        settings.companion.light.strength = 0.0;
        let (light, _, report) = light_sample(
            &settings,
            &SolarFrame::default(),
            2000.0,
            planet_rs::DVec3::Y,
            0.00465,
            12000.0,
            true,
        );
        assert!(
            light.intensity.abs() < 1e-9 && report.lux.abs() < 1e-9 && !light.shadow_maps_enabled
        );
        settings.companion.light.strength = 8.0;
        settings.companion.light.albedo = 0.0;
        let (light, _, _) = light_sample(
            &settings,
            &SolarFrame::default(),
            2000.0,
            planet_rs::DVec3::Y,
            0.00465,
            12000.0,
            true,
        );
        assert!(light.intensity.abs() < 1e-9 && !light.shadow_maps_enabled);
        settings.companion.light.albedo = 0.12;
        settings.companion.light.shadows = true;
        let (shadowed, _, _) = light_sample(
            &settings,
            &SolarFrame::default(),
            2000.0,
            planet_rs::DVec3::Y,
            0.00465,
            12000.0,
            true,
        );
        assert!(shadowed.shadow_maps_enabled);
        settings.companion.light.shadows = false;
        let (light, _, _) = light_sample(
            &settings,
            &SolarFrame::default(),
            2000.0,
            planet_rs::DVec3::Y,
            0.00465,
            12000.0,
            true,
        );
        assert!(light.intensity > 0.0 && !light.shadow_maps_enabled);
        for value in [f64::NAN, -1.0, 21.0] {
            assert!(
                MoonlightSettings {
                    strength: value,
                    ..default()
                }
                .validate()
                .is_err()
            );
        }
    }
}
