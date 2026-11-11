//! Periodic roof materials: overlapping slate and longitudinal bundled straw.
//!
//! U follows the roof slope; V follows the ridge. Albedo contains only pigment
//! variation, while the shared normal/AO pipeline interprets physical relief.
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
    recipe::{SlateRoofSettings, ThatchRoofSettings},
};

struct SlateSample {
    course: i32,
    slate: i32,
    coverage: f32,
    lap: f32,
    grain: f32,
}

/// Evaluates one physical slate height buffer from borrowed settings.
pub fn generate_slate_height(
    s: &SlateRoofSettings,
    d: FieldDimensions,
    seed: u64,
) -> Result<Vec<f32>, FieldError> {
    validate_slate(s, d)?;
    Ok((0..d.pixel_count())
        .map(|i| {
            let (u, v) = pixel_uv(i, d);
            slate_height(s, d, seed, u, v)
        })
        .collect())
}

fn slate_height(s: &SlateRoofSettings, d: FieldDimensions, seed: u64, u: f32, v: f32) -> f32 {
    let sample = slate_at(s, d, seed, u, v);
    sample.coverage * (sample.lap * s.overlap_height_m + sample.grain * s.surface_relief_m)
        - (1. - sample.coverage) * s.overlap_height_m * 0.25
}

fn slate_at(s: &SlateRoofSettings, d: FieldDimensions, seed: u64, u: f32, v: f32) -> SlateSample {
    let down = u.rem_euclid(1.) * s.courses as f32;
    let course = down.floor() as i32;
    // Stable per-course stagger also wraps for odd course counts.
    let stagger = hash_unit(seed, course, 0, 0x534c_4154_5354);
    let across = v.rem_euclid(1.) * s.slates_per_course as f32 + stagger;
    let slate_count = i32::try_from(s.slates_per_course).expect("validated slate count <= 64");
    let slate = (across.floor() as i32).rem_euclid(slate_count);
    let edge = across.fract().min(1. - across.fract()) * d.tile_height / s.slates_per_course as f32;
    let course_edge = down.fract().min(1. - down.fract()) * d.tile_width / s.courses as f32;
    let coverage = smoothstep(
        s.seam_width_m * 0.5,
        s.seam_width_m * 0.5 + s.edge_bevel_m,
        edge.min(course_edge),
    );
    let grain = periodic_value(
        seed,
        u.rem_euclid(1.) * 24.,
        v.rem_euclid(1.) * 24.,
        24,
        24,
        0x534c_4154_4752,
    ) * 0.65
        + hash_signed(seed, course, slate, 0x534c_4154_4845) * 0.35;
    SlateSample {
        course,
        slate,
        coverage,
        lap: 1. - down.fract(),
        grain,
    }
}

/// Applies intrinsic slate colour variation without directional lighting.
pub(crate) fn tint_slate(
    pixels: &mut [[f32; 3]],
    s: &SlateRoofSettings,
    d: FieldDimensions,
    seed: u64,
) {
    for (i, colour) in pixels.iter_mut().enumerate() {
        let (u, v) = pixel_uv(i, d);
        let sample = slate_at(s, d, seed, u, v);
        let tone =
            hash_signed(seed, sample.course, sample.slate, 0x534c_4154_434f) * s.colour_variation;
        let pigment = 1. + tone + sample.grain * 0.12;
        let joint = 0.6 + sample.coverage * 0.4;
        for channel in colour {
            *channel = (*channel * pigment * joint).clamp(0., 1.);
        }
    }
}

struct ThatchSample {
    fibre: f32,
    bundle: f32,
    course: f32,
    tone: f32,
}

/// Evaluates one physical thatch height buffer from borrowed settings.
pub fn generate_thatch_height(
    s: &ThatchRoofSettings,
    d: FieldDimensions,
    seed: u64,
) -> Result<Vec<f32>, FieldError> {
    validate_thatch(s, d)?;
    Ok((0..d.pixel_count())
        .map(|i| {
            let (u, v) = pixel_uv(i, d);
            thatch_height(s, seed, u, v)
        })
        .collect())
}

