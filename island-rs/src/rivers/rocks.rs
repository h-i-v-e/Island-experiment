use std::{collections::HashMap, sync::OnceLock};

use super::{ISLAND_WORLD_METRES, Mesh, Rng, Vec2, Vec3, is_river_bed_triangle, noise};
use crate::terrain::SettledRock;

const ROCK_SEED_SALT: u64 = 0x72a4_6f63_6b73_21d9;
const DENSITY_NOISE_SCALE_METRES: f32 = 5.0;
const DENSITY_NOISE_OCTAVES: u8 = 2;
const MINIMUM_DENSITY_ACCEPTANCE: f32 = 0.75;
const MAXIMUM_DENSITY_ACCEPTANCE: f32 = 1.0;
const SQUARE_METRES_PER_CANDIDATE: f32 = 0.35;
const STONE_MINIMUM_DIAMETER_METRES: f32 = 0.06;
const STONE_MAXIMUM_DIAMETER_METRES: f32 = 0.22;
const BOULDER_MINIMUM_DIAMETER_METRES: f32 = 0.28;
const BOULDER_MAXIMUM_DIAMETER_METRES: f32 = 0.65;
const BOULDER_FRACTION: f32 = 0.01;
const MINIMUM_SURFACE_NORMAL_Z: f32 = 0.906_307_8;
const PLACEMENT_CELL_METRES: f32 = 1.2;
const MINIMUM_GAP_METRES: f32 = 0.03;

#[derive(Clone, Copy)]
struct RockPlacement {
    position: Vec3,
    normal: Vec3,
    face_normal_z: f32,
    radius: f32,
    boulder: bool,
}

#[derive(Clone, Copy)]
struct PlacedFootprint {
    position: Vec2,
    radius: f32,
}

struct RockPrototype {
    vertices: Vec<Vec3>,
    triangles: Vec<u32>,
}

struct RiverRockGenerator<'a> {
    seed: u64,
    terrain: &'a Mesh,
    coverage: &'a [u8],
    rng: Rng,
    footprints: Vec<PlacedFootprint>,
    footprint_cells: HashMap<(i32, i32), Vec<usize>>,
    output: Mesh,
}

impl<'a> RiverRockGenerator<'a> {
    fn new(seed: u64, terrain: &'a Mesh, coverage: &'a [u8]) -> Self {
        Self {
            seed,
            terrain,
            coverage,
            rng: Rng::new(seed ^ ROCK_SEED_SALT),
            footprints: Vec::new(),
            footprint_cells: HashMap::new(),
            output: Mesh::default(),
        }
    }

    fn generate(mut self) -> Mesh {
        for triangle in self.terrain.triangles.chunks_exact(3) {
            if is_river_bed_triangle(triangle, self.coverage) {
                self.populate_triangle([triangle[0], triangle[1], triangle[2]]);
            }
        }
        self.output.calculate_normals();
        self.output
    }

    fn populate_triangle(&mut self, triangle: [u32; 3]) {
        let vertices = triangle.map(|index| self.terrain.vertices[index as usize]);
        let area_metres = projected_area(vertices) * ISLAND_WORLD_METRES * ISLAND_WORLD_METRES;
        let expected = area_metres / SQUARE_METRES_PER_CANDIDATE;
        let whole = expected.floor() as usize;
        let candidate_count = whole + usize::from(self.rng.unit() < expected.fract());
        for _ in 0..candidate_count {
            let placement = self.sample_placement(triangle, vertices);
            if self.accepts(placement) {
                self.record(placement);
            }
        }
    }

    fn sample_placement(&mut self, triangle: [u32; 3], vertices: [Vec3; 3]) -> RockPlacement {
        let root = self.rng.unit().sqrt();
        let split = self.rng.unit();
        let weights = [1.0 - root, root * (1.0 - split), root * split];
        let position =
            vertices[0] * weights[0] + vertices[1] * weights[1] + vertices[2] * weights[2];
        let face_normal = (vertices[1] - vertices[0])
            .cross(vertices[2] - vertices[0])
            .try_normalize()
            .unwrap_or(Vec3::Z);
        let vertex_normals = triangle.map(|index| {
            self.terrain
                .normals
                .get(index as usize)
                .copied()
                .unwrap_or(face_normal)
        });
        let normal = vertex_normals
            .into_iter()
            .zip(weights)
            .fold(Vec3::ZERO, |sum, (normal, weight)| sum + normal * weight)
            .try_normalize()
            .unwrap_or(face_normal);
        let (boulder, diameter_metres) = sample_rock_size(&mut self.rng);
        RockPlacement {
            position,
            normal,
            face_normal_z: face_normal.z,
            radius: diameter_metres * 0.5 / ISLAND_WORLD_METRES,
            boulder,
        }
    }

