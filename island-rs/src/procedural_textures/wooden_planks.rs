//! Periodic timber boards with recessed seams and anisotropic longitudinal grain.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc
)]

use super::{
    field_program::{
        FieldDimensions, FieldError, hash_signed, hash_unit, periodic_value, smoothstep,
    },
    recipe::WoodenPlanksSettings,
};

struct BoardSample {
    coverage: f32,
    column: i32,
    row: i32,
    grain: f32,
}

/// Evaluate one owned physical height buffer from borrowed settings.
pub fn generate_height_values(
    settings: &WoodenPlanksSettings,
    dimensions: FieldDimensions,
    seed: u64,
) -> Result<Vec<f32>, FieldError> {
    validate(settings, dimensions)?;
    Ok((0..dimensions.pixel_count())
        .map(|index| {
            let (u, v) = pixel_uv(index, dimensions);
            height_at(settings, dimensions, seed, u, v)
        })
        .collect())
}

fn height_at(
    settings: &WoodenPlanksSettings,
    dimensions: FieldDimensions,
    seed: u64,
    u: f32,
    v: f32,
) -> f32 {
    let sample = board_at(settings, dimensions, seed, u, v);
    let board =
        hash_signed(seed, sample.column, sample.row, 0x574f_4f44_4845) * settings.plank_variation_m;
    sample.coverage * (board + sample.grain * settings.grain_strength_m)
        - (1. - sample.coverage) * settings.seam_depth_m
}

/// Apply lighting-free board colour and grain to the shared linear albedo buffer.
pub(crate) fn tint_boards(
    pixels: &mut [[f32; 3]],
    settings: &WoodenPlanksSettings,
    dimensions: FieldDimensions,
    seed: u64,
) {
    for (index, colour) in pixels.iter_mut().enumerate() {
        let (u, v) = pixel_uv(index, dimensions);
        let sample = board_at(settings, dimensions, seed, u, v);
        let tone = hash_signed(seed, sample.column, sample.row, 0x574f_4f44_434f)
            * settings.colour_variation;
        let warmth = hash_signed(seed, sample.column, sample.row, 0x574f_4f44_5741)
            * settings.colour_variation
            * 0.16;
        let grain = sample.grain * 0.19;
        let seam = 0.24 + sample.coverage * 0.76;
        for (channel, value) in colour.iter_mut().enumerate() {
            let tint = match channel {
                0 => warmth,
                2 => -warmth,
                _ => 0.,
            };
            *value = (*value * (1. + (tone + grain + tint) * sample.coverage) * seam).clamp(0., 1.);
        }
    }
}

fn pixel_uv(index: usize, d: FieldDimensions) -> (f32, f32) {
    (
        ((index % d.width as usize) as f32 + 0.5) / d.width as f32,
        ((index / d.width as usize) as f32 + 0.5) / d.height as f32,
    )
}

fn board_at(
    settings: &WoodenPlanksSettings,
    d: FieldDimensions,
    seed: u64,
    u: f32,
    v: f32,
) -> BoardSample {
    let u = u.rem_euclid(1.);
    let v = v.rem_euclid(1.);
    let across = u * settings.planks as f32;
    let column = (across.floor() as i32)
        .rem_euclid(i32::try_from(settings.planks).expect("validated board count"));
    let fraction = across.fract();
    let edge = fraction.min(1. - fraction) * d.tile_width / settings.planks as f32;
    let (row, end) = end_joint(settings, d, seed, column, v);
    let coverage = smoothstep(
        settings.seam_width_m * 0.5,
        settings.seam_width_m * 0.5 + settings.bevel_width_m,
        edge,
    )
    .min(filtered_end_coverage(
        settings,
        end,
        d.tile_height / d.height as f32,
    ));
    let grain = grain_at(settings, seed, column, u, v, fraction);
    BoardSample {
        coverage,
        column,
        row,
        grain,
    }
}

