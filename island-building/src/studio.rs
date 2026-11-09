//! Standalone building authoring application. Workers own snapshots, never editor state.
#![allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]

mod camera;
mod capture;
mod ui;

use crate::{
    BuildingPrototype, BuildingRecipe,
    document::StudioDocument,
    export::{ExportManifest, export_package},
    generate_with_base,
    interaction::{BuildingInstance, BuildingObserver, BuildingRuntimePlugin, BuildingRuntimeSet},
    render::{compile_building, spawn_building},
    validate_recipe,
};
use bevy::{
    app::ScheduleRunnerPlugin,
    light::GlobalAmbientLight,
    pbr::wireframe::{WireframeConfig, WireframePlugin},
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future},
    window::{ExitCondition, PresentMode, WindowResolution},
    winit::WinitPlugin,
};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Resource, Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] // Independent CLI switches.
pub struct Settings {
    pub recipe: Option<PathBuf>,
    pub preset: String,
    pub screenshot: Option<PathBuf>,
    pub lod: usize,
    pub automatic: bool,
    pub view: String,
    pub door_open: bool,
    pub capture_ui: bool,
    pub seed: Option<u64>,
    pub export: Option<PathBuf>,
    pub validate: bool,
    pub sun: f32,
    pub ambient: f32,
    pub room_light: f32,
    pub exposure: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recipe: None,
            preset: "hut".into(),
            screenshot: None,
            lod: 0,
            automatic: true,
            view: "exterior".into(),
            door_open: false,
            capture_ui: false,
            seed: None,
            export: None,
            validate: false,
            sun: 14_000.,
            ambient: 450.,
            room_light: 160_000.,
            exposure: 9.7,
        }
    }
}

pub fn run(arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let Some(settings) = parse_arguments(arguments)? else {
        return Ok(());
    };
    let mut document = if let Some(path) = &settings.recipe {
        StudioDocument::open(path).map_err(|e| e.to_string())?
    } else {
        StudioDocument::new(preset(&settings.preset))
    };
    if let Some(seed) = settings.seed {
        document.edit(|r| r.seed = seed);
    }
    if settings.validate || settings.export.is_some() {
        return headless_output(&document, &settings);
    }
    if let Some(path) = &settings.screenshot {
        if path.exists() {
            return Err(format!(
                "{} exists; choose a new screenshot filename",
                path.display()
            ));
        }
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let capturing = settings.screenshot.is_some();
    let mut app = App::new();
    let mut plugins = DefaultPlugins.set(if capturing {
        WindowPlugin {
            close_when_requested: false,
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        }
    } else {
        WindowPlugin {
            close_when_requested: false,
            primary_window: Some(Window {
                title: "Island Building Studio".into(),
                resolution: WindowResolution::new(1540, 960),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }
    });
    if capturing {
        plugins = plugins.disable::<WinitPlugin>();
    }
    app.add_plugins(plugins)
        .add_plugins(BuildingRuntimePlugin)
        .add_plugins(WireframePlugin::default())
        .insert_resource(ClearColor(Color::srgb(0.60, 0.71, 0.76)))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.79, 0.88, 1.),
            brightness: 450.,
            ..default()
        })
        .insert_resource(StudioState::new(document, &settings))
        .insert_resource(settings.clone())
        .init_resource::<camera::CameraControls>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                poll_preview,
                poll_export,
                camera::update_camera,
                camera::sync_observer,
                diagnostics,
                close_requests,
            )
                .chain()
                .before(BuildingRuntimeSet),
        );
    if capturing {
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(8)));
        capture::install(&mut app, &settings);
    }
    if !capturing || settings.capture_ui {
        app.add_plugins(EguiPlugin::default())
            .add_systems(EguiPrimaryContextPass, ui::draw);
    }
    app.run();
    Ok(())
}

