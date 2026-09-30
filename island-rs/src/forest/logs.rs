//! Fallen wood is owned separately from live trees, but shares their render batches.
use super::{
    FOREST_NOISE_DOMAIN, ForestMeshTile, ForestMeshes, ForestOptions, ForestSurface,
    ForestTrunkCollider, HashMap, ISLAND_WORLD_METRES, LOOSE_DEPTH_EPSILON, MINIMUM_NORMAL_Z, Mesh,
    MeshRange, TAU, Terrain, Vec2, Vec3, append_identity, append_range, encode_bark_axis, noise,
    shader_beach_candidate, stable_key, stable_unit,
};

const LOG_DOMAIN: u64 = 0x6661_6c6c_656e_6c67;
const LOG_SPOKE_DOMAIN: u64 = 0x7370_6f6b_6573_21d1;
const CELL_METRES: f32 = 12.0;
const SIDES: [usize; 3] = [10, 6, 4];
/// Tube resolution for broken roots and branches. The distant log stays bare.
const SPOKE_SIDES: [usize; 3] = [6, 4, 0];
const MAX_SPOKES: usize = 12;
const MIN_SPOKE_LENGTH_METRES: f32 = 0.15;
/// Stationary bark. Living wood is 0.5 and sways; values under 0.3 do not.
const BARK_TAG: f32 = 0.25;
/// Flat dirt colour. Root tubes store this on the tip ring and bark on the
/// base ring, and the shader blends the interpolated value between them.
const ROTTEN_WOOD_TAG: f32 = 0.15;

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
                let mesh = log_mesh(
                    &log,
                    lod,
                    Some(SpokeClearance {
                        terrain,
                        obstacles: &obstacles,
                    }),
                );
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

#[derive(Clone, Copy)]
struct SpokeClearance<'a> {
    terrain: &'a Terrain,
    obstacles: &'a Obstacles,
}

#[derive(Clone, Copy, Debug)]
struct Spoke {
    origin: Vec3,
    direction: Vec3,
    length: f32,
    base_radius: f32,
    tip_radius: f32,
    rotten: bool,
}

impl Spoke {
    const ZERO: Self = Self {
        origin: Vec3::ZERO,
        direction: Vec3::ZERO,
        length: 0.0,
        base_radius: 0.0,
        tip_radius: 0.0,
        rotten: false,
    };
}

#[derive(Clone, Copy)]
struct SpokePlan {
    along: f32,
    angle: f32,
    length_metres: f32,
    base_fraction: f32,
    tip_fraction: f32,
    axial_weight: f32,
    radial_weight: f32,
    lift_weight: f32,
    /// Join the thick-end rim instead of the side bark.
    on_bottom_ring: bool,
}

fn frame(log: &FallenLog) -> (Vec3, Vec3, Vec3) {
    let axis = (log.top - log.bottom).normalize();
    let right = axis.cross(Vec3::Z).normalize();
    (axis, right, axis.cross(right))
}

/// Cross-section whose angle zero is horizontal and whose positive turn rises
/// toward world up. `frame`'s third axis is world-down for an +X log.
fn upper_basis(axis: Vec3) -> (Vec3, Vec3) {
    let projected = Vec3::Z - axis * axis.z;
    let up = if projected.length_squared() > 1.0e-8 {
        projected.normalize()
    } else if axis.x.abs() < 0.9 {
        axis.cross(Vec3::X).normalize()
    } else {
        axis.cross(Vec3::Y).normalize()
    };
    let side = axis.cross(up).normalize_or(Vec3::Y);
    (side, up)
}

fn spoke_basis(direction: Vec3) -> (Vec3, Vec3) {
    let reference = if direction.z.abs() < 0.85 {
        Vec3::Z
    } else {
        Vec3::X
    };
    let side = direction.cross(reference).normalize_or(Vec3::Y);
    (side, direction.cross(side))
}

fn log_variation_key(log: &FallenLog) -> u64 {
    [
        log.bottom.x,
        log.bottom.y,
        log.bottom.z,
        log.top.x,
        log.top.y,
        log.top.z,
        log.radius,
    ]
    .into_iter()
    .fold(LOG_SPOKE_DOMAIN, |key, value| {
        stable_key(key, u64::from(value.to_bits()), LOG_SPOKE_DOMAIN)
    })
}

