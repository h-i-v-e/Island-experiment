//! Fallen wood is owned separately from live trees, but shares their render batches.
use super::{
    FOREST_NOISE_DOMAIN, ForestMeshTile, ForestMeshes, ForestOptions, ForestSurface,
    ForestTrunkCollider, HashMap, ISLAND_WORLD_METRES, LOOSE_DEPTH_EPSILON, MINIMUM_NORMAL_Z, Mesh,
    MeshRange, TAU, Terrain, Vec2, Vec3, append_identity, append_range, encode_bark_axis, noise,
    shader_beach_candidate, stable_key, stable_unit,
};

const LOG_DOMAIN: u64 = 0x6661_6c6c_656e_6c67;
const CELL_METRES: f32 = 12.0;
const SIDES: [usize; 3] = [10, 6, 4];

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct FallenLog {
    pub(crate) bottom: Vec3,
    pub(crate) top: Vec3,
    pub(crate) radius: f32,
    pub(crate) ranges: [MeshRange; 3],
}

impl FallenLog {
    pub(crate) fn anchor(&self) -> Vec3 {
        (self.bottom + self.top) * 0.5
    }

    pub(crate) fn collider(&self) -> ForestTrunkCollider {
        let axis = (self.top - self.bottom).normalize();
        ForestTrunkCollider {
            bottom: self.bottom - axis * self.radius * 0.12,
            top: self.top + axis * self.radius * 0.12,
            owner: self.anchor().truncate(),
            radius: self.radius * 1.12,
        }
    }
}

// Put each obstacle in every cell touched by its footprint. Queries remain local
// even on islands with tens of thousands of trees; iteration order is deterministic.
#[derive(Clone, Copy)]
struct Obstacle {
    start: Vec2,
    end: Vec2,
    radius: f32,
}

#[derive(Default)]
struct Obstacles {
    cells: HashMap<(i32, i32), Vec<Obstacle>>,
}

fn cells(a: Vec2, b: Vec2, radius: f32) -> impl Iterator<Item = (i32, i32)> {
    let scale = ISLAND_WORLD_METRES / CELL_METRES;
    let min = ((a.min(b) - Vec2::splat(radius)) * scale)
        .floor()
        .as_ivec2();
    let max = ((a.max(b) + Vec2::splat(radius)) * scale)
        .floor()
        .as_ivec2();
    (min.y..=max.y).flat_map(move |y| (min.x..=max.x).map(move |x| (x, y)))
}

impl Obstacles {
    fn insert(&mut self, a: Vec2, b: Vec2, radius: f32) {
        for key in cells(a, b, radius) {
            self.cells.entry(key).or_default().push(Obstacle {
                start: a,
                end: b,
                radius,
            });
        }
    }

    fn intersects(&self, a: Vec2, b: Vec2, radius: f32) -> bool {
        cells(a, b, radius).any(|key| {
            self.cells.get(&key).is_some_and(|obstacles| {
                obstacles.iter().any(|other| {
                    segment_distance_squared(a, b, other.start, other.end)
                        < (radius + other.radius).powi(2)
                })
            })
        })
    }
}

fn point_segment_distance_squared(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let axis = b - a;
    let t = ((p - a).dot(axis) / axis.length_squared().max(1.0e-16)).clamp(0.0, 1.0);
    p.distance_squared(a + axis * t)
}

