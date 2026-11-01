//! Close circular two-body orbit, with a period and velocity derived from gravity.
use super::{SolarSettings, SolarState};
use crate::DVec3;
use glam::DQuat;
use std::f64::consts::{PI, TAU};

pub const DEFAULT_RADIUS_RATIO: f64 = 0.175;
pub const DEFAULT_DISTANCE_RATIO: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MoonOrbit {
    pub radius_ratio: f64,
    pub distance_ratio: f64,
    pub inclination: f64,
    pub initial_angle: f64,
}
impl Default for MoonOrbit {
    fn default() -> Self {
        Self {
            radius_ratio: DEFAULT_RADIUS_RATIO,
            distance_ratio: DEFAULT_DISTANCE_RATIO,
            inclination: 8.0_f64.to_radians(),
            initial_angle: 110.0_f64.to_radians(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoonState {
    /// Offset from the planet centre in the rotating planet-local frame, metres.
    pub position: DVec3,
    /// Moon-local +X permanently points towards the planet; +Y is orbit north.
    pub rotation: DQuat,
    pub radius: f64,
}
impl MoonOrbit {
    #[must_use]
    pub fn gravitational_parameter(self, radius: f64) -> f64 {
        super::gravity::planet_mu(radius)
            * (1.0 + super::gravity::MOON_DENSITY_RATIO * self.radius_ratio.powi(3))
    }
    #[must_use]
    pub fn orbital_period(self, radius: f64) -> f64 {
        super::gravity::period(
            self.distance_ratio * radius,
            self.gravitational_parameter(radius),
        )
    }
    #[must_use]
    pub fn orbital_speed(self, radius: f64) -> f64 {
        super::gravity::circular_speed(
            self.distance_ratio * radius,
            self.gravitational_parameter(radius),
        )
    }
    #[must_use]
    pub fn distance_for_period(self, radius: f64, seconds: f64) -> f64 {
        super::gravity::semimajor(seconds, self.gravitational_parameter(radius)) / radius
    }
    /// # Errors
    /// Rejects intersecting orbits and moons outside the conservative Hill limit.
    pub fn validate_for(self, settings: SolarSettings, radius: f64) -> Result<(), String> {
        self.validate()?;
        if self.distance_ratio * radius >= 0.45 * settings.hill_radius(radius) {
            return Err(
                "moon orbit must lie inside 45% of the planet's periapsis Hill radius".into(),
            );
        }
        Ok(())
    }

    /// # Errors
    /// Rejects nonfinite, intersecting or unbounded game-scale orbits.
    pub fn validate(self) -> Result<(), String> {
        for (value, lo, hi) in [
            (self.radius_ratio, 0.05, 0.4),
            (self.distance_ratio, 2.0, 10.0),
            (self.inclination, 0.0, PI / 3.0),
        ] {
            if !value.is_finite() || !(lo..=hi).contains(&value) {
                return Err(
                    "invalid companion moon radius, distance, period or inclination".into(),
                );
            }
        }
        if !self.initial_angle.is_finite() {
            return Err("moon angle must be finite".into());
        }
        Ok(())
    }
    #[must_use]
    pub fn state(
        self,
        seconds: f64,
        planet_radius: f64,
        _settings: SolarSettings,
        planet: Option<SolarState>,
    ) -> MoonState {
        let seconds = if seconds.is_finite() { seconds } else { 0.0 };
        let period = self.orbital_period(planet_radius);
        let phase =
            (seconds.rem_euclid(period) / period * TAU + self.initial_angle).rem_euclid(TAU);
        let plane = DQuat::from_rotation_z(self.inclination);
        let to_local = planet.map_or(DQuat::IDENTITY, |p| p.local_to_inertial.conjugate());
        let direction = to_local * plane * DVec3::new(phase.cos(), 0.0, -phase.sin());
        MoonState {
            position: direction * (planet_radius * self.distance_ratio),
            rotation: to_local * plane * DQuat::from_rotation_y(phase + PI),
            radius: planet_radius * self.radius_ratio,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solar::SolarSystem;
    #[test]
    fn moon_period_balances_gravity_and_is_independent_of_planet_spin() {
        let orbit = MoonOrbit::default();
        let radius = 2000.0;
        let settings = SolarSettings::default();
        orbit.validate_for(settings, radius).unwrap();
        let distance = orbit.distance_ratio * radius;
        let period = orbit.orbital_period(radius);
        assert!(
            (orbit.distance_for_period(radius, period) / orbit.distance_ratio - 1.0).abs() < 1e-12
        );
        assert!(
            (orbit.orbital_speed(radius).powi(2) / distance
                - orbit.gravitational_parameter(radius) / distance.powi(2))
            .abs()
                < 1e-12
        );
        let time = 123.0;
        let dt = 0.001;
        let sample = |t| orbit.state(t, radius, settings, None).position;
        let velocity = (sample(time + dt) - sample(time - dt)) / (2.0 * dt);
        assert!((velocity.length() / orbit.orbital_speed(radius) - 1.0).abs() < 1e-8);
        let faster_spin = SolarSettings {
            rotation_period: 3600.0,
            ..settings
        };
        assert_eq!(
            orbit.state(time, radius, settings, None),
            orbit.state(time, radius, faster_spin, None)
        );
        let unsafe_sun = SolarSettings {
            orbit_radius_ratio: 500.0,
            ..settings
        };
        assert!(orbit.validate_for(unsafe_sun, radius).is_err());
        let farther = MoonOrbit {
            distance_ratio: 6.0,
            ..orbit
        };
        assert!((farther.orbital_period(radius) / period - 2.0_f64.powf(1.5)).abs() < 1e-12);
    }
    #[test]
    fn scaled_companion_has_surface_clearance_and_stays_tidally_locked_over_many_days() {
        let orbit = MoonOrbit::default();
        orbit.validate().unwrap();
        for radius in [1000.0, 2000.0] {
            let settings = SolarSettings::default();
            let system = SolarSystem::new(settings, radius).unwrap();
            for seconds in [0.0, 43200.0, 86400.0 * 8.0, 86400.0 * 12345.0] {
                let state = orbit.state(seconds, radius, settings, Some(system.state(seconds)));
                assert!((state.position.length() - radius * 3.0).abs() < 1e-7);
                assert!((state.radius - radius * 0.175).abs() < 1e-10);
                assert!((state.rotation * DVec3::X).dot(-state.position.normalize()) > 1.0 - 1e-12);
                assert!((state.position.length() - radius - state.radius) / radius > 1.8);
            }
        }
    }
    #[test]
    fn default_moon_begins_clear_of_the_planets_shadow() {
        let settings = SolarSettings::default();
        let system = crate::solar::SolarSystem::new(settings, 2000.0).unwrap();
        let planet = system.state(0.0);
        let moon = MoonOrbit::default().state(0.0, 2000.0, settings, Some(planet));
        let projection = (-moon.position.dot(planet.sun_direction)).max(0.0);
        let nearest = moon.position + planet.sun_direction * projection;
        assert!(nearest.length() > 2000.0 + moon.radius);
    }

    #[test]
    fn relative_inertial_orbit_repeats_and_invalid_orbits_are_rejected() {
        let orbit = MoonOrbit::default();
        let settings = SolarSettings::default();
        let a = orbit.state(0.0, 2000.0, settings, None);
        let b = orbit.state(orbit.orbital_period(2000.0), 2000.0, settings, None);
        assert!(a.position.distance(b.position) < 1e-8);
        for distance in [f64::NAN, 1.0, 11.0] {
            assert!(
                MoonOrbit {
                    distance_ratio: distance,
                    ..orbit
                }
                .validate()
                .is_err()
            );
        }
    }
}
