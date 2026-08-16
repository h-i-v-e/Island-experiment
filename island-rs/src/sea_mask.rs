#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::thread;

use crate::{ISLAND_WORLD_METRES, Terrain, Vec2, rivers::RiverMouth};

const COAST_WAVE_DEPTH_METRES: f32 = 5.0;
const MINIMUM_PLUME_LENGTH_METRES: f32 = 40.0;
const MAXIMUM_PLUME_LENGTH_METRES: f32 = 200.0;
const MINIMUM_PLUME_HALF_WIDTH_METRES: f32 = 8.0;
const MAXIMUM_PLUME_HALF_WIDTH_METRES: f32 = 50.0;

/// Interleaved linear RG8 data. R contains the land/shallow-water wave mask;
/// G contains the final river-mouth silt plume mask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeaMask {
    width: u32,
    height: u32,
    rg: Vec<u8>,
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
    pub fn rg(&self) -> &[u8] {
        &self.rg
    }
}

#[derive(Clone, Copy, Debug)]
struct MouthPlume {
    position: Vec2,
    downstream: Vec2,
    cross_stream: Vec2,
    length_metres: f32,
    half_width_metres: f32,
}

impl MouthPlume {
    fn new(mouth: RiverMouth, maximum_flow: u32) -> Self {
        let flow_scale = (mouth.flow as f32 / maximum_flow.max(1) as f32)
            .clamp(0.0, 1.0)
            .sqrt();
        let length_metres = MINIMUM_PLUME_LENGTH_METRES
            + (MAXIMUM_PLUME_LENGTH_METRES - MINIMUM_PLUME_LENGTH_METRES) * flow_scale;
        let half_width_metres = MINIMUM_PLUME_HALF_WIDTH_METRES
            + (MAXIMUM_PLUME_HALF_WIDTH_METRES - MINIMUM_PLUME_HALF_WIDTH_METRES) * flow_scale;
        Self {
            position: mouth.position,
            downstream: mouth.downstream,
            cross_stream: Vec2::new(-mouth.downstream.y, mouth.downstream.x),
            length_metres,
            half_width_metres,
        }
    }

    fn influence(self, point: Vec2) -> f32 {
        let offset_metres = (point - self.position) * ISLAND_WORLD_METRES;
        let along = offset_metres.dot(self.downstream);
        let across = offset_metres.dot(self.cross_stream).abs();
        if along < -self.half_width_metres
            || along > self.length_metres
            || across >= self.half_width_metres
        {
            return 0.0;
        }

        let along_weight = if along < 0.0 {
            smoothstep(-self.half_width_metres, 0.0, along)
        } else {
            1.0 - smoothstep(0.0, self.length_metres, along)
        };
        let across_weight = 1.0 - smoothstep(0.0, self.half_width_metres, across);
        along_weight * across_weight
    }

    fn normalized_bounds(self) -> [f32; 4] {
        let upstream = self.half_width_metres / ISLAND_WORLD_METRES;
        let downstream = self.length_metres / ISLAND_WORLD_METRES;
        let half_width = self.half_width_metres / ISLAND_WORLD_METRES;
        let start = self.position - self.downstream * upstream;
        let end = self.position + self.downstream * downstream;
        let across = self.cross_stream * half_width;
        let corners = [start - across, start + across, end - across, end + across];
        corners.iter().fold(
            [
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ],
            |[min_x, max_x, min_y, max_y], point| {
                [
                    min_x.min(point.x),
                    max_x.max(point.x),
                    min_y.min(point.y),
                    max_y.max(point.y),
                ]
            },
        )
    }
}

struct MouthIndex {
    dimension: usize,
    offsets: Vec<usize>,
    plume_indices: Vec<usize>,
}

impl MouthIndex {
    fn new(plumes: &[MouthPlume]) -> Self {
        let dimension = ((plumes.len() as f32 * 4.0).sqrt().ceil() as usize).clamp(8, 64);
        let bin_count = dimension * dimension;
        let mut counts = vec![0_usize; bin_count];
        for plume in plumes {
            let [min_x, max_x, min_y, max_y] = plume.normalized_bounds();
            let [min_x, max_x, min_y, max_y] = [
                bin_coordinate(min_x, dimension),
                bin_coordinate(max_x, dimension),
                bin_coordinate(min_y, dimension),
                bin_coordinate(max_y, dimension),
            ];
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    counts[y * dimension + x] += 1;
                }
            }
        }

        let mut offsets = Vec::with_capacity(bin_count + 1);
        offsets.push(0);
        for count in counts {
            offsets.push(offsets.last().copied().unwrap_or_default() + count);
        }
        let mut cursor = offsets[..bin_count].to_vec();
        let mut plume_indices = vec![0_usize; *offsets.last().unwrap_or(&0)];
        for (plume_index, plume) in plumes.iter().enumerate() {
            let [min_x, max_x, min_y, max_y] = plume.normalized_bounds();
            let [min_x, max_x, min_y, max_y] = [
                bin_coordinate(min_x, dimension),
                bin_coordinate(max_x, dimension),
                bin_coordinate(min_y, dimension),
                bin_coordinate(max_y, dimension),
            ];
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let bin = y * dimension + x;
                    plume_indices[cursor[bin]] = plume_index;
                    cursor[bin] += 1;
                }
            }
        }
        Self {
            dimension,
            offsets,
            plume_indices,
        }
    }

    fn candidates(&self, point: Vec2) -> &[usize] {
        let x = bin_coordinate(point.x, self.dimension);
        let y = bin_coordinate(point.y, self.dimension);
        let bin = y * self.dimension + x;
        &self.plume_indices[self.offsets[bin]..self.offsets[bin + 1]]
    }
}

