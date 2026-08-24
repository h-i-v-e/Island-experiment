//! Separated, rounded river stones with a sand/silt gap field.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc
)]

use super::field_program::{
    FieldDimensions, FieldError, LayeredField, fbm, hash_signed, periodic_value, smoothstep,
};
use super::{cellular, periodic};

/// Controls for the rounded river-stone height model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedStonesConfig {
    pub cells_x: u32,
    pub cells_y: u32,
    pub stone_radius: f32,
    pub cell_jitter: f32,
    pub warp_amplitude: f32,
    pub anisotropy: f32,
    pub stone_height: f32,
    pub stone_variation: f32,
    pub gap_height: f32,
    pub sand_amplitude: f32,
    pub edge_softness: f32,
}

impl Default for RoundedStonesConfig {
    fn default() -> Self {
        Self {
            cells_x: 14,
            cells_y: 14,
            stone_radius: 0.36,
            cell_jitter: 0.23,
            warp_amplitude: 0.08,
            anisotropy: 1.0,
            stone_height: 0.12,
            stone_variation: 0.045,
            gap_height: -0.012,
            sand_amplitude: 0.009,
            edge_softness: 0.08,
        }
    }
}

/// Generates authoritative linear f32 heights for a rounded-stone tile.
pub fn generate_height_values(
    config: RoundedStonesConfig,
    dimensions: FieldDimensions,
    seed: u64,
) -> Result<Vec<f32>, FieldError> {
    validate(config)?;
    let mut values = vec![0.0; dimensions.pixel_count()];
    for y in 0..dimensions.height {
        let v = (y as f32 + 0.5) / dimensions.height as f32;
        for x in 0..dimensions.width {
            let u = (x as f32 + 0.5) / dimensions.width as f32;
            let metrics = stone_metrics(config, seed, u, v);
            let cell_bias =
                hash_signed(seed, metrics.cell_x, metrics.cell_y, 0x5354_4f4e_455f_4249)
                    * config.stone_variation;
            let radius_bias =
                hash_signed(seed, metrics.cell_x, metrics.cell_y, 0x5354_4f4e_455f_5241)
                    .mul_add(0.22, 1.0);
            let radius = config.stone_radius * radius_bias;
            let angle = hash_signed(seed, metrics.cell_x, metrics.cell_y, 0x5354_4f4e_455f_414e)
                * std::f32::consts::PI;
            let (sin, cos) = angle.sin_cos();
            let local_x = metrics.local_x * cos - metrics.local_y * sin;
            let local_y = metrics.local_x * sin + metrics.local_y * cos;
            let distance = (local_x * config.anisotropy).hypot(local_y / config.anisotropy);
            let interior = 1.0 - smoothstep(radius, radius + config.edge_softness, distance);
            // A soft dome keeps stones rounded, while the interior mask leaves
            // deliberate, lower gaps rather than filling the entire tile.
            let dome = (1.0 - (distance / radius.max(f32::EPSILON)).powi(2)).clamp(0.0, 1.0);
            let stone = interior * dome;
            let sand = fbm(
                seed.wrapping_add(17),
                u,
                v,
                LayeredField {
                    frequency: 17.0,
                    amplitude: config.sand_amplitude,
                    octaves: 2,
                    lacunarity: 2.2,
                    gain: 0.45,
                    offset: 0.0,
                },
            );
            let broad = periodic_value(
                seed.wrapping_add(19),
                u * 3.0,
                v * 3.0,
                3,
                3,
                0x0053_414e_445f_4252,
            ) * config.sand_amplitude
                * 0.5;
            let height =
                config.gap_height + sand + broad + stone * (config.stone_height + cell_bias);
            values[y as usize * dimensions.width as usize + x as usize] = height;
        }
    }
    Ok(values)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StoneMetrics {
    cell_x: i32,
    cell_y: i32,
    local_x: f32,
    local_y: f32,
}

fn stone_metrics(config: RoundedStonesConfig, seed: u64, u: f32, v: f32) -> StoneMetrics {
    let warp_x = periodic_value(
        seed.wrapping_add(23),
        u * 2.0,
        v * 2.0,
        2,
        2,
        0x5354_4f4e_455f_5758,
    ) * config.warp_amplitude;
    let warp_y = periodic_value(
        seed.wrapping_add(29),
        u * 2.0 - 4.0,
        v * 2.0 + 6.0,
        2,
        2,
        0x5354_4f4e_455f_5759,
    ) * config.warp_amplitude;
    let query_x = (u + warp_x / config.cells_x as f32) * config.cells_x as f32;
    let query_y = (v + warp_y / config.cells_y as f32) * config.cells_y as f32;
    let period =
        periodic::Period2D::new(config.cells_x, config.cells_y).expect("validated cellular period");
    let sample = cellular::sample(seed, [query_x, query_y], period, config.cell_jitter);
    let nearest_cell = (sample.cell_x as i32, sample.cell_y as i32);
    let nearest_origin = (sample.cell_x as f32 + 0.5, sample.cell_y as f32 + 0.5);
    StoneMetrics {
        cell_x: nearest_cell.0,
        cell_y: nearest_cell.1,
        local_x: query_x - nearest_origin.0,
        local_y: query_y - nearest_origin.1,
    }
}

fn validate(config: RoundedStonesConfig) -> Result<(), FieldError> {
    if config.cells_x == 0
        || config.cells_y == 0
        || config.cell_jitter < 0.0
        || config.cell_jitter > 1.0
        || config.warp_amplitude < 0.0
        || config.warp_amplitude > 0.4
        || config.stone_radius <= 0.0
        || config.stone_radius > 1.0
        || config.anisotropy <= 0.0
        || config.stone_height < 0.0
        || config.stone_variation < 0.0
        || config.gap_height > config.stone_height
        || config.sand_amplitude < 0.0
        || config.edge_softness <= 0.0
        || ![
            config.stone_radius,
            config.cell_jitter,
            config.warp_amplitude,
            config.anisotropy,
            config.stone_height,
            config.stone_variation,
            config.gap_height,
            config.sand_amplitude,
            config.edge_softness,
        ]
        .iter()
        .all(|value| value.is_finite())
    {
        return Err(FieldError::NonFiniteParameter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dimensions() -> FieldDimensions {
        FieldDimensions::new(72, 72, 2.0, 2.0).expect("dimensions")
    }

    #[test]
    fn rounded_stones_are_deterministic_and_have_gaps() {
        let config = RoundedStonesConfig::default();
        let a = generate_height_values(config, dimensions(), 7).expect("height");
        let b = generate_height_values(config, dimensions(), 7).expect("height");
        assert_eq!(a, b);
        let min = a.iter().copied().fold(f32::INFINITY, f32::min);
        let max = a.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(min < 0.0);
        assert!(max > 0.04);
    }

    #[test]
    fn changing_seed_changes_the_stochastic_layout() {
        let config = RoundedStonesConfig::default();
        let first = generate_height_values(config, dimensions(), 1).expect("height");
        let second = generate_height_values(config, dimensions(), 2).expect("height");
        assert_ne!(first, second);
    }

    #[test]
    fn invalid_anisotropy_is_rejected() {
        assert_eq!(
            generate_height_values(
                RoundedStonesConfig {
                    anisotropy: 0.0,
                    ..RoundedStonesConfig::default()
                },
                dimensions(),
                5,
            ),
            Err(FieldError::NonFiniteParameter)
        );
    }
}
