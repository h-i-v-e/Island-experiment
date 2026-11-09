//! Explicit, metre-scale Building Studio placements on the spherical terrain.
// Planet radii are validated f64 values; this adapter uses the renderer's f32 units.
#![allow(clippy::cast_possible_truncation)]

use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, IoTaskPool, Task, futures::check_ready},
};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui, input::EguiWantsInput};
use island_building::{
    BuildingPrototype, BuildingRecipe, generate_with_base,
    interaction::{
        BuildingInstance, BuildingObserver, BuildingRuntimePlugin, BuildingRuntimeSet,
        BuildingSceneState,
    },
    render::{CompiledBuildingPrototype, compile_building, spawn_building},
};
use serde::{Deserialize, Serialize};

use crate::{Orbit, Scene, collision::Ground, explore::Explorer};

const MAX_PLACEMENTS: usize = 128;
const MAX_SCENE_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BuildingRuntimePlugin)
            .init_resource::<BuildingWorld>()
            .add_systems(
                Update,
                update
                    .after(crate::explore::update)
                    .before(BuildingRuntimeSet),
            )
            .add_systems(EguiPrimaryContextPass, panel);
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SavedPlacement {
    id: String,
    recipe: BuildingRecipe,
    material_base: PathBuf,
    translation: [f32; 3],
    rotation: [f32; 4],
    foundation_depth: f32,
    state: Option<BuildingSceneState>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SceneFile {
    version: u32,
    seed: u64,
    radius_metres: f64,
    terrain_hash: u64,
    placements: Vec<SavedPlacement>,
}

#[derive(Component)]
struct PlacedBuilding(SavedPlacement);

#[derive(Component)]
struct ExplorerObserver;

type ObserverQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static mut GlobalTransform,
        &'static mut BuildingObserver,
    ),
    (
        With<ExplorerObserver>,
        Without<Orbit>,
        Without<PlacedBuilding>,
    ),
>;

struct BuildJob {
    placement: SavedPlacement,
    task: Task<Result<BuildingPrototype, String>>,
}

// Only one completion is retained; keep its owned snapshot inline rather than heap-boxing it.
#[allow(clippy::large_enum_variant)]
enum FileCompletion {
    Recipe {
        path: String,
        recipe: BuildingRecipe,
        base: PathBuf,
        camera: Transform,
    },
    Loaded {
        path: PathBuf,
        bytes: Vec<u8>,
        file: SceneFile,
    },
    Saved {
        path: PathBuf,
        bytes: Vec<u8>,
    },
}

struct PendingLoad {
    path: PathBuf,
    bytes: Vec<u8>,
    next_id: u64,
    prepared: Vec<(SavedPlacement, Arc<CompiledBuildingPrototype>)>,
}

#[derive(Clone, Copy)]
enum Request {
    Place,
    Save,
    Load,
}

#[derive(Resource)]
pub(crate) struct BuildingWorld {
    recipe_path: String,
    scene_path: String,
    loaded_path: Option<PathBuf>,
    loaded_bytes: Option<Vec<u8>>,
    status: String,
    request: Option<Request>,
    pending: VecDeque<SavedPlacement>,
    job: Option<BuildJob>,
    loading: Option<PendingLoad>,
    file_job: Option<Task<Result<FileCompletion, String>>>,
    compiled: BTreeMap<String, Arc<CompiledBuildingPrototype>>,
    generation: Option<(usize, crate::command::BedView)>,
    next_id: u64,
}

impl Default for BuildingWorld {
    fn default() -> Self {
        Self {
            recipe_path: String::new(),
            scene_path: String::from("buildings.scene.json"),
            loaded_path: None,
            loaded_bytes: None,
            status: String::from("H places a hut on suitable ground. E opens doors."),
            request: None,
            pending: VecDeque::new(),
            job: None,
            loading: None,
            file_job: None,
            compiled: BTreeMap::new(),
            generation: None,
            next_id: 1,
        }
    }
}