    fn accepts(&mut self, placement: RockPlacement) -> bool {
        if placement.position.z <= 0.0
            || placement.normal.z < MINIMUM_SURFACE_NORMAL_Z
            || placement.face_normal_z < MINIMUM_SURFACE_NORMAL_Z
        {
            return false;
        }
        let density_acceptance = density_acceptance(self.seed, placement.position.truncate());
        if self.rng.unit() > density_acceptance {
            return false;
        }
        let cell = footprint_cell(placement.position.truncate());
        for y in cell.1 - 1..=cell.1 + 1 {
            for x in cell.0 - 1..=cell.0 + 1 {
                let Some(indices) = self.footprint_cells.get(&(x, y)) else {
                    continue;
                };
                for &index in indices {
                    let existing = self.footprints[index];
                    let separation = placement.radius
                        + existing.radius
                        + MINIMUM_GAP_METRES / ISLAND_WORLD_METRES;
                    if placement
                        .position
                        .truncate()
                        .distance_squared(existing.position)
                        < separation * separation
                    {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn record(&mut self, placement: RockPlacement) {
        let footprint = PlacedFootprint {
            position: placement.position.truncate(),
            radius: placement.radius,
        };
        let index = self.footprints.len();
        self.footprints.push(footprint);
        self.footprint_cells
            .entry(footprint_cell(footprint.position))
            .or_default()
            .push(index);
        append_rock(&mut self.output, placement, &mut self.rng);
    }
}

pub(super) fn generate_river_rock_mesh(seed: u64, terrain: &Mesh, coverage: &[u8]) -> Mesh {
    RiverRockGenerator::new(seed, terrain, coverage).generate()
}

pub(crate) fn append_settled_rocks(seed: u64, rocks: &[SettledRock], output: &mut Mesh) {
    for rock in rocks {
        let mut rng = Rng::new(
            (u64::from(seed as u32) << 32) ^ u64::from(rock.appearance_id) ^ ROCK_SEED_SALT,
        );
        append_rock(
            output,
            RockPlacement {
                position: rock.anchor,
                normal: rock.normal,
                face_normal_z: rock.normal.z,
                radius: rock.radius,
                boulder: rock.boulder,
            },
            &mut rng,
        );
    }
    output.calculate_normals();
}

fn projected_area(vertices: [Vec3; 3]) -> f32 {
    let a = vertices[1].truncate() - vertices[0].truncate();
    let b = vertices[2].truncate() - vertices[0].truncate();
    a.perp_dot(b).abs() * 0.5
}

fn sample_rock_size(rng: &mut Rng) -> (bool, f32) {
    let boulder = rng.unit() < BOULDER_FRACTION;
    let amount = rng.unit().powi(3);
    let (minimum, maximum) = if boulder {
        (
            BOULDER_MINIMUM_DIAMETER_METRES,
            BOULDER_MAXIMUM_DIAMETER_METRES,
        )
    } else {
        (STONE_MINIMUM_DIAMETER_METRES, STONE_MAXIMUM_DIAMETER_METRES)
    };
    (boulder, (maximum - minimum).mul_add(amount, minimum))
}

fn density_acceptance(seed: u64, position: Vec2) -> f32 {
    let scale = ISLAND_WORLD_METRES / DENSITY_NOISE_SCALE_METRES;
    let noise = noise::fractal(
        seed ^ ROCK_SEED_SALT,
        position.x * scale,
        position.y * scale,
        DENSITY_NOISE_OCTAVES,
    );
    let normalized = noise.mul_add(0.5, 0.5).clamp(0.0, 1.0);
    (MAXIMUM_DENSITY_ACCEPTANCE - MINIMUM_DENSITY_ACCEPTANCE)
        .mul_add(normalized, MINIMUM_DENSITY_ACCEPTANCE)
}

fn footprint_cell(position: Vec2) -> (i32, i32) {
    let cell_size = PLACEMENT_CELL_METRES / ISLAND_WORLD_METRES;
    (
        (position.x / cell_size).floor() as i32,
        (position.y / cell_size).floor() as i32,
    )
}

fn append_rock(output: &mut Mesh, placement: RockPlacement, rng: &mut Rng) {
    let prototype = rock_prototype();
    let yaw = rng.range(0.0, std::f32::consts::TAU);
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let tangent = if placement.normal.z.abs() < 0.99 {
        Vec3::Z.cross(placement.normal).normalize()
    } else {
        Vec3::X
    };
    let bitangent = placement.normal.cross(tangent).normalize();
    let x_axis = tangent * cos_yaw + bitangent * sin_yaw;
    let y_axis = bitangent * cos_yaw - tangent * sin_yaw;
    let horizontal_x = placement.radius * rng.range(0.88, 1.16);
    let horizontal_y = placement.radius * rng.range(0.82, 1.12);
    let vertical = placement.radius
        * if placement.boulder {
            rng.range(0.68, 1.02)
        } else {
            rng.range(0.58, 0.92)
        };
    let deformation_seed = rng.next_u64();
    let first_vertex = output.vertices.len() as u32;
    let mut support = f32::MAX;
    for (index, direction) in prototype.vertices.iter().enumerate() {
        let variation = noise::fractal(
            deformation_seed.wrapping_add(index as u64),
            direction.x.mul_add(1.7, direction.z * 0.8),
            direction.y.mul_add(1.7, direction.z * 0.4),
            3,
        );
        let radius = variation.mul_add(0.14, 1.0).clamp(0.78, 1.22);
        let offset = x_axis * (direction.x * horizontal_x * radius)
            + y_axis * (direction.y * horizontal_y * radius)
            + placement.normal * (direction.z * vertical * radius);
        support = support.min(offset.dot(placement.normal));
        output.vertices.push(offset);
    }
    let embed = vertical
        * if placement.boulder {
            rng.range(0.55, 0.78)
        } else {
            rng.range(0.20, 0.48)
        };
    let centre = placement.position - placement.normal * (support + embed);
    for vertex in &mut output.vertices[first_vertex as usize..] {
        *vertex += centre;
    }
    output
        .triangles
        .extend(prototype.triangles.iter().map(|index| first_vertex + index));
}

fn rock_prototype() -> &'static RockPrototype {
    static PROTOTYPE: OnceLock<RockPrototype> = OnceLock::new();
    PROTOTYPE.get_or_init(icosahedron)
}

fn icosahedron() -> RockPrototype {
    let golden_ratio = (1.0 + 5.0_f32.sqrt()) * 0.5;
    let vertices = [
        Vec3::new(-1.0, golden_ratio, 0.0),
        Vec3::new(1.0, golden_ratio, 0.0),
        Vec3::new(-1.0, -golden_ratio, 0.0),
        Vec3::new(1.0, -golden_ratio, 0.0),
        Vec3::new(0.0, -1.0, golden_ratio),
        Vec3::new(0.0, 1.0, golden_ratio),
        Vec3::new(0.0, -1.0, -golden_ratio),
        Vec3::new(0.0, 1.0, -golden_ratio),
        Vec3::new(golden_ratio, 0.0, -1.0),
        Vec3::new(golden_ratio, 0.0, 1.0),
        Vec3::new(-golden_ratio, 0.0, -1.0),
        Vec3::new(-golden_ratio, 0.0, 1.0),
    ]
    .map(Vec3::normalize)
    .to_vec();
    let triangles = vec![
        0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7,
        1, 8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9,
        8, 1,
    ];
    RockPrototype {
        vertices,
        triangles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rock_prototype_is_closed_at_expected_detail() {
        let prototype = rock_prototype();
        assert_eq!(
            (prototype.vertices.len(), prototype.triangles.len() / 3),
            (12, 20)
        );
        let mut edge_uses = HashMap::<(u32, u32), usize>::new();
        for triangle in prototype.triangles.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                *edge_uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(edge_uses.values().all(|uses| *uses == 2));
    }

    #[test]
    fn projected_area_ignores_height() {
        let area = projected_area([
            Vec3::new(0.0, 0.0, -3.0),
            Vec3::new(2.0, 0.0, 4.0),
            Vec3::new(0.0, 3.0, 9.0),
        ]);
        assert!((area - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn density_field_varies_without_hard_empty_regions() {
        let (minimum, maximum) = (0..200)
            .map(|step| {
                density_acceptance(42, Vec2::new(step as f32 * 0.5 / ISLAND_WORLD_METRES, 0.25))
            })
            .fold((f32::MAX, f32::MIN), |(minimum, maximum), value| {
                (minimum.min(value), maximum.max(value))
            });

        assert!(minimum >= MINIMUM_DENSITY_ACCEPTANCE);
        assert!(maximum <= MAXIMUM_DENSITY_ACCEPTANCE);
        assert!(maximum - minimum > 0.1);
    }

    #[test]
    fn size_distribution_strongly_favours_small_stones() {
        let mut rng = Rng::new(42);
        let mut boulders = 0;
        let mut stones_below_midpoint = 0;
        let mut stones = 0;
        let stone_midpoint = (STONE_MINIMUM_DIAMETER_METRES + STONE_MAXIMUM_DIAMETER_METRES) * 0.5;
        for _ in 0..10_000 {
            let (boulder, diameter) = sample_rock_size(&mut rng);
            if boulder {
                boulders += 1;
            } else {
                stones += 1;
                stones_below_midpoint += usize::from(diameter < stone_midpoint);
            }
        }

        assert!((60..=140).contains(&boulders));
        assert!(stones_below_midpoint * 4 > stones * 3);
    }

    #[test]
    fn river_rock_mesh_is_deterministic_and_complete() {
        let size = 20.0 / ISLAND_WORLD_METRES;
        let height = 3.0 / ISLAND_WORLD_METRES;
        let terrain = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, height),
                Vec3::new(size, 0.0, height),
                Vec3::new(size, size, height),
                Vec3::new(0.0, size, height),
            ],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: Vec::new(),
        };
        let first = generate_river_rock_mesh(42, &terrain, &[1; 4]);
        let second = generate_river_rock_mesh(42, &terrain, &[1; 4]);

        assert_eq!(first, second);
        assert!(!first.triangles.is_empty());
        assert!(first.triangles.len() / (20 * 3) >= 100);
        assert_eq!(first.normals.len(), first.vertices.len());
        assert!(first.uv.is_empty());
        assert!(
            first
                .triangles
                .iter()
                .all(|index| *index < first.vertices.len() as u32)
        );
    }

    #[test]
    fn settled_rocks_are_appended_to_the_existing_mesh() {
        let existing = RockPlacement {
            position: Vec3::new(0.1, 0.1, 0.02),
            normal: Vec3::Z,
            face_normal_z: 1.0,
            radius: 0.1 / ISLAND_WORLD_METRES,
            boulder: false,
        };
        let mut output = Mesh::default();
        append_rock(&mut output, existing, &mut Rng::new(7));
        output.calculate_normals();
        let original_vertices = output.vertices.len();
        let original_triangles = output.triangles.len();
        let settled = [SettledRock {
            anchor: Vec3::new(0.2, 0.2, 0.03),
            normal: Vec3::Z,
            radius: 0.2 / ISLAND_WORLD_METRES,
            boulder: true,
            appearance_id: 11,
        }];

        append_settled_rocks(42, &settled, &mut output);

        let prototype = rock_prototype();
        assert_eq!(
            output.vertices.len(),
            original_vertices + prototype.vertices.len()
        );
        assert_eq!(
            output.triangles.len(),
            original_triangles + prototype.triangles.len()
        );
        assert_eq!(output.normals.len(), output.vertices.len());
        assert!(
            output.triangles[original_triangles..]
                .iter()
                .all(|index| *index >= original_vertices as u32)
        );
    }

    #[test]
    fn river_rocks_are_not_placed_below_sea_level() {
        let size = 20.0 / ISLAND_WORLD_METRES;
        let terrain = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, -0.001),
                Vec3::new(size, 0.0, -0.001),
                Vec3::new(size, size, -0.001),
                Vec3::new(0.0, size, -0.001),
            ],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: Vec::new(),
        };

        let rocks = generate_river_rock_mesh(42, &terrain, &[1; 4]);

        assert!(rocks.vertices.is_empty());
        assert!(rocks.triangles.is_empty());
    }

    #[test]
    fn river_rocks_reject_steep_vertex_and_face_normals() {
        let size = 20.0 / ISLAND_WORLD_METRES;
        let height = 3.0 / ISLAND_WORLD_METRES;
        let mut terrain = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, height),
                Vec3::new(size, 0.0, height),
                Vec3::new(size, size, height),
                Vec3::new(0.0, size, height),
            ],
            normals: vec![Vec3::new(0.8, 0.0, 0.6); 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: Vec::new(),
        };

        let steep_vertex_normals = generate_river_rock_mesh(42, &terrain, &[1; 4]);
        terrain.normals.fill(Vec3::Z);
        terrain.vertices[1].z += size;
        terrain.vertices[2].z += size;
        let steep_faces = generate_river_rock_mesh(42, &terrain, &[1; 4]);

        assert!(steep_vertex_normals.vertices.is_empty());
        assert!(steep_faces.vertices.is_empty());
    }
}
