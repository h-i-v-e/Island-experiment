use super::{
    IndexedParallelIterator, IntoParallelRefMutIterator, Mesh, Ordering, ParallelIterator,
    TRIANGLE_INDEX_OFFSET_BUDGET_BYTES, Vec2, Vec3, barycentric, bin_coordinate, size_of,
    triangle_bin_bounds,
};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct TriangleIndex {
    pub(super) dimension: usize,
    pub(super) offsets: Vec<usize>,
    pub(super) faces: Vec<u32>,
}

impl TriangleIndex {
    pub(super) fn new(mesh: &Mesh) -> Self {
        let face_count = mesh.triangles.len() / 3;
        let maximum_dimension =
            ((TRIANGLE_INDEX_OFFSET_BUDGET_BYTES / size_of::<usize>()) as f64).sqrt() as usize;
        let dimension =
            ((face_count as f64 / 8.0).sqrt().ceil() as usize).clamp(8, maximum_dimension.max(8));
        let bin_count = dimension * dimension;
        let mut counts = vec![0_usize; bin_count];
        for triangle in mesh.triangles.chunks_exact(3) {
            let (min_x, max_x, min_y, max_y) = triangle_bin_bounds(mesh, triangle, dimension);
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
        let mut faces = vec![0_u32; *offsets.last().unwrap_or(&0)];
        for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
            let (min_x, max_x, min_y, max_y) = triangle_bin_bounds(mesh, triangle, dimension);
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let bin = y * dimension + x;
                    faces[cursor[bin]] = face as u32;
                    cursor[bin] += 1;
                }
            }
        }
        Self {
            dimension,
            offsets,
            faces,
        }
    }

    pub(super) fn candidates(&self, point: Vec2) -> &[u32] {
        let [x, y] = self.point_bin(point);
        let bin = y * self.dimension + x;
        &self.faces[self.offsets[bin]..self.offsets[bin + 1]]
    }

    pub(super) fn point_bin(&self, point: Vec2) -> [usize; 2] {
        [
            bin_coordinate(point.x, self.dimension),
            bin_coordinate(point.y, self.dimension),
        ]
    }

    pub(super) fn bin_faces(&self, x: usize, y: usize) -> &[u32] {
        let bin = y * self.dimension + x;
        &self.faces[self.offsets[bin]..self.offsets[bin + 1]]
    }

    pub(super) fn nearest_vertex(&self, mesh: &Mesh, point: Vec2) -> usize {
        let [origin_x, origin_y] = self.point_bin(point);
        let mut best = None::<(f32, usize)>;
        for radius in 0..self.dimension {
            let minimum_x = origin_x.saturating_sub(radius);
            let maximum_x = (origin_x + radius).min(self.dimension - 1);
            let minimum_y = origin_y.saturating_sub(radius);
            let maximum_y = (origin_y + radius).min(self.dimension - 1);
            for y in minimum_y..=maximum_y {
                for x in minimum_x..=maximum_x {
                    if radius != 0
                        && x != minimum_x
                        && x != maximum_x
                        && y != minimum_y
                        && y != maximum_y
                    {
                        continue;
                    }
                    for &face in self.bin_faces(x, y) {
                        let offset = face as usize * 3;
                        for &vertex in &mesh.triangles[offset..offset + 3] {
                            let vertex = vertex as usize;
                            let distance = mesh.vertices[vertex].truncate().distance_squared(point);
                            if best.is_none_or(|current| match distance.total_cmp(&current.0) {
                                Ordering::Less => true,
                                Ordering::Equal => vertex < current.1,
                                Ordering::Greater => false,
                            }) {
                                best = Some((distance, vertex));
                            }
                        }
                    }
                }
            }
            if let Some((distance, vertex)) = best {
                let cell_width = 1.0 / self.dimension as f32;
                if radius == 0 || distance.sqrt() <= (radius as f32 + 1.0) * cell_width {
                    return vertex;
                }
            }
        }
        debug_assert!(
            mesh.vertices.is_empty(),
            "triangle index did not reference any vertex"
        );
        0
    }
}

