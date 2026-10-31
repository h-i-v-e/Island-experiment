//! A scaled Sun and one analytically orbiting, spinning planet.
//!
//! Positions are heliocentric metres in double precision. The planet's terrain,
//! collision and rendering can remain in their existing planet-local frame;
//! only the sampled local Sun direction and irradiance need cross that boundary.
use crate::DVec3;
use glam::DQuat;
use std::f64::consts::{PI, TAU};

/// Mean-Earth-radius scaling, using NASA's Sun and Earth fact-sheet distances.
/// Sources: <https://nssdc.gsfc.nasa.gov/planetary/factsheet/sunfact.html>
/// and <https://nssdc.gsfc.nasa.gov/planetary/factsheet/earthfact.html>.
pub const EARTH_ORBIT_RADIUS_RATIO: f64 = 149_598_000.0 / 6_371.0;
pub const SUN_RADIUS_RATIO: f64 = 695_700.0 / 6_371.0;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SolarSettings {
    /// Orbital semimajor axis divided by the planet's radius.
    pub orbit_radius_ratio: f64,
    pub sun_radius_ratio: f64,
    /// Orbital and sidereal spin periods in simulation seconds.
    pub orbital_period: f64,
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
            orbit_radius_ratio: EARTH_ORBIT_RADIUS_RATIO,
            sun_radius_ratio: SUN_RADIUS_RATIO,
            orbital_period: 365.256 * 86_400.0,
            rotation_period: 86_400.0,
            axial_tilt: 23.44_f64.to_radians(),
            eccentricity: 0.0,
            initial_orbit_angle: 0.0,
            initial_spin_angle: (-59.0_f64).to_radians(),
        }
    }
}
impl SolarSettings {
    /// # Errors
    /// Rejects non-finite, degenerate or star-intersecting configurations.
    pub fn validate(self) -> Result<(), String> {
        for (name, value, minimum, maximum) in [
            ("orbit radius ratio", self.orbit_radius_ratio, 2.0, 1.0e12),
            ("Sun radius ratio", self.sun_radius_ratio, 0.001, 1.0e8),
            ("orbital period", self.orbital_period, 1.0, 1.0e15),
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
    /// Unit direction from planet centre towards the Sun, in planet-local axes.
    pub sun_direction: DVec3,
    pub local_to_inertial: DQuat,
    pub distance: f64,
    pub sun_radius: f64,
    /// Apparent angular radius in radians, not the diameter.
    pub angular_radius: f64,
    /// Irradiance relative to the default scaled Sun at the default orbit.
    /// Changing Sun radius assumes constant stellar surface brightness.
    pub irradiance_ratio: f64,
    /// Wrapped true orbital anomaly and sidereal spin angle, in radians.
    pub orbit_angle: f64,
    pub spin_angle: f64,
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
        let time = if seconds.is_finite() { seconds } else { 0.0 };
        let settings = self.settings;
        let mean = phase(time, settings.orbital_period, settings.initial_orbit_angle);
        let eccentric = eccentric_anomaly(mean, settings.eccentricity);
        let (sine, cosine) = eccentric.sin_cos();
        let semimajor = settings.orbit_radius_ratio * self.planet_radius;
        let planet_position = DVec3::new(
            semimajor * (cosine - settings.eccentricity),
            0.0,
            -semimajor * (1.0 - settings.eccentricity.powi(2)).sqrt() * sine,
        );
        let distance = planet_position.length();
        let spin_angle = phase(time, settings.rotation_period, settings.initial_spin_angle);
        let local_to_inertial =
            DQuat::from_rotation_z(settings.axial_tilt) * DQuat::from_rotation_y(spin_angle);
        let sun_direction = local_to_inertial.conjugate() * (-planet_position / distance);
        let sun_radius = settings.sun_radius_ratio * self.planet_radius;
        let reference_distance = EARTH_ORBIT_RADIUS_RATIO * self.planet_radius;
        SolarState {
            planet_position,
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
    fn scaled_sun_has_earthlike_angular_size_on_a_one_kilometre_planet() {
        let system = SolarSystem::new(SolarSettings::default(), 1000.0).unwrap();
        let state = system.state(0.0);
        assert!((state.sun_radius / 1000.0 - 109.2).abs() < 0.1);
        assert!((state.distance / 1000.0 - 23_481.0).abs() < 1.0);
        assert!((2.0 * state.angular_radius.to_degrees() - 0.533).abs() < 0.002);
        assert!((state.irradiance_ratio - 1.0).abs() < 1.0e-12);
        let large = SolarSystem::new(SolarSettings::default(), 6_371_000.0)
            .unwrap()
            .state(0.0);
        assert!((large.angular_radius - state.angular_radius).abs() < 1.0e-12);
        assert!((large.irradiance_ratio - state.irradiance_ratio).abs() < 1.0e-12);
    }

    #[test]
    fn spin_produces_day_and_night_without_translating_local_terrain() {
        let settings = SolarSettings {
            orbital_period: 1.0e15,
            ..SolarSettings::default()
        };
        let system = SolarSystem::new(settings, 1000.0).unwrap();
        let noon = system.sun_direction(0.0);
        let observer = DVec3::new(noon.x, 0.0, noon.z).normalize();
        assert!(observer.dot(noon) > 0.9);
        assert!(observer.dot(system.sun_direction(settings.rotation_period * 0.5)) < -0.9);
        assert!(
            system
                .sun_direction(settings.rotation_period)
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
        let later = system.state(settings.orbital_period * 0.001);
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
        assert!(system.sun_direction(settings.orbital_period * 0.25).y.abs() < 1.0e-12);
        assert!(
            (system.sun_direction(settings.orbital_period * 0.5).y.asin() + settings.axial_tilt)
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
        let apoapsis = system.state(settings.orbital_period * 0.5);
        let semimajor = settings.orbit_radius_ratio * 1000.0;
        assert!((periapsis.distance / semimajor - 0.8).abs() < 1.0e-12);
        assert!((apoapsis.distance / semimajor - 1.2).abs() < 1.0e-12);
        assert!((periapsis.irradiance_ratio / apoapsis.irradiance_ratio - 2.25).abs() < 1.0e-12);
        assert!(
            system
                .state(settings.orbital_period)
                .planet_position
                .distance(periapsis.planet_position)
                < 1.0e-7
        );
        assert!(
            system
                .state(-settings.orbital_period)
                .planet_position
                .distance(periapsis.planet_position)
                < 1.0e-7
        );
    }

    #[test]
    fn moving_the_orbit_changes_brightness_and_orientation_round_trips() {
        let settings = SolarSettings {
            orbit_radius_ratio: EARTH_ORBIT_RADIUS_RATIO * 2.0,
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
                ..SolarSettings::default()
            },
            1000.0,
        )
        .unwrap();
        for index in 0..200 {
            let time = f64::from(index) * system.settings().orbital_period / 199.0;
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
                orbital_period: 0.0,
                ..SolarSettings::default()
            },
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
