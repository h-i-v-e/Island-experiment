//! Trees and bushes placed from the generator's decoration points.
//!
//! Mature trees use the reviewed procedural pōhutukawa model. One bounded
//! prototype is compiled into a merged wood draw and a merged foliage-pad draw,
//! then shared by every placement so Bevy can instance both. Bushes and distant
//! trees keep vertex-coloured low-detail meshes under one white material.
//! Everything is derived from the island seed and decoration index.
//!
//! The generator emits far more tree points than mature spreading crowns can
//! occupy. A deterministic one-in-sixty-four subset becomes pōhutukawa; the
//! unused points remain open for future understorey rather than producing an
//! overlapping wall of trunks. There are no external tree assets.
//!
//! Each mature tree has three dithered levels: merged botanical wood and
//! foliage inside gameplay distance, a coarse spreading broadleaf mesh through
//! the middle tier, and a non-shadow-casting copy at landscape distance.
//! Bushes retain the original two-tier handover.
//!
//! The plants are not loose in the world: each tier of each region of the
//! terrain grid has a parent entity, and every plant hangs off the parent for
//! its region and tier. A parent whose whole region stands outside its tier's
//! range is hidden, and Bevy's visibility propagation then skips the subtree
//! rather than testing thousands of instances that cannot be drawn. See
//! [`ScatterGroup`].

use std::{f32::consts::TAU, ops::Range};

use crate::{
    budget::BudgetItem,
    camera::ViewPose,
    chunk,
    convert::island_to_world,
    hash::{choice, mix, unit},
    island_gen::{GeneratedIsland, GenerationSettings, IslandEntity, IslandReady},
};
use bevy::{
    camera::visibility::{VisibilityRange, VisibilitySystems},
    ecs::system::SystemParam,
    light::{NotShadowCaster, TransmittedShadowReceiver},
    mesh::VertexAttributeValues,
    prelude::*,
};
use island_tree::{
    BarkMaterial, BotanicalRecipe, CompiledTreePrototype, LeafMaterial,
    compile_static_middle_prototype_with_recipe,
};

/// Trunk dimensions in metres, before the per-plant scale.
const TRUNK_RADIUS: f32 = 0.55;
const TRUNK_HEIGHT: f32 = 6.0;
/// Radians of lean a variant's own hash may put on the trunk, either way. A
/// tree that stands exactly upright is the one thing a whole hillside of them
/// makes obvious.
const TRUNK_LEAN: f32 = 0.10;

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

/// A canopy is as much gap as leaf, so what stands in for one at range is
/// darker than the leaf it averages.
const IMPOSTOR_SHADE: f32 = 0.86;

/// Sides each lathe-turned part is built with. The trunk is only ever read from
/// arm's length, a canopy tier from a field away, and an impostor from a
/// two hundred, where seven sides already hold a round silhouette.
const IMPOSTOR_TRUNK_SIDES: u32 = 3;
const LOBE_SIDES: u32 = 8;
const LOBE_RINGS: u32 = 5;
const IMPOSTOR_SIDES: u32 = 7;
const SHRUB_SIDES: u32 = 6;
const SHRUB_RINGS: u32 = 3;

/// Metres at which a plant hands its full mesh over to its impostor, and the
/// run the two dither across.
const NEAR_METRES: f32 = 220.0;
const NEAR_DITHER: f32 = 30.0;
/// Full botanical geometry is only perceptually useful at gameplay distance.
/// Selected trees hand over to the shared broadleaf forest mesh across this
/// interval; unselected trees use that mesh throughout the near tier.
const DETAIL_METRES: f32 = 95.0;
const DETAIL_DITHER: f32 = 20.0;
/// Where the impostor itself stops. `overview` stands 1.7 km off the island's
/// centre and 3.1 km from its far shore, so this is a backstop against a camera
/// taken out to sea rather than a working cull: every plant the capture poses
/// frame is well inside it.
const FAR_METRES: f32 = 3_200.0;
const FAR_DITHER: f32 = 400.0;

/// Regions along each edge of the island that the scatter is grouped on.
///
/// The terrain's own grid, so a group covers exactly one chunk's square and the
/// two answer the frustum with the same granularity. At eight divisions that is
/// 250 m and about sixty plants a group.
const GROUPS: u32 = chunk::DIVISIONS;

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

