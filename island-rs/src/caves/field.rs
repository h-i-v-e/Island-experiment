use crate::{Vec3, noise};

#[allow(clippy::many_single_char_names)]
pub(crate) fn noise3(seed: u64, p: Vec3) -> f32 {
    // Interpolate seeded 2D layers: continuous in all three dimensions.
    let z = p.z.floor();
    let t = p.z - z;
    let t = t * t * (3.0 - 2.0 * t);
    let a = noise::value(seed.wrapping_add((z as i64).cast_unsigned()), p.x, p.y);
    let b = noise::value(
        seed.wrapping_add(((z as i64) + 1).cast_unsigned()),
        p.x,
        p.y,
    );
    a + (b - a) * t
}
