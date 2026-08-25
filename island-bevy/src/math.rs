//! Small deterministic scalar helpers shared by CPU-baked render fields.

/// The same cubic interpolation WGSL's `smoothstep` applies.
#[must_use]
pub fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let span = high - low;
    if span.abs() <= f32::EPSILON {
        return f32::from(value >= high);
    }
    let progress = ((value - low) / span).clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

/// Sums half-amplitude, double-frequency octaves and normalizes their weight.
#[must_use]
pub fn octave_sum(first_period: u32, octaves: u32, mut sample: impl FnMut(u32, u32) -> f32) -> f32 {
    let mut total = 0.0;
    let mut normalization = 0.0;
    let mut amplitude = 1.0;
    let mut period = first_period;
    for octave in 0..octaves {
        total += amplitude * sample(octave, period);
        normalization += amplitude;
        amplitude *= 0.5;
        period = period.saturating_mul(2);
    }
    if normalization > 0.0 {
        total / normalization
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::{octave_sum, smoothstep};

    #[test]
    fn octave_sum_halves_each_weight() {
        let value = octave_sum(2, 3, |octave, period| {
            f32::from(u16::try_from(octave + period).expect("small test periods"))
        });
        let expected = (2.0 + 0.5 * 5.0 + 0.25 * 10.0) / 1.75;
        assert!((value - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn smoothstep_holds_both_edges() {
        assert!(smoothstep(2.0, 4.0, 1.0).abs() < f32::EPSILON);
        assert!((smoothstep(2.0, 4.0, 5.0) - 1.0).abs() < f32::EPSILON);
        assert!((smoothstep(2.0, 4.0, 3.0) - 0.5).abs() < f32::EPSILON);
    }
}
