use super::{
    GenerationMethod, ISLAND_WORLD_METRES, Mesh, River, Rng, StageTimer, Terrain, Vec2, Vec3,
    bin_coordinate, noise, sample_mesh_triangle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoration {
    Tree,
    Bush,
    Rock,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Decorations {
    pub(super) trees: Vec<Vec3>,
    pub(super) bushes: Vec<Vec3>,
    pub(super) stone_vertices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SettledRock {
    pub(crate) anchor: Vec3,
    pub(crate) normal: Vec3,
    pub(crate) radius: f32,
    pub(crate) boulder: bool,
    pub(crate) appearance_id: u32,
}

pub(super) const ROCK_BODY_COUNT_MULTIPLIER: usize = 7;
pub(super) const ROCK_DROP_MINIMUM_HEIGHT: f32 = 2.0 / ISLAND_WORLD_METRES;
pub(super) const ROCK_DROP_MAXIMUM_HEIGHT: f32 = 14.0 / ISLAND_WORLD_METRES;
pub(super) const ROCK_DROP_NOISE_SCALE: f32 = 72.0;
pub(super) const ROCK_DROP_NOISE_OCTAVES: u8 = 4;
pub(super) const ROCK_DROP_NOISE_CONTRAST: f32 = 0.65;
pub(super) const ROCK_DROP_NOISE_MINIMUM_DENSITY: f32 = 0.15;
pub(super) const ROCK_INITIAL_HORIZONTAL_SPEED: f32 = 1.5 / ISLAND_WORLD_METRES;
pub(super) const ROCK_GRAVITY: f32 = 9.81 / ISLAND_WORLD_METRES;
pub(super) const ROCK_SIMULATION_STEP: f32 = 1.0 / 60.0;
pub(super) const ROCK_SIMULATION_STEPS: usize = 360;
pub(super) const ROCK_CONTACT_DAMPING: f32 = 0.985;
pub(super) const ROCK_RESTITUTION: f32 = 0.04;
pub(super) const ROCK_SLEEP_SPEED: f32 = 0.06 / ISLAND_WORLD_METRES;
pub(super) const ROCK_WAKE_SPEED: f32 = 0.20 / ISLAND_WORLD_METRES;
pub(super) const ROCK_SLEEP_STEPS: u8 = 24;
pub(super) const ROCK_COLLISION_GRID_DIMENSION: usize = 256;
pub(super) const ROCK_MINIMUM_SETTLED_NORMAL_Z: f32 = 0.906_307_8;
pub(super) const ROCK_DROP_SOURCE_MAXIMUM_NORMAL_Z: f32 = std::f32::consts::FRAC_1_SQRT_2;
pub(super) const ROCK_APPEARANCE_DOMAIN: u64 = 0xd1b5_4a32_d192_ed03;
pub(super) const ROCK_DROP_NOISE_DOMAIN: u64 = 0x3c6e_f372_fe94_f82b;
pub(super) const ROCK_STONE_MINIMUM_DIAMETER_METRES: f32 = 0.10;
pub(super) const ROCK_STONE_MAXIMUM_DIAMETER_METRES: f32 = 0.30;
pub(super) const ROCK_BOULDER_MINIMUM_DIAMETER_METRES: f32 = 0.30;
pub(super) const ROCK_BOULDER_MAXIMUM_DIAMETER_METRES: f32 = 0.60;
pub(super) const RIVER_POINT_SAMPLE_SPACING: f32 = 2.0 / ISLAND_WORLD_METRES;
pub(super) struct RiverPointIndex {
    pub(super) dimension: usize,
    pub(super) offsets: Vec<usize>,
    pub(super) points: Vec<Vec2>,
}

impl RiverPointIndex {
    pub(super) fn new(rivers: &[River]) -> Self {
        let maximum_step = RIVER_POINT_SAMPLE_SPACING;
        let mut sampled_points = Vec::new();
        for river in rivers {
            for segment in river.nodes.windows(2) {
                let start = segment[0].position.truncate();
                let end = segment[1].position.truncate();
                let steps = ((end - start).length() / maximum_step).ceil().max(1.0) as usize;
                sampled_points
                    .extend((0..steps).map(|step| start.lerp(end, step as f32 / steps as f32)));
            }
            if let Some(node) = river.nodes.last() {
                sampled_points.push(node.position.truncate());
            }
        }
        let point_count = sampled_points.len();
        let dimension = ((point_count as f32 / 4.0).sqrt().ceil() as usize).clamp(8, 512);
        let mut counts = vec![0_usize; dimension * dimension];
        for &point in &sampled_points {
            let x = bin_coordinate(point.x, dimension);
            let y = bin_coordinate(point.y, dimension);
            counts[y * dimension + x] += 1;
        }
        let mut offsets = Vec::with_capacity(counts.len() + 1);
        offsets.push(0);
        for count in counts {
            offsets.push(offsets.last().copied().unwrap_or_default() + count);
        }
        let mut cursor = offsets[..dimension * dimension].to_vec();
        let mut points = vec![Vec2::ZERO; point_count];
        for point in sampled_points {
            let x = bin_coordinate(point.x, dimension);
            let y = bin_coordinate(point.y, dimension);
            let bin = y * dimension + x;
            points[cursor[bin]] = point;
            cursor[bin] += 1;
        }
        Self {
            dimension,
            offsets,
            points,
        }
    }

    pub(super) fn contains_within(&self, point: Vec2, distance_squared: f32) -> bool {
        let radius = distance_squared.sqrt();
        let cell_radius = (radius * self.dimension as f32).ceil() as usize;
        let origin_x = bin_coordinate(point.x, self.dimension);
        let origin_y = bin_coordinate(point.y, self.dimension);
        let minimum_x = origin_x.saturating_sub(cell_radius);
        let maximum_x = (origin_x + cell_radius).min(self.dimension - 1);
        let minimum_y = origin_y.saturating_sub(cell_radius);
        let maximum_y = (origin_y + cell_radius).min(self.dimension - 1);
        (minimum_y..=maximum_y).any(|y| {
            (minimum_x..=maximum_x).any(|x| {
                let bin = y * self.dimension + x;
                self.points[self.offsets[bin]..self.offsets[bin + 1]]
                    .iter()
                    .any(|river_point| river_point.distance_squared(point) < distance_squared)
            })
        })
    }
}

impl Decorations {
    #[must_use]
    pub fn trees(&self) -> &[Vec3] {
        &self.trees
    }

    #[must_use]
    pub fn bushes(&self) -> &[Vec3] {
        &self.bushes
    }

    pub(super) fn stone_vertices(&self) -> &[u32] {
        &self.stone_vertices
    }

    pub(super) fn set_tree_anchors(&mut self, anchors: impl IntoIterator<Item = Vec3>) {
        self.trees = anchors.into_iter().collect();
    }

    pub(super) fn generate(
        seed: u64,
        terrain: &Terrain,
        rivers: &[River],
        target: usize,
        method: GenerationMethod,
    ) -> Result<(Self, Vec<SettledRock>), String> {
        let _timer = StageTimer::new("decorations.lazy");
        let mut rng = Rng::new(seed ^ 0xe703_7ed1_a0b4_28db);
        let mut out = Self::default();
        out.trees.reserve(target * 3 / 5);
        out.bushes.reserve(target / 4);
        let vegetation_target = target * 15 / 16;
        let river_index = RiverPointIndex::new(rivers);
        for _ in 0..target * 6 {
            if out.trees.len() + out.bushes.len() >= vegetation_target {
                break;
            }
            let u = rng.range(0.01, 0.99);
            let v = rng.range(0.01, 0.99);
            let (height, normal) = terrain.sample_surface(u, v);
            if height <= 0.001 {
                continue;
            }
            let slope = 1.0 - normal.z;
            let point = Vec3::new(u, v, height);
            if river_index.contains_within(point.truncate(), 0.000_025) {
                continue;
            }
            let moisture = noise::fractal(seed ^ 0x8ebc_6af0_9c88_c6e3, u * 7.0, v * 7.0, 3);
            if slope > 0.38 || height > 0.145 {
                continue;
            }
            if moisture > -0.05 && height > 0.012 && height < 0.13 {
                if rng.unit() < 0.72 {
                    out.trees.push(point);
                }
            } else if rng.unit() < 0.54 {
                out.bushes.push(point);
            }
        }
        let (settled_rocks, stone_vertices) =
            generate_settled_rocks(seed, terrain, target * ROCK_BODY_COUNT_MULTIPLIER, method)?;
        out.stone_vertices = stone_vertices;
        Ok((out, settled_rocks))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RockBody {
    pub(super) centre: Vec3,
    pub(super) velocity: Vec3,
    pub(super) radius: f32,
    pub(super) appearance_id: u32,
    pub(super) supported: bool,
    pub(super) stable_support: bool,
    pub(super) quiet_steps: u8,
    pub(super) sleeping: bool,
}

pub(super) struct RockCollisionGrid {
    pub(super) dimension: usize,
    pub(super) counts: Vec<usize>,
    pub(super) offsets: Vec<usize>,
    pub(super) cursors: Vec<usize>,
    pub(super) body_indices: Vec<usize>,
}

impl RockCollisionGrid {
    pub(super) fn new(body_capacity: usize) -> Self {
        let dimension = ROCK_COLLISION_GRID_DIMENSION;
        Self {
            dimension,
            counts: vec![0; dimension * dimension],
            offsets: vec![0; dimension * dimension + 1],
            cursors: vec![0; dimension * dimension],
            body_indices: Vec::with_capacity(body_capacity),
        }
    }

    pub(super) fn rebuild(&mut self, bodies: &[RockBody]) {
        self.counts.fill(0);
        for body in bodies {
            let bin = self.bin(body.centre.truncate());
            self.counts[bin] += 1;
        }
        self.offsets[0] = 0;
        for (index, &count) in self.counts.iter().enumerate() {
            self.offsets[index + 1] = self.offsets[index] + count;
        }
        self.cursors
            .copy_from_slice(&self.offsets[..self.counts.len()]);
        self.body_indices.resize(bodies.len(), 0);
        for (index, body) in bodies.iter().enumerate() {
            let bin = self.bin(body.centre.truncate());
            self.body_indices[self.cursors[bin]] = index;
            self.cursors[bin] += 1;
        }
    }

    pub(super) fn bin(&self, point: Vec2) -> usize {
        let x = bin_coordinate(point.x, self.dimension);
        let y = bin_coordinate(point.y, self.dimension);
        y * self.dimension + x
    }
}

pub(super) fn generate_settled_rocks(
    seed: u64,
    terrain: &Terrain,
    body_target: usize,
    method: GenerationMethod,
) -> Result<(Vec<SettledRock>, Vec<u32>), String> {
    let _timer = StageTimer::new("decorations.rock_settling");
    let mut bodies = spawn_rock_bodies(seed, terrain, body_target);
    settle_rock_bodies(terrain, &mut bodies, method)?;

    let mut rocks = Vec::with_capacity(bodies.len());
    let mut stone_vertices = Vec::new();
    for body in bodies {
        if !body.supported || !body.centre.is_finite() {
            continue;
        }
        let point = body
            .centre
            .truncate()
            .clamp(Vec2::splat(0.01), Vec2::splat(0.99));
        let (terrain_height, normal) = terrain.sample_surface(point.x, point.y);
        if normal.z < ROCK_MINIMUM_SETTLED_NORMAL_Z {
            continue;
        }
        let Some((triangle, _)) =
            sample_mesh_triangle(&terrain.mesh, &terrain.triangle_index, point)
        else {
            continue;
        };
        let terrain_centre_height = terrain_height + body.radius / normal.z.max(0.2);
        let piled = body.centre.z > terrain_centre_height + body.radius * 0.35;
        stone_vertices.extend(triangle.map(|vertex| vertex as u32));
        let anchor_height = if piled {
            (body.centre.z - body.radius).max(terrain_height)
        } else {
            terrain_height
        };
        let anchor = point.extend(anchor_height);
        if !anchor.is_finite() {
            continue;
        }
        rocks.push(SettledRock {
            anchor,
            normal,
            radius: body.radius,
            boulder: rock_is_boulder(seed, body.appearance_id),
            appearance_id: body.appearance_id,
        });
    }
    stone_vertices.sort_unstable();
    stone_vertices.dedup();
    if method == GenerationMethod::Gpu && std::env::var_os("MOTU_GPU_ROCK_STATS").is_some() {
        eprintln!(
            "gpu-rock-output rocks={} stone_vertices={}",
            rocks.len(),
            stone_vertices.len(),
        );
    }
    Ok((rocks, stone_vertices))
}

#[cfg_attr(
    not(feature = "gpu-generation"),
    allow(
        clippy::unnecessary_wraps,
        reason = "the GPU implementation adds fallible device and readback work"
    )
)]
fn settle_rock_bodies(
    terrain: &Terrain,
    bodies: &mut [RockBody],
    method: GenerationMethod,
) -> Result<(), String> {
    if method == GenerationMethod::Gpu {
        #[cfg(feature = "gpu-generation")]
        {
            super::gpu_generation::simulate_rock_bodies_gpu(terrain, bodies)
                .map_err(|error| format!("GPU rock settling failed: {error}"))?;
            return Ok(());
        }
        #[cfg(not(feature = "gpu-generation"))]
        return method.require_available();
    }
    simulate_rock_bodies(terrain, bodies);
    Ok(())
}

pub(super) fn spawn_rock_bodies(seed: u64, terrain: &Terrain, target: usize) -> Vec<RockBody> {
    let mut rng = Rng::new(seed ^ 0x6a09_e667_f3bc_c909);
    let mut bodies = Vec::with_capacity(target);
    if target == 0 {
        return bodies;
    }
    let total_weight: f32 = terrain
        .mesh
        .triangles
        .chunks_exact(3)
        .map(|triangle| rock_drop_face_weight(seed, &terrain.mesh, triangle))
        .sum();
    if !total_weight.is_finite() || total_weight <= f32::EPSILON {
        return bodies;
    }

    let mut cumulative_target = rng.unit();
    let mut allocated = 0_usize;
    for triangle in terrain.mesh.triangles.chunks_exact(3) {
        let weight = rock_drop_face_weight(seed, &terrain.mesh, triangle);
        cumulative_target += target as f32 * weight / total_weight;
        let allocated_through_face = (cumulative_target.floor() as usize).min(target);
        let count = allocated_through_face.saturating_sub(allocated);
        allocated = allocated_through_face;
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
            .map(|vertex| terrain.mesh.vertices[vertex as usize]);
        for _ in 0..count {
            let surface = sample_triangle(a, b, c, &mut rng);
            let point = surface.truncate();
            if surface.z <= 0.001 || !surface.is_finite() || !inside_decoration_bounds(point) {
                continue;
            }
            let appearance_id = bodies.len() as u32;
            let radius = rock_collision_radius(seed, appearance_id);
            let drop_height = rng.range(ROCK_DROP_MINIMUM_HEIGHT, ROCK_DROP_MAXIMUM_HEIGHT);
            bodies.push(RockBody {
                centre: point.extend(
                    surface.z + radius + drop_height + rng.range(0.0, 1.0 / ISLAND_WORLD_METRES),
                ),
                velocity: Vec3::new(
                    rng.range(
                        -ROCK_INITIAL_HORIZONTAL_SPEED,
                        ROCK_INITIAL_HORIZONTAL_SPEED,
                    ),
                    rng.range(
                        -ROCK_INITIAL_HORIZONTAL_SPEED,
                        ROCK_INITIAL_HORIZONTAL_SPEED,
                    ),
                    0.0,
                ),
                radius,
                appearance_id,
                supported: false,
                stable_support: false,
                quiet_steps: 0,
                sleeping: false,
            });
        }
    }
    bodies
}

pub(super) fn rock_drop_face_weight(seed: u64, mesh: &Mesh, triangle: &[u32]) -> f32 {
    let [a, b, c] =
        [triangle[0], triangle[1], triangle[2]].map(|vertex| mesh.vertices[vertex as usize]);
    if a.z.min(b.z).min(c.z) <= 0.001 {
        return 0.0;
    }
    let area_normal = (b - a).cross(c - a);
    let doubled_area = area_normal.length();
    if !doubled_area.is_finite() || doubled_area <= f32::EPSILON {
        return 0.0;
    }
    let normal = area_normal / doubled_area;
    if !is_rock_drop_source(normal) {
        return 0.0;
    }
    let centroid = (a + b + c) / 3.0;
    let noise = noise::fractal(
        seed ^ ROCK_DROP_NOISE_DOMAIN,
        centroid.x * ROCK_DROP_NOISE_SCALE,
        centroid.y * ROCK_DROP_NOISE_SCALE,
        ROCK_DROP_NOISE_OCTAVES,
    );
    let density = rock_drop_noise_density(noise);
    doubled_area * 0.5 * density
}

pub(super) fn rock_drop_noise_density(noise: f32) -> f32 {
    let contrasted = noise.mul_add(ROCK_DROP_NOISE_CONTRAST, 0.5).clamp(0.0, 1.0);
    (1.0 - ROCK_DROP_NOISE_MINIMUM_DENSITY)
        .mul_add(contrasted.powi(2), ROCK_DROP_NOISE_MINIMUM_DENSITY)
}

pub(super) fn sample_triangle(a: Vec3, b: Vec3, c: Vec3, rng: &mut Rng) -> Vec3 {
    let radial = rng.unit().sqrt();
    let along = rng.unit();
    a * (1.0 - radial) + b * (radial * (1.0 - along)) + c * (radial * along)
}

pub(super) fn rock_collision_radius(seed: u64, appearance_id: u32) -> f32 {
    rock_appearance(seed, appearance_id).1
}

fn rock_is_boulder(seed: u64, appearance_id: u32) -> bool {
    rock_appearance(seed, appearance_id).0
}

fn rock_appearance(seed: u64, appearance_id: u32) -> (bool, f32) {
    let state = (u64::from(seed as u32) << 32) ^ u64::from(appearance_id) ^ ROCK_APPEARANCE_DOMAIN;
    let mut rng = Rng::new(state);
    let is_boulder = rng.unit() < 0.15;
    let _prototype = rng.unit();
    let diameter_metres = if is_boulder {
        rng.range(
            ROCK_BOULDER_MINIMUM_DIAMETER_METRES,
            ROCK_BOULDER_MAXIMUM_DIAMETER_METRES,
        )
    } else {
        rng.range(
            ROCK_STONE_MINIMUM_DIAMETER_METRES,
            ROCK_STONE_MAXIMUM_DIAMETER_METRES,
        )
    };
    (is_boulder, diameter_metres * 0.5 / ISLAND_WORLD_METRES)
}

pub(super) fn is_rock_drop_source(normal: Vec3) -> bool {
    normal.is_finite() && normal.z.abs() <= ROCK_DROP_SOURCE_MAXIMUM_NORMAL_Z
}

pub(super) fn inside_decoration_bounds(point: Vec2) -> bool {
    (0.01..=0.99).contains(&point.x) && (0.01..=0.99).contains(&point.y)
}

pub(super) fn simulate_rock_bodies(terrain: &Terrain, bodies: &mut [RockBody]) {
    let mut grid = RockCollisionGrid::new(bodies.len());
    for _ in 0..ROCK_SIMULATION_STEPS {
        if bodies.iter().all(|body| body.sleeping) {
            break;
        }
        for body in &mut *bodies {
            body.supported = false;
            body.stable_support = false;
            if body.sleeping {
                body.supported = true;
                body.stable_support = true;
                continue;
            }
            body.velocity.z -= ROCK_GRAVITY * ROCK_SIMULATION_STEP;
            body.velocity *= 0.999_5;
            body.centre += body.velocity * ROCK_SIMULATION_STEP;
            constrain_rock_to_island(body);
            resolve_rock_terrain_contact(terrain, body);
        }
        grid.rebuild(bodies);
        resolve_rock_body_contacts(bodies, &grid);
        for body in &mut *bodies {
            resolve_rock_terrain_contact(terrain, body);
            if body.stable_support && body.velocity.length_squared() <= ROCK_SLEEP_SPEED.powi(2) {
                body.quiet_steps = body.quiet_steps.saturating_add(1);
                if body.quiet_steps >= ROCK_SLEEP_STEPS {
                    body.velocity = Vec3::ZERO;
                    body.sleeping = true;
                }
            } else {
                body.quiet_steps = 0;
            }
        }
    }
}

pub(super) fn constrain_rock_to_island(body: &mut RockBody) {
    let minimum = 0.01 + body.radius;
    let maximum = 0.99 - body.radius;
    for axis in 0..2 {
        if body.centre[axis] < minimum {
            body.centre[axis] = minimum;
            body.velocity[axis] = body.velocity[axis].abs() * ROCK_RESTITUTION;
        } else if body.centre[axis] > maximum {
            body.centre[axis] = maximum;
            body.velocity[axis] = -body.velocity[axis].abs() * ROCK_RESTITUTION;
        }
    }
}

pub(super) fn resolve_rock_terrain_contact(terrain: &Terrain, body: &mut RockBody) {
    let (height, normal) = terrain.sample_surface(body.centre.x, body.centre.y);
    let contact_height = height + body.radius / normal.z.max(0.2);
    if body.centre.z > contact_height {
        return;
    }
    body.centre.z = contact_height;
    let normal_velocity = body.velocity.dot(normal);
    if normal_velocity < 0.0 {
        body.velocity -= normal * ((1.0 + ROCK_RESTITUTION) * normal_velocity);
    }
    let remaining_normal_velocity = body.velocity.dot(normal).max(0.0);
    let tangent_velocity = body.velocity - normal * body.velocity.dot(normal);
    body.velocity = normal * remaining_normal_velocity + tangent_velocity * ROCK_CONTACT_DAMPING;
    body.supported = true;
    body.stable_support = normal.z >= ROCK_MINIMUM_SETTLED_NORMAL_Z;
}

pub(super) fn resolve_rock_body_contacts(bodies: &mut [RockBody], grid: &RockCollisionGrid) {
    for first in 0..bodies.len() {
        let point = bodies[first].centre.truncate();
        let origin_x = bin_coordinate(point.x, grid.dimension);
        let origin_y = bin_coordinate(point.y, grid.dimension);
        let minimum_x = origin_x.saturating_sub(1);
        let maximum_x = (origin_x + 1).min(grid.dimension - 1);
        let minimum_y = origin_y.saturating_sub(1);
        let maximum_y = (origin_y + 1).min(grid.dimension - 1);
        for y in minimum_y..=maximum_y {
            for x in minimum_x..=maximum_x {
                let bin = y * grid.dimension + x;
                for &second in &grid.body_indices[grid.offsets[bin]..grid.offsets[bin + 1]] {
                    if second <= first {
                        continue;
                    }
                    let (left, right) = bodies.split_at_mut(second);
                    resolve_rock_pair(&mut left[first], &mut right[0]);
                }
            }
        }
    }
}

pub(super) fn resolve_rock_pair(first: &mut RockBody, second: &mut RockBody) {
    let offset = second.centre - first.centre;
    let minimum_distance = first.radius + second.radius;
    let distance_squared = offset.length_squared();
    if distance_squared >= minimum_distance * minimum_distance {
        return;
    }
    let distance = distance_squared.sqrt();
    let normal = if distance > f32::EPSILON {
        offset / distance
    } else if first.appearance_id <= second.appearance_id {
        Vec3::X
    } else {
        -Vec3::X
    };
    let relative_normal_velocity = (second.velocity - first.velocity).dot(normal);
    if relative_normal_velocity < -ROCK_WAKE_SPEED {
        first.sleeping = false;
        second.sleeping = false;
    }
    let first_inverse_mass = (!first.sleeping).then(|| 1.0 / first.radius.powi(3));
    let second_inverse_mass = (!second.sleeping).then(|| 1.0 / second.radius.powi(3));
    let inverse_mass_sum = first_inverse_mass.unwrap_or(0.0) + second_inverse_mass.unwrap_or(0.0);
    if inverse_mass_sum <= f32::EPSILON {
        return;
    }
    let overlap = minimum_distance - distance;
    if let Some(inverse_mass) = first_inverse_mass {
        first.centre -= normal * (overlap * inverse_mass / inverse_mass_sum);
    }
    if let Some(inverse_mass) = second_inverse_mass {
        second.centre += normal * (overlap * inverse_mass / inverse_mass_sum);
    }
    if relative_normal_velocity < 0.0 {
        let impulse = -(1.0 + ROCK_RESTITUTION) * relative_normal_velocity / inverse_mass_sum;
        if let Some(inverse_mass) = first_inverse_mass {
            first.velocity -= normal * (impulse * inverse_mass);
        }
        if let Some(inverse_mass) = second_inverse_mass {
            second.velocity += normal * (impulse * inverse_mass);
        }
    }
    if normal.z > 0.35 {
        second.supported = true;
        second.stable_support |= first.stable_support;
    } else if normal.z < -0.35 {
        first.supported = true;
        first.stable_support |= second.stable_support;
    }
}

#[cfg(test)]
mod decoration_tests {
    use super::super::{Island, IslandOptions};
    use super::*;

    #[test]
    pub(super) fn drop_sources_are_steep_faces() {
        assert!(is_rock_drop_source(Vec3::new(0.8, 0.0, 0.6)));
        assert!(is_rock_drop_source(Vec3::X));
        assert!(!is_rock_drop_source(Vec3::new(0.6, 0.0, 0.8)));
        assert!(!is_rock_drop_source(Vec3::Z));
        assert!(!is_rock_drop_source(Vec3::splat(f32::NAN)));
    }

    #[test]
    pub(super) fn steep_face_weight_uses_surface_area_and_noise() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.2, 0.2, 0.1),
                Vec3::new(0.8, 0.2, 0.1),
                Vec3::new(0.2, 0.3, 0.7),
            ],
            normals: Vec::new(),
            triangles: vec![0, 1, 2],
            uv: Vec::new(),
        };

        assert!(rock_drop_face_weight(2018, &mesh, &mesh.triangles) > 0.0);
    }

    #[test]
    pub(super) fn drop_noise_keeps_low_regions_eligible_and_strengthens_dense_regions() {
        let low = rock_drop_noise_density(-1.0);
        let middle = rock_drop_noise_density(0.0);
        let high = rock_drop_noise_density(1.0);

        assert!((low - 0.15).abs() < f32::EPSILON);
        assert!(low < middle && middle < high);
        assert!((high - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    pub(super) fn triangle_sampling_stays_inside_the_source_face() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        let mut rng = crate::rng::Rng::new(2018);

        for _ in 0..64 {
            let point = sample_triangle(a, b, c, &mut rng);
            assert!(point.x >= 0.0 && point.y >= 0.0 && point.x + point.y <= 1.0);
        }
    }

    #[test]
    pub(super) fn collision_radius_matches_the_unity_size_ranges_deterministically() {
        let first = rock_collision_radius(2018, 7);
        let second = rock_collision_radius(2018, 7);
        assert_eq!(first.to_bits(), second.to_bits());
        assert!(
            (0.05 / super::ISLAND_WORLD_METRES..=0.30 / super::ISLAND_WORLD_METRES)
                .contains(&first)
        );
    }

    #[test]
    pub(super) fn terrain_contact_lifts_a_falling_body_onto_the_surface() {
        let terrain = Terrain::new(Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.1),
                Vec3::new(1.0, 0.0, 0.1),
                Vec3::new(0.0, 1.0, 0.1),
            ],
            normals: vec![Vec3::Z; 3],
            triangles: vec![0, 1, 2],
            uv: Vec::new(),
        });
        let mut body = body(0, Vec3::new(0.25, 0.25, 0.05), 0.01);
        body.velocity.z = -0.1;

        resolve_rock_terrain_contact(&terrain, &mut body);

        assert!((body.centre.z - 0.11).abs() < 1.0e-6);
        assert!(body.supported);
        assert!(body.velocity.z >= 0.0);
    }

    #[test]
    pub(super) fn body_contact_separates_overlapping_rocks_and_supports_the_upper_one() {
        let mut lower = body(0, Vec3::new(0.5, 0.5, 0.1), 0.01);
        lower.sleeping = true;
        let mut upper = body(1, Vec3::new(0.5, 0.5, 0.115), 0.01);
        upper.velocity.z = -0.01;

        resolve_rock_pair(&mut lower, &mut upper);

        assert!(lower.centre.distance(upper.centre) >= 0.02 - 1.0e-6);
        assert!(upper.supported);
    }

    #[test]
    pub(super) fn steep_terrain_contact_does_not_become_stable_support() {
        let normal = Vec3::new(
            (1.0 - ROCK_MINIMUM_SETTLED_NORMAL_Z.powi(2)).sqrt(),
            0.0,
            ROCK_MINIMUM_SETTLED_NORMAL_Z - 0.01,
        )
        .normalize();
        let terrain = Terrain::new(Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.1),
                Vec3::new(1.0, 0.0, 0.1),
                Vec3::new(0.0, 1.0, 0.1),
            ],
            normals: vec![normal; 3],
            triangles: vec![0, 1, 2],
            uv: Vec::new(),
        });
        let mut body = body(0, Vec3::new(0.25, 0.25, 0.05), 0.01);

        resolve_rock_terrain_contact(&terrain, &mut body);

        assert!(body.supported);
        assert!(!body.stable_support);
    }

    #[test]
    pub(super) fn generated_settlements_export_stones_without_clearing_vertex_cover() {
        let island = Island::generate(
            2018,
            IslandOptions {
                terrain_size: 65,
                ..IslandOptions::default()
            },
        )
        .unwrap();
        let stones = island.decorations().stone_vertices();

        assert!(!stones.is_empty());
        assert!(
            stones
                .iter()
                .all(|&vertex| island.environment.values[vertex as usize].y > 0.99)
        );
        assert!(
            stones
                .iter()
                .any(|&vertex| island.material.values[vertex as usize].y > 0.0)
        );
    }

    pub(super) fn body(appearance_id: u32, centre: Vec3, radius: f32) -> RockBody {
        RockBody {
            centre,
            velocity: Vec3::ZERO,
            radius,
            appearance_id,
            supported: false,
            stable_support: false,
            quiet_steps: 0,
            sleeping: false,
        }
    }
}
