#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::Index,
};

use rayon::prelude::*;

use crate::{BoundingBox, Vec2, Vec3};

pub const CLAMP_TOP: u8 = 1;
pub const CLAMP_LEFT: u8 = 2;
pub const CLAMP_BOTTOM: u8 = 4;
pub const CLAMP_RIGHT: u8 = 8;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub triangles: Vec<u32>,
    pub uv: Vec<Vec2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NewVertexStencil {
    pub vertex: u32,
    pub surrounding: [u32; 4],
    pub count: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TessellationResult {
    pub mesh: Mesh,
    pub new_vertices: Vec<NewVertexStencil>,
}

/// Compact compressed-sparse-row mesh connectivity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adjacency {
    offsets: Vec<usize>,
    neighbours: Vec<usize>,
}

impl Adjacency {
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &[usize]> {
        self.offsets
            .windows(2)
            .map(|range| &self.neighbours[range[0]..range[1]])
    }
}

impl Index<usize> for Adjacency {
    type Output = [usize];

    fn index(&self, index: usize) -> &Self::Output {
        &self.neighbours[self.offsets[index]..self.offsets[index + 1]]
    }
}

impl Mesh {
    /// Constructs a free-form Delaunay mesh with the Bowyer-Watson algorithm.
    #[must_use]
    pub fn delaunay(points: &[Vec2]) -> Self {
        if points.len() < 3 {
            return Self::default();
        }

        let point_count = points.len();
        let mut working_points = points.to_vec();
        working_points.extend([
            Vec2::new(-10.0, -10.0),
            Vec2::new(10.0, -10.0),
            Vec2::new(0.5, 10.0),
        ]);
        let mut triangles = vec![[point_count, point_count + 1, point_count + 2]];

        for point_index in 0..point_count {
            let point = working_points[point_index];
            let mut bad = Vec::new();
            for (triangle_index, triangle) in triangles.iter().enumerate() {
                if circumcircle_contains(
                    working_points[triangle[0]],
                    working_points[triangle[1]],
                    working_points[triangle[2]],
                    point,
                ) {
                    bad.push(triangle_index);
                }
            }

            let mut boundary = BTreeMap::<(usize, usize), (usize, usize, u8)>::new();
            for &triangle_index in &bad {
                let triangle = triangles[triangle_index];
                for (a, b) in [
                    (triangle[0], triangle[1]),
                    (triangle[1], triangle[2]),
                    (triangle[2], triangle[0]),
                ] {
                    let key = ordered_edge(a, b);
                    boundary
                        .entry(key)
                        .and_modify(|edge| edge.2 += 1)
                        .or_insert((a, b, 1));
                }
            }

            let mut bad = bad.into_iter().peekable();
            let mut triangle_index = 0;
            triangles.retain(|_| {
                let keep = bad.peek().copied() != Some(triangle_index);
                if !keep {
                    bad.next();
                }
                triangle_index += 1;
                keep
            });

            for (_, (a, b, count)) in boundary {
                if count != 1 {
                    continue;
                }
                let triangle = orient_triangle([a, b, point_index], &working_points);
                if triangle_area_2d(
                    working_points[triangle[0]],
                    working_points[triangle[1]],
                    working_points[triangle[2]],
                ) > f32::EPSILON
                {
                    triangles.push(triangle);
                }
            }
        }

        triangles.retain(|triangle| triangle.iter().all(|index| *index < point_count));
        let mut mesh = Self {
            vertices: points
                .iter()
                .map(|point| Vec3::new(point.x, point.y, 0.0))
                .collect(),
            normals: vec![Vec3::new(0.0, 0.0, 1.0); point_count],
            triangles: Vec::with_capacity(triangles.len() * 3),
            uv: points.to_vec(),
        };
        for triangle in triangles {
            mesh.triangles.extend(triangle.map(|index| index as u32));
        }
        mesh
    }

    /// Splits every irregular triangle into four triangles, sharing edge
    /// midpoints between neighbouring faces.
    #[must_use]
    pub fn tessellated(&self) -> Self {
        self.tessellated_attributed().mesh
    }

    pub(crate) fn tessellated_attributed(&self) -> TessellationResult {
        self.tessellated_faces_attributed(|_| true)
    }

    /// Adds detail to land triangles while conformingly stitching adjacent
    /// unsplit sea triangles to any new shared-edge midpoint.
    #[must_use]
    pub fn tessellated_above(&self, threshold: f32) -> Self {
        self.tessellated_where(|vertices| vertices.iter().any(|vertex| vertex.z > threshold))
    }

    /// Adds a second detail tier to coasts and high-relief land while leaving
    /// already-flat regions at their current density.
    #[must_use]
    pub fn tessellated_detail(&self, relief_threshold: f32) -> Self {
        self.tessellated_where(|vertices| {
            let minimum = vertices
                .iter()
                .map(|vertex| vertex.z)
                .fold(f32::MAX, f32::min);
            let maximum = vertices
                .iter()
                .map(|vertex| vertex.z)
                .fold(f32::MIN, f32::max);
            maximum > 0.0 && (minimum <= 0.0 || maximum - minimum > relief_threshold)
        })
    }

    /// Refines only faces incident to vertices whose local normal displacement
    /// is large relative to their mean connected-edge length. Flat regions are
    /// left untouched, including flat regions on an inclined plane.
    #[must_use]
    pub fn tessellated_displaced(&self, displacement_ratio: f32) -> Self {
        self.tessellated_displaced_attributed(displacement_ratio)
            .mesh
    }

    pub(crate) fn tessellated_displaced_attributed(
        &self,
        displacement_ratio: f32,
    ) -> TessellationResult {
        let adjacency = self.adjacency();
        let displaced: Vec<bool> = self
            .vertices
            .iter()
            .enumerate()
            .map(|(index, _)| {
                self.normal_displacement_ratio(&adjacency, index)
                    .is_some_and(|ratio| ratio.abs() > displacement_ratio)
            })
            .collect();
        self.tessellated_faces_attributed(|triangle| {
            triangle.iter().any(|&vertex| displaced[vertex as usize])
        })
    }

    /// Returns signed displacement along the vertex normal relative to the
    /// average position of its neighbours, normalized by mean edge length.
    /// Positive values protrude out of the local surface.
    pub(crate) fn normal_displacement_ratio(
        &self,
        adjacency: &Adjacency,
        vertex: usize,
    ) -> Option<f32> {
        let neighbours = &adjacency[vertex];
        if neighbours.is_empty() {
            return None;
        }
        let position = self.vertices[vertex];
        let (position_total, edge_length_total) = neighbours.iter().fold(
            (Vec3::ZERO, 0.0_f32),
            |(position_total, edge_length_total), &neighbour| {
                let neighbour = self.vertices[neighbour];
                (
                    position_total + neighbour,
                    edge_length_total + neighbour.distance(position),
                )
            },
        );
        let inverse_count = 1.0 / neighbours.len() as f32;
        let mean_edge_length = edge_length_total * inverse_count;
        if mean_edge_length <= f32::EPSILON {
            return None;
        }
        let neighbour_average = position_total * inverse_count;
        let normal = self.normals.get(vertex).copied().unwrap_or(Vec3::Z);
        Some((position - neighbour_average).dot(normal) / mean_edge_length)
    }

