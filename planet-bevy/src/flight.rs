//! Inertial free flight in physical metres. Rendering converts once into the
//! planet's rotating unit-sphere frame, so travel is independent of day speed.
use bevy::{
    math::{DQuat, DVec3},
    prelude::*,
};
use planet_rs::solar::SolarState;

pub struct Flight {
    pub position: DVec3,
    orientation: DQuat,
    pub speed: f64,
    pub captured: bool,
}
impl Flight {
    pub fn enter(camera: &Transform, state: SolarState, radius: f64) -> Self {
        Self {
            position: DVec3::from_array(
                state
                    .local_to_inertial_position(
                        planet_rs::DVec3::from_array(camera.translation.to_array().map(f64::from))
                            * radius,
                    )
                    .to_array(),
            ),
            orientation: (DQuat::from_array(state.local_to_inertial.to_array())
                * DQuat::from_array(camera.rotation.to_array().map(f64::from)))
            .normalize(),
            speed: 10_000.0,
            captured: true,
        }
    }
    pub fn advance(
        &mut self,
        movement: Vec3,
        look: Vec2,
        scroll: f32,
        boost: bool,
        seconds: f64,
        sun_radius: f64,
    ) {
        self.orientation = (self.orientation
            * DQuat::from_rotation_y(-f64::from(look.x) * 0.0025)
            * DQuat::from_rotation_x(-f64::from(look.y) * 0.0025))
        .normalize();
        self.speed = (self.speed * (f64::from(scroll) * 0.18).exp()).clamp(1.0, 100_000.0);
        let step = self.orientation
            * movement.normalize_or_zero().as_dvec3()
            * self.speed
            * if boost { 10.0 } else { 1.0 }
            * seconds.clamp(0.0, 0.1);
        self.position = stop_at_photosphere(self.position, step, sun_radius + 100.0);
    }
    #[allow(clippy::cast_possible_truncation)] // Camera-only boundary; flight state remains double precision.
    pub fn camera(&self, state: SolarState, radius: f64) -> Transform {
        Transform {
            translation: crate::surface::vector(
                state.inertial_to_local(planet_rs::DVec3::from_array(self.position.to_array()))
                    / radius,
            ),
            rotation: (DQuat::from_array(state.local_to_inertial.conjugate().to_array())
                * self.orientation)
                .as_quat(),
            ..default()
        }
    }
}
fn stop_at_photosphere(start: DVec3, step: DVec3, radius: f64) -> DVec3 {
    let start = if start.length() < radius {
        start.try_normalize().unwrap_or(DVec3::X) * radius
    } else {
        start
    };
    let a = step.length_squared();
    if a > 0.0 {
        let b = start.dot(step);
        let c = (start.length_squared() - radius * radius).max(0.0);
        let discriminant = b * b - a * c;
        if b < 0.0 && discriminant >= 0.0 {
            let t = (-b - discriminant.sqrt()) / a;
            if (0.0..=1.0).contains(&t) {
                return (start + step * t).normalize() * radius;
            }
        }
    }
    start + step
}

#[cfg(test)]
mod tests {
    use super::*;
    use planet_rs::solar::{SolarSettings, SolarSystem};
    #[test]
    fn inertial_flight_stays_fixed_while_the_planet_spins_and_orbits() {
        let system = SolarSystem::new(SolarSettings::default(), 2000.0).unwrap();
        let camera = Transform::from_xyz(1.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y);
        let flight = Flight::enter(&camera, system.state(0.0), 2000.0);
        for t in [0.0, 1000.0, 5000.0] {
            let state = system.state_at(t, t * 48.0);
            let drawn = flight.camera(state, 2000.0);
            let recovered = state.local_to_inertial_position(
                planet_rs::DVec3::from_array(drawn.translation.to_array().map(f64::from)) * 2000.0,
            );
            assert!(
                recovered.distance(planet_rs::DVec3::from_array(flight.position.to_array())) < 0.5
            );
            assert!(
                (DQuat::from_array(state.local_to_inertial.to_array())
                    * DQuat::from_array(drawn.rotation.to_array().map(f64::from)))
                .normalize()
                .angle_between(flight.orientation)
                    < 1e-6
            );
        }
    }
    #[test]
    fn flying_to_the_sun_stops_at_its_surface_even_for_a_step_that_crosses_it() {
        let radius = 200_000.0;
        let start = DVec3::X * 3.0 * radius;
        let end = stop_at_photosphere(start, -DVec3::X * 6.0 * radius, radius + 100.0);
        assert!((end.length() - radius - 100.0).abs() < 1e-6);
        let inward = stop_at_photosphere(end, -DVec3::X * 6.0 * radius, radius + 100.0);
        assert!(inward.distance(end) < 1e-6);
        let away = stop_at_photosphere(end, DVec3::X * 1000.0, radius + 100.0);
        assert!((away.length() - end.length() - 1000.0).abs() < 1e-6);
    }
}
