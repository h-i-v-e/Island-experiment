//! GPU-synchronized offscreen comparison of the two building material compilers.
//! Metal exposes CPU render diagnostics; each timed frame waits for GPU completion.
#![allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]

use bevy::{
    app::ScheduleRunnerPlugin,
    diagnostic::DiagnosticsStore,
    light::GlobalAmbientLight,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        diagnostic::RenderDiagnosticsPlugin,
        render_resource::{
            CachedPipelineState, PipelineCache, PollType, TextureFormat, TextureUsages,
        },
        renderer::RenderDevice,
        view::screenshot::{Screenshot, save_to_disk},
    },
    shader::ShaderCacheError,
    window::ExitCondition,
    winit::WinitPlugin,
};
use island_building::{
    BuildingRecipe,
    document::StudioDocument,
    generate_with_base,
    interaction::{BuildingInstance, BuildingRuntimePlugin, LodMode},
    render::{
        BuildingArrayMaterial, BuildingArrayPlugin, BuildingGrubPlugin, GrubMaterial,
        compile_building_with_arrays, compile_building_with_grub, spawn_building,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Resource, Clone, Default)]
struct PipelinesReady(Arc<AtomicBool>);

#[derive(Resource)]
struct Options {
    output: PathBuf,
    recipe: Option<PathBuf>,
    preset: String,
    arrays: bool,
    buildings: u16,
    variants: u16,
    lod: usize,
    warmup: u32,
    frames: u32,
    screenshot: Option<PathBuf>,
    normal_prepass: bool,
    ssao: bool,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Self {
            output: PathBuf::new(),
            recipe: None,
            preset: "cottage".into(),
            arrays: false,
            buildings: 32,
            variants: 1,
            lod: 0,
            warmup: 300,
            frames: 600,
            screenshot: None,
            normal_prepass: false,
            ssao: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
            match arg.as_str() {
                "--output" => options.output = value.into(),
                "--recipe" => options.recipe = Some(value.into()),
                "--preset"
                    if ["cottage", "church", "hut", "workshop"].contains(&value.as_str()) =>
                {
                    options.preset = value;
                }
                "--renderer" if ["standard", "arrays"].contains(&value.as_str()) => {
                    options.arrays = value == "arrays";
                }
                "--buildings" => options.buildings = value.parse().map_err(|_| "Invalid count")?,
                "--variants" => options.variants = value.parse().map_err(|_| "Invalid variants")?,
                "--lod" => options.lod = value.parse().map_err(|_| "Invalid LOD")?,
                "--warmup" => options.warmup = value.parse().map_err(|_| "Invalid warmup")?,
                "--frames" => options.frames = value.parse().map_err(|_| "Invalid frames")?,
                "--screenshot" => options.screenshot = Some(value.into()),
                "--normal-prepass" => {
                    options.normal_prepass = value.parse().map_err(|_| "Use true or false")?;
                }
                "--ssao" => {
                    options.ssao = value.parse().map_err(|_| "Use true or false")?;
                }
                _ => return Err(format!("Invalid argument {arg} {value}")),
            }
        }
        if options.output.as_os_str().is_empty()
            || options.output.exists()
            || !(1..=512).contains(&options.buildings)
            || !(1..=16).contains(&options.variants)
            || options.variants > options.buildings
            || options.lod > 2
            || options.warmup < 120
            || options.frames < 60
            || options.frames > 10_000
            || options
                .screenshot
                .as_ref()
                .is_some_and(|path| path.exists())
        {
            return Err("Use --output NEW.json, --renderer standard|arrays, --buildings 1..512, --variants 1..16, --lod 0..2, --warmup >=120, --frames 60..10000, --screenshot NEW.png".into());
        }
        Ok(options)
    }
}

#[derive(Resource)]
struct Samples {
    frame: u32,
    previous: Instant,
    intervals: Vec<f64>,
    diagnostics: BTreeMap<String, Vec<f64>>,
    metadata: Value,
    target: Handle<Image>,
    requested: bool,
    started: Instant,
}

fn main() {
    let options = Options::parse().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    if let Some(parent) = options.output.parent() {
        std::fs::create_dir_all(parent).expect("output directory");
    }
    if let Some(parent) = options.screenshot.as_ref().and_then(|path| path.parent()) {
        std::fs::create_dir_all(parent).expect("screenshot directory");
    }
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .add_plugins((
        BuildingRuntimePlugin,
        BuildingGrubPlugin,
        BuildingArrayPlugin,
        RenderDiagnosticsPlugin,
        ScheduleRunnerPlugin::default(),
    ))
    .insert_resource(ClearColor(Color::srgb(0.60, 0.71, 0.76)))
    .insert_resource(GlobalAmbientLight {
        brightness: 450.,
        ..default()
    })
    .insert_resource(options)
    .add_systems(Startup, setup)
    .add_systems(Last, sample);
    // One shared flag crosses the render/main-world boundary; no per-frame allocation.
    let readiness = PipelinesReady::default();
    app.insert_resource(readiness.clone());
    app.sub_app_mut(RenderApp)
        .insert_resource(readiness)
        .add_systems(Render, check_pipelines.in_set(RenderSystems::Cleanup));
    app.run();
}

