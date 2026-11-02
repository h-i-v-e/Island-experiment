//! Shared Unity tree models on radial roots with distance-based detail and canopies.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub(crate) mod mask;
mod material;
mod models;
mod needles;
mod placement;
pub(crate) use placement::{mean_temperature, noise};
pub(crate) mod undergrowth;

use crate::{
    InitialOrbit, Orbit, Scene,
    command::{BedView, GenerationMode},
    explore::Explorer,
    orbit_transform,
    terrain_material::TerrainTextures,
};
use bevy::{
    light::NotShadowCaster,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use material::Prototype;
pub(crate) use material::{ForestMaterial, create_material};
use placement::{Built, Plant};
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct ForestPlugin;
impl Plugin for ForestPlugin {
    fn build(&self, app: &mut App) {
        material::register(app);
        app.init_resource::<ForestSettings>()
            .init_resource::<Forests>();
    }
}
#[allow(clippy::struct_excessive_bools)] // Independent vegetation visibility controls.
#[derive(Resource, Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ForestSettings {
    pub enabled: bool,
    pub distance: f32,
    pub density: f32,
    pub ferns: bool,
    pub fallen_logs: bool,
    pub pine_needles: bool,
}
impl Default for ForestSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            distance: 240.0,
            density: 2.0,
            ferns: true,
            fallen_logs: true,
            pine_needles: true,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
struct Key {
    pointer: usize,
    seed: u64,
    bed: BedView,
    mode: GenerationMode,
    density: u32,
}
struct Group {
    entity: Entity,
    direction: Vec3,
}
struct NearTree {
    entities: [Entity; 2],
    shell: Option<Entity>,
    fine_wood: bool,
    shells: bool,
}

#[allow(clippy::struct_excessive_bools)] // Independent upload, visibility, toggle and inspection state.
#[derive(Resource, Default)]
pub struct Forests {
    key: Option<Key>,
    density_edit: Option<(u32, std::time::Instant)>,
    job: Option<Task<Built>>,
    plants: Vec<Plant>,
    pub(crate) undergrowth: undergrowth::World,
    prototypes: Vec<Prototype>,
    materials: Option<[Handle<ForestMaterial>; 4]>,
    groups: BTreeMap<u32, Group>,
    canopies: Vec<Entity>,
    uploaded: usize,
    near: HashMap<usize, NearTree>,
    wanted: Vec<usize>,
    eye: Option<Vec3>,
    range: u32,
    needle_detail: bool,
    pixels: Vec<u8>,
    upper: f32,
    occluder: f32,
    pending: bool,
    was_enabled: bool,
    pub trees: usize,
    pub conifers: usize,
    pub detailed: usize,
    pub inspect: bool,
    pub inspect_conifers: bool,
    pub inspect_needles: bool,
}
impl Forests {
    pub fn with_view(view: crate::command::View) -> Self {
        let mut result = Self {
            inspect: matches!(
                view,
                crate::command::View::Forest
                    | crate::command::View::Conifers
                    | crate::command::View::Needles
            ),
            inspect_conifers: matches!(
                view,
                crate::command::View::Conifers | crate::command::View::Needles
            ),
            inspect_needles: view == crate::command::View::Needles,
            ..default()
        };
        result.undergrowth.inspect = view == crate::command::View::Undergrowth;
        result
    }
    pub fn simplified_trees(&self) -> usize {
        self.canopies.len()
            - self
                .near
                .keys()
                .filter(|&&id| id < self.canopies.len())
                .count()
    }
    pub fn settled(&self) -> bool {
        self.job.is_none() && !self.pending && !self.inspect && self.undergrowth.settled()
    }

