//! The inherited value/fractal noise extended into Cartesian 3D. Sampling
//! directions directly has no longitude seam or special pole coordinates.
use crate::DVec3;

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[allow(clippy::cast_precision_loss)]
fn lattice(seed: u64, point: [i64; 3]) -> f64 {
    let key = seed
        ^ point[0].cast_unsigned().wrapping_mul(0x9e37_79b9)
        ^ point[1].cast_unsigned().wrapping_mul(0x85eb_ca6b)
        ^ point[2].cast_unsigned().wrapping_mul(0xc2b2_ae35);
    (mix(key) >> 11) as f64 / 4_503_599_627_370_496.0 - 1.0
}

fn lerp(a: f64, b: f64, amount: f64) -> f64 {
    (b - a).mul_add(amount, a)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn value(seed: u64, point: DVec3) -> f64 {
    // Frequencies are validated at the public boundary, well inside i64 range.
    let base = point.floor().to_array().map(|v| v as i64);
    let offset = point - point.floor();
    let fade = offset * offset * (DVec3::splat(3.0) - 2.0 * offset);
    let plane = |z| {
        lerp(
            lerp(
                lattice(seed, [base[0], base[1], z]),
                lattice(seed, [base[0] + 1, base[1], z]),
                fade.x,
            ),
            lerp(
                lattice(seed, [base[0], base[1] + 1, z]),
                lattice(seed, [base[0] + 1, base[1] + 1, z]),
                fade.x,
            ),
            fade.y,
        )
    };
    lerp(plane(base[2]), plane(base[2] + 1), fade.z)
}

pub(crate) fn fractal(seed: u64, point: DVec3, octaves: u8) -> f64 {
    let mut frequency = 1.0;
    let mut amplitude = 1.0;
    let mut total = 0.0;
    let mut weight = 0.0;
    for octave in 0..octaves {
        total += value(seed.wrapping_add(u64::from(octave)), point * frequency) * amplitude;
        weight += amplitude;
        frequency *= 2.03;
        amplitude *= 0.5;
    }
    total / weight
}

/// Warped F2-F1 cellular ridges: broad plate interiors with connected uplifted
/// boundaries. This is a tectonic-style height field, not a plate simulation.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub(crate) fn voronoi_plates(seed: u64, point: DVec3) -> f64 {
    let warp = DVec3::new(
        fractal(seed ^ 0x4528_21e6_38d0_1377, point * 0.7, 3),
        fractal(seed ^ 0xbe54_66cf_34e9_0c6c, point * 0.7, 3),
        fractal(seed ^ 0xc0ac_29b7_c97c_50dd, point * 0.7, 3),
    );
    let point = point + warp * 0.55;
    let base = point.floor().to_array().map(|v| v as i64);
    let mut nearest = [f64::INFINITY; 2];
    // Generous fixed neighbourhood retains the two nearest jittered sites even
    // across cell boundaries. Seeds/features depend only on planet coordinates.
    for z in -2..=2 {
        for y in -2..=2 {
            for x in -2..=2 {
                let cell = [base[0] + x, base[1] + y, base[2] + z];
                let feature = DVec3::new(
                    cell[0] as f64 + 0.5 + 0.35 * lattice(seed, cell),
                    cell[1] as f64 + 0.5 + 0.35 * lattice(seed ^ 0x3f84_d5b5_b547_0917, cell),
                    cell[2] as f64 + 0.5 + 0.35 * lattice(seed ^ 0x9216_d5d9_8979_fb1b, cell),
                );
                let distance = point.distance_squared(feature);
                if distance < nearest[0] {
                    nearest = [distance, nearest[0]];
                } else if distance < nearest[1] {
                    nearest[1] = distance;
                }
            }
        }
    }
    let gap = (nearest[1].sqrt() - nearest[0].sqrt()) / 0.45;
    let t = gap.clamp(0.0, 1.0);
    let edge = 1.0 - t * t * (3.0 - 2.0 * t);
    let broad = fractal(seed ^ 0xd131_0ba6_98df_b5ac, point * 0.45, 3);
    (1.6 * edge * edge - 0.65 + 0.2 * broad).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cartesian_noise_is_continuous_at_poles_and_longitude_seam() {
        for direction in [DVec3::Y, -DVec3::Y, -DVec3::X] {
            let a = (direction + DVec3::Z * 1.0e-9).normalize();
            let b = (direction - DVec3::Z * 1.0e-9).normalize();
            assert!((fractal(42, a * 12.0, 4) - fractal(42, b * 12.0, 4)).abs() < 1.0e-6);
        }
    }
}