fn headless_output(document: &StudioDocument, settings: &Settings) -> Result<(), String> {
    let prototype = generate_with_base(
        document.recipe(),
        document.base_directory().unwrap_or(Path::new(".")),
    )
    .map_err(|e| e.to_string())?;
    if let Some(path) = &settings.export {
        export_package(
            &prototype,
            path,
            document.base_directory().unwrap_or(Path::new(".")),
        )
        .map_err(|e| e.to_string())?;
        println!("Exported {} ({})", path.display(), prototype.hash);
    } else {
        println!(
            "Valid: {} · {} · triangles {:?}",
            prototype.recipe.name,
            prototype.hash,
            prototype
                .lods
                .each_ref()
                .map(crate::BuildingLod::triangle_count)
        );
    }
    Ok(())
}

pub(super) fn preset(name: &str) -> BuildingRecipe {
    match name {
        "cottage" => BuildingRecipe::cottage(),
        "workshop" => BuildingRecipe::workshop(),
        _ => BuildingRecipe::hut(),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Structure,
    Layout,
    Openings,
    Roof,
    Materials,
    Bake,
}

#[derive(Clone, Debug)]
enum PendingAction {
    New(String),
    Open(PathBuf),
    Revert,
    Quit,
}

struct BuildRequest {
    serial: u64,
    recipe: BuildingRecipe,
    base: PathBuf,
    queued: Instant,
}
struct BuildResponse {
    serial: u64,
    result: Result<BuildingPrototype, String>,
    seconds: f32,
}
struct ExportResponse {
    serial: u64,
    destination: PathBuf,
    result: Result<ExportManifest, String>,
    seconds: f32,
}

#[derive(Resource)]
#[allow(clippy::struct_excessive_bools)] // Independent editor and display toggles.
pub(super) struct StudioState {
    document: StudioDocument,
    tab: Tab,
    status: String,
    error: Option<String>,
    pending_action: Option<PendingAction>,
    serial: u64,
    pending: Option<BuildRequest>,
    running: Option<Task<BuildResponse>>,
    last_good: Option<Arc<BuildingPrototype>>,
    root: Option<Entity>,
    export_task: Option<Task<ExportResponse>>,
    file_dialog: Option<Task<ui::dialogs::Completion>>,
    baked_serial: Option<u64>,
    build_seconds: f32,
    bake_seconds: f32,
    lod: usize,
    automatic: bool,
    door_command: Option<bool>,
    collision_overlay: bool,
    wireframe: bool,
    cutaway: bool,
    recompile: bool,
    light: f32,
    ambient: f32,
    room_light: f32,
    exposure: f32,
    recipe_revision: u64,
    final_preview: bool,
    exit_requested: bool,
}

impl StudioState {
    fn new(document: StudioDocument, settings: &Settings) -> Self {
        let mut state = Self {
            recipe_revision: document.revision(),
            document,
            tab: Tab::Structure,
            status: "Preparing first preview…".into(),
            error: None,
            pending_action: None,
            serial: 0,
            pending: None,
            running: None,
            last_good: None,
            root: None,
            export_task: None,
            file_dialog: None,
            baked_serial: None,
            build_seconds: 0.,
            bake_seconds: 0.,
            lod: settings.lod,
            automatic: settings.automatic,
            door_command: Some(settings.door_open),
            collision_overlay: false,
            wireframe: false,
            cutaway: false,
            recompile: false,
            final_preview: settings.screenshot.is_some(),
            exit_requested: false,
            light: settings.sun,
            ambient: settings.ambient,
            room_light: settings.room_light,
            exposure: settings.exposure,
        };
        state.request_preview();
        state
    }

    fn request_preview(&mut self) {
        self.serial += 1;
        self.recipe_revision = self.document.revision();
        self.pending = None;
        if let Err(error) = validate_recipe(self.document.recipe()) {
            self.error = Some(error.to_string());
            self.status = "Invalid draft · last good preview retained".into();
            return;
        }
        self.error = None;
        let mut recipe = self.document.recipe().clone();
        // Geometry previews carry a low-cost atlas. Explicit exports use the authored resolution.
        if !self.final_preview {
            recipe.atlas_size = recipe.atlas_size.min(128);
        }
        self.pending = Some(BuildRequest {
            serial: self.serial,
            recipe,
            base: self
                .document
                .base_directory()
                .unwrap_or(Path::new("."))
                .to_path_buf(),
            queued: Instant::now(),
        });
        self.status = "Preview queued…".into();
    }

    fn start_pending_preview(&mut self) {
        if self.running.is_none()
            && self
                .pending
                .as_ref()
                .is_some_and(|request| request.queued.elapsed() > Duration::from_millis(160))
        {
            let request = self.pending.take().expect("pending request checked");
            self.status = "Generating preview…".into();
            self.running = Some(AsyncComputeTaskPool::get().spawn(async move {
                let start = Instant::now();
                let result =
                    generate_with_base(&request.recipe, &request.base).map_err(|e| e.to_string());
                BuildResponse {
                    serial: request.serial,
                    result,
                    seconds: start.elapsed().as_secs_f32(),
                }
            }));
        }
    }

    fn request_export(&mut self, destination: PathBuf) {
        if self.export_task.is_some() {
            return;
        }
        let recipe = self.document.recipe().clone();
        let base = self
            .document
            .base_directory()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let serial = self.serial;
        self.status = "Baking portable package in background…".into();
        self.export_task = Some(AsyncComputeTaskPool::get().spawn(async move {
            let start = Instant::now();
            let result = generate_with_base(&recipe, &base)
                .map_err(|e| e.to_string())
                .and_then(|prototype| {
                    export_package(&prototype, &destination, &base).map_err(|e| e.to_string())
                });
            ExportResponse {
                serial,
                destination,
                result,
                seconds: start.elapsed().as_secs_f32(),
            }
        }));
    }

    fn apply_action(&mut self, action: PendingAction) {
        let result = match action {
            PendingAction::New(name) => {
                self.document = StudioDocument::new(preset(&name));
                Ok(())
            }
            PendingAction::Open(path) => {
                StudioDocument::open(&path).map(|document| self.document = document)
            }
            PendingAction::Revert => self.document.revert(),
            PendingAction::Quit => {
                self.exit_requested = true;
                Ok(())
            }
        };
        match result {
            Ok(()) => self.request_preview(),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}

#[derive(Component)]
struct StudioCamera;
#[derive(Component)]
struct StudioObserver;
#[derive(Component)]
struct StudioSun;
#[derive(Component)]
struct StudioInteriorLight;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    settings: Res<Settings>,
    target: Option<Res<capture::CaptureTarget>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(180., 180.))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.27, 0.32, 0.25),
            perceptual_roughness: 0.96,
            ..default()
        })),
        Transform::from_xyz(0., -0.18, 0.),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 14_000.,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-9., 12., 8.).looking_at(Vec3::ZERO, Vec3::Y),
        StudioSun,
    ));
    commands.spawn((
        PointLight {
            intensity: 160_000.,
            range: 18.,
            color: Color::srgb(1., 0.88, 0.69),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(0., 2.3, 0.),
        StudioInteriorLight,
    ));
    let mut camera = commands.spawn((
        Camera3d::default(),
        bevy::camera::Exposure::default(),
        bevy::camera::ShadowLodOrigin,
        Transform::from_xyz(11., 7., 13.).looking_at(Vec3::new(0., 1.6, 0.), Vec3::Y),
        StudioCamera,
    ));
    if let Some(target) = target {
        camera.insert(bevy::camera::RenderTarget::Image(
            target.image.clone().into(),
        ));
    }
    commands.spawn((
        Transform::default(),
        GlobalTransform::default(),
        BuildingObserver::default(),
        StudioObserver,
    ));
    if settings.screenshot.is_none() {
        info!(
            "Building Studio · drag RMB to orbit/look · wheel to zoom · walkthrough WASD, E door"
        );
    }
}

