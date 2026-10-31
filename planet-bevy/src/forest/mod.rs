//! Shared Unity tree models on radial roots with distance-based detail and canopies.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub(crate) mod mask;
mod material;
mod placement;

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
use material::{ForestMaterial, Prototype};
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
#[derive(Resource, Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ForestSettings {
    pub enabled: bool,
    pub distance: f32,
}
impl Default for ForestSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            distance: 240.0,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
struct Key {
    pointer: usize,
    seed: u64,
    bed: BedView,
    mode: GenerationMode,
}
struct Group {
    entity: Entity,
    direction: Vec3,
}
struct NearTree {
    entities: [Entity; 3],
    fine_wood: bool,
    shells: bool,
}

#[allow(clippy::struct_excessive_bools)] // Independent upload, visibility, toggle and inspection state.
#[derive(Resource, Default)]
pub struct Forests {
    key: Option<Key>,
    job: Option<Task<Built>>,
    plants: Vec<Plant>,
    prototypes: Vec<Prototype>,
    materials: Option<[Handle<ForestMaterial>; 4]>,
    groups: BTreeMap<u32, Group>,
    canopies: Vec<Entity>,
    uploaded: usize,
    near: HashMap<usize, NearTree>,
    wanted: Vec<usize>,
    eye: Option<Vec3>,
    range: u32,
    pixels: Vec<u8>,
    upper: f32,
    occluder: f32,
    pending: bool,
    was_enabled: bool,
    pub trees: usize,
    pub detailed: usize,
    pub inspect: bool,
}
impl Forests {
    pub fn with_inspection(inspect: bool) -> Self {
        Self {
            inspect,
            ..default()
        }
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
        self.job.is_none() && !self.pending && !self.inspect
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
        self.eye = None;
        self.job = None;
        self.pending = false;
    }
    fn clear_near(&mut self, commands: &mut Commands) {
        for (id, tree) in self.near.drain() {
            if let Some(&canopy) = self.canopies.get(id) {
                commands.entity(canopy).insert(Visibility::Inherited);
            }
            for entity in tree.entities {
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
    };
    if world.key != Some(key) {
        world.clear(&mut commands, &mut meshes, &mut materials);
        world.key = Some(key);
        if let Some(mut image) = images.get_mut(&textures.forest_mask) {
            *image = mask::image(vec![0; mask::PIXELS]);
        }
    }
    if !settings.enabled || scene.generation != GenerationMode::Terrain {
        if world.was_enabled {
            world.clear_near(&mut commands);
            for group in world.groups.values() {
                commands.entity(group.entity).insert(Visibility::Hidden);
            }
            if let Some(mut image) = images.get_mut(&textures.forest_mask) {
                *image = mask::image(vec![0; mask::PIXELS]);
            }
        }
        world.was_enabled = false;
        world.pending = false;
        world.inspect = false;
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
        world.job =
            Some(AsyncComputeTaskPool::get().spawn(async move { placement::build(source, query) }));
    }
    if let Some(job) = &mut world.job
        && let Some(built) = block_on(poll_once(job))
    {
        world.job = None;
        explorer.adopt_ground(&scene, built.ground);
        world.upper = built.upper;
        world.occluder = built.occluder;
        if let Some(mut image) = images.get_mut(&textures.forest_mask) {
            *image = mask::image(built.pixels.clone());
        }
        world.pixels = built.pixels;
        world.plants = built.plants;
        world.trees = world.plants.len();
        world.prototypes = built
            .prototypes
            .into_iter()
            .map(|p| material::upload(p, &mut meshes))
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
    if world.inspect {
        world.inspect = false;
        if let Some(plant) = world.plants.iter().max_by(|a, b| {
            let sun = Vec3::new(-3.0, 4.0, 5.0).normalize();
            a.root
                .normalize()
                .dot(sun)
                .total_cmp(&b.root.normalize().dot(sun))
        }) {
            *orbit = inspection(plant, radius);
            *camera = orbit_transform(&orbit);
            initial.0 = *orbit;
            explorer.return_to_orbit = explorer.walking();
        }
    }
    let moved = world
        .eye
        .is_none_or(|eye| eye.distance(camera.translation) * radius > 3.0);
    let changed = !world.was_enabled || world.range != settings.distance.to_bits();
    world.range = settings.distance.to_bits();
    world.was_enabled = true;
    let uploading = world.uploaded < world.plants.len();
    upload_canopies(&mut world, &mut commands);
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
        world.wanted = wanted;
    }
    update_near(&mut world, camera.translation, radius, &mut commands);
    world.pending = world.uploaded < world.plants.len()
        || world.wanted.iter().any(|id| !world.near.contains_key(id));
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
            for entity in tree.entities {
                commands.entity(entity).despawn();
            }
            false
        }
    });
    let mut spawned = 0;
    for &id in &world.wanted {
        let plant = &world.plants[id];
        let model = &world.prototypes[plant.variant];
        let fine = plant.root.distance(eye) * radius < 30.0;
        let wood = if fine { &model.wood } else { &model.branches };
        if let Some(tree) = world.near.get_mut(&id) {
            if tree.fine_wood != fine {
                tree.fine_wood = fine;
                commands
                    .entity(tree.entities[0])
                    .insert(Mesh3d(wood.clone()));
            }
            let shells = plant.root.distance(eye) * radius < 38.0;
            if tree.shells != shells {
                tree.shells = shells;
                commands.entity(tree.entities[2]).insert(if shells {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                });
            }
        } else if spawned < 4 {
            if let Some(&canopy) = world.canopies.get(id) {
                commands.entity(canopy).insert(Visibility::Hidden);
            }
            let handles = world.materials.as_ref().unwrap();
            let entities = std::array::from_fn(|kind| {
                let mesh = match kind {
                    0 => wood,
                    1 => &model.leaves,
                    _ => &model.shells,
                };
                let mut entity = commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(handles[kind].clone()),
                    plant.transform,
                ));
                if kind == 2 {
                    entity.insert((NotShadowCaster, Visibility::Hidden));
                }
                entity.id()
            });
            world.near.insert(
                id,
                NearTree {
                    entities,
                    fine_wood: fine,
                    shells: false,
                },
            );
            spawned += 1;
        }
    }
    world.detailed = world.near.len();
}

fn inspection(plant: &Plant, radius: f32) -> Orbit {
    let up = plant.root.normalize();
    Orbit {
        focus: plant.root + up * (7.0 / radius),
        frame: Quat::from_rotation_arc(Vec3::Y, up),
        distance: 22.0 / radius,
        yaw: 0.5,
        pitch: -0.15,
        minimum: 0.5 / radius,
        maximum: 0.5,
    }
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
            shells: make(),
            canopy: make(),
        }
    }

    fn forest_app(count: usize) -> (App, Entity, Entity) {
        let scene = crate::collision::test_scene();
        let key = Key {
            pointer: scene.terrain.elevations.as_ptr() as usize,
            seed: scene.seed,
            bed: scene.bed,
            mode: scene.generation,
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
    fn conservative_canopy_culling_keeps_near_cells_and_hides_far_side() {
        assert!(horizon_visible(Vec3::Y * 2.5, Vec3::Y, 0.92, 1.10));
        assert!(!horizon_visible(Vec3::Y * 2.5, -Vec3::Y, 0.92, 1.10));
        assert!(horizon_visible(Vec3::Y * 2.5, Vec3::X, 0.92, 1.10));
    }
}
