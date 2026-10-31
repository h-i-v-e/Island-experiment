//! Continental crust is carried by plates, but its outline need not be the
//! Voronoi cell. Overlapping stretched crustal blocks form cratons, peninsulas
//! and thinner offshore fragments. Their source field is independent of meshes.
use super::{Plate, TectonicOptions};
use crate::{
    DVec3,
    terrain::{feature_index::FeatureIndex, random::Random},
};
use glam::DQuat;

pub(super) struct FragmentField {
    fragments: Vec<Fragment>,
    index: FeatureIndex,
    strength: f64,
    spacing: f64,
    transition: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fragment {
    centre: DVec3,
    major: DVec3,
    minor: DVec3,
    radii: [f64; 2],
    thickness: f64,
    support: f64,
    support_cos: f64,
}
impl Fragment {
    fn new(centre: DVec3, major: DVec3, radii: [f64; 2], thickness: f64, spacing: f64) -> Self {
        let major = (major - centre * major.dot(centre)).normalize();
        let support = (radii[0].max(radii[1]) + spacing * 0.6).min(std::f64::consts::PI);
        Self {
            centre,
            major,
            minor: centre.cross(major),
            radii,
            thickness,
            support,
            support_cos: support.cos(),
        }
    }
    fn margin(self, point: DVec3) -> Option<f64> {
        let dot = point.dot(self.centre);
        if dot < self.support_cos {
            return None;
        }
        // A finite great-circle spine gives the stretched block rounded ends.
        // Along distance distinguishes its front from the opposite hemisphere.
        let along = point.dot(self.major).atan2(dot);
        let end = (along.abs() - (self.radii[0] - self.radii[1])).max(0.0);
        let side = point.dot(self.minor).clamp(-1.0, 1.0).asin();
        let margin = self.radii[1] - side.hypot(end);
        Some(margin)
    }
    fn features(self) -> ([DVec3; 3], f64) {
        let angle = self.radii[0] * 0.65;
        (
            [
                self.centre * angle.cos() - self.major * angle.sin(),
                self.centre,
                self.centre * angle.cos() + self.major * angle.sin(),
            ],
            self.radii[0].max(self.radii[1]),
        )
    }
}

impl FragmentField {
    pub fn new(plates: &[Plate], spacing: f64, seed: u64, options: TectonicOptions) -> Self {
        let mut fragments = Vec::new();
        // Pure crust-type planets keep their original interpretation. No target
        // water ratio is imposed and no ocean is manufactured by fragmentation.
        if options.fragmentation > 0.0
            && plates.iter().any(|p| p.continental)
            && plates.iter().any(|p| !p.continental)
        {
            for (id, plate) in plates.iter().enumerate().filter(|(_, p)| p.continental) {
                let id = u64::try_from(id).expect("bounded plate index");
                seed_fragments(
                    &mut fragments,
                    *plate,
                    spacing,
                    seed ^ id.wrapping_mul(0x9e37_79b9),
                    options,
                );
            }
        }
        let index = FeatureIndex::new(fragments.iter().map(|f| (f.centre, f.support)));
        Self {
            fragments,
            index,
            strength: options.fragmentation,
            spacing,
            transition: spacing * 0.045 * options.fragment_scale.sqrt(),
        }
    }
    /// Blend signed geographic margins, not raw heights: thin offshore blocks
    /// can emerge even at intermediate strength, instead of being averaged away.
    pub fn weight(&self, point: DVec3, legacy: f64) -> f64 {
        if self.fragments.is_empty() {
            return legacy;
        }
        let mut margin = -self.spacing * 0.6;
        let mut thickness_sum = 0.0;
        let mut thickness_weight = 0.0;
        for &id in self.index.at(point) {
            let fragment = self.fragments[id];
            if let Some(candidate) = fragment.margin(point) {
                margin = margin.max(candidate);
                let t = ((candidate + self.spacing * 0.6) / (self.spacing * 0.6)).clamp(0.0, 1.0);
                let weight = t * t * (3.0 - 2.0 * t);
                thickness_sum += fragment.thickness * weight;
                thickness_weight += weight;
            }
        }
        // Invert our smoothstep transfer before mixing margins. Using a
        // different transfer for the old plate field would jump at strength0+.
        let inverse = 0.5 - ((1.0 - 2.0 * legacy).clamp(-1.0, 1.0).asin() / 3.0).sin();
        let legacy_margin = (2.0 * inverse - 1.0) * self.transition;
        let margin = legacy_margin * (1.0 - self.strength) + margin * self.strength;
        let t = ((margin + self.transition) / (2.0 * self.transition)).clamp(0.0, 1.0);
        let mask = t * t * (3.0 - 2.0 * t);
        // Smoothly combine crust ages across overlapping blocks so neither
        // height nor hardness jumps when the closest fragment changes.
        let thickness = 1.0 - (thickness_weight - thickness_sum) / thickness_weight.max(1.0);
        let thickness = 1.0 - (1.0 - thickness) * self.strength * mask;
        mask * thickness
    }
    pub fn features(&self) -> impl Iterator<Item = ([DVec3; 3], f64)> + '_ {
        self.fragments.iter().map(|f| f.features())
    }
}