const BUSH_SHAPES: usize = BUSH_LOBES.len();
const BUSH_VARIANTS: usize = BUSH_SHAPES * SHRUB_TONES.len();
/// A bounded handful of generated trees breaks repeated silhouette and bark
/// without giving every placement unique geometry or textures.
const PROCEDURAL_TREE_VARIANTS: usize = 1;
/// Decoration density was authored for small placeholder trees. One in sixty-
/// four points becomes a mature spreading tree; the unused points remain
/// available to a later understorey pass rather than overlapping large crowns.
const PROCEDURAL_TREE_PLACEMENT_MASK: u64 = 0b11_1111;

/// One tier of one region of the scatter, as the thing that is culled.
///
/// Frustum culling and [`VisibilityRange`] both work one entity at a time, and
/// at terrain size 1024 that is 7,680 of them, every frame, most of which are
/// nowhere near the range they are drawn over. A group stands for all the
/// plants of one tier inside one square of the terrain grid: the sphere that
/// holds every one of their origins, and the range that tier is drawn over.
///
/// Hiding the parent is exact rather than approximate. Bevy measures a plant's
/// range against its own translation, so a group whose sphere does not reach
/// the tier's range holds nothing that could be drawn — and nothing that could
/// cast into the shadow cascades either, which take the same range from the
/// same camera.
#[derive(Component)]
struct ScatterGroup {
    /// The centre of the group's plant origins, and the furthest any of them
    /// stands from it.
    centre: Vec3,
    radius: f32,
    /// The distances this tier is drawn over, dither margins included.
    range: Range<f32>,
}

impl ScatterGroup {
    /// Whether any plant in the group can be inside its tier's range.
    fn reaches(&self, eye: Vec3) -> bool {
        let distance = eye.distance(self.centre);
        distance + self.radius >= self.range.start && distance - self.radius <= self.range.end
    }
}

pub struct VegetationPlugin;

impl Plugin for VegetationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_vegetation.run_if(on_message::<IslandReady>))
            // Before the propagation that carries a hidden parent down to its
            // plants, and so before the visibility pass that would otherwise
            // test each of them.
            .add_systems(
                PostUpdate,
                cull_groups.before(VisibilitySystems::VisibilityPropagate),
            );
    }
}

/// Mutable renderer registries used only while an island's shared vegetation
/// assets are compiled. Grouping them keeps the Bevy system boundary explicit
/// without passing ownership of any registry beyond this frame.
#[derive(SystemParam)]
struct VegetationAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    bark_materials: ResMut<'w, Assets<BarkMaterial>>,
    leaf_materials: ResMut<'w, Assets<LeafMaterial>>,
}

