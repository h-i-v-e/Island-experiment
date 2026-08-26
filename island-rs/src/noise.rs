#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

pub(crate) const FRACTAL_LACUNARITY: f32 = 2.03;

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn lattice(seed: u64, x: i32, y: i32) -> f32 {
    let key = seed
        ^ u64::from(x.cast_unsigned()).wrapping_mul(0x9e37_79b9)
        ^ u64::from(y.cast_unsigned()).wrapping_mul(0x85eb_ca6b);
    let value = (mix(key) >> 40) as u32;
    value as f32 / 8_388_608.0 - 1.0
}

fn fade(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn lerp(a: f32, b: f32, amount: f32) -> f32 {
    (b - a).mul_add(amount, a)
}

pub fn value(seed: u64, x: f32, y: f32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = fade(x - x0 as f32);
    let ty = fade(y - y0 as f32);
    let top = lerp(lattice(seed, x0, y0), lattice(seed, x0 + 1, y0), tx);
    let bottom = lerp(lattice(seed, x0, y0 + 1), lattice(seed, x0 + 1, y0 + 1), tx);
    lerp(top, bottom, ty)
}

pub fn fractal(seed: u64, x: f32, y: f32, octaves: u8) -> f32 {
    let mut frequency = 1.0;
    let mut amplitude = 1.0;
    let mut total = 0.0;
    let mut weight = 0.0;
    for octave in 0..octaves {
        total += value(
            seed.wrapping_add(u64::from(octave)),
            x * frequency,
            y * frequency,
        ) * amplitude;
        weight += amplitude;
        frequency *= FRACTAL_LACUNARITY;
        amplitude *= 0.5;
    }
    total / weight
}
