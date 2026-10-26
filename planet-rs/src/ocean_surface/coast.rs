//! Seam-connected cube-map bake of actual radial terrain depth and geodesic
//! distance to the zero-height contour. Data is fixed to the planet, not camera.
use crate::{DVec3, surface_atlas::SurfaceSampler};
use std::{cmp::Ordering, collections::BinaryHeap};

#[derive(Clone, Debug, PartialEq)]
pub struct CoastMap {
    pub size: u32,
    pub pixels: Vec<u8>,
}
/// WebGPU cube order: +X, -X, +Y, -Y, +Z, -Z. UV origin is top left.
#[must_use]
pub fn cube_direction(face: usize, u: f64, v: f64) -> DVec3 {
    match face {
        0 => DVec3::new(1.0, -v, -u),
        1 => DVec3::new(-1.0, -v, u),
        2 => DVec3::new(u, 1.0, v),
        3 => DVec3::new(u, -1.0, -v),
        4 => DVec3::new(u, -v, 1.0),
        _ => DVec3::new(-u, -v, -1.0),
    }
    .normalize()
}
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn cube_pixel(direction: DVec3, size: u32) -> (usize, u32, u32) {
    let a = direction.abs();
    let (face, u, v) = if a.x >= a.y && a.x >= a.z {
        if direction.x > 0.0 {
            (0, -direction.z / a.x, -direction.y / a.x)
        } else {
            (1, direction.z / a.x, -direction.y / a.x)
        }
    } else if a.y >= a.z {
        if direction.y > 0.0 {
            (2, direction.x / a.y, direction.z / a.y)
        } else {
            (3, direction.x / a.y, -direction.z / a.y)
        }
    } else if direction.z > 0.0 {
        (4, direction.x / a.z, -direction.y / a.z)
    } else {
        (5, -direction.x / a.z, -direction.y / a.z)
    };
    let pixel = |value: f64| (((value + 1.0) * 0.5 * f64::from(size)).floor() as u32).min(size - 1);
    (face, pixel(u), pixel(v))
}
fn direction(index: usize, size: u32) -> DVec3 {
    let width = size as usize;
    let face = index / (width * width);
    let local = index % (width * width);
    cube_direction(
        face,
        (f64::from(u32::try_from(local % width).unwrap()) + 0.5) * 2.0 / f64::from(size) - 1.0,
        (f64::from(u32::try_from(local / width).unwrap()) + 0.5) * 2.0 / f64::from(size) - 1.0,
    )
}
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn neighbours(index: usize, size: u32) -> [usize; 8] {
    let width = size as usize;
    let face = index / (width * width);
    let local = index % (width * width);
    let x = i32::try_from(local % width).expect("bounded map size");
    let y = i32::try_from(local / width).expect("bounded map size");
    [
        (x - 1, y),
        (x + 1, y),
        (x, y - 1),
        (x, y + 1),
        (x - 1, y - 1),
        (x + 1, y - 1),
        (x - 1, y + 1),
        (x + 1, y + 1),
    ]
    .map(|(x, y)| {
        if x >= 0 && y >= 0 && x < size.cast_signed() && y < size.cast_signed() {
            return face * width * width + y as usize * width + x as usize;
        }
        let d = cube_direction(
            face,
            (f64::from(x) + 0.5) * 2.0 / f64::from(size) - 1.0,
            (f64::from(y) + 0.5) * 2.0 / f64::from(size) - 1.0,
        );
        let (face, x, y) = cube_pixel(d, size);
        face * width * width + y as usize * width + x as usize
    })
}
#[derive(Clone, Copy)]
struct Visit {
    distance: f32,
    index: usize,
}
impl PartialEq for Visit {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.distance.to_bits() == other.distance.to_bits()
    }
}
impl Eq for Visit {}
impl PartialOrd for Visit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Visit {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then(other.index.cmp(&self.index))
    }
}
impl CoastMap {
    /// # Errors
    /// A missing radial terrain hit or invalid size leaves the current planet intact.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn bake(surface: &SurfaceSampler<'_>, radius: f64, size: u32) -> Result<Self, String> {
        if ![32, 64, 128, 256, 512, 1024].contains(&size) || !radius.is_finite() || radius <= 0.0 {
            return Err("invalid ocean map size or radius".into());
        }
        let width = size as usize;
        let count = width * width * 6;
        let faces = std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..6)
                .map(|face| {
                    scope.spawn(move || {
                        (0..width * width)
                            .map(|local| {
                                let d = direction(face * width * width + local, size);
                                let (surface, channels) = surface
                                    .radial_surface(d)
                                    .ok_or_else(|| "ocean map ray missed terrain".to_string())?;
                                Ok(((1.0 - surface) as f32 * radius as f32, channels[2]))
                            })
                            .collect::<Result<Vec<_>, String>>()
                    })
                })
                .collect();
            jobs.into_iter()
                .map(|job| {
                    job.join()
                        .map_err(|_| "ocean map worker failed".to_string())?
                })
                .collect::<Result<Vec<_>, String>>()
        })?;
        let samples: Vec<_> = faces.into_iter().flatten().collect();
        let mut distances = vec![128.0_f32; count];
        let mut queue = BinaryHeap::new();
        for index in 0..count {
            let depth = samples[index].0;
            if depth <= 0.0 {
                distances[index] = 0.0;
                continue;
            }
            let centre = direction(index, size);
            for neighbour in neighbours(index, size) {
                let other = samples[neighbour].0;
                if other <= 0.0 {
                    let edge = centre.distance(direction(neighbour, size)) * radius;
                    let distance = (edge * f64::from(depth / (depth - other))) as f32;
                    if distance < distances[index] {
                        distances[index] = distance;
                        queue.push(Visit { distance, index });
                    }
                }
            }
        }
        while let Some(Visit { distance, index }) = queue.pop() {
            if distance > distances[index] {
                continue;
            }
            let centre = direction(index, size);
            for neighbour in neighbours(index, size) {
                if samples[neighbour].0 <= 0.0 {
                    continue;
                }
                let next = distance + (centre.distance(direction(neighbour, size)) * radius) as f32;
                if next < distances[neighbour] {
                    distances[neighbour] = next;
                    queue.push(Visit {
                        distance: next,
                        index: neighbour,
                    });
                }
            }
        }
        let pixels = samples
            .iter()
            .zip(&distances)
            .flat_map(|(&(depth, river), &distance)| {
                [
                    (depth.clamp(0.0, 10.0) * 25.5).floor() as u8,
                    (distance.clamp(0.0, 128.0) * (255.0 / 128.0)).floor() as u8,
                    (river.clamp(0.0, 1.0) * 255.0).round() as u8,
                    ((depth / 16.0 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8,
                ]
            })
            .collect();
        Ok(Self { size, pixels })
    }
}