impl BuildingWorld {
    pub fn with_recipe(path: Option<&Path>) -> Self {
        Self {
            recipe_path: path.map_or_else(String::new, |p| p.to_string_lossy().into_owned()),
            ..default()
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct BuildingAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    scene: Res<Scene>,
    keys: Res<ButtonInput<KeyCode>>,
    wants: Res<EguiWantsInput>,
    mut explorer: ResMut<Explorer>,
    forests: Res<crate::forest::Forests>,
    mut world: ResMut<BuildingWorld>,
    mut assets: BuildingAssets,
    cameras: Query<(&Transform, &Projection), With<Orbit>>,
    mut observers: ObserverQuery,
    placements: Query<(Entity, &Transform, &PlacedBuilding, &BuildingInstance)>,
) {
    let Ok((camera, projection)) = cameras.single() else {
        return;
    };
    let identity = (scene.terrain.elevations.as_ptr() as usize, scene.bed);
    if world.generation != Some(identity) {
        for (entity, ..) in &placements {
            commands.entity(entity).despawn();
        }
        world.pending.clear();
        world.job = None;
        world.loading = None;
        world.file_job = None;
        world.compiled.clear();
        world.generation = Some(identity);
    }
    sync_observer(
        &mut commands,
        &mut observers,
        &explorer,
        &scene,
        camera,
        projection,
    );
    if keys.just_pressed(KeyCode::KeyH)
        && !wants.wants_any_keyboard_input()
        && explorer.flight.is_none()
    {
        world.request = Some(Request::Place);
    }
    if let Some(result) = poll_file(&mut world) {
        let result = result.and_then(|completion| {
            finish_file(
                completion,
                &mut world,
                &scene,
                &mut explorer,
                &forests,
                &placements,
            )
        });
        if let Err(error) = result {
            world.status = error;
        }
    }
    if let Some(request) = world.request.take() {
        let result = if world.file_job.is_some() {
            Err(String::from(
                "Wait for the current file operation to finish.",
            ))
        } else {
            match request {
                Request::Place => request_placement(
                    &mut world,
                    &scene,
                    &mut explorer,
                    &forests,
                    camera,
                    &placements,
                ),
                Request::Save => save_scene(&mut world, &scene, &placements),
                Request::Load => load_scene(&mut world),
            }
        };
        if let Err(error) = result {
            world.status = error;
        }
    }
    finish_build(
        &mut world,
        &mut commands,
        &mut assets,
        scene.terrain.topology.radius as f32,
    );
    finish_load(
        &mut world,
        &mut commands,
        &mut assets,
        &placements,
        scene.terrain.topology.radius as f32,
    );
    start_build(&mut world);
}

fn sync_observer(
    commands: &mut Commands,
    observers: &mut ObserverQuery,
    explorer: &Explorer,
    scene: &Scene,
    camera: &Transform,
    projection: &Projection,
) {
    let radius = scene.terrain.topology.radius as f32;
    let observer = BuildingObserver {
        eye_height: if explorer.walking() {
            1.7 / radius
        } else {
            0.0
        },
        radius: 0.28 / radius,
        height: 1.8 / radius,
        view_direction: *camera.forward(),
        interaction_enabled: explorer.walking() && explorer.captured(),
        vertical_fov_radians: match projection {
            Projection::Perspective(lens) => lens.fov,
            _ => 60_f32.to_radians(),
        },
        ..default()
    };
    let feet = camera.translation - camera.translation.normalize_or_zero() * observer.eye_height;
    let transform = Transform::from_translation(feet);
    if let Ok((_, mut current, mut global, mut settings)) = observers.single_mut() {
        *current = transform;
        *global = GlobalTransform::from(transform);
        *settings = observer;
    } else {
        commands.spawn((
            ExplorerObserver,
            transform,
            GlobalTransform::from(transform),
            observer,
        ));
    }
}

fn read_recipe(path: &str) -> Result<(BuildingRecipe, PathBuf), String> {
    if path.trim().is_empty() {
        return Ok((BuildingRecipe::hut(), PathBuf::from(".")));
    }
    let path = Path::new(path);
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_SCENE_BYTES {
        return Err(String::from("Building recipe exceeds the 4 MiB limit."));
    }
    let bytes = fs::read(path).map_err(|e| format!("Could not read building recipe: {e}"))?;
    let recipe =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid building recipe: {e}"))?;
    let base = fs::canonicalize(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    )
    .map_err(|e| e.to_string())?;
    Ok((recipe, base))
}

fn request_placement(
    world: &mut BuildingWorld,
    scene: &Scene,
    explorer: &mut Explorer,
    forests: &crate::forest::Forests,
    camera: &Transform,
    placements: &Query<(Entity, &Transform, &PlacedBuilding, &BuildingInstance)>,
) -> Result<(), String> {
    if world.recipe_path.trim().is_empty() {
        return place_recipe(
            world,
            scene,
            explorer,
            forests,
            camera,
            placements,
            BuildingRecipe::hut(),
            PathBuf::from("."),
        );
    }
    let path = world.recipe_path.clone();
    let camera = *camera;
    world.file_job = Some(IoTaskPool::get().spawn(async move {
        let (recipe, base) = read_recipe(&path)?;
        Ok(FileCompletion::Recipe {
            path,
            recipe,
            base,
            camera,
        })
    }));
    world.status = String::from("Reading building recipe in the background.");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn place_recipe(
    world: &mut BuildingWorld,
    scene: &Scene,
    explorer: &mut Explorer,
    forests: &crate::forest::Forests,
    camera: &Transform,
    placements: &Query<(Entity, &Transform, &PlacedBuilding, &BuildingInstance)>,
    recipe: BuildingRecipe,
    material_base: PathBuf,
) -> Result<(), String> {
    let count = placements.iter().count();
    if !forests.settled() {
        return Err(String::from(
            "Wait for nearby vegetation to finish preparing before placing a building.",
        ));
    }
    if world.loading.is_some() {
        return Err(String::from(
            "Wait for the scene load to finish before placing another building.",
        ));
    }
    if count + world.pending.len() + usize::from(world.job.is_some()) >= MAX_PLACEMENTS {
        return Err(format!("Scene limit is {MAX_PLACEMENTS} buildings."));
    }
    island_building::validate(&recipe).map_err(|e| e.to_string())?;
    let walking = explorer.walking();
    let ground = explorer.terrain_ground(scene);
    let radius = scene.terrain.topology.radius as f32;
    let point = if walking {
        let up = camera.translation.normalize();
        let heading = (*camera.forward() - up * camera.forward().dot(up)).normalize_or_zero();
        let direction = (camera.translation + heading * (9.0 / radius)).normalize();
        ground
            .surface_sample(
                &scene.terrain,
                direction,
                scene.bed == crate::command::BedView::Original,
            )
            .map(|sample| sample.point)
    } else {
        ground.hit(&scene.terrain, camera.translation, *camera.forward(), 50.0)
    }
    .ok_or("Aim at a dry, level terrain patch before placing a building.")?;
    let (transform, foundation_depth) =
        fit_placement(scene, ground, &recipe, point, *camera.forward())?;
    if forests.placement_obstructed(
        &GlobalTransform::from(transform),
        Vec2::new(recipe.width + 1.0, recipe.depth + 2.0),
    ) {
        return Err(String::from(
            "The building or entrance approach is too close to a tree. Choose a clear patch.",
        ));
    }
    let separation = recipe.width.hypot(recipe.depth) / (2.0 * radius);
    let occupied = placements
        .iter()
        .map(|(_, _, saved, _)| &saved.0)
        .chain(world.pending.iter())
        .chain(world.job.iter().map(|job| &job.placement));
    if occupied.into_iter().any(|other| {
        transform
            .translation
            .distance(Vec3::from_array(other.translation))
            < separation + (other.recipe.width.hypot(other.recipe.depth) * 0.5 + 0.5) / radius
    }) {
        return Err(String::from(
            "The building footprint overlaps another placement.",
        ));
    }
    // Stable IDs are scene-local and never reused while placements exist.
    let id = format!("building-{}-{}", scene.seed, world.next_id);
    world.next_id += 1;
    world.pending.push_back(SavedPlacement {
        id,
        recipe,
        material_base,
        translation: transform.translation.to_array(),
        rotation: transform.rotation.to_array(),
        foundation_depth,
        state: None,
    });
    world.status = format!(
        "Generating building in the background (footprint radius {:.1} m).",
        separation * radius
    );
    Ok(())
}

fn fit_placement(
    scene: &Scene,
    ground: &Ground,
    recipe: &BuildingRecipe,
    point: Vec3,
    heading: Vec3,
) -> Result<(Transform, f32), String> {
    let radius = scene.terrain.topology.radius as f32;
    let up = point.normalize();
    let forward = (heading - up * heading.dot(up))
        .try_normalize()
        .unwrap_or_else(|| up.any_orthonormal_vector());
    let rotation = Transform::default().looking_to(forward, up).rotation;
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    // Include the complete footprint and a one-metre entrance approach.
    for ix in 0_u8..7 {
        for iz in 0_u8..7 {
            let local = Vec3::new(
                (f32::from(ix) / 6.0 - 0.5) * (recipe.width + 1.0),
                0.0,
                (f32::from(iz) / 6.0 - 0.5) * (recipe.depth + 2.0),
            );
            let direction = (point + rotation * local / radius).normalize();
            let sample = ground
                .surface_sample(
                    &scene.terrain,
                    direction,
                    scene.bed == crate::command::BedView::Original,
                )
                .ok_or("Terrain is unavailable under the complete building footprint.")?;
            if sample.clearance > 1.0e-8
                || (sample.point.length() <= 1.0 && !scene.terrain.is_moon())
            {
                return Err(String::from(
                    "Building footprint or entrance overlaps water.",
                ));
            }
            let level = (sample.point - point).dot(up) * radius;
            low = low.min(level);
            high = high.max(level);
        }
    }
    if high - low > 0.27 {
        return Err(String::from(
            "Ground is too uneven for an accessible entrance. Choose a flatter patch.",
        ));
    }
    let transform = Transform {
        translation: point + up * ((high + 0.05) / radius),
        rotation,
        scale: Vec3::splat(radius.recip()),
    };
    if ground.placement_obstructed(
        &GlobalTransform::from(transform),
        Vec2::new(recipe.width + 1.0, recipe.depth + 2.0),
        recipe.wall_height,
    ) {
        return Err(String::from(
            "The building or entrance approach overlaps a rock or fallen log.",
        ));
    }
    Ok((transform, high - low + 0.08))
}

fn start_build(world: &mut BuildingWorld) {
    if world.job.is_some() {
        return;
    }
    let Some(placement) = world.pending.pop_front() else {
        return;
    };
    // This snapshot must outlive the UI; the worker borrows it during generation.
    let recipe = placement.recipe.clone();
    let base = placement.material_base.clone();
    let task = AsyncComputeTaskPool::get()
        .spawn(async move { generate_with_base(&recipe, &base).map_err(|e| e.to_string()) });
    world.job = Some(BuildJob { placement, task });
}

fn finish_build(
    world: &mut BuildingWorld,
    commands: &mut Commands,
    assets: &mut BuildingAssets,
    physical_radius: f32,
) {
    let Some(job) = &mut world.job else {
        return;
    };
    let Some(result) = check_ready(&mut job.task) else {
        return;
    };
    let job = world.job.take().expect("completed build");
    let prototype = match result {
        Ok(p) => Arc::new(p),
        Err(error) => {
            if world.loading.take().is_some() {
                world.pending.clear();
            }
            world.status =
                format!("Building generation failed; previous placements retained: {error}");
            return;
        }
    };
    let compiled = Arc::clone(
        world
            .compiled
            .entry(prototype.hash.clone())
            .or_insert_with(|| {
                compile_building(
                    prototype,
                    &mut assets.meshes,
                    &mut assets.materials,
                    &mut assets.images,
                )
            }),
    );
    if let Some(loading) = &mut world.loading {
        loading.prepared.push((job.placement, compiled));
        return;
    }
    spawn_placement(commands, assets, job.placement, compiled, physical_radius);
    world.status = String::from("Placed building. E opens a nearby door; H places another.");
}

fn spawn_placement(
    commands: &mut Commands,
    assets: &mut BuildingAssets,
    placement: SavedPlacement,
    compiled: Arc<CompiledBuildingPrototype>,
    physical_radius: f32,
) {
    let transform = Transform {
        translation: Vec3::from_array(placement.translation),
        rotation: Quat::from_array(placement.rotation),
        scale: Vec3::splat(physical_radius.recip()),
    };
    let entity = spawn_building(
        commands,
        Arc::clone(&compiled),
        placement.id.clone(),
        transform,
    );
    let mut instance = BuildingInstance::new(Arc::clone(&compiled), placement.id.clone());
    if let Some(state) = &placement.state {
        instance.restore_state(state);
    }
    let foundation = Cuboid::new(
        placement.recipe.width + 0.2,
        placement.foundation_depth,
        placement.recipe.depth + 0.2,
    );
    commands.spawn((
        Name::new("Level building foundation"),
        Mesh3d(assets.meshes.add(foundation)),
        MeshMaterial3d(assets.materials.add(StandardMaterial {
            base_color: Color::srgb(0.32, 0.31, 0.28),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::from_xyz(0.0, -placement.foundation_depth * 0.5 - 0.01, 0.0),
        ChildOf(entity),
    ));
    commands
        .entity(entity)
        .insert((instance, PlacedBuilding(placement)));
}

fn finish_load(
    world: &mut BuildingWorld,
    commands: &mut Commands,
    assets: &mut BuildingAssets,
    placements: &Query<(Entity, &Transform, &PlacedBuilding, &BuildingInstance)>,
    physical_radius: f32,
) {
    if world.job.is_some() || !world.pending.is_empty() {
        return;
    }
    let Some(loading) = world.loading.take() else {
        return;
    };
    for (entity, ..) in placements {
        commands.entity(entity).despawn();
    }
    for (saved, compiled) in loading.prepared {
        spawn_placement(commands, assets, saved, compiled, physical_radius);
    }
    world.loaded_path = Some(loading.path);
    world.loaded_bytes = Some(loading.bytes);
    world.next_id = loading.next_id;
    world.status = String::from("Loaded complete scene and retained door states.");
}

fn save_scene(
    world: &mut BuildingWorld,
    scene: &Scene,
    placements: &Query<(Entity, &Transform, &PlacedBuilding, &BuildingInstance)>,
) -> Result<(), String> {
    let path = PathBuf::from(world.scene_path.trim());
    if path.as_os_str().is_empty() {
        return Err(String::from("Choose a scene file path."));
    }
    if world.job.is_some() || !world.pending.is_empty() || world.loading.is_some() {
        return Err(String::from(
            "Wait for building generation before saving the scene.",
        ));
    }
    let saved = placements
        .iter()
        .map(|(_, transform, saved, instance)| {
            let mut saved = saved.0.clone();
            saved.translation = transform.translation.to_array();
            saved.rotation = transform.rotation.to_array();
            saved.state = Some(instance.snapshot());
            saved
        })
        .collect();
    let file = SceneFile {
        version: 1,
        seed: scene.seed,
        radius_metres: scene.terrain.topology.radius,
        terrain_hash: terrain_hash(scene),
        placements: saved,
    };
    let mut bytes = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    let expected = if world.loaded_path.as_ref() == Some(&path) {
        world.loaded_bytes.clone()
    } else {
        None
    };
    world.file_job = Some(IoTaskPool::get().spawn(async move {
        save_scene_file(&path, &bytes, expected.as_deref())?;
        Ok(FileCompletion::Saved { path, bytes })
    }));
    world.status = String::from("Saving building placements and door states in the background.");
    Ok(())
}

fn save_scene_file(path: &Path, bytes: &[u8], expected: Option<&[u8]>) -> Result<(), String> {
    let current = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("Could not inspect scene file: {e}")),
    };
    if current.is_some() && current.as_deref() != expected {
        return Err(String::from(
            "Scene file already exists or changed externally. Load it first or choose another path.",
        ));
    }
    atomic_write(path, bytes).map_err(|e| format!("Could not save scene: {e}"))
}

fn load_scene(world: &mut BuildingWorld) -> Result<(), String> {
    let path = PathBuf::from(world.scene_path.trim());
    if path.as_os_str().is_empty() {
        return Err(String::from("Choose a scene file path."));
    }
    world.file_job = Some(IoTaskPool::get().spawn(async move {
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > MAX_SCENE_BYTES {
            return Err(String::from("Building scene exceeds the 4 MiB limit."));
        }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let file =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid scene file: {e}"))?;
        Ok(FileCompletion::Loaded { path, bytes, file })
    }));
    world.status = String::from("Reading saved buildings in the background.");
    Ok(())
}

fn poll_file(world: &mut BuildingWorld) -> Option<Result<FileCompletion, String>> {
    let result = check_ready(world.file_job.as_mut()?)?;
    world.file_job = None;
    Some(result)
}

fn finish_file(
    completion: FileCompletion,
    world: &mut BuildingWorld,
    scene: &Scene,
    explorer: &mut Explorer,
    forests: &crate::forest::Forests,
    placements: &Query<(Entity, &Transform, &PlacedBuilding, &BuildingInstance)>,
) -> Result<(), String> {
    match completion {
        FileCompletion::Recipe {
            path,
            recipe,
            base,
            camera,
        } => {
            if path != world.recipe_path {
                return Err(String::from(
                    "Recipe path changed while reading. Place again to use it.",
                ));
            }
            place_recipe(
                world, scene, explorer, forests, &camera, placements, recipe, base,
            )
        }
        FileCompletion::Saved { path, bytes } => {
            world.loaded_path = Some(path);
            world.loaded_bytes = Some(bytes);
            world.status = String::from("Saved building placements and door states.");
            Ok(())
        }
        FileCompletion::Loaded { path, bytes, file } => {
            stage_scene(world, scene, path, bytes, file)
        }
    }
}

fn stage_scene(
    world: &mut BuildingWorld,
    scene: &Scene,
    path: PathBuf,
    bytes: Vec<u8>,
    file: SceneFile,
) -> Result<(), String> {
    validate_scene(&file, scene)?;
    world.job = None;
    world.compiled.clear();
    let next_id = file
        .placements
        .iter()
        .filter_map(|p| p.id.rsplit('-').next()?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    world.pending = file.placements.into();
    world.loading = Some(PendingLoad {
        path,
        bytes,
        next_id,
        prepared: Vec::new(),
    });
    world.status = String::from("Loading saved buildings in the background.");
    Ok(())
}

fn validate_scene(file: &SceneFile, scene: &Scene) -> Result<(), String> {
    if file.version != 1
        || file.seed != scene.seed
        || (file.radius_metres - scene.terrain.topology.radius).abs() > 0.001
        || file.terrain_hash != terrain_hash(scene)
    {
        return Err(String::from(
            "Scene version or generated terrain does not match this world.",
        ));
    }
    if file.placements.len() > MAX_PLACEMENTS {
        return Err(String::from("Scene has too many buildings."));
    }
    let mut ids = std::collections::BTreeSet::new();
    for placement in &file.placements {
        if !ids.insert(&placement.id) || placement.id.is_empty() {
            return Err(String::from("Scene has duplicate or empty placement IDs."));
        }
        if !placement
            .translation
            .iter()
            .chain(&placement.rotation)
            .all(|v| v.is_finite())
            || (Quat::from_array(placement.rotation).length_squared() - 1.0).abs() > 0.01
        {
            return Err(String::from(
                "Scene contains an invalid placement transform.",
            ));
        }
        if !placement.foundation_depth.is_finite()
            || !(0.0..=0.5).contains(&placement.foundation_depth)
        {
            return Err(String::from("Scene contains an invalid foundation depth."));
        }
        island_building::validate(&placement.recipe).map_err(|e| e.to_string())?;
        if let Some(state) = &placement.state
            && (state.version != 1
                || state.placement_id != placement.id
                || state
                    .doors
                    .values()
                    .any(|s| !s.angle.is_finite() || !s.target.is_finite()))
        {
            return Err(String::from("Scene contains invalid retained door state."));
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension(format!("building-tmp-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn terrain_hash(scene: &Scene) -> u64 {
    // Explicit FNV-1a makes persisted identity stable across Rust releases.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut append = |bytes: &[u8]| {
        for &byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    };
    append(&scene.seed.to_le_bytes());
    append(&scene.terrain.topology.radius.to_bits().to_le_bytes());
    for triangle in &scene.terrain.topology.triangles {
        for index in triangle {
            append(&u64::from(*index).to_le_bytes());
        }
    }
    for index in 0..scene.terrain.elevations.len() {
        for value in crate::surface::displayed_position(
            &scene.terrain,
            index,
            scene.bed == crate::command::BedView::Original,
        )
        .to_array()
        {
            append(&value.to_bits().to_le_bytes());
        }
    }
    hash
}

fn panel(
    mut contexts: EguiContexts,
    mut world: ResMut<BuildingWorld>,
    instances: Query<&BuildingInstance>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::Window::new("Buildings")
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
        .default_open(false)
        .default_width(290.0)
        .show(ctx, |ui| {
            ui.label("Building Studio recipe (empty uses the hut)");
            ui.text_edit_singleline(&mut world.recipe_path);
            if ui.button("Place ahead · H").clicked() {
                world.request = Some(Request::Place);
            }
            ui.label("Aim at level dry ground. E operates the nearest door.");
            ui.separator();
            ui.label("Building scene");
            ui.text_edit_singleline(&mut world.scene_path);
            ui.horizontal(|ui| {
                if ui.button("Save scene").clicked() {
                    world.request = Some(Request::Save);
                }
                if ui.button("Load scene").clicked() {
                    world.request = Some(Request::Load);
                }
            });
            ui.label(format!("{} buildings", instances.iter().count()));
            ui.label(&world.status);
        });
}

/// Building contacts share the same independent collision/residency metadata as
/// the studio. Tests and terrain-only controllers can pass an empty query.
pub(crate) type Contacts<'w, 's> =
    Query<'w, 's, (&'static BuildingInstance, &'static GlobalTransform)>;

pub(crate) fn support(contacts: &Contacts, position: Vec3, up: Vec3, max_step: f32) -> Option<f32> {
    contacts
        .iter()
        .filter_map(|(building, transform)| {
            let scale = transform.affine().matrix3.x_axis.length();
            building.support_capsule_point(transform, position, max_step, 0.27 * scale)
        })
        .map(|point| point.dot(up))
        .max_by(f32::total_cmp)
}

pub(crate) fn blocked(contacts: &Contacts, feet: Vec3, up: Vec3, physical_radius: f32) -> bool {
    contacts.iter().any(|(building, transform)| {
        building.blocks_capsule(
            transform,
            feet + up * (0.005 / physical_radius),
            0.27 / physical_radius,
            1.795 / physical_radius,
        )
    })
}

#[cfg(test)]
pub(crate) fn test_flat_site(up: Vec3) -> Scene {
    let mut scene = crate::collision::test_scene();
    let terrain = Arc::get_mut(&mut scene.terrain).expect("exclusive terrain fixture");
    for (height, direction) in terrain
        .elevations
        .iter_mut()
        .zip(&terrain.topology.directions)
    {
        let projection = crate::surface::vector(*direction).dot(up);
        *height = if projection > 0.8 {
            1.01 / f64::from(projection) - 1.0
        } else {
            0.01
        };
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dry_scene() -> Scene {
        let mut scene = crate::collision::test_scene();
        for height in &mut Arc::get_mut(&mut scene.terrain)
            .expect("exclusive fixture")
            .elevations
        {
            *height = 0.01;
        }
        scene
    }

    #[test]
    fn footprint_fitting_preserves_metre_dimensions_radial_up_and_dry_access() {
        let recipe = BuildingRecipe::hut();
        for up in [
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Z,
            Vec3::NEG_Z,
        ] {
            let scene = test_flat_site(up);
            let ground = Ground::new(&scene);
            let physical_radius = scene.terrain.topology.radius as f32;
            let sample = ground.surface_sample(&scene.terrain, up, false).unwrap();
            let (transform, depth) = fit_placement(
                &scene,
                &ground,
                &recipe,
                sample.point,
                up.any_orthonormal_vector(),
            )
            .unwrap();
            assert!(transform.up().dot(up) > 0.9999);
            assert!((transform.scale.x * physical_radius - 1.0).abs() < 1e-6);
            assert!((0.0..0.4).contains(&depth));
            assert!((transform.translation - sample.point).dot(up) > 0.0);
        }
        let wet = crate::collision::test_scene();
        let ground = Ground::new(&wet);
        let sample = ground.surface_sample(&wet.terrain, Vec3::Y, false).unwrap();
        assert!(fit_placement(&wet, &ground, &recipe, sample.point, Vec3::Z).is_err());
    }

    #[test]
    fn saved_scene_rejects_different_terrain_and_duplicate_placement_ids() {
        let scene = dry_scene();
        let mut file = SceneFile {
            version: 1,
            seed: scene.seed,
            radius_metres: scene.terrain.topology.radius,
            terrain_hash: terrain_hash(&scene),
            placements: Vec::new(),
        };
        assert!(validate_scene(&file, &scene).is_ok());
        file.terrain_hash ^= 1;
        assert!(validate_scene(&file, &scene).is_err());
        file.terrain_hash = terrain_hash(&scene);
        let placement = SavedPlacement {
            id: String::from("same"),
            recipe: BuildingRecipe::hut(),
            material_base: PathBuf::from("."),
            translation: Vec3::Y.to_array(),
            rotation: Quat::IDENTITY.to_array(),
            foundation_depth: 0.1,
            state: None,
        };
        file.placements = vec![placement.clone(), placement];
        assert!(validate_scene(&file, &scene).is_err());
    }

    #[test]
    fn footprint_rejects_small_rocks_and_crossing_logs_between_terrain_samples() {
        use crate::collision::{LogCollider, RockCollider};
        let scene = dry_scene();
        for scale in [0.01, 1.0 / 2000.0] {
            let transform = Transform::from_translation(Vec3::Y * 1.01)
                .with_rotation(Quat::from_rotation_z(0.7))
                .with_scale(Vec3::splat(scale));
            let root = GlobalTransform::from(transform);
            let mut ground = Ground::new(&scene);
            ground.set_rocks(vec![RockCollider {
                centre: transform.transform_point(Vec3::new(0.3, 0.12, 0.3)),
                radius: 0.15 * scale,
            }]);
            assert!(ground.placement_obstructed(&root, Vec2::splat(6.0), 3.0));
            ground.set_rocks(vec![RockCollider {
                centre: transform.transform_point(Vec3::new(5.0, 0.12, 0.3)),
                radius: 0.15 * scale,
            }]);
            assert!(!ground.placement_obstructed(&root, Vec2::splat(6.0), 3.0));
            ground.set_rocks(Vec::new());
            ground.set_logs(vec![LogCollider {
                a: transform.transform_point(Vec3::new(-5.0, 0.1, 0.0)),
                b: transform.transform_point(Vec3::new(5.0, 0.1, 0.0)),
                radius: 0.12 * scale,
            }]);
            assert!(ground.placement_obstructed(&root, Vec2::splat(6.0), 3.0));
        }
    }

    #[derive(Resource, Default)]
    struct IoHeartbeat(u32);

    #[derive(Resource, Default)]
    struct IoCompletions(Vec<Result<FileCompletion, String>>);

    fn poll_test_file(
        mut world: ResMut<BuildingWorld>,
        mut heartbeat: ResMut<IoHeartbeat>,
        mut completions: ResMut<IoCompletions>,
    ) {
        heartbeat.0 += 1;
        if let Some(result) = poll_file(&mut world) {
            completions.0.push(result);
        }
    }

    #[test]
    fn pending_file_io_keeps_app_updates_running() {
        use std::{
            sync::mpsc,
            thread,
            time::{Duration, Instant},
        };
        for outcome in [
            Ok(FileCompletion::Recipe {
                path: String::from("gated.json"),
                recipe: BuildingRecipe::hut(),
                base: PathBuf::from("."),
                camera: Transform::IDENTITY,
            }),
            Err(String::from("simulated read failure")),
        ] {
            let expected_ok = outcome.is_ok();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .init_resource::<BuildingWorld>()
                .init_resource::<IoHeartbeat>()
                .init_resource::<IoCompletions>()
                .add_systems(Update, poll_test_file);
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let task = IoTaskPool::get().spawn(async move {
                started_tx.send(()).expect("started receiver");
                release_rx.recv().expect("release sender");
                outcome
            });
            app.world_mut().resource_mut::<BuildingWorld>().file_job = Some(task);
            started_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("I/O worker starts");
            // A broken blocking poll must fail instead of hanging the test suite.
            let (cancel_tx, cancel_rx) = mpsc::channel();
            let watchdog_release = release_tx.clone();
            let watchdog = thread::spawn(move || {
                if cancel_rx.recv_timeout(Duration::from_secs(5)).is_err() {
                    let _ = watchdog_release.send(());
                }
            });
            for _ in 0..10 {
                app.update();
            }
            assert_eq!(app.world().resource::<IoHeartbeat>().0, 10);
            assert!(app.world().resource::<BuildingWorld>().file_job.is_some());
            assert!(app.world().resource::<BuildingWorld>().pending.is_empty());
            assert!(app.world().resource::<IoCompletions>().0.is_empty());
            release_tx.send(()).expect("waiting worker");
            cancel_tx.send(()).expect("watchdog receiver");
            watchdog.join().expect("watchdog thread");
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.world().resource::<IoCompletions>().0.is_empty() {
                assert!(Instant::now() < deadline, "worker completion watchdog");
                app.update();
                thread::yield_now();
            }
            assert!(app.world().resource::<BuildingWorld>().file_job.is_none());
            let completions = &app.world().resource::<IoCompletions>().0;
            assert_eq!(completions.len(), 1);
            assert_eq!(completions[0].is_ok(), expected_ok);
            app.update();
            assert_eq!(app.world().resource::<IoCompletions>().0.len(), 1);
        }
    }
}
