#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::collections::HashMap;

use crate::{
    BoundingBox, Mesh, River, Vec2, Vec3,
    coast::GeologyField,
    mesh::{CLAMP_BOTTOM, CLAMP_LEFT, CLAMP_RIGHT, CLAMP_TOP},
};

const CLIFF_FIELD_THRESHOLD: f32 = 0.12;
const MAX_REFINED_FACE_RATIO: f32 = 0.35;
const MAX_RETREAT_EDGE_RATIO: f32 = 0.62;
const RELAXATION_STRENGTH: f32 = 0.16;
const MIN_PROJECTED_AREA_RATIO: f32 = 0.08;
const PROJECTED_STABILIZATION_PASSES: usize = 16;
const CREASE_COSINE: f32 = 0.707_106_77;
const TRANSITION_FRACTION: f32 = 0.12;
// Squared twice-area in normalized island coordinates. LOD0 support edges can
// be below 1e-3, so a conventional world-space epsilon would reject valid
// terrain before the render stage has changed it.
const TRIANGLE_AREA_EPSILON: f32 = 1.0e-20;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RenderMesh {
    mesh: Mesh,
    support_positions: Vec<Vec3>,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipVertex {
    position: Vec3,
    normal: Vec3,
    uv: Vec2,
    support_position: Vec3,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ClipVertexKey([u32; 11]);

struct MeshSampler<'a> {
    mesh: &'a Mesh,
    dimension: usize,
    offsets: Vec<usize>,
    faces: Vec<u32>,
}

#[derive(Clone, Copy)]
struct EdgeUse {
    key: u64,
    face: usize,
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
            vertex.support_position.x.to_bits(),
            vertex.support_position.y.to_bits(),
            vertex.support_position.z.to_bits(),
        ])
    }
}

impl RenderMesh {
    #[must_use]
    pub(crate) fn identity(support: &Mesh) -> Self {
        Self {
            mesh: support.clone(),
            support_positions: support.vertices.clone(),
        }
    }

    #[must_use]
    pub(crate) fn generate(
        support: &Mesh,
        geology: GeologyField,
        rivers: &[River],
        hydraulic_strength: f32,
    ) -> Self {
        if hydraulic_strength <= 0.0 || support.triangles.is_empty() {
            return Self::identity(support);
        }

        let adjacency = support.adjacency();
        let protected = protected_vertices(support, &adjacency, rivers);
        let (vertex_strength, face_strength) =
            cliff_field(support, &adjacency, geology, hydraulic_strength, &protected);
        let selected = select_faces(support, &face_strength, &protected);
        if !selected.iter().any(|&value| value) {
            return Self::identity(support);
        }

        let mut render = Self::identity(support);
        let movable = render.refine_selected(&selected, &vertex_strength);
        let adjacency = render.form_cliffs(&movable, hydraulic_strength);
        render.improve_edges(&movable, &adjacency);
        render.split_crease_normals(&movable);
        if render.is_valid() {
            render
        } else {
            Self::identity(support)
        }
    }

