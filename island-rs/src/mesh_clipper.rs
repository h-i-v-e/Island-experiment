#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::collections::HashMap;

use crate::{
    BoundingBox, Mesh, Vec2, Vec3,
    mesh::{CLAMP_BOTTOM, CLAMP_LEFT, CLAMP_RIGHT, CLAMP_TOP},
};

const TRANSITION_FRACTION: f32 = 0.12;
const TRIANGLE_AREA_EPSILON: f32 = 1.0e-20;

pub(crate) struct MeshClipper<'a> {
    mesh: &'a Mesh,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipVertex {
    position: Vec3,
    normal: Vec3,
    uv: Vec2,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ClipVertexKey([u32; 8]);

struct MeshSampler<'a> {
    mesh: &'a Mesh,
    dimension: usize,
    offsets: Vec<usize>,
    faces: Vec<u32>,
}

impl From<ClipVertex> for ClipVertexKey {
    fn from(vertex: ClipVertex) -> Self {
        Self([
            vertex.position.x.to_bits(),
            vertex.position.y.to_bits(),
            vertex.position.z.to_bits(),
            vertex.normal.x.to_bits(),
            vertex.normal.y.to_bits(),
            vertex.normal.z.to_bits(),
            vertex.uv.x.to_bits(),
            vertex.uv.y.to_bits(),
        ])
    }
}

impl<'a> MeshClipper<'a> {
    pub(crate) const fn new(mesh: &'a Mesh) -> Self {
        Self { mesh }
    }

    #[must_use]
    pub(crate) fn sliced(
        &self,
        bounds: BoundingBox,
        coarser: Option<&Mesh>,
        clamp_sides: u8,
    ) -> Mesh {
        self.sliced_grid(bounds, 1, coarser, clamp_sides)
            .pop()
            .unwrap_or_default()
    }

    #[must_use]
    pub(crate) fn sliced_grid(
        &self,
        bounds: BoundingBox,
        divisions: usize,
        coarser: Option<&Mesh>,
        clamp_sides: u8,
    ) -> Vec<Mesh> {
        if divisions == 0 {
            return Vec::new();
        }
        let width = bounds.max.x - bounds.min.x;
        let height = bounds.max.y - bounds.min.y;
        if width <= 0.0 || height <= 0.0 {
            return vec![Mesh::default(); divisions * divisions];
        }

        let mut output = vec![Mesh::default(); divisions * divisions];
        let mut remaps: Vec<HashMap<ClipVertexKey, u32>> =
            (0..output.len()).map(|_| HashMap::new()).collect();
        let transition_width = transition_width(self.mesh, bounds);
        let coarse_sampler = coarser.map(MeshSampler::new);
        let coordinate = |value: f32, minimum: f32, span: f32| {
            (((value - minimum) / span * divisions as f32).floor() as usize).min(divisions - 1)
        };

        for triangle in self.mesh.triangles.chunks_exact(3) {
            let indices = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            if !triangle_touches_bounds(indices.map(|index| self.mesh.vertices[index]), bounds) {
                continue;
            }
            let source = indices.map(|index| {
                morphed_vertex(
                    self.mesh,
                    index,
                    bounds,
                    coarse_sampler.as_ref(),
                    clamp_sides,
                    transition_width,
                )
            });
            let minimum_x = source
                .iter()
                .map(|vertex| vertex.position.x)
                .fold(f32::MAX, f32::min);
            let maximum_x = source
                .iter()
                .map(|vertex| vertex.position.x)
                .fold(f32::MIN, f32::max);
            let minimum_y = source
                .iter()
                .map(|vertex| vertex.position.y)
                .fold(f32::MAX, f32::min);
            let maximum_y = source
                .iter()
                .map(|vertex| vertex.position.y)
                .fold(f32::MIN, f32::max);
            if maximum_x < bounds.min.x
                || minimum_x > bounds.max.x
                || maximum_y < bounds.min.y
                || minimum_y > bounds.max.y
            {
                continue;
            }
            let start_x = coordinate(minimum_x.max(bounds.min.x), bounds.min.x, width);
            let end_x = coordinate(maximum_x.min(bounds.max.x), bounds.min.x, width);
            let start_y = coordinate(minimum_y.max(bounds.min.y), bounds.min.y, height);
            let end_y = coordinate(maximum_y.min(bounds.max.y), bounds.min.y, height);
            for y in start_y..=end_y {
                for x in start_x..=end_x {
                    let tile = y * divisions + x;
                    let tile_bounds = BoundingBox::new(
                        Vec3::new(
                            bounds.min.x + width * x as f32 / divisions as f32,
                            bounds.min.y + height * y as f32 / divisions as f32,
                            bounds.min.z,
                        ),
                        Vec3::new(
                            bounds.min.x + width * (x + 1) as f32 / divisions as f32,
                            bounds.min.y + height * (y + 1) as f32 / divisions as f32,
                            bounds.max.z,
                        ),
                    );
                    append_clipped_triangle(
                        source,
                        tile_bounds,
                        bounds,
                        coarse_sampler.as_ref(),
                        clamp_sides,
                        &mut output[tile],
                        &mut remaps[tile],
                    );
                }
            }
        }
        output
    }
}

