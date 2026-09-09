//! Persistent random walks with independent branching and probabilistic endings.
//! Ending probability doubles with each branch generation; lengths are not targets.
//! Exceptional resource exhaustion is an error.
use super::{
    Cave, CaveBranch, CaveOptions, Node, clearance, surface::SurfaceGrid, terrain_height, volume,
};
use crate::{ISLAND_WORLD_METRES, Vec2, rng::Rng, terrain::Terrain};
use serde::{Deserialize, Serialize};

// Serialization/work watchdog, not a shape target. Never silently truncate here.
pub(crate) const MAX_STEPS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct CaveWalkOptions {
    pub enabled: u32,
    pub end_probability: f32,
    pub branch_probability: f32,
    pub step_metres: f32,
    pub turn_degrees: f32,
    pub width_variation: f32,
}
impl Default for CaveWalkOptions {
    fn default() -> Self {
        Self {
            enabled: 0,
            end_probability: 0.09,
            branch_probability: 0.25,
            step_metres: 4.0,
            turn_degrees: 55.0,
            width_variation: 0.35,
        }
    }
}
impl CaveWalkOptions {
    /// # Errors
    /// Probabilities must be in range and all dimensions must be finite.
    pub fn validate(self) -> Result<Self, String> {
        if self.enabled > 1
            || ![
                self.end_probability,
                self.branch_probability,
                self.step_metres,
                self.turn_degrees,
                self.width_variation,
            ]
            .into_iter()
            .all(f32::is_finite)
            || !(0.001..=1.0).contains(&self.end_probability)
            || !(0.0..=1.0).contains(&self.branch_probability)
            || !(2.0..=8.0).contains(&self.step_metres)
            || !(0.0..=100.0).contains(&self.turn_degrees)
            || !(0.0..=0.8).contains(&self.width_variation)
        {
            return Err("invalid cave walk settings: end probability must be 0.001..1, branch probability 0..1, step metres 2..8, turn degrees 0..100 and width variation 0..0.8; all values must be finite".into());
        }
        Ok(self)
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn expand(
    cave: &mut Cave,
    terrain: &Terrain,
    o: &CaveOptions,
    walk: CaveWalkOptions,
    hazard: &impl Fn(Vec2) -> bool,
    occupied: &[(Vec2, Vec2)],
) -> Result<(), String> {
    let (nodes, branches) = layout(cave, terrain, o, walk, hazard, occupied)?;
    cave.nodes = nodes;
    cave.branches = branches;
    let (minimum, maximum) =
        cave.paths()
            .flatten()
            .fold((cave.minimum, cave.maximum), |(min, max), n| {
                let r = Vec2::splat(n.width * 0.5 + o.broad_amplitude + o.fine_amplitude + 3.0);
                (
                    min.min(n.floor.truncate() - r),
                    max.max(n.floor.truncate() + r),
                )
            });
    cave.surface = SurfaceGrid::new(terrain, minimum, maximum, o.voxel_size)?;
    cave.minimum = cave.surface.minimum;
    cave.maximum = cave.surface.maximum();
    cave.chunks = volume::build(cave, o)?;
    if !clearance::walkable(cave, o) {
        return Err("random-walk cave failed final capsule clearance".into());
    }
    Ok(())
}
#[allow(clippy::too_many_lines)]
pub(crate) fn layout(
    cave: &Cave,
    terrain: &Terrain,
    o: &CaveOptions,
    walk: CaveWalkOptions,
    hazard: &impl Fn(Vec2) -> bool,
    occupied: &[(Vec2, Vec2)],
) -> Result<(Vec<Node>, Vec<CaveBranch>), String> {
    let _timer = crate::profiling::StageTimer::new("caves.random_walk");
    let throat = cave.portal.throat();
    // The short protected vestibule preserves the already validated exterior join.
    let root = Node {
        floor: throat + cave.inward.extend(0.0) * 8.0,
        width: o.entrance_width,
        height: o.entrance_height,
    };
    let mut paths = vec![vec![root]];
    let mut parents = Vec::new();
    let mut pending = vec![(0, cave.inward, walk.end_probability)];
    let mut rng = Rng::new(cave.id ^ 0x5741_4c4b);
    let mut steps = 0;
    while let Some((path, mut direction, end_probability)) = pending.pop() {
        let mut turn = 0.0;
        loop {
            if steps >= MAX_STEPS {
                return Err("cave random walk exceeded the emergency work safeguard".into());
            }
            steps += 1;
            let last = *paths[path].last().unwrap();
            turn = turn * 0.55 + rng.range(-1.0, 1.0) * walk.turn_degrees.to_radians() * 0.65;
            direction = rotate(direction, turn);
            let chamber = rng.range(0.0, 1.0) < 0.08;
            let target_width = if chamber {
                o.chamber_width
            } else {
                rng.range(o.width_min, o.width_max)
            } * rng
                .range(1.0 - walk.width_variation, 1.0 + walk.width_variation);
            let target_height = if chamber {
                o.chamber_height
            } else {
                rng.range(o.height_min, o.height_max)
            };
            let next = Node {
                floor: last.floor + direction.extend(0.0) * walk.step_metres,
                width: (last.width + (target_width - last.width) * 0.45).max(2.5),
                height: (last.height + (target_height - last.height) * 0.45).max(2.5),
            };
            // Only the original vestibule may intersect the entrance blend plane.
            // A returning branch there would otherwise be clipped into another lip.
            let join_margin = last.width.max(next.width) * 0.5
                + o.broad_amplitude
                + o.fine_amplitude
                + o.side_cover
                + volume::ENTRANCE_BLEND_LENGTH;
            let past_join = |n: Node| (n.floor - throat).truncate().dot(cave.inward) >= join_margin;
            let first_step = path == 0 && paths[path].len() == 1;
            if !past_join(next)
                || (!first_step && !past_join(last))
                || !covered(last, next, cave, terrain, o, hazard, occupied)
            {
                break;
            }
            paths[path].push(next);
            if rng.range(0.0, 1.0) < end_probability {
                break;
            }
            if rng.range(0.0, 1.0) < walk.branch_probability {
                let child = paths.len();
                paths.push(vec![next]);
                parents.push((path as u32, (paths[path].len() - 1) as u32));
                let side = if rng.range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
                // Each generation ends sooner than its parent. Once this reaches
                // one, the child takes one covered step and cannot spawn children.
                // Siblings inherit the same rate; creating one never ages its parent.
                pending.push((
                    child,
                    rotate(direction, side * rng.range(0.7, 1.5)),
                    (end_probability * 2.0).min(1.0),
                ));
            }
        }
    }
    // Remove a branch stopped immediately by surrounding geology, retaining
    // parent indices for descendants. Such an empty branch cannot have children.
    let mut remap = vec![0; paths.len()];
    let mut branches = Vec::new();
    let mut paths = paths.into_iter();
    let main = paths.next().unwrap();
    for (index, (nodes, (parent, junction))) in paths.zip(parents).enumerate() {
        if nodes.len() < 2 {
            continue;
        }
        remap[index + 1] = branches.len() as u32 + 1;
        branches.push(CaveBranch {
            parent_path: remap[parent as usize],
            junction_node: junction + if parent == 0 { 2 } else { 0 },
            nodes,
        });
    }
    let mut nodes = vec![
        Node {
            floor: cave.entrance,
            width: o.entrance_width,
            height: o.entrance_height,
        },
        Node {
            floor: throat,
            width: o.entrance_width,
            height: o.entrance_height,
        },
    ];
    nodes.extend(main);
    Ok((nodes, branches))
}

fn rotate(d: Vec2, angle: f32) -> Vec2 {
    Vec2::new(
        d.x * angle.cos() - d.y * angle.sin(),
        d.x * angle.sin() + d.y * angle.cos(),
    )
}
#[allow(clippy::many_single_char_names)]
fn covered(
    a: Node,
    b: Node,
    cave: &Cave,
    terrain: &Terrain,
    o: &CaveOptions,
    hazard: &impl Fn(Vec2) -> bool,
    occupied: &[(Vec2, Vec2)],
) -> bool {
    let radius = a.width.max(b.width) * 0.5 + o.broad_amplitude + o.fine_amplitude + o.side_cover;
    let roof =
        a.floor.z + a.height.max(b.height) + o.roof_cover + o.broad_amplitude + o.fine_amplitude;
    (0..=8).all(|i| {
        let p = a.floor.lerp(b.floor, i as f32 / 8.0);
        (p - cave.portal.throat()).truncate().dot(cave.inward) >= radius + 1.0
            && !hazard(p.truncate() / ISLAND_WORLD_METRES)
            && !occupied.iter().any(|(min, max)| {
                p.truncate().cmpge(*min - Vec2::splat(radius)).all()
                    && p.truncate().cmple(*max + Vec2::splat(radius)).all()
            })
            && (0..16).all(|side| {
                let angle = side as f32 * std::f32::consts::TAU / 16.0;
                let q = p.truncate() + Vec2::new(angle.cos(), angle.sin()) * radius;
                q.cmpgt(Vec2::ONE).all()
                    && q.cmplt(Vec2::splat(ISLAND_WORLD_METRES - 1.0)).all()
                    && terrain_height(terrain, q) >= roof
            })
    })
}
