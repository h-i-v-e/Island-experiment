#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::thread;

use crate::{ISLAND_WORLD_METRES, Terrain};

const COAST_WAVE_DEPTH_METRES: f32 = 5.0;
const LAND_DISTANCE_RANGE_METRES: f32 = 16.0;

/// Interleaved linear RGBA8 data. R contains the land/shallow-water wave mask,
/// G contains distance from land normalized over sixteen metres, B contains
/// finalized river-bed and accumulated submerged river-carve coverage, and A is reserved at full
/// strength.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeaMask {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl SeaMask {
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

pub(crate) fn bake_sea_mask(
    terrain: &Terrain,
    distance_to_land: &[f32],
    wave_suppression: &[f32],
    width: u32,
    height: u32,
) -> Option<SeaMask> {
    let _timer = crate::profiling::StageTimer::new("sea_mask.bake");
    let width = width.max(1);
    let height = height.max(1);
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let byte_count = pixel_count.checked_mul(4)?;
    if distance_to_land.len() != terrain.vertex_count()
        || wave_suppression.len() != terrain.vertex_count()
    {
        return None;
    }
    let mut rgba = vec![0_u8; byte_count];
    let thread_count = mask_thread_count(pixel_count, height as usize);
    bake_with_threads(
        terrain,
        distance_to_land,
        wave_suppression,
        width,
        height,
        thread_count,
        &mut rgba,
    );
    Some(SeaMask {
        width,
        height,
        rgba,
    })
}

fn bake_with_threads(
    terrain: &Terrain,
    distance_to_land: &[f32],
    wave_suppression: &[f32],
    width: u32,
    height: u32,
    thread_count: usize,
    rgba: &mut [u8],
) {
    let width_usize = width as usize;
    let rows_per_chunk = (height as usize).div_ceil(thread_count.max(1));
    thread::scope(|scope| {
        for (chunk, rows) in rgba
            .chunks_mut(rows_per_chunk * width_usize * 4)
            .enumerate()
        {
            let start_y = chunk * rows_per_chunk;
            scope.spawn(move || {
                bake_rows(
                    terrain,
                    distance_to_land,
                    wave_suppression,
                    width,
                    height,
                    start_y,
                    rows,
                );
            });
        }
    });
}

fn bake_rows(
    terrain: &Terrain,
    distance_to_land: &[f32],
    wave_suppression: &[f32],
    width: u32,
    height: u32,
    start_y: usize,
    rows: &mut [u8],
) {
    let width_usize = width as usize;
    for (local_y, row) in rows.chunks_exact_mut(width_usize * 4).enumerate() {
        let y = start_y + local_y;
        let v = (y as f32 + 0.5) / height as f32;
        for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
            let u = (x as f32 + 0.5) / width as f32;
            let elevation = terrain.sample(u, v);
            pixel[0] = quantize(coast_wave_weight(elevation));
            pixel[3] = u8::MAX;
            if elevation > 0.0 {
                continue;
            }
            let land_distance = terrain.sample_vertex_scalar(distance_to_land, u, v);
            pixel[1] = quantize(land_distance_weight(land_distance));
            pixel[2] = quantize(terrain.sample_vertex_scalar(wave_suppression, u, v));
        }
    }
}

fn coast_wave_weight(elevation: f32) -> f32 {
    let depth_metres = (-elevation * ISLAND_WORLD_METRES).max(0.0);
    (1.0 - depth_metres / COAST_WAVE_DEPTH_METRES).clamp(0.0, 1.0)
}

fn land_distance_weight(normalized_distance: f32) -> f32 {
    (normalized_distance * ISLAND_WORLD_METRES / LAND_DISTANCE_RANGE_METRES).clamp(0.0, 1.0)
}

fn quantize(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn mask_thread_count(pixel_count: usize, height: usize) -> usize {
    if pixel_count < 65_536 {
        return 1;
    }
    thread::available_parallelism()
        .map_or(1, usize::from)
        .min(height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mesh, Vec3};

    fn flat_terrain(elevation: f32) -> Terrain {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, elevation),
                Vec3::new(1.0, 0.0, elevation),
                Vec3::new(0.0, 1.0, elevation),
                Vec3::new(1.0, 1.0, elevation),
            ],
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        };
        mesh.calculate_normals();
        Terrain::new(mesh)
    }

    #[test]
    fn coast_wave_weight_uses_a_fixed_five_metre_depth() {
        assert_eq!(quantize(coast_wave_weight(0.1)), 255);
        assert_eq!(quantize(coast_wave_weight(0.0)), 255);
        assert_eq!(quantize(coast_wave_weight(-2.5 / ISLAND_WORLD_METRES)), 128);
        assert_eq!(quantize(coast_wave_weight(-5.0 / ISLAND_WORLD_METRES)), 0);
        assert_eq!(quantize(coast_wave_weight(-20.0 / ISLAND_WORLD_METRES)), 0);
    }

    #[test]
    fn mask_channels_have_the_exact_interleaved_contract() {
        let terrain = flat_terrain(-10.0 / ISLAND_WORLD_METRES);
        let eight_metres = 8.0 / ISLAND_WORLD_METRES;
        let mask = bake_sea_mask(&terrain, &[eight_metres; 4], &[0.0; 4], 1, 1).unwrap();
        assert_eq!(mask.width(), 1);
        assert_eq!(mask.height(), 1);
        assert_eq!(mask.rgba(), [0, 128, 0, 255]);
    }

    #[test]
    fn above_sea_terrain_suppresses_land_distance() {
        let terrain = flat_terrain(0.01);
        let mask = bake_sea_mask(&terrain, &[1.0; 4], &[1.0; 4], 1, 1).unwrap();
        assert_eq!(mask.rgba(), [255, 0, 0, 255]);
    }

    #[test]
    fn submerged_river_carve_is_exported_in_blue() {
        let terrain = flat_terrain(-10.0 / ISLAND_WORLD_METRES);
        let mask = bake_sea_mask(&terrain, &[0.0; 4], &[1.0; 4], 1, 1).unwrap();
        assert_eq!(mask.rgba(), [0, 0, 255, 255]);
    }

    #[test]
    fn land_distance_is_linear_and_saturates_at_sixteen_metres() {
        assert_eq!(quantize(land_distance_weight(0.0)), 0);
        assert_eq!(
            quantize(land_distance_weight(8.0 / ISLAND_WORLD_METRES)),
            128
        );
        assert_eq!(
            quantize(land_distance_weight(16.0 / ISLAND_WORLD_METRES)),
            255
        );
        assert_eq!(
            quantize(land_distance_weight(32.0 / ISLAND_WORLD_METRES)),
            255
        );
    }

    #[test]
    fn serial_and_threaded_bakes_match() {
        let terrain = flat_terrain(-2.5 / ISLAND_WORLD_METRES);
        let distance_to_land = [0.0, 0.005, 0.01, 0.02];
        let wave_suppression = [0.0, 0.25, 0.75, 1.0];
        let mut serial = vec![0_u8; 128 * 128 * 4];
        let mut threaded = serial.clone();
        bake_with_threads(
            &terrain,
            &distance_to_land,
            &wave_suppression,
            128,
            128,
            1,
            &mut serial,
        );
        bake_with_threads(
            &terrain,
            &distance_to_land,
            &wave_suppression,
            128,
            128,
            4,
            &mut threaded,
        );
        assert_eq!(serial, threaded);
    }
}
