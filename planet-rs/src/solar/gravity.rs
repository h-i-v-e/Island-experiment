//! Effective game-body gravity. Surface accelerations stay familiar while the
//! radii are small; masses scale with R², rather than using real-world masses.
use super::SolarSettings;
use std::f64::consts::TAU;
pub const PLANET_SURFACE_GRAVITY: f64 = 9.81;
/// NASA Sun fact sheet: <https://nssdc.gsfc.nasa.gov/planetary/factsheet/sunfact.html>.
pub const SUN_SURFACE_GRAVITY: f64 = 274.0;
/// Lunar/terrestrial density ratio, NASA Moon fact sheet.
pub const MOON_DENSITY_RATIO: f64 = 0.606;
#[must_use]
pub fn planet_mu(radius: f64) -> f64 {
    PLANET_SURFACE_GRAVITY * radius * radius
}
#[must_use]
pub fn sun_mu(settings: SolarSettings, radius: f64) -> f64 {
    SUN_SURFACE_GRAVITY * (settings.sun_radius_ratio * radius).powi(2)
}
#[must_use]
pub fn period(semimajor: f64, mu: f64) -> f64 {
    TAU * (semimajor.powi(3) / mu).sqrt()
}
#[must_use]
pub fn semimajor(period: f64, mu: f64) -> f64 {
    (mu * (period / TAU).powi(2)).cbrt()
}
#[must_use]
pub fn circular_speed(radius: f64, mu: f64) -> f64 {
    (mu / radius).sqrt()
}