fn aimed_direction(direction: Vec3, up: Vec3, hold_above: bool) -> Vec3 {
    let direction = direction.normalize_or(if hold_above { up } else { -up });
    if hold_above && direction.dot(up) < 0.12 {
        (direction + up).normalize_or(up)
    } else {
        direction
    }
}

fn laid_spoke(log: &FallenLog, axis: Vec3, side: Vec3, up: Vec3, plan: SpokePlan) -> Spoke {
    let (sin, cos) = plan.angle.sin_cos();
    let radial = side * cos + up * sin;
    let local_radius = log.radius * (1.0 - plan.along * 0.22);
    let base_radius = local_radius * plan.base_fraction;
    let direction = aimed_direction(
        radial * plan.radial_weight + axis * plan.axial_weight + up * plan.lift_weight,
        up,
        plan.lift_weight > 0.0,
    );
    // Slide the tube back along itself so the open base sits inside the wood.
    // Roots go in by another radius, which covers the hole at the top of the join.
    let embed = log.radius * 0.16 + base_radius * if plan.on_bottom_ring { 1.45 } else { 0.45 };
    let attach = if plan.on_bottom_ring {
        log.bottom + radial * log.radius * 0.92
    } else {
        log.bottom.lerp(log.top, plan.along) + radial * local_radius
    };
    Spoke {
        origin: attach - direction * embed,
        direction,
        length: plan.length_metres / ISLAND_WORLD_METRES,
        base_radius,
        tip_radius: base_radius * plan.tip_fraction,
        rotten: plan.on_bottom_ring,
    }
}

fn all_spokes(log: &FallenLog) -> (usize, [Spoke; MAX_SPOKES]) {
    let key = log_variation_key(log);
    let mut cursor = 0_u64;
    let mut unit = || {
        let value = stable_unit(stable_key(key, cursor, LOG_SPOKE_DOMAIN));
        cursor += 1;
        value
    };
    let root_count = 4 + (unit() * 4.0) as usize;
    let branch_count = 2 + (unit() * 4.0) as usize;
    debug_assert!(root_count + branch_count <= MAX_SPOKES);
    let axis = (log.top - log.bottom).normalize();
    let (side, up) = upper_basis(axis);
    let mut spokes = [Spoke::ZERO; MAX_SPOKES];
    for (index, spoke) in spokes.iter_mut().enumerate().take(root_count) {
        // Even slots around the thick-end rim. The root then grows out through
        // that face, so it does not leave the side bark like a branch.
        let slot = (index as f32 + 0.5 + (unit() - 0.5) * 0.45) / root_count as f32;
        *spoke = laid_spoke(
            log,
            axis,
            side,
            up,
            SpokePlan {
                along: 0.0,
                angle: slot * TAU,
                length_metres: 0.45 + unit() * 0.75,
                base_fraction: 0.34 + unit() * 0.18,
                tip_fraction: 0.42 + unit() * 0.12,
                axial_weight: -1.05,
                radial_weight: 0.62,
                lift_weight: 0.0,
                on_bottom_ring: true,
            },
        );
    }
    for (index, spoke) in spokes
        .iter_mut()
        .skip(root_count)
        .take(branch_count)
        .enumerate()
    {
        let along = 0.38 + (index as f32 + unit()) * 0.45 / branch_count as f32;
        let angle = TAU * (0.12 + (index as f32 + unit()) * 0.26 / branch_count as f32);
        *spoke = laid_spoke(
            log,
            axis,
            side,
            up,
            SpokePlan {
                along,
                angle,
                length_metres: 0.35 + unit() * 0.55,
                base_fraction: 0.10 + unit() * 0.10,
                tip_fraction: 0.42 + unit() * 0.12,
                axial_weight: 0.55,
                radial_weight: 0.62,
                lift_weight: 0.45,
                on_bottom_ring: false,
            },
        );
    }
    (root_count + branch_count, spokes)
}

fn spoke_is_clear(
    origin: Vec3,
    direction: Vec3,
    length: f32,
    radius: f32,
    clearance: SpokeClearance<'_>,
) -> bool {
    let tip = origin + direction * length;
    let xy = tip.truncate();
    if !(0.0..=1.0).contains(&xy.x) || !(0.0..=1.0).contains(&xy.y) {
        return false;
    }
    if clearance
        .obstacles
        .intersects(origin.truncate(), xy, radius)
    {
        return false;
    }
    // Roots aim into the soil. Shortening them against the ground would turn
    // the underside fan back into a row of nubs, so only rising spokes yield.
    if direction.z < 0.05 {
        return true;
    }
    let ground = clearance.terrain.sample_support(xy).position.z;
    tip.z >= ground + radius * 0.25
}