    #[must_use]
    pub(crate) const fn mesh(&self) -> &Mesh {
        &self.mesh
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
        let transition_width = transition_width(&self.mesh, bounds);
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
                self.morphed_vertex(
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

    fn refine_selected(&mut self, selected: &[bool], strength: &[f32]) -> Vec<bool> {
        let source_triangles = std::mem::take(&mut self.mesh.triangles);
        let mut edges = Vec::with_capacity(selected.iter().filter(|&&value| value).count() * 3);
        for (face, triangle) in source_triangles.chunks_exact(3).enumerate() {
            if selected[face] {
                edges.extend([
                    edge_key(triangle[0], triangle[1]),
                    edge_key(triangle[1], triangle[2]),
                    edge_key(triangle[2], triangle[0]),
                ]);
            }
        }
        edges.sort_unstable();

        let mut unique_edges = Vec::with_capacity(edges.len());
        let mut edge_selected_faces = Vec::with_capacity(edges.len());
        let mut cursor = 0;
        while cursor < edges.len() {
            let key = edges[cursor];
            let end = edges[cursor..].partition_point(|candidate| *candidate == key) + cursor;
            unique_edges.push(key);
            edge_selected_faces.push((end - cursor) as u8);
            cursor = end;
        }

        let original_vertex_count = self.mesh.vertices.len();
        self.mesh.vertices.reserve(
            unique_edges
                .len()
                .saturating_add(original_vertex_count / 16),
        );
        self.mesh.uv.reserve(unique_edges.len());
        self.support_positions.reserve(unique_edges.len());
        let mut refined_strength = strength.to_vec();
        refined_strength.reserve(unique_edges.len());
        let mut movable = vec![false; original_vertex_count];
        movable.reserve(unique_edges.len());
        let mut midpoint_indices = Vec::with_capacity(unique_edges.len());
        for (&key, &selected_faces) in unique_edges.iter().zip(&edge_selected_faces) {
            let [a, b] = edge_vertices(key);
            let midpoint = self.mesh.vertices.len() as u32;
            midpoint_indices.push(midpoint);
            self.mesh
                .vertices
                .push((self.mesh.vertices[a] + self.mesh.vertices[b]) * 0.5);
            self.mesh.uv.push((self.mesh.uv[a] + self.mesh.uv[b]) * 0.5);
            self.support_positions
                .push((self.support_positions[a] + self.support_positions[b]) * 0.5);
            let midpoint_strength = (strength[a] + strength[b]) * 0.5;
            refined_strength.push(midpoint_strength);
            movable.push(selected_faces == 2 && midpoint_strength >= CLIFF_FIELD_THRESHOLD);
        }

        self.mesh.triangles = Vec::with_capacity(
            source_triangles
                .len()
                .saturating_add(unique_edges.len().saturating_mul(9)),
        );
        for (face, triangle) in source_triangles.chunks_exact(3).enumerate() {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
            let ab = midpoint_for(a, b, &unique_edges, &midpoint_indices);
            let ac = midpoint_for(a, c, &unique_edges, &midpoint_indices);
            let cb = midpoint_for(c, b, &unique_edges, &midpoint_indices);
            if selected[face] {
                let (Some(ab), Some(ac), Some(cb)) = (ab, ac, cb) else {
                    continue;
                };
                self.mesh
                    .triangles
                    .extend([a, ab, ac, b, cb, ab, c, ac, cb, ab, cb, ac]);
            } else {
                add_conforming_triangle(&mut self.mesh.triangles, a, b, c, ab, ac, cb);
            }
        }
        self.mesh.calculate_normals();
        movable
    }

    fn form_cliffs(&mut self, movable: &[bool], hydraulic_strength: f32) -> crate::Adjacency {
        let adjacency = self.mesh.adjacency();
        let strength_scale = (hydraulic_strength / 4.0).clamp(0.0, 2.0).sqrt();
        let mut before = self.mesh.vertices.clone();
        let mut next = before.clone();
        let mut unstable = vec![false; before.len()];

        for iteration in 0..2 {
            self.mesh.calculate_normals();
            before.copy_from_slice(&self.mesh.vertices);
            next.copy_from_slice(&before);
            for index in 0..self.mesh.vertices.len() {
                if !movable[index] {
                    continue;
                }
                let neighbours = &adjacency[index];
                if neighbours.is_empty() {
                    continue;
                }
                let mean_edge = neighbours
                    .iter()
                    .map(|&other| before[index].distance(before[other]))
                    .sum::<f32>()
                    / neighbours.len() as f32;
                let normal = self.mesh.normals[index];
                let steepness = slope_displacement_weight(normal.z);
                let step = mean_edge * 0.26 * strength_scale * steepness;
                let candidate = before[index] - normal * step;
                let anchor = self.support_positions[index];
                let maximum = mean_edge * MAX_RETREAT_EDGE_RATIO * strength_scale.max(0.5);
                let displacement = candidate - anchor;
                next[index] = if displacement.length() > maximum {
                    anchor + displacement.normalize_or_zero() * maximum
                } else {
                    candidate
                };
            }
            stabilize_projected_faces(
                &self.support_positions,
                &before,
                &mut next,
                &self.mesh.triangles,
                movable,
                &mut unstable,
            );
            std::mem::swap(&mut self.mesh.vertices, &mut next);
            if iteration == 0 {
                before.copy_from_slice(&self.mesh.vertices);
                next.copy_from_slice(&before);
                for index in 0..self.mesh.vertices.len() {
                    if !movable[index] {
                        continue;
                    }
                    let neighbours = &adjacency[index];
                    let (sum, count) =
                        neighbours
                            .iter()
                            .fold((Vec3::ZERO, 0_u32), |(sum, count), &other| {
                                if movable[other] {
                                    (sum + before[other], count + 1)
                                } else {
                                    (sum, count)
                                }
                            });
                    if count == 0 {
                        continue;
                    }
                    let delta = sum / count as f32 - before[index];
                    let normal = self.mesh.normals[index];
                    let tangent = delta - normal * delta.dot(normal);
                    next[index] = before[index] + tangent * RELAXATION_STRENGTH;
                }
                stabilize_projected_faces(
                    &self.support_positions,
                    &before,
                    &mut next,
                    &self.mesh.triangles,
                    movable,
                    &mut unstable,
                );
                std::mem::swap(&mut self.mesh.vertices, &mut next);
            }
        }
        self.mesh.calculate_normals();

        for (index, vertex) in self.mesh.vertices.iter_mut().enumerate() {
            if !vertex.is_finite() {
                *vertex = self.support_positions[index];
            }
        }
        self.mesh.calculate_normals();
        adjacency
    }

    fn improve_edges(&mut self, movable: &[bool], adjacency: &crate::Adjacency) {
        let mut uses = edge_uses(&self.mesh, movable, true);
        uses.sort_unstable_by_key(|edge| edge.key);
        let mut face_used = vec![false; self.mesh.triangles.len() / 3];
        let mut cursor = 0;
        while cursor < uses.len() {
            let key = uses[cursor].key;
            let end = uses[cursor..].partition_point(|edge| edge.key == key) + cursor;
            if end - cursor == 2 {
                let [a, b] = edge_vertices(key);
                let first_face = uses[cursor].face;
                let second_face = uses[cursor + 1].face;
                if movable[a] && movable[b] && !face_used[first_face] && !face_used[second_face] {
                    let first = face_indices(&self.mesh, first_face);
                    let second = face_indices(&self.mesh, second_face);
                    let Some(c) = opposite_vertex(first, a, b) else {
                        cursor = end;
                        continue;
                    };
                    let Some(d) = opposite_vertex(second, a, b) else {
                        cursor = end;
                        continue;
                    };
                    if c != d && adjacency[c].binary_search(&d).is_err() {
                        let before =
                            minimum_angle(&self.mesh, first).min(minimum_angle(&self.mesh, second));
                        let reference =
                            face_normal(&self.mesh, first) + face_normal(&self.mesh, second);
                        let proposed_first = oriented_face(&self.mesh, [c, d, a], reference);
                        let proposed_second = oriented_face(&self.mesh, [d, c, b], reference);
                        let after = minimum_angle(&self.mesh, proposed_first)
                            .min(minimum_angle(&self.mesh, proposed_second));
                        let first_render_projection =
                            projected_face_area(&self.mesh, proposed_first);
                        let second_render_projection =
                            projected_face_area(&self.mesh, proposed_second);
                        let first_support_projection =
                            projected_area(&self.support_positions, proposed_first);
                        let second_support_projection =
                            projected_area(&self.support_positions, proposed_second);
                        if after > before + 1.0e-4
                            && face_area_squared(&self.mesh, proposed_first) > TRIANGLE_AREA_EPSILON
                            && face_area_squared(&self.mesh, proposed_second)
                                > TRIANGLE_AREA_EPSILON
                            && projected_orientation_is_stable(
                                first_support_projection,
                                first_render_projection,
                            )
                            && projected_orientation_is_stable(
                                second_support_projection,
                                second_render_projection,
                            )
                        {
                            set_face(&mut self.mesh, first_face, proposed_first);
                            set_face(&mut self.mesh, second_face, proposed_second);
                            face_used[first_face] = true;
                            face_used[second_face] = true;
                        }
                    }
                }
            }
            cursor = end;
        }
        self.mesh.calculate_normals();
    }

    fn split_crease_normals(&mut self, movable: &[bool]) {
        let mut uses = edge_uses(&self.mesh, movable, false);
        uses.sort_unstable_by_key(|edge| edge.key);
        let mut crease_faces = vec![false; self.mesh.triangles.len() / 3];
        let mut cursor = 0;
        while cursor < uses.len() {
            let key = uses[cursor].key;
            let end = uses[cursor..].partition_point(|edge| edge.key == key) + cursor;
            if end - cursor == 2 {
                let first = uses[cursor].face;
                let second = uses[cursor + 1].face;
                if face_normal(&self.mesh, face_indices(&self.mesh, first))
                    .dot(face_normal(&self.mesh, face_indices(&self.mesh, second)))
                    < CREASE_COSINE
                {
                    crease_faces[first] = true;
                    crease_faces[second] = true;
                }
            }
            cursor = end;
        }
        if !crease_faces.iter().any(|&value| value) {
            return;
        }

        let duplicate_count = crease_faces.iter().filter(|&&value| value).count() * 3;
        self.mesh.vertices.reserve(duplicate_count);
        self.mesh.normals.reserve(duplicate_count);
        self.mesh.uv.reserve(duplicate_count);
        self.support_positions.reserve(duplicate_count);
        for (face, duplicate) in crease_faces.into_iter().enumerate() {
            if !duplicate {
                continue;
            }
            let offset = face * 3;
            let face_normal = face_normal(&self.mesh, face_indices(&self.mesh, face));
            for corner in 0..3 {
                let source = self.mesh.triangles[offset + corner] as usize;
                let target = self.mesh.vertices.len() as u32;
                self.mesh.vertices.push(self.mesh.vertices[source]);
                self.mesh.normals.push(face_normal);
                self.mesh.uv.push(self.mesh.uv[source]);
                self.support_positions.push(self.support_positions[source]);
                self.mesh.triangles[offset + corner] = target;
            }
        }
        self.mesh.calculate_normals();
    }

    fn morphed_vertex(
        &self,
        index: usize,
        bounds: BoundingBox,
        coarse_sampler: Option<&MeshSampler<'_>>,
        clamp_sides: u8,
        transition_width: f32,
    ) -> ClipVertex {
        let support = self.support_positions[index];
        let uv = self
            .mesh
            .uv
            .get(index)
            .copied()
            .unwrap_or(support.truncate());
        let Some(coarse_sampler) = coarse_sampler.filter(|_| clamp_sides != 0) else {
            return ClipVertex {
                position: self.mesh.vertices[index],
                normal: self.mesh.normals[index],
                uv,
                support_position: support,
            };
        };
        let distance = clamp_distance(support, bounds, clamp_sides);
        let detail_weight = smoothstep((distance / transition_width).clamp(0.0, 1.0));
        let (coarse_height, coarse_normal) = coarse_sampler
            .sample(support.truncate())
            .unwrap_or((support.z, self.mesh.normals[index]));
        let coarse = Vec3::new(support.x, support.y, coarse_height);
        let support_position = coarse.lerp(support, detail_weight);
        ClipVertex {
            position: support_position + (self.mesh.vertices[index] - support) * detail_weight,
            normal: coarse_normal
                .lerp(self.mesh.normals[index], detail_weight)
                .normalize_or_zero(),
            uv,
            support_position,
        }
    }

    fn is_valid(&self) -> bool {
        self.mesh.vertices.len() == self.mesh.normals.len()
            && self.mesh.vertices.len() == self.mesh.uv.len()
            && self.mesh.vertices.len() == self.support_positions.len()
            && self
                .mesh
                .vertices
                .iter()
                .chain(&self.mesh.normals)
                .chain(&self.support_positions)
                .all(|value| value.is_finite())
            && self.mesh.triangles.chunks_exact(3).all(|triangle| {
                let [a, b, c] = [
                    triangle[0] as usize,
                    triangle[1] as usize,
                    triangle[2] as usize,
                ];
                if a >= self.mesh.vertices.len()
                    || b >= self.mesh.vertices.len()
                    || c >= self.mesh.vertices.len()
                {
                    return false;
                }
                let render_area = (self.mesh.vertices[b] - self.mesh.vertices[a])
                    .cross(self.mesh.vertices[c] - self.mesh.vertices[a])
                    .length_squared();
                let support_area = (self.support_positions[b] - self.support_positions[a])
                    .cross(self.support_positions[c] - self.support_positions[a])
                    .length_squared();
                let render_projection = projected_area(&self.mesh.vertices, [a, b, c]);
                let support_projection = projected_area(&self.support_positions, [a, b, c]);
                render_area >= support_area * 1.0e-4
                    && projected_orientation_is_stable(support_projection, render_projection)
            })
    }
}

fn cliff_field(
    mesh: &Mesh,
    adjacency: &crate::Adjacency,
    geology: GeologyField,
    hydraulic_strength: f32,
    protected: &[bool],
) -> (Vec<f32>, Vec<f32>) {
    let face_count = mesh.triangles.len() / 3;
    let mut face_strength = vec![0.0; face_count];
    let mut vertex_sum = vec![0.0; mesh.vertices.len()];
    let mut vertex_faces = vec![0_u16; mesh.vertices.len()];
    let hydraulic_scale = (hydraulic_strength / 4.0).clamp(0.0, 2.0).sqrt();

    for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
        let indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if indices.iter().any(|&index| protected[index]) {
            continue;
        }
        let positions = indices.map(|index| mesh.vertices[index]);
        if positions.iter().any(|position| position.z <= 0.0) {
            continue;
        }
        let cross = (positions[1] - positions[0]).cross(positions[2] - positions[0]);
        let normal = cross.try_normalize().unwrap_or(Vec3::Z);
        let slope = slope_displacement_weight(normal.z);
        if slope == 0.0 {
            continue;
        }
        let mean_edge = (positions[0].distance(positions[1])
            + positions[1].distance(positions[2])
            + positions[2].distance(positions[0]))
            / 3.0;
        let minimum = positions
            .iter()
            .map(|point| point.z)
            .fold(f32::MAX, f32::min);
        let maximum = positions
            .iter()
            .map(|point| point.z)
            .fold(f32::MIN, f32::max);
        let relief = smoothstep(((maximum - minimum) / mean_edge.max(f32::EPSILON) - 0.15) / 0.65);
        let hardness = indices
            .iter()
            .map(|&index| geology.hardness(mesh.vertices[index].truncate()))
            .sum::<f32>()
            / 3.0;
        let value = slope * relief.clamp(0.0, 1.0) * hardness.mul_add(0.65, 0.35) * hydraulic_scale;
        face_strength[face] = value;
        for index in indices {
            vertex_sum[index] += value;
            vertex_faces[index] = vertex_faces[index].saturating_add(1);
        }
    }

    let raw: Vec<f32> = vertex_sum
        .iter()
        .zip(&vertex_faces)
        .map(|(&sum, &count)| sum / f32::from(count.max(1)))
        .collect();
    let smoothed = raw
        .iter()
        .enumerate()
        .map(|(index, &value)| {
            if protected[index] {
                return 0.0;
            }
            let (sum, count) = adjacency[index]
                .iter()
                .filter(|&&other| !protected[other])
                .fold((value, 1_u32), |(sum, count), &other| {
                    (sum + raw[other], count + 1)
                });
            sum / count as f32
        })
        .collect();
    (smoothed, face_strength)
}

fn select_faces(mesh: &Mesh, face_strength: &[f32], protected: &[bool]) -> Vec<bool> {
    let mut candidates = Vec::new();
    for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
        if face_strength[face] >= CLIFF_FIELD_THRESHOLD
            && triangle.iter().all(|&vertex| !protected[vertex as usize])
        {
            candidates.push((face_strength[face], face));
        }
    }
    candidates.sort_unstable_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    let maximum = ((mesh.triangles.len() / 3) as f32 * MAX_REFINED_FACE_RATIO) as usize;
    candidates.truncate(maximum.max(1));
    let mut selected = vec![false; mesh.triangles.len() / 3];
    for (_, face) in candidates {
        selected[face] = true;
    }
    selected
}

