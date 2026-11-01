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
    /// Real minutes per rotation at the 1x simulation speed.
    pub day_minutes: f64,
    pub companion: crate::moon::CompanionSettings,
    pub stars: crate::stars::StarSettings,
}
impl Default for SolarControls {
    fn default() -> Self {
        Self {
            enabled: true,
            model: SolarSettings::default(),
            draft: SolarSettings::default(),
            revision: 0,
            paused: false,
            day_minutes: 30.0,
            companion: crate::moon::CompanionSettings::default(),
            stars: crate::stars::StarSettings::default(),
        }
    }
}
impl SolarControls {
    /// Upgrade only the untouched original solar distance. Explicitly selected
    /// custom orbits and their legacy lighting calibration remain intact.
    pub(super) fn migrate_legacy_sun(&mut self) {
        use planet_rs::solar::EARTH_ORBIT_RADIUS_RATIO;
        for settings in [&mut self.model, &mut self.draft] {
            if (settings.orbit_radius_ratio / EARTH_ORBIT_RADIUS_RATIO - 1.0).abs() < 1e-12
                && (settings.irradiance_reference_ratio / EARTH_ORBIT_RADIUS_RATIO - 1.0).abs()
                    < 1e-12
            {
                let _ = settings.match_moon_disk(self.companion.orbit);
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.companion.validate()?;
        self.stars.validate()?;
        self.model.validate()?;
        self.draft.validate()?;
        if !self.day_minutes.is_finite() || !(1.0..=1440.0).contains(&self.day_minutes) {
            return Err("day/night cycle must be 1..1440 real minutes".into());
        }
        Ok(())
    }

    /// Shared spin clock for the rendered Sun and worker-owned climate. Changing
    /// its rate never changes phase, resets weather, or copies simulation state.
    pub fn climate_settings(
        &self,
        mut weather: planet_rs::weather::WeatherSettings,
    ) -> planet_rs::weather::WeatherSettings {
        if self.enabled {
            weather.time_scale = self.model.rotation_period / (self.day_minutes * 60.0);
            weather.rotation_period = self.model.rotation_period;
        }
        weather
    }
}
#[derive(Resource, Default)]
pub struct SolarFrame {
    pub state: Option<SolarState>,
    pub seconds: f64,
    pub orbital_seconds: f64,
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
                    .before(crate::ocean::update)
                    .before(crate::explore::update),
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
        frame.orbital_seconds = 0.0;
        frame.revision = frame.revision.wrapping_add(1);
    }
    frame.enabled = settings.enabled;
    frame.paused = settings.enabled && settings.paused;
    if weather.enabled && !scene.terrain.is_moon() {
        frame.seconds = fields.render_seconds();
        frame.orbital_seconds = fields.render_orbital_seconds();
    } else if !settings.paused && !environment.paused && !weather.paused {
        frame.orbital_seconds += time.delta_secs_f64() * weather.speed;
        frame.seconds += time.delta_secs_f64()
            * settings.climate_settings(weather.model).time_scale
            * weather.speed;
    }
    frame.state = if settings.enabled {
        SolarSystem::new(settings.model, scene.terrain.topology.radius)
            .ok()
            .map(|system| system.state_at(frame.orbital_seconds, frame.seconds))
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
            disk.intensity = if frame.state.is_some() { 0.0 } else { 1.0 };
            if disk.angular_size.to_bits() != angular_size.to_bits() {
                disk.angular_size = angular_size;
            }
        } else {
            commands.entity(entity).insert(SunDisk {
                angular_size,
                intensity: if frame.state.is_some() { 0.0 } else { 1.0 },
            });
        }
    }
}

