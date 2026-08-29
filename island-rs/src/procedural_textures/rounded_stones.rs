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
use super::{
    cellular, periodic,
    recipe::{ROUNDED_STONES_MAX_RADIUS, ROUNDED_STONES_MAX_WARP_AMPLITUDE},
};

const PROFILE_SAMPLES_PER_CELL: u32 = 16;

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
    validate(config, dimensions)?;
    let cell_profiles = cell_profiles(config, seed)?;
    let mut values = vec![0.0; dimensions.pixel_count()];
    for y in 0..dimensions.height {
        let v = (y as f32 + 0.5) / dimensions.height as f32;
        for x in 0..dimensions.width {
            let u = (x as f32 + 0.5) / dimensions.width as f32;
            let sample = stone_sample(config, seed, u, v);
            let cell_bias = hash_signed(seed, sample.cell_x, sample.cell_y, 0x5354_4f4e_455f_4249)
                * config.stone_variation;
            let inset = (0.5 - config.stone_radius).max(0.0);
            let inner_distance = (sample.edge_distance - inset).max(0.0);
            let profile = cell_profiles[sample.cell_index];
            let edge_radius = (profile.edge_radius - inset).max(f32::EPSILON);
            let edge_softness = config.edge_softness.min(edge_radius * 0.75);
            let interior = smoothstep(0.0, edge_softness.max(f32::EPSILON), inner_distance);
            let normalized_depth = (inner_distance / edge_radius).clamp(0.0, 1.0);
            // Distance from the inset Voronoi boundary is the raster equivalent
            // of separating each polygon, insetting it, and smoothing its sides.
            // A radial hemisphere rounds away the raw polygon's medial ridges;
            // retaining part of the boundary profile keeps each outline irregular.
            let boundary_dome = (normalized_depth * std::f32::consts::FRAC_PI_2).sin();
            let radial_distance =
                (sample.nearest_distance / profile.radial_radius.max(f32::EPSILON)).clamp(0.0, 1.0);
            let radial_dome = (1.0 - radial_distance * radial_distance).sqrt();
            let dome = (radial_dome * 0.8 + boundary_dome * 0.2).powf(config.anisotropy);
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
struct StoneSample {
    cell_index: usize,
    cell_x: i32,
    cell_y: i32,
    nearest_distance: f32,
    edge_distance: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CellProfile {
    edge_radius: f32,
    radial_radius: f32,
}

fn stone_sample(config: RoundedStonesConfig, seed: u64, u: f32, v: f32) -> StoneSample {
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
    let sample = cellular::sample_rounded_edge(
        seed,
        [query_x, query_y],
        period,
        config.cell_jitter,
        config.edge_softness * 2.5,
    );
    let cell_x = sample.cell_x.rem_euclid(i64::from(config.cells_x)) as i32;
    let cell_y = sample.cell_y.rem_euclid(i64::from(config.cells_y)) as i32;
    StoneSample {
        cell_index: cell_y as usize * config.cells_x as usize + cell_x as usize,
        cell_x,
        cell_y,
        nearest_distance: sample.nearest_distance,
        edge_distance: sample.edge_distance,
    }
}

fn cell_profiles(config: RoundedStonesConfig, seed: u64) -> Result<Vec<CellProfile>, FieldError> {
    let profile_width = config
        .cells_x
        .checked_mul(PROFILE_SAMPLES_PER_CELL)
        .ok_or(FieldError::DimensionOverflow)?;
    let profile_height = config
        .cells_y
        .checked_mul(PROFILE_SAMPLES_PER_CELL)
        .ok_or(FieldError::DimensionOverflow)?;
    let cell_count = usize::try_from(config.cells_x)
        .ok()
        .and_then(|width| {
            usize::try_from(config.cells_y)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(FieldError::DimensionOverflow)?;
    let mut profiles = vec![CellProfile::default(); cell_count];
    for y in 0..profile_height {
        let v = (y as f32 + 0.5) / profile_height as f32;
        for x in 0..profile_width {
            let u = (x as f32 + 0.5) / profile_width as f32;
            let sample = stone_sample(config, seed, u, v);
            let profile = &mut profiles[sample.cell_index];
            profile.edge_radius = profile.edge_radius.max(sample.edge_distance);
            profile.radial_radius = profile.radial_radius.max(sample.nearest_distance);
        }
    }
    Ok(profiles)
}

fn validate(config: RoundedStonesConfig, dimensions: FieldDimensions) -> Result<(), FieldError> {
    if config.cells_x == 0
        || config.cells_y == 0
        || config.cells_x > dimensions.width
        || config.cells_y > dimensions.height
        || config.cell_jitter < 0.0
        || config.cell_jitter > 1.0
        || config.warp_amplitude < 0.0
        || config.warp_amplitude > ROUNDED_STONES_MAX_WARP_AMPLITUDE
        || config.stone_radius <= 0.0
        || config.stone_radius > ROUNDED_STONES_MAX_RADIUS
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

    #[test]
    fn rounded_stones_leave_a_gap_on_both_sides_of_cell_boundaries() {
        let dimensions = FieldDimensions::new(256, 256, 2.0, 2.0).expect("dimensions");
        let config = RoundedStonesConfig {
            sand_amplitude: 0.0,
            stone_variation: 0.0,
            ..RoundedStonesConfig::default()
        };
        let values = generate_height_values(config, dimensions, 17).expect("height");
        let mut boundary_pairs = 0;

        for y in 0..dimensions.height {
            for x in 0..dimensions.width {
                let index = y as usize * dimensions.width as usize + x as usize;
                let u = (x as f32 + 0.5) / dimensions.width as f32;
                let v = (y as f32 + 0.5) / dimensions.height as f32;
                let current = stone_sample(config, 17, u, v);
                for (neighbour_x, neighbour_y) in [
                    ((x + 1) % dimensions.width, y),
                    (x, (y + 1) % dimensions.height),
                ] {
                    let neighbour_u = (neighbour_x as f32 + 0.5) / dimensions.width as f32;
                    let neighbour_v = (neighbour_y as f32 + 0.5) / dimensions.height as f32;
                    let neighbour = stone_sample(config, 17, neighbour_u, neighbour_v);
                    if current.cell_index == neighbour.cell_index {
                        continue;
                    }
                    let neighbour_index =
                        neighbour_y as usize * dimensions.width as usize + neighbour_x as usize;
                    assert!((values[index] - config.gap_height).abs() <= f32::EPSILON);
                    assert!((values[neighbour_index] - config.gap_height).abs() <= f32::EPSILON);
                    boundary_pairs += 1;
                }
            }
        }

        assert!(
            boundary_pairs > 100,
            "expected many independent Voronoi cells"
        );
    }

    #[test]
    fn cell_profiles_are_resolution_independent_and_cover_every_cell() {
        let config = RoundedStonesConfig::default();
        let profiles = cell_profiles(config, 29).expect("cell profiles");
        assert_eq!(profiles.len(), (config.cells_x * config.cells_y) as usize);
        assert!(
            profiles
                .iter()
                .all(|profile| profile.edge_radius > 0.0 && profile.radial_radius > 0.0)
        );
    }
}
