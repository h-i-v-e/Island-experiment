//! Independently generated square sections of one periodic coastal height field.
use std::collections::{BTreeMap, HashMap};

use super::{GeologyField, IslandOptions, Mesh, Rng, SurfaceMaterial, Vec2, Vec3, geology};

const PIN_BLEND_METRES: f32 = 30.0;
const SUPPORT_EPSILON: f32 = 1.0e-6; // Two millimetres in the normalized 2 km section.
type SupportVertexMap = HashMap<([u32; 3], u8), u32>;

/// A section of a flat coast which repeats along X; Y=0 is sea, Y=1 is inland.
/// Dimensions are normalized by the existing island width (2,000 metres).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct CoastalStripOptions {
    pub section_index: u32,
    pub section_count: u32,
    /// Initial edge intervals; joins and the rear edge double at every terrain refinement.
    pub seam_segments: u32,
    /// Width of the seam watershed and river exclusion collar.
    pub seam_blend_width: f32,
    /// Additional height at the section seam, tapered down before the sea.
    pub seam_ridge_height: f32,
}

impl Default for CoastalStripOptions {
    fn default() -> Self {
        Self {
            section_index: 0,
            section_count: 8,
            seam_segments: 32,
            seam_blend_width: 0.08,
            seam_ridge_height: 0.02,
        }
    }
}

