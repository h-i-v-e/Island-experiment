//! Seeded impacts arrive before terrain stages. Analytic profiles are retained
//! for midpoint residuals, so refinement resolves bowls without undoing erosion.
use super::random::Random;
use crate::{DVec3, PlanetError, PlanetMesh};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ImpactOptions {
    pub enabled: bool,
    pub per_stage: u32,
    pub size: f64,
    pub strength: f64,
}
impl Default for ImpactOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            per_stage: 4,
            size: 1.0,
            strength: 1.0,
        }
    }
}
impl ImpactOptions {
    pub(super) fn validate(self) -> Result<(), PlanetError> {
        if self.per_stage > 24
            || !self.size.is_finite()
            || !(0.25..=3.0).contains(&self.size)
            || !self.strength.is_finite()
            || !(0.0..=3.0).contains(&self.strength)
        {
            return Err(PlanetError::InvalidOptions(
                "impacts need 0..24 per stage, size 0.25..3 and strength 0..3",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpactCrater {
    pub centre: DVec3,
    pub angular_radius: f64,
    pub stage: u32,
    pub reference_height: f64,
    pub strength: f64,
    outer_cos: f64,
}
impl ImpactCrater {
    fn sample(self, p: DVec3) -> f64 {
        let dot = self.centre.dot(p);
        if dot < self.outer_cos {
            return 0.0;
        }
        let distance = self.centre.cross(p).length().atan2(dot) / self.angular_radius;
        crater_profile(distance) * self.angular_radius * self.strength
    }
    pub(super) fn observable(self) -> bool {
        self.reference_height + self.angular_radius * self.strength * 0.06 >= -0.01
    }
}

fn bump(x: f64) -> f64 {
    if x.abs() >= 1.0 {
        0.0
    } else {
        (1.0 - x * x).powi(2)
    }
}
fn crater_profile(radius: f64) -> f64 {
    let bowl = if radius < 1.0 {
        -0.18 * (1.0 - radius * radius).powi(2)
    } else {
        0.0
    };
    bowl + 0.04 * bump((radius - 1.0) / 0.22) + 0.008 * bump((radius - 1.25) / 0.55)
}

pub(super) struct ImpactField {
    seed: u64,
    options: ImpactOptions,
    base_radius: f64,
    craters: Vec<ImpactCrater>,
}
impl ImpactField {
    pub fn new(seed: u64, options: ImpactOptions, vertices: usize) -> Self {
        let count = f64::from(u32::try_from(vertices).expect("validated initial count"));
        Self {
            seed,
            options,
            base_radius: (4.0 * std::f64::consts::PI / count).sqrt() * 2.2,
            craters: Vec::new(),
        }
    }
    pub fn add_stage(&mut self, mesh: &PlanetMesh, heights: &mut [f64], stage: u32) {
        if !self.options.enabled || self.options.per_stage == 0 || self.options.strength == 0.0 {
            return;
        }
        let first = self.craters.len();
        let mut random = Random::new(
            self.seed ^ 0x87cd_61a4_237e_9b5f ^ u64::from(stage).wrapping_mul(0x9e37_79b9),
        );
        for _ in 0..self.options.per_stage {
            let target = random.direction();
            // Uniform spherical targets snapped onto current vertices avoid
            // concentrating impacts in densely tessellated river corridors.
            let index = mesh
                .directions
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.dot(target).total_cmp(&b.1.dot(target)))
                .map(|(i, _)| i)
                .expect("terrain vertices");
            let radius = (self.base_radius * self.options.size * (0.6 + random.unit() * 0.9)
                / 2.0_f64.powf(f64::from(stage)))
            .clamp(0.0002, 0.3);
            self.craters.push(ImpactCrater {
                centre: mesh.directions[index],
                angular_radius: radius,
                stage,
                reference_height: heights[index],
                strength: self.options.strength,
                outer_cos: (radius * 1.8).cos(),
            });
        }
        for (p, height) in mesh.directions.iter().zip(heights) {
            *height += self.sample(*p) - sample_craters(&self.craters[..first], *p);
        }
    }
    pub fn sample(&self, p: DVec3) -> f64 {
        sample_craters(&self.craters, p)
    }
    pub fn features(&self) -> impl Iterator<Item = (DVec3, f64)> + '_ {
        self.craters
            .iter()
            .filter(|c| c.observable())
            .map(|c| (c.centre, c.angular_radius * 1.8))
    }
    pub fn into_craters(self) -> Vec<ImpactCrater> {
        self.craters
    }
}
fn sample_craters(craters: &[ImpactCrater], p: DVec3) -> f64 {
    // Overlapping impacts remain bounded even on small test spheres.
    (craters.iter().map(|c| c.sample(p)).sum::<f64>() / 0.18).tanh() * 0.18
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlanetOptions, generate};
    #[test]
    fn impact_profile_has_a_lower_floor_raised_rim_and_smooth_outer_limit() {
        assert!(crater_profile(0.0) < -0.17);
        assert!(crater_profile(1.0) > 0.04);
        assert!(crater_profile(1.8).abs() < 1.0e-12);
        assert!((crater_profile(1.8 - 1.0e-6) - crater_profile(1.8)).abs() < 1.0e-9);
    }
    #[test]
    fn batches_are_seeded_shrink_with_stage_and_do_not_reapply_old_impacts() {
        let mesh = generate(PlanetOptions {
            vertex_count: 1024,
            ..PlanetOptions::default()
        })
        .unwrap();
        let options = ImpactOptions {
            enabled: true,
            ..ImpactOptions::default()
        };
        let mut field = ImpactField::new(42, options, mesh.directions.len());
        let mut same = ImpactField::new(42, options, mesh.directions.len());
        let mut heights = vec![0.1; mesh.directions.len()];
        let mut other = heights.clone();
        for stage in 0..4 {
            field.add_stage(&mesh, &mut heights, stage);
            same.add_stage(&mesh, &mut other, stage);
        }
        assert_eq!(field.craters, same.craters);
        assert_eq!(heights, other);
        assert!(field.craters[12].angular_radius < field.craters[0].angular_radius);
        for (&p, &h) in mesh.directions.iter().zip(&heights) {
            assert!((h - 0.1 - field.sample(p)).abs() < 1.0e-12);
        }
    }
}