fn filtered_end_coverage(settings: &WoodenPlanksSettings, distance: f32, texel: f32) -> f32 {
    let seam = settings.seam_width_m * 0.5;
    let bevel = settings.bevel_width_m;
    let half = texel * 0.5;
    if distance >= seam + bevel + half {
        return 1.;
    }
    // Integrate the bevel across the texel. Point samples miss narrow joints
    // entirely when regular ends lie on a texture boundary or half-tile line.
    let integral = |position: f32| {
        let offset = position.abs() - seam;
        let area = if offset <= 0. {
            0.
        } else if offset >= bevel {
            offset - bevel * 0.5
        } else {
            let t = offset / bevel;
            bevel * t.powi(3) * (1. - t * 0.5)
        };
        area.copysign(position)
    };
    ((integral(distance + half) - integral(distance - half)) / texel).clamp(0., 1.)
}

fn end_joint(
    settings: &WoodenPlanksSettings,
    d: FieldDimensions,
    seed: u64,
    column: i32,
    v: f32,
) -> (i32, f32) {
    if settings.joints_per_plank == 0 {
        return (0, f32::INFINITY);
    }
    let stagger = if settings.regular_stagger {
        column.rem_euclid(2) as f32 * 0.5
    } else {
        hash_unit(seed, column, 0, 0x574f_4f44_4a4f)
    };
    let along = v * settings.joints_per_plank as f32 + stagger;
    let row = (along.floor() as i32)
        .rem_euclid(i32::try_from(settings.joints_per_plank).expect("validated joint count"));
    let fraction = along.fract();
    (
        row,
        fraction.min(1. - fraction) * d.tile_height / settings.joints_per_plank as f32,
    )
}