    fn clear(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<ForestMaterial>,
    ) {
        for (_, group) in std::mem::take(&mut self.groups) {
            commands.entity(group.entity).despawn();
        }
        self.canopies.clear();
        self.clear_near(commands);
        self.undergrowth.clear(commands, meshes, materials);
        for model in self.prototypes.drain(..) {
            model.remove(meshes);
        }
        if let Some(handles) = self.materials.take() {
            for handle in handles {
                materials.remove(handle.id());
            }
        }
        self.plants.clear();
        self.pixels.clear();
        self.wanted.clear();
        self.uploaded = 0;
        self.trees = 0;
        self.conifers = 0;
        self.eye = None;
        self.job = None;
        self.pending = false;
        // Fresh crown materials must receive saved detail flags after a rebuild,
        // even when the camera, range and needle toggle have not changed.
        self.was_enabled = false;
    }
    fn clear_near(&mut self, commands: &mut Commands) {
        self.undergrowth.hide(commands);
        for (id, tree) in self.near.drain() {
            if let Some(&canopy) = self.canopies.get(id) {
                commands.entity(canopy).insert(Visibility::Inherited);
            }
            for entity in tree.entities.into_iter().chain(tree.shell) {
                commands.entity(entity).despawn();
            }
        }
        self.detailed = 0;
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn update(
    mut commands: Commands,
    scene: Res<Scene>,
    settings: Res<ForestSettings>,
    textures: Res<TerrainTextures>,
    mut world: ResMut<Forests>,
    mut explorer: ResMut<Explorer>,
    mut cameras: Query<(&mut Orbit, &mut Transform)>,
    mut initial: ResMut<InitialOrbit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ForestMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Ok((mut orbit, mut camera)) = cameras.single_mut() else {
        return;
    };
    let key = Key {
        pointer: scene.terrain.elevations.as_ptr() as usize,
        seed: scene.seed,
        bed: scene.bed,
        mode: scene.generation,
        density: settings.density.to_bits(),
    };
    let key = generation_key(&mut world, key);
    if world.key != Some(key) {
        world.clear(&mut commands, &mut meshes, &mut materials);
        if let Some(logs) = world.undergrowth.colliders() {
            explorer.terrain_ground(&scene).set_logs(logs);
        }
        world.key = Some(key);
        if let Some(mut image) = images.get_mut(&textures.forest_mask) {
            *image = mask::image(vec![0; mask::PIXELS]);
        }
    }
    if !settings.enabled || scene.generation != GenerationMode::Terrain || scene.terrain.is_moon() {
        if world.was_enabled {
            world.clear_near(&mut commands);
            for group in world.groups.values() {
                commands.entity(group.entity).insert(Visibility::Hidden);
            }
            if let Some(mut image) = images.get_mut(&textures.forest_mask) {
                *image = mask::image(vec![0; mask::PIXELS]);
            }
        }
        if let Some(logs) = world.undergrowth.colliders() {
            explorer.terrain_ground(&scene).set_logs(logs);
        }
        world.was_enabled = false;
        world.pending = false;
        world.inspect = false;
        world.undergrowth.inspect = false;
        return;
    }
    if !world.was_enabled
        && !world.pixels.is_empty()
        && let Some(mut image) = images.get_mut(&textures.forest_mask)
    {
        *image = mask::image(world.pixels.clone());
    }
    if world.job.is_none() && world.prototypes.is_empty() {
        // This snapshot only clones the Arc, retaining this generation while
        // the worker owns root queries and prototype construction.
        let source = scene.as_ref().clone();
        let query = explorer.shared_ground(&scene);
        let settings_density = settings.density;
        world.job = Some(
            AsyncComputeTaskPool::get()
                .spawn(async move { placement::build(source, query, settings_density) }),
        );
    }
    if let Some(job) = &mut world.job
        && let Some(built) = block_on(poll_once(job))
    {
        world.job = None;
        world.undergrowth.upload(
            built.undergrowth,
            scene.terrain.topology.radius as f32,
            &textures,
            &mut meshes,
            &mut materials,
        );
        explorer.adopt_ground(&scene, built.ground);
        world.upper = built.upper;
        world.occluder = built.occluder;
        if let Some(mut image) = images.get_mut(&textures.forest_mask) {
            *image = mask::image(built.pixels.clone());
        }
        world.pixels = built.pixels;
        world.plants = built.plants;
        world.trees = world.plants.len();
        world.conifers = world
            .plants
            .iter()
            .filter(|p| p.variant >= placement::VARIANTS)
            .count();
        world.prototypes = built
            .prototypes
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                if i >= placement::VARIANTS {
                    material::upload_species(p, true, &mut meshes)
                } else {
                    material::upload(p, &mut meshes)
                }
            })
            .collect();
        world.materials = Some(material::create_materials(
            scene.terrain.topology.radius as f32,
            &textures,
            &mut materials,
        ));
        info!(
            "Planet forest: {} terrain-fitted trees, {} shared Unity variants",
            world.trees,
            world.prototypes.len()
        );
    }
    if world.prototypes.is_empty() {
        world.pending = true;
        return;
    }
    let radius = scene.terrain.topology.radius as f32;
    if world.undergrowth.inspect {
        world.undergrowth.inspect = false;
        if let Some(view) = world.undergrowth.inspection(radius, &world.plants) {
            *orbit = view;
            *camera = orbit_transform(&orbit);
            initial.0 = *orbit;
            explorer.return_to_orbit = explorer.navigating();
        }
    }
    world.undergrowth.update(
        camera.translation,
        radius,
        &settings,
        &mut commands,
        &mut materials,
    );
    if let Some(logs) = world.undergrowth.colliders() {
        explorer.terrain_ground(&scene).set_logs(logs);
    }
    if world.inspect {
        world.inspect = false;
        if let Some(plant) = world
            .plants
            .iter()
            .filter(|p| !world.inspect_conifers || p.variant >= placement::VARIANTS)
            .max_by(|a, b| {
                let score = |p: &Plant| {
                    let up = p.root.normalize();
                    let light = up.dot(Vec3::new(-3.0, 4.0, 5.0).normalize());
                    let cold = if world.inspect_conifers {
                        placement::conifer_weight(placement::mean_temperature(
                            up,
                            (p.root.length() - 1.0) * radius,
                            scene.seed,
                        )) * 0.6
                    } else {
                        0.0
                    };
                    light + cold
                };
                score(a).total_cmp(&score(b))
            })
        {
            *orbit = inspection(plant, radius, world.inspect_needles, &world.plants);
            *camera = orbit_transform(&orbit);
            initial.0 = *orbit;
            explorer.return_to_orbit = explorer.navigating();
        }
    }
    let moved = world
        .eye
        .is_none_or(|eye| eye.distance(camera.translation) * radius > 3.0);
    let changed = !world.was_enabled
        || world.range != settings.distance.to_bits()
        || world.needle_detail != settings.pine_needles;
    world.needle_detail = settings.pine_needles;
    world.range = settings.distance.to_bits();
    world.was_enabled = true;
    let uploading = world.uploaded < world.plants.len();
    upload_canopies(&mut world, &mut commands);
    if changed && let Some(handles) = &world.materials {
        for handle in &handles[1..=2] {
            if let Some(mut material) = materials.get_mut(handle) {
                material.extension.settings.shape.w = f32::from(u8::from(settings.pine_needles));
            }
        }
    }
    if (moved || changed)
        && let Some(handles) = &world.materials
        && let Some(mut material) = materials.get_mut(&handles[2])
    {
        material.extension.settings.viewer = camera.translation.extend(0.0);
    }
    if moved || changed || uploading {
        world.eye = Some(camera.translation);
        let eye = camera.translation;
        let max_radius = world.upper + 25.0 / radius;
        for group in world.groups.values() {
            let visible = horizon_visible(eye, group.direction, world.occluder, max_radius);
            commands.entity(group.entity).insert(if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            });
        }
    }
    if moved || changed {
        world.wanted = world
            .plants
            .iter()
            .enumerate()
            .filter_map(|(id, plant)| {
                (plant.root.distance(camera.translation) * radius < settings.distance).then_some(id)
            })
            .collect();
        let mut wanted = std::mem::take(&mut world.wanted);
        let plants = &world.plants;
        wanted.sort_unstable_by(|&a, &b| {
            plants[a]
                .root
                .distance_squared(camera.translation)
                .total_cmp(&plants[b].root.distance_squared(camera.translation))
        });
        wanted.truncate(96);
        world.wanted = wanted;
    }
    if moved || changed || world.wanted.iter().any(|id| !world.near.contains_key(id)) {
        update_near(&mut world, camera.translation, radius, &mut commands);
    }
    world.pending = world.uploaded < world.plants.len()
        || world.wanted.iter().any(|id| !world.near.contains_key(id));
}