pub(crate) fn bake_sea_mask(
    terrain: &Terrain,
    mouths: &[RiverMouth],
    width: u32,
    height: u32,
) -> Option<SeaMask> {
    let _timer = crate::profiling::StageTimer::new("sea_mask.bake");
    let width = width.max(1);
    let height = height.max(1);
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let byte_count = pixel_count.checked_mul(2)?;
    let maximum_flow = mouths.iter().map(|mouth| mouth.flow).max().unwrap_or(1);
    let plumes: Vec<_> = mouths
        .iter()
        .copied()
        .map(|mouth| MouthPlume::new(mouth, maximum_flow))
        .collect();
    let index = MouthIndex::new(&plumes);
    let mut rg = vec![0_u8; byte_count];
    let thread_count = mask_thread_count(pixel_count, height as usize);
    bake_with_threads(
        terrain,
        &plumes,
        &index,
        width,
        height,
        thread_count,
        &mut rg,
    );
    Some(SeaMask { width, height, rg })
}

fn bake_with_threads(
    terrain: &Terrain,
    plumes: &[MouthPlume],
    index: &MouthIndex,
    width: u32,
    height: u32,
    thread_count: usize,
    rg: &mut [u8],
) {
    let width_usize = width as usize;
    let rows_per_chunk = (height as usize).div_ceil(thread_count.max(1));
    thread::scope(|scope| {
        for (chunk, rows) in rg.chunks_mut(rows_per_chunk * width_usize * 2).enumerate() {
            let start_y = chunk * rows_per_chunk;
            scope.spawn(move || {
                bake_rows(terrain, plumes, index, width, height, start_y, rows);
            });
        }
    });
}

fn bake_rows(
    terrain: &Terrain,
    plumes: &[MouthPlume],
    index: &MouthIndex,
    width: u32,
    height: u32,
    start_y: usize,
    rows: &mut [u8],
) {
    let width_usize = width as usize;
    for (local_y, row) in rows.chunks_exact_mut(width_usize * 2).enumerate() {
        let y = start_y + local_y;
        let v = (y as f32 + 0.5) / height as f32;
        for (x, pixel) in row.chunks_exact_mut(2).enumerate() {
            let u = (x as f32 + 0.5) / width as f32;
            let point = Vec2::new(u, v);
            let elevation = terrain.sample(u, v);
            pixel[0] = quantize(coast_wave_weight(elevation));
            if elevation > 0.0 {
                continue;
            }
            let influence = index
                .candidates(point)
                .iter()
                .map(|&candidate| plumes[candidate].influence(point))
                .fold(0.0_f32, f32::max);
            pixel[1] = quantize(influence);
        }
    }
}

fn coast_wave_weight(elevation: f32) -> f32 {
    let depth_metres = (-elevation * ISLAND_WORLD_METRES).max(0.0);
    (1.0 - depth_metres / COAST_WAVE_DEPTH_METRES).clamp(0.0, 1.0)
}

fn smoothstep(minimum: f32, maximum: f32, value: f32) -> f32 {
    let t = ((value - minimum) / (maximum - minimum).max(f32::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn quantize(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn bin_coordinate(value: f32, dimension: usize) -> usize {
    (value.clamp(0.0, 1.0) * dimension as f32)
        .floor()
        .min((dimension - 1) as f32) as usize
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

    fn mouth() -> RiverMouth {
        RiverMouth {
            position: Vec2::new(0.5, 0.5),
            downstream: Vec2::X,
            flow: 100,
        }
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
    fn plume_is_directional_smooth_and_independent_of_depth() {
        let plume = MouthPlume::new(mouth(), 100);
        assert!((plume.influence(Vec2::new(0.5, 0.5)) - 1.0).abs() < f32::EPSILON);
        assert!(plume.influence(Vec2::new(0.54, 0.5)) > 0.0);
        assert!(plume.influence(Vec2::new(0.54, 0.5)) < 1.0);
        assert_eq!(plume.influence(Vec2::new(0.61, 0.5)).to_bits(), 0);
        assert_eq!(plume.influence(Vec2::new(0.5, 0.53)).to_bits(), 0);
    }

    #[test]
    fn mask_channels_have_the_exact_interleaved_contract() {
        let terrain = flat_terrain(-10.0 / ISLAND_WORLD_METRES);
        let mask = bake_sea_mask(&terrain, &[mouth()], 1, 1).unwrap();
        assert_eq!(mask.width(), 1);
        assert_eq!(mask.height(), 1);
        assert_eq!(mask.rg().len(), 2);
        assert_eq!(mask.rg()[0], 0);
        assert_eq!(mask.rg()[1], 255);
    }

    #[test]
    fn above_sea_terrain_suppresses_silt() {
        let terrain = flat_terrain(0.01);
        let mask = bake_sea_mask(&terrain, &[mouth()], 1, 1).unwrap();
        assert_eq!(mask.rg(), [255, 0]);
    }

    #[test]
    fn serial_and_threaded_bakes_match() {
        let terrain = flat_terrain(-2.5 / ISLAND_WORLD_METRES);
        let plumes = [MouthPlume::new(mouth(), 100)];
        let index = MouthIndex::new(&plumes);
        let mut serial = vec![0_u8; 128 * 128 * 2];
        let mut threaded = serial.clone();
        bake_with_threads(&terrain, &plumes, &index, 128, 128, 1, &mut serial);
        bake_with_threads(&terrain, &plumes, &index, 128, 128, 4, &mut threaded);
        assert_eq!(serial, threaded);
    }
}