fn grain_at(
    settings: &WoodenPlanksSettings,
    seed: u64,
    column: i32,
    u: f32,
    v: f32,
    fraction: f32,
) -> f32 {
    let board_seed = seed ^ (u64::from(column.cast_unsigned()).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let bend = periodic_value(board_seed, v * 3., 0.37, 3, 1, 0x4752_4149_4e42);
    let broad = periodic_value(board_seed, fraction * 3., v * 2., 3, 2, 0x4752_4149_4e43);
    let phase = fraction * settings.grain_scale as f32 + bend * 0.9 + broad * 0.65;
    let ring = (phase * std::f32::consts::TAU).sin();
    let dark_band = ((ring + 1.) * 0.5).powi(8);
    let frequency =
        i32::try_from(settings.planks * settings.grain_scale * 2).expect("bounded grain frequency");
    let fibres = periodic_value(
        seed,
        u * frequency as f32 + bend * 0.3,
        v * 4.,
        frequency,
        4,
        0x4752_4149_4e46,
    );
    0.45 * broad + 0.25 * fibres - 0.8 * dark_band
}

fn validate(s: &WoodenPlanksSettings, d: FieldDimensions) -> Result<(), FieldError> {
    let board_width = d.tile_width / s.planks as f32;
    if s.planks == 0
        || s.planks > 128
        || s.planks > d.width
        || s.joints_per_plank > 64
        || s.joints_per_plank > d.height
        || (s.regular_stagger && s.joints_per_plank > 0 && !s.planks.is_multiple_of(2))
        || s.grain_scale == 0
        || s.grain_scale > 64
        || ![
            s.seam_width_m,
            s.bevel_width_m,
            s.seam_depth_m,
            s.grain_strength_m,
            s.plank_variation_m,
            s.colour_variation,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.)
        || s.seam_width_m <= 0.
        || s.bevel_width_m <= 0.
        || s.colour_variation > 1.
        || s.seam_width_m > board_width * 0.4
        || s.bevel_width_m > board_width * 0.2
        || (s.joints_per_plank > 0
            && s.seam_width_m > d.tile_height / s.joints_per_plank as f32 * 0.4)
    {
        return Err(FieldError::NonFiniteParameter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dimensions() -> FieldDimensions {
        FieldDimensions::new(128, 256, 1.2, 2.4).unwrap()
    }

    #[test]
    fn regular_joints_share_alternate_rows_and_half_length_offsets_for_every_seed() {
        let settings = WoodenPlanksSettings {
            joints_per_plank: 2,
            regular_stagger: true,
            ..WoodenPlanksSettings::default()
        };
        let d = dimensions();
        for seed in [0, 42, 41827] {
            for column in 0..6 {
                let u = (column as f32 + 0.5) / 6.;
                let start = if column % 2 == 0 { 0. } else { 0.25 };
                for joint in [start, start + 0.5, start + 1.] {
                    assert!(end_joint(&settings, d, seed, column, joint).1 < 0.00001);
                    assert!(board_at(&settings, d, seed, u, joint + 0.25).coverage > 0.99);
                    assert!(
                        (height_at(&settings, d, seed, u, joint)
                            - height_at(&settings, d, seed, u + 1., joint))
                        .abs()
                            < 0.00001
                    );
                }
            }
        }
        let odd = WoodenPlanksSettings {
            planks: 5,
            ..settings
        };
        assert!(generate_height_values(&odd, d, 42).is_err());
        let legacy: WoodenPlanksSettings =
            serde_json::from_str("{\"joints_per_plank\":2}").unwrap();
        assert!(!legacy.regular_stagger);
    }

    #[test]
    fn rasterized_floorboard_end_joints_survive_preview_and_export_sampling() {
        use super::super::recipe::{MaterialModel, TextureRecipe};

        let recipe: TextureRecipe = serde_json::from_str(include_str!(
            "../../texture-recipes/wooden-floorboards.json"
        ))
        .unwrap();
        let MaterialModel::WoodenPlanks(settings) = recipe.material else {
            panic!("floorboards must use the wooden-plank evaluator");
        };
        let continuous = WoodenPlanksSettings {
            joints_per_plank: 0,
            ..settings
        };
        for resolution in [256, 512, 1024] {
            let d = FieldDimensions::new(resolution, resolution, 1.2, 2.345).unwrap();
            let heights = generate_height_values(&settings, d, recipe.seed).unwrap();
            let base_heights = generate_height_values(&continuous, d, recipe.seed).unwrap();
            let mut colours = vec![[0.3, 0.18, 0.085]; d.pixel_count()];
            let mut base_colours = colours.clone();
            tint_boards(&mut colours, &settings, d, recipe.seed);
            tint_boards(&mut base_colours, &continuous, d, recipe.seed);
            let width = resolution as usize;
            for column in 0..settings.planks {
                let x =
                    ((column as f32 + 0.5) * resolution as f32 / settings.planks as f32) as usize;
                let rows = if column % 2 == 0 {
                    [0, width - 1]
                } else {
                    [width / 2 - 1, width / 2]
                };
                for row in rows {
                    let index = row * width + x;
                    assert!(
                        colours[index][0] < base_colours[index][0] * 0.9,
                        "{resolution}px column {column}, row {row}: seam {}, board {}",
                        colours[index][0],
                        base_colours[index][0]
                    );
                    assert!(heights[index] < base_heights[index] - 0.0004);
                }
                let middle = width / 4 * width + x;
                assert_eq!(colours[middle], base_colours[middle]);
                assert_eq!(heights[middle], base_heights[middle]);
            }
        }
    }

    #[test]
    fn seams_are_physical_and_long_boards_wrap_with_and_without_end_joints() {
        for joints_per_plank in [0, 2, 3] {
            let s = WoodenPlanksSettings {
                joints_per_plank,
                ..WoodenPlanksSettings::default()
            };
            let d = dimensions();
            assert!(board_at(&s, d, 42, 0., 0.43).coverage < 0.01);
            assert!(board_at(&s, d, 42, 0.08, 0.43).coverage > 0.99);
            for (u, v) in [(0.02, 0.13), (0.27, 0.89), (0.56, 0.63)] {
                let reference = height_at(&s, d, 42, u, v);
                assert!((reference - height_at(&s, d, 42, u + 1., v)).abs() < 1e-6);
                assert!((reference - height_at(&s, d, 42, u, v + 1.)).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn grain_changes_faster_across_boards_than_along_them() {
        let s = WoodenPlanksSettings::default();
        let mut across = 0.;
        let mut along = 0.;
        for index in 0..256 {
            let u = (index as f32 + 0.5) / 256.;
            let v = 0.36;
            let a = board_at(&s, dimensions(), 42, u, v).grain;
            across += (a - board_at(&s, dimensions(), 42, u + 0.002, v).grain).abs();
            along += (a - board_at(&s, dimensions(), 42, u, v + 0.002).grain).abs();
        }
        assert!(
            across > along * 5.,
            "longitudinal grain: across={across}, along={along}"
        );
    }
}