fn segment_distance_squared(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> f32 {
    let ab = b - a;
    let cd = d - c;
    let denominator = ab.perp_dot(cd);
    if denominator.abs() > 1.0e-16 {
        let along_ab = (c - a).perp_dot(cd) / denominator;
        let along_cd = (c - a).perp_dot(ab) / denominator;
        if (0.0..=1.0).contains(&along_ab) && (0.0..=1.0).contains(&along_cd) {
            return 0.0;
        }
    }
    point_segment_distance_squared(a, c, d)
        .min(point_segment_distance_squared(b, c, d))
        .min(point_segment_distance_squared(c, a, b))
        .min(point_segment_distance_squared(d, a, b))
}

pub(super) fn append_logs(
    forest: &mut ForestMeshes,
    seed: u64,
    terrain: &Terrain,
    surface: ForestSurface<'_>,
    options: ForestOptions,
    caves: &crate::caves::CaveSet,
) -> Result<(), String> {
    if options.fallen_log_density == 0.0 {
        return Ok(());
    }
    let mut obstacles = Obstacles::default();
    for collider in forest.trunk_colliders() {
        let collider = collider?;
        obstacles.insert(
            collider.bottom.truncate(),
            collider.top.truncate(),
            collider.radius,
        );
    }
    for tree_index in 0..forest.placements.len() {
        let tree = forest.placements[tree_index];
        let key = stable_key(seed, u64::from(tree.seed_ordinal), LOG_DOMAIN);
        if stable_unit(key) >= options.fallen_log_density {
            continue;
        }
        for attempt in 0..4 {
            let key = stable_key(key, attempt, LOG_DOMAIN);
            let random = |index| stable_unit(stable_key(key, index, LOG_DOMAIN));
            let yaw = random(0) * TAU;
            let offset_yaw = random(1) * TAU;
            let centre = tree.anchor.truncate()
                + Vec2::from_angle(offset_yaw) * ((3.0 + random(2) * 7.0) / ISLAND_WORLD_METRES);
            let half_axis =
                Vec2::from_angle(yaw) * ((3.0 + random(3) * 5.0) * 0.5 / ISLAND_WORLD_METRES);
            let radius = (0.18 + random(4) * 0.27) / ISLAND_WORLD_METRES;
            let a = centre - half_axis;
            let b = centre + half_axis;
            if obstacles.intersects(a, b, radius * 1.12 + 0.2 / ISLAND_WORLD_METRES) {
                continue;
            }
            let Some((bottom, top)) = fit_log(a, b, radius, terrain, surface, seed, options, caves)
            else {
                continue;
            };
            let mut log = FallenLog {
                bottom,
                top,
                radius,
                ranges: [MeshRange::default(); 3],
            };
            for (lod, destination) in [
                &mut forest.lod0_wood,
                &mut forest.lod1_wood,
                &mut forest.lod2_wood,
            ]
            .into_iter()
            .enumerate()
            {
                let mesh = log_mesh(&log, SIDES[lod]);
                log.ranges[lod] = append_identity(destination, &mesh)?;
            }
            obstacles.insert(a, b, radius * 1.12);
            forest.logs.push(log);
            break;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn fit_log(
    a: Vec2,
    b: Vec2,
    radius: f32,
    terrain: &Terrain,
    surface: ForestSurface<'_>,
    seed: u64,
    options: ForestOptions,
    caves: &crate::caves::CaveSet,
) -> Option<(Vec3, Vec3)> {
    let mut heights = [0.0_f32; 9];
    for (i, height) in heights.iter_mut().enumerate() {
        let point = a.lerp(b, i as f32 / 8.0);
        let lateral = (b - a).normalize().perp() * radius;
        for across in [-1.0, 0.0, 1.0] {
            let point = point + lateral * across;
            if point.min_element() < 0.0 || point.max_element() > 1.0 {
                return None;
            }
            let support = terrain.sample_support(point);
            let position = support.position;
            if !position.is_finite()
                || position.z <= radius
                || position.z * ISLAND_WORLD_METRES >= options.snowline_metres
                || support.normal.z < MINIMUM_NORMAL_Z
                || caves.excludes(
                    position * ISLAND_WORLD_METRES,
                    radius * ISLAND_WORLD_METRES + 0.3,
                )
                || support.triangle.iter().any(|&v| {
                    surface.river_bed[v]
                        || surface.stones.binary_search(&(v as u32)).is_ok()
                        || surface.reeds.binary_search(&(v as u32)).is_ok()
                })
            {
                return None;
            }
            let scalar = |values: &[f32]| {
                support
                    .triangle
                    .iter()
                    .zip(support.weights)
                    .map(|(&v, weight)| values[v] * weight)
                    .sum::<f32>()
            };
            let depth = scalar(surface.deposited_depths);
            let coverage = noise::fractal(
                seed ^ FOREST_NOISE_DOMAIN,
                point.x * ISLAND_WORLD_METRES / options.patch_size_metres,
                point.y * ISLAND_WORLD_METRES / options.patch_size_metres,
                options.noise_octaves,
            )
            .mul_add(0.5, 0.5)
            .clamp(0.0, 1.0);
            if depth <= LOOSE_DEPTH_EPSILON
                || coverage <= options.noise_threshold
                || shader_beach_candidate(
                    position,
                    depth.clamp(0.0, 1.0),
                    scalar(surface.sea_proximity),
                )
            {
                return None;
            }
            if across == 0.0 {
                *height = position.z;
            }
        }
    }
    let first = heights[0];
    let last = heights[8];
    // Raise the rigid axis above the highest support, then reject hollows that
    // would leave the log floating. A little burial hides the polygonal underside.
    let lift = heights
        .iter()
        .enumerate()
        .map(|(i, &h)| h - (first + (last - first) * i as f32 / 8.0))
        .fold(0.0_f32, f32::max);
    if heights
        .iter()
        .enumerate()
        .any(|(i, &h)| first + (last - first) * i as f32 / 8.0 + lift - h > radius * 0.65)
    {
        return None;
    }
    let burial_offset = radius * 0.72;
    Some((
        a.extend(first + lift + burial_offset),
        b.extend(last + lift + burial_offset),
    ))
}

fn frame(log: &FallenLog) -> (Vec3, Vec3, Vec3) {
    let axis = (log.top - log.bottom).normalize();
    let right = axis.cross(Vec3::Z).normalize();
    (axis, right, axis.cross(right))
}

fn log_mesh(log: &FallenLog, sides: usize) -> Mesh {
    let (axis, right, up) = frame(log);
    let mut mesh = Mesh::default();
    let encoded_axis = encode_bark_axis(axis);
    for end in 0..2 {
        let centre = if end == 0 { log.bottom } else { log.top };
        for side in 0..sides {
            let angle = side as f32 / sides as f32 * TAU;
            let direction = right * angle.cos() + up * angle.sin();
            let radius =
                log.radius * (1.0 - end as f32 * 0.22) * (1.0 + 0.07 * (angle * 3.0 + 0.4).sin());
            let broken_offset = log.radius * 0.1 * (angle * 4.0 + end as f32).sin();
            mesh.vertices
                .push(centre + direction * radius + axis * broken_offset);
            mesh.normals.push(direction);
            mesh.uv.push(encoded_axis);
        }
    }
    for side in 0..sides {
        let next = (side + 1) % sides;
        mesh.triangles
            .extend([side, next, sides + side, next, sides + next, sides + side].map(|v| v as u32));
    }
    // Separate rim vertices prevent the cap's tag and normals bleeding into bark.
    for end in 0..2 {
        let start = mesh.vertices.len() as u32;
        mesh.vertices
            .push(if end == 0 { log.bottom } else { log.top });
        mesh.normals.push(axis * if end == 0 { -1.0 } else { 1.0 });
        mesh.uv.push(encoded_axis);
        for side in 0..sides {
            mesh.vertices.push(mesh.vertices[end * sides + side]);
            mesh.normals.push(axis * if end == 0 { -1.0 } else { 1.0 });
            mesh.uv.push(encoded_axis);
        }
        for side in 0..sides {
            let a = start + 1 + side as u32;
            let b = start + 1 + ((side + 1) % sides) as u32;
            mesh.triangles.extend(if end == 0 {
                [start, b, a]
            } else {
                [start, a, b]
            });
        }
    }
    mesh
}

pub(super) fn append_to_tile(
    tile: &mut ForestMeshTile,
    source: &Mesh,
    log: &FallenLog,
    lod: usize,
) -> Result<(), String> {
    let range = log.ranges[lod];
    let start = tile.mesh.vertices.len();
    append_range(&mut tile.mesh, source, range)?;
    let (_, right, up) = frame(log);
    let anchor = log.anchor();
    for (i, &vertex) in tile.mesh.vertices[start..].iter().enumerate() {
        let cap = i >= SIDES[lod] * 2;
        tile.material
            .push(anchor.extend(if cap { 0.0 } else { 0.25 }));
        let offset = vertex - anchor;
        tile.environment.push(if cap {
            Vec2::new(offset.dot(right), offset.dot(up)) / (2.0 * log.radius) + Vec2::splat(0.5)
        } else {
            Vec2::ZERO
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoundingBox, forest::ForestMeshKind};

    fn log() -> FallenLog {
        FallenLog {
            bottom: Vec3::new(0.499, 0.5, 0.02),
            top: Vec3::new(0.502, 0.5, 0.0205),
            radius: 0.0002,
            ranges: [MeshRange::default(); 3],
        }
    }

    #[test]
    fn caps_are_separate_outward_faces_and_axes_follow_the_log_at_every_lod() {
        let log = log();
        for sides in SIDES {
            let mesh = log_mesh(&log, sides);
            assert_eq!(mesh.vertices.len(), sides * 4 + 2);
            assert_eq!(mesh.uv.len(), mesh.vertices.len());
            for triangle in mesh.triangles.chunks_exact(3) {
                let [a, b, c] = [
                    triangle[0] as usize,
                    triangle[1] as usize,
                    triangle[2] as usize,
                ];
                let normal = (mesh.vertices[b] - mesh.vertices[a])
                    .cross(mesh.vertices[c] - mesh.vertices[a])
                    .normalize();
                assert!(
                    normal.dot(mesh.normals[a]) > 0.5,
                    "inward or degenerate face"
                );
                assert_eq!(
                    a < sides * 2,
                    b < sides * 2,
                    "cap/bark tag interpolates across a face"
                );
                assert_eq!(a < sides * 2, c < sides * 2);
            }
            let axis = (log.top - log.bottom).normalize();
            for uv in mesh.uv {
                assert!(crate::trees::decode_bark_axis(uv).dot(axis) > 0.999);
            }
        }
    }

    #[test]
    fn crossing_tile_boundaries_keeps_one_whole_log_and_matching_cap_attributes() {
        let mut forest = ForestMeshes::default();
        let mut log = log();
        for (lod, mesh) in [
            &mut forest.lod0_wood,
            &mut forest.lod1_wood,
            &mut forest.lod2_wood,
        ]
        .into_iter()
        .enumerate()
        {
            log.ranges[lod] = append_identity(mesh, &log_mesh(&log, SIDES[lod])).unwrap();
        }
        forest.logs.push(log);
        for (lod, sides) in SIDES.into_iter().enumerate() {
            let tiles = forest
                .mesh_grid(
                    ForestMeshKind::Wood,
                    lod,
                    BoundingBox::new(Vec3::ZERO, Vec3::ONE),
                    8,
                )
                .unwrap();
            assert_eq!(
                tiles
                    .iter()
                    .filter(|tile| !tile.mesh.vertices.is_empty())
                    .count(),
                1
            );
            let tile = &tiles[4 * 8 + 4];
            assert_eq!(tile.material.len(), sides * 4 + 2);
            assert_eq!(tile.environment.len(), tile.material.len());
            assert!(
                tile.material[..sides * 2]
                    .iter()
                    .all(|v| v.w.to_bits() == 0.25_f32.to_bits())
            );
            assert!(tile.material[sides * 2..].iter().all(|v| v.w == 0.0));
            assert!(tile.environment[sides * 2].distance(Vec2::splat(0.5)) < 0.0001);
        }
    }

    #[test]
    fn full_segments_clear_trunks_and_other_logs_even_when_centres_are_far_apart() {
        let mut obstacles = Obstacles::default();
        obstacles.insert(Vec2::new(0.1, 0.1), Vec2::new(0.11, 0.1), 0.0002);
        assert!(obstacles.intersects(Vec2::new(0.109, 0.09), Vec2::new(0.109, 0.11), 0.0002));
        assert!(!obstacles.intersects(Vec2::new(0.112, 0.09), Vec2::new(0.112, 0.11), 0.0002));
        let trunk = Vec2::new(0.13, 0.1);
        obstacles.insert(trunk, trunk, 0.0003);
        assert!(obstacles.intersects(Vec2::new(0.12, 0.1), Vec2::new(0.14, 0.1), 0.0002));
    }

    fn terrain(height: f32, slope: f32) -> Terrain {
        Terrain::new(Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, height),
                Vec3::new(1.0, 0.0, height + slope),
                Vec3::new(0.0, 1.0, height),
                Vec3::new(1.0, 1.0, height + slope),
            ],
            normals: vec![Vec3::new(-slope, 0.0, 1.0).normalize(); 4],
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        })
    }

    #[test]
    fn placement_is_repeatable_density_zero_disables_and_water_and_steep_ground_reject() {
        let surface = ForestSurface {
            river_bed: &[false; 4],
            stones: &[],
            reeds: &[],
            deposited_depths: &[1.0; 4],
            sea_proximity: &[0.0; 4],
        };
        let options = ForestOptions {
            fallen_log_density: 1.0,
            noise_threshold: 0.0,
            ..ForestOptions::default()
        };
        let caves = crate::caves::CaveSet::default();
        let generate = |options, terrain: &Terrain, surface| {
            let mut forest = ForestMeshes::default();
            forest.placements = (0..30)
                .map(|i| crate::forest::TreePlacement {
                    seed_ordinal: i,
                    terrain_vertex: 0,
                    anchor: Vec3::new(
                        0.45 + (i % 6) as f32 * 0.006,
                        0.45 + (i / 6) as f32 * 0.006,
                        0.02,
                    ),
                    yaw_radians: 0.0,
                    scale: 1.0,
                    prototype: 0,
                })
                .collect();
            append_logs(&mut forest, 77, terrain, surface, options, &caves).unwrap();
            forest
        };
        let flat = terrain(0.02, 0.0);
        let first = generate(options, &flat, surface);
        assert!(first.logs.len() >= 20);
        assert_eq!(first, generate(options, &flat, surface));
        let bytes = bincode::serialize(&first).unwrap();
        assert_eq!(first, bincode::deserialize::<ForestMeshes>(&bytes).unwrap());
        assert!(
            generate(
                ForestOptions {
                    fallen_log_density: 0.0,
                    ..options
                },
                &flat,
                surface
            )
            .logs
            .is_empty()
        );
        assert!(
            generate(options, &terrain(-0.01, 0.0), surface)
                .logs
                .is_empty()
        );
        assert!(
            generate(options, &terrain(0.01, 0.5), surface)
                .logs
                .is_empty()
        );
        assert!(
            generate(
                options,
                &flat,
                ForestSurface {
                    river_bed: &[true; 4],
                    ..surface
                }
            )
            .logs
            .is_empty()
        );
        for (index, log) in first.logs.iter().enumerate() {
            assert!((log.bottom.z - 0.02 - log.radius * 0.72).abs() < 1.0e-7);
            for other in &first.logs[..index] {
                assert!(
                    segment_distance_squared(
                        log.bottom.truncate(),
                        log.top.truncate(),
                        other.bottom.truncate(),
                        other.top.truncate()
                    ) >= (log.radius + other.radius).powi(2)
                );
            }
        }
    }
}
