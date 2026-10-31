//! Small deterministic streams for independent geological features.
use crate::DVec3;

pub(super) struct Random(u64);
impl Random {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn bits(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
    pub fn unit(&mut self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let result = (self.bits() >> 11) as f64 / 9_007_199_254_740_992.0;
        result
    }
    pub fn direction(&mut self) -> DVec3 {
        let y = self.unit() * 2.0 - 1.0;
        let phi = self.unit() * std::f64::consts::TAU;
        let r = (1.0 - y * y).sqrt();
        DVec3::new(r * phi.cos(), y, r * phi.sin())
    }
}
