#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::fmt;

const MIN_RESOLUTION: u32 = 32;
const MAX_RESOLUTION: u32 = 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudWeatherError {
    InvalidResolution(u32),
    AllocationOverflow,
}

impl fmt::Display for CloudWeatherError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidResolution(resolution) => write!(
                formatter,
                "cloud weather-map resolution must be a power of two from {MIN_RESOLUTION} to {MAX_RESOLUTION}, got {resolution}"
            ),
            Self::AllocationOverflow => {
                formatter.write_str("cloud weather-map allocation size overflowed")
            }
        }
    }
}

impl std::error::Error for CloudWeatherError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudWeatherMap {
    resolution: u32,
    rgba: Vec<u8>,
}

impl CloudWeatherMap {
    #[must_use]
    pub const fn resolution(&self) -> u32 {
        self.resolution
    }

    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn lattice(seed: u64, x: i32, y: i32, period: i32) -> f32 {
    let wrapped_x = x.rem_euclid(period);
    let wrapped_y = y.rem_euclid(period);
    let key = seed
        ^ u64::from(wrapped_x.cast_unsigned()).wrapping_mul(0x9e37_79b9)
        ^ u64::from(wrapped_y.cast_unsigned()).wrapping_mul(0x85eb_ca6b);
    let value = (mix(key) >> 40) as u32;
    value as f32 / 16_777_215.0
}

fn fade(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, amount: f32) -> f32 {
    (b - a).mul_add(amount, a)
}

fn periodic_value(seed: u64, x: f32, y: f32, period: i32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = fade(x - x0 as f32);
    let ty = fade(y - y0 as f32);
    let top = lerp(
        lattice(seed, x0, y0, period),
        lattice(seed, x0 + 1, y0, period),
        tx,
    );
    let bottom = lerp(
        lattice(seed, x0, y0 + 1, period),
        lattice(seed, x0 + 1, y0 + 1, period),
        tx,
    );
    lerp(top, bottom, ty)
}

fn periodic_fractal(seed: u64, u: f32, v: f32, base_period: i32, octaves: u8) -> f32 {
    let mut period = base_period;
    let mut amplitude = 1.0;
    let mut total = 0.0;
    let mut weight = 0.0;
    for octave in 0..octaves {
        total += periodic_value(
            seed.wrapping_add(u64::from(octave) * 0x9e37_79b9),
            u * period as f32,
            v * period as f32,
            period,
        ) * amplitude;
        weight += amplitude;
        period *= 2;
        amplitude *= 0.5;
    }
    total / weight
}

fn encode_unit(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Generates a deterministic seamless RGBA cloud weather field.
///
/// Every channel is periodic over the complete image. Samples are taken at
/// texel centres so repeat-wrapped bilinear filtering crosses the seam without
/// a duplicated edge row or column.
///
/// # Errors
///
/// Returns [`CloudWeatherError::InvalidResolution`] unless `resolution` is a
/// power of two between 32 and 1024 inclusive, or
/// [`CloudWeatherError::AllocationOverflow`] if the requested buffer size
/// cannot be represented on the current platform.
pub fn generate_cloud_weather_map(
    seed: u64,
    resolution: u32,
) -> Result<CloudWeatherMap, CloudWeatherError> {
    if !(MIN_RESOLUTION..=MAX_RESOLUTION).contains(&resolution) || !resolution.is_power_of_two() {
        return Err(CloudWeatherError::InvalidResolution(resolution));
    }
    let texel_count = usize::try_from(resolution)
        .ok()
        .and_then(|side| side.checked_mul(side))
        .ok_or(CloudWeatherError::AllocationOverflow)?;
    let byte_count = texel_count
        .checked_mul(4)
        .ok_or(CloudWeatherError::AllocationOverflow)?;
    let mut rgba = Vec::with_capacity(byte_count);
    let inverse_resolution = 1.0 / resolution as f32;
    for y in 0..resolution {
        let v = (y as f32 + 0.5) * inverse_resolution;
        for x in 0..resolution {
            let u = (x as f32 + 0.5) * inverse_resolution;
            let broad = periodic_fractal(seed ^ 0x243f_6a88, u, v, 3, 4);
            let medium = periodic_fractal(seed ^ 0x85a3_08d3, u, v, 8, 3);
            let detail = periodic_fractal(seed ^ 0x1319_8a2e, u, v, 24, 3);
            let regional = periodic_fractal(seed ^ 0x0370_7344, u, v, 2, 3);
            rgba.extend([
                encode_unit(broad),
                encode_unit(medium),
                encode_unit(detail),
                encode_unit(regional),
            ]);
        }
    }
    debug_assert_eq!(rgba.len(), byte_count);
    Ok(CloudWeatherMap { resolution, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_map_is_deterministic_and_seeded() {
        let first = generate_cloud_weather_map(42, 64).unwrap();
        let repeated = generate_cloud_weather_map(42, 64).unwrap();
        let different = generate_cloud_weather_map(43, 64).unwrap();

        assert_eq!(first, repeated);
        assert_ne!(first.rgba(), different.rgba());
    }

    #[test]
    fn weather_map_has_expected_size_and_varying_channels() {
        let weather = generate_cloud_weather_map(9, 64).unwrap();
        assert_eq!(weather.resolution(), 64);
        assert_eq!(weather.rgba().len(), 64 * 64 * 4);

        for channel in 0..4 {
            let values = weather.rgba().iter().skip(channel).step_by(4);
            let (minimum, maximum) = values.fold((u8::MAX, u8::MIN), |range, value| {
                (range.0.min(*value), range.1.max(*value))
            });
            assert!(maximum.saturating_sub(minimum) > 32);
        }
    }

    #[test]
    fn periodic_field_matches_after_complete_wrap() {
        for (seed, period) in [(1, 3), (17, 8), (99, 24)] {
            let sample = periodic_value(seed, 1.375, 2.625, period);
            assert!(
                (sample - periodic_value(seed, 1.375 + period as f32, 2.625, period)).abs()
                    < 1.0e-6
            );
            assert!(
                (sample - periodic_value(seed, 1.375, 2.625 - period as f32, period)).abs()
                    < 1.0e-6
            );
        }
    }

    #[test]
    fn invalid_resolutions_are_rejected() {
        for resolution in [0, 31, 48, 2_048] {
            assert_eq!(
                generate_cloud_weather_map(1, resolution),
                Err(CloudWeatherError::InvalidResolution(resolution))
            );
        }
    }
}
