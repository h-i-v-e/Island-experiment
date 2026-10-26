use crate::DVec3;
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanGridOptions {
    pub spacing: f64,
    pub fine_radius: f64,
    pub rows_per_level: usize,
}
impl Default for OceanGridOptions {
    fn default() -> Self {
        Self {
            spacing: 1.0,
            fine_radius: 256.0,
            rows_per_level: 8,
        }
    }
}
pub struct OceanGrid {
    pub positions: Vec<DVec3>,
    pub sampling: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
    pub rings: usize,
}
impl OceanGrid {
    /// # Errors
    /// Rejects invalid physical scales and meshes above Unity's fixed runtime budget.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::too_many_lines
    )]
    pub fn build(radius: f64, options: OceanGridOptions) -> Result<Self, String> {
        if !radius.is_finite()
            || radius <= 0.0
            || !options.spacing.is_finite()
            || options.spacing < 0.25
            || !options.fine_radius.is_finite()
            || options.fine_radius < options.spacing * 4.0
            || !(2..=32).contains(&options.rows_per_level)
        {
            return Err("invalid spherical ocean grid settings".into());
        }
        let mut result = Self {
            positions: vec![DVec3::Y],
            sampling: vec![[0.0, options.spacing as f32]],
            triangles: Vec::new(),
            rings: 0,
        };
        let extent = PI * radius;
        let fine_extent = options.fine_radius.min(extent * 0.5);
        // A sphere still curves after displacement fades out. Bound the far angular
        // spacing so the outer grid does not turn into giant, sunken chord triangles.
        let maximum_spacing = (radius * 0.008).max(options.spacing);
        let mut spacing = options.spacing;
        let mut arc = spacing;
        let mut rows = 0;
        let mut previous: Option<(u32, u32)> = None;
        while arc < extent - spacing * 0.5 {
            let theta = arc / radius;
            let circumference = TAU * radius * theta.sin();
            let count = ((circumference / spacing).ceil() as u32).div_ceil(8).max(1) * 8;
            let start =
                u32::try_from(result.positions.len()).map_err(|_| "ocean grid too large")?;
            if u64::from(start) + u64::from(count) > 500_000 {
                return Err(
                    "ocean grid exceeds 500000 vertices; increase spacing or reduce fine radius"
                        .into(),
                );
            }
            for index in 0..count {
                let angle = TAU * f64::from(index) / f64::from(count);
                result.positions.push(DVec3::new(
                    theta.sin() * angle.cos(),
                    theta.cos(),
                    theta.sin() * angle.sin(),
                ));
                result.sampling.push([arc as f32, spacing as f32]);
            }
            if let Some((inner, inner_count)) = previous {
                let (mut a, mut b) = (0, 0);
                while a < inner_count || b < count {
                    let ai = inner + a % inner_count;
                    let bi = start + b % count;
                    if a < inner_count
                        && (b == count
                            || u64::from(a + 1) * u64::from(count)
                                <= u64::from(b + 1) * u64::from(inner_count))
                    {
                        result
                            .triangles
                            .push([ai, inner + (a + 1) % inner_count, bi]);
                        a += 1;
                    } else {
                        result.triangles.push([ai, start + (b + 1) % count, bi]);
                        b += 1;
                    }
                }
            } else {
                for b in 0..count {
                    result
                        .triangles
                        .push([0, start + (b + 1) % count, start + b]);
                }
            }
            previous = Some((start, count));
            result.rings += 1;
            if arc >= fine_extent {
                rows += 1;
                if rows == 1 && spacing.to_bits() == options.spacing.to_bits()
                    || rows >= options.rows_per_level
                {
                    spacing = (spacing * 2.0).min(maximum_spacing);
                    rows = 0;
                }
            }
            let next = if arc < fine_extent {
                (arc + spacing).min(fine_extent)
            } else {
                arc + spacing
            };
            if next <= arc {
                return Err("ocean grid spacing cannot advance".into());
            }
            arc = next;
        }
        let pole = u32::try_from(result.positions.len()).map_err(|_| "ocean grid too large")?;
        result.positions.push(-DVec3::Y);
        result.sampling.push([extent as f32, spacing as f32]);
        if let Some((last, count)) = previous {
            for a in 0..count {
                result
                    .triangles
                    .push([last + a, last + (a + 1) % count, pole]);
            }
        }
        // Keep the outward convention independent of zipper/cap construction.
        for face in &mut result.triangles {
            let [a, b, c] = face.map(|i| result.positions[i as usize]);
            if (b - a).cross(c - a).dot(a + b + c) < 0.0 {
                face.swap(1, 2);
            }
        }
        if result.triangles.len() > 1_000_000 {
            return Err("ocean grid exceeds one million triangles".into());
        }
        Ok(result)
    }
}
