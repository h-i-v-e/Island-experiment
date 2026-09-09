use super::{
    Cave, CaveOptions, CaveSet, Node, clearance, passage, portal::Portal, surface::SurfaceGrid,
    terrain_height,
};
use crate::{ISLAND_WORLD_METRES, Vec2, Vec3, rng::Rng, terrain::Terrain};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Copy)]
#[repr(C)]
pub struct CaveStats {
    pub examined: u32,
    pub approach_rejected: u32,
    pub face_rejected: u32,
    pub cover_rejected: u32,
    pub hazard_rejected: u32,
    pub spacing_rejected: u32,
    pub accepted: u32,
}

pub(crate) fn generate(
    seed: u64,
    terrain: &Terrain,
    options: CaveOptions,
    hazard: impl Fn(Vec2) -> bool,
) -> Result<CaveSet, String> {
    let mut result = CaveSet {
        options,
        ..CaveSet::default()
    };
    let mut rng = Rng::new(seed ^ u64::from(options.seed_offset) ^ 0x4341_5645_0000_0001);
    let min_normal = options.minimum_face_slope.to_radians().cos();
    let mesh = terrain.mesh();
    let mut candidates = Vec::new();
    // One deterministic bounded reservoir of actual steep faces, not a grid
    // whose spacing can miss narrow cliffs. Geometry IDs break random ties.
    let mut face_count = 0_u64;
    for (id, t) in mesh.triangles.chunks_exact(3).enumerate() {
        let a = mesh.vertices[t[0] as usize];
        let b = mesh.vertices[t[1] as usize];
        let c = mesh.vertices[t[2] as usize];
        let normal = (b - a).cross(c - a).normalize_or_zero();
        if normal.z < 0.0 || normal.z > min_normal {
            continue;
        }
        let p = (a + b + c) / 3.0 * ISLAND_WORLD_METRES;
        if p.z < options.sea_clearance {
            continue;
        }
        let candidate = (
            rng.next_u64(),
            id,
            p,
            -normal.truncate().normalize_or_zero(),
        );
        face_count += 1;
        if candidates.len() < options.candidate_limit as usize {
            candidates.push(candidate);
        } else {
            let slot = (rng.next_u64() % face_count) as usize;
            if slot < candidates.len() {
                candidates[slot] = candidate;
            }
        }
    }
    candidates.sort_by_key(|c| (c.0, c.1));
    let mut feet = BTreeSet::new();
    for (id, _, point, inward) in candidates {
        if result.caves.len() >= options.maximum_caves as usize {
            break;
        }
        result.stats.examined += 1;
        let Some(entrance) = find_foot(terrain, point, inward, &options) else {
            result.stats.approach_rejected += 1;
            continue;
        };
        let footkey = ((entrance.x / 2.0) as i32, (entrance.y / 2.0) as i32);
        if !feet.insert(footkey) {
            continue;
        }
        if result
            .caves
            .iter()
            .any(|c| c.entrance.distance(entrance) < options.spacing)
        {
            result.stats.spacing_rejected += 1;
            continue;
        }
        if hazard(entrance.truncate() / ISLAND_WORLD_METRES) {
            result.stats.hazard_rejected += 1;
            continue;
        }
        if !face_valid(terrain, entrance, inward, &options) {
            result.stats.face_rejected += 1;
            continue;
        }
        let Some(mut cave) = layout(id, terrain, entrance, inward, &options, &hazard)? else {
            result.stats.cover_rejected += 1;
            continue;
        };
        if result
            .caves
            .iter()
            .any(|c| c.minimum.cmple(cave.maximum).all() && cave.minimum.cmple(c.maximum).all())
        {
            result.stats.spacing_rejected += 1;
            continue;
        }
        cave.chunks = vec![passage::build(&cave, &options)];
        if !clearance::walkable(&cave, &options) {
            result.stats.cover_rejected += 1;
            continue;
        }
        result.caves.push(cave);
        result.stats.accepted += 1;
        result.validate()?;
    }
    Ok(result)
}