fn thatch_height(s: &ThatchRoofSettings, seed: u64, u: f32, v: f32) -> f32 {
    let sample = thatch_at(s, seed, u, v);
    sample.fibre * s.strand_relief_m
        + sample.bundle * s.bundle_relief_m
        + sample.course * s.course_relief_m
}

fn thatch_at(s: &ThatchRoofSettings, seed: u64, u: f32, v: f32) -> ThatchSample {
    let u = u.rem_euclid(1.);
    let v = v.rem_euclid(1.);
    let bundles = i32::try_from(s.bundles).expect("validated bundle count <= 64");
    let strands = i32::try_from(s.bundles * s.strands_per_bundle)
        .expect("validated strand frequency <= 64 * 32");
    let bend = periodic_value(
        seed,
        u * 4.,
        v * s.bundles as f32,
        4,
        bundles,
        0x5448_4154_4245,
    );
    let across = v * strands as f32 + bend * 0.38;
    let column = across.floor() as i32;
    let mut fibre = -1.;
    let mut course = 0.;
    let mut straw_tone = 0.;
    // Three adjacent, overlapping reed profiles suffice. Their widths, tips and
    // pigments vary independently, so there are no synchronized bundle ties.
    for neighbour in -1..=1 {
        let raw_column = column + neighbour;
        let strand = raw_column.rem_euclid(strands);
        let centre =
            raw_column as f32 + 0.5 + hash_signed(seed, strand, 0, 0x5448_4154_4a49) * 0.26;
        let radius = 0.67 + hash_unit(seed, strand, 0, 0x5448_4154_5749) * 0.2;
        let distance = ((across - centre) / radius).abs().min(1.);
        let rounded = (1. - distance * distance).sqrt();
        let lengths = s.courses + (hash_unit(seed, strand, 0, 0x5448_4154_4c45) * 3.) as u32;
        let stagger = hash_unit(seed, strand, 0, 0x5448_4154_5354);
        let along = u * lengths as f32 + stagger;
        let fraction = along.fract();
        // Short, individually staggered cut ends interrupt the long fibres.
        // The cut is shallow; it never exposes a black horizontal trench.
        let tip = smoothstep(0., 0.065, fraction) * smoothstep(0., 0.1, 1. - fraction);
        let profile = rounded * (0.8 + tip * 0.2);
        if profile * 2. - 1. > fibre {
            fibre = profile * 2. - 1.;
            course = rounded * (tip - 0.5);
            straw_tone = reed_pigment(seed, strand, lengths, along);
        }
    }
    // Broad packing undulations and weathering remain continuous, rather than
    // repeating sine ridges and identical rectangular course cells.
    let bundle = periodic_value(
        seed,
        u * 3.,
        v * s.bundles as f32,
        3,
        bundles,
        0x5448_4154_4255,
    );
    let weather = periodic_value(seed, u * 5., v * 7., 5, 7, 0x5448_4154_5745);
    ThatchSample {
        fibre,
        bundle,
        course,
        tone: straw_tone * 0.72 + weather * 0.28,
    }
}

fn reed_pigment(seed: u64, strand: i32, lengths: u32, along: f32) -> f32 {
    let period = i32::try_from(lengths).expect("validated reed segment count <= 66");
    let segment = (along.floor() as i32).rem_euclid(period);
    let current = hash_signed(seed, strand, segment, 0x5448_4154_5347);
    let next = hash_signed(
        seed,
        strand,
        (segment + 1).rem_euclid(period),
        0x5448_4154_5347,
    );
    // Reed lengths have separate pigments, fading only near their staggered
    // tips. Wrapped indices keep the transition continuous across the tile.
    let transition = smoothstep(0.82, 1., along.fract());
    let segment_tone = current + (next - current) * transition;
    hash_signed(seed, strand, 0, 0x5448_4154_434f) * 0.25 + segment_tone * 0.75
}