fn protected_vertices(mesh: &Mesh, adjacency: &crate::Adjacency, rivers: &[River]) -> Vec<bool> {
    let mut protected = mesh.perimeter_mask();
    let mut frontier = Vec::new();
    for node in rivers.iter().flat_map(|river| &river.nodes) {
        if node.vertex < protected.len() && !protected[node.vertex] {
            protected[node.vertex] = true;
            frontier.push(node.vertex);
        }
    }
    for _ in 0..3 {
        let mut next = Vec::with_capacity(frontier.len().saturating_mul(4));
        for vertex in frontier.drain(..) {
            for &other in &adjacency[vertex] {
                if !protected[other] {
                    protected[other] = true;
                    next.push(other);
                }
            }
        }
        frontier = next;
    }
    protected
}

fn slope_displacement_weight(normal_z: f32) -> f32 {
    let vertical_alignment = normal_z.clamp(0.0, 1.0);
    let horizontal_alignment = (1.0 - vertical_alignment * vertical_alignment).sqrt();
    2.0 * vertical_alignment * horizontal_alignment
}

fn transition_width(mesh: &Mesh, bounds: BoundingBox) -> f32 {
    let extent = (bounds.max.x - bounds.min.x)
        .abs()
        .min((bounds.max.y - bounds.min.y).abs());
    let sampled_edges = mesh.triangles.chunks_exact(3).take(4096);
    let (total, count) = sampled_edges.fold((0.0, 0_u32), |(total, count), triangle| {
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
    let blend = |values: [Vec3; 3]| {
        values[0] * weights[0] + values[1] * weights[1] + values[2] * weights[2]
    };
    vertex.position.z = source
        .iter()
        .zip(weights)
        .map(|(source, weight)| source.position.z * weight)
        .sum();
    vertex.normal = blend(source.map(|source| source.normal)).normalize_or_zero();
    vertex.uv = source
        .iter()
        .zip(weights)
        .map(|(source, weight)| source.uv * weight)
        .sum();
    vertex.support_position = blend(source.map(|source| source.support_position));
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
        support_position: a.support_position.lerp(b.support_position, interpolation),
    }
}

fn canonicalize_clip_vertex(mut vertex: ClipVertex) -> ClipVertex {
    let canonical = |value: f32| (value * 10_000_000.0).round() / 10_000_000.0;
    vertex.position = vertex.position.map(canonical);
    vertex.normal = vertex.normal.map(canonical).normalize_or_zero();
    vertex.uv = vertex.uv.map(canonical);
    vertex.support_position = vertex.support_position.map(canonical);
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
        vertex.support_position = vertex.position;
    }
    vertex
}