fn generation_key(world: &mut Forests, mut target: Key) -> Key {
    if let Some(previous) = world.key
        && previous.pointer == target.pointer
        && previous.seed == target.seed
        && previous.bed == target.bed
        && previous.mode == target.mode
        && previous.density != target.density
    {
        if world
            .density_edit
            .is_none_or(|(bits, _)| bits != target.density)
        {
            world.density_edit = Some((target.density, std::time::Instant::now()));
        }
        if world
            .density_edit
            .is_some_and(|(_, when)| when.elapsed() < std::time::Duration::from_millis(350))
        {
            target.density = previous.density;
        }
    } else {
        world.density_edit = None;
    }
    target
}

fn horizon_visible(eye: Vec3, direction: Vec3, occluder: f32, outer: f32) -> bool {
    let angle = (occluder / eye.length().max(occluder))
        .clamp(0.0, 1.0)
        .acos()
        + (occluder / outer.max(occluder)).clamp(0.0, 1.0).acos()
        + 0.25;
    eye.normalize_or_zero().dot(direction) >= angle.min(std::f32::consts::PI).cos()
}

fn upload_canopies(world: &mut Forests, commands: &mut Commands) {
    let end = (world.uploaded + 192).min(world.plants.len());
    for index in world.uploaded..end {
        let plant = &world.plants[index];
        let group = world.groups.entry(plant.owner).or_insert_with(|| Group {
            entity: commands
                .spawn((
                    Transform::default(),
                    Visibility::Hidden,
                    Name::new("Distant forest cell"),
                ))
                .id(),
            direction: plant.root.normalize(),
        });
        let canopy = commands
            .spawn((
                Mesh3d(world.prototypes[plant.variant].canopy.clone()),
                MeshMaterial3d(world.materials.as_ref().unwrap()[3].clone()),
                plant.transform,
                NotShadowCaster,
                ChildOf(group.entity),
                if world.near.contains_key(&index) {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                },
            ))
            .id();
        world.canopies.push(canopy);
    }
    world.uploaded = end;
}