    fn tessellated_where(&self, should_split: impl Fn([Vec3; 3]) -> bool) -> Self {
        self.tessellated_faces(|triangle| {
            should_split([
                self.vertices[triangle[0] as usize],
                self.vertices[triangle[1] as usize],
                self.vertices[triangle[2] as usize],
            ])
        })
    }

    fn tessellated_faces(&self, should_split: impl Fn([u32; 3]) -> bool) -> Self {
        self.tessellated_faces_attributed(should_split).mesh
    }

    fn tessellated_faces_attributed(
        &self,
        should_split: impl Fn([u32; 3]) -> bool,
    ) -> TessellationResult {
        let split: Vec<bool> = self
            .triangles
            .chunks_exact(3)
            .map(|triangle| should_split([triangle[0], triangle[1], triangle[2]]))
            .collect();
        let selected_faces = split.iter().filter(|&&selected| selected).count();
        let mut out = Self {
            vertices: self.vertices.clone(),
            normals: Vec::new(),
            triangles: Vec::with_capacity(selected_faces.saturating_mul(12)),
            uv: self.uv.clone(),
        };
        let mut midpoints =
            HashMap::<u64, (u32, usize)>::with_capacity(selected_faces.saturating_mul(3));
        let mut new_vertices = Vec::with_capacity(selected_faces.saturating_mul(3));
        for (face, triangle) in self.triangles.chunks_exact(3).enumerate() {
            let a = triangle[0];
            let b = triangle[1];
            let c = triangle[2];
            if !split[face] {
                continue;
            }
            let ab = midpoint(a, b, c, self, &mut out, &mut midpoints, &mut new_vertices);
            let bc = midpoint(b, c, a, self, &mut out, &mut midpoints, &mut new_vertices);
            let ca = midpoint(c, a, b, self, &mut out, &mut midpoints, &mut new_vertices);
            out.triangles
                .extend([a, ab, ca, ab, b, bc, ca, bc, c, ab, bc, ca]);
        }
        let conforming_indices = self
            .triangles
            .chunks_exact(3)
            .enumerate()
            .filter(|(face, _)| !split[*face])
            .map(|(_, triangle)| {
                let midpoint_count = [
                    packed_edge(triangle[0], triangle[1]),
                    packed_edge(triangle[0], triangle[2]),
                    packed_edge(triangle[2], triangle[1]),
                ]
                .iter()
                .filter(|edge| midpoints.contains_key(edge))
                .count();
                [3, 6, 9, 12][midpoint_count]
            })
            .sum::<usize>();
        out.triangles.reserve(conforming_indices);
        for (face, triangle) in self.triangles.chunks_exact(3).enumerate() {
            let a = triangle[0];
            let b = triangle[1];
            let c = triangle[2];
            if split[face] {
                continue;
            }
            add_conforming_triangle(
                &mut out.triangles,
                a,
                b,
                c,
                conforming_midpoint(midpoints.get(&packed_edge(a, b)), c, &mut new_vertices),
                conforming_midpoint(midpoints.get(&packed_edge(a, c)), b, &mut new_vertices),
                conforming_midpoint(midpoints.get(&packed_edge(c, b)), a, &mut new_vertices),
            );
        }
        out.calculate_normals();
        TessellationResult {
            mesh: out,
            new_vertices,
        }
    }

    pub(crate) fn tessellate_incident_to(
        &mut self,
        marked_vertices: &[bool],
    ) -> Vec<NewVertexStencil> {
        let source_triangles = std::mem::take(&mut self.triangles);
        let marked_count = marked_vertices.iter().filter(|&&marked| marked).count();
        let midpoint_capacity = marked_count.saturating_mul(6);
        let mut triangles = Vec::with_capacity(
            source_triangles
                .len()
                .saturating_add(marked_count.saturating_mul(54)),
        );
        let mut midpoints = HashMap::<(u32, u32), (u32, usize)>::with_capacity(midpoint_capacity);
        let mut added = Vec::with_capacity(midpoint_capacity);

        for triangle in source_triangles.chunks_exact(3) {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
            if ![a, b, c]
                .iter()
                .any(|&vertex| marked_vertices[vertex as usize])
            {
                continue;
            }
            let ab = midpoint_in_place(a, b, c, self, &mut midpoints, &mut added);
            let bc = midpoint_in_place(b, c, a, self, &mut midpoints, &mut added);
            let ca = midpoint_in_place(c, a, b, self, &mut midpoints, &mut added);
            triangles.extend([a, ab, ca, ab, b, bc, ca, bc, c, ab, bc, ca]);
        }
        for triangle in source_triangles.chunks_exact(3) {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
            if [a, b, c]
                .iter()
                .any(|&vertex| marked_vertices[vertex as usize])
            {
                continue;
            }
            add_conforming_triangle(
                &mut triangles,
                a,
                b,
                c,
                conforming_midpoint(midpoints.get(&ordered_edge(a, b)), c, &mut added),
                conforming_midpoint(midpoints.get(&ordered_edge(a, c)), b, &mut added),
                conforming_midpoint(midpoints.get(&ordered_edge(c, b)), a, &mut added),
            );
        }
        self.triangles = triangles;
        self.normals.clear();
        added
    }

    /// Applies the original perimeter-preserving Laplacian smoothing rule to
    /// XY position and elevation together.
    pub fn smooth(&mut self) {
        let adjacency = self.adjacency();
        self.smooth_with(&adjacency);
    }

    /// Smooths only land, preserving underwater vertices and the perimeter.
    pub fn smooth_land(&mut self) {
        let adjacency = self.adjacency();
        self.smooth_land_with(&adjacency);
    }

    /// Smooths only the seabed, preserving land and the perimeter.
    pub fn smooth_seabed(&mut self) {
        let adjacency = self.adjacency();
        self.smooth_seabed_with(&adjacency);
    }

    pub(crate) fn smooth_with(&mut self, adjacency: &Adjacency) {
        self.smooth_excluding(adjacency, |_| false);
    }

    pub(crate) fn smooth_land_with(&mut self, adjacency: &Adjacency) {
        self.smooth_excluding(adjacency, |vertex| vertex.z < 0.0);
    }

    pub(crate) fn smooth_seabed_with(&mut self, adjacency: &Adjacency) {
        self.smooth_excluding(adjacency, |vertex| vertex.z >= 0.0);
    }