fn limit_spoke_length(
    origin: Vec3,
    direction: Vec3,
    length: f32,
    radius: f32,
    clearance: Option<SpokeClearance<'_>>,
) -> f32 {
    let Some(clearance) = clearance else {
        return length;
    };
    let minimum = MIN_SPOKE_LENGTH_METRES / ISLAND_WORLD_METRES;
    let mut length = length.max(minimum);
    for _ in 0..8 {
        if length <= minimum || spoke_is_clear(origin, direction, length, radius, clearance) {
            break;
        }
        length = (length * 0.62).max(minimum);
    }
    length
}

/// Near logs keep every stub. The middle lod keeps the thicker half, which is
/// mostly roots. Shortening changes reach only; the stub still exists.
fn select_spokes(
    log: &FallenLog,
    lod: usize,
    clearance: Option<SpokeClearance<'_>>,
) -> (usize, [Spoke; MAX_SPOKES]) {
    if SPOKE_SIDES[lod] == 0 {
        return (0, [Spoke::ZERO; MAX_SPOKES]);
    }
    let (count, spokes) = all_spokes(log);
    let mut order: [usize; MAX_SPOKES] = std::array::from_fn(|index| index);
    let keep = if lod == 0 {
        count
    } else {
        order[..count].sort_by(|&left, &right| {
            spokes[right]
                .base_radius
                .total_cmp(&spokes[left].base_radius)
                .then(left.cmp(&right))
        });
        count.div_ceil(2)
    };
    let mut selected = [Spoke::ZERO; MAX_SPOKES];
    for (slot, &index) in order.iter().take(keep).enumerate() {
        let mut spoke = spokes[index];
        spoke.length = limit_spoke_length(
            spoke.origin,
            spoke.direction,
            spoke.length,
            spoke.base_radius,
            clearance,
        );
        selected[slot] = spoke;
    }
    (keep, selected)
}

fn spoke_count(log: &FallenLog, lod: usize) -> usize {
    select_spokes(log, lod, None).0
}

fn bark_vertex_count(log: &FallenLog, lod: usize) -> usize {
    SIDES[lod] * 2 + spoke_count(log, lod) * SPOKE_SIDES[lod] * 2
}

fn log_mesh(log: &FallenLog, lod: usize, clearance: Option<SpokeClearance<'_>>) -> Mesh {
    let sides = SIDES[lod];
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
    // Bark (cylinder, then stub tubes) is a prefix. Every broken face comes
    // after it, so the end-grain tag can be rebuilt from the log alone.
    let (count, spokes) = select_spokes(log, lod, clearance);
    let spoke_sides = SPOKE_SIDES[lod];
    let mut tip_rings = [0_usize; MAX_SPOKES];
    for (index, spoke) in spokes.iter().take(count).enumerate() {
        tip_rings[index] = append_spoke_tube(&mut mesh, spoke, spoke_sides);
    }
    append_log_caps(&mut mesh, log, sides, axis, right, up);
    for (index, spoke) in spokes.iter().take(count).enumerate() {
        append_spoke_cap(&mut mesh, spoke, spoke_sides, tip_rings[index]);
    }
    mesh
}

fn append_spoke_tube(mesh: &mut Mesh, spoke: &Spoke, sides: usize) -> usize {
    let (side, binormal) = spoke_basis(spoke.direction);
    let phase = spoke.origin.dot(Vec3::new(40.0, 17.0, 9.0));
    let encoded = encode_bark_axis(spoke.direction);
    let base = mesh.vertices.len();
    for ring in 0..2 {
        let centre = spoke.origin + spoke.direction * spoke.length * ring as f32;
        let ring_radius = if ring == 0 {
            spoke.base_radius
        } else {
            spoke.tip_radius
        };
        for step in 0..sides {
            let angle = step as f32 / sides as f32 * TAU;
            let (sin, cos) = angle.sin_cos();
            let radial = side * cos + binormal * sin;
            let chip = if ring == 1 {
                spoke.direction * spoke.tip_radius * 0.16 * (angle * 3.0 + phase).sin()
            } else {
                Vec3::ZERO
            };
            let radius = ring_radius * (1.0 + 0.08 * (angle * 3.0 + phase).sin());
            mesh.vertices.push(centre + radial * radius + chip);
            mesh.normals.push(radial);
            mesh.uv.push(encoded);
        }
    }
    for step in 0..sides {
        let next = (step + 1) % sides;
        mesh.triangles.extend(
            [step, next, sides + step, next, sides + next, sides + step]
                .map(|vertex| (base + vertex) as u32),
        );
    }
    base + sides
}

