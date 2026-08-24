//! Trees and bushes placed from the generator's decoration points.
//!
//! Bark, canopy and shrub tones all ride in the mesh's vertex colours, so one
//! white material renders every part of every plant and Bevy can batch the
//! instances. A shared material handle cannot carry a per-instance tint, so the
//! per-plant variation is baked into a small set of meshes instead: each class
//! batches once per variant rather than once in total. Everything else is
//! derived from the decoration index, so the scene stays as deterministic as
//! the island it decorates.
//!
//! A variant is a shape and a tone together, and the shapes are built rather
//! than loaded: cone tiers and merged lobes over a leaning trunk, with the
//! variant's own hash spreading the tiers and setting the lean. There are no
//! asset files anywhere in this crate.
//!
//! Every plant is two entities at the same transform, one carrying the full
//! mesh and one the cone that stands in for it, each with the
//! [`VisibilityRange`] that hands over to the other. Bevy dithers across the
//! margin they share, so the swap has no frame it happens on.

use std::f32::consts::TAU;

use bevy::{
    camera::visibility::VisibilityRange, light::NotShadowCaster, mesh::VertexAttributeValues,
    prelude::*,
};

use crate::{
    convert::island_to_world,
    hash::{choice, mix, unit},
    island_gen::{GeneratedIsland, IslandEntity, IslandReady},
};

/// Trunk dimensions in metres, before the per-plant scale.
const TRUNK_RADIUS: f32 = 0.55;
const TRUNK_HEIGHT: f32 = 6.0;
/// Radians of lean a variant's own hash may put on the trunk, either way. A
/// tree that stands exactly upright is the one thing a whole hillside of them
/// makes obvious.
const TRUNK_LEAN: f32 = 0.10;

/// The cone tiers a conifer is merged from: the height its base sits at, its
/// radius and its own height, in metres. The last one tops out at seventeen,
/// which is the envelope the impostor is cut to.
const CONIFER_TIERS: [(f32, f32, f32); 3] = [(4.4, 3.60, 6.6), (8.2, 2.75, 5.6), (12.0, 1.70, 5.0)];
/// The lobes a broadleaf canopy is merged from: centre and radius in metres.
/// Lower and wider than the conifer, and none of them concentric, so the
/// silhouette breaks up against the sky. The first sits on the trunk's axis and
/// low enough to swallow its top even at the narrowest spread; every other one
/// overlaps that one, or the canopy would float over a bare pole.
const BROADLEAF_LOBES: [(Vec3, f32); 4] = [
    (Vec3::new(0.0, 8.4, 0.0), 3.0),
    (Vec3::new(1.4, 10.3, 0.4), 2.4),
    (Vec3::new(-1.4, 9.9, -1.2), 2.3),
    (Vec3::new(-0.3, 11.9, 1.3), 2.1),
];
/// The lobes each bush variant is merged from, in the same terms.
const BUSH_LOBES: [&[(Vec3, f32)]; 2] = [
    &[
        (Vec3::new(0.0, 1.00, 0.0), 1.9),
        (Vec3::new(1.3, 0.75, 0.5), 1.3),
        (Vec3::new(-0.9, 0.70, -1.0), 1.2),
    ],
    &[
        (Vec3::new(0.0, 0.90, 0.0), 2.1),
        (Vec3::new(-0.4, 1.50, 0.9), 1.3),
    ],
];

/// The cone one tree becomes past [`NEAR_METRES`]: base, radius and height in
/// metres, cut to the envelope both shapes are built inside so nothing moves
/// across the handover.
const IMPOSTOR_BASE: f32 = 4.6;
const IMPOSTOR_RADIUS: f32 = 3.7;
const IMPOSTOR_HEIGHT: f32 = 12.2;
/// A canopy is as much gap as leaf, so what stands in for one at range is
/// darker than the leaf it averages.
const IMPOSTOR_SHADE: f32 = 0.86;

/// Sides each lathe-turned part is built with. The trunk is only ever read from
/// arm's length, a canopy tier from a field away, and an impostor from a
/// two hundred, where seven sides already hold a round silhouette.
const TRUNK_SIDES: u32 = 10;
const IMPOSTOR_TRUNK_SIDES: u32 = 3;
const TIER_SIDES: u32 = 12;
const LOBE_SIDES: u32 = 8;
const LOBE_RINGS: u32 = 5;
const IMPOSTOR_SIDES: u32 = 7;
const SHRUB_SIDES: u32 = 6;
const SHRUB_RINGS: u32 = 3;

