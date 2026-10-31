//! Celestial motion stays in f64; terrain, streaming and collision stay planet-local.
use crate::{
    Scene,
    environment::EnvironmentSettings,
    lighting::Sun,
    weather::{WeatherControls, WeatherFrame},
};
use bevy::{light::SunDisk, prelude::*};
use planet_rs::solar::{SolarSettings, SolarState, SolarSystem};

#[derive(Resource, Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SolarControls {
    pub enabled: bool,
    pub model: SolarSettings,
    pub draft: SolarSettings,
    #[serde(skip)]
    pub revision: u64,
    pub paused: bool,
}
impl Default for SolarControls {
    fn default() -> Self {
        Self {
            enabled: true,
            model: SolarSettings::default(),
            draft: SolarSettings::default(),
            revision: 0,
            paused: false,
        }
    }
}
#[derive(Resource, Default)]
pub struct SolarFrame {
    pub state: Option<SolarState>,
    pub seconds: f64,
    pub enabled: bool,
    pub paused: bool,
    pub revision: u64,
    geometry: Option<(usize, u64, u64)>,
}
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct SolarUpdate;
pub struct SolarPlugin;
impl Plugin for SolarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SolarControls>()
            .init_resource::<SolarFrame>()
            .add_systems(
                Update,
                update
                    .in_set(SolarUpdate)
                    .after(crate::weather::WeatherUpdate)
                    .before(crate::ocean::update),
            );
    }
}
#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
fn update(
    mut commands: Commands,
    scene: Res<Scene>,
    time: Res<Time>,
    settings: Res<SolarControls>,
    environment: Res<EnvironmentSettings>,
    weather: Res<WeatherControls>,
    fields: Res<WeatherFrame>,
    mut frame: ResMut<SolarFrame>,
    mut suns: Query<(Entity, &mut DirectionalLight, Option<&mut SunDisk>), With<Sun>>,
) {
    let identity = (
        std::sync::Arc::as_ptr(&scene.terrain) as usize,
        scene.seed,
        settings.revision,
    );
    if frame.geometry != Some(identity) || frame.enabled != settings.enabled {
        frame.geometry = Some(identity);
        frame.seconds = 0.0;
        frame.revision = frame.revision.wrapping_add(1);
    }
    frame.enabled = settings.enabled;
    frame.paused = settings.enabled && settings.paused;
    if weather.enabled {
        frame.seconds = fields.render_seconds();
    } else if !settings.paused && !environment.paused && !weather.paused {
        frame.seconds += time.delta_secs_f64() * weather.model.time_scale * weather.speed;
    }
    frame.state = if settings.enabled {
        SolarSystem::new(settings.model, scene.terrain.topology.radius)
            .ok()
            .map(|system| system.state(frame.seconds))
    } else {
        None
    };
    for (entity, mut light, disk) in &mut suns {
        let ratio = frame
            .state
            .filter(|_| settings.enabled)
            .map_or(1.0, |s| s.irradiance_ratio);
        let illuminance = (12_000.0 * ratio) as f32;
        if light.illuminance.to_bits() != illuminance.to_bits() {
            light.illuminance = illuminance;
        }
        let angular_size = frame
            .state
            .filter(|_| settings.enabled)
            .map_or(f64::from(SunDisk::EARTH.angular_size), |s| {
                s.angular_radius * 2.0
            }) as f32;
        if let Some(mut disk) = disk {
            if disk.angular_size.to_bits() != angular_size.to_bits() {
                disk.angular_size = angular_size;
            }
        } else {
            commands.entity(entity).insert(SunDisk {
                angular_size,
                intensity: 1.0,
            });
        }
    }
}

