//! Three fixed settings are borrowed while sampling; no per-site allocations.
use crate::{DVec3, noise};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NoiseKind {
    Fractal,
    Ridged,
    Voronoi,
}

impl NoiseKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fractal => "Fractal",
            Self::Ridged => "Ridged fractal",
            Self::Voronoi => "Voronoi plates",
        }
    }
}

impl std::str::FromStr for NoiseKind {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "fractal" => Ok(Self::Fractal),
            "ridged" => Ok(Self::Ridged),
            "voronoi" => Ok(Self::Voronoi),
            _ => Err("noise type must be fractal, ridged or voronoi"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoiseLayer {
    pub kind: NoiseKind,
    /// Cartesian frequency on the unit sphere; higher values make smaller features.
    pub frequency: f64,
    /// Relative height contribution. Zero disables this layer; weights normalize.
    pub weight: f64,
}

impl NoiseLayer {
    #[must_use]
    pub const fn classic() -> [Self; 3] {
        [
            Self {
                kind: NoiseKind::Fractal,
                frequency: 2.2,
                weight: 0.78,
            },
            Self {
                kind: NoiseKind::Fractal,
                frequency: 12.0,
                weight: 0.22,
            },
            Self {
                kind: NoiseKind::Voronoi,
                frequency: 3.0,
                weight: 0.0,
            },
        ]
    }

    #[must_use]
    pub const fn plates() -> [Self; 3] {
        [
            Self {
                kind: NoiseKind::Fractal,
                frequency: 2.2,
                weight: 0.58,
            },
            Self {
                kind: NoiseKind::Fractal,
                frequency: 12.0,
                weight: 0.17,
            },
            Self {
                kind: NoiseKind::Voronoi,
                frequency: 3.0,
                weight: 0.25,
            },
        ]
    }
}

pub(super) fn sample(seed: u64, direction: DVec3, layers: &[NoiseLayer; 3]) -> f64 {
    const SEEDS: [u64; 3] = [
        0x243f_6a88_85a3_08d3,
        0x1319_8a2e_0370_7344,
        0xa409_3822_299f_31d0,
    ];
    let total: f64 = layers.iter().map(|layer| layer.weight).sum();
    if total == 0.0 {
        return 0.0;
    }
    layers
        .iter()
        .zip(SEEDS)
        .enumerate()
        .filter(|(_, (layer, _))| layer.weight > 0.0)
        .map(|(i, (layer, salt))| {
            let point = direction * layer.frequency;
            let value = match layer.kind {
                NoiseKind::Fractal => {
                    noise::fractal(seed ^ salt, point, if i == 0 { 5 } else { 4 })
                }
                NoiseKind::Ridged => 1.0 - 2.0 * noise::fractal(seed ^ salt, point, 5).abs(),
                NoiseKind::Voronoi => noise::voronoi_plates(seed ^ salt, point),
            };
            layer.weight * value
        })
        .sum::<f64>()
        / total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_keeps_existing_heights_and_weights_blend_disabled_layers_safely() {
        for seed in [0, 42, u64::MAX] {
            for p in [DVec3::X, -DVec3::Y, DVec3::new(0.3, 0.5, 0.8).normalize()] {
                let expected = 0.78 * noise::fractal(seed ^ 0x243f_6a88_85a3_08d3, p * 2.2, 5)
                    + 0.22 * noise::fractal(seed ^ 0x1319_8a2e_0370_7344, p * 12.0, 4);
                assert_eq!(
                    sample(seed, p, &NoiseLayer::classic()).to_bits(),
                    expected.to_bits()
                );
                let mut layers = NoiseLayer::plates();
                let first = sample(seed, p, &layers);
                for layer in &mut layers {
                    layer.weight *= 2.0;
                }
                assert!((sample(seed, p, &layers) - first).abs() < 1e-14);
                for layer in &mut layers {
                    layer.weight = 0.0;
                }
                assert_eq!(sample(seed, p, &layers).to_bits(), 0.0_f64.to_bits());
            }
        }
    }

    #[test]
    fn every_layer_type_is_seeded_bounded_and_continuous_at_poles_and_seam() {
        for kind in [NoiseKind::Fractal, NoiseKind::Ridged, NoiseKind::Voronoi] {
            let layers = [NoiseLayer {
                kind,
                frequency: 3.0,
                weight: 1.0,
            }; 3];
            let values: Vec<_> = (0..512)
                .map(|i| {
                    let y = 1.0 - (f64::from(i) + 0.5) / 256.0;
                    let angle = f64::from(i) * 2.399_963_229_728_653;
                    let r = (1.0 - y * y).sqrt();
                    let p = DVec3::new(r * angle.cos(), y, r * angle.sin());
                    let value = sample(42, p, &layers);
                    assert!(value.is_finite() && (-1.0..=1.0).contains(&value));
                    assert_eq!(value.to_bits(), sample(42, p, &layers).to_bits());
                    value
                })
                .collect();
            assert!(
                values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                    - values.iter().copied().fold(f64::INFINITY, f64::min)
                    > 0.3
            );
            for p in [DVec3::Y, -DVec3::Y, -DVec3::X] {
                let a = sample(42, (p + DVec3::Z * 1e-8).normalize(), &layers);
                let b = sample(42, (p - DVec3::Z * 1e-8).normalize(), &layers);
                assert!((a - b).abs() < 1e-5, "{kind:?} tears at {p:?}");
                assert!((sample(43, p, &layers) - sample(42, p, &layers)).abs() > 1e-6);
            }
        }
    }
}