fn poll_preview(
    mut commands: Commands,
    mut state: ResMut<StudioState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut controls: ResMut<camera::CameraControls>,
    settings: Res<Settings>,
    instances: Query<&BuildingInstance>,
) {
    if state.document.revision() != state.recipe_revision {
        state.request_preview();
    }
    let response = state
        .running
        .as_mut()
        .and_then(|task| block_on(future::poll_once(task)));
    if let Some(response) = response {
        state.running = None;
        if response.serial == state.serial {
            match response.result {
                Ok(prototype) => {
                    let prototype = Arc::new(prototype);
                    let compiled = compile_building(
                        display_prototype(&prototype, state.cutaway),
                        &mut meshes,
                        &mut materials,
                        &mut images,
                    );
                    let saved = state
                        .root
                        .and_then(|root| instances.get(root).ok())
                        .map(BuildingInstance::snapshot);
                    if let Some(root) = state.root.take() {
                        commands.entity(root).despawn();
                    }
                    let root = spawn_building(
                        &mut commands,
                        Arc::clone(&compiled),
                        "studio-preview",
                        Transform::IDENTITY,
                    );
                    if let Some(saved) = saved {
                        let mut instance = BuildingInstance::new(compiled, "studio-preview".into());
                        instance.restore_state(&saved);
                        commands.entity(root).insert(instance);
                    }
                    state.root = Some(root);
                    controls.frame(&prototype, &settings.view);
                    state.last_good = Some(prototype);
                    state.build_seconds = response.seconds;
                    state.status = if state.final_preview {
                        "Capture ready · full authored atlas"
                    } else {
                        "Preview ready · editing atlas 128 px"
                    }
                    .into();
                    state.error = None;
                }
                Err(error) => {
                    state.error = Some(error);
                    state.status = "Generation failed · last good preview retained".into();
                }
            }
        }
    }
    if state.recompile {
        state.recompile = false;
        if let Some(prototype) = state.last_good.as_ref().map(Arc::clone) {
            let compiled = compile_building(
                display_prototype(&prototype, state.cutaway),
                &mut meshes,
                &mut materials,
                &mut images,
            );
            let saved = state
                .root
                .and_then(|root| instances.get(root).ok())
                .map(BuildingInstance::snapshot);
            if let Some(root) = state.root.take() {
                commands.entity(root).despawn();
            }
            let root = spawn_building(
                &mut commands,
                Arc::clone(&compiled),
                "studio-preview",
                Transform::IDENTITY,
            );
            if let Some(saved) = saved {
                let mut instance = BuildingInstance::new(compiled, "studio-preview".into());
                instance.restore_state(&saved);
                commands.entity(root).insert(instance);
            }
            state.root = Some(root);
        }
    }
    state.start_pending_preview();
}

