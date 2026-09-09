//! Bounded side passages joined by the same watertight portal used at the mouth.
use super::{
    Cave, CaveBranch, CaveOptions, Node, clearance, passage, portal::Portal, surface::SurfaceGrid,
    terrain_height,
};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, rng::Rng, terrain::Terrain};
use serde::{Deserialize, Serialize};

/// Separate additive ABI block; the original `CaveOptions` layout stays unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct CaveNetworkOptions {
    pub maximum_branches: u32,
    pub branch_length_min: f32,
    pub branch_length_max: f32,
    pub chamber_scale: f32,
}
impl Default for CaveNetworkOptions {
    fn default() -> Self {
        Self {
            maximum_branches: 0,
            branch_length_min: 18.0,
            branch_length_max: 30.0,
            chamber_scale: 1.25,
        }
    }
}
impl CaveNetworkOptions {
    /// # Errors
    /// Rejects non-finite dimensions and excessive network budgets.
    pub fn validate(self, o: &CaveOptions) -> Result<Self, String> {
        if self.maximum_branches > 3
            || !self.branch_length_min.is_finite()
            || !self.branch_length_max.is_finite()
            || !(12.0..=48.0).contains(&self.branch_length_min)
            || self.branch_length_max < self.branch_length_min
            || self.branch_length_max > 48.0
            || !self.chamber_scale.is_finite()
            || !(1.0..=2.0).contains(&self.chamber_scale)
            || (self.maximum_branches > 0
                && (o.chamber_width * self.chamber_scale > 24.0
                    || o.chamber_height * self.chamber_scale > 20.0))
        {
            return Err("invalid cave network settings".into());
        }
        Ok(self)
    }
}

pub(crate) fn expand(
    cave: &mut Cave,
    terrain: &Terrain,
    o: &CaveOptions,
    network: CaveNetworkOptions,
    hazard: &impl Fn(Vec2) -> bool,
    occupied: &[(Vec2, Vec2)],
) {
    let _timer = crate::profiling::StageTimer::new("caves.branches");
    let mut original_surface = None;
    let mut rng = Rng::new(cave.id ^ 0x4252_414e_4348_0001);
    // Work back from the chamber toward the entrance. No junction touches the
    // protected entrance transition or the terminal chamber's end cap.
    'junctions: for junction in (3..cave.nodes.len().saturating_sub(2)).rev() {
        for side in [1.0, -1.0] {
            if cave.branches.len() >= network.maximum_branches as usize {
                break 'junctions;
            }
            let length = rng.range(network.branch_length_min, network.branch_length_max);
            let angle = rng.range(0.95, 1.2) * side;
            let direction = (cave.nodes[junction + 1].floor - cave.nodes[junction - 1].floor)
                .truncate()
                .normalize();
            let outward = Vec2::new(
                direction.x * angle.cos() - direction.y * angle.sin(),
                direction.x * angle.sin() + direction.y * angle.cos(),
            );
            let Some(candidate) = candidate(cave, junction, outward, length, o, network) else {
                continue;
            };
            let (branch, portal) = candidate;
            let (minimum, maximum) = bounds(
                cave.paths().chain(std::iter::once(branch.nodes.as_slice())),
                o,
            );
            if minimum.cmple(Vec2::splat(1.0)).any()
                || maximum.cmpge(Vec2::splat(ISLAND_WORLD_METRES - 1.0)).any()
                || occupied
                    .iter()
                    .any(|(a, b)| minimum.cmple(*b).all() && maximum.cmpge(*a).all())
                || !covered(&branch.nodes, terrain, o, hazard)
                || !clearance::Triangles::new([terrain.mesh()], minimum, maximum).rock_cover_path(
                    &branch.nodes,
                    cave.entrance,
                    cave.inward,
                    o,
                )
            {
                continue;
            }
            let Ok(surface) = SurfaceGrid::new(terrain, minimum, maximum, o.voxel_size) else {
                continue;
            };
            let collar = portal.collider(&cave.chunks[0]);
            if collar.triangles.is_empty() {
                continue;
            }
            // Preserve the accepted network until this proposed cut passes real
            // mesh/capsule checks. Only the parent mesh needs a trial copy.
            let parent = portal.cut(cave.chunks[0].clone(), &collar.vertices);
            let child = passage::build_path(
                cave.id ^ junction as u64,
                &branch.nodes,
                &portal,
                &collar,
                o,
            );
            let meshes = || {
                std::iter::once(&parent)
                    .chain(cave.chunks[1..].iter())
                    .chain(std::iter::once(&child))
                    .chain(std::iter::once(&cave.entrance_collider))
            };
            if meshes().map(|m| m.triangles.len() / 3).sum::<usize>() > super::MAX_TRIANGLES / 4 {
                continue;
            }
            let triangles = clearance::Triangles::new(meshes(), surface.minimum, surface.maximum());
            if !clearance::walkable_paths(
                &triangles,
                cave.paths().chain(std::iter::once(branch.nodes.as_slice())),
                o,
            ) {
                continue;
            }
            cave.chunks[0] = parent;
            cave.chunks.push(child);
            cave.minimum = surface.minimum;
            cave.maximum = surface.maximum();
            let previous = std::mem::replace(&mut cave.surface, surface);
            if original_surface.is_none() {
                original_surface = Some(previous);
            }
            cave.branches.push(branch);
        }
    }
    if !cave.branches.is_empty() && !finish_mesh(cave) {
        // A failed topology repair must never publish a cracked network.
        cave.branches.clear();
        cave.surface = original_surface.unwrap();
        cave.minimum = cave.surface.minimum;
        cave.maximum = cave.surface.maximum();
        cave.chunks = vec![passage::build(cave, o)];
    }
}