pub fn controls(
    ui: &mut bevy_egui::egui::Ui,
    settings: &mut SolarControls,
    frame: Option<&SolarFrame>,
    planet_radius: f64,
    simulation_speed: f64,
) {
    use bevy_egui::egui;
    crate::moon::companion_controls(ui, &mut settings.companion, planet_radius, settings.model);
    crate::stars::controls(ui, &mut settings.stars);
    egui::CollapsingHeader::new("Solar system").show(ui, |ui| {
        if ui
            .checkbox(&mut settings.enabled, "Orbiting planet and scaled Sun")
            .changed()
        {
            settings.revision = settings.revision.wrapping_add(1);
        }
        ui.checkbox(&mut settings.paused, "Pause the system and weather");
        let minimum = (settings.model.rotation_period / (3600.0 * 60.0)).max(1.0);
        ui.add(
            egui::Slider::new(&mut settings.day_minutes, minimum..=1440.0)
                .logarithmic(true)
                .text("Day/night cycle (real minutes at 1×)"),
        );
        ui.small("A day is 24 planet hours. Cycle speed changes apply live.");
        if simulation_speed > 0.0 {
            ui.small(format!(
                "At {simulation_speed:.2}×: {:.1} real minutes per day",
                settings.day_minutes / simulation_speed
            ));
        } else {
            ui.small("Simulation speed is zero; the cycle is paused.");
        }
        ui.small("Rotation and climate follow the day clock; orbits follow physical seconds.");
        orbit_controls(ui, &mut settings.draft, settings.companion.orbit);
        linked_year_control(ui, &mut settings.draft, planet_radius, settings.day_minutes);
        if ui.button("Apply system / restart weather").clicked()
            && settings.draft.validate().is_ok()
            && settings
                .companion
                .orbit
                .validate_for(settings.draft, planet_radius)
                .is_ok()
        {
            settings.model = settings.draft;
            settings.revision = settings.revision.wrapping_add(1);
        }
        if let Err(error) = settings
            .companion
            .orbit
            .validate_for(settings.draft, planet_radius)
        {
            ui.colored_label(egui::Color32::YELLOW, error);
        }
        if let Some(frame) = frame
            && let Some(state) = frame.state
        {
            ui.small(format!(
                "Time {:.2} planet days · Sun radius {:.1} km · Distance {:.0} km",
                frame.seconds / settings.model.rotation_period,
                state.sun_radius / 1000.0,
                state.distance / 1000.0
            ));
            ui.small(format!(
                "Sun diameter {:.3}° · Irradiance {:.2}×",
                (state.angular_radius * 2.0).to_degrees(),
                state.irradiance_ratio
            ));
            ui.small(format!(
                "Orbital speed {:.1} m/s · physical time {:.1} minutes",
                state.planet_velocity.length(),
                frame.orbital_seconds / 60.0
            ));
            overview(ui, settings.model, state, planet_radius);
        }
    });
}
fn orbit_controls(
    ui: &mut bevy_egui::egui::Ui,
    draft: &mut SolarSettings,
    moon: planet_rs::solar::moon::MoonOrbit,
) {
    use bevy_egui::egui;
    let mut tilt = draft.axial_tilt.to_degrees();
    ui.add(egui::Slider::new(&mut tilt, 0.0..=90.0).text("Axial tilt (degrees)"));
    ui.add(egui::Slider::new(&mut draft.eccentricity, 0.0..=0.5).text("Orbital eccentricity"));
    draft.axial_tilt = tilt.to_radians();
    ui.add(
        egui::Slider::new(&mut draft.sun_radius_ratio, 20.0..=300.0).text("Sun / planet radius"),
    );
    let minimum_orbit = minimum_orbit_ratio(*draft);
    ui.add(
        egui::Slider::new(&mut draft.orbit_radius_ratio, minimum_orbit..=100_000.0)
            .logarithmic(true)
            .text("Orbit / planet radius"),
    );
    let mut matched = *draft;
    let result = matched.match_moon_disk(moon);
    if ui
        .add_enabled(result.is_ok(), egui::Button::new("Match Moon disk"))
        .on_hover_text(result.as_ref().err().map_or(
            "Adjust orbit and luminosity, then Apply system.",
            String::as_str,
        ))
        .clicked()
    {
        *draft = matched;
    }
    ui.small("Disk matching uses the overhead view from the surface and preserves daylight brightness. Apply system to use it.");
}
fn minimum_orbit_ratio(settings: SolarSettings) -> f64 {
    (settings.sun_radius_ratio + 1.0) / (1.0 - settings.eccentricity) * 1.01
}
fn linked_year_control(
    ui: &mut bevy_egui::egui::Ui,
    draft: &mut SolarSettings,
    planet_radius: f64,
    day_minutes: f64,
) {
    use bevy_egui::egui;
    let year_min = SolarSettings {
        orbit_radius_ratio: minimum_orbit_ratio(*draft),
        ..*draft
    }
    .orbital_period(planet_radius)
        / 86400.0;
    let year_max = SolarSettings {
        orbit_radius_ratio: 100_000.0,
        ..*draft
    }
    .orbital_period(planet_radius)
        / 86400.0;
    let mut year_days = draft.orbital_period(planet_radius) / 86400.0;
    if ui
        .add(
            egui::Slider::new(&mut year_days, year_min..=year_max)
                .logarithmic(true)
                .text("Year (physical 24-hour days)"),
        )
        .changed()
    {
        draft.orbit_radius_ratio = draft.orbit_ratio_for_period(planet_radius, year_days * 86400.0);
    }
    ui.small(format!(
        "Year: {:.1} planet rotations at 1×. Period and orbit distance are linked by gravity.",
        year_days * 86400.0 / (day_minutes * 60.0)
    ));
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
                        .state(settings.orbital_period(radius) * f64::from(i) / 96.0)
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
    fn changing_day_duration_without_weather_preserves_phase_and_planet_hours() {
        use bevy::time::TimeUpdateStrategy;
        use std::time::Duration;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::collision::test_scene())
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
            .init_resource::<SolarControls>()
            .init_resource::<SolarFrame>()
            .insert_resource(WeatherControls {
                enabled: false,
                ..WeatherControls::default()
            })
            .init_resource::<WeatherFrame>()
            .init_resource::<EnvironmentSettings>()
            .add_systems(Update, update);
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::from_secs(2));
        app.update();
        app.update();
        let before = app.world().resource::<SolarFrame>();
        let revision = before.revision;
        let state = before.state.unwrap();
        assert!(
            (before.seconds - 48.0).abs() < 1e-9,
            "default 30-minute day advances 48 model seconds per real second"
        );
        app.world_mut().resource_mut::<SolarControls>().day_minutes = 60.0;
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        app.update();
        let unchanged = app.world().resource::<SolarFrame>();
        assert_eq!(unchanged.revision, revision);
        assert!(
            unchanged
                .state
                .unwrap()
                .sun_direction
                .distance(state.sun_direction)
                < 1e-12
        );
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)));
        app.update();
        let next = app.world().resource::<SolarFrame>();
        assert!((next.seconds - 72.0).abs() < 1e-9);
        assert!((next.orbital_seconds - 2.0).abs() < 1e-9);
        let expected = SolarSystem::new(SolarSettings::default(), 2000.0)
            .unwrap()
            .state_at(2.0, 72.0);
        assert!(
            next.state
                .unwrap()
                .sun_direction
                .distance(expected.sun_direction)
                < 1e-12
        );
        assert!((SolarControls::default().model.rotation_period / 3600.0 - 24.0).abs() < 1e-12);
    }

    #[test]
    fn lighting_uses_weather_clock_without_invalidating_history_each_frame() {
        let mut scene = crate::collision::test_scene();
        let radius = planet_rs::PlanetOptions::default().radius;
        std::sync::Arc::get_mut(&mut scene.terrain)
            .unwrap()
            .topology
            .radius = radius;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(scene)
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
        let state = first.state.unwrap();
        assert!(
            (state.sun_radius - SolarSettings::default().sun_radius_ratio * radius).abs() < 1e-9
        );
        assert!(
            (state.distance - SolarSettings::default().orbit_radius_ratio * radius).abs() < 1e-8
        );
        let disk = app.world().get::<SunDisk>(sun).unwrap();
        assert!(
            disk.intensity.abs() < 1e-6,
            "the real Sun mesh replaces the sky disk"
        );
        assert!((f64::from(disk.angular_size).to_degrees() - 10.032).abs() < 0.002);
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
        assert!((app.world().get::<SunDisk>(sun).unwrap().intensity - 1.0).abs() < 1e-6);
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