fn update_near(world: &mut Forests, eye: Vec3, radius: f32, commands: &mut Commands) {
    let wanted: HashSet<_> = world.wanted.iter().copied().collect();
    world.near.retain(|id, tree| {
        if wanted.contains(id) {
            true
        } else {
            if let Some(&canopy) = world.canopies.get(*id) {
                commands.entity(canopy).insert(Visibility::Inherited);
            }
            for entity in tree.entities.into_iter().chain(tree.shell) {
                commands.entity(entity).despawn();
            }
            false
        }
    });
    let mut spawned = 0;
    for (rank, &id) in world.wanted.iter().enumerate() {
        let plant = &world.plants[id];
        let model = &world.prototypes[plant.variant];
        let distance = plant.root.distance(eye) * radius;
        let fine = distance < 30.0;
        let shells = rank < 12
            && distance < if model.conifer { 32.0 } else { 38.0 }
            && (!model.conifer || world.needle_detail);
        let wood = if fine { &model.wood } else { &model.branches };
        if let Some(tree) = world.near.get_mut(&id) {
            if tree.fine_wood != fine {
                tree.fine_wood = fine;
                commands
                    .entity(tree.entities[0])
                    .insert(Mesh3d(wood.clone()));
            }
            if tree.shells != shells {
                tree.shells = shells;
                if let Some(entity) = tree.shell {
                    commands.entity(entity).insert(if shells {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    });
                }
            }
        } else if spawned < 4 {
            if let Some(&canopy) = world.canopies.get(id) {
                commands.entity(canopy).insert(Visibility::Hidden);
            }
            let handles = world.materials.as_ref().unwrap();
            let entities = std::array::from_fn(|kind| {
                let mesh = if kind == 0 { wood } else { &model.leaves };
                commands
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(handles[kind].clone()),
                        plant.transform,
                    ))
                    .id()
            });
            let shell = model.shells.as_ref().map(|mesh| {
                let mut entity = commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(handles[2].clone()),
                    plant.transform,
                    if shells {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    },
                ));
                if !model.conifer {
                    entity.insert(NotShadowCaster);
                }
                entity.id()
            });
            world.near.insert(
                id,
                NearTree {
                    entities,
                    shell,
                    fine_wood: fine,
                    shells,
                },
            );
            spawned += 1;
        }
    }
    world.detailed = world.near.len();
}

