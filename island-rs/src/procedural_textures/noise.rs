//! Periodic scalar noise primitives used by texture recipes.
//!
//! The functions in this module do not allocate and have no global random
//! state.  A caller supplies both a seed domain and a lattice period, making
//! repeated bakes independent of thread scheduling and output resolution.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::too_many_arguments
)]

use super::periodic::{self, Period2D};

/// Safety limit shared by recipe validation and fractal samplers.
pub const MAX_OCTAVES: u8 = 16;

/// Parameters shared by fBM, billow and ridged noise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FractalParameters {
    /// Number of source fields to sum.
    pub octaves: u8,
    /// Frequency multiplier between successive octaves.
    pub lacunarity: f32,
    /// Amplitude multiplier between successive octaves.
    pub gain: f32,
}

impl Default for FractalParameters {
    fn default() -> Self {
        Self {
            octaves: 5,
            lacunarity: 2.0,
            gain: 0.5,
        }
    }
}

impl FractalParameters {
    /// Creates checked fractal parameters.
    pub fn new(octaves: u8, lacunarity: f32, gain: f32) -> Result<Self, NoiseParameterError> {
        let parameters = Self {
            octaves,
            lacunarity,
            gain,
        };
        parameters.validate()?;
        Ok(parameters)
    }

    /// Validates the values that would otherwise make a sampler undefined.
    pub fn validate(self) -> Result<(), NoiseParameterError> {
        if self.octaves == 0 {
            return Err(NoiseParameterError::ZeroOctaves);
        }
        if self.octaves > MAX_OCTAVES {
            return Err(NoiseParameterError::TooManyOctaves {
                found: self.octaves,
                maximum: MAX_OCTAVES,
            });
        }
        if !self.lacunarity.is_finite() || !self.gain.is_finite() {
            return Err(NoiseParameterError::NonFiniteParameter);
        }
        if self.lacunarity <= 0.0 {
            return Err(NoiseParameterError::NonPositiveLacunarity);
        }
        Ok(())
    }
}

/// Errors returned by [`FractalParameters::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoiseParameterError {
    /// At least one octave is needed for a fractal field.
    ZeroOctaves,
    /// The sampler safety limit would be exceeded.
    TooManyOctaves { found: u8, maximum: u8 },
    /// A lacunarity or gain was NaN or infinite.
    NonFiniteParameter,
    /// Frequencies must move forward through the field.
    NonPositiveLacunarity,
}

/// Samples periodic interpolated value noise.
#[inline]
pub fn value(seed: u64, position: [f32; 2], period: Period2D) -> f32 {
    periodic::value_noise(seed, position, period)
}

/// Alias with an explicit sampler-oriented name.
#[inline]
pub fn sample_value(seed: u64, position: [f32; 2], period: Period2D) -> f32 {
    value(seed, position, period)
}

/// Samples normalized fractal Brownian motion.
pub fn fbm(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    octaves: u8,
    lacunarity: f32,
    gain: f32,
) -> f32 {
    sample_fractal(
        seed,
        position,
        period,
        FractalParameters {
            octaves,
            lacunarity,
            gain,
        },
        FractalShape::Value,
    )
}

/// Samples fBM with [`FractalParameters`].
#[inline]
pub fn fbm_with_parameters(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    parameters: FractalParameters,
) -> f32 {
    sample_fractal(seed, position, period, parameters, FractalShape::Value)
}

/// Samples billow noise (`abs(fBM)` remapped to `[-1, 1]`).
pub fn billow(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    octaves: u8,
    lacunarity: f32,
    gain: f32,
) -> f32 {
    sample_fractal(
        seed,
        position,
        period,
        FractalParameters {
            octaves,
            lacunarity,
            gain,
        },
        FractalShape::Billow,
    )
}

/// Samples billow noise with [`FractalParameters`].
#[inline]
pub fn billow_with_parameters(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    parameters: FractalParameters,
) -> f32 {
    sample_fractal(seed, position, period, parameters, FractalShape::Billow)
}

/// Samples ridged noise (`1 - abs(fBM)` remapped to `[-1, 1]`).
pub fn ridged(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    octaves: u8,
    lacunarity: f32,
    gain: f32,
) -> f32 {
    sample_fractal(
        seed,
        position,
        period,
        FractalParameters {
            octaves,
            lacunarity,
            gain,
        },
        FractalShape::Ridged,
    )
}

/// Samples ridged noise with [`FractalParameters`].
#[inline]
pub fn ridged_with_parameters(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    parameters: FractalParameters,
) -> f32 {
    sample_fractal(seed, position, period, parameters, FractalShape::Ridged)
}

/// Warps a periodic coordinate using two independent periodic fBM fields.
///
/// `frequency` is expressed in source lattice cells.  Integer frequencies
/// preserve the declared period exactly; the helper rounds non-integer
/// frequencies to the nearest positive lattice frequency so it remains
/// seamless rather than introducing a fractional lattice seam.
pub fn domain_warp(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    amplitude: f32,
    frequency: f32,
    octaves: u8,
    lacunarity: f32,
    gain: f32,
) -> [f32; 2] {
    if !amplitude.is_finite() || !frequency.is_finite() || frequency <= 0.0 {
        return position;
    }

    let lattice_frequency = positive_lattice_frequency(frequency);
    let warped_period = period_for_frequency(period, lattice_frequency);
    let warped_position = [
        position[0] * lattice_frequency as f32,
        position[1] * lattice_frequency as f32,
    ];
    let parameters = FractalParameters {
        octaves,
        lacunarity,
        gain,
    };
    let x = fbm_with_parameters(
        seed ^ 0x68bc_21eb_8f5a_7c13,
        [warped_position[0] + 19.19, warped_position[1] - 7.31],
        warped_period,
        parameters,
    );
    let y = fbm_with_parameters(
        seed ^ 0x9e37_79b9_7f4a_7c15,
        [warped_position[0] - 41.73, warped_position[1] + 13.57],
        warped_period,
        parameters,
    );
    [position[0] + x * amplitude, position[1] + y * amplitude]
}