/// Hides every group with nothing left to draw. The camera has no parent, so
/// its own transform is already its world position.
fn cull_groups(
    cameras: Query<&Transform, With<Camera3d>>,
    mut groups: Query<(&ScatterGroup, &mut Visibility)>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let eye = camera.translation;
    for (group, mut visibility) in &mut groups {
        visibility.set_if_neq(if group.reaches(eye) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

fn spawn_vegetation(
    mut commands: Commands,
    mut assets: VegetationAssets,
    island: Res<GeneratedIsland>,
    settings: Res<GenerationSettings>,
    pose: Res<ViewPose>,
) {
    let island = &island.0;
    // White, because every tone is in the vertex colours the material
    // multiplies through.
    let plant = assets.materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.92,
        reflectance: 0.03,
        ..default()
    });
    // The vertex count crosses with the handle: the budget census reads it off
    // the entity rather than looking the mesh asset up thousands of times a
    // frame.
    let add = |meshes: &mut Assets<Mesh>, mesh: Mesh| {
        let vertices = u32::try_from(mesh.count_vertices()).unwrap_or(u32::MAX);
        (meshes.add(mesh), vertices)
    };
    let tree_near: Vec<CompiledTreePrototype> = (0..PROCEDURAL_TREE_VARIANTS)
        .map(|variant| {
            let seed = mix(settings.seed, TREE_SALT ^ variant as u64);
            compile_static_middle_prototype_with_recipe(
                seed,
                BotanicalRecipe {
                    leaves_per_terminal: 32,
                    ..BotanicalRecipe::default()
                },
                &mut assets.meshes,
                &mut assets.images,
                &mut assets.bark_materials,
                &mut assets.leaf_materials,
            )
            .unwrap_or_else(|error| panic!("procedural tree {variant} failed: {error}"))
        })
        .collect();
    let tree_far: Vec<(Handle<Mesh>, u32)> = (0..PROCEDURAL_TREE_VARIANTS)
        .map(|variant| add(&mut assets.meshes, pohutukawa_impostor(variant)))
        .collect();
    let bush_near: Vec<(Handle<Mesh>, u32)> = (0..BUSH_VARIANTS)
        .map(|variant| add(&mut assets.meshes, bush_mesh(variant)))
        .collect();
    let bush_far: Vec<(Handle<Mesh>, u32)> = (0..BUSH_VARIANTS)
        .map(|variant| add(&mut assets.meshes, bush_impostor(variant)))
        .collect();

    // A canopy inside the near range is metres across and stands over ground
    // the camera is looking at, so it casts into the shadow cascades like
    // anything else. Past the handover the same canopy is a few pixels and its
    // shadow is under one, and there are thousands of them, so the impostors
    // stay out of the shadow passes and the contact shadows go on seating them.
    //
    // Both tiers are collected per region rather than as one flat list, because
    // what is spawned is a subtree per region and not a plant per world.
    let regions = (GROUPS * GROUPS) as usize;
    let mut near: Vec<Vec<NearPlant>> = (0..regions).map(|_| Vec::new()).collect();
    let mut near_wood: Vec<Vec<NearWoodPlant>> = (0..regions).map(|_| Vec::new()).collect();
    let mut near_foliage: Vec<Vec<NearFoliagePlant>> = (0..regions).map(|_| Vec::new()).collect();
    let mut far: Vec<Vec<FarPlant>> = (0..regions).map(|_| Vec::new()).collect();
    let mut origins: Vec<Vec<Vec3>> = (0..regions).map(|_| Vec::new()).collect();
    let tree_scatter = {
        let lods = TreeRenderLods {
            near: &tree_near,
            far: &tree_far,
            far_material: &plant,
        };
        let mut buffers = TreeScatterBuffers {
            origins: &mut origins,
            coarse: &mut near,
            wood: &mut near_wood,
            foliage: &mut near_foliage,
            far: &mut far,
        };
        scatter_trees(&island.trees, lods, &mut buffers, pose.eye)
    };
    scatter(
        &island.bushes,
        PlantScatter {
            salt: BUSH_SALT,
            near: &bush_near,
            far: &bush_far,
            minimum: 0.7,
            spread: 0.7,
        },
        &plant,
        &mut origins,
        &mut near,
        &mut far,
    );

    let groups = spawn_groups(
        &mut commands,
        &origins,
        &mut near,
        &mut near_wood,
        &mut near_foliage,
        &mut far,
    );
    info!(
        "vegetation: {} mature procedural trees from {} decoration points ({} shared variants), and {} bushes over {groups} scatter groups",
        tree_scatter.placed,
        island.trees.len(),
        tree_near.len(),
        island.bushes.len()
    );
    if let Some(nearest) = tree_scatter.nearest {
        info!(
            "nearest procedural tree to camera: {:.1} m at {:.1}, {:.1}, {:.1}",
            nearest.distance, nearest.position.x, nearest.position.y, nearest.position.z
        );
    }
    for (variant, tree) in tree_near.iter().enumerate() {
        info!(
            "procedural tree variant {variant}: {} wood vertices + {} foliage vertices",
            tree.wood.vertices, tree.foliage.vertices
        );
    }
}

fn scatter_trees(
    points: &[motu::Vec3],
    lods: TreeRenderLods<'_>,
    buffers: &mut TreeScatterBuffers<'_>,
    eye: Vec3,
) -> TreeScatter {
    debug_assert_eq!(lods.near.len(), lods.far.len());
    let mut result = TreeScatter::default();
    for (index, &point) in points.iter().enumerate() {
        let hash = mix(index as u64, TREE_SALT);
        if !place_procedural_tree(hash) {
            continue;
        }
        let variant = choice(hash, lods.near.len());
        let placement = placement(hash, point, TREE_SALT, 0.78, 0.42);
        let region = region(point);
        buffers.origins[region].push(placement.translation);
        result.placed += 1;
        let distance = eye.distance(placement.translation);
        if result
            .nearest
            .is_none_or(|nearest| distance < nearest.distance)
        {
            result.nearest = Some(NearestTree {
                distance,
                position: placement.translation,
            });
        }
        buffers.coarse[region].push((
            BudgetItem::scatter(lods.far[variant].1),
            Mesh3d(lods.far[variant].0.clone()),
            MeshMaterial3d(lods.far_material.clone()),
            placement,
            middle_tier(),
        ));
        buffers.wood[region].push((
            BudgetItem::scatter(lods.near[variant].wood.vertices),
            Mesh3d(lods.near[variant].wood.mesh.clone()),
            MeshMaterial3d(lods.near[variant].wood.material.clone()),
            placement,
            detail_tier(),
        ));
        buffers.foliage[region].push((
            BudgetItem::scatter(lods.near[variant].foliage.vertices),
            Mesh3d(lods.near[variant].foliage.mesh.clone()),
            MeshMaterial3d(lods.near[variant].foliage.material.clone()),
            TransmittedShadowReceiver,
            placement,
            detail_tier(),
        ));
        buffers.far[region].push((
            BudgetItem::scatter(lods.far[variant].1),
            Mesh3d(lods.far[variant].0.clone()),
            MeshMaterial3d(lods.far_material.clone()),
            placement,
            far_tier(),
            NotShadowCaster,
        ));
    }
    result
}

#[derive(Clone, Copy)]
struct TreeRenderLods<'a> {
    near: &'a [CompiledTreePrototype],
    far: &'a [(Handle<Mesh>, u32)],
    far_material: &'a Handle<StandardMaterial>,
}

