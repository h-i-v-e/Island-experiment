//! Deterministic hashing, used instead of a random source so repeated runs of
//! the same seed produce an identical scene.

/// `SplitMix64` finalizer.
#[must_use]
pub fn mix(value: u64, salt: u64) -> u64 {
    let mut state = value.wrapping_add(salt).wrapping_add(0x9e37_79b9_7f4a_7c15);
    state = (state ^ (state >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    state = (state ^ (state >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    state ^ (state >> 31)
}

/// A hash as a value in `[0, 1)`.
#[must_use]
pub fn unit(hash: u64) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        (hash >> 40) as f32 / 16_777_216.0
    }
}

/// A hash as an index into `count` choices, taken from bits [`unit`] ignores.
#[must_use]
pub fn choice(hash: u64, count: usize) -> usize {
    #[allow(clippy::cast_possible_truncation)]
    {
        (hash & 0xffff) as usize % count.max(1)
    }
}