/// Applies pale and darker straw pigments along the fibre direction.
pub(crate) fn tint_thatch(
    pixels: &mut [[f32; 3]],
    s: &ThatchRoofSettings,
    d: FieldDimensions,
    seed: u64,
) {
    for (i, colour) in pixels.iter_mut().enumerate() {
        let (u, v) = pixel_uv(i, d);
        let sample = thatch_at(s, seed, u, v);
        let pigment = 1. + sample.tone * s.colour_variation + sample.fibre * 0.075;
        // Weathered reeds vary from pale gold to grey-brown. This is intrinsic
        // pigment variation; normals and AO supply all directional shading.
        let weathering = (sample.bundle * 0.5 + 0.5) * 0.09;
        for (channel, value) in colour.iter_mut().enumerate() {
            let tint = match channel {
                0 => 1. - weathering,
                2 => 1. + weathering * 0.65,
                _ => 1. - weathering * 0.25,
            };
            *value = (*value * pigment * tint).clamp(0., 1.);
        }
    }
}

fn pixel_uv(index: usize, d: FieldDimensions) -> (f32, f32) {
    (
        ((index % d.width as usize) as f32 + 0.5) / d.width as f32,
        ((index / d.width as usize) as f32 + 0.5) / d.height as f32,
    )
}

fn finite_nonnegative(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite() && *value >= 0.)
}

fn validate_slate(s: &SlateRoofSettings, d: FieldDimensions) -> Result<(), FieldError> {
    let cell = (d.tile_width / s.courses as f32).min(d.tile_height / s.slates_per_course as f32);
    if s.courses == 0
        || s.courses > 64
        || s.courses > d.width
        || s.slates_per_course == 0
        || s.slates_per_course > 64
        || s.slates_per_course > d.height
        || !finite_nonnegative(&[
            s.seam_width_m,
            s.edge_bevel_m,
            s.overlap_height_m,
            s.surface_relief_m,
            s.colour_variation,
        ])
        || s.seam_width_m <= 0.
        || s.edge_bevel_m <= 0.
        || s.seam_width_m > cell * 0.3
        || s.edge_bevel_m > cell * 0.2
        || s.colour_variation > 1.
    {
        return Err(FieldError::NonFiniteParameter);
    }
    Ok(())
}

