//! A scaled Sun and one analytically orbiting, spinning planet.
//!
//! Positions are heliocentric metres in double precision. The planet's terrain,
//! collision and rendering can remain in their existing planet-local frame;
//! only the sampled local Sun direction and irradiance need cross that boundary.
pub mod gravity;
pub mod moon;
pub mod moonlight;

use crate::DVec3;
use glam::DQuat;
use std::f64::consts::{PI, TAU};

/// Mean-Earth-radius scaling, using NASA's Sun and Earth fact-sheet distances.
/// Sources: <https://nssdc.gsfc.nasa.gov/planetary/factsheet/sunfact.html>
/// and <https://nssdc.gsfc.nasa.gov/planetary/factsheet/earthfact.html>.
pub const EARTH_ORBIT_RADIUS_RATIO: f64 = 149_598_000.0 / 6_371.0;
pub const SUN_RADIUS_RATIO: f64 = 695_700.0 / 6_371.0;
/// Match the overhead Moon's angular disk for an observer on the planet surface.
pub const DEFAULT_ORBIT_RADIUS_RATIO: f64 =
    1.0 + SUN_RADIUS_RATIO * (moon::DEFAULT_DISTANCE_RATIO - 1.0) / moon::DEFAULT_RADIUS_RATIO;
const fn legacy_irradiance_reference() -> f64 {
    EARTH_ORBIT_RADIUS_RATIO
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SolarSettings {
    /// Orbital semimajor axis divided by the planet's radius.
    pub orbit_radius_ratio: f64,
    pub sun_radius_ratio: f64,
    /// Distance in planet radii receiving unit irradiance from the reference Sun.
    /// Legacy files retain the former luminosity until explicitly disk-matched.
    #[serde(default = "legacy_irradiance_reference")]
    pub irradiance_reference_ratio: f64,
    /// Sidereal spin period in climate/model seconds; orbital periods derive from gravity.
    pub rotation_period: f64,
    /// Angle between the planet's north axis and orbital +Y, in radians.
    pub axial_tilt: f64,
    pub eccentricity: f64,
    /// Mean anomaly at the epoch, in radians; zero starts at periapsis.
    pub initial_orbit_angle: f64,
    pub initial_spin_angle: f64,
}
impl Default for SolarSettings {
    fn default() -> Self {
        Self {
            orbit_radius_ratio: DEFAULT_ORBIT_RADIUS_RATIO,
            sun_radius_ratio: SUN_RADIUS_RATIO,
            irradiance_reference_ratio: DEFAULT_ORBIT_RADIUS_RATIO,
            rotation_period: 86_400.0,
            axial_tilt: 23.44_f64.to_radians(),
            eccentricity: 0.0,
            initial_orbit_angle: 0.0,
            initial_spin_angle: (-59.0_f64).to_radians(),
        }
    }
}
impl SolarSettings {
    /// Match both angular radii at conjunction above a surface observer, keeping
    /// the current irradiance and the gravity-derived period/distance link.
    /// Eccentric orbits match at the semimajor distance and vary over the year.
    /// # Errors
    /// Rejects invalid bodies, star intersections and unsafe lunar orbits.
    pub fn match_moon_disk(&mut self, moon: moon::MoonOrbit) -> Result<(), String> {
        self.validate()?;
        moon.validate()?;
        let mut matched = *self;
        matched.orbit_radius_ratio =
            1.0 + self.sun_radius_ratio * (moon.distance_ratio - 1.0) / moon.radius_ratio;
        matched.irradiance_reference_ratio *= matched.orbit_radius_ratio / self.orbit_radius_ratio;
        matched.validate()?;
        moon.validate_for(matched, 1.0)?;
        *self = matched;
        Ok(())
    }

    #[must_use]
    pub fn gravitational_parameter(self, radius: f64) -> f64 {
        gravity::sun_mu(self, radius) + gravity::planet_mu(radius)
    }
    #[must_use]
    pub fn orbital_period(self, radius: f64) -> f64 {
        gravity::period(
            self.orbit_radius_ratio * radius,
            self.gravitational_parameter(radius),
        )
    }
    #[must_use]
    pub fn orbit_ratio_for_period(self, radius: f64, seconds: f64) -> f64 {
        gravity::semimajor(seconds, self.gravitational_parameter(radius)) / radius
    }
    /// Conservative periapsis Hill radius for the planet in physical metres.
    #[must_use]
    pub fn hill_radius(self, radius: f64) -> f64 {
        self.orbit_radius_ratio
            * radius
            * (1.0 - self.eccentricity)
            * (gravity::planet_mu(radius) / (3.0 * gravity::sun_mu(self, radius))).cbrt()
    }

    /// # Errors
    /// Rejects non-finite, degenerate or star-intersecting configurations.
    pub fn validate(self) -> Result<(), String> {
        for (name, value, minimum, maximum) in [
            ("orbit radius ratio", self.orbit_radius_ratio, 2.0, 1.0e12),
            ("Sun radius ratio", self.sun_radius_ratio, 0.001, 1.0e8),
            (
                "irradiance reference ratio",
                self.irradiance_reference_ratio,
                0.001,
                1.0e12,
            ),
            ("rotation period", self.rotation_period, 1.0, 1.0e15),
            ("axial tilt", self.axial_tilt, 0.0, PI),
            ("orbital eccentricity", self.eccentricity, 0.0, 0.95),
        ] {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                return Err(format!("{name} must be {minimum}..{maximum}"));
            }
        }
        if !self.initial_orbit_angle.is_finite() || !self.initial_spin_angle.is_finite() {
            return Err("initial orbital and spin angles must be finite".into());
        }
        if self.orbit_radius_ratio * (1.0 - self.eccentricity) <= self.sun_radius_ratio + 1.0 {
            return Err("the planet's periapsis must remain outside the Sun".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolarState {
    pub planet_position: DVec3,
    /// Relative orbital velocity in heliocentric axes, metres per physical second.
    pub planet_velocity: DVec3,
    /// Unit direction from planet centre towards the Sun, in planet-local axes.
    pub sun_direction: DVec3,
    pub local_to_inertial: DQuat,
    pub distance: f64,
    pub sun_radius: f64,
    /// Apparent angular radius in radians, not the diameter.
    pub angular_radius: f64,
    /// Irradiance relative to the configured reference luminosity.
    /// Ordinary distance edits retain inverse-square falloff; disk matching
    /// adjusts luminosity to preserve the existing daylight and climate.
    pub irradiance_ratio: f64,
    /// Wrapped true orbital anomaly and sidereal spin angle, in radians.
    pub orbit_angle: f64,
    pub spin_angle: f64,
}

impl SolarState {
    /// Convert heliocentric metres into rotating planet-local metres.
    #[must_use]
    pub fn inertial_to_local(self, position: DVec3) -> DVec3 {
        self.local_to_inertial.conjugate() * (position - self.planet_position)
    }
    /// Convert rotating planet-local metres into heliocentric metres.
    #[must_use]
    pub fn local_to_inertial_position(self, position: DVec3) -> DVec3 {
        self.planet_position + self.local_to_inertial * position
    }
    /// The Sun is fixed at the heliocentric origin, never attached to a camera.
    #[must_use]
    pub fn sun_position(self) -> DVec3 {
        self.inertial_to_local(DVec3::ZERO)
    }
}

/// Validated configuration, owning only scalars. Sampling borrows it and allocates nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolarSystem {
    settings: SolarSettings,
    planet_radius: f64,
}
impl SolarSystem {
    /// # Errors
    /// Rejects invalid settings and unusable physical planet radii.
    pub fn new(settings: SolarSettings, planet_radius: f64) -> Result<Self, String> {
        settings.validate()?;
        if !planet_radius.is_finite() || !(0.001..=1.0e15).contains(&planet_radius) {
            return Err("planet radius must be 0.001..1e15 metres".into());
        }
        Ok(Self {
            settings,
            planet_radius,
        })
    }
    #[must_use]
    pub const fn settings(&self) -> SolarSettings {
        self.settings
    }
    #[must_use]
    pub const fn planet_radius(&self) -> f64 {
        self.planet_radius
    }
    /// Samples any finite timestamp directly, without accumulating orbital drift.
    /// Negative times sample before the epoch; non-finite times sample the epoch.
    #[must_use]
    pub fn state(&self, seconds: f64) -> SolarState {
        self.state_at(seconds, seconds)
    }
    /// Orbit uses physical seconds; spin may use an independently paced climate clock.
    #[must_use]
    pub fn state_at(&self, orbital_seconds: f64, spin_seconds: f64) -> SolarState {
        let time = if orbital_seconds.is_finite() {
            orbital_seconds
        } else {
            0.0
        };
        let spin = if spin_seconds.is_finite() {
            spin_seconds
        } else {
            0.0
        };
        let settings = self.settings;
        let period = settings.orbital_period(self.planet_radius);
        let mean = phase(time, period, settings.initial_orbit_angle);
        let eccentric = eccentric_anomaly(mean, settings.eccentricity);
        let (sine, cosine) = eccentric.sin_cos();
        let semimajor = settings.orbit_radius_ratio * self.planet_radius;
        let planet_position = DVec3::new(
            semimajor * (cosine - settings.eccentricity),
            0.0,
            -semimajor * (1.0 - settings.eccentricity.powi(2)).sqrt() * sine,
        );
        let distance = planet_position.length();
        let velocity_scale = TAU / period * semimajor / (1.0 - settings.eccentricity * cosine);
        let planet_velocity = DVec3::new(
            -sine,
            0.0,
            -(1.0 - settings.eccentricity.powi(2)).sqrt() * cosine,
        ) * velocity_scale;
        let spin_angle = phase(spin, settings.rotation_period, settings.initial_spin_angle);
        let local_to_inertial =
            DQuat::from_rotation_z(settings.axial_tilt) * DQuat::from_rotation_y(spin_angle);
        let sun_direction = local_to_inertial.conjugate() * (-planet_position / distance);
        let sun_radius = settings.sun_radius_ratio * self.planet_radius;
        let reference_distance = settings.irradiance_reference_ratio * self.planet_radius;
        SolarState {
            planet_position,
            planet_velocity,
            sun_direction,
            local_to_inertial,
            distance,
            sun_radius,
            angular_radius: (sun_radius / distance).clamp(0.0, 1.0).asin(),
            irradiance_ratio: (settings.sun_radius_ratio / SUN_RADIUS_RATIO).powi(2)
                * (reference_distance / distance).powi(2),
            orbit_angle: (-planet_position.z)
                .atan2(planet_position.x)
                .rem_euclid(TAU),
            spin_angle,
        }
    }
    #[must_use]
    pub fn sun_direction(&self, seconds: f64) -> DVec3 {
        self.state(seconds).sun_direction
    }
}

fn phase(time: f64, period: f64, initial: f64) -> f64 {
    (time.rem_euclid(period) / period * TAU + initial.rem_euclid(TAU)).rem_euclid(TAU)
}
fn eccentric_anomaly(mean: f64, eccentricity: f64) -> f64 {
    let mut angle = if eccentricity < 0.8 { mean } else { PI };
    for _ in 0..20 {
        let correction =
            (angle - eccentricity * angle.sin() - mean) / (1.0 - eccentricity * angle.cos());
        angle -= correction;
        if correction.abs() < 1.0e-13 {
            break;
        }
    }
    angle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_real_sun_remains_at_the_heliocentric_origin_in_every_render_frame() {
        let system = SolarSystem::new(SolarSettings::default(), 2000.0).unwrap();
        for time in [0.0, 1000.0, 5000.0, 10000.0] {
            let state = system.state_at(time, time * 48.0);
            let sun = state.sun_position();
            assert!(state.local_to_inertial_position(sun).length() < 1e-8);
            assert!(sun.normalize().distance(state.sun_direction) < 1e-12);
            assert!((sun.length() - state.distance).abs() < 1e-8);
            let ship = DVec3::new(300_000.0, 5000.0, -200_000.0);
            assert!(
                state
                    .local_to_inertial_position(state.inertial_to_local(ship))
                    .distance(ship)
                    < 1e-8
            );
        }
    }
    #[test]
    fn matching_surface_disks_keeps_daylight_and_a_safe_gravity_linked_moon() {
        for (sun_radius, moon_radius, moon_distance, eccentricity) in [
            (SUN_RADIUS_RATIO, 0.175, 3.0, 0.0),
            (SUN_RADIUS_RATIO, 0.175, 3.0, 0.2),
            (90.0, 0.12, 4.0, 0.0),
        ] {
            let moon = moon::MoonOrbit {
                radius_ratio: moon_radius,
                distance_ratio: moon_distance,
                ..moon::MoonOrbit::default()
            };
            let mut settings = SolarSettings {
                orbit_radius_ratio: EARTH_ORBIT_RADIUS_RATIO,
                irradiance_reference_ratio: EARTH_ORBIT_RADIUS_RATIO,
                sun_radius_ratio: sun_radius,
                eccentricity,
                ..SolarSettings::default()
            };
            let before = SolarSystem::new(settings, 2000.0).unwrap().state(0.0);
            settings.match_moon_disk(moon).unwrap();
            let sun_angle =
                (settings.sun_radius_ratio / (settings.orbit_radius_ratio - 1.0)).asin();
            let moon_angle = (moon.radius_ratio / (moon.distance_ratio - 1.0)).asin();
            assert!((sun_angle - moon_angle).abs() < 1e-12);
            moon.validate_for(settings, 2000.0).unwrap();
            let system = SolarSystem::new(settings, 2000.0).unwrap();
            let after = system.state(0.0);
            assert!((after.irradiance_ratio / before.irradiance_ratio - 1.0).abs() < 1e-12);
            assert!(
                (system
                    .state(settings.orbital_period(2000.0))
                    .planet_position
                    - after.planet_position)
                    .length()
                    < 1e-7
            );
            let once = settings;
            settings.match_moon_disk(moon).unwrap();
            assert_eq!(once, settings);
        }
        let mut settings = SolarSettings::default();
        let original = settings;
        assert!(
            settings
                .match_moon_disk(moon::MoonOrbit {
                    radius_ratio: 0.4,
                    distance_ratio: 2.0,
                    ..moon::MoonOrbit::default()
                })
                .is_err()
        );
        assert_eq!(
            settings, original,
            "failed matches preserve the active settings"
        );
    }

    #[test]
    fn gravity_sets_period_speed_and_conserves_elliptic_energy_and_momentum() {
        let radius = 2000.0;
        let settings = SolarSettings {
            eccentricity: 0.4,
            ..SolarSettings::default()
        };
        let system = SolarSystem::new(settings, radius).unwrap();
        let a = settings.orbit_radius_ratio * radius;
        let mu = settings.gravitational_parameter(radius);
        let period = settings.orbital_period(radius);
        assert!(
            (settings.orbit_ratio_for_period(radius, period) / settings.orbit_radius_ratio - 1.0)
                .abs()
                < 1e-12
        );
        let reference = system.state(0.0);
        let angular_momentum = reference.planet_position.cross(reference.planet_velocity);
        for i in 0..100 {
            let time = period * f64::from(i) / 100.0;
            let state = system.state(time);
            let energy = state.planet_velocity.length_squared() / 2.0 - mu / state.distance;
            assert!((energy / (-mu / (2.0 * a)) - 1.0).abs() < 1e-12);
            assert!(
                state
                    .planet_position
                    .cross(state.planet_velocity)
                    .distance(angular_momentum)
                    / angular_momentum.length()
                    < 1e-12
            );
            let dt = 0.01;
            let acceleration = (system.state(time + dt).planet_velocity
                - system.state(time - dt).planet_velocity)
                / (2.0 * dt);
            let gravity = -state.planet_position * (mu / state.distance.powi(3));
            assert!(acceleration.distance(gravity) / gravity.length() < 1e-6);
            assert_eq!(
                system.state_at(time, 123.0).planet_position,
                state.planet_position
            );
        }
        let farther = SolarSettings {
            orbit_radius_ratio: settings.orbit_radius_ratio * 2.0,
            ..settings
        };
        assert!((farther.orbital_period(radius) / period - 2.0_f64.powf(1.5)).abs() < 1e-12);
    }

    #[test]
    fn scaled_system_follows_the_two_kilometre_default_and_preserves_angular_size_and_heating() {
        let radius = crate::PlanetOptions::default().radius;
        let system = SolarSystem::new(SolarSettings::default(), radius).unwrap();
        let state = system.state(0.0);
        assert!((state.sun_radius / 1000.0 - 218.4).abs() < 0.1);
        assert!((state.distance / 1000.0 - 2497.953).abs() < 1.0);
        assert!((2.0 * state.angular_radius.to_degrees() - 10.032).abs() < 0.002);
        assert!((state.irradiance_ratio - 1.0).abs() < 1.0e-12);
        let previous = SolarSystem::new(SolarSettings::default(), 1000.0)
            .unwrap()
            .state(0.0);
        assert!((state.sun_radius - previous.sun_radius * 2.0).abs() < 1e-9);
        assert!((state.distance - previous.distance * 2.0).abs() < 1e-8);
        for time in [
            0.0,
            43_200.0,
            system.settings().orbital_period(system.planet_radius()) * 0.5,
        ] {
            let current = system.state(time);
            let old = SolarSystem::new(SolarSettings::default(), 1000.0)
                .unwrap()
                .state_at(time / (2.0_f64).sqrt(), time);
            assert!(current.sun_direction.distance(old.sun_direction) < 1e-12);
            assert!((current.angular_radius - old.angular_radius).abs() < 1e-12);
            assert!((current.irradiance_ratio - old.irradiance_ratio).abs() < 1e-12);
        }
        let large = SolarSystem::new(SolarSettings::default(), 6_371_000.0)
            .unwrap()
            .state(0.0);
        assert!((large.angular_radius - state.angular_radius).abs() < 1.0e-12);
        assert!((large.irradiance_ratio - state.irradiance_ratio).abs() < 1.0e-12);
    }

    #[test]
    fn spin_produces_day_and_night_without_translating_local_terrain() {
        let settings = SolarSettings::default();
        let system = SolarSystem::new(settings, 1000.0).unwrap();
        let noon = system.state_at(0.0, 0.0).sun_direction;
        let observer = DVec3::new(noon.x, 0.0, noon.z).normalize();
        assert!(observer.dot(noon) > 0.9);
        assert!(
            observer.dot(
                system
                    .state_at(0.0, settings.rotation_period * 0.5)
                    .sun_direction
            ) < -0.9
        );
        assert!(
            system
                .state_at(0.0, settings.rotation_period)
                .sun_direction
                .distance(noon)
                < 1.0e-9
        );
    }

    #[test]
    fn orbit_and_spin_are_both_prograde_about_the_declared_orbital_normal() {
        let settings = SolarSettings {
            axial_tilt: 0.0,
            initial_spin_angle: 0.0,
            ..SolarSettings::default()
        };
        let system = SolarSystem::new(settings, 1000.0).unwrap();
        let start = system.state(0.0);
        let later = system.state(settings.orbital_period(system.planet_radius()) * 0.001);
        assert!(
            start
                .planet_position
                .cross(later.planet_position)
                .dot(DVec3::Y)
                > 0.0
        );
        let spinning = system.state(settings.rotation_period * 0.001);
        let initial_marker = start.local_to_inertial * DVec3::X;
        let moved_marker = spinning.local_to_inertial * DVec3::X;
        assert!(initial_marker.cross(moved_marker).dot(DVec3::Y) > 0.0);
    }

    #[test]
    fn tilted_axis_gives_opposite_seasons_and_an_equinoctial_crossing() {
        let settings = SolarSettings::default();
        let system = SolarSystem::new(settings, 1000.0).unwrap();
        assert!((system.sun_direction(0.0).y.asin() - settings.axial_tilt).abs() < 1.0e-12);
        assert!(
            system
                .sun_direction(settings.orbital_period(system.planet_radius()) * 0.25)
                .y
                .abs()
                < 1.0e-12
        );
        assert!(
            (system
                .sun_direction(settings.orbital_period(system.planet_radius()) * 0.5)
                .y
                .asin()
                + settings.axial_tilt)
                .abs()
                < 1.0e-12
        );
    }

    #[test]
    fn orbit_closes_and_eccentric_distance_controls_irradiance() {
        let settings = SolarSettings {
            eccentricity: 0.2,
            ..SolarSettings::default()
        };
        let system = SolarSystem::new(settings, 1000.0).unwrap();
        let periapsis = system.state(0.0);
        let apoapsis = system.state(settings.orbital_period(system.planet_radius()) * 0.5);
        let semimajor = settings.orbit_radius_ratio * 1000.0;
        assert!((periapsis.distance / semimajor - 0.8).abs() < 1.0e-12);
        assert!((apoapsis.distance / semimajor - 1.2).abs() < 1.0e-12);
        assert!((periapsis.irradiance_ratio / apoapsis.irradiance_ratio - 2.25).abs() < 1.0e-12);
        assert!(
            system
                .state(settings.orbital_period(system.planet_radius()))
                .planet_position
                .distance(periapsis.planet_position)
                < 1.0e-7
        );
        assert!(
            system
                .state(-settings.orbital_period(system.planet_radius()))
                .planet_position
                .distance(periapsis.planet_position)
                < 1.0e-7
        );
    }

    #[test]
    fn moving_the_orbit_changes_brightness_and_orientation_round_trips() {
        let settings = SolarSettings {
            orbit_radius_ratio: DEFAULT_ORBIT_RADIUS_RATIO * 2.0,
            ..SolarSettings::default()
        };
        let system = SolarSystem::new(settings, 1000.0).unwrap();
        for time in [0.0, 40_000.0, 1.0e12, -500_000.0, f64::NAN] {
            let state = system.state(time);
            assert!(state.sun_direction.is_finite());
            assert!((state.sun_direction.length() - 1.0).abs() < 1.0e-12);
            assert!((state.irradiance_ratio - 0.25).abs() < 1.0e-12);
            assert!(
                (state.local_to_inertial * state.sun_direction + state.planet_position.normalize())
                    .length()
                    < 1.0e-12
            );
        }
    }

    #[test]
    fn kepler_solver_remains_finite_for_extreme_supported_eccentricity() {
        let system = SolarSystem::new(
            SolarSettings {
                eccentricity: 0.95,
                orbit_radius_ratio: EARTH_ORBIT_RADIUS_RATIO,
                ..SolarSettings::default()
            },
            1000.0,
        )
        .unwrap();
        for index in 0..200 {
            let time =
                f64::from(index) * system.settings().orbital_period(system.planet_radius()) / 199.0;
            let state = system.state(time);
            assert!(state.planet_position.is_finite());
            assert!(state.sun_direction.is_finite());
            assert!(state.distance > state.sun_radius + system.planet_radius());
            assert!(state.irradiance_ratio.is_finite() && state.irradiance_ratio > 0.0);
        }
    }

    #[test]
    fn invalid_settings_and_star_intersections_are_rejected() {
        for settings in [
            SolarSettings {
                rotation_period: f64::NAN,
                ..SolarSettings::default()
            },
            SolarSettings {
                eccentricity: 1.0,
                ..SolarSettings::default()
            },
            SolarSettings {
                sun_radius_ratio: f64::INFINITY,
                ..SolarSettings::default()
            },
            SolarSettings {
                axial_tilt: -0.1,
                ..SolarSettings::default()
            },
            SolarSettings {
                initial_spin_angle: f64::INFINITY,
                ..SolarSettings::default()
            },
            SolarSettings {
                orbit_radius_ratio: 100.0,
                ..SolarSettings::default()
            },
        ] {
            assert!(SolarSystem::new(settings, 1000.0).is_err());
        }
        for radius in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(SolarSystem::new(SolarSettings::default(), radius).is_err());
        }
    }
}