fn append_log_caps(
    mesh: &mut Mesh,
    log: &FallenLog,
    sides: usize,
    axis: Vec3,
    right: Vec3,
    up: Vec3,
) {
    for end in 0..2 {
        let centre = if end == 0 { log.bottom } else { log.top };
        let normal = axis * if end == 0 { -1.0 } else { 1.0 };
        let start = mesh.vertices.len() as u32;
        mesh.vertices.push(centre);
        mesh.normals.push(normal);
        mesh.uv.push(Vec2::splat(0.5));
        for side in 0..sides {
            let vertex = mesh.vertices[end * sides + side];
            let offset = vertex - centre;
            mesh.vertices.push(vertex);
            mesh.normals.push(normal);
            mesh.uv.push(
                Vec2::new(offset.dot(right), offset.dot(up)) / (2.0 * log.radius)
                    + Vec2::splat(0.5),
            );
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
}

fn append_spoke_cap(mesh: &mut Mesh, spoke: &Spoke, sides: usize, tip_ring: usize) {
    let (side, binormal) = spoke_basis(spoke.direction);
    let centre = spoke.origin + spoke.direction * spoke.length;
    let start = mesh.vertices.len() as u32;
    mesh.vertices.push(centre);
    mesh.normals.push(spoke.direction);
    mesh.uv.push(Vec2::splat(0.5));
    for step in 0..sides {
        let vertex = mesh.vertices[tip_ring + step];
        let offset = vertex - centre;
        mesh.vertices.push(vertex);
        mesh.normals.push(spoke.direction);
        mesh.uv.push(
            Vec2::new(offset.dot(side), offset.dot(binormal)) / (2.0 * spoke.tip_radius)
                + Vec2::splat(0.5),
        );
    }
    for step in 0..sides {
        let a = start + 1 + step as u32;
        let b = start + 1 + ((step + 1) % sides) as u32;
        mesh.triangles.extend([start, a, b]);
    }
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
    let anchor = log.anchor();
    let bark_vertices = bark_vertex_count(log, lod);
    let (_, spokes) = select_spokes(log, lod, None);
    let log_bark = SIDES[lod] * 2;
    let spoke_bark = SPOKE_SIDES[lod] * 2;
    for (index, uv) in tile.mesh.uv[start..].iter().enumerate() {
        let rotten = if index >= bark_vertices {
            true
        } else if index >= log_bark {
            let local = index - log_bark;
            let spoke = spokes[local / spoke_bark];
            spoke.rotten && local % spoke_bark >= spoke_bark / 2
        } else {
            false
        };
        let tag = if rotten { ROTTEN_WOOD_TAG } else { BARK_TAG };
        tile.material.push(anchor.extend(tag));
        tile.environment.push(if index >= bark_vertices {
            *uv
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

    fn mesh_vertex_count(lod: usize, spokes: usize) -> usize {
        SIDES[lod] * 4 + 2 + spokes * (SPOKE_SIDES[lod] * 3 + 1)
    }

    fn assert_separated_outward_faces(mesh: &Mesh, bark: usize) {
        assert_eq!(mesh.triangles.len() % 3, 0);
        for triangle in mesh.triangles.chunks_exact(3) {
            let [a, b, c] = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            assert!(a < mesh.vertices.len() && b < mesh.vertices.len() && c < mesh.vertices.len());
            let normal = (mesh.vertices[b] - mesh.vertices[a])
                .cross(mesh.vertices[c] - mesh.vertices[a])
                .normalize_or(Vec3::ZERO);
            assert!(
                normal.dot(mesh.normals[a]) > 0.5,
                "inward or degenerate face at {triangle:?}"
            );
            assert_eq!(
                a < bark,
                b < bark,
                "cap/bark tag interpolates across a face"
            );
            assert_eq!(a < bark, c < bark);
        }
    }

    #[test]
    fn caps_are_separate_outward_faces_and_axes_follow_the_log_at_every_lod() {
        let log = log();
        let axis = (log.top - log.bottom).normalize();
        let (side, up) = upper_basis(axis);
        let (full_count, full) = select_spokes(&log, 0, None);
        assert!((6..=MAX_SPOKES).contains(&full_count));
        assert_eq!(spoke_count(&log, 1), full_count.div_ceil(2));
        assert_eq!(spoke_count(&log, 2), 0);
        let (kept_count, kept) = select_spokes(&log, 1, None);
        let thickest_dropped = full[..full_count]
            .iter()
            .map(|spoke| spoke.base_radius)
            .filter(|radius| {
                kept[..kept_count]
                    .iter()
                    .all(|spoke| (spoke.base_radius - radius).abs() > 1.0e-8)
            })
            .fold(0.0_f32, f32::max);
        let thinnest_kept = kept[..kept_count]
            .iter()
            .map(|spoke| spoke.base_radius)
            .fold(f32::MAX, f32::min);
        assert!(thinnest_kept + 1.0e-6 >= thickest_dropped);
        for lod in 0..3 {
            let mesh = log_mesh(&log, lod, None);
            let sides = SIDES[lod];
            let spokes = spoke_count(&log, lod);
            let bark = bark_vertex_count(&log, lod);
            assert_eq!(mesh.vertices.len(), mesh_vertex_count(lod, spokes));
            assert_eq!(mesh.uv.len(), mesh.vertices.len());
            assert_eq!(mesh.normals.len(), mesh.vertices.len());
            assert!(mesh.vertices.iter().all(|vertex| vertex.is_finite()));
            assert_separated_outward_faces(&mesh, bark);
            for uv in &mesh.uv[..sides * 2] {
                assert!(crate::trees::decode_bark_axis(*uv).dot(axis) > 0.999);
            }
            let (count, spoke_list) = select_spokes(&log, lod, None);
            let mut cursor = sides * 2;
            for spoke in &spoke_list[..count] {
                if spoke.direction.dot(axis) > 0.2 {
                    assert!(spoke.direction.dot(up) > 0.12);
                } else {
                    assert!(spoke.direction.dot(axis) < -0.55);
                }
                let end = cursor + SPOKE_SIDES[lod] * 2;
                for uv in &mesh.uv[cursor..end] {
                    assert!(crate::trees::decode_bark_axis(*uv).dot(spoke.direction) > 0.999);
                }
                cursor = end;
            }
            assert_eq!(cursor, bark);
            let mut max_radial = 0.0_f32;
            let mut past_end = 0.0_f32;
            for vertex in &mesh.vertices {
                let offset = *vertex - log.bottom;
                let along = offset.dot(axis);
                max_radial = max_radial.max((offset - axis * along).length());
                past_end = past_end.max(-along);
            }
            if lod == 2 {
                assert_eq!(mesh.vertices.len(), sides * 4 + 2);
                assert!(max_radial < log.radius * 1.2);
                assert!(past_end < log.radius * 0.2);
            } else {
                assert!(max_radial > log.radius * 1.15);
                assert!(
                    past_end > log.radius * 0.4,
                    "roots should project from the thick end, reach {past_end}"
                );
            }
        }
        let mut bearings = Vec::new();
        for spoke in &full[..full_count] {
            if spoke.direction.dot(axis) < -0.55 {
                let from_end = spoke.origin - log.bottom;
                let along = from_end.dot(axis);
                let ring = (from_end - axis * along).length();
                assert!(
                    (0.0..log.radius).contains(&along),
                    "root base is not seated in the thick end"
                );
                assert!(ring > log.radius * 0.45 && ring < log.radius);
                let radial =
                    (spoke.direction - axis * spoke.direction.dot(axis)).normalize_or(Vec3::ZERO);
                bearings.push(radial.dot(up).atan2(radial.dot(side)));
            } else {
                assert!(spoke.direction.dot(axis) > 0.2);
                assert!(spoke.direction.dot(up) > 0.12);
            }
        }
        bearings.sort_by(f32::total_cmp);
        assert!(bearings.len() >= 4);
        let mut max_gap = bearings
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .fold(0.0_f32, f32::max);
        max_gap = max_gap.max(bearings[0] + TAU - bearings[bearings.len() - 1]);
        assert!(
            max_gap < 2.7,
            "roots are not spread around the end ring, gap {max_gap}"
        );
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
            log.ranges[lod] = append_identity(mesh, &log_mesh(&log, lod, None)).unwrap();
        }
        forest.logs.push(log);
        for lod in 0..3 {
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
            let log = &forest.logs[0];
            let spokes = spoke_count(log, lod);
            let bark = bark_vertex_count(log, lod);
            assert_eq!(tile.material.len(), mesh_vertex_count(lod, spokes));
            assert_eq!(tile.material.len(), tile.mesh.vertices.len());
            assert_eq!(tile.environment.len(), tile.material.len());
            let log_bark = SIDES[lod] * 2;
            assert!(
                tile.material[..log_bark]
                    .iter()
                    .all(|value| value.w.to_bits() == BARK_TAG.to_bits())
            );
            let (count, spoke_list) = select_spokes(log, lod, None);
            let mut cursor = log_bark;
            for spoke in &spoke_list[..count] {
                let spoke_vertices = SPOKE_SIDES[lod] * 2;
                let base_end = cursor + SPOKE_SIDES[lod];
                let end = cursor + spoke_vertices;
                if spoke.rotten {
                    assert!(
                        tile.material[cursor..base_end]
                            .iter()
                            .all(|value| value.w.to_bits() == BARK_TAG.to_bits())
                    );
                    assert!(
                        tile.material[base_end..end]
                            .iter()
                            .all(|value| value.w.to_bits() == ROTTEN_WOOD_TAG.to_bits())
                    );
                } else {
                    assert!(
                        tile.material[cursor..end]
                            .iter()
                            .all(|value| value.w.to_bits() == BARK_TAG.to_bits())
                    );
                }
                cursor = end;
            }
            assert_eq!(cursor, bark);
            assert!(
                tile.material[bark..]
                    .iter()
                    .all(|value| value.w.to_bits() == ROTTEN_WOOD_TAG.to_bits())
            );
            assert!(tile.environment[..bark].iter().all(|uv| *uv == Vec2::ZERO));
            assert!(tile.environment[bark].distance(Vec2::splat(0.5)) < 0.0001);
        }
    }

    #[test]
    fn spoke_length_stops_at_a_trunk_or_the_island_edge_and_roots_enter_soil() {
        let ground = terrain(0.02, 0.0);
        let mut obstacles = Obstacles::default();
        let trunk = Vec2::new(0.51, 0.5);
        obstacles.insert(trunk, trunk, 0.0004);
        let clearance = Some(SpokeClearance {
            terrain: &ground,
            obstacles: &obstacles,
        });
        let origin = Vec3::new(0.5, 0.5, 0.03);
        let radius = 0.0002;
        let minimum = MIN_SPOKE_LENGTH_METRES / ISLAND_WORLD_METRES;
        assert_eq!(
            limit_spoke_length(origin, Vec3::X, 0.02, radius, None).to_bits(),
            0.02_f32.to_bits()
        );
        let blocked = limit_spoke_length(origin, Vec3::X, 0.02, radius, clearance);
        assert!(blocked < 0.012, "{blocked}");
        assert!(blocked >= minimum);
        assert!(!obstacles.intersects(
            origin.truncate(),
            origin.truncate() + Vec2::X * blocked,
            radius
        ));
        let clear = limit_spoke_length(origin, -Vec3::X, 0.004, radius, clearance);
        assert!((clear - 0.004).abs() < 1.0e-6);
        let buried = limit_spoke_length(origin, -Vec3::Z, 0.02, radius, clearance);
        assert!(
            (buried - 0.02).abs() < 1.0e-6,
            "downward roots stay in the soil, got {buried}"
        );
        let edge = limit_spoke_length(
            Vec3::new(0.999, 0.5, 0.03),
            Vec3::X,
            0.02,
            radius,
            clearance,
        );
        assert!(edge < 0.002, "{edge}");
        assert!(edge >= minimum);
        assert!(0.999 + edge <= 1.0);
    }

    #[test]
    fn clearance_shortens_spokes_without_changing_topology() {
        let log = log();
        let bare = log_mesh(&log, 0, None);
        let ground = terrain(0.02, 0.0);
        let mut obstacles = Obstacles::default();
        obstacles.insert(Vec2::new(0.5005, 0.5), Vec2::new(0.5005, 0.5), 0.002);
        let blocked = log_mesh(
            &log,
            0,
            Some(SpokeClearance {
                terrain: &ground,
                obstacles: &obstacles,
            }),
        );
        assert_eq!(bare.vertices.len(), blocked.vertices.len());
        assert_eq!(bare.triangles.len(), blocked.triangles.len());
        assert_ne!(bare.vertices, blocked.vertices);
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
