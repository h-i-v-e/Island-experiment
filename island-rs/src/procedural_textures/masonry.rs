//! Periodic running-bond stonework: alternate courses overlap their neighbours.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc
)]

use super::field_program::{
    FieldDimensions, FieldError, LayeredField, fbm, hash_signed, periodic_value, smoothstep,
};
use super::recipe::MasonrySettings;

struct StoneSample {
    coverage: f32,
    column: i32,
    row: i32,
}

/// Bake one owned height buffer from borrowed recipe settings.
pub fn generate_height_values(
    settings: &MasonrySettings,
    dimensions: FieldDimensions,
    seed: u64,
) -> Result<Vec<f32>, FieldError> {
    validate(settings, dimensions)?;
    Ok((0..dimensions.pixel_count())
        .map(|index| {
            let u = (index % dimensions.width as usize) as f32 / dimensions.width as f32
                + 0.5 / dimensions.width as f32;
            let v = (index / dimensions.width as usize) as f32 / dimensions.height as f32
                + 0.5 / dimensions.height as f32;
            height_at(settings, dimensions, seed, u, v)
        })
        .collect())
}

fn height_at(
    settings: &MasonrySettings,
    dimensions: FieldDimensions,
    seed: u64,
    u: f32,
    v: f32,
) -> f32 {
    let sample = stone_at(settings, dimensions, seed, u, v);
    let variation =
        hash_signed(seed, sample.column, sample.row, 0x5354_4f4e_4548) * settings.stone_variation;
    let relief = |frequency, domain| {
        fbm(
            seed.wrapping_add(domain),
            u,
            v,
            LayeredField {
                frequency,
                amplitude: settings.surface_amplitude,
                octaves: 2,
                lacunarity: 2.0,
                gain: 0.45,
                offset: 0.0,
            },
        )
    };
    let surface = relief(12.0, 31) + relief(96.0, 37);
    sample.coverage * (variation + surface) - (1.0 - sample.coverage) * settings.mortar_depth
}

/// Tint the existing linear albedo in place; no second colour image is allocated.
pub(crate) fn tint_stones(
    pixels: &mut [[f32; 3]],
    settings: &MasonrySettings,
    dimensions: FieldDimensions,
    seed: u64,
) {
    for (index, colour) in pixels.iter_mut().enumerate() {
        let u = (index % dimensions.width as usize) as f32 / dimensions.width as f32;
        let v = (index / dimensions.width as usize) as f32 / dimensions.height as f32;
        let sample = stone_at(
            settings,
            dimensions,
            seed,
            u + 0.5 / dimensions.width as f32,
            v + 0.5 / dimensions.height as f32,
        );
        let tone = hash_signed(seed, sample.column, sample.row, 0x5354_4f4e_4543)
            * settings.colour_variation
            * sample.coverage;
        let warmth = hash_signed(seed, sample.column, sample.row, 0x5354_4f4e_4557)
            * settings.colour_variation
            * sample.coverage
            * 0.12;
        for (channel, value) in colour.iter_mut().enumerate() {
            let tint = if channel == 0 {
                warmth
            } else if channel == 2 {
                -warmth
            } else {
                0.0
            };
            *value = (*value * (1.0 + tone + tint)).clamp(0.0, 1.0);
        }
    }
}

