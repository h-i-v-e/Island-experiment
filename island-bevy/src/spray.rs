//! The mist a fall throws, as one merged cloud of camera-facing quads.
//!
//! Every droplet is four vertices and nothing else: no per-frame CPU work, no
//! buffer rewritten between frames, no entity of its own. Its whole arc — where
//! it leaves the water, how fast, how big, how long it lasts and where in that
//! life it starts — is written into the mesh once, and `spray.wgsl` evaluates
//! the arc from the same water clock both water surfaces animate on. A frozen
//! clock therefore freezes the cloud exactly as it freezes a crest, and two
//! captures of one command find every droplet in the same place.
//!
//! Everything a droplet is comes from hashing its own index against the drop it
//! belongs to, so the cloud is as deterministic as the island under it. What it
//! is aiming at is restraint: a fall this size throws a haze that hangs at its
//! foot and drifts a few metres downstream, not a fountain.

use bevy::{
    asset::RenderAssetUsages,
    camera::primitives::Aabb,
    light::NotShadowCaster,
    math::Vec3A,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use motu::ISLAND_WORLD_METRES;

use crate::{
    convert::island_to_world,
    hash::{mix, unit},
    island_gen::{GeneratedIsland, IslandEntity, IslandReady, RiverDrop},
    surface::{SprayExtension, SprayMaterial},
};

/// Droplets a fall throws before its own height is counted, and how many more
/// the tallest one adds. A hundred and ninety quads on the biggest fall on the
/// island is seven hundred and sixty vertices, and every fall on the
/// island together is one mesh and one draw. What makes a cloud read as mist is
/// many faint droplets; a few strong ones only ever read as sprites.
const BASE_DROPLETS: u32 = 40;
const STRENGTH_DROPLETS: f32 = 150.0;
/// Metres per second a droplet leaves the water at, before and after the fall's
/// own strength is counted, and the share of that speed the fall's own heading
/// and the channel's width contribute. Upward dominates: what stands at the
/// foot of a fall is thrown off the impact, not blown along the reach.
const RISE_SPEED: f32 = 0.80;
const RISE_STRENGTH: f32 = 1.50;
const DRIFT_SPEED: f32 = 0.90;
const SIDEWAYS_SPEED: f32 = 0.50;
/// Seconds a droplet lasts, and the metres across it opens at. Both are a range
/// the droplet's own hash picks from, or the cloud reads as one object.
///
/// Big and faint rather than small and solid: what the eye reads as mist is
/// many overlapping veils, and a droplet small enough to be seen whole is a
/// sprite whatever it is drawn with.
const LIFE_SECONDS: f32 = 0.75;
const LIFE_SPREAD: f32 = 0.80;
const SIZE_METRES: f32 = 0.35;
const SIZE_SPREAD: f32 = 0.55;
/// How far downstream of the foot and how far up from the water a droplet may
/// start, in metres, and the widest the cloud spreads across the channel. The
/// lateral spread is the channel's own half width up to that cap: a fourteen
/// metre channel does not throw mist off its whole width, it throws it where
/// the water actually lands.
const LAUNCH_ALONG_METRES: f32 = 1.2;
const LAUNCH_RISE_METRES: f32 = 0.4;
const LAUNCH_ACROSS_METRES: f32 = 2.5;
/// Metres the mesh's own bounds are grown by past the launch points, which is
/// further than any arc these speeds and lives can carry a droplet.
const CLOUD_MARGIN: f32 = 9.0;

/// Distinguishes spray from the crate's other hashed values.
const SPRAY_SALT: u64 = 0x53c7_1a94_e60d_2fb5;

pub struct SprayPlugin;

impl Plugin for SprayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_spray.run_if(on_message::<IslandReady>));
    }
}

fn spawn_spray(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SprayMaterial>>,
    island: Res<GeneratedIsland>,
) {
    let Some((mesh, bounds)) = cloud(&island.0.river_drops) else {
        return;
    };
    // The extension writes base colour, opacity and roughness outright, so what
    // the base material still decides is only how a droplet is drawn: blended,
    // from either side, after the sky.
    let material = materials.add(SprayMaterial {
        base: StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        },
        extension: SprayExtension::default(),
    });
    commands.spawn((
        Name::new("Spray"),
        IslandEntity,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform::default(),
        // The vertex stage throws each droplet metres away from the position
        // the bounds would otherwise be computed from, so they are given rather
        // than derived — a cloud culled on its launch points alone would blink
        // out while its own mist was still on screen.
        bounds,
        // Mist that cast a shadow would print the quads it is drawn on across
        // the ground under it.
        NotShadowCaster,
    ));
}