impl CoastalStripOptions {
    /// Rejects inconsistent section identities and unbounded seam geometry.
    /// # Errors
    /// Returns an error for invalid section identities, seam resolution or dimensions.
    pub fn validate(self, terrain: IslandOptions) -> Result<Self, String> {
        if !(1..=4096).contains(&self.section_count) || self.section_index >= self.section_count {
            return Err(
                "coastal strip requires 1..4096 sections and section_index < section_count".into(),
            );
        }
        if !(8..=256).contains(&self.seam_segments) {
            return Err("coastal seam_segments must be in 8..256".into());
        }
        if !self.seam_blend_width.is_finite()
            || !(0.02..=0.25).contains(&self.seam_blend_width)
            || !self.seam_ridge_height.is_finite()
            || !(0.0..=terrain.max_height).contains(&self.seam_ridge_height)
        {
            return Err(
                "coastal seam width must be 0.02..0.25 and ridge height 0..maximum height".into(),
            );
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CoastalStripField {
    pub seed: u64,
    pub terrain: IslandOptions,
    pub options: CoastalStripOptions,
}

impl CoastalStripField {
    pub fn height(self, point: Vec2) -> f32 {
        let settings = geology::TerrainNoiseSettings::from(self.terrain);
        let position = geology::periodic_coordinates(
            point,
            self.options.section_index,
            self.options.section_count,
        );
        let noise = geology::terrain_noise(self.seed, position, settings);
        let mountains =
            geology::terrain_noise(self.seed ^ 0x6a09_e667_f3bc_c909, position, settings)
                .continental;
        // Each section has a broad bay between its two headlands. Global
        // periodic noise moves the shore and the mountain mass independently.
        let local_x = f64::from(point.x).rem_euclid(1.0);
        let bay = ((1.0 - (local_x * std::f64::consts::TAU).cos()) * 0.5) as f32;
        let wetness = ((self.terrain.water_ratio - 0.6) / 0.35).clamp(0.0, 1.0);
        let warp = 0.20 * self.terrain.continental_noise_strength
            / (1.0 + self.terrain.continental_noise_strength);
        let shore = 0.26 + wetness * 0.14 + bay * 0.36 + noise.continental * warp;
        let inland = smooth_unit((point.y - shore + 0.05) / 0.45);
        // Keep the front edge in open water where the next world row uses its
        // own island field. Shore and bay relief is fully active after 300 m.
        let offshore = smooth_unit(point.y / 0.15);
        let mountain_mass = (mountains * 0.5 + 0.5).powi(2);
        let continental = noise.continental * self.terrain.continental_noise_strength;
        // Fine relief stays active on either side of the shore, allowing its
        // high points to emerge as small islands rather than following a mask.
        let detail = noise.detail * self.terrain.detail_noise_strength * 2.0;
        let offset_envelope = (std::f32::consts::PI * point.y.clamp(0.0, 1.0)).sin();
        let base = (point.y - shore) * 0.8
            + continental * (0.25 + inland * 0.75) * 0.30 * offshore
            + mountain_mass * self.terrain.continental_noise_strength * inland * 0.90
            + detail * offshore
            + self.terrain.land_mass_offset * offset_envelope;
        let height = if base >= 0.0 {
            let relief = base.powf(self.terrain.coastal_slope_multiplier);
            self.terrain.max_height * relief / (0.65 + relief)
        } else {
            base * self.terrain.max_height * 0.28
        };
        let wrapped_x = point.x.rem_euclid(1.0);
        let distance = wrapped_x.min(1.0 - wrapped_x);
        let t = (distance / self.options.seam_blend_width).clamp(0.0, 1.0);
        let ridge = (1.0 - t * t * (3.0 - 2.0 * t)) * self.options.seam_ridge_height;
        height + ridge * smooth_unit(base / 0.20)
    }

    pub fn tessellate(self, mesh: &Mesh) -> super::TessellationResult {
        let mut refined =
            mesh.tessellated_displaced_refining_boundary(super::DETAIL_DISPLACEMENT_RATIO);
        for stencil in &refined.new_vertices {
            let point = &mut refined.mesh.vertices[stencil.vertex as usize];
            if Self::is_pinned(*point) {
                point.z = self.height(point.truncate());
            }
        }
        refined.mesh.calculate_normals();
        refined
    }

    pub fn pin_weight(point: Vec2) -> f32 {
        let distance = point.min(Vec2::ONE - point).min_element();
        1.0 - smooth_unit(distance * super::ISLAND_WORLD_METRES / PIN_BLEND_METRES)
    }

    pub fn is_blend_vertex(point: Vec3) -> bool {
        let distance = point.x.min(1.0 - point.x).min(1.0 - point.y);
        distance <= PIN_BLEND_METRES / super::ISLAND_WORLD_METRES + SUPPORT_EPSILON
    }

    /// Cut supporting rows into the finished surface, preserving its height
    /// outside the collar and avoiding changes to erosion or river routing.
    pub fn add_blend_support(self, mesh: &mut Mesh) -> Vec<super::EdgeSplitStencil> {
        let original_vertex_count = mesh.vertices.len();
        let mut splits = Vec::new();
        let mut vertices = Self::index_blend_vertices(mesh);
        for fraction in [1.0 / 3.0, 2.0 / 3.0, 1.0] {
            let distance = PIN_BLEND_METRES * fraction / super::ISLAND_WORLD_METRES;
            for (axis, coordinate) in [(0, distance), (0, 1.0 - distance), (1, 1.0 - distance)] {
                let fractions = Self::support_plane_fractions(mesh, axis, coordinate);
                let added = mesh.split_edges(&fractions);
                for split in &added {
                    let point = &mut mesh.vertices[split.vertex as usize];
                    // Only domain edges need a canonical shared coordinate.
                    // Interior samples must stay on their source edges; an
                    // extra f32 snap can invert a very thin adjacent face.
                    if Self::is_pinned(*point) {
                        point[axis] = coordinate;
                        point.z = self.height(point.truncate());
                    }
                }
                Self::weld_support_splits(mesh, &added, &mut vertices);
                splits.extend(added);
            }
        }
        mesh.repair_projected_foldovers_preserving(|a, b| {
            (a as usize) < original_vertex_count && (b as usize) < original_vertex_count
        });
        splits
    }

    fn point_precedes(point: Vec3, previous: Vec3) -> bool {
        point
            .x
            .total_cmp(&previous.x)
            .then(point.y.total_cmp(&previous.y))
            .then(point.z.total_cmp(&previous.z))
            .is_lt()
    }

    fn index_blend_vertices(mesh: &Mesh) -> SupportVertexMap {
        let mut vertices = HashMap::new();
        for (index, &point) in mesh.vertices.iter().enumerate() {
            if !Self::is_blend_vertex(point) {
                continue;
            }
            let canonical = vertices
                .entry(Self::support_vertex_key(point))
                .or_insert(index as u32);
            if Self::point_precedes(point, mesh.vertices[*canonical as usize]) {
                *canonical = index as u32;
            }
        }
        vertices
    }

    fn support_plane_fractions(
        mesh: &Mesh,
        axis: usize,
        coordinate: f32,
    ) -> BTreeMap<(u32, u32), f32> {
        mesh.triangles
            .chunks_exact(3)
            .flat_map(|triangle| {
                [
                    (triangle[0], triangle[1]),
                    (triangle[1], triangle[2]),
                    (triangle[2], triangle[0]),
                ]
            })
            .filter_map(|(left, right)| {
                let edge = (left.min(right), left.max(right));
                let start = mesh.vertices[edge.0 as usize][axis] - coordinate;
                let end = mesh.vertices[edge.1 as usize][axis] - coordinate;
                // Near-endpoint cuts can round back onto an existing vertex.
                (start * end < 0.0 && start.abs().min(end.abs()) > SUPPORT_EPSILON)
                    .then_some((edge, start / (start - end)))
            })
            .collect()
    }

    fn support_vertex_key(point: Vec3) -> ([u32; 3], u8) {
        // Two millimetres in world space. Domain sides remain distinct so a
        // nearby interior cut cannot replace an exact shared boundary sample.
        let side = u8::from(point.x <= 0.0)
            | (u8::from(point.x >= 1.0) << 1)
            | (u8::from(point.y <= 0.0) << 2)
            | (u8::from(point.y >= 1.0) << 3);
        (
            point
                .to_array()
                .map(|value| (value * 1_000_000.0).round().to_bits()),
            side,
        )
    }

    fn matching_support_vertex(
        mesh: &Mesh,
        point: Vec3,
        vertices: &SupportVertexMap,
    ) -> Option<u32> {
        let (key, side) = Self::support_vertex_key(point);
        let grid = key.map(f32::from_bits);
        let mut best = None;
        for dx in [-1.0, 0.0, 1.0] {
            for dy in [-1.0, 0.0, 1.0] {
                for dz in [-1.0, 0.0, 1.0] {
                    let key = [
                        (grid[0] + dx).to_bits(),
                        (grid[1] + dy).to_bits(),
                        (grid[2] + dz).to_bits(),
                    ];
                    let Some(&index) = vertices.get(&(key, side)) else {
                        continue;
                    };
                    let candidate = mesh.vertices[index as usize];
                    if (candidate - point).abs().max_element() > SUPPORT_EPSILON {
                        continue;
                    }
                    if best.is_none_or(|previous: u32| {
                        Self::point_precedes(candidate, mesh.vertices[previous as usize])
                    }) {
                        best = Some(index);
                    }
                }
            }
        }
        best
    }

    fn weld_support_splits(
        mesh: &mut Mesh,
        splits: &[super::EdgeSplitStencil],
        vertices: &mut SupportVertexMap,
    ) {
        // Intersecting support rows can round two cuts onto the same point.
        // Share their index and discard the collapsed, zero-area face. Keep
        // vertex slots so all material/river stencils retain their indices.
        let aliases: HashMap<_, _> = splits
            .iter()
            .filter_map(|split| {
                let point = mesh.vertices[split.vertex as usize];
                let key = Self::support_vertex_key(point);
                let canonical =
                    Self::matching_support_vertex(mesh, point, vertices).unwrap_or(split.vertex);
                vertices.entry(key).or_insert(canonical);
                (canonical != split.vertex).then_some((split.vertex, canonical))
            })
            .collect();
        for index in &mut mesh.triangles {
            if let Some(&canonical) = aliases.get(index) {
                *index = canonical;
            }
        }
        let mut write = 0;
        for read in (0..mesh.triangles.len()).step_by(3) {
            let [a, b, c] = [
                mesh.triangles[read],
                mesh.triangles[read + 1],
                mesh.triangles[read + 2],
            ];
            if a != b && a != c && b != c {
                mesh.triangles.copy_within(read..read + 3, write);
                write += 3;
            }
        }
        mesh.triangles.truncate(write);
    }

    pub fn blend_boundary_heights(self, mesh: &mut Mesh) {
        for point in &mut mesh.vertices {
            let weight = Self::pin_weight(point.truncate());
            if weight > 0.0 {
                let original = self.height(point.truncate());
                point.z = original + (point.z - original) * (1.0 - weight);
            }
        }
    }

    pub fn river_exclusions(self, mesh: &Mesh, adjacency: &super::Adjacency) -> Vec<bool> {
        let mut excluded: Vec<bool> = mesh
            .vertices
            .iter()
            .map(|&point| {
                Self::is_pinned(point)
                    || point.x.min(1.0 - point.x) <= self.options.seam_blend_width
            })
            .collect();
        // River rings, confluence connectors and bank smoothing need space too.
        for _ in 0..8 {
            let old = excluded.clone();
            for (index, value) in excluded.iter_mut().enumerate() {
                *value |= adjacency[index].iter().any(|&neighbor| old[neighbor]);
            }
        }
        excluded
    }

    pub fn is_pinned(point: Vec3) -> bool {
        point.x <= 0.0 || point.x >= 1.0 || point.y <= 0.0 || point.y >= 1.0
    }

    pub fn normal(mut self, mut point: Vec2) -> Vec3 {
        // Evaluate both representations with exactly the same f32 stencil.
        if point.x.to_bits() == 1.0_f32.to_bits() {
            self.options.section_index =
                (self.options.section_index + 1) % self.options.section_count;
            point.x = 0.0;
        }
        let h = 1.0e-4;
        let x = self.height(point + Vec2::X * h) - self.height(point - Vec2::X * h);
        let y = self.height(point + Vec2::Y * h) - self.height(point - Vec2::Y * h);
        Vec3::new(-x, -y, 2.0 * h).normalize()
    }

    pub fn boundary_distance_to_land(self, point: Vec2) -> f32 {
        if self.height(point) >= 0.0 {
            return 0.0;
        }
        let mut previous = point;
        let mut previous_height = self.height(previous);
        for step in 1..=128 {
            let y = point.y + (1.0 - point.y) * (step as f32 / 128.0);
            let next = Vec2::new(point.x, y);
            let height = self.height(next);
            if height >= 0.0 {
                let t = -previous_height / (height - previous_height).max(f32::EPSILON);
                return (previous.y + (y - previous.y) * t - point.y).max(0.0);
            }
            previous = next;
            previous_height = height;
        }
        1.0 - point.y
    }

    pub fn apply_boundary_normals(self, mesh: &mut Mesh) {
        for (normal, &position) in mesh.normals.iter_mut().zip(&mesh.vertices) {
            let weight = Self::pin_weight(position.truncate());
            if weight > 0.0 {
                let original = self.normal(position.truncate());
                *normal = if Self::is_pinned(position) {
                    original
                } else {
                    normal.lerp(original, weight).normalize()
                };
            }
        }
    }

    pub fn apply_boundary_materials(self, mesh: &Mesh, material: &mut super::TerrainMaterialField) {
        let geology = GeologyField::periodic(
            self.seed,
            self.terrain,
            self.options.section_index,
            self.options.section_count,
        );
        for (value, &position) in material.values.iter_mut().zip(&mesh.vertices) {
            if !Self::is_pinned(position) {
                continue;
            }
            let height = self.height(position.truncate());
            let cover = if height > 0.0 {
                let depth = self.terrain.initial_soil_depth_metres / super::ISLAND_WORLD_METRES;
                smooth_unit(depth / 0.002)
            } else {
                0.0
            };
            let sea = 1.0 - smooth_unit((height * super::ISLAND_WORLD_METRES - 2.0) / 18.0);
            *value = super::Vec4::new(geology.hardness(position.truncate()), cover, 0.0, sea);
        }
    }

    pub fn initial(self) -> (Mesh, SurfaceMaterial) {
        let n = self.options.seam_segments;
        let mut points = Vec::with_capacity(self.terrain.terrain_size as usize + (n * 4) as usize);
        let mut rng =
            Rng::new(self.seed ^ u64::from(self.options.section_index).wrapping_mul(0x9e37_79b9));
        for _ in 0..self.terrain.terrain_size.saturating_sub(8) {
            // Keep random points off the fixed boundary and its skinny triangles.
            points.push(Vec2::new(
                rng.unit().mul_add(0.98, 0.01),
                rng.unit().mul_add(0.98, 0.01),
            ));
        }
        for index in 0..n {
            let p = index as f32 / n as f32;
            let q = (n - index) as f32 / n as f32;
            points.extend([
                Vec2::new(0.0, p),
                Vec2::new(p, 1.0),
                Vec2::new(1.0, q),
                Vec2::new(q, 0.0),
            ]);
        }
        let mut mesh = Mesh::delaunay(&points);
        for vertex in &mut mesh.vertices {
            vertex.z = self.height(vertex.truncate());
        }
        mesh.calculate_normals();
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        material.coastal_strip = Some(self);
        material.initialize_geology(
            &mesh,
            GeologyField::periodic(
                self.seed,
                self.terrain,
                self.options.section_index,
                self.options.section_count,
            ),
        );
        material.initialize_soil(&mesh, self.terrain.initial_soil_depth_metres);
        (mesh, material)
    }
}

fn smooth_unit(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caves::{CaveNetworkOptions, CaveOptions, CaveWalkOptions};
    use crate::{FernOptions, ForestOptions, GenerationMethod, Island, ReedOptions};

    fn terrain_options() -> IslandOptions {
        IslandOptions {
            terrain_size: 32,
            water_ratio: 0.6,
            initial_soil_depth_metres: 1.0,
            ..IslandOptions::default()
        }
    }

    fn field(index: u32, count: u32) -> CoastalStripField {
        CoastalStripField {
            seed: 42,
            terrain: terrain_options(),
            options: CoastalStripOptions {
                section_index: index,
                section_count: count,
                seam_segments: 9,
                ..CoastalStripOptions::default()
            },
        }
    }

    fn edge(mesh: &Mesh, x: f32) -> Vec<(Vec3, Vec3)> {
        let mut edge: Vec<_> = mesh
            .vertices
            .iter()
            .zip(&mesh.normals)
            .filter(|(point, _)| point.x == x)
            .map(|(&point, &normal)| (Vec3::new(0.0, point.y, point.z), normal))
            .collect();
        edge.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
        edge
    }

    #[test]
    fn periodic_field_matches_adjacent_and_closing_height_hardness_and_normals() {
        for count in [1, 2, 3, 8, 17, 4096] {
            for index in [0, count - 1] {
                let a = field(index, count);
                let b = field((index + 1) % count, count);
                let hard_a = GeologyField::periodic(a.seed, a.terrain, index, count);
                let hard_b = GeologyField::periodic(b.seed, b.terrain, (index + 1) % count, count);
                for y in [0.0, 0.1, 0.5, 0.65, 1.0] {
                    let right = Vec2::new(1.0, y);
                    let left = Vec2::new(0.0, y);
                    assert_eq!(a.height(right).to_bits(), b.height(left).to_bits());
                    assert_eq!(a.normal(right), b.normal(left));
                    assert_eq!(
                        hard_a.hardness(right).to_bits(),
                        hard_b.hardness(left).to_bits()
                    );
                }
            }
        }
    }

    #[test]
    fn long_faces_remain_sea_and_inland_even_at_extreme_offsets() {
        for offset in [-2.0, 2.0] {
            let mut field = field(0, 3);
            field.terrain.land_mass_offset = offset;
            for x in [0.0, 0.25, 0.5, 1.0] {
                assert!(field.height(Vec2::new(x, 0.0)) < 0.0);
                assert!(field.height(Vec2::new(x, 1.0)) > 0.0);
            }
        }
    }

    #[test]
    fn seaward_edge_stays_submerged_with_strong_authored_noise_and_offsets() {
        for count in [1, 3, 8] {
            for index in [0, count - 1] {
                for seed in [0, 42, 1840] {
                    for (continental, detail, offset) in
                        [(0.0, 4.0, -2.0), (4.0, 0.0, 2.0), (4.0, 4.0, -2.0)]
                    {
                        let mut field = field(index, count);
                        field.seed = seed;
                        field.terrain.continental_noise_strength = continental;
                        field.terrain.detail_noise_strength = detail;
                        field.terrain.land_mass_offset = offset;
                        for sample in 0..=128 {
                            let point = Vec2::new(sample as f32 / 128.0, 0.0);
                            assert!(field.height(point) < 0.0, "open sea at {field:?}/{point:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn default_back_face_remains_irregular_at_both_water_settings() {
        for count in [1, 3, 8] {
            for water_ratio in [0.6, 0.95] {
                let mut field = field(0, count);
                field.terrain.water_ratio = water_ratio;
                let heights = (0..=256).map(|x| field.height(Vec2::new(x as f32 / 256.0, 1.0)));
                let (minimum, maximum) = heights.fold(
                    (f32::INFINITY, f32::NEG_INFINITY),
                    |(minimum, maximum), height| (minimum.min(height), maximum.max(height)),
                );
                assert!(maximum - minimum > 0.02, "back relief for {field:?}");
                assert!(maximum <= field.terrain.max_height + field.options.seam_ridge_height);
            }
        }
    }

    #[test]
    fn zero_noise_still_has_one_broad_bay_between_the_section_headlands() {
        for count in [1, 3, 8] {
            for water_ratio in [0.6, 0.95] {
                let mut field = field(0, count);
                field.terrain.water_ratio = water_ratio;
                field.terrain.continental_noise_strength = 0.0;
                field.terrain.detail_noise_strength = 0.0;
                let headland = if water_ratio < 0.9 { 0.26 } else { 0.40 };
                let middle = headland + 0.18;
                assert!(field.height(Vec2::new(0.0, middle)) > 0.0);
                assert!(field.height(Vec2::new(1.0, middle)) > 0.0);
                assert!(field.height(Vec2::new(0.5, middle)) < 0.0);
                assert!(field.height(Vec2::new(0.5, headland + 0.38)) > 0.0);
            }
        }
    }

    struct LandPatch {
        area: usize,
        bounds: [usize; 4],
        peak: f32,
    }

    fn land_patches(heights: &[f32], dimension: usize) -> (Vec<usize>, Vec<LandPatch>) {
        let mut labels = vec![usize::MAX; heights.len()];
        let mut patches = Vec::new();
        for (first, &height) in heights.iter().enumerate() {
            if height <= 0.0 || labels[first] != usize::MAX {
                continue;
            }
            let id = patches.len();
            let mut pending = vec![first];
            labels[first] = id;
            let mut patch = LandPatch {
                area: 0,
                bounds: [dimension, dimension, 0, 0],
                peak: 0.0,
            };
            while let Some(index) = pending.pop() {
                let (x, y) = (index % dimension, index / dimension);
                patch.area += 1;
                patch.bounds[0] = patch.bounds[0].min(x);
                patch.bounds[1] = patch.bounds[1].min(y);
                patch.bounds[2] = patch.bounds[2].max(x);
                patch.bounds[3] = patch.bounds[3].max(y);
                patch.peak = patch.peak.max(heights[index]);
                for (nx, ny) in [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ] {
                    if nx >= dimension || ny >= dimension {
                        continue;
                    }
                    let neighbor = ny * dimension + nx;
                    if heights[neighbor] > 0.0 && labels[neighbor] == usize::MAX {
                        labels[neighbor] = id;
                        pending.push(neighbor);
                    }
                }
            }
            patches.push(patch);
        }
        (labels, patches)
    }

    #[test]
    fn processed_bay_keeps_detached_offshore_land_and_a_recessed_mainland_shore() {
        let island = Island::generate_coastal_strip_with_cave_walks(
            42,
            IslandOptions {
                terrain_size: 256,
                water_ratio: 0.95,
                river_source_elevation_boost: 20.0,
                ..IslandOptions::default()
            },
            CoastalStripOptions {
                section_count: 3,
                ..CoastalStripOptions::default()
            },
            ForestOptions {
                noise_threshold: 1.0,
                prototype_count: 1,
                ..ForestOptions::default()
            },
            ReedOptions {
                coverage_threshold: 1.0,
                ..ReedOptions::default()
            },
            FernOptions {
                coverage_threshold: 1.0,
                ..FernOptions::default()
            },
            CaveOptions::default(),
            CaveNetworkOptions::default(),
            CaveWalkOptions::default(),
            GenerationMethod::Cpu,
        )
        .unwrap();
        let dimension = 257;
        let heights = island.height_map(dimension, dimension);
        let dimension = dimension as usize;
        let (labels, patches) = land_patches(&heights, dimension);
        assert!(
            patches.iter().any(|patch| {
                let [min_x, min_y, max_x, max_y] = patch.bounds;
                patch.area >= 16
                    && patch.peak > 1.0 / super::super::ISLAND_WORLD_METRES
                    && min_x > 0
                    && min_y > 0
                    && max_x < dimension - 1
                    && max_y < dimension - 1
                    && max_x - min_x >= 4
                    && max_y - min_y >= 4
            }),
            "a finished bay must retain a real detached island, not a sub-metre pixel"
        );
        let mainland = patches
            .iter()
            .enumerate()
            .max_by_key(|(_, patch)| patch.area)
            .unwrap()
            .0;
        let shore = |x| {
            (0..dimension)
                .find(|&y| labels[y * dimension + x] == mainland)
                .unwrap()
        };
        let median_shore = |start, end| {
            let mut samples: Vec<_> = (start..=end).map(shore).collect();
            samples.sort_unstable();
            samples[samples.len() / 2]
        };
        let centre = median_shore(90, 166);
        let headlands = median_shore(7, 38).max(median_shore(218, 249));
        assert!(
            centre > headlands + 20,
            "the broad mainland bay must be recessed at least 156 m behind both headland bands"
        );
        assert!(
            !island.rivers().is_empty(),
            "the finished bay must also process rivers"
        );
    }

    #[test]
    fn pinning_fades_over_thirty_metres_from_side_and_rear_edges() {
        let field = field(0, 2);
        let distances = [0.0, 0.001, 7.5, 15.0, 22.5, 29.999, 30.0, 31.0];
        let points = distances.into_iter().flat_map(|metres| {
            let d = metres / super::super::ISLAND_WORLD_METRES;
            [
                Vec2::new(d, 0.8),
                Vec2::new(1.0 - d, 0.8),
                Vec2::new(0.5, 1.0 - d),
            ]
        });
        let mut mesh = Mesh {
            vertices: points
                .map(|point| point.extend(field.height(point) + 0.05))
                .collect(),
            ..Mesh::default()
        };
        let source = mesh.vertices.clone();
        field.blend_boundary_heights(&mut mesh);
        for (original, actual) in source.iter().zip(&mesh.vertices) {
            let distance = actual
                .truncate()
                .min(Vec2::ONE - actual.truncate())
                .min_element()
                * super::super::ISLAND_WORLD_METRES;
            let expected_weight = smooth_unit(distance / 30.0);
            let original_noise = field.height(actual.truncate());
            assert!((actual.z - original_noise - 0.05 * expected_weight).abs() < 1.0e-7);
            if distance >= 30.0 {
                assert_eq!(
                    original, actual,
                    "terrain outside the collar must be unchanged"
                );
            }
        }
        assert_eq!(CoastalStripField::pin_weight(Vec2::new(0.0, 0.8)), 1.0);
        assert!((CoastalStripField::pin_weight(Vec2::new(0.0075, 0.8)) - 0.5).abs() < 1.0e-6);
        assert_eq!(CoastalStripField::pin_weight(Vec2::new(0.02, 0.8)), 0.0);
    }

    #[test]
    fn refined_midpoints_recover_noise_relief_instead_of_linear_edge_heights() {
        let field = field(0, 2);
        let (mesh, _) = field.initial();
        let refined = field.tessellate(&mesh);
        let mut recovered_relief = false;
        for stencil in &refined.new_vertices {
            let point = refined.mesh.vertices[stencil.vertex as usize];
            if !CoastalStripField::is_pinned(point) {
                continue;
            }
            assert_eq!(point.z, field.height(point.truncate()));
            let linear = (mesh.vertices[stencil.surrounding[0] as usize].z
                + mesh.vertices[stencil.surrounding[1] as usize].z)
                * 0.5;
            recovered_relief |= (point.z - linear).abs() > 1.0e-5;
        }
        assert!(
            recovered_relief,
            "new edge samples must reveal the original noise"
        );
        let rear_count = |mesh: &Mesh| mesh.vertices.iter().filter(|p| p.y == 1.0).count();
        assert_eq!(rear_count(&refined.mesh), (rear_count(&mesh) - 1) * 2 + 1);
    }

    #[test]
    fn dense_edges_have_support_vertices_inside_the_thirty_metre_blend() {
        let field = field(0, 2);
        let (mut mesh, _) = field.initial();
        for _ in 0..6 {
            mesh = field.tessellate(&mesh).mesh;
        }
        let _splits = field.add_blend_support(&mut mesh);
        let mut compared = 0;
        for triangle in mesh.triangles.chunks_exact(3) {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
            for (left, right, other) in [(a, b, c), (a, c, b), (b, c, a)] {
                let left = mesh.vertices[left as usize];
                let right = mesh.vertices[right as usize];
                let other = mesh.vertices[other as usize];
                let distance = if left.x <= 0.0 && right.x <= 0.0 {
                    other.x
                } else if left.x >= 1.0 && right.x >= 1.0 {
                    1.0 - other.x
                } else if left.y >= 1.0 && right.y >= 1.0 {
                    1.0 - other.y
                } else {
                    continue;
                };
                assert!(
                    distance * super::super::ISLAND_WORLD_METRES < 30.0,
                    "edge blend needs an interior support sample: {left:?}, {right:?}, {other:?}"
                );
                compared += 1;
            }
        }
        assert!(compared > 0);
    }

    #[test]
    fn non_power_of_two_boundary_doubles_with_noise_samples_and_survives_erosion() {
        let field = field(0, 2);
        let (mut mesh, mut material) = field.initial();
        let positions: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|&&p| CoastalStripField::is_pinned(p))
            .copied()
            .collect();
        let left = edge(&mesh, 0.0);
        assert_eq!(left.len(), 10);
        let mut scratch = super::super::generation::GenerationScratch::default();
        for pass in 1..=3 {
            let tessellation = field.tessellate(&mesh);
            (mesh, material) = material.into_tessellated(&mesh, tessellation);
            let adjacency = mesh.adjacency();
            super::super::erosion::hydraulic_erode_stage(
                &mut mesh,
                &adjacency,
                &mut material,
                0.5,
                field.terrain,
                &mut scratch,
            )
            .unwrap();
            super::super::erosion::erode_mesh(
                &mut mesh,
                &adjacency,
                &mut material,
                field.terrain,
                3,
            );
            mesh.smooth_with(&adjacency);
            for &original in &positions {
                assert!(mesh.vertices.contains(&original));
            }
            assert_eq!(edge(&mesh, 0.0).len(), (left.len() - 1) * (1 << pass) + 1);
            for &point in mesh
                .vertices
                .iter()
                .filter(|&&point| CoastalStripField::is_pinned(point))
            {
                assert_eq!(point.z, field.height(point.truncate()));
            }
            assert_eq!(mesh.projected_foldover_count(), 0);
        }
    }

    #[test]
    fn processed_sections_keep_identical_seams_at_every_render_lod() {
        let generate = |index| {
            Island::generate_coastal_strip_with_cave_walks(
                42,
                terrain_options(),
                field(index, 2).options,
                ForestOptions {
                    noise_threshold: 1.0,
                    prototype_count: 1,
                    ..ForestOptions::default()
                },
                ReedOptions {
                    coverage_threshold: 1.0,
                    ..ReedOptions::default()
                },
                FernOptions {
                    coverage_threshold: 1.0,
                    ..FernOptions::default()
                },
                CaveOptions::default(),
                CaveNetworkOptions::default(),
                CaveWalkOptions::default(),
                GenerationMethod::Cpu,
            )
            .unwrap()
        };
        let a = generate(0);
        let b = generate(1);
        assert!(
            !a.rivers().is_empty() || !b.rivers().is_empty(),
            "coastal fixture must exercise routed rivers"
        );
        let support_profile = |mesh: &Mesh| {
            mesh.triangles
                .iter()
                .map(|&index| mesh.vertices[index as usize])
                .filter(|&point| CoastalStripField::is_blend_vertex(point))
                .map(|point| point.to_array().map(f32::to_bits))
                .collect::<std::collections::BTreeSet<_>>()
        };
        let expected_support = support_profile(a.lod(0).unwrap());
        for lod in 0..=2 {
            let a_mesh = a.lod(lod).unwrap();
            assert!(
                expected_support.is_subset(&support_profile(a_mesh)),
                "LOD{lod} lost smoothing-strip geometry"
            );
            let b_mesh = b.lod(lod).unwrap();
            assert_eq!(edge(a_mesh, 1.0), edge(b_mesh, 0.0), "adjacent LOD{lod}");
            assert_eq!(edge(b_mesh, 1.0), edge(a_mesh, 0.0), "closing LOD{lod}");
            assert_eq!(
                edge(a_mesh, 0.0),
                edge(a.lod(0).unwrap(), 0.0),
                "same partition across LOD{lod}"
            );
            assert_eq!(a_mesh.projected_foldover_count(), 0, "LOD{lod}");
            for (island, mesh) in [(&a, a_mesh), (&b, b_mesh)] {
                assert!(mesh.vertices.iter().all(|p| p.is_finite()));
                assert!(
                    island
                        .rivers()
                        .iter()
                        .flat_map(|r| &r.nodes)
                        .all(|n| n.position.x > field(0, 2).options.seam_blend_width
                            && n.position.x < 1.0 - field(0, 2).options.seam_blend_width)
                );
            }
        }
        let exported_edge = |island: &Island, lod, side| {
            let tiles = island
                .render_mesh_grid(
                    lod,
                    crate::BoundingBox::new(
                        Vec3::new(0.0, 0.0, f32::MIN),
                        Vec3::new(1.0, 1.0, f32::MAX),
                    ),
                    2,
                    15,
                )
                .unwrap();
            let mut samples: Vec<_> = tiles.iter().flat_map(|mesh| edge(mesh, side)).collect();
            samples.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
            samples.dedup_by(|a, b| (a.0.y - b.0.y).abs() < 1.0e-6);
            samples
        };
        for lod in 0..=2 {
            for (left, right) in [(&a, &b), (&b, &a)] {
                let a_edge = exported_edge(left, lod, 1.0);
                let b_edge = exported_edge(right, lod, 0.0);
                assert_eq!(
                    a_edge.len(),
                    b_edge.len(),
                    "exported seam partition LOD{lod}"
                );
                for ((pa, na), (pb, nb)) in a_edge.into_iter().zip(b_edge) {
                    assert!(
                        pa.distance(pb) < 1.0e-6,
                        "exported seam positions LOD{lod}: {pa:?} vs {pb:?}"
                    );
                    assert!(
                        na.distance(nb) < 1.0e-5,
                        "exported seam normals LOD{lod}: {na:?} vs {nb:?}"
                    );
                }
            }
        }
        let material_edge = |island: &Island, x| {
            let mesh = island.lod(0).unwrap();
            let mut values: Vec<_> = mesh
                .vertices
                .iter()
                .zip(&island.material.values)
                .filter(|(p, _)| p.x == x)
                .map(|(p, v)| (p.y, *v))
                .collect();
            values.sort_by(|a, b| a.0.total_cmp(&b.0));
            values
        };
        assert_eq!(material_edge(&a, 1.0), material_edge(&b, 0.0));
        assert_eq!(material_edge(&b, 1.0), material_edge(&a, 0.0));
        let horizon_edge = |island: &Island, x| {
            let mesh = island.horizon_mesh();
            let mut values: Vec<_> = mesh
                .vertices
                .iter()
                .filter(|p| p.x == x)
                .map(|p| (p.y, p.z))
                .collect();
            values.sort_by(|a, b| a.0.total_cmp(&b.0));
            values
        };
        assert_eq!(horizon_edge(&a, 1.0), horizon_edge(&b, 0.0));
        assert_eq!(horizon_edge(&b, 1.0), horizon_edge(&a, 0.0));
        let path = std::env::temp_dir().join(format!(
            "motu-coastal-roundtrip-{}.motu",
            std::process::id()
        ));
        a.save(&path).unwrap();
        let restored = Island::load(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        for lod in 0..=2 {
            assert_eq!(restored.lod(lod), a.lod(lod), "cached coast LOD{lod}");
        }
        assert_eq!(restored.material, a.material);
        assert_eq!(restored.environment, a.environment);
        assert_eq!(restored.distance_to_land, a.distance_to_land);
        assert_eq!(restored.rivers, a.rivers);
        assert_eq!(restored.horizon_mesh(), a.horizon_mesh());
        assert_eq!(restored.height_map(33, 33), a.height_map(33, 33));
    }

    #[test]
    fn default_density_river_finishing_keeps_the_closing_render_partition() {
        let terrain = IslandOptions {
            water_ratio: 0.95,
            ..IslandOptions::default()
        };
        let generate = |section_index| {
            Island::generate_coastal_strip_with_cave_walks(
                42,
                terrain,
                CoastalStripOptions {
                    section_index,
                    section_count: 2,
                    seam_segments: 16,
                    ..CoastalStripOptions::default()
                },
                ForestOptions {
                    noise_threshold: 1.0,
                    prototype_count: 1,
                    ..ForestOptions::default()
                },
                ReedOptions {
                    coverage_threshold: 1.0,
                    ..ReedOptions::default()
                },
                FernOptions {
                    coverage_threshold: 1.0,
                    ..FernOptions::default()
                },
                CaveOptions::default(),
                CaveNetworkOptions::default(),
                CaveWalkOptions::default(),
                GenerationMethod::Cpu,
            )
            .unwrap()
        };
        let a = generate(0);
        let b = generate(1);
        for (island, index) in [(&a, 0), (&b, 1)] {
            let field = CoastalStripField {
                seed: 42,
                terrain,
                options: CoastalStripOptions {
                    section_index: index,
                    section_count: 2,
                    seam_segments: 16,
                    ..CoastalStripOptions::default()
                },
            };
            let (original, _) = field.initial();
            for &pin in original
                .vertices
                .iter()
                .filter(|&&p| CoastalStripField::is_pinned(p))
            {
                assert!(
                    island.terrain().vertices().contains(&pin),
                    "finished section {index} changed an original pinned sample: {pin:?}"
                );
            }
        }
        for lod in 0..=2 {
            let bounds = crate::BoundingBox::new(
                Vec3::new(0.0, 0.0, f32::MIN),
                Vec3::new(1.0, 1.0, f32::MAX),
            );
            let left = a.render_mesh_in(lod, bounds, 0).unwrap();
            let right = b.render_mesh_in(lod, bounds, 0).unwrap();
            let rendered_edge = |mesh, side| {
                let mut points = edge(mesh, side);
                // UV/normal seams may duplicate a render vertex at one exact
                // position. Distinct collinear boundary samples stay present.
                points.dedup_by(|a, b| a.0.distance(b.0) < 1.0e-6);
                points
            };
            for (a, b) in [
                (rendered_edge(&left, 1.0), rendered_edge(&right, 0.0)),
                (rendered_edge(&right, 1.0), rendered_edge(&left, 0.0)),
            ] {
                assert_eq!(
                    a.len(),
                    b.len(),
                    "default-density LOD{lod} seam partition: {a:?} vs {b:?}"
                );
                for ((pa, na), (pb, nb)) in a.into_iter().zip(b) {
                    assert!(
                        pa.distance(pb) < 1.0e-6,
                        "default-density LOD{lod} seam positions: {pa:?} vs {pb:?}"
                    );
                    assert!(
                        na.distance(nb) < 1.0e-5,
                        "default-density LOD{lod} seam normals"
                    );
                }
            }
        }
    }

    #[test]
    fn invalid_strip_options_and_gpu_fail_before_generation() {
        assert!(
            CoastalStripOptions {
                section_count: 0,
                ..CoastalStripOptions::default()
            }
            .validate(terrain_options())
            .is_err()
        );
        assert!(
            CoastalStripOptions {
                seam_blend_width: f32::NAN,
                ..CoastalStripOptions::default()
            }
            .validate(terrain_options())
            .is_err()
        );
        assert!(
            Island::generate_coastal_strip_with_cave_walks(
                42,
                terrain_options(),
                CoastalStripOptions::default(),
                ForestOptions::default(),
                ReedOptions::default(),
                FernOptions::default(),
                CaveOptions::default(),
                CaveNetworkOptions::default(),
                CaveWalkOptions::default(),
                GenerationMethod::Gpu
            )
            .unwrap_err()
            .contains("CPU")
        );
    }
}