    fn smooth_excluding(&mut self, adjacency: &Adjacency, exclude: impl Fn(Vec3) -> bool + Sync) {
        let perimeter = self.perimeter_mask();
        let moved: Vec<Vec3> = self
            .vertices
            .par_iter()
            .enumerate()
            .map(|(index, &vertex)| {
                if perimeter[index] || exclude(vertex) {
                    return vertex;
                }
                let (total, count) = adjacency[index]
                    .iter()
                    .filter_map(|&neighbour| {
                        let candidate = self.vertices[neighbour];
                        (!exclude(candidate)).then_some(candidate)
                    })
                    .fold((vertex, 1_u32), |(total, count), candidate| {
                        (total + candidate, count + 1)
                    });
                total / count as f32
            })
            .collect();
        self.vertices = moved;
        if !self.uv.is_empty() {
            self.uv = self
                .vertices
                .par_iter()
                .map(|vertex| vertex.truncate())
                .collect();
        }
        self.calculate_normals();
    }

    #[must_use]
    pub fn adjacency(&self) -> Adjacency {
        let vertex_count = self.vertices.len();
        let mut counts = vec![0_usize; vertex_count];
        for triangle in self.triangles.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let (a, b) = (a as usize, b as usize);
                counts[a] += 1;
                counts[b] += 1;
            }
        }
        let mut offsets = Vec::with_capacity(vertex_count + 1);
        offsets.push(0);
        for count in counts {
            offsets.push(offsets.last().copied().unwrap_or_default() + count);
        }
        let mut cursor = offsets[..vertex_count].to_vec();
        let mut neighbours = vec![0_usize; *offsets.last().unwrap_or(&0)];
        for triangle in self.triangles.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let (a, b) = (a as usize, b as usize);
                neighbours[cursor[a]] = b;
                cursor[a] += 1;
                neighbours[cursor[b]] = a;
                cursor[b] += 1;
            }
        }
        for range in offsets.windows(2) {
            neighbours[range[0]..range[1]].sort_unstable();
        }
        let mut compact_offsets = Vec::with_capacity(vertex_count + 1);
        let mut write = 0;
        for range in offsets.windows(2) {
            compact_offsets.push(write);
            let mut previous = None;
            for read in range[0]..range[1] {
                let neighbour = neighbours[read];
                if previous == Some(neighbour) {
                    continue;
                }
                neighbours[write] = neighbour;
                write += 1;
                previous = Some(neighbour);
            }
        }
        compact_offsets.push(write);
        neighbours.truncate(write);
        Adjacency {
            offsets: compact_offsets,
            neighbours,
        }
    }

    #[must_use]
    pub fn perimeter_vertices(&self) -> BTreeSet<usize> {
        self.perimeter_mask()
            .into_iter()
            .enumerate()
            .filter_map(|(vertex, perimeter)| perimeter.then_some(vertex))
            .collect()
    }

    pub(crate) fn perimeter_mask(&self) -> Vec<bool> {
        let mut edges = Vec::with_capacity(self.triangles.len());
        for triangle in self.triangles.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let (a, b) = ordered_edge(a, b);
                edges.push((u64::from(a) << 32) | u64::from(b));
            }
        }
        edges.sort_unstable();
        let mut perimeter = vec![false; self.vertices.len()];
        let mut start = 0;
        while start < edges.len() {
            let edge = edges[start];
            let mut end = start + 1;
            while end < edges.len() && edges[end] == edge {
                end += 1;
            }
            if end - start == 1 {
                perimeter[(edge >> 32) as usize] = true;
                perimeter[(edge & u64::from(u32::MAX)) as usize] = true;
            }
            start = end;
        }
        perimeter
    }

    pub fn calculate_normals(&mut self) {
        self.normals.clear();
        self.normals.resize(self.vertices.len(), Vec3::default());
        for triangle in self.triangles.chunks_exact(3) {
            let (a, b, c) = (
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            );
            let normal =
                (self.vertices[b] - self.vertices[a]).cross(self.vertices[c] - self.vertices[a]);
            self.normals[a] += normal;
            self.normals[b] += normal;
            self.normals[c] += normal;
        }
        self.normals.iter_mut().for_each(|normal| {
            *normal = normal.try_normalize().unwrap_or(Vec3::Z);
        });
    }

    #[must_use]
    pub fn surface_area_xy(&self) -> f32 {
        self.triangles
            .chunks_exact(3)
            .map(|triangle| {
                triangle_area_2d(
                    self.vertices[triangle[0] as usize].truncate(),
                    self.vertices[triangle[1] as usize].truncate(),
                    self.vertices[triangle[2] as usize].truncate(),
                ) * 0.5
            })
            .sum()
    }

    /// Consumes the mesh and geometrically clips away everything below a
    /// horizontal plane. Surviving vertices are compacted and shared edge
    /// intersections are inserted once, retaining normals and UVs.
    #[must_use]
    pub(crate) fn clipped_above(self, minimum_z: f32) -> Self {
        if self.vertices.iter().all(|vertex| vertex.z >= minimum_z) {
            return self;
        }

        let has_normals = self.normals.len() == self.vertices.len();
        let has_uv = self.uv.len() == self.vertices.len();
        let mut output = Self {
            vertices: Vec::with_capacity(self.vertices.len()),
            normals: if has_normals {
                Vec::with_capacity(self.normals.len())
            } else {
                Vec::new()
            },
            triangles: Vec::with_capacity(self.triangles.len()),
            uv: if has_uv {
                Vec::with_capacity(self.uv.len())
            } else {
                Vec::new()
            },
        };
        let mut vertex_remap = vec![u32::MAX; self.vertices.len()];
        let mut edge_remap = HashMap::<(u32, u32), u32>::new();

        for triangle in self.triangles.chunks_exact(3) {
            let (polygon, length) = clip_triangle_above(
                [triangle[0], triangle[1], triangle[2]],
                &self.vertices,
                minimum_z,
            );
            if length < 3 {
                continue;
            }
            let mut mapped = [0_u32; 4];
            for (destination, vertex) in mapped[..length].iter_mut().zip(polygon) {
                *destination = map_height_clip_vertex(
                    vertex,
                    minimum_z,
                    &self,
                    has_normals,
                    has_uv,
                    &mut vertex_remap,
                    &mut edge_remap,
                    &mut output,
                );
            }
            for index in 1..length - 1 {
                let triangle = [mapped[0], mapped[index], mapped[index + 1]];
                if triangle[0] != triangle[1]
                    && triangle[1] != triangle[2]
                    && triangle[2] != triangle[0]
                {
                    output.triangles.extend(triangle);
                }
            }
        }
        output
    }

    #[must_use]
    pub fn sliced(&self, bounds: BoundingBox) -> Self {
        if bounds == BoundingBox::default() {
            return self.clone();
        }
        self.sliced_grid(bounds, 1).pop().unwrap_or_default()
    }

    /// Geometrically clips a rectangular region into a fixed grid in one
    /// source-triangle pass. Sibling tile edges therefore contain identical
    /// boundary points rather than overlapping whole triangles.
    #[must_use]
    pub fn sliced_grid(&self, bounds: BoundingBox, divisions: usize) -> Vec<Self> {
        self.sliced_grid_unclamped(bounds, divisions)
    }

    /// Clips a fixed grid and projects selected outer edges onto a coarser LOD.
    /// Fine edge vertices remain in place in XY while their height and normal
    /// are interpolated along the coarser mesh's clipped boundary profile.
    #[must_use]
    pub fn sliced_grid_clamped(
        &self,
        bounds: BoundingBox,
        divisions: usize,
        coarser: &Self,
        clamp_sides: u8,
    ) -> Vec<Self> {
        let mut output = self.sliced_grid_unclamped(bounds, divisions);
        if clamp_sides == 0 {
            return output;
        }
        let Some(coarse_patch) = coarser.sliced_grid_unclamped(bounds, 1).pop() else {
            return output;
        };
        clamp_grid_boundaries(&mut output, &coarse_patch, bounds, clamp_sides);
        output
    }

    fn sliced_grid_unclamped(&self, bounds: BoundingBox, divisions: usize) -> Vec<Self> {
        if divisions == 0 {
            return Vec::new();
        }

        let width = bounds.max.x - bounds.min.x;
        let height = bounds.max.y - bounds.min.y;
        if width <= 0.0 || height <= 0.0 {
            return vec![Self::default(); divisions * divisions];
        }

        let tile_count = divisions * divisions;
        let mut output = vec![Self::default(); tile_count];
        let mut remaps: Vec<HashMap<VertexKey, u32>> =
            (0..tile_count).map(|_| HashMap::new()).collect();
        let coordinate = |value: f32, minimum: f32, span: f32| {
            (((value - minimum) / span * divisions as f32).floor() as usize).min(divisions - 1)
        };

        for triangle in self.triangles.chunks_exact(3) {
            let indices = [triangle[0], triangle[1], triangle[2]];
            let vertices = [
                self.vertices[indices[0] as usize],
                self.vertices[indices[1] as usize],
                self.vertices[indices[2] as usize],
            ];
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
                        self,
                        indices,
                        tile_bounds,
                        &mut output[tile],
                        &mut remaps[tile],
                    );
                }
            }
        }
        canonicalize_grid_corners(&mut output, bounds, divisions);
        output
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct VertexKey([u32; 3]);