/// The finest irregular terrain mesh plus a derived spatial lookup index.
///
/// The index uses bins only to locate triangles for sampling. It never defines
/// vertices, connectivity, erosion, flow, rivers, or levels of detail.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Terrain {
    pub(super) mesh: Mesh,
    pub(super) triangle_index: TriangleIndex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceMaps {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) normal_rgb: Vec<u8>,
    pub(super) occlusion: Vec<u8>,
}

impl SurfaceMaps {
    /// Transfers baked texture buffers to a renderer without copying pixels.
    #[must_use]
    pub fn into_parts(self) -> (u32, u32, Vec<u8>, Vec<u8>) {
        (self.width, self.height, self.normal_rgb, self.occlusion)
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
    pub fn normal_rgb(&self) -> &[u8] {
        &self.normal_rgb
    }

    #[must_use]
    pub fn occlusion(&self) -> &[u8] {
        &self.occlusion
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SurfaceSample {
    pub(super) position: Vec3,
    pub(super) normal: Vec3,
}

/// One final-LOD0 terrain lookup, including the supporting triangle used for
/// interpolation. Decoration generators use this to classify and anchor an
/// object without repeating the spatial-index query for every terrain field.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TerrainSupportSample {
    pub(crate) position: Vec3,
    pub(crate) normal: Vec3,
    pub(crate) triangle: [usize; 3],
    pub(crate) weights: [f32; 3],
}

impl Terrain {
    #[cfg(test)]
    pub(crate) fn new(mesh: Mesh) -> Self {
        let triangle_index = TriangleIndex::new(&mesh);
        Self::with_index(mesh, triangle_index)
    }

    pub(super) fn with_index(mesh: Mesh, triangle_index: TriangleIndex) -> Self {
        Self {
            mesh,
            triangle_index,
        }
    }

    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub fn vertices(&self) -> &[Vec3] {
        &self.mesh.vertices
    }

    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.mesh.vertices.len()
    }

    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.mesh.triangles.len() / 3
    }