fn check_pipelines(
    cache: Res<PipelineCache>,
    ready: Res<PipelinesReady>,
    device: Res<RenderDevice>,
) {
    let mut complete = cache.pipelines().next().is_some();
    for pipeline in cache.pipelines() {
        match &pipeline.state {
            CachedPipelineState::Ok(_) => {}
            CachedPipelineState::Queued
            | CachedPipelineState::Creating(_)
            | CachedPipelineState::Err(
                ShaderCacheError::ShaderNotLoaded(_)
                | ShaderCacheError::ShaderImportNotYetAvailable,
            ) => complete = false,
            CachedPipelineState::Err(error) => panic!("Benchmark shader failed: {error}"),
        }
    }
    // Offscreen images have no swapchain pacing: wait for submitted rendering
    // before the next sample, rather than letting CPU submission outrun the GPU.
    device
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(20)),
        })
        .expect("benchmark frame completed");
    ready.0.store(
        complete && cache.waiting_pipelines().next().is_none(),
        Ordering::Release,
    );
}

fn setup(
    mut commands: Commands,
    options: Res<Options>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut grub: ResMut<Assets<GrubMaterial>>,
    mut arrays: ResMut<Assets<BuildingArrayMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let started = Instant::now();
    let (recipe, base) = load_recipe(&options);
    let mut prototypes = Vec::new();
    for variant in 0..options.variants {
        let mut recipe = recipe.clone();
        recipe.seed = recipe.seed.wrapping_add(u64::from(variant));
        let source = Arc::new(generate_with_base(&recipe, &base).expect("valid recipe"));
        prototypes.push(if options.arrays {
            compile_building_with_arrays(
                source,
                &mut meshes,
                &mut materials,
                &mut grub,
                &mut arrays,
                &mut images,
            )
        } else {
            compile_building_with_grub(source, &mut meshes, &mut materials, &mut grub, &mut images)
        });
    }
    let side = f32::from(options.buildings).sqrt().ceil();
    let columns = (1..=options.buildings)
        .find(|value| f32::from(*value) >= side)
        .expect("grid columns");
    let spacing = recipe.width.max(recipe.depth) + 5.;
    for index in 0..options.buildings {
        let prototype = Arc::clone(&prototypes[usize::from(index % options.variants)]);
        let x = (f32::from(index % columns) - (side - 1.) * 0.5) * spacing;
        let z = (f32::from(index / columns) - (side - 1.) * 0.5) * spacing;
        let root = spawn_building(
            &mut commands,
            Arc::clone(&prototype),
            format!("benchmark-{index}"),
            Transform::from_xyz(x, 0., z),
        );
        let mut instance = BuildingInstance::new(prototype, format!("benchmark-{index}"));
        instance.lod_mode = [LodMode::Lod0, LodMode::Lod1, LodMode::Lod2][options.lod];
        commands.entity(root).insert(instance);
    }
    let extent = spacing * side;
    let target = spawn_scene(
        &mut commands,
        &recipe,
        extent,
        options.normal_prepass,
        options.ssao,
        &mut meshes,
        &mut materials,
        &mut images,
    );
    let texture_bytes: usize = images
        .iter()
        .filter_map(|(_, image)| image.data.as_ref())
        .map(Vec::len)
        .sum();
    commands.insert_resource(Samples {
        frame: 0,
        previous: Instant::now(),
        intervals: Vec::new(),
        diagnostics: BTreeMap::new(),
        metadata: json!({
            "renderer": if options.arrays { "arrays" } else { "standard" },
            "buildings": options.buildings,
            "variants": options.variants,
            "lod": options.lod,
            "resolution": [1600,1100],
            "warmup_frames": options.warmup,
            "sample_frames": options.frames,
            "normal_depth_motion_prepass": options.normal_prepass,
            "ssao": options.ssao,
            "generation_and_compilation_ms": started.elapsed().as_secs_f64()*1000.,
            "initial_image_payload_bytes": texture_bytes,
            "static_sections_per_prototype": prototypes.iter().map(|p| p.lods[options.lod].len()).collect::<Vec<_>>(),
            "door_sections_per_prototype": prototypes.iter().map(|p| p.doors.iter().chain(&p.door_hardware).filter(|p| p.triangles>0).count()).collect::<Vec<_>>(),
            "triangles_per_prototype": prototypes.iter().map(|p| p.source.lods[options.lod].triangle_count()).collect::<Vec<_>>(),
            "limits": ["Section counts are not measured GPU draw submissions.", "Frame intervals include CPU work and GPU-synchronized offscreen frame completion, not isolated GPU durations or presented game FPS.", "Bevy render diagnostics expose CPU timing only on Metal.", "Image payload excludes driver allocations and is not measured VRAM."]
        }),
        target,
        requested: false,
        started: Instant::now(),
    });
}