fn display_prototype(source: &Arc<BuildingPrototype>, cutaway: bool) -> Arc<BuildingPrototype> {
    if !cutaway {
        return Arc::clone(source);
    }
    let mut display = source.as_ref().clone();
    for lod in &mut display.lods {
        lod.parts
            .retain(|part| part.material != crate::MaterialSlot::Roof);
        for part in &mut lod.parts {
            if part.material != crate::MaterialSlot::Interior {
                continue;
            }
            let mesh = &mut part.mesh;
            mesh.indices = mesh
                .indices
                .chunks_exact(3)
                .filter(|triangle| {
                    !triangle.iter().all(|index| {
                        mesh.positions[*index as usize][2] >= source.recipe.wall_height - 0.02
                    })
                })
                .flatten()
                .copied()
                .collect();
        }
    }
    Arc::new(display)
}

fn diagnostics(
    state: Res<StudioState>,
    instances: Query<(&BuildingInstance, &GlobalTransform)>,
    mut gizmos: Gizmos,
    mut wireframe: ResMut<WireframeConfig>,
    mut exposure: Single<&mut bevy::camera::Exposure, With<StudioCamera>>,
    mut interior: Single<&mut PointLight, With<StudioInteriorLight>>,
) {
    exposure.ev100 = state.exposure;
    interior.intensity = state.room_light;
    wireframe.global = state.wireframe;
    if !state.collision_overlay {
        return;
    }
    for (instance, root) in &instances {
        for shape in &instance.prototype.source.colliders {
            let min = crate::render::native_to_bevy(shape.min);
            let max = crate::render::native_to_bevy(shape.max);
            gizmos.cube(
                Transform::from_translation(root.transform_point((min + max) * 0.5))
                    .with_scale((max - min).abs()),
                Color::srgba(0.2, 0.9, 0.95, 0.8),
            );
        }
        for descriptor in &instance.prototype.source.doors {
            let angle = instance
                .doors
                .get(&descriptor.id)
                .map_or(0., |door| door.angle);
            let shape = descriptor.collider_at(angle);
            let transform =
                Transform::from_translation(crate::render::native_to_bevy(shape.centre))
                    .with_rotation(Quat::from_rotation_y(shape.yaw))
                    .with_scale(Vec3::new(
                        shape.half_extents[0] * 2.,
                        shape.half_extents[2] * 2.,
                        shape.half_extents[1] * 2.,
                    ));
            gizmos.cube(transform, Color::srgb(1., 0.65, 0.2));
        }
    }
}