fn morphed_vertex(
    mesh: &Mesh,
    index: usize,
    bounds: BoundingBox,
    coarse_sampler: Option<&MeshSampler<'_>>,
    clamp_sides: u8,
    transition_width: f32,
) -> ClipVertex {
    let position = mesh.vertices[index];
    let uv = mesh.uv.get(index).copied().unwrap_or(position.truncate());
    let Some(coarse_sampler) = coarse_sampler.filter(|_| clamp_sides != 0) else {
        return ClipVertex {
            position,
            normal: mesh.normals[index],
            uv,
        };
    };
    let distance = clamp_distance(position, bounds, clamp_sides);
    let detail_weight = smoothstep((distance / transition_width).clamp(0.0, 1.0));
    let (coarse_height, coarse_normal) = coarse_sampler
        .sample(position.truncate())
        .unwrap_or((position.z, mesh.normals[index]));
    ClipVertex {
        position: Vec3::new(
            position.x,
            position.y,
            (position.z - coarse_height).mul_add(detail_weight, coarse_height),
        ),
        normal: coarse_normal
            .lerp(mesh.normals[index], detail_weight)
            .normalize_or_zero(),
        uv,
    }
}

fn transition_width(mesh: &Mesh, bounds: BoundingBox) -> f32 {
    let extent = (bounds.max.x - bounds.min.x)
        .abs()
        .min((bounds.max.y - bounds.min.y).abs());
    let (total, count) =
        mesh.triangles
            .chunks_exact(3)
            .take(4096)
            .fold((0.0, 0_u32), |(total, count), triangle| {
                let a = mesh.vertices[triangle[0] as usize];
                let b = mesh.vertices[triangle[1] as usize];
                (total + a.distance(b), count + 1)
            });
    let mean_edge = total / count.max(1) as f32;
    (mean_edge * 4.0)
        .max(extent * TRANSITION_FRACTION)
        .min(extent * 0.3)
}

fn clamp_distance(position: Vec3, bounds: BoundingBox, sides: u8) -> f32 {
    let mut distance = f32::INFINITY;
    if sides & CLAMP_TOP != 0 {
        distance = distance.min((bounds.max.y - position.y).max(0.0));
    }
    if sides & CLAMP_LEFT != 0 {
        distance = distance.min((position.x - bounds.min.x).max(0.0));
    }
    if sides & CLAMP_BOTTOM != 0 {
        distance = distance.min((position.y - bounds.min.y).max(0.0));
    }
    if sides & CLAMP_RIGHT != 0 {
        distance = distance.min((bounds.max.x - position.x).max(0.0));
    }
    distance
}

fn append_clipped_triangle(
    source: [ClipVertex; 3],
    tile_bounds: BoundingBox,
    outer_bounds: BoundingBox,
    coarse_sampler: Option<&MeshSampler<'_>>,
    clamp_sides: u8,
    output: &mut Mesh,
    remap: &mut HashMap<ClipVertexKey, u32>,
) {
    let mut first = [ClipVertex::default(); 8];
    first[..3].copy_from_slice(&source);
    let mut second = [ClipVertex::default(); 8];
    let mut length = clip_polygon(&first[..3], &mut second, 0, tile_bounds.min.x, true);
    length = clip_polygon(&second[..length], &mut first, 0, tile_bounds.max.x, false);
    length = clip_polygon(&first[..length], &mut second, 1, tile_bounds.min.y, true);
    length = clip_polygon(&second[..length], &mut first, 1, tile_bounds.max.y, false);
    if length < 3 {
        return;
    }

    let mut mapped = [0_u32; 8];
    for (index, vertex) in first[..length].iter().copied().enumerate() {
        let vertex = reproject_clip_vertex(vertex, source);
        let vertex = clamp_outer_vertex(vertex, outer_bounds, coarse_sampler, clamp_sides);
        let key = ClipVertexKey::from(vertex);
        mapped[index] = *remap.entry(key).or_insert_with(|| {
            let mapped = output.vertices.len() as u32;
            output.vertices.push(vertex.position);
            output.normals.push(vertex.normal);
            output.uv.push(vertex.uv);
            mapped
        });
    }
    for index in 1..length - 1 {
        let triangle = [mapped[0], mapped[index], mapped[index + 1]];
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0] {
            continue;
        }
        let [a, b, c] = triangle.map(|vertex| output.vertices[vertex as usize]);
        if (b - a).cross(c - a).length_squared() > TRIANGLE_AREA_EPSILON {
            output.triangles.extend(triangle);
        }
    }
}

