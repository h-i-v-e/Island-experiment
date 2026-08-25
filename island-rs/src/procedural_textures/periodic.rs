//! Deterministic helpers for fields that repeat over a rectangular period.
//!
//! Coordinates passed to the functions in this module are lattice
//! coordinates.  A [`Period2D`] therefore describes the number of integer
//! lattice cells in one tile, rather than the number of output pixels.  This
//! distinction is important: changing an image's resolution must not change
//! the physical frequency of a recipe.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

use core::fmt;

/// A non-empty rectangular period measured in lattice cells.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Period2D {
    /// Number of lattice cells in the x direction.
    pub x: u32,
    /// Number of lattice cells in the y direction.
    pub y: u32,
}

impl Period2D {
    /// Creates a period, rejecting empty axes.
    pub const fn new(x: u32, y: u32) -> Result<Self, PeriodError> {
        if x == 0 {
            Err(PeriodError::ZeroWidth)
        } else if y == 0 {
            Err(PeriodError::ZeroHeight)
        } else {
            Ok(Self { x, y })
        }
    }

    /// Creates a period from known non-zero dimensions.
    ///
    /// This is useful for constants and for callers that have already
    /// validated their recipe.  The checked [`new`](Self::new) constructor is
    /// preferable at public input boundaries.
    pub const fn new_unchecked(x: u32, y: u32) -> Self {
        debug_assert!(x != 0 && y != 0);
        Self { x, y }
    }

    /// Returns this period with each axis multiplied by `factor`.
    pub const fn checked_mul(self, factor: u32) -> Result<Self, PeriodError> {
        if factor == 0 {
            return Err(PeriodError::ZeroWidth);
        }

        let Some(x) = self.x.checked_mul(factor) else {
            return Err(PeriodError::Overflow);
        };
        let Some(y) = self.y.checked_mul(factor) else {
            return Err(PeriodError::Overflow);
        };
        Ok(Self { x, y })
    }

    /// Wraps an x-coordinate into this period.
    #[inline]
    pub fn wrap_x(self, coordinate: i64) -> u32 {
        wrap_coordinate(coordinate, self.x)
    }

    /// Wraps a y-coordinate into this period.
    #[inline]
    pub fn wrap_y(self, coordinate: i64) -> u32 {
        wrap_coordinate(coordinate, self.y)
    }
}

/// Errors returned while constructing a periodic domain.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PeriodError {
    /// The x axis has no cells.
    ZeroWidth,
    /// The y axis has no cells.
    ZeroHeight,
    /// Multiplying a period would exceed the representable range.
    Overflow,
}

impl fmt::Display for PeriodError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWidth => formatter.write_str("period width must be greater than zero"),
            Self::ZeroHeight => formatter.write_str("period height must be greater than zero"),
            Self::Overflow => formatter.write_str("period multiplication overflowed"),
        }
    }
}

impl std::error::Error for PeriodError {}

/// Wraps an integer coordinate using Euclidean (rather than truncating)
/// remainder semantics.
#[inline]
pub fn wrap_coordinate(coordinate: i64, period: u32) -> u32 {
    debug_assert!(period != 0, "a periodic coordinate needs a non-zero period");
    coordinate.rem_euclid(i64::from(period)) as u32
}

/// SplitMix64's finalizer used by all texture source fields.
///
/// Keeping this function small and stateless makes the random basis portable
/// between the in-process API and an offline baker.  It intentionally mirrors
/// the integer mixing constants used by the existing terrain noise without
/// changing that module's sampling behavior.
#[inline]
pub fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Hashes a wrapped lattice coordinate.
///
/// The seed is the recipe/layer seed domain.  Coordinates are wrapped before
/// hashing, so the same lattice cell on opposite tile borders has the same
/// random value.
#[inline]
pub fn hash_2d(seed: u64, x: i64, y: i64, period: Period2D) -> u64 {
    let wrapped_x = u64::from(period.wrap_x(x));
    let wrapped_y = u64::from(period.wrap_y(y));
    let key = seed
        ^ wrapped_x.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ wrapped_y.wrapping_mul(0x85eb_ca6b_27d4_eb2f);
    mix64(key)
}