impl<'a> MeshSampler<'a> {
    fn new(mesh: &'a Mesh) -> Self {
        let face_count = mesh.triangles.len() / 3;
        let dimension = ((face_count as f32).sqrt().ceil() as usize / 2).clamp(8, 512);
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

fn edge_key(a: u32, b: u32) -> u64 {
    let [a, b] = if a < b { [a, b] } else { [b, a] };
    (u64::from(a) << 32) | u64::from(b)
}

fn edge_uses(mesh: &Mesh, included: &[bool], require_both: bool) -> Vec<EdgeUse> {
    let marked = included.iter().filter(|&&value| value).count();
    let mut output = Vec::with_capacity(marked.saturating_mul(12));
    for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
        for [a, b] in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            let a_included = included.get(a as usize).copied().unwrap_or(false);
            let b_included = included.get(b as usize).copied().unwrap_or(false);
            let use_edge = if require_both {
                a_included && b_included
            } else {
                a_included || b_included
            };
            if use_edge {
                output.push(EdgeUse {
                    key: edge_key(a, b),
                    face,
                });
            }
        }
    }
    output
}

fn face_indices(mesh: &Mesh, face: usize) -> [usize; 3] {
    let offset = face * 3;
    [
        mesh.triangles[offset] as usize,
        mesh.triangles[offset + 1] as usize,
        mesh.triangles[offset + 2] as usize,
    ]
}