fn reproject_clip_vertex(mut vertex: ClipVertex, source: [ClipVertex; 3]) -> ClipVertex {
    let positions = source.map(|source| source.position.truncate());
    let Some(weights) = barycentric(vertex.position.truncate(), positions) else {
        return canonicalize_clip_vertex(vertex);
    };
    vertex.position.z = source
        .iter()
        .zip(weights)
        .map(|(source, weight)| source.position.z * weight)
        .sum();
    vertex.normal = (source[0].normal * weights[0]
        + source[1].normal * weights[1]
        + source[2].normal * weights[2])
        .normalize_or_zero();
    vertex.uv = source
        .iter()
        .zip(weights)
        .map(|(source, weight)| source.uv * weight)
        .sum();
    canonicalize_clip_vertex(vertex)
}

fn clip_polygon(
    input: &[ClipVertex],
    output: &mut [ClipVertex; 8],
    axis: usize,
    boundary: f32,
    keep_greater: bool,
) -> usize {
    if input.is_empty() {
        return 0;
    }
    let component = |vertex: ClipVertex| {
        if axis == 0 {
            vertex.position.x
        } else {
            vertex.position.y
        }
    };
    let inside = |vertex: ClipVertex| {
        if keep_greater {
            component(vertex) >= boundary
        } else {
            component(vertex) <= boundary
        }
    };
    let mut length = 0;
    let mut previous = input[input.len() - 1];
    let mut previous_inside = inside(previous);
    for &current in input {
        let current_inside = inside(current);
        if current_inside != previous_inside {
            let denominator = component(current) - component(previous);
            if denominator.abs() > f32::EPSILON {
                let interpolation = (boundary - component(previous)) / denominator;
                let mut intersection = canonicalize_clip_vertex(interpolate_clip_vertex(
                    previous,
                    current,
                    interpolation,
                ));
                if axis == 0 {
                    intersection.position.x = boundary;
                } else {
                    intersection.position.y = boundary;
                }
                push_clip_vertex(output, &mut length, intersection);
            }
        }
        if current_inside {
            push_clip_vertex(output, &mut length, current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    length
}

fn interpolate_clip_vertex(a: ClipVertex, b: ClipVertex, interpolation: f32) -> ClipVertex {
    ClipVertex {
        position: a.position.lerp(b.position, interpolation),
        normal: a.normal.lerp(b.normal, interpolation).normalize_or_zero(),
        uv: a.uv.lerp(b.uv, interpolation),
    }
}

fn canonicalize_clip_vertex(mut vertex: ClipVertex) -> ClipVertex {
    let canonical = |value: f32| (value * 10_000_000.0).round() / 10_000_000.0;
    vertex.position = vertex.position.map(canonical);
    vertex.normal = vertex.normal.map(canonical).normalize_or_zero();
    vertex.uv = vertex.uv.map(canonical);
    vertex
}

fn push_clip_vertex(output: &mut [ClipVertex; 8], length: &mut usize, vertex: ClipVertex) {
    if *length == 0
        || output[*length - 1]
            .position
            .distance_squared(vertex.position)
            > f32::EPSILON.powi(2)
    {
        output[*length] = vertex;
        *length += 1;
    }
}

fn clamp_outer_vertex(
    mut vertex: ClipVertex,
    bounds: BoundingBox,
    coarse_sampler: Option<&MeshSampler<'_>>,
    clamp_sides: u8,
) -> ClipVertex {
    let Some(coarse_sampler) = coarse_sampler.filter(|_| clamp_sides != 0) else {
        return vertex;
    };
    let epsilon =
        ((bounds.max.x - bounds.min.x).abs() + (bounds.max.y - bounds.min.y).abs()) * 1.0e-6;
    let mut boundary = false;
    if clamp_sides & CLAMP_TOP != 0 && (vertex.position.y - bounds.max.y).abs() <= epsilon {
        vertex.position.y = bounds.max.y;
        vertex.uv.y = bounds.max.y;
        boundary = true;
    }
    if clamp_sides & CLAMP_LEFT != 0 && (vertex.position.x - bounds.min.x).abs() <= epsilon {
        vertex.position.x = bounds.min.x;
        vertex.uv.x = bounds.min.x;
        boundary = true;
    }
    if clamp_sides & CLAMP_BOTTOM != 0 && (vertex.position.y - bounds.min.y).abs() <= epsilon {
        vertex.position.y = bounds.min.y;
        vertex.uv.y = bounds.min.y;
        boundary = true;
    }
    if clamp_sides & CLAMP_RIGHT != 0 && (vertex.position.x - bounds.max.x).abs() <= epsilon {
        vertex.position.x = bounds.max.x;
        vertex.uv.x = bounds.max.x;
        boundary = true;
    }
    if boundary && let Some((height, normal)) = coarse_sampler.sample(vertex.position.truncate()) {
        vertex.position.z = height;
        vertex.normal = normal;
    }
    vertex
}

impl<'a> MeshSampler<'a> {
    fn new(mesh: &'a Mesh) -> Self {
        let face_count = mesh.triangles.len() / 3;
        let dimension = ((face_count as f32 / 8.0).sqrt().ceil() as usize).clamp(8, 2048);
        let bin_count = dimension * dimension;
        let mut counts = vec![0_usize; bin_count];
        for triangle in mesh.triangles.chunks_exact(3) {
            let [minimum_x, maximum_x, minimum_y, maximum_y] =
                triangle_bin_bounds(mesh, triangle, dimension);
            for y in minimum_y..=maximum_y {
                for x in minimum_x..=maximum_x {
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
        let mut faces = vec![0_u32; *offsets.last().unwrap_or(&0)];
        for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
            let [minimum_x, maximum_x, minimum_y, maximum_y] =
                triangle_bin_bounds(mesh, triangle, dimension);
            for y in minimum_y..=maximum_y {
                for x in minimum_x..=maximum_x {
                    let bin = y * dimension + x;
                    faces[cursor[bin]] = face as u32;
                    cursor[bin] += 1;
                }
            }
        }
        Self {
            mesh,
            dimension,
            offsets,
            faces,
        }
    }

    fn sample(&self, point: Vec2) -> Option<(f32, Vec3)> {
        let x = bin_coordinate(point.x, self.dimension);
        let y = bin_coordinate(point.y, self.dimension);
        let bin = y * self.dimension + x;
        self.faces[self.offsets[bin]..self.offsets[bin + 1]]
            .iter()
            .find_map(|&face| self.sample_face(face as usize, point))
    }

    fn sample_face(&self, face: usize, point: Vec2) -> Option<(f32, Vec3)> {
        let offset = face * 3;
        let triangle = &self.mesh.triangles[offset..offset + 3];
        let indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let positions = indices.map(|index| self.mesh.vertices[index]);
        barycentric(point, positions.map(Vec3::truncate)).map(|weights| {
            let height = positions[0].z * weights[0]
                + positions[1].z * weights[1]
                + positions[2].z * weights[2];
            let normal = (self.mesh.normals[indices[0]] * weights[0]
                + self.mesh.normals[indices[1]] * weights[1]
                + self.mesh.normals[indices[2]] * weights[2])
                .normalize_or_zero();
            (height, normal)
        })
    }
}

fn triangle_bin_bounds(mesh: &Mesh, triangle: &[u32], dimension: usize) -> [usize; 4] {
    let mut minimum = Vec2::splat(f32::MAX);
    let mut maximum = Vec2::splat(f32::MIN);
    for &index in triangle {
        let point = mesh.vertices[index as usize].truncate();
        minimum = minimum.min(point);
        maximum = maximum.max(point);
    }
    [
        bin_coordinate(minimum.x, dimension),
        bin_coordinate(maximum.x, dimension),
        bin_coordinate(minimum.y, dimension),
        bin_coordinate(maximum.y, dimension),
    ]
}

fn bin_coordinate(value: f32, dimension: usize) -> usize {
    (value.clamp(0.0, 1.0) * dimension as f32)
        .floor()
        .min((dimension - 1) as f32) as usize
}

fn triangle_touches_bounds(vertices: [Vec3; 3], bounds: BoundingBox) -> bool {
    let minimum_x = vertices
        .iter()
        .map(|vertex| vertex.x)
        .fold(f32::MAX, f32::min);
    let maximum_x = vertices
        .iter()
        .map(|vertex| vertex.x)
        .fold(f32::MIN, f32::max);
    let minimum_y = vertices
        .iter()
        .map(|vertex| vertex.y)
        .fold(f32::MAX, f32::min);
    let maximum_y = vertices
        .iter()
        .map(|vertex| vertex.y)
        .fold(f32::MIN, f32::max);
    maximum_x >= bounds.min.x
        && minimum_x <= bounds.max.x
        && maximum_y >= bounds.min.y
        && minimum_y <= bounds.max.y
}

fn barycentric(point: Vec2, positions: [Vec2; 3]) -> Option<[f32; 3]> {
    let denominator = (positions[1] - positions[0]).perp_dot(positions[2] - positions[0]);
    if denominator.abs() <= f32::EPSILON {
        return None;
    }
    let second = (point - positions[0]).perp_dot(positions[2] - positions[0]) / denominator;
    let third = (positions[1] - positions[0]).perp_dot(point - positions[0]) / denominator;
    let first = 1.0 - second - third;
    let epsilon = -1.0e-5;
    (first >= epsilon && second >= epsilon && third >= epsilon).then_some([first, second, third])
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}
