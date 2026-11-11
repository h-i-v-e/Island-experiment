//! Packed reed end grain with four nested cross-section masks for the tips.
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
use crate::{MaterialSlot, MaterialTexture};

const SIZE: u32 = 256;
const CELLS: i32 = 24;

pub(super) fn generate(roof: &MaterialTexture, seed: u64) -> [MaterialTexture; 5] {
    let pigment = mean_pigment(&roof.albedo);
    std::array::from_fn(|index| {
        let depth = f32::from(u8::try_from(index).expect("base and four shells")) * 0.2;
        let mut albedo = Vec::with_capacity((SIZE * SIZE * 4) as usize);
        let mut relief = Vec::with_capacity((SIZE * SIZE) as usize);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let [coverage, tone, height] = sample(seed, x, y, depth);
                let tone = if index == 0 {
                    0.62 + coverage * (tone - 0.62)
                } else {
                    tone
                };
                albedo.extend([
                    srgb(pigment[0] * tone),
                    srgb(pigment[1] * tone),
                    srgb(pigment[2] * tone),
                    if index == 0 { 255 } else { byte(coverage) },
                ]);
                relief.push(height);
            }
        }
        let source_hash = format!("thatch-cut-ends-v2:{index}:{}", super::hash_bytes(&albedo));
        MaterialTexture {
            slot: if index == 0 {
                MaterialSlot::ThatchEnd
            } else {
                MaterialSlot::THATCH_SHELLS[index - 1]
            },
            size: SIZE,
            albedo,
            normal: if index == 0 {
                normals(&relief)
            } else {
                [128, 128, 255, 255].repeat((SIZE * SIZE) as usize)
            },
            masks: relief
                .iter()
                .flat_map(|height| [byte(0.86 + 0.14 * height), 250, 0, 255])
                .collect(),
            physical_size: [0.16; 2],
            source_hash,
        }
    })
}

fn sample(seed: u64, x: u32, y: u32, depth: f32) -> [f32; 3] {
    let u = (x as f32 + 0.5) / SIZE as f32 * CELLS as f32;
    let v = (y as f32 + 0.5) / SIZE as f32 * CELLS as f32;
    let cell = [u.floor() as i32, v.floor() as i32];
    let mut coverage = 0_f32;
    let mut tone = 1.;
    let mut height = 0.;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let column = cell[0] + dx;
            let row = cell[1] + dy;
            let sample = reed(seed, [column, row], [u, v], depth);
            if sample[0] > coverage {
                coverage = sample[0];
                tone = sample[1];
                height = sample[2];
            }
        }
    }
    [coverage, tone, height]
}

fn reed(seed: u64, cell: [i32; 2], uv: [f32; 2], depth: f32) -> [f32; 3] {
    let random = |salt| noise(seed, cell, salt);
    let centre = [
        cell[0] as f32 + 0.5 + (random(17) - 0.5) * 0.95,
        cell[1] as f32 + 0.5 + (random(31) - 0.5) * 0.95,
    ];
    let length = 0.25 + random(53) * 0.75;
    let taper = 1. - depth / length * 0.3;
    let radius = [
        (0.35 + random(71) * 0.22) * taper,
        (0.32 + random(89) * 0.24) * taper,
    ];
    let distance = ((uv[0] - centre[0]) / radius[0]).hypot((uv[1] - centre[1]) / radius[1]);
    let edge = smooth(0.94, 1.06, distance);
    let tip = smooth(depth - 0.015, depth + 0.015, length);
    let coverage = (1. - edge) * tip;
    let hollow = 1. - smooth(0.06, 0.22, distance);
    let rim = 1. - smooth(0.7, 1., distance);
    [
        coverage,
        (0.82 + random(101) * 0.36) * (1. - hollow * 0.34),
        coverage * (0.5 + rim * 0.25 - hollow * 0.35),
    ]
}

fn normals(height: &[f32]) -> Vec<u8> {
    let side = SIZE as usize;
    let mut normal = Vec::with_capacity(height.len() * 4);
    for y in 0..side {
        for x in 0..side {
            let at = |x, y| height[(y % side) * side + x % side];
            let nx = (at(x + side - 1, y) - at(x + 1, y)) * 0.8;
            let ny = (at(x, y + side - 1) - at(x, y + 1)) * 0.8;
            let length = (1. + nx * nx + ny * ny).sqrt();
            normal.extend([
                byte(nx / length * 0.5 + 0.5),
                byte(ny / length * 0.5 + 0.5),
                byte(1. / length * 0.5 + 0.5),
                255,
            ]);
        }
    }
    normal
}

fn noise(seed: u64, cell: [i32; 2], salt: u64) -> f32 {
    let [x, y] = cell.map(|v| u64::try_from(v.rem_euclid(CELLS)).expect("wrapped positive cell"));
    let mut value = seed ^ x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b) ^ salt;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    (value & 0xffff) as f32 / 65_535.
}

fn smooth(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

fn mean_pigment(bytes: &[u8]) -> [f32; 3] {
    let count = (bytes.len() / 4).max(1) as f32;
    bytes.chunks_exact(4).fold([0.; 3], |mut sum, pixel| {
        for i in 0..3 {
            sum[i] += linear(pixel[i]) / count;
        }
        sum
    })
}
fn linear(value: u8) -> f32 {
    let value = f32::from(value) / 255.;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
fn srgb(value: f32) -> u8 {
    let value = value.clamp(0., 1.);
    let value = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1. / 2.4) - 0.055
    };
    byte(value)
}

#[allow(clippy::cast_sign_loss)] // RGBA8 quantization is clamped to the unsigned range.
fn byte(value: f32) -> u8 {
    (value * 255.).round().clamp(0., 255.) as u8
}
