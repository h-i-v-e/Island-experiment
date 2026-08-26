#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

/// Small deterministic generator used to keep tree seeds portable across
/// platforms without coupling the Bevy-owned generator to `island-rs` internals.
#[derive(Clone, Debug)]
pub(super) struct Rng(u64);

impl Rng {
    pub(super) const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(super) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    pub(super) fn unit(&mut self) -> f32 {
        let mantissa = (self.next_u64() >> 40) as u32;
        mantissa as f32 / 16_777_216.0
    }

    pub(super) fn range(&mut self, start: f32, end: f32) -> f32 {
        (end - start).mul_add(self.unit(), start)
    }
}