fn finish_mesh(cave: &mut Cave) -> bool {
    let mut mesh = Mesh::default();
    for chunk in cave.chunks.drain(..) {
        let offset = mesh.vertices.len() as u32;
        mesh.vertices.extend(chunk.vertices);
        mesh.normals.extend(chunk.normals);
        mesh.uv.extend(chunk.uv);
        mesh.triangles
            .extend(chunk.triangles.into_iter().map(|i| i + offset));
    }
    let throat = cave.portal.throat();
    let direction = cave.inward;
    if !super::stitch::close_boundary_seams(&mut mesh, |p| {
        (p - throat).truncate().dot(direction).abs() < 0.01
    }) || (mesh.triangles.len() + cave.entrance_collider.triangles.len()) / 3
        > super::MAX_TRIANGLES / 4
    {
        return false;
    }
    // Split only after welding, retaining identical edge positions and normals.
    // Bounded collision batches keep a larger network from one long collider cook.
    for triangles in mesh.triangles.chunks(10_000 * 3) {
        let mut chunk = Mesh::default();
        let mut indices = std::collections::HashMap::new();
        for &index in triangles {
            let local = *indices.entry(index).or_insert_with(|| {
                let local = chunk.vertices.len() as u32;
                chunk.vertices.push(mesh.vertices[index as usize]);
                chunk.normals.push(mesh.normals[index as usize]);
                chunk.uv.push(mesh.uv[index as usize]);
                local
            });
            chunk.triangles.push(local);
        }
        cave.chunks.push(chunk);
    }
    true
}

fn candidate(
    cave: &Cave,
    junction: usize,
    outward: Vec2,
    length: f32,
    o: &CaveOptions,
    network: CaveNetworkOptions,
) -> Option<(CaveBranch, Portal)> {
    let root = cave.nodes[junction];
    // Keep separate doorways far enough apart that their rounded lips do not
    // cut through each other. Opposite-side doors at one junction are allowed.
    if cave.branches.iter().any(|b| {
        let delta = b.nodes[1].floor - root.floor;
        delta.truncate().dot(outward) > 0.0 && delta.length() < 12.0
    }) {
        return None;
    }
    let mouth_width = (root.width * 0.8).max(1.5).min(o.width_min);
    let mouth_height = (root.height * 0.8).clamp(2.5, 3.2);
    let portal = Portal {
        origin: root.floor + outward.extend(0.0) * 0.5,
        inward: outward,
        length: root.width * 0.7 + 3.0,
        floor_clearance: o.floor_roughness + 0.15,
        exterior: false,
        section: passage::section(mouth_width, mouth_height),
    };
    let mut nodes = vec![
        root,
        Node {
            floor: portal.throat(),
            width: mouth_width,
            height: mouth_height,
        },
    ];
    let chamber_width = o.chamber_width * network.chamber_scale;
    let chamber_height = o.chamber_height * network.chamber_scale;
    // A full-width chamber section gives a room beyond the widening passage.
    let throat = portal.throat();
    for (distance, width, height) in [
        (4.0, o.width_min, o.height_min),
        (length - 8.0, o.width_max, o.height_max),
        (length - 4.0, chamber_width, chamber_height),
        (length, chamber_width, chamber_height),
    ] {
        let floor = throat + outward.extend(0.0) * distance;
        if floor.distance_squared(nodes.last().unwrap().floor) < 1.0e-6 {
            continue;
        }
        let radius = width * 0.5 + o.broad_amplitude + o.fine_amplitude + 0.5;
        // This stage creates branches, not accidental intersections or loops.
        if cave.paths().any(|path| {
            path.windows(2).any(|pair| {
                let edge = pair[1].floor - pair[0].floor;
                let t = ((floor - pair[0].floor).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
                floor.distance(pair[0].floor + edge * t)
                    < radius + pair[0].width.max(pair[1].width) * 0.5
            })
        }) {
            return None;
        }
        nodes.push(Node {
            floor,
            width,
            height,
        });
    }
    Some((
        CaveBranch {
            parent_path: 0,
            junction_node: junction as u32,
            nodes,
        },
        portal,
    ))
}

fn bounds<'a>(paths: impl Iterator<Item = &'a [Node]>, o: &CaveOptions) -> (Vec2, Vec2) {
    paths.flatten().fold(
        (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
        |(min, max), node| {
            let margin = Vec2::splat(node.width * 0.5 + o.broad_amplitude + o.fine_amplitude + 3.0);
            (
                min.min(node.floor.truncate() - margin),
                max.max(node.floor.truncate() + margin),
            )
        },
    )
}

fn covered(
    nodes: &[Node],
    terrain: &Terrain,
    o: &CaveOptions,
    hazard: &impl Fn(Vec2) -> bool,
) -> bool {
    nodes.windows(2).all(|pair| {
        let count = (pair[0].floor.distance(pair[1].floor) / 0.5).ceil() as usize;
        (0..=count).all(|i| {
            let t = i as f32 / count.max(1) as f32;
            let floor = pair[0].floor.lerp(pair[1].floor, t);
            let radius = (pair[0].width + (pair[1].width - pair[0].width) * t) * 0.5
                + o.broad_amplitude
                + o.fine_amplitude
                + o.side_cover;
            let roof = floor.z
                + pair[0].height
                + (pair[1].height - pair[0].height) * t
                + o.roof_cover
                + o.broad_amplitude
                + o.fine_amplitude;
            !hazard(floor.truncate() / ISLAND_WORLD_METRES)
                && (0..16).all(|side| {
                    let a = side as f32 * std::f32::consts::TAU / 16.0;
                    terrain_height(
                        terrain,
                        floor.truncate() + Vec2::new(a.cos(), a.sin()) * radius,
                    ) >= roof
                })
        })
    })
}