fn set_face(mesh: &mut Mesh, face: usize, vertices: [usize; 3]) {
    let offset = face * 3;
    mesh.triangles[offset..offset + 3].copy_from_slice(&vertices.map(|vertex| vertex as u32));
}

fn opposite_vertex(face: [usize; 3], a: usize, b: usize) -> Option<usize> {
    face.into_iter().find(|&vertex| vertex != a && vertex != b)
}

fn face_normal(mesh: &Mesh, face: [usize; 3]) -> Vec3 {
    let [a, b, c] = face.map(|vertex| mesh.vertices[vertex]);
    (b - a).cross(c - a).normalize_or_zero()
}

fn face_area_squared(mesh: &Mesh, face: [usize; 3]) -> f32 {
    let [a, b, c] = face.map(|vertex| mesh.vertices[vertex]);
    (b - a).cross(c - a).length_squared()
}

fn projected_face_area(mesh: &Mesh, face: [usize; 3]) -> f32 {
    projected_area(&mesh.vertices, face)
}

fn projected_area(vertices: &[Vec3], face: [usize; 3]) -> f32 {
    let [a, b, c] = face.map(|vertex| vertices[vertex]);
    (b - a).truncate().perp_dot((c - a).truncate())
}

fn stabilize_projected_faces(
    reference: &[Vec3],
    before: &[Vec3],
    candidate: &mut [Vec3],
    triangles: &[u32],
    movable: &[bool],
    unstable: &mut [bool],
) {
    for _ in 0..PROJECTED_STABILIZATION_PASSES {
        unstable.fill(false);
        let mut found_unstable = false;
        for triangle in triangles.chunks_exact(3) {
            let face = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            let baseline = projected_area(reference, face);
            if baseline == 0.0 {
                continue;
            }
            let projected = projected_area(candidate, face);
            if projected * baseline <= 0.0
                || projected.abs() < baseline.abs() * MIN_PROJECTED_AREA_RATIO
            {
                for vertex in face {
                    if movable[vertex] {
                        unstable[vertex] = true;
                        found_unstable = true;
                    }
                }
            }
        }
        if !found_unstable {
            return;
        }
        for (index, is_unstable) in unstable.iter().copied().enumerate() {
            if is_unstable {
                candidate[index] = before[index].lerp(candidate[index], 0.5);
            }
        }
    }

    for (index, is_unstable) in unstable.iter().copied().enumerate() {
        if is_unstable {
            candidate[index] = before[index];
        }
    }
}