fn load_recipe(options: &Options) -> (BuildingRecipe, PathBuf) {
    if let Some(path) = &options.recipe {
        let document = StudioDocument::open(path).expect("recipe");
        (
            document.recipe().clone(),
            document
                .base_directory()
                .expect("recipe directory")
                .to_path_buf(),
        )
    } else {
        let recipe = match options.preset.as_str() {
            "church" => BuildingRecipe::gothic_church(),
            "hut" => BuildingRecipe::hut(),
            "workshop" => BuildingRecipe::workshop(),
            _ => BuildingRecipe::cottage(),
        };
        (recipe, PathBuf::from("."))
    }
}

fn spawn_scene(
    commands: &mut Commands,
    recipe: &BuildingRecipe,
    extent: f32,
    normal_prepass: bool,
    ssao: bool,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Handle<Image> {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(extent * 2., extent * 2.))),
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
    ));
    let mut image = Image::new_target_texture(1600, 1100, TextureFormat::Rgba8UnormSrgb, None);
    image
        .texture_descriptor
        .usage
        .remove(TextureUsages::TEXTURE_BINDING);
    let target = images.add(image);
    let mut camera = commands.spawn((
        Camera3d::default(),
        bevy::camera::RenderTarget::Image(target.clone().into()),
        bevy::camera::Exposure::default(),
        bevy::camera::ShadowLodOrigin,
        Transform::from_xyz(extent * 0.68, extent * 0.6, extent * 0.8)
            .looking_at(Vec3::new(0., recipe.wall_height * 0.4, 0.), Vec3::Y),
    ));
    if normal_prepass {
        camera.insert((
            bevy::core_pipeline::prepass::NormalPrepass,
            bevy::core_pipeline::prepass::DepthPrepass,
            bevy::core_pipeline::prepass::MotionVectorPrepass,
        ));
    }
    if ssao {
        camera.insert((
            bevy::pbr::ScreenSpaceAmbientOcclusion::default(),
            Msaa::Off,
            bevy::core_pipeline::prepass::MotionVectorPrepass,
        ));
    }
    target
}

fn sample(
    mut commands: Commands,
    options: Res<Options>,
    mut samples: ResMut<Samples>,
    diagnostics: Res<DiagnosticsStore>,
    ready: Res<PipelinesReady>,
    images: Res<Assets<Image>>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = Instant::now();
    let elapsed = now.duration_since(samples.previous).as_secs_f64() * 1000.;
    samples.previous = now;
    if samples.requested {
        if options
            .screenshot
            .as_ref()
            .is_some_and(|path| path.exists())
        {
            exit.write(AppExit::Success);
        }
        return;
    }
    if !ready.0.load(Ordering::Acquire) {
        assert!(
            samples.started.elapsed().as_secs() < 120,
            "Benchmark pipelines did not become ready"
        );
        samples.frame = 0;
        samples.intervals.clear();
        samples.diagnostics.clear();
        return;
    }
    samples.frame += 1;
    if samples.frame <= options.warmup {
        return;
    }
    samples.intervals.push(elapsed);
    for diagnostic in diagnostics.iter() {
        if diagnostic.path().as_str().starts_with("render/")
            && diagnostic.path().as_str().ends_with("elapsed_cpu")
            && let Some(value) = diagnostic.value()
        {
            samples
                .diagnostics
                .entry(diagnostic.path().as_str().into())
                .or_default()
                .push(value);
        }
    }
    if samples.intervals.len() < usize::try_from(options.frames).expect("bounded sample count") {
        return;
    }
    let mut output = samples.metadata.clone();
    output["image_payload_bytes_after_warmup"] = images
        .iter()
        .filter_map(|(_, image)| image.data.as_ref())
        .map(Vec::len)
        .sum::<usize>()
        .into();
    output["frame_interval_ms"] = statistics(&samples.intervals);
    output["render_diagnostics"] = samples
        .diagnostics
        .iter()
        .map(|(key, values)| (key.clone(), statistics(values)))
        .collect();
    std::fs::write(
        &options.output,
        serde_json::to_vec_pretty(&output).expect("metrics JSON"),
    )
    .expect("write metrics");
    println!("Wrote {}", options.output.display());
    samples.requested = true;
    if let Some(path) = &options.screenshot {
        commands
            .spawn(Screenshot::image(samples.target.clone()))
            .observe(save_to_disk(path.clone()));
    } else {
        exit.write(AppExit::Success);
    }
}

#[allow(clippy::cast_precision_loss)] // Bounded sample counts, at most 10,000 frames.
fn statistics(samples: &[f64]) -> Value {
    let mut ordered = samples.to_vec();
    ordered.sort_by(f64::total_cmp);
    json!({
        "count": ordered.len(),
        "mean": ordered.iter().sum::<f64>()/ordered.len() as f64,
        "median": ordered[ordered.len()/2],
        "p95": ordered[(ordered.len()-1)*95/100],
        "min": ordered[0],
        "max": ordered[ordered.len()-1],
    })
}