    #[must_use]
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        sample_mesh_triangle(
            &self.mesh,
            &self.triangle_index,
            Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)),
        )
        .map_or_else(
            || {
                let point = Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                self.mesh.vertices[self.triangle_index.nearest_vertex(&self.mesh, point)].z
            },
            |(triangle, weights)| {
                weights[0].mul_add(
                    self.mesh.vertices[triangle[0]].z,
                    weights[1].mul_add(
                        self.mesh.vertices[triangle[1]].z,
                        weights[2] * self.mesh.vertices[triangle[2]].z,
                    ),
                )
            },
        )
    }

    #[must_use]
    pub fn sample_normal(&self, u: f32, v: f32) -> Vec3 {
        sample_mesh_triangle(
            &self.mesh,
            &self.triangle_index,
            Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)),
        )
        .map_or_else(
            || {
                let point = Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                let nearest = self.triangle_index.nearest_vertex(&self.mesh, point);
                self.mesh.normals[nearest]
            },
            |(triangle, weights)| {
                (self.mesh.normals[triangle[0]] * weights[0]
                    + self.mesh.normals[triangle[1]] * weights[1]
                    + self.mesh.normals[triangle[2]] * weights[2])
                    .try_normalize()
                    .unwrap_or(Vec3::Z)
            },
        )
    }

    pub(crate) fn sample_vertex_scalar(&self, values: &[f32], u: f32, v: f32) -> f32 {
        debug_assert_eq!(values.len(), self.mesh.vertices.len());
        let point = Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        sample_mesh_triangle(&self.mesh, &self.triangle_index, point).map_or_else(
            || values[self.triangle_index.nearest_vertex(&self.mesh, point)],
            |(triangle, weights)| {
                weights[0].mul_add(
                    values[triangle[0]],
                    weights[1].mul_add(values[triangle[1]], weights[2] * values[triangle[2]]),
                )
            },
        )
    }

    pub(crate) fn sample_surface(&self, u: f32, v: f32) -> (f32, Vec3) {
        sample_mesh_surface(&self.mesh, &self.triangle_index, u, v)
    }

    /// Atlas baking needs interpolation even on sub-metre source triangles.
    /// Double precision and translated edges avoid the legacy sampler's
    /// absolute projected-area cutoff without changing physics or field queries.
    #[cfg(feature = "surface-atlas")]
    pub(crate) fn sample_atlas_surface(&self, u: f32, v: f32) -> (f32, Vec3) {
        let point = Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)).as_dvec2();
        self.triangle_index
            .candidates(point.as_vec2())
            .iter()
            .find_map(|&face| {
                let offset = face as usize * 3;
                let triangle = [
                    self.mesh.triangles[offset] as usize,
                    self.mesh.triangles[offset + 1] as usize,
                    self.mesh.triangles[offset + 2] as usize,
                ];
                let positions = triangle.map(|index| self.mesh.vertices[index].as_dvec3());
                let first = positions[1].truncate() - positions[0].truncate();
                let second = positions[2].truncate() - positions[0].truncate();
                let relative = point - positions[0].truncate();
                let area = first.perp_dot(second);
                if !area.is_finite() || area == 0.0 {
                    return None;
                }
                let b = relative.perp_dot(second) / area;
                let c = first.perp_dot(relative) / area;
                let weights = [1.0 - b - c, b, c];
                if weights
                    .iter()
                    .any(|&weight| !weight.is_finite() || weight < -1.0e-5)
                {
                    return None;
                }
                let height = positions[0].z * weights[0]
                    + positions[1].z * weights[1]
                    + positions[2].z * weights[2];
                let normals = triangle.map(|index| self.mesh.normals[index].as_dvec3());
                let normal =
                    (normals[0] * weights[0] + normals[1] * weights[1] + normals[2] * weights[2])
                        .try_normalize()
                        .map_or(Vec3::Z, glam::DVec3::as_vec3);
                Some((height as f32, normal))
            })
            .unwrap_or_else(|| self.sample_surface(u, v))
    }

    pub(crate) fn sample_support(&self, point: Vec2) -> TerrainSupportSample {
        let point = point.clamp(Vec2::ZERO, Vec2::ONE);
        sample_mesh_triangle(&self.mesh, &self.triangle_index, point).map_or_else(
            || {
                let nearest = self.triangle_index.nearest_vertex(&self.mesh, point);
                TerrainSupportSample {
                    position: self.mesh.vertices[nearest],
                    normal: self.mesh.normals[nearest],
                    triangle: [nearest; 3],
                    weights: [1.0, 0.0, 0.0],
                }
            },
            |(triangle, weights)| {
                let position = self.mesh.vertices[triangle[0]] * weights[0]
                    + self.mesh.vertices[triangle[1]] * weights[1]
                    + self.mesh.vertices[triangle[2]] * weights[2];
                let normal = (self.mesh.normals[triangle[0]] * weights[0]
                    + self.mesh.normals[triangle[1]] * weights[1]
                    + self.mesh.normals[triangle[2]] * weights[2])
                    .try_normalize()
                    .unwrap_or(Vec3::Z);
                TerrainSupportSample {
                    position,
                    normal,
                    triangle,
                    weights,
                }
            },
        )
    }
}