/// Samples value noise after applying [`domain_warp`].
#[inline]
pub fn warped_value(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    amplitude: f32,
    frequency: f32,
    octaves: u8,
    lacunarity: f32,
    gain: f32,
) -> f32 {
    let warped = domain_warp(
        seed, position, period, amplitude, frequency, octaves, lacunarity, gain,
    );
    value(seed, warped, period)
}

/// Samples fBM after applying [`domain_warp`].
#[inline]
pub fn warped_fbm(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    warp_amplitude: f32,
    warp_frequency: f32,
    warp_octaves: u8,
    warp_lacunarity: f32,
    warp_gain: f32,
    source: FractalParameters,
) -> f32 {
    let warped = domain_warp(
        seed,
        position,
        period,
        warp_amplitude,
        warp_frequency,
        warp_octaves,
        warp_lacunarity,
        warp_gain,
    );
    fbm_with_parameters(seed, warped, period, source)
}

#[derive(Clone, Copy)]
enum FractalShape {
    Value,
    Billow,
    Ridged,
}

fn sample_fractal(
    seed: u64,
    position: [f32; 2],
    period: Period2D,
    parameters: FractalParameters,
    shape: FractalShape,
) -> f32 {
    if parameters.octaves == 0
        || !parameters.lacunarity.is_finite()
        || !parameters.gain.is_finite()
        || parameters.lacunarity <= 0.0
    {
        return 0.0;
    }

    let mut frequency = 1.0_f32;
    let mut amplitude = 1.0_f32;
    let mut total = 0.0_f32;
    let mut weight = 0.0_f32;

    for octave in 0..parameters.octaves {
        let lattice_frequency = positive_lattice_frequency(frequency);
        let sample_period = period_for_frequency(period, lattice_frequency);
        let sample_position = [
            position[0] * lattice_frequency as f32,
            position[1] * lattice_frequency as f32,
        ];
        let mut source = value(
            seed.wrapping_add(u64::from(octave).wrapping_mul(0x9e37_79b9_7f4a_7c15)),
            sample_position,
            sample_period,
        );
        source = match shape {
            FractalShape::Value => source,
            FractalShape::Billow => source.abs() * 2.0 - 1.0,
            FractalShape::Ridged => (1.0 - source.abs()) * 2.0 - 1.0,
        };
        total += source * amplitude;
        weight += amplitude.abs();
        frequency *= parameters.lacunarity;
        amplitude *= parameters.gain;
    }

    if weight > f32::EPSILON {
        total / weight
    } else {
        0.0
    }
}

fn positive_lattice_frequency(frequency: f32) -> u32 {
    if !frequency.is_finite() || frequency <= 0.0 {
        return 1;
    }
    frequency.round().max(1.0).min(u32::MAX as f32) as u32
}

fn period_for_frequency(period: Period2D, frequency: u32) -> Period2D {
    period.checked_mul(frequency).unwrap_or(period)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{FractalParameters, billow, domain_warp, fbm, ridged};
    use crate::procedural_textures::periodic::Period2D;

    fn test_period() -> Period2D {
        Period2D::new(23, 19).expect("non-empty period")
    }

    #[test]
    fn fractal_parameters_reject_unsafe_values() {
        assert!(FractalParameters::new(0, 2.0, 0.5).is_err());
        assert!(FractalParameters::new(17, 2.0, 0.5).is_err());
        assert!(FractalParameters::new(4, f32::NAN, 0.5).is_err());
    }

    #[test]
    fn all_fractal_shapes_are_periodic() {
        let period = test_period();
        let point = [4.375, 12.625];
        for sample in [
            fbm(12, point, period, 5, 2.0, 0.5),
            billow(12, point, period, 5, 2.0, 0.5),
            ridged(12, point, period, 5, 2.0, 0.5),
        ] {
            assert!(sample.is_finite());
        }
        assert_eq!(
            fbm(12, point, period, 5, 2.0, 0.5),
            fbm(
                12,
                [point[0] + period.x as f32, point[1]],
                period,
                5,
                2.0,
                0.5
            )
        );
        assert_eq!(
            billow(12, point, period, 5, 2.0, 0.5),
            billow(
                12,
                [point[0], point[1] + period.y as f32],
                period,
                5,
                2.0,
                0.5
            )
        );
        assert_eq!(
            ridged(12, point, period, 5, 2.0, 0.5),
            ridged(
                12,
                [point[0] + period.x as f32, point[1]],
                period,
                5,
                2.0,
                0.5
            )
        );
    }

    #[test]
    fn domain_warp_is_periodic() {
        let period = test_period();
        let point = [2.25, 8.75];
        let original = domain_warp(77, point, period, 0.35, 2.0, 3, 2.0, 0.5);
        let wrapped_x = domain_warp(
            77,
            [point[0] + period.x as f32, point[1]],
            period,
            0.35,
            2.0,
            3,
            2.0,
            0.5,
        );
        let wrapped_y = domain_warp(
            77,
            [point[0], point[1] + period.y as f32],
            period,
            0.35,
            2.0,
            3,
            2.0,
            0.5,
        );
        assert!((original[0] - (wrapped_x[0] - period.x as f32)).abs() < 1.0e-5);
        assert!((original[1] - wrapped_x[1]).abs() < 1.0e-5);
        assert!((original[0] - wrapped_y[0]).abs() < 1.0e-5);
        assert!((original[1] - (wrapped_y[1] - period.y as f32)).abs() < 1.0e-5);
    }
}