pub fn controls(
    ui: &mut bevy_egui::egui::Ui,
    settings: &mut SolarControls,
    frame: Option<&SolarFrame>,
    planet_radius: f64,
) {
    use bevy_egui::egui;
    egui::CollapsingHeader::new("Solar system").show(ui, |ui| {
        if ui
            .checkbox(&mut settings.enabled, "Orbiting planet and scaled Sun")
            .changed()
        {
            settings.revision = settings.revision.wrapping_add(1);
        }
        ui.checkbox(&mut settings.paused, "Pause the system and weather");
        ui.small("Uses the same simulation clock as weather. Terrain remains planet-local.");
        let mut day = settings.draft.rotation_period / 3600.0;
        let mut year = settings.draft.orbital_period / 86400.0;
        let mut tilt = settings.draft.axial_tilt.to_degrees();
        ui.add(egui::Slider::new(&mut day, 4.0..=72.0).text("Day (hours)"));
        ui.add(
            egui::Slider::new(&mut year, 2.0..=730.0)
                .logarithmic(true)
                .text("Year (days)"),
        );
        ui.add(egui::Slider::new(&mut tilt, 0.0..=90.0).text("Axial tilt (degrees)"));
        ui.add(
            egui::Slider::new(&mut settings.draft.eccentricity, 0.0..=0.5)
                .text("Orbital eccentricity"),
        );
        settings.draft.rotation_period = day * 3600.0;
        settings.draft.orbital_period = year * 86400.0;
        settings.draft.axial_tilt = tilt.to_radians();
        ui.add(
            egui::Slider::new(&mut settings.draft.sun_radius_ratio, 20.0..=300.0)
                .text("Sun / planet radius"),
        );
        ui.add(
            egui::Slider::new(&mut settings.draft.orbit_radius_ratio, 5_000.0..=100_000.0)
                .logarithmic(true)
                .text("Orbit / planet radius"),
        );
        if ui.button("Apply system / restart weather").clicked()
            && settings.draft.validate().is_ok()
        {
            settings.model = settings.draft;
            settings.revision = settings.revision.wrapping_add(1);
        }
        if let Some(frame) = frame
            && let Some(state) = frame.state
        {
            ui.small(format!(
                "Time {:.2} days · Sun radius {:.1} km · Distance {:.0} km",
                frame.seconds / 86400.0,
                state.sun_radius / 1000.0,
                state.distance / 1000.0
            ));
            ui.small(format!(
                "Sun diameter {:.3}° · Irradiance {:.2}×",
                (state.angular_radius * 2.0).to_degrees(),
                state.irradiance_ratio
            ));
            overview(ui, settings.model, state, planet_radius);
        }
    });
}
#[allow(clippy::cast_possible_truncation)]
fn overview(ui: &mut bevy_egui::egui::Ui, settings: SolarSettings, state: SolarState, radius: f64) {
    use bevy_egui::egui;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 190.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let centre = rect.center();
    let scale = 80.0 / (settings.orbit_radius_ratio * radius * (1.0 + settings.eccentricity));
    let project =
        |v: planet_rs::DVec3| centre + egui::vec2((v.x * scale) as f32, (v.z * scale) as f32);
    if let Ok(system) = SolarSystem::new(settings, radius) {
        let points = (0..=96)
            .map(|i| {
                project(
                    system
                        .state(settings.orbital_period * f64::from(i) / 96.0)
                        .planet_position,
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(1.0, egui::Color32::GRAY),
        ));
    }
    painter.circle_filled(
        centre,
        ((state.sun_radius * scale) as f32).max(4.0),
        egui::Color32::from_rgb(255, 210, 90),
    );
    let planet = project(state.planet_position);
    painter.circle_filled(planet, 5.0, egui::Color32::from_rgb(70, 155, 230));
    painter.line_segment(
        [planet, planet + (centre - planet).normalized() * 15.0],
        egui::Stroke::new(1.0, egui::Color32::WHITE),
    );
    ui.small("Orbital positions are to scale; body markers are enlarged for visibility.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighting_uses_weather_clock_without_invalidating_history_each_frame() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::collision::test_scene())
            .init_resource::<SolarControls>()
            .init_resource::<SolarFrame>()
            .init_resource::<WeatherControls>()
            .init_resource::<WeatherFrame>()
            .init_resource::<EnvironmentSettings>()
            .add_systems(Update, update);
        let sun = app
            .world_mut()
            .spawn((Sun, DirectionalLight::default()))
            .id();
        app.world_mut().resource_mut::<WeatherFrame>().seconds = 1000.0;
        app.world_mut()
            .resource_mut::<WeatherFrame>()
            .previous_seconds = 1000.0;
        app.update();
        let first = app.world().resource::<SolarFrame>();
        let revision = first.revision;
        let direction = first.state.unwrap().sun_direction;
        assert!((first.seconds - 1000.0).abs() < 1e-10);
        let disk = app.world().get::<SunDisk>(sun).unwrap();
        assert!((f64::from(disk.angular_size).to_degrees() - 0.533).abs() < 0.002);
        {
            let mut weather = app.world_mut().resource_mut::<WeatherFrame>();
            weather.seconds = 20_000.0;
            weather.previous_seconds = 20_000.0;
        }
        app.update();
        let next = app.world().resource::<SolarFrame>();
        assert_eq!(revision, next.revision);
        assert!(next.state.unwrap().sun_direction.distance(direction) > 1.0);
        app.world_mut().resource_mut::<SolarControls>().enabled = false;
        app.update();
        assert!(app.world().resource::<SolarFrame>().state.is_none());
        assert!(
            (app.world()
                .get::<DirectionalLight>(sun)
                .unwrap()
                .illuminance
                - 12_000.0)
                .abs()
                < 0.1
        );
    }
}
