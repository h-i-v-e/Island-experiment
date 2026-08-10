#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use std::thread;

use crate::{Island, Vec3};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raster {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Raster {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 3],
        }
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[must_use]
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    pub fn render(&mut self, island: &Island) {
        let width = self.width;
        let height = self.height;
        let available_threads = thread::available_parallelism().map_or(1, usize::from);
        let pixel_count = width as usize * height as usize;
        let thread_count = if pixel_count >= 65_536 {
            available_threads.min(height as usize)
        } else {
            1
        };
        let rows_per_chunk = (height as usize).div_ceil(thread_count);
        let bytes_per_row = width as usize * 3;
        thread::scope(|scope| {
            for (chunk, pixels) in self
                .pixels
                .chunks_mut(rows_per_chunk * bytes_per_row)
                .enumerate()
            {
                let start_y = chunk * rows_per_chunk;
                scope.spawn(move || {
                    render_rows(island, width, height, start_y, pixels);
                });
            }
        });
        self.draw_rivers(island);
    }

    fn set(&mut self, x: u32, y: u32, colour: [u8; 3]) {
        let offset = (y as usize * self.width as usize + x as usize) * 3;
        self.pixels[offset..offset + 3].copy_from_slice(&colour);
    }

    fn draw_rivers(&mut self, island: &Island) {
        let max_flow = island
            .rivers()
            .iter()
            .flat_map(|river| &river.nodes)
            .map(|node| node.flow)
            .max()
            .unwrap_or(1) as f32;
        for river in island.rivers() {
            for pair in river.nodes.windows(2) {
                let a = pair[0].position;
                let b = pair[1].position;
                let x0 = (a.x * (self.width - 1) as f32).round() as i32;
                let y0 = (a.y * (self.height - 1) as f32).round() as i32;
                let x1 = (b.x * (self.width - 1) as f32).round() as i32;
                let y1 = (b.y * (self.height - 1) as f32).round() as i32;
                let thickness =
                    ((pair[0].flow.max(pair[1].flow) as f32 / max_flow).sqrt() * 2.0).ceil() as i32;
                self.line(x0, y0, x1, y1, thickness, [55, 151, 205]);
            }
        }
    }

    fn line(
        &mut self,
        mut x0: i32,
        mut y0: i32,
        x1: i32,
        y1: i32,
        thickness: i32,
        colour: [u8; 3],
    ) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            for offset_y in -thickness / 2..=thickness / 2 {
                for offset_x in -thickness / 2..=thickness / 2 {
                    let pixel_x = x0 + offset_x;
                    let pixel_y = y0 + offset_y;
                    if pixel_x >= 0
                        && pixel_y >= 0
                        && pixel_x < self.width as i32
                        && pixel_y < self.height as i32
                    {
                        self.set(pixel_x as u32, pixel_y as u32, colour);
                    }
                }
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let twice = error * 2;
            if twice >= dy {
                error += dy;
                x0 += sx;
            }
            if twice <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }
}

fn render_rows(island: &Island, width: u32, height: u32, start_y: usize, pixels: &mut [u8]) {
    let sun = Vec3::new(-0.35, 0.45, 0.82).normalize();
    let max_height = island.options().max_height.max(f32::EPSILON);
    let bytes_per_row = width as usize * 3;
    for (local_y, row) in pixels.chunks_exact_mut(bytes_per_row).enumerate() {
        let y = start_y + local_y;
        let v = y as f32 / height.saturating_sub(1).max(1) as f32;
        for x in 0..width {
            let u = x as f32 / width.saturating_sub(1).max(1) as f32;
            let (elevation, normal) = island.terrain().sample_surface(u, v);
            let light = normal.dot(sun).max(0.0).mul_add(0.72, 0.28);
            let colour = if elevation <= 0.0 {
                let depth = (-elevation / max_height).clamp(0.0, 1.0);
                [
                    (10.0 * light) as u8,
                    ((85.0 - depth * 38.0) * light) as u8,
                    ((145.0 - depth * 55.0) * light) as u8,
                ]
            } else {
                let height = (elevation / max_height).clamp(0.0, 1.0);
                let slope = 1.0 - normal.z.clamp(0.0, 1.0);
                let (red, green, blue) = if height > 0.72 || slope > 0.62 {
                    let snow = ((height - 0.72) / 0.28).clamp(0.0, 1.0);
                    (
                        112.0_f32.mul_add(1.0 - snow, 235.0 * snow),
                        105.0_f32.mul_add(1.0 - snow, 238.0 * snow),
                        91.0_f32.mul_add(1.0 - snow, 240.0 * snow),
                    )
                } else if height < 0.045 {
                    (194.0, 178.0, 118.0)
                } else {
                    (48.0 + height * 34.0, 118.0 - height * 25.0, 42.0)
                };
                [
                    (red * light) as u8,
                    (green * light) as u8,
                    (blue * light) as u8,
                ]
            };
            let offset = x as usize * 3;
            row[offset..offset + 3].copy_from_slice(&colour);
        }
    }
}