struct TreeScatterBuffers<'a> {
    origins: &'a mut [Vec<Vec3>],
    coarse: &'a mut [Vec<NearPlant>],
    wood: &'a mut [Vec<NearWoodPlant>],
    foliage: &'a mut [Vec<NearFoliagePlant>],
    far: &'a mut [Vec<FarPlant>],
}

#[derive(Clone, Copy, Debug)]
struct NearestTree {
    distance: f32,
    position: Vec3,
}

#[derive(Debug, Default)]
struct TreeScatter {
    placed: usize,
    nearest: Option<NearestTree>,
}

/// The class-specific inputs for the otherwise identical scatter pass.
struct PlantScatter<'a> {
    salt: u64,
    near: &'a [(Handle<Mesh>, u32)],
    far: &'a [(Handle<Mesh>, u32)],
    minimum: f32,
    spread: f32,
}

fn scatter(
    points: &[motu::Vec3],
    class: PlantScatter<'_>,
    material: &Handle<StandardMaterial>,
    origins: &mut [Vec<Vec3>],
    near: &mut [Vec<NearPlant>],
    far: &mut [Vec<FarPlant>],
) {
    debug_assert_eq!(class.near.len(), class.far.len());
    for (index, &point) in points.iter().enumerate() {
        let hash = mix(index as u64, class.salt);
        let variant = choice(hash, class.near.len());
        let placement = placement(hash, point, class.salt, class.minimum, class.spread);
        let region = region(point);
        origins[region].push(placement.translation);
        near[region].push((
            BudgetItem::scatter(class.near[variant].1),
            Mesh3d(class.near[variant].0.clone()),
            MeshMaterial3d(material.clone()),
            placement,
            near_tier(),
        ));
        far[region].push((
            BudgetItem::scatter(class.far[variant].1),
            Mesh3d(class.far[variant].0.clone()),
            MeshMaterial3d(material.clone()),
            placement,
            far_tier(),
            NotShadowCaster,
        ));
    }
}

/// Puts one parent per tier over every region that has plants in it, and hangs
/// that region's plants off them. Returns how many parents were spawned.
fn spawn_groups(
    commands: &mut Commands,
    origins: &[Vec<Vec3>],
    near: &mut [Vec<NearPlant>],
    near_wood: &mut [Vec<NearWoodPlant>],
    near_foliage: &mut [Vec<NearFoliagePlant>],
    far: &mut [Vec<FarPlant>],
) -> usize {
    let mut groups = 0;
    for (index, origins) in origins.iter().enumerate() {
        let Some((centre, radius)) = sphere(origins) else {
            continue;
        };
        let place = u32::try_from(index).unwrap_or(0);
        let (column, row) = (place % GROUPS, place / GROUPS);
        let mut group = |tier: &str, range: Range<f32>| {
            groups += 1;
            commands
                .spawn((
                    Name::new(format!("Scatter {column},{row} {tier}")),
                    IslandEntity,
                    BudgetItem::group(),
                    ScatterGroup {
                        centre,
                        radius,
                        range,
                    },
                    // The plants keep their own world placement, so the parent
                    // sits at the origin and carries only the visibility its
                    // subtree inherits.
                    Transform::default(),
                    Visibility::default(),
                ))
                .id()
        };
        let detail_group = group("detail", 0.0..(DETAIL_METRES + DETAIL_DITHER));
        let near_group = group("near", 0.0..(NEAR_METRES + NEAR_DITHER));
        let far_group = group("far", NEAR_METRES..(FAR_METRES + FAR_DITHER));
        commands.spawn_batch(
            std::mem::take(&mut near[index])
                .into_iter()
                .map(move |plant| (plant, ChildOf(near_group))),
        );
        commands.spawn_batch(
            std::mem::take(&mut near_wood[index])
                .into_iter()
                .map(move |plant| (plant, ChildOf(detail_group))),
        );
        commands.spawn_batch(
            std::mem::take(&mut near_foliage[index])
                .into_iter()
                .map(move |plant| (plant, ChildOf(detail_group))),
        );
        commands.spawn_batch(
            std::mem::take(&mut far[index])
                .into_iter()
                .map(move |plant| (plant, ChildOf(far_group))),
        );
    }
    groups
}

