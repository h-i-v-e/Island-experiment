//! Irregular connected slabs, bevelled cracks and per-slab variation.

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

/// Controls for the cracked-stone height model.
///
/// Frequencies are expressed in cells per physical tile, while all relief
/// values are metres. Keeping this configuration independent of an image type
/// lets recipe adapters feed the same model from JSON, Unity or Bevy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrackedStoneConfig {
    pub cells_x: u32,
    pub cells_y: u32,
    pub cell_jitter: f32,
    pub warp_amplitude: f32,
    pub crack_width: f32,
    pub shoulder_width: f32,
    pub crack_depth: f32,
    pub slab_variation: f32,
    pub fracture_probability: f32,
    pub fracture_depth: f32,
    pub surface_amplitude: f32,
    pub broad_variation: f32,
}

impl Default for CrackedStoneConfig {
    fn default() -> Self {
        Self {
            cells_x: 8,
            cells_y: 8,
            cell_jitter: 0.27,
            warp_amplitude: 0.16,
            crack_width: 0.035,
            shoulder_width: 0.18,
            crack_depth: 0.13,
            slab_variation: 0.035,
            fracture_probability: 0.28,
            fracture_depth: 0.045,
            surface_amplitude: 0.014,
            broad_variation: 0.018,
        }
    }
}

/// Generates authoritative linear f32 heights for a cracked-stone tile.
pub fn generate_height_values(
    config: CrackedStoneConfig,
    dimensions: FieldDimensions,
    seed: u64,
) -> Result<Vec<f32>, FieldError> {
    validate(config)?;
    let mut values = vec![0.0; dimensions.pixel_count()];
    for y in 0..dimensions.height {
        let v = (y as f32 + 0.5) / dimensions.height as f32;
        for x in 0..dimensions.width {
            let u = (x as f32 + 0.5) / dimensions.width as f32;
            let metrics = cellular_metrics(config, seed, u, v);
            let cell_bias =
                hash_signed(seed, metrics.cell_x, metrics.cell_y, 0x534c_4142_5f42_4941)
                    * config.slab_variation;

            // A quiet broad pass keeps neighbouring slabs from forming an
            // obviously repeated checkerboard while retaining the cellular
            // layout as the dominant shape.
            let broad = periodic_value(
                seed.wrapping_add(3),
                u * 2.0,
                v * 2.0,
                2,
                2,
                0x4252_4f41_445f_4e4f,
            ) * config.broad_variation;
            let layered = fbm(
                seed.wrapping_add(7),
                u,
                v,
                LayeredField {
                    frequency: 5.0,
                    amplitude: config.surface_amplitude,
                    octaves: 3,
                    lacunarity: 2.1,
                    gain: 0.45,
                    offset: 0.0,
                },
            );

            // The second-nearest distance difference is zero on a cell edge
            // and grows towards a cell centre. A smooth two-stage remap gives
            // the crack a low floor, a rounded inner wall and a quiet slab.
            let gap = metrics.second_distance - metrics.nearest_distance;
            let floor_to_bevel = smoothstep(0.0, config.crack_width, gap);
            let bevel_to_slab = smoothstep(config.crack_width, config.shoulder_width, gap);
            let crack_profile = 0.85_f32.mul_add(floor_to_bevel, 0.15 * bevel_to_slab);
            let crack_depression = config.crack_depth * (1.0 - crack_profile);

            let fracture = secondary_fracture(config, seed, &metrics, u, v);
            let height =
                cell_bias + broad + layered - crack_depression - fracture * config.fracture_depth;
            values[y as usize * dimensions.width as usize + x as usize] = height;
        }
    }
    Ok(values)
}

/// The nearest two periodic feature-point distances and owning cell id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellMetrics {
    pub nearest_distance: f32,
    pub second_distance: f32,
    pub cell_x: i32,
    pub cell_y: i32,
    pub local_x: f32,
    pub local_y: f32,
}