fn rotate(plate: Plate) -> DQuat {
    let angle = plate.omega.length();
    if angle > 0.0 {
        DQuat::from_axis_angle(plate.omega / angle, angle)
    } else {
        DQuat::IDENTITY
    }
}
fn offset(centre: DVec3, direction: DVec3, angle: f64) -> DVec3 {
    (centre * angle.cos() + direction * angle.sin()).normalize()
}
fn seed_fragments(
    fragments: &mut Vec<Fragment>,
    plate: Plate,
    spacing: f64,
    seed: u64,
    options: TectonicOptions,
) {
    let mut random = Random::new(seed ^ 0xd1b5_4a32_d192_ed03);
    let rotation = rotate(plate);
    let bearing = random.direction();
    let major = (bearing - plate.centre * bearing.dot(plate.centre)).normalize_or_zero();
    let minor = plate.centre.cross(major);
    let scale = spacing * options.fragment_scale;
    // Several differently stretched blocks overlap along a crooked continental
    // spine. Narrower lateral pieces leave bays between fingers of thick crust.
    for block in 0..3 {
        let along = (f64::from(block) - 1.0) * scale * 0.24;
        let across = (random.unit() - 0.5) * scale * 0.22;
        let centre = offset(offset(plate.centre, major, along), minor, across);
        let turn = (random.unit() - 0.5) * 1.1;
        let axis = major * turn.cos() + minor * turn.sin();
        let radii = [
            (scale * (0.34 + random.unit() * 0.16)).clamp(0.025, 0.85),
            (scale * (0.18 + random.unit() * 0.13)).clamp(0.015, 0.65),
        ];
        fragments.push(Fragment::new(
            rotation * centre,
            rotation * axis,
            radii,
            0.92 + random.unit() * 0.08,
            spacing,
        ));
    }
    for block in 0..5 {
        let side = if block % 2 == 0 { 1.0 } else { -1.0 };
        let along = (random.unit() - 0.5) * scale * 0.95;
        let across = side * scale * (0.28 + random.unit() * 0.22);
        let centre = offset(offset(plate.centre, major, along), minor, across);
        let turn = side * (0.5 + random.unit() * 0.7);
        let axis = major * turn.cos() + minor * turn.sin();
        let radii = [
            (scale * (0.16 + random.unit() * 0.13)).clamp(0.015, 0.5),
            (scale * (0.045 + random.unit() * 0.065)).clamp(0.008, 0.25),
        ];
        fragments.push(Fragment::new(
            rotation * centre,
            rotation * axis,
            radii,
            0.86 + random.unit() * 0.12,
            spacing,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plates() -> [Plate; 2] {
        [
            Plate {
                centre: DVec3::Z,
                omega: DVec3::Y * 0.2,
                continental: true,
            },
            Plate {
                centre: -DVec3::Z,
                omega: DVec3::ZERO,
                continental: false,
            },
        ]
    }
    #[test]
    fn crustal_blocks_advect_with_their_plate_without_antipodal_copies() {
        let options = TectonicOptions::default();
        let field = FragmentField::new(&plates(), 0.8, 42, options);
        let repeated = FragmentField::new(&plates(), 0.8, 42, options);
        assert_eq!(field.fragments, repeated.fragments);
        let mut stationary = plates();
        stationary[0].omega = DVec3::ZERO;
        let before = FragmentField::new(&stationary, 0.8, 42, options);
        let rotation = rotate(plates()[0]);
        for (a, b) in before.fragments.iter().zip(&field.fragments) {
            assert!((rotation * a.centre).distance(b.centre) < 1e-12);
            assert!((rotation * a.major).distance(b.major) < 1e-12);
            assert!(b.margin(b.centre).unwrap() > 0.0);
            assert!(b.margin(-b.centre).is_none());
        }
        assert_ne!(
            field.fragments,
            FragmentField::new(&plates(), 0.8, 43, options).fragments
        );
    }
    #[test]
    fn disabled_and_uniform_crust_preserve_the_original_field_exactly() {
        let field = FragmentField::new(
            &plates(),
            0.8,
            42,
            TectonicOptions {
                fragmentation: 0.0,
                ..TectonicOptions::default()
            },
        );
        for legacy in [0.0_f64, 0.2, 0.5, 0.83, 1.0] {
            assert_eq!(field.weight(DVec3::Z, legacy).to_bits(), legacy.to_bits());
        }
        assert_eq!(field.features().count(), 0);
        for continental in [true, false] {
            let uniform = plates().map(|mut p| {
                p.continental = continental;
                p
            });
            assert_eq!(
                FragmentField::new(&uniform, 0.8, 42, TectonicOptions::default())
                    .features()
                    .count(),
                0
            );
        }
    }
    #[test]
    fn fragment_thickness_fades_continuously_at_its_outer_support() {
        let fragment = Fragment::new(DVec3::Z, DVec3::X, [0.3, 0.1], 0.86, 0.8);
        let field = FragmentField {
            index: FeatureIndex::new([(fragment.centre, fragment.support)]),
            fragments: vec![fragment],
            strength: 0.05,
            spacing: 0.8,
            transition: 0.036,
        };
        let sample =
            |angle: f64| field.weight(DVec3::Z * angle.cos() + DVec3::X * angle.sin(), 1.0);
        assert!((sample(fragment.support - 1e-7) - sample(fragment.support + 1e-7)).abs() < 1e-5);
    }

    #[test]
    fn lookup_is_conservative_and_weight_remains_continuous_at_cap_edges() {
        let field = FragmentField::new(&plates(), 0.8, 42, TectonicOptions::default());
        let mut random = Random::new(71);
        for _ in 0..4000 {
            let p = random.direction();
            for (id, fragment) in field.fragments.iter().enumerate() {
                if fragment.margin(p).is_some_and(|m| m > -field.spacing * 0.6) {
                    assert!(field.index.at(p).contains(&id));
                }
            }
            let value = field.weight(p, 0.5);
            assert!((0.0..=1.0).contains(&value));
            let nearby = (p + p.any_orthonormal_vector() * 1e-7).normalize();
            assert!((value - field.weight(nearby, 0.5)).abs() < 1e-4);
        }
    }
}
