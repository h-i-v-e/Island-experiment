//! Periodic 3D cloud shape volume: coherent macro structure and three Worley
//! scales. Built once; cloud, reflection and shadow passes share the GPU image.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub(super) const SIZE: u32 = 64;

fn hash(mut v: u32) -> f32 {
    v = (v ^ (v >> 16)).wrapping_mul(0x7feb_352d);
    v = (v ^ (v >> 15)).wrapping_mul(0x846c_a68b);
    ((v ^ (v >> 16)) & 0x00ff_ffff) as f32 / 16_777_215.0
}
struct Grid {
    size: i32,
    features: Vec<[f32; 3]>,
    values: Vec<f32>,
}
impl Grid {
    fn new(size: i32, seed: u32) -> Self {
        let count = (size * size * size) as usize;
        Self {
            size,
            features: (0..count)
                .map(|i| {
                    std::array::from_fn(|axis| {
                        0.08 + 0.84
                            * hash(
                                (i as u32).wrapping_mul(0x9e37_79b9)
                                    ^ seed
                                    ^ (axis as u32).wrapping_mul(0x85eb_ca6b),
                            )
                    })
                })
                .collect(),
            values: (0..count)
                .map(|i| hash((i as u32).wrapping_mul(0xc2b2_ae35) ^ seed ^ 0x27d4_eb2f))
                .collect(),
        }
    }
    fn index(&self, p: [i32; 3]) -> usize {
        let [x, y, z] = p.map(|v| v.rem_euclid(self.size));
        (x + self.size * (y + self.size * z)) as usize
    }
    fn sample(&self, unit: [f32; 3]) -> (f32, f32) {
        let q = unit.map(|v| v * self.size as f32);
        let cell = q.map(|v| v.floor() as i32);
        let frac = std::array::from_fn::<_, 3, _>(|i| q[i] - cell[i] as f32);
        let fade = frac.map(|v| v * v * v * (v * (v * 6.0 - 15.0) + 10.0));
        let mut nearest = f32::INFINITY;
        let mut coherent = 0.0;
        for z in -1..=1 {
            for y in -1..=1 {
                for x in -1..=1 {
                    let offset = [x, y, z];
                    let index = self.index(std::array::from_fn(|i| cell[i] + offset[i]));
                    let feature = self.features[index];
                    let distance: f32 = (0..3)
                        .map(|i| {
                            let delta = offset[i] as f32 + feature[i] - frac[i];
                            delta * delta
                        })
                        .sum();
                    nearest = nearest.min(distance);
                    if x >= 0 && y >= 0 && z >= 0 {
                        let weight: f32 = (0..3)
                            .map(|i| {
                                if offset[i] == 0 {
                                    1.0 - fade[i]
                                } else {
                                    fade[i]
                                }
                            })
                            .product();
                        coherent += self.values[index] * weight;
                    }
                }
            }
        }
        (coherent, 1.0 - nearest.sqrt().clamp(0.0, 1.0))
    }
}
fn value(grids: &[Grid; 3], p: [f32; 3]) -> [f32; 4] {
    let a = grids[0].sample(p);
    let b = grids[1].sample(p);
    let c = grids[2].sample(p);
    let coherent = a.0 * 0.65 + b.0 * 0.25 + c.0 * 0.10;
    [
        (0.5 + (coherent * 0.70 + a.1 * 0.30 - 0.47) * 1.35).clamp(0.0, 1.0),
        a.1,
        b.1,
        c.1,
    ]
}
pub(super) fn generate() -> Vec<u8> {
    let grids = [
        Grid::new(4, 0xa341_316c),
        Grid::new(8, 0xc801_3ea4),
        Grid::new(16, 0xad90_777d),
    ];
    let mut pixels = Vec::with_capacity((SIZE * SIZE * SIZE * 4) as usize);
    for z in 0..SIZE {
        for y in 0..SIZE {
            for x in 0..SIZE {
                let p = [x, y, z].map(|v| (v as f32 + 0.5) / SIZE as f32);
                pixels.extend(value(&grids, p).map(|v| (v * 255.0).round() as u8));
            }
        }
    }
    pixels
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_volume_is_periodic_in_three_dimensions_and_has_resolved_billow_scales() {
        let grids = [Grid::new(4, 7), Grid::new(8, 13), Grid::new(16, 31)];
        for p in [[0.17, 0.43, 0.79], [-0.01, 0.31, 1.13], [0.0, 0.0, 0.0]] {
            let expected = value(&grids, p);
            for axis in 0..3 {
                let mut wrapped = p;
                wrapped[axis] += 1.0;
                let actual = value(&grids, wrapped);
                for i in 0..4 {
                    assert!((expected[i] - actual[i]).abs() < 1e-5);
                }
            }
        }
        let pixels = generate();
        assert_eq!(pixels.len(), 64 * 64 * 64 * 4);
        for channel in 0..4 {
            let range = pixels.chunks_exact(4).fold((255u8, 0u8), |(lo, hi), p| {
                (lo.min(p[channel]), hi.max(p[channel]))
            });
            assert!(
                range.0 < 90 && range.1 > 180,
                "flat cloud noise channel {channel}: {range:?}"
            );
        }
    }
}