/// One plant of each tier, as the bundle it is spawned from. Written out
/// because the two lists are built per region and a `Vec` needs the type.
type NearPlant = (
    BudgetItem,
    Mesh3d,
    MeshMaterial3d<StandardMaterial>,
    Transform,
    VisibilityRange,
);
type NearWoodPlant = (
    BudgetItem,
    Mesh3d,
    MeshMaterial3d<BarkMaterial>,
    Transform,
    VisibilityRange,
);
type NearFoliagePlant = (
    BudgetItem,
    Mesh3d,
    MeshMaterial3d<LeafMaterial>,
    TransmittedShadowReceiver,
    Transform,
    VisibilityRange,
);
type FarPlant = (
    BudgetItem,
    Mesh3d,
    MeshMaterial3d<StandardMaterial>,
    Transform,
    VisibilityRange,
    NotShadowCaster,
);

/// The smallest sphere around a set of points that a centre-and-radius pair can
/// state: the centre of their bounding box, and the furthest of them from it.
/// `None` for a region with no plants in it, which needs no group at all.
fn sphere(points: &[Vec3]) -> Option<(Vec3, f32)> {
    let mut low = *points.first()?;
    let mut high = low;
    for &point in points {
        low = low.min(point);
        high = high.max(point);
    }
    let centre = low.midpoint(high);
    let radius = points
        .iter()
        .map(|point| centre.distance(*point))
        .fold(0.0, f32::max);
    Some((centre, radius))
}

/// The region a decoration point falls in. The grid is the terrain's own, so a
/// scatter group covers exactly one terrain chunk's square.
fn region(point: motu::Vec3) -> usize {
    let divisions = f32::from(u8::try_from(GROUPS).unwrap_or(u8::MAX));
    let cell = |coordinate: f32| {
        // Clamped into the grid before the cast, so nothing here can truncate,
        // lose a sign or index past the last region.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cell = (coordinate * divisions).clamp(0.0, divisions - 1.0) as usize;
        cell
    };
    cell(point.y) * GROUPS as usize + cell(point.x)
}

/// The range a full mesh is drawn over: everything up to the handover.
fn near_tier() -> VisibilityRange {
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: NEAR_METRES..(NEAR_METRES + NEAR_DITHER),
        use_aabb: false,
    }
}

fn detail_tier() -> VisibilityRange {
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: DETAIL_METRES..(DETAIL_METRES + DETAIL_DITHER),
        use_aabb: false,
    }
}