fn close_requests(
    mut requests: MessageReader<bevy::window::WindowCloseRequested>,
    mut state: ResMut<StudioState>,
    mut exit: MessageWriter<AppExit>,
) {
    if requests.read().next().is_some() && state.file_dialog.is_none() {
        if state.document.is_dirty() {
            state.pending_action = Some(PendingAction::Quit);
        } else {
            state.exit_requested = true;
        }
    }
    if state.exit_requested {
        exit.write(AppExit::Success);
    }
}

fn poll_export(mut state: ResMut<StudioState>) {
    let response = state
        .export_task
        .as_mut()
        .and_then(|task| block_on(future::poll_once(task)));
    if let Some(response) = response {
        state.export_task = None;
        state.bake_seconds = response.seconds;
        match response.result {
            Ok(_) => {
                state.baked_serial = Some(response.serial);
                state.status = format!(
                    "Exported {}{}",
                    response.destination.display(),
                    if response.serial == state.serial {
                        ""
                    } else {
                        " · an older revision"
                    }
                );
            }
            Err(error) => {
                state.error = Some(error);
                state.status = "Export failed · destination protected".into();
            }
        }
    }
}

pub fn parse_arguments(
    arguments: impl Iterator<Item = String>,
) -> Result<Option<Settings>, String> {
    let mut settings = Settings::default();
    let mut args = arguments;
    let mut explicit_lod = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "Island Building Studio\n  --recipe FILE.json | --preset hut|cottage|workshop\n  --seed N\n  --screenshot NEW.png [--lod 0|1|2|auto] [--view exterior|entrance|interior|south|east|north|west] [--door-open] [--capture-ui]\n  --validate\n  --export NEW_DIRECTORY\nInteractive: RMB orbit/look, wheel zoom, WASD walkthrough, E toggle nearby door."
                );
                return Ok(None);
            }
            "--recipe" => {
                settings.recipe = Some(PathBuf::from(args.next().ok_or("--recipe needs a path")?));
            }
            "--preset" => {
                settings.preset = args.next().ok_or("--preset needs a name")?;
                if !["hut", "cottage", "workshop"].contains(&settings.preset.as_str()) {
                    return Err("Preset must be hut, cottage, or workshop".into());
                }
            }
            "--seed" => {
                settings.seed = Some(
                    args.next()
                        .ok_or("--seed needs an integer")?
                        .parse()
                        .map_err(|_| "Invalid seed")?,
                );
            }
            "--screenshot" => {
                settings.screenshot = Some(PathBuf::from(
                    args.next().ok_or("--screenshot needs a path")?,
                ));
            }
            "--lod" => {
                explicit_lod = true;
                let value = args.next().ok_or("--lod needs 0, 1, 2, or auto")?;
                settings.automatic = value == "auto";
                if !settings.automatic {
                    settings.lod = value.parse().map_err(|_| "LOD must be 0, 1, 2, or auto")?;
                    if settings.lod > 2 {
                        return Err("LOD must be 0, 1, 2, or auto".into());
                    }
                }
            }
            "--view" => {
                settings.view = args.next().ok_or("--view needs a name")?;
                if ![
                    "exterior", "entrance", "interior", "south", "east", "north", "west",
                ]
                .contains(&settings.view.as_str())
                {
                    return Err("Unknown camera view".into());
                }
            }
            "--door-open" => settings.door_open = true,
            "--capture-ui" => settings.capture_ui = true,
            "--validate" => settings.validate = true,
            "--sun" | "--ambient" | "--room-light" | "--exposure" => lighting_argument(
                &mut settings,
                &arg,
                &args.next().ok_or("Lighting flag needs a number")?,
            )?,
            "--export" => {
                settings.export = Some(PathBuf::from(
                    args.next().ok_or("--export needs a new directory")?,
                ));
            }
            _ => return Err(format!("Unknown argument {arg:?}; use --help")),
        }
    }
    if settings.screenshot.is_some() && !explicit_lod {
        settings.automatic = false;
    }
    if settings.capture_ui && settings.screenshot.is_none() {
        return Err("--capture-ui requires --screenshot".into());
    }
    Ok(Some(settings))
}