/// Metres at which a plant hands its full mesh over to its impostor, and the
/// run the two dither across.
const NEAR_METRES: f32 = 220.0;
const NEAR_DITHER: f32 = 30.0;
/// Where the impostor itself stops. `overview` stands 1.7 km off the island's
/// centre and 3.1 km from its far shore, so this is a backstop against a camera
/// taken out to sea rather than a working cull: every plant the capture poses
/// frame is well inside it.
const FAR_METRES: f32 = 3_200.0;
const FAR_DITHER: f32 = 400.0;

const TREE_SALT: u64 = 0x54c1_9b0e_a3f7_2d41;
const BUSH_SALT: u64 = 0x9f27_3b6d_1c85_ea07;
const PAINT_SALT: u64 = 0x2a7f_e315_c840_9db6;
const SHAPE_SALT: u64 = 0x7b31_04ea_5d69_c2f8;

/// Canopy tones as sRGB. Restrained against the ground they stand on: a
/// saturated green at this density reads as paint.
const CANOPY_TONES: [[f32; 3]; 4] = [
    [0.22, 0.29, 0.17],
    [0.19, 0.25, 0.15],
    [0.26, 0.32, 0.19],
    [0.16, 0.22, 0.13],
];
/// Undergrowth sits a little warmer and lighter than the canopy over it.
const SHRUB_TONES: [[f32; 3]; 4] = [
    [0.29, 0.34, 0.19],
    [0.25, 0.30, 0.17],
    [0.33, 0.38, 0.22],
    [0.22, 0.27, 0.15],
];
const BARK_TONE: [f32; 3] = [0.26, 0.20, 0.14];
/// How much each bush shape is flattened. A shrub that is as tall as it is wide
/// reads as a boulder.
const BUSH_FLATTENING: f32 = 0.62;
/// How far a surface pointing straight down is darkened against one pointing
/// up. This is what gives a smooth canopy or shrub any read of volume.
const CANOPY_VOLUME: f32 = 0.58;
const SHRUB_VOLUME: f32 = 0.62;
const BARK_VOLUME: f32 = 0.25;
/// Per-vertex break-up, so a lathe-turned surface does not read as one.
const LEAF_JITTER: f32 = 0.10;

/// Shape and tone together, which is what one baked mesh carries. Two shapes
/// and four tones stay few enough that each class still batches per variant.
const TREE_SHAPES: usize = 2;
const BUSH_SHAPES: usize = BUSH_LOBES.len();
const TREE_VARIANTS: usize = TREE_SHAPES * CANOPY_TONES.len();
const BUSH_VARIANTS: usize = BUSH_SHAPES * SHRUB_TONES.len();

pub struct VegetationPlugin;

impl Plugin for VegetationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_vegetation.run_if(on_message::<IslandReady>));
    }
}

fn spawn_vegetation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    island: Res<GeneratedIsland>,
) {
    let island = &island.0;
    // White, because every tone is in the vertex colours the material
    // multiplies through.
    let plant = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.92,
        reflectance: 0.03,
        ..default()
    });
    let tree_near: Vec<Handle<Mesh>> = (0..TREE_VARIANTS)
        .map(|variant| meshes.add(tree_mesh(variant)))
        .collect();
    let tree_far: Vec<Handle<Mesh>> = (0..TREE_VARIANTS)
        .map(|variant| meshes.add(tree_impostor(variant)))
        .collect();
    let bush_near: Vec<Handle<Mesh>> = (0..BUSH_VARIANTS)
        .map(|variant| meshes.add(bush_mesh(variant)))
        .collect();
    let bush_far: Vec<Handle<Mesh>> = (0..BUSH_VARIANTS)
        .map(|variant| meshes.add(bush_impostor(variant)))
        .collect();

    // A canopy inside the near range is metres across and stands over ground
    // the camera is looking at, so it casts into the shadow cascades like
    // anything else. Past the handover the same canopy is a few pixels and its
    // shadow is under one, and there are thousands of them, so the impostors
    // stay out of the shadow passes and the contact shadows go on seating them.
    let mut near = Vec::with_capacity(island.trees.len() + island.bushes.len());
    let mut far = Vec::with_capacity(near.capacity());
    for (index, point) in island.trees.iter().enumerate() {
        let hash = mix(index as u64, TREE_SALT);
        let variant = choice(hash, TREE_VARIANTS);
        let placement = placement(hash, *point, TREE_SALT, 0.75, 0.55);
        near.push((
            IslandEntity,
            Mesh3d(tree_near[variant].clone()),
            MeshMaterial3d(plant.clone()),
            placement,
            near_tier(),
        ));
        far.push((
            IslandEntity,
            Mesh3d(tree_far[variant].clone()),
            MeshMaterial3d(plant.clone()),
            placement,
            far_tier(),
            NotShadowCaster,
        ));
    }
    for (index, point) in island.bushes.iter().enumerate() {
        let hash = mix(index as u64, BUSH_SALT);
        let variant = choice(hash, BUSH_VARIANTS);
        let placement = placement(hash, *point, BUSH_SALT, 0.7, 0.7);
        near.push((
            IslandEntity,
            Mesh3d(bush_near[variant].clone()),
            MeshMaterial3d(plant.clone()),
            placement,
            near_tier(),
        ));
        far.push((
            IslandEntity,
            Mesh3d(bush_far[variant].clone()),
            MeshMaterial3d(plant.clone()),
            placement,
            far_tier(),
            NotShadowCaster,
        ));
    }
    commands.spawn_batch(near);
    commands.spawn_batch(far);
}

