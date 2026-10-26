//! Four small, seeded random walks, independent of camera position and frame rate.
use bevy::prelude::{Vec3, Vec4};
use planet_rs::DVec3;

const STEP: f64 = 1.0 / 60.0;

struct Random(u64);
impl Random {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut bits = self.0;
        bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        f64::from(u32::try_from((bits ^ (bits >> 31)) >> 32).unwrap()) / 4_294_967_296.0
    }
    fn direction(&mut self) -> DVec3 {
        let y = self.unit().mul_add(2.0, -1.0);
        let angle = self.unit() * std::f64::consts::TAU;
        let radius = (1.0 - y * y).sqrt();
        DVec3::new(radius * angle.cos(), y, radius * angle.sin())
    }
}

struct Source {
    direction: DVec3,
    heading: DVec3,
    turn: f64,
    target: f64,
    remaining: f64,
    phase: f32,
}
pub(super) struct WaveSources {
    pub seed: u64,
    random: Random,
    points: [Source; 4],
    remainder: f64,
}
impl WaveSources {
    #[allow(clippy::cast_possible_truncation)]
    pub fn new(seed: u64) -> Self {
        let mut random = Random(seed ^ 0x5741_5645_534f_5552);
        let points = std::array::from_fn(|_| {
            let direction = random.direction();
            let side = direction.any_orthonormal_vector();
            let angle = random.unit() * std::f64::consts::TAU;
            Source {
                direction,
                heading: side * angle.cos() + direction.cross(side) * angle.sin(),
                turn: 0.0,
                target: random.unit().mul_add(4.0, -2.0),
                remaining: random.unit().mul_add(40.0, 20.0),
                phase: (random.unit() * std::f64::consts::TAU) as f32,
            }
        });
        Self {
            seed,
            random,
            points,
            remainder: 0.0,
        }
    }

    pub fn advance(&mut self, seconds: f64, speed_mps: f64, radius: f64) {
        if seconds <= 0.0 || speed_mps <= 0.0 {
            return;
        }
        self.remainder += seconds;
        let angle = speed_mps * STEP / radius;
        let (sin, cos) = angle.sin_cos();
        while self.remainder + 1e-12 >= STEP {
            self.remainder = (self.remainder - STEP).max(0.0);
            for point in &mut self.points {
                point.remaining -= STEP;
                if point.remaining <= 0.0 {
                    point.target = self.random.unit().mul_add(4.0, -2.0);
                    point.remaining = self.random.unit().mul_add(40.0, 20.0);
                }
                point.turn += (point.target - point.turn) * (1.0 - (-STEP / 4.0).exp());
                // Curvature scales with travel around the globe. A fixed turn
                // rate per second would trap slow sources in tiny local circles.
                let (steer_sin, steer_cos) = (point.turn * angle).sin_cos();
                let heading =
                    point.heading * steer_cos + point.direction.cross(point.heading) * steer_sin;
                let direction = (point.direction * cos + heading * sin).normalize();
                let forward = heading * cos - point.direction * sin;
                point.direction = direction;
                point.heading = (forward - direction * direction.dot(forward)).normalize();
            }
        }
    }
    pub fn packed(&self) -> [Vec4; 4] {
        std::array::from_fn(|i| {
            Vec3::from_array(self.points[i].direction.as_vec3().to_array())
                .extend(self.points[i].phase)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn walks_are_seeded_smooth_and_frame_rate_independent() {
        let mut fine = WaveSources::new(42);
        let mut coarse = WaveSources::new(42);
        assert_ne!(fine.packed(), WaveSources::new(43).packed());
        let start = fine.packed();
        for _ in 0..7200 {
            fine.advance(1.0 / 60.0, 0.35, 1000.0);
        }
        for _ in 0..1200 {
            coarse.advance(0.1, 0.35, 1000.0);
        }
        assert_eq!(fine.packed(), coarse.packed());
        for (before, point) in start.iter().zip(&fine.points) {
            assert!((point.direction.length() - 1.0).abs() < 1e-12);
            assert!(point.heading.dot(point.direction).abs() < 1e-12);
            assert!(
                point.direction.distance(DVec3::from_array(
                    before.truncate().to_array().map(f64::from)
                )) > 0.001
            );
        }
        let stopped = fine.packed();
        fine.advance(100.0, 0.0, 1000.0);
        fine.advance(0.0, 1.0, 1000.0);
        assert_eq!(stopped, fine.packed());
        fine.advance(0.1, 0.35, 1000.0);
        for (a, b) in stopped.iter().zip(fine.packed()) {
            assert!(a.truncate().distance(b.truncate()) < 0.00004);
        }
    }
    #[test]
    fn drift_uses_physical_metres_on_different_planet_radii() {
        let mut small = WaveSources::new(17);
        let mut large = WaveSources::new(17);
        let start = small.points[0].direction;
        small.advance(1.0 / 60.0, 1.0, 100.0);
        large.advance(1.0 / 60.0, 1.0, 1000.0);
        for (walk, radius) in [(small, 100.0), (large, 1000.0)] {
            let n = walk.points[0].direction;
            let distance = start.cross(n).length().atan2(start.dot(n)) * radius;
            assert!((distance - STEP).abs() < 1e-10);
        }
    }
    #[test]
    fn sources_travel_globally_instead_of_circling_one_small_patch() {
        let mut walk = WaveSources::new(42);
        let origins = walk.points.each_ref().map(|p| p.direction);
        walk.advance(1200.0, 1.0, 1000.0);
        for (origin, point) in origins.iter().zip(&walk.points) {
            let angle = origin
                .cross(point.direction)
                .length()
                .atan2(origin.dot(point.direction));
            assert!(angle > 0.6, "wave source stuck near its start: {angle}");
        }
    }
}