fn find_foot(terrain: &Terrain, p: Vec3, inward: Vec2, o: &CaveOptions) -> Option<Vec3> {
    for distance in 0..65 {
        let xy = p.truncate() - inward * (distance as f32 * 0.5);
        let height = terrain_height(terrain, xy);
        if height < o.sea_clearance {
            continue;
        }
        let entrance = xy.extend(height);
        if approach_valid(terrain, entrance, inward, o) {
            return Some(entrance);
        }
    }
    None
}

pub(crate) fn approach_valid(
    terrain: &Terrain,
    entrance: Vec3,
    inward: Vec2,
    o: &CaveOptions,
) -> bool {
    let right = Vec2::new(-inward.y, inward.x);
    let half = o.entrance_width * 0.5 + o.side_margin;
    let slope = o.approach_slope.to_radians().tan();
    // Half-metre samples cover both crossfall and the direction of travel.
    for y in 0..=(o.approach_length * 2.0).ceil() as i32 {
        for x in -(half * 2.0).ceil() as i32..=(half * 2.0).ceil() as i32 {
            let xy = entrance.truncate() - inward * (y as f32 * 0.5) + right * (x as f32 * 0.5);
            if !inside(xy, 2.0) {
                return false;
            }
            let h = terrain_height(terrain, xy);
            if h < o.sea_clearance
                || (h - entrance.z).abs()
                    > slope * xy.distance(entrance.truncate()) + o.approach_step
            {
                return false;
            }
            for axis in [inward, right] {
                if (terrain_height(terrain, xy + axis * 0.25)
                    - terrain_height(terrain, xy - axis * 0.25))
                .abs()
                    > slope * 0.5 + o.approach_step
                {
                    return false;
                }
            }
        }
    }
    connected_approach(terrain, entrance.truncate() - inward * o.approach_length, o)
}

fn connected_approach(terrain: &Terrain, start: Vec2, o: &CaveOptions) -> bool {
    let radius = o.route_radius.ceil() as i32;
    let mut visited = BTreeSet::from([(0, 0)]);
    let mut queue = VecDeque::from([(0, 0)]);
    while let Some((x, y)) = queue.pop_front() {
        if x * x + y * y >= radius * radius {
            return true;
        }
        let h = terrain_height(terrain, start + Vec2::new(x as f32, y as f32));
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = (x + dx, y + dy);
            if visited.contains(&next) {
                continue;
            }
            let p = start + Vec2::new(next.0 as f32, next.1 as f32);
            let nh = terrain_height(terrain, p);
            if inside(p, 2.0)
                && nh >= o.sea_clearance
                && (nh - h).abs() <= o.approach_slope.to_radians().tan() + o.approach_step
            {
                visited.insert(next);
                queue.push_back(next);
            }
        }
    }
    false
}

fn face_valid(terrain: &Terrain, p: Vec3, inward: Vec2, o: &CaveOptions) -> bool {
    let right = Vec2::new(-inward.y, inward.x);
    // A rounded cliff toe may occupy part of the entrance transition. The
    // source triangle already passed the steep-face test; require the full
    // opening's height by the end of the transition, where roof checks begin.
    let run = o.transition_length;
    (-4..=4).all(|x| {
        let side = right * (x as f32 / 4.0 * (o.entrance_width * 0.5 + o.side_margin));
        let top = terrain_height(terrain, p.truncate() + side + inward * run);
        top - p.z >= o.minimum_face_height
    })
}