pub(super) fn sample_mesh_surface(
    mesh: &Mesh,
    triangle_index: &TriangleIndex,
    u: f32,
    v: f32,
) -> (f32, Vec3) {
    let point = Vec2::new(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    sample_mesh_triangle(mesh, triangle_index, point).map_or_else(
        || {
            let nearest = triangle_index.nearest_vertex(mesh, point);
            (mesh.vertices[nearest].z, mesh.normals[nearest])
        },
        |(triangle, weights)| {
            let elevation = weights[0].mul_add(
                mesh.vertices[triangle[0]].z,
                weights[1].mul_add(
                    mesh.vertices[triangle[1]].z,
                    weights[2] * mesh.vertices[triangle[2]].z,
                ),
            );
            let normal = (mesh.normals[triangle[0]] * weights[0]
                + mesh.normals[triangle[1]] * weights[1]
                + mesh.normals[triangle[2]] * weights[2])
                .try_normalize()
                .unwrap_or(Vec3::Z);
            (elevation, normal)
        },
    )
}

pub(super) fn sample_mesh_triangle(
    mesh: &Mesh,
    triangle_index: &TriangleIndex,
    point: Vec2,
) -> Option<([usize; 3], [f32; 3])> {
    triangle_index.candidates(point).iter().find_map(|&face| {
        let offset = face as usize * 3;
        let triangle = [
            mesh.triangles[offset] as usize,
            mesh.triangles[offset + 1] as usize,
            mesh.triangles[offset + 2] as usize,
        ];
        barycentric(
            point,
            mesh.vertices[triangle[0]].truncate(),
            mesh.vertices[triangle[1]].truncate(),
            mesh.vertices[triangle[2]].truncate(),
        )
        .map(|weights| (triangle, weights))
    })
}

pub(super) fn bury_river_banks(mesh: &mut Mesh, terrain: &Mesh, triangle_index: &TriangleIndex) {
    let banks = mesh.perimeter_mask();
    mesh.vertices
        .par_iter_mut()
        .zip(banks)
        .filter(|(_, is_bank)| *is_bank)
        .for_each(|(vertex, _)| {
            let point = vertex.truncate();
            let terrain_height = triangle_index
                .candidates(point)
                .iter()
                .filter_map(|&face| {
                    let offset = face as usize * 3;
                    let triangle = [
                        terrain.triangles[offset] as usize,
                        terrain.triangles[offset + 1] as usize,
                        terrain.triangles[offset + 2] as usize,
                    ];
                    river_bank_height(point, triangle.map(|i| terrain.vertices[i]))
                })
                .min_by(f32::total_cmp)
                .unwrap_or_else(|| {
                    terrain.vertices[triangle_index.nearest_vertex(terrain, point)].z
                });
            // A submerged bank is an ocean handoff, not a dry contact to bury
            // into the seabed. Keep the boundary on the sea surface so the
            // later clip cannot expose a new floating edge above it.
            vertex.z = if terrain_height <= 0.0 {
                0.0
            } else {
                vertex.z.min(terrain_height)
            };
        });
    mesh.calculate_normals();
}

/// Waterfall refinement makes sub-metre faces. The legacy height sampler's
/// absolute area cutoff skips those faces and can bury a correctly pinned bank
/// at an unrelated nearest vertex. Use translated double-precision edges here;
/// physics and material-field sampling retain their existing query contract.
#[allow(clippy::cast_possible_truncation)]
fn river_bank_height(point: Vec2, triangle: [Vec3; 3]) -> Option<f32> {
    let [a, b, c] = triangle.map(Vec3::as_dvec3);
    let first = b.truncate() - a.truncate();
    let second = c.truncate() - a.truncate();
    let relative = point.as_dvec2() - a.truncate();
    let area = first.perp_dot(second);
    if !area.is_finite() || area == 0.0 {
        return None;
    }
    let b_weight = relative.perp_dot(second) / area;
    let c_weight = first.perp_dot(relative) / area;
    let weights = [1.0 - b_weight - c_weight, b_weight, c_weight];
    if weights.iter().any(|w| !w.is_finite() || *w < -1.0e-5) {
        return None;
    }
    Some((a.z * weights[0] + b.z * weights[1] + c.z * weights[2]) as f32)
}

#[cfg(test)]
mod river_bank_burial_tests {
    use super::*;

    #[test]
    fn submerged_bank_hands_water_to_the_sea_surface_instead_of_the_seabed() {
        let terrain = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, -0.01),
                Vec3::new(1.0, 0.0, -0.01),
                Vec3::new(0.0, 1.0, -0.01),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let mut water = Mesh {
            vertices: terrain
                .vertices
                .iter()
                .map(|p| Vec3::new(p.x, p.y, 0.00001))
                .collect(),
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        bury_river_banks(&mut water, &terrain, &TriangleIndex::new(&terrain));
        assert!(water.vertices.iter().all(|p| p.z.abs() < f32::EPSILON));
        assert!(
            crate::rivers::clip_river_above_sea(water)
                .triangles
                .is_empty()
        );
    }

    #[test]
    fn final_bank_burial_interpolates_refined_faces_instead_of_nearest_vertex_heights() {
        let origin = Vec2::new(0.25, 0.5);
        let side = 0.0001;
        let terrain = Mesh {
            vertices: vec![
                origin.extend(0.0),
                (origin + Vec2::X * side).extend(0.0003),
                (origin + Vec2::Y * side).extend(0.0006),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        let points = [
            Vec2::splat(side * 0.25),
            Vec2::new(side * 0.5, side * 0.25),
            Vec2::new(side * 0.25, side * 0.5),
        ]
        .map(|p| origin + p);
        let expected = points.map(|p| {
            river_bank_height(
                p,
                [
                    terrain.vertices[0],
                    terrain.vertices[1],
                    terrain.vertices[2],
                ],
            )
            .unwrap()
        });
        let mut water = Mesh {
            vertices: points.iter().map(|p| p.extend(0.001)).collect(),
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        bury_river_banks(&mut water, &terrain, &TriangleIndex::new(&terrain));
        for (position, expected) in water.vertices.iter().zip(expected) {
            assert!((position.z - expected).abs() < 1.0e-8);
            assert!(position.z > 0.0002);
        }
    }
}

#[cfg(all(test, feature = "surface-atlas"))]
mod atlas_sampling_tests {
    use super::*;

    fn varying_normal_triangle(offset: Vec2, scale: f32) -> Terrain {
        Terrain::new(Mesh {
            vertices: vec![
                offset.extend(0.0),
                (offset + Vec2::X * scale).extend(0.3),
                (offset + Vec2::Y * scale).extend(0.6),
            ],
            normals: vec![Vec3::Z, Vec3::X, Vec3::Y],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        })
    }

    #[test]
    fn atlas_sampler_interpolates_tiny_triangles_without_changing_legacy_sampling() {
        let expected = Vec3::ONE.normalize();
        for scale in [1.0, 0.0001] {
            let terrain = varying_normal_triangle(Vec2::ZERO, scale);
            let point = Vec2::splat(scale / 3.0);
            let (height, normal) = terrain.sample_atlas_surface(point.x, point.y);
            assert!((height - 0.3).abs() < 1.0e-6);
            assert!(normal.distance(expected) < 1.0e-6);
            if scale < 0.001 {
                let (legacy_height, legacy_normal) = terrain.sample_surface(point.x, point.y);
                assert!(legacy_height.abs() < f32::EPSILON);
                assert!(legacy_normal.distance(Vec3::Z) < f32::EPSILON);
                assert!(normal.dot(legacy_normal) < 0.6);
            }
        }
    }

    #[test]
    fn atlas_sampler_retains_precision_on_offset_tiny_triangles() {
        let offset = Vec2::new(0.25, 0.5);
        let scale = 0.0001;
        let terrain = varying_normal_triangle(offset, scale);
        let point = offset + Vec2::splat(scale / 3.0);
        let (height, normal) = terrain.sample_atlas_surface(point.x, point.y);
        // Input positions are still f32, so their offset quantization limits
        // the centroid precision before the double-precision interpolation.
        assert!((height - 0.3).abs() < 0.001);
        assert!(normal.distance(Vec3::ONE.normalize()) < 0.001);
    }

    #[test]
    fn atlas_sampler_retains_the_existing_fallback_outside_the_surface() {
        let terrain = varying_normal_triangle(Vec2::ZERO, 0.1);
        let expected = terrain.sample_surface(0.09, 0.09);
        let actual = terrain.sample_atlas_surface(0.09, 0.09);
        assert!((actual.0 - expected.0).abs() < f32::EPSILON);
        assert!(actual.1.distance(expected.1) < f32::EPSILON);
    }
}