fn projected_orientation_is_stable(reference: f32, candidate: f32) -> bool {
    reference == 0.0
        || (candidate * reference > 0.0
            && candidate.abs() >= reference.abs() * MIN_PROJECTED_AREA_RATIO)
}

fn oriented_face(mesh: &Mesh, mut face: [usize; 3], reference: Vec3) -> [usize; 3] {
    if face_normal(mesh, face).dot(reference) < 0.0 {
        face.swap(0, 1);
    }
    face
}

fn minimum_angle(mesh: &Mesh, face: [usize; 3]) -> f32 {
    let positions = face.map(|vertex| mesh.vertices[vertex]);
    (0..3)
        .map(|index| {
            let center = positions[index];
            let first = (positions[(index + 1) % 3] - center).normalize_or_zero();
            let second = (positions[(index + 2) % 3] - center).normalize_or_zero();
            first.dot(second).clamp(-1.0, 1.0).acos()
        })
        .fold(f32::MAX, f32::min)
}

fn edge_vertices(key: u64) -> [usize; 2] {
    [(key >> 32) as usize, (key & u64::from(u32::MAX)) as usize]
}

fn midpoint_for(a: u32, b: u32, edges: &[u64], midpoints: &[u32]) -> Option<u32> {
    edges
        .binary_search(&edge_key(a, b))
        .ok()
        .map(|index| midpoints[index])
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

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::{
        ClipVertex, RenderMesh, append_clipped_triangle, projected_area, slope_displacement_weight,
        stabilize_projected_faces,
    };
    use crate::{BoundingBox, Mesh, Vec2, Vec3};
    use std::collections::HashMap;

    fn overhang() -> RenderMesh {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.25, 0.25, 0.0),
                Vec3::new(0.75, 0.25, 0.0),
                Vec3::new(0.25, 0.25, 1.0),
            ],
            normals: vec![Vec3::Y; 3],
            triangles: vec![0, 1, 2],
            uv: vec![Vec2::ZERO, Vec2::X, Vec2::Y],
        };
        RenderMesh {
            support_positions: mesh.vertices.clone(),
            mesh,
        }
    }

    #[test]
    fn identity_keeps_support_positions_and_uvs() {
        let render = RenderMesh::identity(overhang().mesh());
        assert_eq!(render.mesh.vertices, render.support_positions);
        assert_eq!(render.mesh.uv.len(), render.mesh.vertices.len());
    }

    #[test]
    fn cliff_retreat_peaks_at_forty_five_degrees_and_stops_at_vertical() {
        let flat = slope_displacement_weight(1.0);
        let thirty_degrees = slope_displacement_weight(0.866_025_4);
        let forty_five_degrees = slope_displacement_weight(0.707_106_77);
        let sixty_degrees = slope_displacement_weight(0.5);
        let near_vertical = slope_displacement_weight(0.017_452_406);
        let vertical = slope_displacement_weight(0.0);

        assert_eq!(flat.to_bits(), 0.0_f32.to_bits());
        assert!((forty_five_degrees - 1.0).abs() < 1.0e-6);
        assert!((thirty_degrees - sixty_degrees).abs() < 1.0e-6);
        assert!(forty_five_degrees > thirty_degrees);
        assert!(near_vertical < sixty_degrees);
        assert_eq!(vertical.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn cliff_retreat_guard_prevents_projected_face_folding() {
        let before = vec![Vec3::ZERO, Vec3::X, Vec3::Y];
        let mut candidate = vec![Vec3::ZERO, Vec3::X, -Vec3::Y];
        let triangles = [0, 1, 2];
        let movable = [false, false, true];
        let mut unstable = [false; 3];

        stabilize_projected_faces(
            &before,
            &before,
            &mut candidate,
            &triangles,
            &movable,
            &mut unstable,
        );

        assert!(projected_area(&candidate, [0, 1, 2]) > 0.0);
    }

    #[test]
    fn vertical_triangle_clips_without_projected_barycentrics() {
        let render = overhang();
        let tiles = render.sliced_grid(BoundingBox::default(), 2, None, 0);
        assert_eq!(tiles.len(), 4);
        assert!(
            tiles[0]
                .vertices
                .iter()
                .any(|vertex| (vertex.z - 1.0).abs() < f32::EPSILON)
        );
        assert!(tiles[1].triangles.len() >= 3);
        assert!(
            tiles
                .iter()
                .flat_map(|tile| &tile.vertices)
                .all(|vertex| vertex.is_finite())
        );
    }

    #[test]
    fn clipping_interpolates_every_vertex_attribute() {
        let source = [
            ClipVertex {
                position: Vec3::new(-1.0, 0.0, 0.0),
                normal: Vec3::X,
                uv: Vec2::ZERO,
                support_position: Vec3::new(-1.0, 0.0, 0.5),
            },
            ClipVertex {
                position: Vec3::new(1.0, 0.0, 1.0),
                normal: Vec3::Y,
                uv: Vec2::X,
                support_position: Vec3::new(1.0, 0.0, 1.5),
            },
            ClipVertex {
                position: Vec3::new(0.0, 1.0, 0.5),
                normal: Vec3::Z,
                uv: Vec2::Y,
                support_position: Vec3::new(0.0, 1.0, 1.0),
            },
        ];
        let mut mesh = Mesh::default();
        let mut remap = HashMap::new();
        let bounds = BoundingBox::new(
            Vec3::new(0.0, -1.0, f32::MIN),
            Vec3::new(1.0, 1.0, f32::MAX),
        );
        append_clipped_triangle(source, bounds, bounds, None, 0, &mut mesh, &mut remap);
        assert_eq!(mesh.vertices.len(), mesh.normals.len());
        assert_eq!(mesh.vertices.len(), mesh.uv.len());
        assert!(mesh.vertices.iter().any(|vertex| {
            vertex.x.to_bits() == 0.0_f32.to_bits() && (vertex.z - 0.5).abs() < 1.0e-6
        }));
    }
}
