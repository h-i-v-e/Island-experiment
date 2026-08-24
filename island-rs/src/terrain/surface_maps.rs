use super::{
    Mesh, StageTimer, SurfaceMaps, SurfaceSample, Terrain, TriangleIndex, Vec2, Vec3,
    sample_mesh_surface, thread,
};

pub(super) const OCCLUSION_OFFSETS: [(isize, isize); 15] = [
    (-8, -2),
    (-4, 2),
    (-2, 1),
    (-1, 1),
    (-1, -1),
    (-1, -4),
    (1, 2),
    (1, 1),
    (1, -1),
    (1, -2),
    (2, 4),
    (2, -1),
    (2, -8),
    (4, -2),
    (8, 2),
];

const CANOPY_OCCLUSION_VALUE: u8 = 112;
const PROJECTED_TRIANGLE_EPSILON: f32 = 1.0e-5;

#[derive(Clone, Copy)]
struct SurfaceMapSampler<'a> {
    high_detail: &'a Terrain,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy)]
struct DetailMapBaker<'a> {
    surface: SurfaceMapSampler<'a>,
    target: &'a Mesh,
    target_index: &'a TriangleIndex,
}

pub(super) fn bake_surface_maps(
    high_detail: &Terrain,
    target: Option<&Mesh>,
    low_poly_canopy: Option<&Mesh>,
    width: u32,
    height: u32,
) -> SurfaceMaps {
    let _timer = StageTimer::new("surface_maps.bake");
    let width_usize = width as usize;
    let height_usize = height as usize;
    let pixel_count = width_usize * height_usize;
    let mut samples = vec![SurfaceSample::default(); pixel_count];
    let mut normal_rgb = vec![0_u8; pixel_count * 3];
    let thread_count = surface_map_thread_count(pixel_count, height_usize);
    let rows_per_chunk = height_usize.div_ceil(thread_count);
    let surface = SurfaceMapSampler {
        high_detail,
        width,
        height,
    };

    if let Some(target) = target {
        let target_index = TriangleIndex::new(target);
        let baker = DetailMapBaker {
            surface,
            target,
            target_index: &target_index,
        };
        thread::scope(|scope| {
            for (chunk, (sample_rows, normal_rows)) in samples
                .chunks_mut(rows_per_chunk * width_usize)
                .zip(normal_rgb.chunks_mut(rows_per_chunk * width_usize * 3))
                .enumerate()
            {
                let start_y = chunk * rows_per_chunk;
                scope.spawn(move || {
                    baker.bake_rows(start_y, sample_rows, normal_rows);
                });
            }
        });
    } else {
        thread::scope(|scope| {
            for (chunk, (sample_rows, normal_rows)) in samples
                .chunks_mut(rows_per_chunk * width_usize)
                .zip(normal_rgb.chunks_mut(rows_per_chunk * width_usize * 3))
                .enumerate()
            {
                let start_y = chunk * rows_per_chunk;
                scope.spawn(move || {
                    surface.bake_rows(start_y, sample_rows, normal_rows);
                });
            }
        });
    }

    let mut occlusion = vec![u8::MAX; pixel_count];
    thread::scope(|scope| {
        for (chunk, rows) in occlusion
            .chunks_mut(rows_per_chunk * width_usize)
            .enumerate()
        {
            let start_y = chunk * rows_per_chunk;
            let samples = &samples;
            scope.spawn(move || {
                bake_occlusion_rows(samples, width_usize, height_usize, start_y, rows);
            });
        }
    });
    if let Some(canopy) = low_poly_canopy {
        project_canopy_occlusion(&mut occlusion, canopy, width_usize, height_usize);
    }

    SurfaceMaps {
        width,
        height,
        normal_rgb,
        occlusion,
    }
}