/// One mesh for every droplet on the island, and the bounds to cull it by.
/// `None` where the island has no fall worth throwing any.
fn cloud(drops: &[RiverDrop]) -> Option<(Mesh, Aabb)> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut velocities: Vec<[f32; 3]> = Vec::new();
    let mut corners: Vec<[f32; 2]> = Vec::new();
    let mut droplets: Vec<[f32; 4]> = Vec::new();
    let mut triangles: Vec<u32> = Vec::new();

    for (index, drop) in drops.iter().enumerate() {
        let strength = drop.strength();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = BASE_DROPLETS + (STRENGTH_DROPLETS * strength) as u32;
        let seed = mix(index as u64, SPRAY_SALT);
        for droplet in 0..count {
            let hash = mix(u64::from(droplet), seed);
            let launch = launch_point(*drop, hash);
            let velocity = launch_velocity(*drop, strength, hash);
            // Phase, size, life and brightness. The phase is what spreads a
            // cloud across its own cycle rather than pulsing as one body.
            let traits = [
                unit(mix(hash, 0x11)),
                SIZE_METRES + SIZE_SPREAD * unit(mix(hash, 0x22)),
                LIFE_SECONDS + LIFE_SPREAD * unit(mix(hash, 0x33)),
                0.55 + 0.45 * strength,
            ];
            #[allow(clippy::cast_possible_truncation)]
            let first = positions.len() as u32;
            for corner in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
                positions.push(launch.to_array());
                velocities.push(velocity.to_array());
                corners.push(corner);
                droplets.push(traits);
            }
            triangles.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
    }
    let mut bounds = Aabb::enclosing(positions.iter().map(|&point| Vec3::from(point)))?;
    bounds.half_extents += Vec3A::splat(CLOUD_MARGIN);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, velocities);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, corners);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, droplets);
    mesh.insert_indices(Indices::U32(triangles));
    Some((mesh, bounds))
}

/// Where one droplet leaves the water: across the channel at the foot, a little
/// way downstream of it and a little way above it.
///
/// The lateral offset is the sum of two hashes rather than one, which puts most
/// of the cloud near the middle of the channel and thins it towards the sides —
/// where the water lands, rather than evenly across a width the fall may not
/// use.
fn launch_point(drop: RiverDrop, hash: u64) -> Vec3 {
    let spread = drop
        .half_width
        .min(LAUNCH_ACROSS_METRES / ISLAND_WORLD_METRES);
    let offset = unit(mix(hash, 0x41)) + unit(mix(hash, 0x44)) - 1.0;
    let sideways = drop.direction.perp() * offset * spread;
    let along =
        drop.direction * (unit(mix(hash, 0x42)) * LAUNCH_ALONG_METRES / ISLAND_WORLD_METRES);
    let foot = drop.foot.truncate() + sideways + along;
    island_to_world(foot.x, foot.y, drop.foot.z)
        + Vec3::Y * (unit(mix(hash, 0x43)) * LAUNCH_RISE_METRES)
}

/// How fast and which way, in metres per second of world space. Island space
/// puts x across the square and y along it, which the render space takes as x
/// and z, so a heading crosses without any scale at all.
fn launch_velocity(drop: RiverDrop, strength: f32, hash: u64) -> Vec3 {
    let downstream = Vec3::new(drop.direction.x, 0.0, drop.direction.y).normalize_or(Vec3::X);
    let sideways = downstream.cross(Vec3::Y);
    let rise = RISE_SPEED + RISE_STRENGTH * strength * unit(mix(hash, 0x51));
    downstream * (DRIFT_SPEED * unit(mix(hash, 0x52)))
        + sideways * (SIDEWAYS_SPEED * (unit(mix(hash, 0x53)) * 2.0 - 1.0))
        + Vec3::Y * rise
}