fn stone_at(
    settings: &MasonrySettings,
    dimensions: FieldDimensions,
    seed: u64,
    u: f32,
    v: f32,
) -> StoneSample {
    let u = u.rem_euclid(1.0);
    let v = v.rem_euclid(1.0);
    let warp =
        |domain| periodic_value(seed, u * 12.0, v * 8.0, 12, 8, domain) * settings.edge_roughness;
    let x = (u * dimensions.tile_width + warp(0x0045_4447_4558)).rem_euclid(dimensions.tile_width);
    let y =
        (v * dimensions.tile_height + warp(0x0045_4447_4559)).rem_euclid(dimensions.tile_height);
    let stone_width = dimensions.tile_width / settings.cells_x as f32;
    let course_height = dimensions.tile_height / settings.cells_y as f32;
    let columns = i32::try_from(settings.cells_x).expect("validated masonry columns");
    let courses = i32::try_from(settings.cells_y).expect("validated masonry courses");
    let row = ((y / course_height).floor() as i32).min(courses - 1);
    let offset = (row % 2) as f32 * stone_width * 0.5;
    let boundary = |column: i32| {
        column as f32 * stone_width - offset
            + hash_signed(seed, column.rem_euclid(columns), row, 0x004a_4f49_4e54)
                * settings.joint_jitter
                * stone_width
    };
    let mut column = ((x + offset) / stone_width).floor() as i32;
    if x < boundary(column) {
        column -= 1;
    } else if x > boundary(column + 1) {
        column += 1;
    }
    let horizontal = (x - boundary(column)).min(boundary(column + 1) - x);
    let vertical = (y - row as f32 * course_height).min((row + 1) as f32 * course_height - y);
    StoneSample {
        coverage: smoothstep(
            settings.mortar_width * 0.5,
            settings.mortar_width * 0.5 + settings.bevel_width,
            horizontal.min(vertical),
        ),
        column: column.rem_euclid(columns),
        row,
    }
}

fn validate(settings: &MasonrySettings, dimensions: FieldDimensions) -> Result<(), FieldError> {
    if settings.cells_x == 0
        || i32::try_from(settings.cells_x).is_err()
        || i32::try_from(settings.cells_y).is_err()
        || settings.cells_x > dimensions.width
        || settings.cells_y < 2
        || settings.cells_y > dimensions.height
        || !settings.cells_y.is_multiple_of(2)
        || ![
            settings.joint_jitter,
            settings.mortar_width,
            settings.bevel_width,
            settings.mortar_depth,
            settings.stone_variation,
            settings.edge_roughness,
            settings.surface_amplitude,
            settings.colour_variation,
        ]
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
        || settings.joint_jitter > 0.2
        || settings.colour_variation > 1.0
        || settings.mortar_width <= 0.0
        || settings.bevel_width <= 0.0
    {
        return Err(FieldError::NonFiniteParameter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dimensions() -> FieldDimensions {
        FieldDimensions::new(128, 64, 2.4, 1.2).unwrap()
    }

    #[test]
    fn courses_overlap_by_half_a_stone_and_wrap_both_axes() {
        let settings = MasonrySettings {
            joint_jitter: 0.0,
            edge_roughness: 0.0,
            ..MasonrySettings::default()
        };
        let d = dimensions();
        assert!(stone_at(&settings, d, 42, 0.0, 0.125).coverage < 0.01);
        assert!(stone_at(&settings, d, 42, 0.0, 0.375).coverage > 0.99);
        assert!(stone_at(&settings, d, 42, 0.125, 0.125).coverage > 0.99);
        assert!(stone_at(&settings, d, 42, 0.125, 0.375).coverage < 0.01);
        for (u, v) in [(0.0, 0.0), (0.2, 0.3), (0.9, 0.85)] {
            let reference = height_at(&settings, d, 42, u, v);
            assert!((reference - height_at(&settings, d, 42, u + 1.0, v)).abs() < 1e-5);
            assert!((reference - height_at(&settings, d, 42, u, v + 1.0)).abs() < 1e-5);
        }
    }

    #[test]
    fn generation_is_deterministic_and_odd_course_counts_are_rejected() {
        let settings = MasonrySettings::default();
        let a = generate_height_values(&settings, dimensions(), 42).unwrap();
        assert_eq!(
            a,
            generate_height_values(&settings, dimensions(), 42).unwrap()
        );
        assert!(a.iter().all(|height| height.is_finite()));
        let invalid = MasonrySettings {
            cells_y: 3,
            ..settings
        };
        assert_eq!(
            generate_height_values(&invalid, dimensions(), 42),
            Err(FieldError::NonFiniteParameter)
        );
    }
}