pub(crate) fn cellular_metrics(
    config: CrackedStoneConfig,
    seed: u64,
    u: f32,
    v: f32,
) -> CellMetrics {
    let warp_x = periodic_value(
        seed.wrapping_add(11),
        u * 2.0,
        v * 2.0,
        2,
        2,
        0x5741_5250_5f58,
    ) * config.warp_amplitude;
    let warp_y = periodic_value(
        seed.wrapping_add(13),
        u * 2.0 + 9.0,
        v * 2.0 - 3.0,
        2,
        2,
        0x5741_5250_5f59,
    ) * config.warp_amplitude;
    let query_x = (u + warp_x / config.cells_x as f32) * config.cells_x as f32;
    let query_y = (v + warp_y / config.cells_y as f32) * config.cells_y as f32;
    let period =
        periodic::Period2D::new(config.cells_x, config.cells_y).expect("validated cellular period");
    let sample = cellular::sample(seed, [query_x, query_y], period, config.cell_jitter);
    let nearest = sample.nearest_distance;
    let second = sample.second_nearest_distance;
    let nearest_cell = (sample.cell_x as i32, sample.cell_y as i32);
    let nearest_origin = (sample.cell_x as f32 + 0.5, sample.cell_y as f32 + 0.5);
    CellMetrics {
        nearest_distance: nearest,
        second_distance: second,
        cell_x: nearest_cell.0,
        cell_y: nearest_cell.1,
        local_x: query_x - nearest_origin.0,
        local_y: query_y - nearest_origin.1,
    }
}

fn secondary_fracture(
    config: CrackedStoneConfig,
    seed: u64,
    metrics: &CellMetrics,
    u: f32,
    v: f32,
) -> f32 {
    let presence =
        (hash_signed(seed, metrics.cell_x, metrics.cell_y, 0x4652_4143_545f_5052) + 1.0) * 0.5;
    if presence > config.fracture_probability {
        return 0.0;
    }
    let angle = hash_signed(seed, metrics.cell_x, metrics.cell_y, 0x4652_4143_545f_414e) * 1.4;
    let (sin, cos) = angle.sin_cos();
    let along = metrics.local_x * cos + metrics.local_y * sin;
    let across = metrics.local_x * sin - metrics.local_y * cos;
    let width = 0.026 + 0.012 * (presence / config.fracture_probability.max(f32::EPSILON));
    let line = 1.0 - smoothstep(0.0, width, across.abs());
    let taper = (1.0 - (along.abs() / 0.62)).clamp(0.0, 1.0);
    let interior = smoothstep(
        0.0,
        0.12,
        metrics.second_distance - metrics.nearest_distance,
    );
    let branch = (u * 37.0 + v * 41.0 + angle).sin() * 0.12 + 0.88;
    (line * taper * interior * branch).clamp(0.0, 1.0)
}

fn validate(config: CrackedStoneConfig) -> Result<(), FieldError> {
    if config.cells_x == 0
        || config.cells_y == 0
        || config.cell_jitter < 0.0
        || config.cell_jitter > 1.0
        || config.warp_amplitude < 0.0
        || config.warp_amplitude > 0.45
        || config.crack_width <= 0.0
        || config.shoulder_width < config.crack_width
        || config.crack_depth < 0.0
        || config.slab_variation < 0.0
        || !(0.0..=1.0).contains(&config.fracture_probability)
        || config.fracture_depth < 0.0
        || config.surface_amplitude < 0.0
        || config.broad_variation < 0.0
        || ![
            config.cell_jitter,
            config.warp_amplitude,
            config.crack_width,
            config.shoulder_width,
            config.crack_depth,
            config.slab_variation,
            config.fracture_probability,
            config.fracture_depth,
            config.surface_amplitude,
            config.broad_variation,
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
        FieldDimensions::new(64, 48, 4.0, 4.0).expect("dimensions")
    }

    #[test]
    fn cracked_stone_is_deterministic() {
        let config = CrackedStoneConfig::default();
        let a = generate_height_values(config, dimensions(), 42).expect("height");
        let b = generate_height_values(config, dimensions(), 42).expect("height");
        assert_eq!(a, b);
        assert!(a.iter().all(|value| value.is_finite()));
        assert!(a.iter().any(|value| *value < -0.05));
    }

    #[test]
    fn source_height_wraps_across_both_axes() {
        let dimensions = dimensions();
        let values =
            generate_height_values(CrackedStoneConfig::default(), dimensions, 11).expect("height");
        for y in 0..dimensions.height {
            for x in 0..dimensions.width {
                let opposite = ((y as usize * dimensions.width as usize)
                    + ((x + dimensions.width - 1) % dimensions.width) as usize)
                    % values.len();
                assert!(values[y as usize * dimensions.width as usize + x as usize].is_finite());
                assert!(values[opposite].is_finite());
            }
        }
    }

    #[test]
    fn invalid_crack_profile_is_rejected() {
        let result = generate_height_values(
            CrackedStoneConfig {
                crack_width: 0.4,
                shoulder_width: 0.1,
                ..CrackedStoneConfig::default()
            },
            dimensions(),
            4,
        );
        assert_eq!(result, Err(FieldError::NonFiniteParameter));
    }
}