impl From<Vec3> for VertexKey {
    fn from(value: Vec3) -> Self {
        Self([value.x.to_bits(), value.y.to_bits(), value.z.to_bits()])
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum HeightClipVertex {
    Existing(u32),
    Edge {
        from: u32,
        to: u32,
        interpolation: f32,
    },
}

fn clip_triangle_above(
    triangle: [u32; 3],
    vertices: &[Vec3],
    minimum_z: f32,
) -> ([HeightClipVertex; 4], usize) {
    let mut output = [HeightClipVertex::Existing(0); 4];
    let mut length = 0;
    let mut previous = triangle[2];
    let mut previous_inside = vertices[previous as usize].z >= minimum_z;
    for current in triangle {
        let current_inside = vertices[current as usize].z >= minimum_z;
        if current_inside != previous_inside {
            let previous_z = vertices[previous as usize].z;
            let current_z = vertices[current as usize].z;
            let interpolation = (minimum_z - previous_z) / (current_z - previous_z);
            let intersection = if interpolation <= f32::EPSILON {
                HeightClipVertex::Existing(previous)
            } else if interpolation >= 1.0 - f32::EPSILON {
                HeightClipVertex::Existing(current)
            } else {
                HeightClipVertex::Edge {
                    from: previous,
                    to: current,
                    interpolation,
                }
            };
            push_height_clip_vertex(&mut output, &mut length, intersection);
        }
        if current_inside {
            push_height_clip_vertex(
                &mut output,
                &mut length,
                HeightClipVertex::Existing(current),
            );
        }
        previous = current;
        previous_inside = current_inside;
    }
    if length > 1 && output[0] == output[length - 1] {
        length -= 1;
    }
    (output, length)
}

fn push_height_clip_vertex(
    output: &mut [HeightClipVertex; 4],
    length: &mut usize,
    vertex: HeightClipVertex,
) {
    if *length == 0 || output[*length - 1] != vertex {
        output[*length] = vertex;
        *length += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn map_height_clip_vertex(
    vertex: HeightClipVertex,
    minimum_z: f32,
    source: &Mesh,
    has_normals: bool,
    has_uv: bool,
    vertex_remap: &mut [u32],
    edge_remap: &mut HashMap<(u32, u32), u32>,
    output: &mut Mesh,
) -> u32 {
    match vertex {
        HeightClipVertex::Existing(source_vertex) => {
            let mapped = &mut vertex_remap[source_vertex as usize];
            if *mapped == u32::MAX {
                *mapped = output.vertices.len() as u32;
                output
                    .vertices
                    .push(source.vertices[source_vertex as usize]);
                if has_normals {
                    output.normals.push(source.normals[source_vertex as usize]);
                }
                if has_uv {
                    output.uv.push(source.uv[source_vertex as usize]);
                }
            }
            *mapped
        }
        HeightClipVertex::Edge {
            from,
            to,
            interpolation,
        } => {
            let key = ordered_edge(from, to);
            *edge_remap.entry(key).or_insert_with(|| {
                let mapped = output.vertices.len() as u32;
                let mut position = source.vertices[from as usize]
                    .lerp(source.vertices[to as usize], interpolation);
                position.z = minimum_z;
                output.vertices.push(position);
                if has_normals {
                    output.normals.push(
                        source.normals[from as usize]
                            .lerp(source.normals[to as usize], interpolation)
                            .normalize_or_zero(),
                    );
                }
                if has_uv {
                    output
                        .uv
                        .push(source.uv[from as usize].lerp(source.uv[to as usize], interpolation));
                }
                mapped
            })
        }
    }
}

fn append_clipped_triangle(
    source: &Mesh,
    indices: [u32; 3],
    bounds: BoundingBox,
    output: &mut Mesh,
    remap: &mut HashMap<VertexKey, u32>,
) {
    let positions = indices.map(|index| source.vertices[index as usize]);
    let normals = indices.map(|index| source.normals[index as usize]);
    let uv = match indices.map(|index| source.uv.get(index as usize).copied()) {
        [Some(a), Some(b), Some(c)] => Some([a, b, c]),
        _ => None,
    };
    let mut first = [Vec2::ZERO; 8];
    first[..3].copy_from_slice(&positions.map(Vec3::truncate));
    let mut second = [Vec2::ZERO; 8];
    let mut length = clip_polygon(&first[..3], &mut second, 0, bounds.min.x, true);
    length = clip_polygon(&second[..length], &mut first, 0, bounds.max.x, false);
    length = clip_polygon(&first[..length], &mut second, 1, bounds.min.y, true);
    length = clip_polygon(&second[..length], &mut first, 1, bounds.max.y, false);
    if length < 3 {
        return;
    }

    let mut mapped = [0_u32; 8];
    for (index, &point) in first[..length].iter().enumerate() {
        let (position, normal, vertex_uv) = interpolate_triangle(point, positions, normals, uv);
        let key = VertexKey::from(position);
        mapped[index] = *remap.entry(key).or_insert_with(|| {
            let mapped = output.vertices.len() as u32;
            output.vertices.push(position);
            output.normals.push(normal);
            if let Some(vertex_uv) = vertex_uv {
                output.uv.push(vertex_uv);
            }
            mapped
        });
    }
    for index in 1..length - 1 {
        let triangle = [mapped[0], mapped[index], mapped[index + 1]];
        if triangle[0] != triangle[1] && triangle[1] != triangle[2] && triangle[2] != triangle[0] {
            output.triangles.extend(triangle);
        }
    }
}

fn clip_polygon(
    input: &[Vec2],
    output: &mut [Vec2; 8],
    axis: usize,
    boundary: f32,
    keep_greater: bool,
) -> usize {
    if input.is_empty() {
        return 0;
    }
    let component = |point: Vec2| if axis == 0 { point.x } else { point.y };
    let inside = |point: Vec2| {
        if keep_greater {
            component(point) >= boundary
        } else {
            component(point) <= boundary
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
                let mut intersection =
                    previous.lerp(current, (boundary - component(previous)) / denominator);
                if axis == 0 {
                    intersection.x = boundary;
                } else {
                    intersection.y = boundary;
                }
                push_clip_point(output, &mut length, intersection);
            }
        }
        if current_inside {
            push_clip_point(output, &mut length, current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    if length > 1 && output[0].distance_squared(output[length - 1]) <= f32::EPSILON.powi(2) {
        length -= 1;
    }
    length
}

fn push_clip_point(output: &mut [Vec2; 8], length: &mut usize, point: Vec2) {
    if *length == 0 || output[*length - 1].distance_squared(point) > f32::EPSILON.powi(2) {
        output[*length] = point;
        *length += 1;
    }
}

fn interpolate_triangle(
    point: Vec2,
    positions: [Vec3; 3],
    normals: [Vec3; 3],
    uv: Option<[Vec2; 3]>,
) -> (Vec3, Vec3, Option<Vec2>) {
    for index in 0..3 {
        if point == positions[index].truncate() {
            return (
                positions[index],
                normals[index],
                uv.map(|values| values[index]),
            );
        }
    }
    let a = positions[0].truncate();
    let b = positions[1].truncate();
    let c = positions[2].truncate();
    let denominator = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
    let first = ((b.y - c.y) * (point.x - c.x) + (c.x - b.x) * (point.y - c.y)) / denominator;
    let second = ((c.y - a.y) * (point.x - c.x) + (a.x - c.x) * (point.y - c.y)) / denominator;
    let third = 1.0 - first - second;
    let position = Vec3::new(
        point.x,
        point.y,
        positions[0].z * first + positions[1].z * second + positions[2].z * third,
    );
    let normal =
        (normals[0] * first + normals[1] * second + normals[2] * third).normalize_or_zero();
    let vertex_uv = uv.map(|values| values[0] * first + values[1] * second + values[2] * third);
    (position, normal, vertex_uv)
}

#[derive(Clone, Copy, Debug)]
struct BoundarySample {
    coordinate: f32,
    height: f32,
    normal: Vec3,
}

#[derive(Clone, Copy, Debug)]
struct GridCornerSample {
    height: f32,
    normal: Vec3,
    uv: Vec2,
}

fn canonicalize_grid_corners(meshes: &mut [Mesh], bounds: BoundingBox, divisions: usize) {
    let width = bounds.max.x - bounds.min.x;
    let height = bounds.max.y - bounds.min.y;
    let coordinate_scale = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
        .into_iter()
        .map(f32::abs)
        .fold(1.0_f32, f32::max);
    let epsilon = coordinate_scale * f32::EPSILON * 4.0;
    for grid_y in 0..=divisions {
        let y = bounds.min.y + height * grid_y as f32 / divisions as f32;
        let first_tile_y = grid_y.saturating_sub(1).min(divisions - 1);
        let last_tile_y = grid_y.min(divisions - 1);
        for grid_x in 0..=divisions {
            let x = bounds.min.x + width * grid_x as f32 / divisions as f32;
            let first_tile_x = grid_x.saturating_sub(1).min(divisions - 1);
            let last_tile_x = grid_x.min(divisions - 1);
            let mut canonical = None;
            for tile_y in first_tile_y..=last_tile_y {
                for tile_x in first_tile_x..=last_tile_x {
                    let mesh = &meshes[tile_y * divisions + tile_x];
                    for (index, vertex) in mesh.vertices.iter().enumerate() {
                        if (vertex.x - x).abs() <= epsilon && (vertex.y - y).abs() <= epsilon {
                            let candidate = GridCornerSample {
                                height: vertex.z,
                                normal: mesh.normals.get(index).copied().unwrap_or(Vec3::Z),
                                uv: mesh.uv.get(index).copied().unwrap_or(Vec2::new(x, y)),
                            };
                            if canonical.is_none_or(|current| {
                                corner_sample_order(candidate, current).is_gt()
                            }) {
                                canonical = Some(candidate);
                            }
                        }
                    }
                }
            }
            let Some(canonical) = canonical else {
                continue;
            };
            for tile_y in first_tile_y..=last_tile_y {
                for tile_x in first_tile_x..=last_tile_x {
                    let mesh = &mut meshes[tile_y * divisions + tile_x];
                    for (index, vertex) in mesh.vertices.iter_mut().enumerate() {
                        if (vertex.x - x).abs() <= epsilon && (vertex.y - y).abs() <= epsilon {
                            vertex.x = x;
                            vertex.y = y;
                            vertex.z = canonical.height;
                            if let Some(normal) = mesh.normals.get_mut(index) {
                                *normal = canonical.normal;
                            }
                            if let Some(uv) = mesh.uv.get_mut(index) {
                                *uv = canonical.uv;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn corner_sample_order(a: GridCornerSample, b: GridCornerSample) -> std::cmp::Ordering {
    a.height
        .total_cmp(&b.height)
        .then_with(|| a.normal.x.total_cmp(&b.normal.x))
        .then_with(|| a.normal.y.total_cmp(&b.normal.y))
        .then_with(|| a.normal.z.total_cmp(&b.normal.z))
}

fn clamp_grid_boundaries(
    meshes: &mut [Mesh],
    coarse_patch: &Mesh,
    bounds: BoundingBox,
    clamp_sides: u8,
) {
    let profiles = [CLAMP_TOP, CLAMP_LEFT, CLAMP_BOTTOM, CLAMP_RIGHT].map(|side| {
        (clamp_sides & side != 0).then(|| boundary_profile(coarse_patch, bounds, side))
    });
    let epsilon =
        ((bounds.max.x - bounds.min.x).abs() + (bounds.max.y - bounds.min.y).abs()) * 1.0e-6;
    for mesh in meshes {
        for index in 0..mesh.vertices.len() {
            for (profile_index, profile) in profiles.iter().enumerate() {
                let Some(profile) = profile.as_deref() else {
                    continue;
                };
                let side = [CLAMP_TOP, CLAMP_LEFT, CLAMP_BOTTOM, CLAMP_RIGHT][profile_index];
                if !vertex_is_on_side(mesh.vertices[index], bounds, side, epsilon) {
                    continue;
                }
                let coordinate = boundary_coordinate(mesh.vertices[index], side);
                if let Some(sample) = sample_boundary(profile, coordinate) {
                    mesh.vertices[index].z = sample.height;
                    mesh.normals[index] = sample.normal;
                }
            }
        }
    }
}

fn boundary_profile(mesh: &Mesh, bounds: BoundingBox, side: u8) -> Vec<BoundarySample> {
    let epsilon =
        ((bounds.max.x - bounds.min.x).abs() + (bounds.max.y - bounds.min.y).abs()) * 1.0e-6;
    let mut samples: Vec<BoundarySample> = mesh
        .vertices
        .iter()
        .enumerate()
        .filter(|(_, vertex)| vertex_is_on_side(**vertex, bounds, side, epsilon))
        .map(|(index, vertex)| BoundarySample {
            coordinate: boundary_coordinate(*vertex, side),
            height: vertex.z,
            normal: mesh.normals.get(index).copied().unwrap_or(Vec3::Z),
        })
        .collect();
    samples.sort_unstable_by(|a, b| a.coordinate.total_cmp(&b.coordinate));
    samples.dedup_by(|a, b| (a.coordinate - b.coordinate).abs() <= epsilon);
    samples
}

fn vertex_is_on_side(vertex: Vec3, bounds: BoundingBox, side: u8, epsilon: f32) -> bool {
    let difference = match side {
        CLAMP_TOP => vertex.y - bounds.max.y,
        CLAMP_LEFT => vertex.x - bounds.min.x,
        CLAMP_BOTTOM => vertex.y - bounds.min.y,
        CLAMP_RIGHT => vertex.x - bounds.max.x,
        _ => return false,
    };
    difference.abs() <= epsilon
}

fn boundary_coordinate(vertex: Vec3, side: u8) -> f32 {
    if side == CLAMP_TOP || side == CLAMP_BOTTOM {
        vertex.x
    } else {
        vertex.y
    }
}

fn sample_boundary(profile: &[BoundarySample], coordinate: f32) -> Option<BoundarySample> {
    let first = *profile.first()?;
    let upper = profile.partition_point(|sample| sample.coordinate < coordinate);
    if upper == 0 {
        return Some(first);
    }
    if upper == profile.len() {
        return profile.last().copied();
    }
    let lower = profile[upper - 1];
    let upper = profile[upper];
    let span = upper.coordinate - lower.coordinate;
    if span.abs() <= f32::EPSILON {
        return Some(lower);
    }
    let interpolation = (coordinate - lower.coordinate) / span;
    Some(BoundarySample {
        coordinate,
        height: lower.height + (upper.height - lower.height) * interpolation,
        normal: lower
            .normal
            .lerp(upper.normal, interpolation)
            .normalize_or_zero(),
    })
}

fn add_stencil_opposite(stencil: &mut NewVertexStencil, opposite: u32) {
    let count = usize::from(stencil.count);
    if count < stencil.surrounding.len() && !stencil.surrounding[..count].contains(&opposite) {
        stencil.surrounding[count] = opposite;
        stencil.count += 1;
    }
}

fn midpoint(
    a: u32,
    b: u32,
    opposite: u32,
    source: &Mesh,
    output: &mut Mesh,
    midpoints: &mut HashMap<u64, (u32, usize)>,
    new_vertices: &mut Vec<NewVertexStencil>,
) -> u32 {
    let key = packed_edge(a, b);
    if let Some(&(index, stencil)) = midpoints.get(&key) {
        add_stencil_opposite(&mut new_vertices[stencil], opposite);
        return index;
    }
    let index = output.vertices.len() as u32;
    output
        .vertices
        .push((source.vertices[a as usize] + source.vertices[b as usize]) * 0.5);
    if !source.uv.is_empty() {
        output
            .uv
            .push((source.uv[a as usize] + source.uv[b as usize]) * 0.5);
    }
    let stencil = new_vertices.len();
    new_vertices.push(NewVertexStencil {
        vertex: index,
        surrounding: [a.min(b), a.max(b), opposite, 0],
        count: 3,
    });
    midpoints.insert(key, (index, stencil));
    index
}

fn midpoint_in_place(
    a: u32,
    b: u32,
    opposite: u32,
    mesh: &mut Mesh,
    midpoints: &mut HashMap<(u32, u32), (u32, usize)>,
    added: &mut Vec<NewVertexStencil>,
) -> u32 {
    let key = ordered_edge(a, b);
    if let Some(&(index, stencil)) = midpoints.get(&key) {
        add_stencil_opposite(&mut added[stencil], opposite);
        return index;
    }
    let index = mesh.vertices.len() as u32;
    let midpoint = (mesh.vertices[a as usize] + mesh.vertices[b as usize]) * 0.5;
    let uv = (!mesh.uv.is_empty()).then(|| (mesh.uv[a as usize] + mesh.uv[b as usize]) * 0.5);
    mesh.vertices.push(midpoint);
    if let Some(uv) = uv {
        mesh.uv.push(uv);
    }
    let stencil = added.len();
    added.push(NewVertexStencil {
        vertex: index,
        surrounding: [key.0, key.1, opposite, 0],
        count: 3,
    });
    midpoints.insert(key, (index, stencil));
    index
}

fn conforming_midpoint(
    midpoint: Option<&(u32, usize)>,
    opposite: u32,
    stencils: &mut [NewVertexStencil],
) -> Option<u32> {
    let &(vertex, stencil) = midpoint?;
    add_stencil_opposite(&mut stencils[stencil], opposite);
    Some(vertex)
}

#[allow(clippy::too_many_arguments)]
fn add_conforming_triangle(
    output: &mut Vec<u32>,
    a: u32,
    b: u32,
    c: u32,
    ab: Option<u32>,
    ac: Option<u32>,
    cb: Option<u32>,
) {
    let mut add = |a, b, c| output.extend([a, b, c]);
    match (ab, ac, cb) {
        (Some(ab), Some(ac), Some(cb)) => {
            add(a, ab, ac);
            add(b, cb, ab);
            add(c, ac, cb);
            add(ab, cb, ac);
        }
        (Some(ab), Some(ac), None) => {
            add(ab, ac, a);
            add(c, ac, b);
            add(b, ac, ab);
        }
        (Some(ab), None, Some(cb)) => {
            add(a, ab, c);
            add(b, cb, ab);
            add(c, ab, cb);
        }
        (Some(ab), None, None) => {
            add(a, ab, c);
            add(b, c, ab);
        }
        (None, Some(ac), Some(cb)) => {
            add(a, cb, ac);
            add(ac, cb, c);
            add(b, cb, a);
        }
        (None, Some(ac), None) => {
            add(a, b, ac);
            add(b, c, ac);
        }
        (None, None, Some(cb)) => {
            add(a, cb, c);
            add(a, b, cb);
        }
        (None, None, None) => add(a, b, c),
    }
}

fn ordered_edge<T: Ord + Copy>(a: T, b: T) -> (T, T) {
    if a < b { (a, b) } else { (b, a) }
}

fn packed_edge(a: u32, b: u32) -> u64 {
    let (a, b) = ordered_edge(a, b);
    (u64::from(a) << 32) | u64::from(b)
}

fn orient_triangle(mut triangle: [usize; 3], points: &[Vec2]) -> [usize; 3] {
    if triangle_area_2d(
        points[triangle[0]],
        points[triangle[1]],
        points[triangle[2]],
    ) < 0.0
    {
        triangle.swap(0, 1);
    }
    triangle
}

fn triangle_area_2d(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b.x - a.x).mul_add(c.y - a.y, -((b.y - a.y) * (c.x - a.x)))
}

fn circumcircle_contains(a: Vec2, b: Vec2, c: Vec2, point: Vec2) -> bool {
    let denominator = 2.0
        * (a.x
            .mul_add(b.y - c.y, b.x.mul_add(c.y - a.y, c.x * (a.y - b.y))));
    if denominator.abs() <= f32::EPSILON {
        return false;
    }
    let aa = a.length_squared();
    let bb = b.length_squared();
    let cc = c.length_squared();
    let centre = Vec2::new(
        aa.mul_add(b.y - c.y, bb.mul_add(c.y - a.y, cc * (a.y - b.y))) / denominator,
        aa.mul_add(c.x - b.x, bb.mul_add(a.x - c.x, cc * (b.x - a.x))) / denominator,
    );
    let radius_squared = (centre - a).length_squared();
    (centre - point).length_squared() <= radius_squared + 1.0e-6
}

#[cfg(test)]
mod tests {
    use super::{CLAMP_BOTTOM, CLAMP_LEFT, CLAMP_RIGHT, CLAMP_TOP, Mesh};
    use crate::{BoundingBox, Vec2, Vec3};

    #[test]
    fn delaunay_covers_square_with_irregular_faces() {
        let points = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(0.23, 0.34),
            Vec2::new(0.71, 0.28),
            Vec2::new(0.62, 0.79),
        ];
        let mesh = Mesh::delaunay(&points);
        assert!((mesh.surface_area_xy() - 1.0).abs() < 1.0e-5);
        assert_eq!(mesh.perimeter_vertices().len(), 4);
        assert!(mesh.triangles.len() >= 24);
    }

    #[test]
    fn tessellation_shares_edge_midpoints() {
        let mesh = Mesh::delaunay(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ]);
        let tessellated = mesh.tessellated();
        assert_eq!(tessellated.triangles.len(), mesh.triangles.len() * 4);
        assert_eq!(tessellated.vertices.len(), 9);
    }

    #[test]
    fn tessellation_stencils_reference_only_the_old_generation() {
        let mesh = Mesh::delaunay(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ]);
        let old_vertex_count = mesh.vertices.len();
        let result = mesh.tessellated_faces_attributed(|_| true);

        assert_eq!(result.new_vertices.len(), 5);
        assert!(result.new_vertices.iter().any(|stencil| stencil.count == 4));
        assert!(result.new_vertices.iter().any(|stencil| stencil.count == 3));
        for (offset, stencil) in result.new_vertices.iter().enumerate() {
            let count = usize::from(stencil.count);
            assert_eq!(stencil.vertex as usize, old_vertex_count + offset);
            assert!(
                stencil.surrounding[..count]
                    .iter()
                    .all(|&vertex| (vertex as usize) < old_vertex_count)
            );
            let mut unique = stencil.surrounding[..count].to_vec();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), count);
        }
    }

    #[test]
    fn grid_slicing_populates_every_touched_cell_in_one_partition() {
        let points: Vec<Vec2> = (0..=4)
            .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 * 0.25, y as f32 * 0.25)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.calculate_normals();

        let tiles = mesh.sliced_grid(BoundingBox::default(), 2);

        assert_eq!(tiles.len(), 4);
        assert!(tiles.iter().all(|tile| !tile.triangles.is_empty()));
        assert!(tiles.iter().all(|tile| {
            tile.triangles
                .iter()
                .all(|&index| (index as usize) < tile.vertices.len())
        }));
        assert!(
            tiles
                .iter()
                .flat_map(|tile| &tile.vertices)
                .any(|vertex| { *vertex == Vec3::new(0.5, 0.5, 0.0) })
        );
    }

    #[test]
    fn height_clipping_compacts_deep_vertices_and_splits_crossing_faces() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, -0.01),
                Vec3::new(1.0, 1.0, -0.01),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y],
        };

        let clipped = mesh.clipped_above(-0.0025);

        assert_eq!(clipped.vertices.len(), 5);
        assert_eq!(clipped.normals.len(), clipped.vertices.len());
        assert_eq!(clipped.uv.len(), clipped.vertices.len());
        assert_eq!(clipped.triangles.len(), 9);
        assert!(clipped.vertices.iter().all(|vertex| vertex.z >= -0.0025));
        assert!(
            clipped
                .vertices
                .iter()
                .any(|vertex| (vertex.z + 0.0025).abs() < f32::EPSILON)
        );
        assert!(
            clipped
                .triangles
                .iter()
                .all(|&vertex| (vertex as usize) < clipped.vertices.len())
        );
        let mut used = vec![false; clipped.vertices.len()];
        for &vertex in &clipped.triangles {
            used[vertex as usize] = true;
        }
        assert!(used.into_iter().all(std::convert::identity));
    }

    #[test]
    fn grid_slicing_produces_identical_sibling_boundaries() {
        let points: Vec<Vec2> = (0..=3)
            .flat_map(|y| (0..=3).map(move |x| Vec2::new(x as f32 / 3.0, y as f32 / 3.0)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        for vertex in &mut mesh.vertices {
            vertex.z = vertex.x + vertex.y;
        }
        mesh.calculate_normals();

        let tiles = mesh.sliced_grid(BoundingBox::default(), 2);
        let edge = |tile: &Mesh| {
            let mut points: Vec<(u32, u32)> = tile
                .vertices
                .iter()
                .filter(|vertex| (vertex.x - 0.5).abs() < 1.0e-6 && vertex.y <= 0.5)
                .map(|vertex| (vertex.y.to_bits(), vertex.z.to_bits()))
                .collect();
            points.sort_unstable();
            points.dedup();
            points
        };

        assert_eq!(edge(&tiles[0]), edge(&tiles[1]));
        assert!(tiles[0].vertices.iter().all(|vertex| vertex.x <= 0.5));
        assert!(tiles[1].vertices.iter().all(|vertex| vertex.x >= 0.5));
    }

    #[test]
    fn grid_slicing_preserves_non_positional_uv_at_grid_corners() {
        let points: Vec<Vec2> = (0..=3)
            .flat_map(|y| (0..=3).map(move |x| Vec2::new(x as f32 / 3.0, y as f32 / 3.0)))
            .collect();
        let mut mesh = Mesh::delaunay(&points);
        mesh.uv.fill(Vec2::new(0.6, -0.8));
        mesh.calculate_normals();

        let tiles = mesh.sliced_grid(BoundingBox::default(), 2);

        assert!(
            tiles
                .iter()
                .all(|tile| tile.uv.len() == tile.vertices.len())
        );
        assert!(
            tiles
                .iter()
                .flat_map(|tile| &tile.uv)
                .all(|uv| uv.distance(Vec2::new(0.6, -0.8)) < 1.0e-6)
        );
    }

    #[test]
    fn clamped_grid_outer_edges_follow_the_coarser_mesh() {
        let points: Vec<Vec2> = (0..=2)
            .flat_map(|y| (0..=2).map(move |x| Vec2::new(x as f32 * 0.5, y as f32 * 0.5)))
            .collect();
        let mut coarse = Mesh::delaunay(&points);
        for vertex in &mut coarse.vertices {
            vertex.z = vertex.x + vertex.y;
        }
        coarse.calculate_normals();
        let mut fine = coarse.tessellated();
        for vertex in &mut fine.vertices {
            vertex.z += 1.0;
        }
        fine.calculate_normals();

        let tiles = fine.sliced_grid_clamped(
            BoundingBox::default(),
            2,
            &coarse,
            CLAMP_TOP | CLAMP_LEFT | CLAMP_BOTTOM | CLAMP_RIGHT,
        );

        assert!(tiles.iter().flat_map(|tile| &tile.vertices).all(|vertex| {
            let outer = vertex.x.abs() < 1.0e-6
                || (vertex.x - 1.0).abs() < 1.0e-6
                || vertex.y.abs() < 1.0e-6
                || (vertex.y - 1.0).abs() < 1.0e-6;
            !outer || (vertex.z - vertex.x - vertex.y).abs() < 1.0e-5
        }));
        assert!(tiles.iter().flat_map(|tile| &tile.vertices).any(|vertex| {
            vertex.x > 0.0
                && vertex.x < 1.0
                && vertex.y > 0.0
                && vertex.y < 1.0
                && (vertex.z - vertex.x - vertex.y - 1.0).abs() < 1.0e-5
        }));
    }

    #[test]
    fn compact_adjacency_is_sorted_unique_and_symmetric() {
        let mesh = Mesh::delaunay(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(0.4, 0.6),
        ]);
        let adjacency = mesh.adjacency();
        assert_eq!(adjacency.len(), mesh.vertices.len());
        for (vertex, neighbours) in adjacency.iter().enumerate() {
            assert!(neighbours.windows(2).all(|pair| pair[0] < pair[1]));
            assert!(
                neighbours
                    .iter()
                    .all(|&neighbour| adjacency[neighbour].binary_search(&vertex).is_ok())
            );
        }
    }

    #[test]
    fn selective_tessellation_stitches_unsplit_neighbours() {
        let mesh = Mesh {
            vertices: vec![
                crate::Vec3::new(0.0, 0.0, 0.0),
                crate::Vec3::new(1.0, 0.0, 1.0),
                crate::Vec3::new(1.0, 1.0, 0.0),
                crate::Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: vec![crate::Vec3::new(0.0, 0.0, 1.0); 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: Vec::new(),
        };
        let detailed = mesh.tessellated_where(|vertices| {
            vertices.iter().any(|vertex| {
                (vertex.x - 1.0).abs() < f32::EPSILON && vertex.y.abs() < f32::EPSILON
            })
        });
        assert_eq!(detailed.vertices.len(), 7);
        assert_eq!(detailed.triangles.len(), 18);
        assert!((detailed.surface_area_xy() - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn displacement_tessellation_refines_a_local_feature_only() {
        let feature_mesh = |scale: f32| {
            let mut mesh = Mesh {
                vertices: [
                    (-1.0, -1.0, 0.0),
                    (1.0, -1.0, 0.0),
                    (1.0, 1.0, 0.0),
                    (-1.0, 1.0, 0.0),
                    (0.0, 0.0, 0.8),
                    (3.0, 0.0, 0.0),
                    (4.0, 0.0, 0.0),
                    (3.0, 1.0, 0.0),
                ]
                .into_iter()
                .map(|(x, y, z)| Vec3::new(x, y, z) * scale)
                .collect(),
                normals: Vec::new(),
                triangles: vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4, 5, 6, 7],
                uv: Vec::new(),
            };
            mesh.calculate_normals();
            mesh
        };

        let mesh = feature_mesh(1.0);
        let detailed = mesh.tessellated_displaced(0.1);
        let scaled = feature_mesh(10.0).tessellated_displaced(0.1);

        assert_eq!(detailed.triangles.len() / 3, 17);
        assert_eq!(detailed.triangles.len(), scaled.triangles.len());
        assert!(detailed.triangles.len() < mesh.triangles.len() * 4);
    }

    #[test]
    fn displacement_tessellation_leaves_an_inclined_plane_flat() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.2),
                Vec3::new(1.0, 1.0, 0.2),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: Vec::new(),
        };
        mesh.calculate_normals();

        let detailed = mesh.tessellated_displaced(1.0e-4);

        assert_eq!(detailed.vertices, mesh.vertices);
        assert_eq!(detailed.triangles, mesh.triangles);
    }

    #[test]
    fn smoothing_moves_interior_but_preserves_perimeter() {
        let mut mesh = Mesh {
            vertices: vec![
                crate::Vec3::new(0.0, 0.0, 0.0),
                crate::Vec3::new(1.0, 0.0, 0.0),
                crate::Vec3::new(1.0, 1.0, 0.0),
                crate::Vec3::new(0.0, 1.0, 0.0),
                crate::Vec3::new(0.6, 0.6, 1.0),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4],
            uv: Vec::new(),
        };
        let perimeter = mesh.vertices[..4].to_vec();
        mesh.smooth();
        assert_eq!(&mesh.vertices[..4], perimeter);
        assert!((mesh.vertices[4].x - 0.52).abs() < 1.0e-6);
        assert!((mesh.vertices[4].y - 0.52).abs() < 1.0e-6);
        assert!((mesh.vertices[4].z - 0.2).abs() < 1.0e-6);
    }
}