fn middle_tier() -> VisibilityRange {
    VisibilityRange {
        start_margin: DETAIL_METRES..(DETAIL_METRES + DETAIL_DITHER),
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

fn place_procedural_tree(hash: u64) -> bool {
    mix(hash, SHAPE_SALT) & PROCEDURAL_TREE_PLACEMENT_MASK == 0
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

/// A coarse low, spreading broadleaf silhouette for the generated tree's far
/// tier. It deliberately keeps no leaf cards or bark material work once the
/// crown is only a few pixels across.
fn pohutukawa_impostor(variant: usize) -> Mesh {
    let hash = mix(variant as u64, SHAPE_SALT);
    let spread = 0.18f32.mul_add(unit(hash), 0.92);
    let lean = Quat::from_rotation_z(TRUNK_LEAN * (unit(mix(hash, TREE_SALT)) - 0.5));
    let mut mesh = Mesh::from(
        Cylinder::new(TRUNK_RADIUS, TRUNK_HEIGHT)
            .mesh()
            .resolution(IMPOSTOR_TRUNK_SIDES),
    )
    .translated_by(Vec3::Y * TRUNK_HEIGHT * 0.5)
    .rotated_by(lean);
    paint(&mut mesh, BARK_TONE, BARK_VOLUME);
    let mut canopy =
        merged_lobes(&BROADLEAF_LOBES, spread, IMPOSTOR_SIDES, SHRUB_RINGS, 0.82).rotated_by(lean);
    paint(
        &mut canopy,
        shaded(CANOPY_TONES[variant % CANOPY_TONES.len()]),
        CANOPY_VOLUME,
    );
    mesh.merge(&canopy)
        .expect("impostor trunk and canopy share the same vertex layout");
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

#[cfg(test)]
mod tests {
    use motu::ISLAND_WORLD_METRES;

    use super::{
        FAR_DITHER, FAR_METRES, GROUPS, NEAR_DITHER, NEAR_METRES, ScatterGroup, Vec3, far_tier,
        near_tier, region, sphere,
    };

    /// A group may only be hidden when nothing inside it could be drawn. The
    /// tier ranges and the group sphere are the same two numbers Bevy measures
    /// a plant against, so the check has to answer for every plant the sphere
    /// holds — including the nearest and furthest of them.
    #[test]
    fn a_group_is_only_hidden_when_every_plant_in_it_is_out_of_range() {
        let group = |range| ScatterGroup {
            centre: Vec3::ZERO,
            radius: 120.0,
            range,
        };
        let near = group(0.0..(NEAR_METRES + NEAR_DITHER));
        // A plant on the near face of the sphere is still inside the near
        // tier's range, so the group stays up even though its centre is past
        // the handover.
        assert!(near.reaches(Vec3::new(NEAR_METRES + NEAR_DITHER + 100.0, 0.0, 0.0)));
        assert!(!near.reaches(Vec3::new(NEAR_METRES + NEAR_DITHER + 130.0, 0.0, 0.0)));

        let far = group(NEAR_METRES..(FAR_METRES + FAR_DITHER));
        // Standing inside the region, the impostors of its own plants are all
        // nearer than the handover and none of them is drawn.
        assert!(!far.reaches(Vec3::ZERO));
        assert!(far.reaches(Vec3::new(NEAR_METRES, 0.0, 0.0)));
    }

    /// The two tiers have to meet exactly, or the range a group is kept over
    /// would leave a band with neither tier in it.
    #[test]
    fn the_group_ranges_are_the_tiers_own() {
        assert_eq!(near_tier().end_margin, far_tier().start_margin);
        assert!(near_tier().start_margin.start.abs() < f32::EPSILON);
        assert!(far_tier().end_margin.end >= FAR_METRES);
    }

    /// Every decoration point lands in the region its coordinates name, and a
    /// point on or past an edge lands inside the grid rather than off it.
    #[test]
    fn a_point_falls_in_the_region_that_covers_it() {
        let last = GROUPS as usize - 1;
        assert_eq!(region(motu::Vec3::new(0.0, 0.0, 0.0)), 0);
        assert_eq!(
            region(motu::Vec3::new(1.0, 1.0, 0.0)),
            last * GROUPS as usize + last
        );
        assert_eq!(region(motu::Vec3::new(0.99, 0.0, 0.0)), last);
        // The generator clamps its decorations to the square, but a point on
        // the far edge must not index past the grid either way.
        assert_eq!(
            region(motu::Vec3::new(2.0, 2.0, 0.0)),
            last * GROUPS as usize + last
        );
    }

    /// The sphere has to hold every point it was built from, or a group could
    /// be hidden with a plant of its own still in range.
    #[test]
    fn the_group_sphere_holds_every_plant() {
        let points = [
            Vec3::new(-100.0, 4.0, -100.0),
            Vec3::new(120.0, 60.0, 30.0),
            Vec3::new(0.0, -2.0, 110.0),
        ];
        let (centre, radius) = sphere(&points).expect("three points make a sphere");
        for point in points {
            assert!(centre.distance(point) <= radius + 1.0e-3, "{point:?}");
        }
        // And a region the generator planted nothing in needs no group.
        assert!(sphere(&[]).is_none());
        // The sphere is a fraction of the island, or grouping would cull
        // nothing: a region is one chunk square across.
        assert!(radius < ISLAND_WORLD_METRES / f32::from(u8::try_from(GROUPS).unwrap()));
    }
}