fn inspection(plant: &Plant, radius: f32, needles: bool, plants: &[Plant]) -> Orbit {
    if needles {
        return needle_inspection(plant, radius, plants);
    }
    let up = plant.root.normalize();
    Orbit {
        focus: plant.root
            + up * (if plant.variant >= placement::VARIANTS {
                13.0
            } else {
                7.0
            } / radius),
        frame: Quat::from_rotation_arc(Vec3::Y, up),
        distance: if plant.variant >= placement::VARIANTS {
            38.0
        } else {
            22.0
        } / radius,
        yaw: 0.5,
        pitch: -0.15,
        minimum: 0.5 / radius,
        maximum: 0.5,
    }
}

fn needle_inspection(plant: &Plant, radius: f32, plants: &[Plant]) -> Orbit {
    let up = plant.root.normalize();
    let frame = Quat::from_rotation_arc(Vec3::Y, up);
    let light = frame.inverse() * Vec3::new(-3.0, 4.0, 5.0).normalize();
    let scale = plant.transform.scale.x * radius / motu::ISLAND_WORLD_METRES;
    let mut view = Orbit {
        focus: plant.root + up * (4.5 * scale / radius),
        frame,
        distance: 4.8 * scale / radius,
        yaw: light.x.atan2(light.z),
        pitch: 0.03,
        minimum: 0.3 / radius,
        maximum: 0.5,
    };
    // Find a clear approach instead of putting the viewer inside a neighbour's
    // canopy. This runs only on an explicit inspection request.
    view.yaw = (0..16)
        .map(|step| view.yaw + step as f32 * std::f32::consts::TAU / 16.0)
        .max_by(|&a, &b| {
            let score = |yaw| {
                let candidate = Orbit { yaw, ..view };
                let eye = orbit_transform(&candidate).translation;
                let clearance = plants
                    .iter()
                    .filter(|p| {
                        p.root.distance(plant.root) * radius > 0.1
                            && p.root.distance(eye) * radius < 25.0
                    })
                    .map(|p| {
                        let other_scale = p.transform.scale.x * radius / motu::ISLAND_WORLD_METRES;
                        let delta = (eye - p.root) * radius;
                        let height = delta.dot(p.root.normalize());
                        if height < 1.6 * other_scale || height > 22.0 * other_scale {
                            return 20.0;
                        }
                        let side = (delta - p.root.normalize() * height).length();
                        let width = if p.variant >= placement::VARIANTS {
                            3.8 * other_scale * (1.0 - height / (20.0 * other_scale)).max(0.1)
                        } else {
                            5.0 * other_scale
                        };
                        side - width
                    })
                    .fold(20.0, f32::min);
                clearance.min(4.0) + (yaw - view.yaw).cos() * 0.5
            };
            score(a).total_cmp(&score(b))
        })
        .unwrap_or(view.yaw);
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    fn empty_prototype(models: &mut Assets<Mesh>) -> Prototype {
        use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology};
        let mut make = || {
            models.add(Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            ))
        };
        Prototype {
            wood: make(),
            branches: make(),
            leaves: make(),
            shells: Some(make()),
            canopy: make(),
            conifer: false,
        }
    }

    fn forest_app(count: usize) -> (App, Entity, Entity) {
        let scene = crate::collision::test_scene();
        let key = Key {
            pointer: scene.terrain.elevations.as_ptr() as usize,
            seed: scene.seed,
            bed: scene.bed,
            mode: scene.generation,
            density: ForestSettings::default().density.to_bits(),
        };
        let orbit = crate::initial_orbit(crate::command::View::Front, &scene.terrain);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(scene)
            .insert_resource(InitialOrbit(orbit))
            .init_resource::<Explorer>()
            .init_resource::<ForestSettings>()
            .init_resource::<TerrainTextures>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<ForestMaterial>>()
            .init_resource::<Assets<Image>>()
            .add_systems(Update, update);
        let prototype = empty_prototype(&mut app.world_mut().resource_mut::<Assets<Mesh>>());
        app.insert_resource(Forests {
            key: Some(key),
            upper: 1.02,
            occluder: 0.92,
            plants: (0..count)
                .map(|_| Plant {
                    root: Vec3::Y * 1.02,
                    transform: Transform::from_translation(Vec3::Y * 1.02),
                    owner: 0,
                    variant: 0,
                })
                .collect(),
            prototypes: vec![prototype],
            materials: Some(std::array::from_fn(|_| Handle::default())),
            ..default()
        });
        let camera = app
            .world_mut()
            .spawn((
                orbit,
                Transform::from_translation(Vec3::Y * 2.5).looking_to(-Vec3::Y, Vec3::Z),
            ))
            .id();
        app.update();
        let group = app.world().resource::<Forests>().groups[&0].entity;
        (app, camera, group)
    }

    #[test]
    fn distance_lods_handover_independently_of_view_direction_and_clear_on_regeneration() {
        let (mut app, camera, group) = forest_app(1);
        let canopy = app.world().resource::<Forests>().canopies[0];
        assert_eq!(
            app.world().get::<Visibility>(group),
            Some(&Visibility::Visible)
        );
        assert_eq!(
            app.world().get::<Visibility>(canopy),
            Some(&Visibility::Inherited)
        );
        // Looking away in orbit must not change the selected distance LOD.
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .look_to(Vec3::Z, Vec3::Y);
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(group),
            Some(&Visibility::Visible)
        );
        assert_eq!(app.world().resource::<Forests>().simplified_trees(), 1);
        // A distant tree stays simplified when looking horizontally at ground level.
        app.world_mut().entity_mut(camera).insert(
            Transform::from_translation(Vec3::Y * 1.025 + Vec3::X * 0.3)
                .looking_to(Vec3::Z, Vec3::Y),
        );
        app.update();
        assert_eq!(app.world().resource::<Forests>().simplified_trees(), 1);
        assert!(app.world().resource::<Forests>().near.is_empty());
        // Nearby trees replace only their own canopy, preserving cell culling.
        app.world_mut()
            .entity_mut(camera)
            .insert(Transform::from_translation(Vec3::Y * 1.025).looking_to(Vec3::Z, Vec3::Y));
        app.update();
        let leaves = app.world().resource::<Forests>().near[&0].entities[1];
        let fine_leaves = &app.world().resource::<Forests>().prototypes[0].leaves;
        assert_eq!(&app.world().get::<Mesh3d>(leaves).unwrap().0, fine_leaves);
        assert!(app.world().resource::<Forests>().near[&0].fine_wood);
        assert_eq!(
            app.world().get::<Visibility>(canopy),
            Some(&Visibility::Hidden)
        );
        assert_eq!(app.world().resource::<Forests>().simplified_trees(), 0);
        // Move beyond fine wood range while retaining detailed foliage.
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation
            .x = 0.1;
        app.update();
        let tree = &app.world().resource::<Forests>().near[&0];
        assert!(!tree.fine_wood);
        assert_eq!(
            &app.world().get::<Mesh3d>(tree.entities[0]).unwrap().0,
            &app.world().resource::<Forests>().prototypes[0].branches
        );
        // Leaving the detailed range restores the coarse canopy in the same update.
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation
            .x = 0.3;
        app.update();
        assert!(app.world().get_entity(leaves).is_err());
        assert_eq!(
            app.world().get::<Visibility>(canopy),
            Some(&Visibility::Inherited)
        );
        // Disabling and re-enabling forests must not leave canopies hidden.
        app.world_mut().resource_mut::<ForestSettings>().enabled = false;
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(group),
            Some(&Visibility::Hidden)
        );
        app.world_mut().resource_mut::<ForestSettings>().enabled = true;
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(group),
            Some(&Visibility::Visible)
        );
        // A replacement terrain must not retain old roots or shared meshes.
        app.world_mut().resource_mut::<ForestSettings>().enabled = false;
        app.insert_resource(crate::collision::test_scene());
        app.update();
        assert!(app.world().get_entity(canopy).is_err());
        assert!(app.world().get_entity(group).is_err());
        assert!(app.world().resource::<Forests>().plants.is_empty());
        assert!(app.world().resource::<Forests>().prototypes.is_empty());
        assert!(
            !app.world().resource::<Forests>().was_enabled,
            "new crown materials must reapply saved needle flags"
        );
    }

    #[test]
    fn coarse_canopies_cover_trees_waiting_for_bounded_detail_uploads() {
        let (mut app, camera, _) = forest_app(12);
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation = Vec3::Y * 1.025;
        app.update();
        assert_eq!(app.world().resource::<Forests>().near.len(), 4);
        assert_eq!(app.world().resource::<Forests>().simplified_trees(), 8);
        let world = app.world().resource::<Forests>();
        for (id, &canopy) in world.canopies.iter().enumerate() {
            let expected = if world.near.contains_key(&id) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            assert_eq!(app.world().get::<Visibility>(canopy), Some(&expected));
        }
        app.update();
        assert_eq!(app.world().resource::<Forests>().near.len(), 8);
        app.update();
        assert_eq!(app.world().resource::<Forests>().near.len(), 12);
        assert_eq!(app.world().resource::<Forests>().simplified_trees(), 0);
        app.world_mut().resource_mut::<ForestSettings>().distance = 1.0;
        app.update();
        assert!(app.world().resource::<Forests>().near.is_empty());
        assert_eq!(app.world().resource::<Forests>().simplified_trees(), 12);
    }

    #[test]
    fn pine_volume_appears_on_first_upload_obeys_the_shared_budget_and_toggles_live() {
        let (mut app, camera, _) = forest_app(20);
        app.world_mut().resource_mut::<Forests>().prototypes[0].conifer = true;
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation = Vec3::Y * 1.025;
        app.update();
        let first = app.world().resource::<Forests>().near[&0].shell.unwrap();
        assert_eq!(
            app.world().get::<Visibility>(first),
            Some(&Visibility::Visible)
        );
        assert!(app.world().get::<NotShadowCaster>(first).is_none());
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<Forests>()
                .near
                .values()
                .filter(|t| t.shells)
                .count(),
            12
        );
        app.world_mut()
            .resource_mut::<ForestSettings>()
            .pine_needles = false;
        app.update();
        assert!(
            app.world()
                .resource::<Forests>()
                .near
                .values()
                .all(|t| !t.shells)
        );
        assert_eq!(
            app.world().get::<Visibility>(first),
            Some(&Visibility::Hidden)
        );
        app.world_mut()
            .resource_mut::<ForestSettings>()
            .pine_needles = true;
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(first),
            Some(&Visibility::Visible)
        );
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation
            .x = 0.06;
        app.update();
        assert!(
            app.world()
                .resource::<Forests>()
                .near
                .values()
                .all(|t| !t.shells)
        );
        app.world_mut().resource_mut::<ForestSettings>().enabled = false;
        app.update();
        assert!(app.world().get_entity(first).is_err());
    }

    #[test]
    fn dense_stands_keep_a_fixed_near_detail_budget_and_reach_a_settled_state() {
        let (mut app, camera, _) = forest_app(160);
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation = Vec3::Y * 1.025;
        for _ in 0..32 {
            app.update();
        }
        let world = app.world().resource::<Forests>();
        assert_eq!(world.detailed, 96);
        assert_eq!(world.simplified_trees(), 64);
        assert_eq!(world.near.values().filter(|tree| tree.shells).count(), 12);
        assert!(world.settled());
    }
    #[test]
    fn conservative_canopy_culling_keeps_near_cells_and_hides_far_side() {
        assert!(horizon_visible(Vec3::Y * 2.5, Vec3::Y, 0.92, 1.10));
        assert!(!horizon_visible(Vec3::Y * 2.5, -Vec3::Y, 0.92, 1.10));
        assert!(horizon_visible(Vec3::Y * 2.5, Vec3::X, 0.92, 1.10));
    }
}