fn validate_thatch(s: &ThatchRoofSettings, d: FieldDimensions) -> Result<(), FieldError> {
    if s.bundles == 0
        || s.bundles > 64
        || s.bundles > d.height
        || s.courses == 0
        || s.courses > 64
        || s.courses > d.width
        || s.strands_per_bundle == 0
        || s.strands_per_bundle > 32
        || !finite_nonnegative(&[
            s.strand_relief_m,
            s.bundle_relief_m,
            s.course_relief_m,
            s.colour_variation,
        ])
        || s.colour_variation > 1.
    {
        return Err(FieldError::NonFiniteParameter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dimensions() -> FieldDimensions {
        FieldDimensions::new(128, 128, 2., 2.).unwrap()
    }

    #[test]
    fn roof_fields_repeat_in_both_axes_for_odd_and_even_course_counts() {
        for courses in [5, 8] {
            let slate = SlateRoofSettings {
                courses,
                ..SlateRoofSettings::default()
            };
            let thatch = ThatchRoofSettings {
                courses,
                ..ThatchRoofSettings::default()
            };
            for (u, v) in [(0.019, 0.14), (0.23, 0.78), (0.89, 0.57), (0., 0.)] {
                for (du, dv) in [(1., 0.), (0., 1.), (-1., -1.)] {
                    assert!(
                        (slate_height(&slate, dimensions(), 42, u, v)
                            - slate_height(&slate, dimensions(), 42, u + du, v + dv))
                        .abs()
                            < 1e-6
                    );
                    assert!(
                        (thatch_height(&thatch, 42, u, v)
                            - thatch_height(&thatch, 42, u + du, v + dv))
                        .abs()
                            < 1e-6
                    );
                }
            }
        }
    }

    #[test]
    fn slate_has_lapped_courses_along_u_and_staggered_widths_along_v() {
        let s = SlateRoofSettings::default();
        let d = dimensions();
        let seam = slate_at(&s, d, 42, 1. / s.courses as f32, 0.37);
        assert!(seam.coverage < 0.01);
        let a = slate_at(&s, d, 42, 0.04, 0.37);
        let b = slate_at(&s, d, 42, 0.09, 0.37);
        assert!(a.lap > b.lap);
        assert!(a.coverage > 0.99 && b.coverage > 0.99);
        assert_ne!(
            slate_at(&s, d, 42, 0.04, 0.12).slate,
            slate_at(&s, d, 42, 0.04, 0.8).slate
        );
    }

    #[test]
    fn individual_reeds_change_pigment_between_lengths_and_wrap_without_a_colour_seam() {
        let tones: Vec<_> = (0..5)
            .map(|segment| reed_pigment(42, 17, 5, segment as f32 + 0.4))
            .collect();
        let low = tones.iter().copied().fold(f32::INFINITY, f32::min);
        let high = tones.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            high - low > 0.25,
            "reed lengths retain one continuous pigment: {tones:?}"
        );
        for along in [0.13, 1.87, 4.94] {
            assert!(
                (reed_pigment(42, 17, 5, along) - reed_pigment(42, 17, 5, along + 5.)).abs() < 1e-5
            );
        }
        assert!((reed_pigment(42, 17, 5, 0.) - reed_pigment(42, 17, 5, 5. - 1e-5)).abs() < 1e-5);
        let s = ThatchRoofSettings::default();
        for v in [0.123, 0.519, 0.877] {
            assert!(
                (thatch_at(&s, 42, 0., v).tone - thatch_at(&s, 42, 1. - 1e-6, v).tone).abs() < 1e-4
            );
        }
    }

    #[test]
    fn straw_tips_do_not_form_synchronized_horizontal_course_trenches() {
        let s = ThatchRoofSettings::default();
        let row_means: Vec<f32> = (0..64)
            .map(|row| {
                (0..256)
                    .map(|column| {
                        thatch_at(
                            &s,
                            42,
                            (row as f32 + 0.5) / 64.,
                            (column as f32 + 0.5) / 256.,
                        )
                        .course
                    })
                    .sum::<f32>()
                    / 256.
            })
            .collect();
        let low = row_means.iter().copied().fold(f32::INFINITY, f32::min);
        let high = row_means.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        // Every reed retains cut tips, but they do not line up into the repeated
        // full-width dark bars of a woven or tied mat.
        assert!(high - low < 0.2, "row-averaged tip relief: {low}..{high}");
    }

    #[test]
    fn natural_straw_pigments_keep_a_continuous_light_surface() {
        let s = ThatchRoofSettings::default();
        let d = dimensions();
        let mut colours = vec![[0.2; 3]; d.pixel_count()];
        tint_thatch(&mut colours, &s, d, 42);
        assert!(
            colours
                .iter()
                .flatten()
                .all(|value| (0.12..0.28).contains(value))
        );
        assert!(
            colours
                .iter()
                .any(|colour| (colour[0] - colour[2]).abs() > 0.005)
        );
        for v in [0.123, 0.519, 0.877] {
            let a = thatch_height(&s, 42, 0., v);
            let b = thatch_height(&s, 42, 1. - 1e-6, v);
            assert!((a - b).abs() < 1e-6, "slope seam: {a} != {b}");
        }
    }

    #[test]
    fn straw_fibres_run_along_u_with_faster_changes_across_v() {
        let s = ThatchRoofSettings::default();
        let (mut along, mut across) = (0., 0.);
        for index in 0..256 {
            let u = (index as f32 + 0.5) / 256.;
            let v = 0.371;
            let value = thatch_at(&s, 42, u, v).fibre;
            along += (value - thatch_at(&s, 42, u + 0.002, v).fibre).abs();
            across += (value - thatch_at(&s, 42, u, v + 0.002).fibre).abs();
        }
        assert!(across > along * 8., "across={across}, along={along}");
    }
}
