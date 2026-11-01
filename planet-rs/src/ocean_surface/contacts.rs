//! Match the moving ocean to the static river mesh at mean sea level.
use super::{CoastMap, cube_direction, cube_pixel};
use crate::DVec3;

impl CoastMap {
    /// Flatten waves across actual river/sea contact edges, then fade back to
    /// the existing wave field. The core covers bilinear map filtering and the
    /// nearby ocean vertices; depth and shore distance remain untouched.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn flatten_sea_contacts(&mut self, contacts: &[[DVec3; 2]]) {
        if self.size == 0 || self.pixels.len() != self.size as usize * self.size as usize * 24 {
            return;
        }
        let size = f64::from(self.size);
        let core = 4.0 / size;
        for &[a, b] in contacts {
            if !a.is_finite()
                || !b.is_finite()
                || a.length_squared() < 0.5
                || b.length_squared() < 0.5
            {
                continue;
            }
            let a = a.normalize();
            let b = b.normalize();
            let steps = (a.distance(b) * size / 0.75).ceil().max(1.0) as u32;
            for step in 0..=steps {
                let point = a
                    .lerp(b, f64::from(step) / f64::from(steps))
                    .normalize_or_zero();
                if point == DVec3::ZERO {
                    continue;
                }
                let (face, x, y) = cube_pixel(point, self.size);
                // Cube-face corners have denser angular sampling. A 12-texel
                // neighbourhood covers the entire two-core feather there too.
                for dy in -12..=12 {
                    for dx in -12..=12 {
                        let direction = cube_direction(
                            face,
                            (f64::from(x) + f64::from(dx) + 0.5) * 2.0 / size - 1.0,
                            (f64::from(y) + f64::from(dy) + 0.5) * 2.0 / size - 1.0,
                        );
                        let (face, x, y) = cube_pixel(direction, self.size);
                        let texel = cube_direction(
                            face,
                            (f64::from(x) + 0.5) * 2.0 / size - 1.0,
                            (f64::from(y) + 0.5) * 2.0 / size - 1.0,
                        );
                        let blend = ((texel.distance(point) - core) / core).clamp(0.0, 1.0);
                        let mask =
                            ((1.0 - blend * blend * (3.0 - 2.0 * blend)) * 255.0).round() as u8;
                        let index = ((face * self.size as usize + y as usize) * self.size as usize
                            + x as usize)
                            * 4
                            + 2;
                        self.pixels[index] = self.pixels[index].max(mask);
                    }
                }
            }
        }
    }
}