/// Projects the coarse canopy vertically into the terrain occlusion map.
///
/// The low-poly forest mesh is borrowed directly and the existing map is
/// darkened in place, so baking does not duplicate the island-wide canopy.
fn project_canopy_occlusion(occlusion: &mut [u8], canopy: &Mesh, width: usize, height: usize) {
    if width == 0 || height == 0 || occlusion.len() != width.saturating_mul(height) {
        return;
    }
    let pixel_scale = Vec2::new(
        width.saturating_sub(1) as f32,
        height.saturating_sub(1) as f32,
    );
    for triangle in canopy.triangles.chunks_exact(3) {
        let &[first, second, third] = triangle else {
            continue;
        };
        let (Some(first), Some(second), Some(third)) = (
            canopy.vertices.get(first as usize),
            canopy.vertices.get(second as usize),
            canopy.vertices.get(third as usize),
        ) else {
            continue;
        };
        let projected = [
            first.truncate() * pixel_scale,
            second.truncate() * pixel_scale,
            third.truncate() * pixel_scale,
        ];
        rasterize_canopy_triangle(occlusion, width, height, projected);
    }
}

fn rasterize_canopy_triangle(
    occlusion: &mut [u8],
    width: usize,
    height: usize,
    triangle: [Vec2; 3],
) {
    if triangle.iter().any(|vertex| !vertex.is_finite()) {
        return;
    }
    let area = projected_edge(triangle[0], triangle[1], triangle[2]);
    if area.abs() <= PROJECTED_TRIANGLE_EPSILON {
        return;
    }
    let maximum_x = width - 1;
    let maximum_y = height - 1;
    let minimum_pixel_x = triangle
        .iter()
        .map(|vertex| vertex.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0.0, maximum_x as f32) as usize;
    let maximum_pixel_x = triangle
        .iter()
        .map(|vertex| vertex.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0.0, maximum_x as f32) as usize;
    let minimum_pixel_y = triangle
        .iter()
        .map(|vertex| vertex.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0.0, maximum_y as f32) as usize;
    let maximum_pixel_y = triangle
        .iter()
        .map(|vertex| vertex.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0.0, maximum_y as f32) as usize;

    for y in minimum_pixel_y..=maximum_pixel_y {
        for x in minimum_pixel_x..=maximum_pixel_x {
            let point = Vec2::new(x as f32, y as f32);
            let edges = [
                projected_edge(triangle[0], triangle[1], point),
                projected_edge(triangle[1], triangle[2], point),
                projected_edge(triangle[2], triangle[0], point),
            ];
            let has_negative = edges.iter().any(|&edge| edge < -PROJECTED_TRIANGLE_EPSILON);
            let has_positive = edges.iter().any(|&edge| edge > PROJECTED_TRIANGLE_EPSILON);
            if !has_negative || !has_positive {
                let pixel = &mut occlusion[y * width + x];
                *pixel = (*pixel).min(CANOPY_OCCLUSION_VALUE);
            }
        }
    }
}

fn projected_edge(start: Vec2, end: Vec2, point: Vec2) -> f32 {
    (point.x - start.x).mul_add(end.y - start.y, -(point.y - start.y) * (end.x - start.x))
}

pub(super) fn surface_map_thread_count(pixel_count: usize, height: usize) -> usize {
    if pixel_count < 65_536 {
        return 1;
    }
    thread::available_parallelism()
        .map_or(1, usize::from)
        .min(height)
}

impl SurfaceMapSampler<'_> {
    fn bake_rows(self, start_y: usize, samples: &mut [SurfaceSample], normal_rgb: &mut [u8]) {
        let width_usize = self.width as usize;
        for (local_y, (sample_row, normal_row)) in samples
            .chunks_exact_mut(width_usize)
            .zip(normal_rgb.chunks_exact_mut(width_usize * 3))
            .enumerate()
        {
            let y = start_y + local_y;
            let v = y as f32 / self.height.saturating_sub(1).max(1) as f32;
            for (x, (sample, normal_pixel)) in sample_row
                .iter_mut()
                .zip(normal_row.chunks_exact_mut(3))
                .enumerate()
            {
                let u = x as f32 / self.width.saturating_sub(1).max(1) as f32;
                let (elevation, normal) = self.high_detail.sample_surface(u, v);
                *sample = SurfaceSample {
                    position: Vec3::new(u, v, elevation),
                    normal,
                };
                normal_pixel[0] = signed_normal_byte(normal.x);
                normal_pixel[1] = signed_normal_byte(normal.y);
                normal_pixel[2] = signed_normal_byte(normal.z);
            }
        }
    }
}