/// Hashes a lattice coordinate without wrapping it.
///
/// This is useful for deriving independent seed domains.  Field samplers
/// should normally use [`hash_2d`] so their source is tileable.
#[inline]
pub fn hash_2d_unwrapped(seed: u64, x: i64, y: i64) -> u64 {
    let key = seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0x85eb_ca6b_27d4_eb2f);
    mix64(key)
}

/// Maps a hash to the half-open interval `[0, 1)`.
#[inline]
pub fn hash_to_unit(hash: u64) -> f32 {
    // A fixed 24-bit mantissa avoids architecture-dependent integer-to-float
    // conversions involving the full 64-bit hash.
    ((hash >> 40) as u32) as f32 / 16_777_216.0
}

/// Maps a hash to the half-open interval `[-1, 1)`.
#[inline]
pub fn hash_to_signed(hash: u64) -> f32 {
    hash_to_unit(hash) * 2.0 - 1.0
}

/// Cubic interpolation fade curve.
#[inline]
pub fn fade(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

/// Quintic interpolation fade curve with zero first and second derivatives at
/// both endpoints.
#[inline]
pub fn fade_quintic(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

/// Linear interpolation.
#[inline]
pub fn lerp(a: f32, b: f32, amount: f32) -> f32 {
    (b - a).mul_add(amount, a)
}

/// Samples periodic interpolated value noise in lattice coordinates.
///
/// Non-finite input coordinates are treated as zero.  Recipe validation
/// rejects such values before generation; this defensive behavior keeps a
/// direct API caller from accidentally producing NaNs throughout a field.
pub fn value_noise(seed: u64, position: [f32; 2], period: Period2D) -> f32 {
    if !position[0].is_finite() || !position[1].is_finite() {
        return 0.0;
    }

    // Canonicalize to one tile copy before flooring. This keeps equivalent
    // points on opposite borders bit-identical instead of merely close after
    // subtracting large absolute coordinates.
    let position = [
        position[0].rem_euclid(period.x as f32),
        position[1].rem_euclid(period.y as f32),
    ];
    let x0 = position[0].floor() as i64;
    let y0 = position[1].floor() as i64;
    let tx = fade_quintic(position[0] - x0 as f32);
    let ty = fade_quintic(position[1] - y0 as f32);

    let top = lerp(
        hash_to_signed(hash_2d(seed, x0, y0, period)),
        hash_to_signed(hash_2d(seed, x0 + 1, y0, period)),
        tx,
    );
    let bottom = lerp(
        hash_to_signed(hash_2d(seed, x0, y0 + 1, period)),
        hash_to_signed(hash_2d(seed, x0 + 1, y0 + 1, period)),
        tx,
    );
    lerp(top, bottom, ty)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{Period2D, hash_2d, mix64, value_noise};

    #[test]
    fn wraps_negative_coordinates() {
        let period = Period2D::new(7, 5).expect("non-empty period");
        assert_eq!(period.wrap_x(-1), 6);
        assert_eq!(period.wrap_y(-11), 4);
        assert_eq!(period.wrap_x(14), 0);
    }

    #[test]
    fn splitmix_finalizer_is_stable() {
        assert_eq!(mix64(0), 0);
        assert_eq!(mix64(1), 0x5692_161d_100b_05e5);
    }

    #[test]
    fn hash_repeats_at_period_boundaries() {
        let period = Period2D::new(11, 9).expect("non-empty period");
        assert_eq!(hash_2d(42, -3, 4, period), hash_2d(42, 8, 13, period));
    }

    #[test]
    fn value_noise_repeats_on_both_axes() {
        let period = Period2D::new(13, 17).expect("non-empty period");
        let point = [2.375, 7.125];
        let original = value_noise(0xfeed_beef, point, period);
        assert_eq!(
            original,
            value_noise(0xfeed_beef, [point[0] + 13.0, point[1]], period)
        );
        assert_eq!(
            original,
            value_noise(0xfeed_beef, [point[0], point[1] + 17.0], period)
        );
    }
}