fn lighting_argument(settings: &mut Settings, flag: &str, value: &str) -> Result<(), String> {
    let value: f32 = value
        .parse()
        .map_err(|_| format!("{flag} needs a finite number"))?;
    let (target, limit) = match flag {
        "--sun" => (&mut settings.sun, 40_000.),
        "--ambient" => (&mut settings.ambient, 1500.),
        "--room-light" => (&mut settings.room_light, 160_000.),
        _ => (&mut settings.exposure, 15.),
    };
    if !value.is_finite() || !(0.0..=limit).contains(&value) {
        return Err(format!("{flag} must be finite and between 0 and {limit}"));
    }
    *target = value;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_arguments_are_strict() {
        assert!(parse_arguments(["--lod", "4"].map(str::to_owned).into_iter()).is_err());
        assert!(parse_arguments(["--capture-ui"].map(str::to_owned).into_iter()).is_err());
        let settings = parse_arguments(
            [
                "--screenshot",
                "test.png",
                "--lod",
                "2",
                "--door-open",
                "--view",
                "interior",
            ]
            .map(str::to_owned)
            .into_iter(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(settings.lod, 2);
        assert!(settings.door_open);
        assert!(!settings.automatic);
    }

    #[test]
    fn newest_edit_replaces_pending_and_invalid_draft_supersedes_it() {
        let mut state = StudioState::new(
            StudioDocument::new(BuildingRecipe::hut()),
            &Settings::default(),
        );
        let old_serial = state.serial;
        state.document.edit(|recipe| recipe.seed = 900);
        state.request_preview();
        let pending = state.pending.as_ref().unwrap();
        assert_eq!(pending.recipe.seed, 900);
        assert!(pending.serial > old_serial);
        state.document.edit(|recipe| recipe.wall_height = -1.);
        state.request_preview();
        assert!(state.pending.is_none());
        assert!(state.error.is_some());
        assert!(state.serial > old_serial);
        assert!(state.last_good.is_none());
    }
}