/// The range a full mesh is drawn over: everything up to the handover.
fn near_tier() -> VisibilityRange {
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: NEAR_METRES..(NEAR_METRES + NEAR_DITHER),
        use_aabb: false,
    }
}

/// The range its impostor is drawn over. The start has to be the other tier's
/// end exactly, or the dither leaves a gap or a doubling.
fn far_tier() -> VisibilityRange {
    VisibilityRange {
        start_margin: NEAR_METRES..(NEAR_METRES + NEAR_DITHER),
        end_margin: FAR_METRES..(FAR_METRES + FAR_DITHER),
        use_aabb: false,
    }
}

/// Decoration points are `(u, v, height)` in normalized island space.
fn placement(hash: u64, point: motu::Vec3, salt: u64, minimum: f32, spread: f32) -> Transform {
    let scale = spread.mul_add(unit(hash), minimum);
    Transform::from_translation(island_to_world(point.x, point.y, point.z))
        .with_rotation(Quat::from_rotation_y(unit(mix(hash, salt)) * TAU))
        .with_scale(Vec3::splat(scale))
}

/// Trunk and canopy are baked into one mesh so each tree stays a single entity.
/// They are painted before the merge, which is what leaves the trunk bark
/// coloured under a material that knows nothing about either.
fn tree_mesh(variant: usize) -> Mesh {
    let hash = mix(variant as u64, SHAPE_SALT);
    // The tiers or lobes of one variant all take the same spread, so a variant
    // stays one tree rather than a stack of unrelated parts.
    let spread = 0.24f32.mul_add(unit(hash), 0.88);
    let lean = Quat::from_rotation_z(TRUNK_LEAN * (unit(mix(hash, SHAPE_SALT)) - 0.5));

    let mut mesh = Mesh::from(
        Cylinder::new(TRUNK_RADIUS, TRUNK_HEIGHT)
            .mesh()
            .resolution(TRUNK_SIDES),
    )
    .translated_by(Vec3::Y * TRUNK_HEIGHT * 0.5)
    .rotated_by(lean);
    paint(&mut mesh, BARK_TONE, BARK_VOLUME);

    let mut canopy = if variant.is_multiple_of(TREE_SHAPES) {
        conifer_canopy(spread)
    } else {
        broadleaf_canopy(spread)
    }
    .rotated_by(lean);
    paint(
        &mut canopy,
        CANOPY_TONES[variant / TREE_SHAPES],
        CANOPY_VOLUME,
    );
    mesh.merge(&canopy)
        .expect("trunk and canopy share the same vertex layout");
    mesh
}

/// Stacked cones of falling radius. Each tier's base is inside the one below
/// it, so the joins never show as a step.
fn conifer_canopy(spread: f32) -> Mesh {
    let tier = |(base, radius, height): (f32, f32, f32)| {
        Mesh::from(
            Cone::new(radius * spread, height)
                .mesh()
                .resolution(TIER_SIDES),
        )
        .translated_by(Vec3::Y * height.mul_add(0.5, base))
    };
    let mut canopy = tier(CONIFER_TIERS[0]);
    for &next in &CONIFER_TIERS[1..] {
        canopy
            .merge(&tier(next))
            .expect("every tier is built the same way");
    }
    canopy
}

fn broadleaf_canopy(spread: f32) -> Mesh {
    merged_lobes(&BROADLEAF_LOBES, spread, LOBE_SIDES, LOBE_RINGS, 1.0)
}