impl DetailMapBaker<'_> {
    fn bake_rows(self, start_y: usize, samples: &mut [SurfaceSample], normal_rgb: &mut [u8]) {
        let width_usize = self.surface.width as usize;
        for (local_y, (sample_row, normal_row)) in samples
            .chunks_exact_mut(width_usize)
            .zip(normal_rgb.chunks_exact_mut(width_usize * 3))
            .enumerate()
        {
            let y = start_y + local_y;
            let v = y as f32 / self.surface.height.saturating_sub(1).max(1) as f32;
            for (x, (sample, normal_pixel)) in sample_row
                .iter_mut()
                .zip(normal_row.chunks_exact_mut(3))
                .enumerate()
            {
                let u = x as f32 / self.surface.width.saturating_sub(1).max(1) as f32;
                let (elevation, high_normal) = self.surface.high_detail.sample_surface(u, v);
                let (_, target_normal) = sample_mesh_surface(self.target, self.target_index, u, v);
                *sample = SurfaceSample {
                    position: Vec3::new(u, v, elevation),
                    normal: high_normal,
                };
                let detail_normal = (Vec3::Z + high_normal - target_normal)
                    .try_normalize()
                    .unwrap_or(Vec3::Z);
                normal_pixel[0] = signed_normal_byte(detail_normal.y);
                normal_pixel[1] = signed_normal_byte(detail_normal.x);
                normal_pixel[2] = (detail_normal.z.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }
}

pub(super) fn signed_normal_byte(value: f32) -> u8 {
    value.mul_add(127.5, 127.5).clamp(0.0, 255.0) as u8
}

pub(super) fn bake_occlusion_rows(
    samples: &[SurfaceSample],
    width: usize,
    height: usize,
    start_y: usize,
    output: &mut [u8],
) {
    for (local_y, row) in output.chunks_exact_mut(width).enumerate() {
        let y = start_y + local_y;
        for (x, value) in row.iter_mut().enumerate() {
            let sample = samples[y * width + x];
            let mut total = 0.0_f32;
            let mut count = 0_u32;
            for (offset_x, offset_y) in OCCLUSION_OFFSETS {
                let Some(px) = x.checked_add_signed(offset_x) else {
                    continue;
                };
                let Some(py) = y.checked_add_signed(offset_y) else {
                    continue;
                };
                if px >= width || py >= height {
                    continue;
                }
                let direction = samples[py * width + px].position - sample.position;
                let Some(direction) = direction.try_normalize() else {
                    continue;
                };
                total += direction.dot(sample.normal).max(0.0);
                count += 1;
            }
            if count > 0 {
                *value = ((1.0 - total / count as f32) * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_poly_canopy_projects_into_existing_occlusion() {
        let canopy = Mesh {
            vertices: vec![
                Vec3::new(0.25, 0.25, 0.20),
                Vec3::new(0.75, 0.25, 0.20),
                Vec3::new(0.50, 0.75, 0.20),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let mut occlusion = vec![u8::MAX; 81];
        occlusion[4 * 9 + 4] = 96;

        project_canopy_occlusion(&mut occlusion, &canopy, 9, 9);

        assert_eq!(occlusion[2 * 9 + 4], CANOPY_OCCLUSION_VALUE);
        assert_eq!(occlusion[4 * 9 + 4], 96);
        assert_eq!(occlusion[8 * 9 + 8], u8::MAX);
    }
}