pub(crate) fn layout(
    id: u64,
    terrain: &Terrain,
    entrance: Vec3,
    inward: Vec2,
    o: &CaveOptions,
    hazard: &impl Fn(Vec2) -> bool,
) -> Result<Option<Cave>, String> {
    let mut rng = Rng::new(id);
    let length = rng.range(o.length_min, o.length_max);
    let mut nodes = vec![
        Node {
            floor: entrance - inward.extend(0.0),
            width: o.entrance_width,
            height: o.entrance_height,
        },
        Node {
            floor: entrance + inward.extend(0.0) * o.transition_length,
            width: o.entrance_width,
            height: o.entrance_height,
        },
    ];
    let mut direction = inward;
    let mut travelled = o.transition_length;
    while travelled < length {
        let angle = rng.range(-0.12, 0.12);
        direction = Vec2::new(
            direction.x * angle.cos() - direction.y * angle.sin(),
            direction.x * angle.sin() + direction.y * angle.cos(),
        );
        let distance = (length - travelled).min(5.0);
        let last = *nodes.last().unwrap();
        let floor = last.floor + direction.extend(0.0) * distance;
        let node = Node {
            floor,
            width: rng.range(o.width_min, o.width_max),
            height: rng.range(o.height_min, o.height_max),
        };
        nodes.push(node);
        travelled += distance;
    }
    let end = nodes.last_mut().unwrap();
    end.width = o.chamber_width;
    end.height = o.chamber_height;
    // Validate the whole interpolated envelope, including lateral cover around
    // each section. The noise envelope is included before sampling any voxels.
    for pair in nodes.windows(2) {
        let count = (pair[0].floor.distance(pair[1].floor) / 0.5).ceil() as usize;
        for i in 0..=count {
            let t = i as f32 / count.max(1) as f32;
            let floor = pair[0].floor.lerp(pair[1].floor, t);
            let width = pair[0].width + (pair[1].width - pair[0].width) * t;
            let height = pair[0].height + (pair[1].height - pair[0].height) * t;
            let progress = (floor - entrance).truncate().dot(inward);
            if progress < o.transition_length {
                continue;
            }
            let radius = width * 0.5 + o.broad_amplitude + o.fine_amplitude + o.side_cover;
            if !inside(floor.truncate(), radius + 2.0)
                || hazard(floor.truncate() / ISLAND_WORLD_METRES)
            {
                return Ok(None);
            }
            for j in 0..16 {
                let angle = j as f32 * std::f32::consts::TAU / 16.0;
                let p = floor.truncate() + Vec2::new(angle.cos(), angle.sin()) * radius;
                // Skip the intended opening direction within the entrance transition.
                if (p - entrance.truncate()).dot(inward) < o.transition_length {
                    continue;
                }
                if terrain_height(terrain, p)
                    < floor.z + height + o.roof_cover + o.broad_amplitude + o.fine_amplitude
                {
                    return Ok(None);
                }
            }
            if terrain_height(terrain, floor.truncate()) < floor.z + height + o.roof_cover {
                return Ok(None);
            }
        }
    }
    let mut minimum = Vec2::splat(f32::INFINITY);
    let mut maximum = Vec2::splat(f32::NEG_INFINITY);
    for node in &nodes {
        let margin = Vec2::splat(node.width * 0.5 + o.broad_amplitude + o.fine_amplitude + 3.0);
        minimum = minimum.min(node.floor.truncate() - margin);
        maximum = maximum.max(node.floor.truncate() + margin);
    }
    if !inside(minimum, 1.0) || !inside(maximum, 1.0) {
        return Ok(None);
    }
    let surface = SurfaceGrid::new(terrain, minimum, maximum, o.voxel_size)?;
    let mut cave = Cave {
        id,
        entrance,
        inward,
        nodes,
        branches: Vec::new(),
        minimum: surface.minimum,
        maximum: surface.maximum(),
        surface,
        chunks: Vec::new(),
        portal: Portal::new(entrance, inward, o),
        entrance_collider: crate::Mesh::default(),
    };
    if !has_rock_cover(terrain, &cave, o) {
        return Ok(None);
    }
    cave.entrance_collider = cave.portal.collider(terrain.mesh());
    Ok(Some(cave))
}

fn has_rock_cover(terrain: &Terrain, cave: &Cave, o: &CaveOptions) -> bool {
    let _timer = crate::profiling::StageTimer::new("caves.rock_cover");
    let margin = Vec2::splat(o.roof_cover.max(o.side_cover));
    clearance::Triangles::new(
        [terrain.mesh()],
        cave.minimum - margin,
        cave.maximum + margin,
    )
    .rock_cover(cave, o)
}

fn inside(p: Vec2, margin: f32) -> bool {
    p.cmpgt(Vec2::splat(margin)).all() && p.cmplt(Vec2::splat(ISLAND_WORLD_METRES - margin)).all()
}