fn bush_mesh(variant: usize) -> Mesh {
    let hash = mix(variant as u64, SHAPE_SALT);
    let spread = 0.24f32.mul_add(unit(hash), 0.88);
    let mut mesh = merged_lobes(
        BUSH_LOBES[variant % BUSH_SHAPES],
        spread,
        LOBE_SIDES,
        LOBE_RINGS,
        BUSH_FLATTENING,
    );
    paint(&mut mesh, SHRUB_TONES[variant / BUSH_SHAPES], SHRUB_VOLUME);
    mesh
}

/// One mesh from a set of overlapping spheres. Nothing removes the surface
/// inside the overlaps: it is never seen, and the alternative is a boolean
/// solver for geometry that is a dozen triangles either way.
fn merged_lobes(
    lobes: &[(Vec3, f32)],
    spread: f32,
    sides: u32,
    rings: u32,
    flattening: f32,
) -> Mesh {
    let lobe = |&(centre, radius): &(Vec3, f32)| {
        Sphere::new(radius * spread)
            .mesh()
            .uv(sides, rings)
            .scaled_by(Vec3::new(1.0, flattening, 1.0))
            .translated_by(centre)
    };
    let mut merged = lobe(&lobes[0]);
    for next in &lobes[1..] {
        merged
            .merge(&lobe(next))
            .expect("every lobe is built the same way");
    }
    merged
}

/// What a tree is past the handover: the cone both shapes are built inside,
/// over a trunk of three sides. A trunk is a few pixels wide at this range and
/// would cost nothing to leave out, but a canopy standing on the ground with no
/// stem under it is what the eye notices instead.
fn tree_impostor(variant: usize) -> Mesh {
    let mut mesh = Mesh::from(
        Cylinder::new(TRUNK_RADIUS, TRUNK_HEIGHT)
            .mesh()
            .resolution(IMPOSTOR_TRUNK_SIDES),
    )
    .translated_by(Vec3::Y * TRUNK_HEIGHT * 0.5);
    paint(&mut mesh, BARK_TONE, BARK_VOLUME);
    let mut canopy = Mesh::from(
        Cone::new(IMPOSTOR_RADIUS, IMPOSTOR_HEIGHT)
            .mesh()
            .resolution(IMPOSTOR_SIDES),
    )
    .translated_by(Vec3::Y * IMPOSTOR_HEIGHT.mul_add(0.5, IMPOSTOR_BASE));
    paint(
        &mut canopy,
        shaded(CANOPY_TONES[variant / TREE_SHAPES]),
        CANOPY_VOLUME,
    );
    mesh.merge(&canopy)
        .expect("trunk and canopy share the same vertex layout");
    mesh
}

/// The same for a bush, which is one flattened sphere at the coarsest ring
/// count that still has an outline.
fn bush_impostor(variant: usize) -> Mesh {
    let lobes = BUSH_LOBES[variant % BUSH_SHAPES];
    let mut mesh = merged_lobes(&lobes[..1], 1.15, SHRUB_SIDES, SHRUB_RINGS, BUSH_FLATTENING);
    paint(
        &mut mesh,
        shaded(SHRUB_TONES[variant / BUSH_SHAPES]),
        SHRUB_VOLUME,
    );
    mesh
}

fn shaded(tone: [f32; 3]) -> [f32; 3] {
    tone.map(|channel| channel * IMPOSTOR_SHADE)
}

/// Writes one linear vertex colour per vertex: the tone, darkened towards the
/// underside by `volume` and broken up by a hash of the vertex position.
fn paint(mesh: &mut Mesh, tone: [f32; 3], volume: f32) {
    let linear = LinearRgba::from(Srgba::rgb(tone[0], tone[1], tone[2]));
    let Some(vertices) = mesh
        .attribute(Mesh::ATTRIBUTE_NORMAL)
        .and_then(VertexAttributeValues::as_float3)
        .map(<[[f32; 3]]>::to_vec)
    else {
        return;
    };
    let colours: Vec<[f32; 4]> = vertices
        .iter()
        .enumerate()
        .map(|(index, normal)| {
            let upward = 0.5f32.mul_add(normal[1], 0.5);
            let jitter = LEAF_JITTER.mul_add(unit(mix(index as u64, PAINT_SALT)) - 0.5, 1.0);
            let shade = volume.mul_add(upward - 0.5, 1.0) * jitter;
            [
                linear.red * shade,
                linear.green * shade,
                linear.blue * shade,
                1.0,
            ]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
}
