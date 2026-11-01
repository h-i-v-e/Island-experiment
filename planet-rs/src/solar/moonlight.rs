//! Reflected sunlight from a diffuse lunar disk, evaluated towards the planet.
use super::moon::MoonState;
use crate::DVec3;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReflectedLight {
    /// Lambert phase function: one at full phase, zero at new phase.
    pub phase: f64,
    /// Smooth fractional lunar illumination during planetary eclipses.
    pub sunlight: f64,
    /// Irradiance at the planet centre, divided by incident solar irradiance.
    pub irradiance_ratio: f64,
}
impl MoonState {
    /// Geometric albedo uses the full-phase disk brightness convention. The
    /// planet's spherical shadow is softened over the finite Sun and Moon disks.
    #[must_use]
    pub fn reflected_light(
        self,
        planet_radius: f64,
        sun_direction: DVec3,
        sun_angular_radius: f64,
        albedo: f64,
    ) -> ReflectedLight {
        let distance = self.position.length();
        if distance <= planet_radius || self.radius <= 0.0 || !distance.is_finite() {
            return ReflectedLight::default();
        }
        let sun = sun_direction.normalize_or_zero();
        if sun.length_squared() < 0.5 {
            return ReflectedLight::default();
        }
        let cosine = (-self.position / distance).dot(sun).clamp(-1.0, 1.0);
        let separation = cosine.acos();
        let phase = lambert_phase(cosine);
        let planet_angle = (planet_radius / distance).clamp(0.0, 1.0).asin();
        let spread = sun_angular_radius.max(0.0) + (self.radius / distance).clamp(0.0, 1.0).asin();
        let progress =
            ((separation - planet_angle + spread) / (2.0 * spread).max(1e-12)).clamp(0.0, 1.0);
        let sunlight = progress * progress * (3.0 - 2.0 * progress);
        ReflectedLight {
            phase,
            sunlight,
            irradiance_ratio: albedo.clamp(0.0, 1.0)
                * phase
                * sunlight
                * (self.radius / distance).powi(2),
        }
    }
}
fn lambert_phase(cosine: f64) -> f64 {
    let cosine = cosine.clamp(-1.0, 1.0);
    (((1.0 - cosine * cosine).max(0.0).sqrt() + (PI - cosine.acos()) * cosine) / PI).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DQuat;
    #[test]
    fn reflected_disk_phase_follows_full_quarter_and_new() {
        assert!((lambert_phase(1.0) - 1.0).abs() < 1e-12);
        assert!((lambert_phase(0.0) - 1.0 / PI).abs() < 1e-12);
        assert!(lambert_phase(-1.0).abs() < 1e-12);
    }
    #[test]
    fn illumination_scales_with_albedo_apparent_area_and_inverse_square_distance() {
        let moon = MoonState {
            position: DVec3::Z * 6000.0,
            radius: 350.0,
            rotation: DQuat::IDENTITY,
        };
        let sample = moon.reflected_light(2000.0, DVec3::X, 0.00465, 0.12);
        assert!((sample.sunlight - 1.0).abs() < 1e-12);
        let twice = MoonState {
            position: moon.position * 2.0,
            ..moon
        }
        .reflected_light(2000.0, DVec3::X, 0.00465, 0.12);
        assert!((twice.irradiance_ratio / sample.irradiance_ratio - 0.25).abs() < 1e-12);
        let reflective = moon.reflected_light(2000.0, DVec3::X, 0.00465, 0.24);
        assert!((reflective.irradiance_ratio / sample.irradiance_ratio - 2.0).abs() < 1e-12);
        let scaled = MoonState {
            position: moon.position * 2.0,
            radius: moon.radius * 2.0,
            ..moon
        }
        .reflected_light(4000.0, DVec3::X, 0.00465, 0.12);
        assert!((scaled.irradiance_ratio - sample.irradiance_ratio).abs() < 1e-12);
        assert!(
            moon.reflected_light(2000.0, DVec3::X, 0.00465, 0.0)
                .irradiance_ratio
                .abs()
                < 1e-12
        );
    }
    #[test]
    fn a_planetary_eclipse_extinguishes_full_moon_reflection_and_fades_at_the_edge() {
        let moon = MoonState {
            position: DVec3::Z * 6000.0,
            radius: 350.0,
            rotation: DQuat::IDENTITY,
        };
        let eclipsed = moon.reflected_light(2000.0, -DVec3::Z, 0.00465, 0.12);
        assert!((eclipsed.phase - 1.0).abs() < 1e-12);
        assert!(eclipsed.irradiance_ratio.abs() < 1e-12);
        let edge = (2000.0_f64 / 6000.0).asin();
        let sun = DVec3::new(edge.sin(), 0.0, -edge.cos());
        let partial = moon.reflected_light(2000.0, sun, 0.00465, 0.12);
        assert!((partial.sunlight - 0.5).abs() < 1e-12);
    }
}
