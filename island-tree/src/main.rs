//! Interactive and offline visual laboratory for procedural vegetation.
//!
//! A normal run opens the tree studio. `--screenshot` keeps the repeatable
//! headless capture path used by visual acceptance.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments
)]

mod studio;

use std::{
    env, fs,
    path::PathBuf,
    process,
    time::{Duration, Instant},
};

use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    app::ScheduleRunnerPlugin,
    asset::RenderAssetUsages,
    camera::{Exposure, Hdr, RenderTarget},
    core_pipeline::tonemapping::Tonemapping,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, GlobalAmbientLight, NotShadowCaster,
        TransmittedShadowReceiver, atmosphere::ScatteringMedium,
    },
    mesh::{
        Indices, PrimitiveTopology, VertexAttributeValues,
        skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
    },
    pbr::{AtmosphereSettings, ContactShadows, ParallaxMappingMethod, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, save_to_disk},
    },
    window::{ExitCondition, PresentMode, WindowPlugin, WindowResolution},
    winit::WinitPlugin,
};
use island_tree::{
    Axis, AxisGraph, BarkMaterial, BarkMaterialPlugin, BarkVertex, BotanicalPrototype,
    BotanicalRecipe, BotanicalTexture, FoliagePad, LEAF_ARCHETYPE_COUNT, LeafMaterial,
    LeafMaterialPlugin, LeafOrgan, ShootTipOrgan, ShootTipState, compile_botanical_impostor,
    generate_botanical_prototype,
};
use motu::Mesh as MotuMesh;

const CAPTURE_RESOLUTION: UVec2 = UVec2::new(2048, 1536);
const SETTLE_FRAMES: u32 = 192;
const FLUSH_FRAMES: u32 = 6;
const CAPTURE_TIMEOUT_FRAMES: u32 = 900;
const MAX_WIND_JOINTS: usize = 256;
const LEAF_EXPOSURE_MATERIAL_COUNT: usize = 3;
const LEAF_PIGMENT_MATERIAL_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ReviewLod {
    #[default]
    Near,
    Middle,
    Far,
}

impl ReviewLod {
    const ALL: [Self; 3] = [Self::Near, Self::Middle, Self::Far];

    const fn label(self) -> &'static str {
        match self {
            Self::Near => "LOD 0 · Near",
            Self::Middle => "LOD 1 · Middle",
            Self::Far => "LOD 2 · Far impostor",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "near" => Ok(Self::Near),
            "middle" => Ok(Self::Middle),
            "far" => Ok(Self::Far),
            _ => Err(format!(
                "unknown LOD {value:?}; expected near, middle, or far"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ReviewView {
    #[default]
    Whole,
    WholeQuarter,
    Crown,
    Detail,
    Leaf,
    Tip,
    Root,
    Scar,
    Epicormic,
    Junction,
}

impl ReviewView {
    const ALL: [Self; 10] = [
        Self::Whole,
        Self::WholeQuarter,
        Self::Crown,
        Self::Detail,
        Self::Leaf,
        Self::Tip,
        Self::Root,
        Self::Scar,
        Self::Epicormic,
        Self::Junction,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Whole => "Whole",
            Self::WholeQuarter => "Quarter",
            Self::Crown => "Crown",
            Self::Detail => "Detail",
            Self::Leaf => "Leaf",
            Self::Tip => "Tip",
            Self::Root => "Root",
            Self::Scar => "Scar",
            Self::Epicormic => "Epicormic",
            Self::Junction => "Junction",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "whole" => Ok(Self::Whole),
            "whole-quarter" => Ok(Self::WholeQuarter),
            "crown" => Ok(Self::Crown),
            "detail" => Ok(Self::Detail),
            "leaf" => Ok(Self::Leaf),
            "tip" => Ok(Self::Tip),
            "root" => Ok(Self::Root),
            "scar" => Ok(Self::Scar),
            "epicormic" => Ok(Self::Epicormic),
            "junction" => Ok(Self::Junction),
            _ => Err(format!(
                "unknown view {value:?}; expected whole, whole-quarter, crown, detail, leaf, tip, root, scar, epicormic, or junction"
            )),
        }
    }

    fn frame(self, prototype: &BotanicalPrototype) -> ReviewFrame {
        if self == Self::Scar {
            return scar_review_frame(&prototype.wood_scars);
        }
        if self == Self::Epicormic {
            return epicormic_review_frame(prototype);
        }
        if self == Self::Junction {
            return junction_review_frame(prototype);
        }
        let (eye, target) = match self {
            Self::Whole => (Vec3::new(16.6, 6.0, 18.4), Vec3::new(0.0, 4.6, 0.0)),
            Self::WholeQuarter => (Vec3::new(-18.4, 6.0, 16.6), Vec3::new(0.0, 4.6, 0.0)),
            Self::Crown => (Vec3::new(9.8, 8.1, 10.7), Vec3::new(0.0, 6.7, 0.0)),
            Self::Detail => (Vec3::new(4.2, 6.4, 4.5), Vec3::new(0.2, 5.7, 0.0)),
            Self::Leaf => (Vec3::new(7.2, 7.8, 6.3), Vec3::new(3.2, 7.2, 2.4)),
            Self::Tip => (Vec3::new(4.65, 7.45, 3.75), Vec3::new(3.2, 7.2, 2.4)),
            Self::Root => (Vec3::new(4.4, 1.7, 4.7), Vec3::new(0.0, 0.75, 0.0)),
            Self::Scar | Self::Epicormic | Self::Junction => {
                unreachable!("specialist views return above")
            }
        };
        ReviewFrame::new(eye, target, Vec3::Y)
    }
}

#[derive(Clone, Copy, Debug)]
struct ReviewFrame {
    transform: Transform,
    target: Vec3,
}

impl ReviewFrame {
    fn new(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        Self {
            transform: Transform::from_translation(eye).looking_at(target, up),
            target,
        }
    }
}

#[derive(Resource)]
struct ReviewFrames([(ReviewView, ReviewFrame); ReviewView::ALL.len()]);

impl ReviewFrames {
    fn new(prototype: &BotanicalPrototype) -> Self {
        Self(ReviewView::ALL.map(|view| (view, view.frame(prototype))))
    }

    fn get(&self, view: ReviewView) -> ReviewFrame {
        self.0
            .iter()
            .find_map(|(candidate, frame)| (*candidate == view).then_some(*frame))
            .expect("every review view has a frame")
    }
}

fn junction_review_frame(prototype: &BotanicalPrototype) -> ReviewFrame {
    let candidate = prototype
        .graph
        .axes
        .iter()
        .filter_map(|child| {
            let parent = prototype.graph.axes.get(child.parent? as usize)?;
            let [_, _, _, before_tip, tip] = parent.points_metres;
            let parent_direction = (tip - before_tip).normalize_or(motu::Vec3::Z);
            let child_direction =
                (child.points_metres[2] - child.points_metres[0]).normalize_or(parent_direction);
            (child.alive
                && matches!(child.order, 1 | 2)
                && parent_direction.dot(child_direction) < 0.72)
                .then_some((child, parent_direction, child_direction))
        })
        .max_by(|left, right| left.0.radii_metres[0].total_cmp(&right.0.radii_metres[0]));
    let Some((child, parent_direction, child_direction)) = candidate else {
        return ReviewFrame::new(Vec3::new(4.2, 6.4, 4.5), Vec3::new(0.2, 5.7, 0.0), Vec3::Y);
    };
    let target = convert(child.points_metres[0] + child_direction * (child.radii_metres[0] * 0.45));
    let broadside = convert(parent_direction.cross(child_direction)).normalize_or(Vec3::X);
    let up = convert(parent_direction).normalize_or(Vec3::Y);
    let distance = (child.radii_metres[0] * 13.0).clamp(2.4, 4.2);
    ReviewFrame::new(
        target + broadside * distance + up * distance * 0.12,
        target,
        up,
    )
}

fn epicormic_review_frame(prototype: &BotanicalPrototype) -> ReviewFrame {
    let epicormic_leaves = prototype
        .leaves
        .iter()
        .filter(|leaf| prototype.graph.axes[leaf.axis as usize].order == 0)
        // One generated epicormic shoot owns five leaves. Framing the first
        // shoot avoids averaging opposite trunk faces into the stem centre.
        .take(5);
    let (centre, count) = epicormic_leaves.fold((motu::Vec3::ZERO, 0_u32), |(sum, count), leaf| {
        (
            sum + leaf.blade_base_metres + leaf.direction * (leaf.length_metres * 0.5),
            count + 1,
        )
    });
    if count == 0 {
        return ReviewFrame::new(Vec3::new(4.4, 1.7, 4.7), Vec3::new(0.0, 0.75, 0.0), Vec3::Y);
    }
    let target = convert(centre / count as f32);
    let outward = Vec3::new(target.x, 0.0, target.z).normalize_or(Vec3::X);
    ReviewFrame::new(target + outward * 1.05 + Vec3::Y * 0.16, target, Vec3::Y)
}

fn scar_review_frame(wood_scars: &MotuMesh) -> ReviewFrame {
    const SCAR_VERTICES: usize = 18;
    let Some(vertices) = wood_scars.vertices.get(..SCAR_VERTICES) else {
        return ReviewFrame::new(Vec3::new(4.2, 6.4, 4.5), Vec3::new(0.2, 5.7, 0.0), Vec3::Y);
    };
    let ring_centre = vertices[1..]
        .iter()
        .copied()
        .fold(motu::Vec3::ZERO, |sum, vertex| sum + vertex)
        / (SCAR_VERTICES - 1) as f32;
    let outward = convert(ring_centre - vertices[0]).normalize_or(Vec3::Z);
    let target = convert(ring_centre);
    let up = if outward.dot(Vec3::Y).abs() > 0.90 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    ReviewFrame::new(target + outward * 0.42 + up * 0.025, target, up)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ReviewLight {
    #[default]
    Front,
    Back,
    Grazing,
}

impl ReviewLight {
    const ALL: [Self; 3] = [Self::Front, Self::Back, Self::Grazing];

    const fn label(self) -> &'static str {
        match self {
            Self::Front => "Front",
            Self::Back => "Backlit",
            Self::Grazing => "Grazing",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "front" => Ok(Self::Front),
            "back" => Ok(Self::Back),
            "grazing" => Ok(Self::Grazing),
            _ => Err(format!(
                "unknown light {value:?}; expected front, back, or grazing"
            )),
        }
    }

    fn direction(self) -> Vec3 {
        match self {
            Self::Front => Vec3::new(-0.46, -0.72, -0.52),
            Self::Back => Vec3::new(0.54, -0.64, 0.55),
            Self::Grazing => Vec3::new(-0.91, -0.22, 0.35),
        }
        .normalize()
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
struct Settings {
    seed: u64,
    recipe: BotanicalRecipe,
    lod: ReviewLod,
    view: ReviewView,
    light: ReviewLight,
    foliage: bool,
    fine_shoots: bool,
    wind_phase: f32,
    wind_strength: f32,
    screenshot: Option<PathBuf>,
    capture_ui: bool,
}

#[derive(Component)]
struct TreeRoot;

#[derive(Component)]
struct ReviewCamera {
    target: Vec3,
}

#[derive(Component)]
struct ReviewSun;

#[derive(Component)]
struct ReviewGround;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TreeMetrics {
    axes: usize,
    leaves: usize,
    shoot_tips: usize,
    foliage_pads: usize,
    wood_triangles: usize,
    scar_triangles: usize,
    generation_millis: u128,
}

impl TreeMetrics {
    fn new(prototype: &BotanicalPrototype, generation_millis: u128) -> Self {
        Self {
            axes: prototype.graph.axes.len(),
            leaves: prototype.leaves.len(),
            shoot_tips: prototype.shoot_tips.len(),
            foliage_pads: prototype.foliage_pads.len(),
            wood_triangles: prototype.wood.triangles.len() / 3,
            scar_triangles: prototype.wood_scars.triangles.len() / 3,
            generation_millis,
        }
    }
}

#[derive(Resource, Debug, Default)]
struct TreeBuildStatus {
    error: Option<String>,
    generating: bool,
    notice_seconds: f32,
}

#[derive(Message, Clone, Debug)]
struct RegenerateTree(Settings);

#[derive(Component)]
struct WindJoint {
    rest_translation: Vec3,
    bend_axis: Vec3,
    phase: f32,
    flexibility: f32,
}

#[derive(Component)]
struct LeafFlutter {
    rest_rotation: Quat,
    phase: f32,
    amplitude: f32,
}

#[derive(Debug)]
struct SkeletonPlan {
    selected_axes: Vec<usize>,
    axis_to_joint: Vec<usize>,
    parent_joints: Vec<Option<usize>>,
    origins: Vec<Vec3>,
    phases: Vec<f32>,
}

struct WindSkeleton {
    joints: Vec<Entity>,
    inverse_bindposes: Handle<SkinnedMeshInverseBindposes>,
    selected_axes: Vec<usize>,
    axis_to_joint: Vec<usize>,
    origins: Vec<Vec3>,
    parent_joints: Vec<Option<usize>>,
}

struct SkinWeights {
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
}

#[derive(Resource)]
struct CaptureTarget {
    image: Handle<Image>,
    path: PathBuf,
}

#[derive(Resource, Default)]
struct CaptureProgress {
    frames: u32,
    requested: bool,
    frames_since_request: u32,
}

fn main() {
    let settings = match parse(env::args().skip(1)) {
        Ok(Some(settings)) => settings,
        Ok(None) => return,
        Err(error) => {
            eprintln!("tree-lab: {error}");
            process::exit(2);
        }
    };
    let capturing = settings.screenshot.is_some();
    let mut app = App::new();
    let mut plugins = DefaultPlugins.set(if capturing {
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        }
    } else {
        WindowPlugin {
            primary_window: Some(Window {
                title: "Island Tree Studio".into(),
                resolution: WindowResolution::new(1440, 900),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }
    });
    if capturing {
        plugins = plugins.disable::<WinitPlugin>();
    }
    app.add_plugins(plugins)
        .add_plugins(BarkMaterialPlugin)
        .add_plugins(LeafMaterialPlugin)
        .insert_resource(ClearColor(Color::srgb(0.72, 0.82, 0.88)))
        .insert_resource(GlobalAmbientLight::NONE)
        .insert_resource(settings.clone());
    if let Some(path) = settings.screenshot.clone() {
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
        install_capture(&mut app, path);
        app.add_systems(Update, capture.after(apply_wind));
    }
    if !capturing || settings.capture_ui {
        app.add_plugins(studio::TreeStudioPlugin);
    }
    app.add_systems(Startup, setup)
        .add_systems(Update, apply_wind)
        .run();
}

fn install_capture(app: &mut App, path: PathBuf) {
    let mut image = Image::new_target_texture(
        CAPTURE_RESOLUTION.x,
        CAPTURE_RESOLUTION.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    );
    image
        .texture_descriptor
        .usage
        .remove(TextureUsages::TEXTURE_BINDING);
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.insert_resource(CaptureTarget { image, path })
        .init_resource::<CaptureProgress>();
}

fn setup(
    mut commands: Commands,
    settings: Res<Settings>,
    target: Option<Res<CaptureTarget>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bark_materials: ResMut<Assets<BarkMaterial>>,
    mut leaf_materials: ResMut<Assets<LeafMaterial>>,
    mut inverse_bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut images: ResMut<Assets<Image>>,
    mut mediums: ResMut<Assets<ScatteringMedium>>,
) {
    if let Some(target) = target.as_deref()
        && let Err(error) = fs::remove_file(&target.path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!("could not remove stale {}: {error}", target.path.display());
    }
    let (prototype, metrics) = generate_review_prototype(&settings)
        .unwrap_or_else(|error| panic!("botanical prototype failed: {error}"));
    log_prototype(&settings, &prototype, metrics);
    let frames = ReviewFrames::new(&prototype);
    let camera_frame = frames.get(settings.view);
    spawn_tree(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut bark_materials,
        &mut leaf_materials,
        &mut inverse_bindposes,
        &mut images,
        &settings,
        prototype,
    );
    commands.insert_resource(frames);
    commands.insert_resource(metrics);
    commands.init_resource::<TreeBuildStatus>();
    spawn_stage(&mut commands, &mut meshes, &mut materials);
    spawn_lighting(&mut commands, &settings, &mut mediums);
    let mut camera = commands.spawn((
        Name::new("Tree review camera"),
        ReviewCamera {
            target: camera_frame.target,
        },
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 45.0_f32.to_radians(),
            near: 0.1,
            far: 150.0,
            ..default()
        }),
        Hdr,
        Exposure {
            ev100: Exposure::EV100_SUNLIGHT - 2.4,
        },
        Tonemapping::AcesFitted,
        AtmosphereSettings::default(),
        AtmosphereEnvironmentMapLight {
            intensity: 1.8,
            size: UVec2::new(256, 256),
            ..default()
        },
        Msaa::Off,
        TemporalAntiAliasing::default(),
        ScreenSpaceAmbientOcclusion::default(),
        ContactShadows {
            length: 2.5,
            thickness: 0.35,
            ..default()
        },
        Bloom {
            intensity: 0.025,
            ..Bloom::NATURAL
        },
        camera_frame.transform,
    ));
    if let Some(target) = target {
        camera.insert(RenderTarget::Image(target.image.clone().into()));
    }
}

fn generate_review_prototype(
    settings: &Settings,
) -> Result<(BotanicalPrototype, TreeMetrics), String> {
    let started = Instant::now();
    let prototype = generate_botanical_prototype(settings.seed, settings.recipe)?;
    let metrics = TreeMetrics::new(&prototype, started.elapsed().as_millis());
    Ok((prototype, metrics))
}

fn log_prototype(settings: &Settings, prototype: &BotanicalPrototype, metrics: TreeMetrics) {
    let exposure_counts = prototype
        .leaves
        .iter()
        .fold([0_usize; 3], |mut counts, leaf| {
            counts[leaf_exposure_bin(leaf.light_exposure)] += 1;
            counts
        });
    let tip_counts = prototype
        .shoot_tips
        .iter()
        .fold([0_usize; 3], |mut counts, tip| {
            let state = match tip.state {
                ShootTipState::ActiveBud => 0,
                ShootTipState::DormantBud => 1,
                ShootTipState::Broken => 2,
            };
            counts[state] += 1;
            counts
        });
    info!(
        "seed {}: {} axes, {} leaves, exposure bins {:?}, tip states {:?}, {} foliage pads, {} wood triangles, {} scar triangles in {} ms",
        settings.seed,
        metrics.axes,
        metrics.leaves,
        exposure_counts,
        tip_counts,
        metrics.foliage_pads,
        metrics.wood_triangles,
        metrics.scar_triangles,
        metrics.generation_millis,
    );
}

fn regenerate_tree(
    mut commands: Commands,
    mut requests: MessageReader<RegenerateTree>,
    roots: Query<Entity, With<TreeRoot>>,
    mut settings: ResMut<Settings>,
    mut frames: ResMut<ReviewFrames>,
    mut metrics: ResMut<TreeMetrics>,
    mut status: ResMut<TreeBuildStatus>,
    mut cameras: Query<(&mut ReviewCamera, &mut Transform), Without<ReviewSun>>,
    mut suns: Query<&mut Transform, (With<ReviewSun>, Without<ReviewCamera>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bark_materials: ResMut<Assets<BarkMaterial>>,
    mut leaf_materials: ResMut<Assets<LeafMaterial>>,
    mut inverse_bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(RegenerateTree(request)) = requests.read().last().cloned() else {
        return;
    };
    let (prototype, next_metrics) = match generate_review_prototype(&request) {
        Ok(generated) => generated,
        Err(error) => {
            status.error = Some(error);
            status.generating = false;
            status.notice_seconds = 0.0;
            return;
        }
    };
    log_prototype(&request, &prototype, next_metrics);
    let next_frames = ReviewFrames::new(&prototype);
    let frame = next_frames.get(request.view);
    for root in &roots {
        commands.entity(root).despawn();
    }
    spawn_tree(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut bark_materials,
        &mut leaf_materials,
        &mut inverse_bindposes,
        &mut images,
        &request,
        prototype,
    );
    for (mut camera, mut transform) in &mut cameras {
        camera.target = frame.target;
        *transform = frame.transform;
    }
    for mut transform in &mut suns {
        *transform = Transform::default().looking_to(request.light.direction(), Vec3::Y);
    }
    *settings = request;
    *frames = next_frames;
    *metrics = next_metrics;
    status.error = None;
    status.generating = false;
    status.notice_seconds = 0.8;
}

#[allow(clippy::too_many_lines)]
fn spawn_tree(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    bark_materials: &mut Assets<BarkMaterial>,
    leaf_materials: &mut Assets<LeafMaterial>,
    inverse_bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    images: &mut Assets<Image>,
    settings: &Settings,
    prototype: BotanicalPrototype,
) {
    let tree_root = commands
        .spawn((
            Name::new("Generated pōhutukawa"),
            TreeRoot,
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    if settings.lod == ReviewLod::Far {
        let impostor = compile_botanical_impostor(&prototype, meshes, images, materials)
            .unwrap_or_else(|error| panic!("tree impostor failed: {error}"));
        commands.spawn((
            Name::new("Pōhutukawa generated far impostor"),
            Mesh3d(impostor.mesh),
            MeshMaterial3d(impostor.material),
            NotShadowCaster,
            ChildOf(tree_root),
        ));
        return;
    }
    let BotanicalPrototype {
        graph,
        wood,
        wood_bark,
        wood_scars,
        wood_scar_albedo,
        microtwigs,
        microtwig_bark,
        leaf_archetypes,
        shoot_tip_archetypes,
        foliage_pad_archetypes,
        leaves,
        shoot_tips,
        foliage_pads,
        bark_albedo,
        bark_normal,
        bark_depth,
        bark_metallic_roughness,
        leaf_albedo,
        leaf_metallic_roughness,
    } = prototype;
    let skeleton = spawn_wind_skeleton(
        commands,
        inverse_bindposes,
        &graph,
        settings.seed,
        tree_root,
    );
    let wood_material = build_bark_material(
        images,
        bark_materials,
        settings.lod,
        bark_albedo,
        bark_normal,
        bark_depth,
        bark_metallic_roughness,
    );
    let leaf_texture = images.add(texture_image(leaf_albedo, false, true));
    let leaf_metallic_roughness = images.add(texture_image(leaf_metallic_roughness, false, false));
    let leaf_materials =
        build_leaf_materials(leaf_materials, &leaf_texture, &leaf_metallic_roughness);
    let shoot_tip_materials = build_shoot_tip_materials(materials);
    let microtwig_material =
        (settings.fine_shoots && settings.lod == ReviewLod::Near).then(|| wood_material.clone());
    spawn_wood(
        commands,
        meshes,
        wood,
        wood_bark,
        wood_material,
        &graph,
        &skeleton,
        tree_root,
    );
    spawn_scaffold_scars(
        commands,
        meshes,
        materials,
        images,
        wood_scars,
        wood_scar_albedo,
        &graph,
        &skeleton,
        tree_root,
    );
    spawn_microtwigs(
        commands,
        meshes,
        microtwigs,
        microtwig_bark,
        microtwig_material,
        &graph,
        &skeleton,
        tree_root,
    );
    if settings.fine_shoots && settings.lod == ReviewLod::Near {
        spawn_shoot_tips(
            commands,
            meshes,
            &shoot_tip_materials,
            shoot_tip_archetypes,
            shoot_tips,
            &skeleton,
        );
    }
    if !settings.foliage {
        return;
    }
    match settings.lod {
        ReviewLod::Near => spawn_leaves(
            commands,
            meshes,
            &leaf_materials,
            leaf_archetypes,
            leaves,
            &skeleton,
        ),
        ReviewLod::Middle => spawn_pads(
            commands,
            meshes,
            &leaf_materials,
            foliage_pad_archetypes,
            foliage_pads,
            &skeleton,
        ),
        ReviewLod::Far => unreachable!("far LOD returns before organ compilation"),
    }
}

fn spawn_wood(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    wood: MotuMesh,
    bark: Vec<BarkVertex>,
    material: Handle<BarkMaterial>,
    graph: &AxisGraph,
    skeleton: &WindSkeleton,
    tree_root: Entity,
) {
    let skin = skin_weights(&wood, graph, skeleton);
    commands.spawn((
        Name::new("Pōhutukawa wood"),
        Mesh3d(meshes.add(bevy_wood_mesh(&wood, &bark, Some(&skin)))),
        MeshMaterial3d(material),
        SkinnedMesh {
            inverse_bindposes: skeleton.inverse_bindposes.clone(),
            joints: skeleton.joints.clone(),
        },
        ChildOf(tree_root),
    ));
}

fn spawn_microtwigs(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    twigs: MotuMesh,
    bark: Vec<BarkVertex>,
    material: Option<Handle<BarkMaterial>>,
    graph: &AxisGraph,
    skeleton: &WindSkeleton,
    tree_root: Entity,
) {
    let Some(material) = material else {
        return;
    };
    let skin = skin_weights(&twigs, graph, skeleton);
    commands.spawn((
        Name::new("Pōhutukawa microtwigs"),
        Mesh3d(meshes.add(bevy_wood_mesh(&twigs, &bark, Some(&skin)))),
        MeshMaterial3d(material),
        SkinnedMesh {
            inverse_bindposes: skeleton.inverse_bindposes.clone(),
            joints: skeleton.joints.clone(),
        },
        ChildOf(tree_root),
    ));
}

fn spawn_scaffold_scars(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    scars: MotuMesh,
    albedo: BotanicalTexture,
    graph: &AxisGraph,
    skeleton: &WindSkeleton,
    tree_root: Entity,
) {
    if scars.vertices.is_empty() {
        return;
    }
    let skin = skin_weights(&scars, graph, skeleton);
    let texture = images.add(texture_image(albedo, false, true));
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(texture),
        perceptual_roughness: 0.94,
        reflectance: 0.22,
        ..default()
    });
    commands.spawn((
        Name::new("Pōhutukawa weathered scaffold scars"),
        Mesh3d(meshes.add(bevy_mesh(&scars, None, Some(&skin)))),
        MeshMaterial3d(material),
        SkinnedMesh {
            inverse_bindposes: skeleton.inverse_bindposes.clone(),
            joints: skeleton.joints.clone(),
        },
        ChildOf(tree_root),
    ));
}

fn build_bark_material(
    images: &mut Assets<Image>,
    materials: &mut Assets<BarkMaterial>,
    lod: ReviewLod,
    albedo: BotanicalTexture,
    normal: BotanicalTexture,
    depth: BotanicalTexture,
    metallic_roughness: BotanicalTexture,
) -> Handle<BarkMaterial> {
    let albedo = images.add(texture_image(albedo, true, true));
    let normal = images.add(texture_image(normal, true, false));
    let depth = images.add(texture_image(depth, true, false));
    let metallic_roughness = images.add(texture_image(metallic_roughness, true, false));
    materials.add(BarkMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(albedo),
            normal_map_texture: Some(normal),
            depth_map: (lod == ReviewLod::Near).then_some(depth),
            parallax_depth_scale: 0.032,
            parallax_mapping_method: ParallaxMappingMethod::Relief { max_steps: 8 },
            max_parallax_layer_count: 16.0,
            metallic_roughness_texture: Some(metallic_roughness),
            perceptual_roughness: 1.0,
            reflectance: 0.28,
            ..default()
        },
        extension: default(),
    })
}

fn build_shoot_tip_materials(
    materials: &mut Assets<StandardMaterial>,
) -> [Handle<StandardMaterial>; 3] {
    [
        Color::srgb(0.24, 0.32, 0.11),
        Color::srgb(0.31, 0.22, 0.11),
        Color::srgb(0.50, 0.36, 0.20),
    ]
    .map(|base_color| {
        materials.add(StandardMaterial {
            base_color,
            perceptual_roughness: 0.88,
            reflectance: 0.24,
            ..default()
        })
    })
}

fn spawn_shoot_tips(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &[Handle<StandardMaterial>; 3],
    archetypes: [MotuMesh; 2],
    tips: Vec<ShootTipOrgan>,
    skeleton: &WindSkeleton,
) {
    let handles = archetypes.map(|mesh| meshes.add(bevy_mesh(&mesh, None, None)));
    for tip in tips {
        let (archetype, material) = match tip.state {
            ShootTipState::ActiveBud => (0, 0),
            ShootTipState::DormantBud => (0, 1),
            ShootTipState::Broken => (1, 2),
        };
        let joint = skeleton.axis_to_joint[tip.axis as usize];
        let mut transform = shoot_tip_transform(tip);
        transform.translation -= skeleton.origins[joint];
        commands.spawn((
            Mesh3d(handles[archetype].clone()),
            MeshMaterial3d(materials[material].clone()),
            transform,
            ChildOf(skeleton.joints[joint]),
        ));
    }
}

fn build_leaf_materials(
    materials: &mut Assets<LeafMaterial>,
    albedo: &Handle<Image>,
    metallic_roughness: &Handle<Image>,
) -> [[Handle<LeafMaterial>; LEAF_PIGMENT_MATERIAL_COUNT]; LEAF_EXPOSURE_MATERIAL_COUNT] {
    std::array::from_fn(|exposure| {
        let optics = leaf_optics(exposure);
        std::array::from_fn(|pigment| {
            let (pigment_tint, reflectance_offset, transmission_offset) = match pigment {
                0 => ([0.88, 0.93, 0.90], -0.015, 0.015),
                2 => ([1.00, 0.94, 0.84], 0.012, -0.015),
                _ => ([1.00, 1.00, 1.00], 0.0, 0.0),
            };
            let colour = Vec3::from_array(optics.base_color) * Vec3::from_array(pigment_tint);
            materials.add(LeafMaterial {
                base: StandardMaterial {
                    base_color: Color::srgb(colour.x, colour.y, colour.z),
                    base_color_texture: Some(albedo.clone()),
                    metallic_roughness_texture: Some(metallic_roughness.clone()),
                    perceptual_roughness: 1.0,
                    reflectance: optics.reflectance + reflectance_offset,
                    diffuse_transmission: optics.diffuse_transmission + transmission_offset,
                    thickness: optics.thickness,
                    attenuation_distance: optics.attenuation_distance,
                    attenuation_color: Color::srgb(0.30, 0.55, 0.20),
                    ior: 1.42,
                    clearcoat: optics.clearcoat,
                    clearcoat_perceptual_roughness: optics.clearcoat_roughness,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                },
                extension: default(),
            })
        })
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LeafOptics {
    base_color: [f32; 3],
    reflectance: f32,
    diffuse_transmission: f32,
    thickness: f32,
    attenuation_distance: f32,
    clearcoat: f32,
    clearcoat_roughness: f32,
}

fn leaf_optics(exposure: usize) -> LeafOptics {
    match exposure {
        0 => LeafOptics {
            base_color: [0.88, 0.93, 0.96],
            reflectance: 0.34,
            diffuse_transmission: 0.52,
            thickness: 0.000_32,
            attenuation_distance: 0.014,
            clearcoat: 0.22,
            clearcoat_roughness: 0.56,
        },
        2 => LeafOptics {
            base_color: [1.00, 0.94, 0.86],
            reflectance: 0.44,
            diffuse_transmission: 0.42,
            thickness: 0.000_50,
            attenuation_distance: 0.010,
            clearcoat: 0.40,
            clearcoat_roughness: 0.44,
        },
        _ => LeafOptics {
            base_color: [0.96, 0.97, 0.94],
            reflectance: 0.39,
            diffuse_transmission: 0.47,
            thickness: 0.000_40,
            attenuation_distance: 0.012,
            clearcoat: 0.30,
            clearcoat_roughness: 0.50,
        },
    }
}

fn leaf_exposure_bin(exposure: f32) -> usize {
    if exposure < 0.38 {
        0
    } else if exposure > 0.68 {
        2
    } else {
        1
    }
}

fn leaf_pigment_bin(age: f32, variation: f32) -> usize {
    let normalized_variation = (variation / std::f32::consts::TAU).rem_euclid(1.0);
    if normalized_variation < 0.26 {
        0
    } else if normalized_variation > 0.74 && age > 0.42 {
        2
    } else {
        1
    }
}

fn spawn_leaves(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &[[Handle<LeafMaterial>; LEAF_PIGMENT_MATERIAL_COUNT]; LEAF_EXPOSURE_MATERIAL_COUNT],
    archetypes: [MotuMesh; LEAF_ARCHETYPE_COUNT],
    leaves: Vec<LeafOrgan>,
    skeleton: &WindSkeleton,
) {
    let tints = [
        [1.00, 1.00, 1.00, 1.0],
        [0.96, 1.00, 0.94, 1.0],
        [1.00, 0.97, 0.88, 1.0],
        [0.92, 0.94, 0.84, 1.0],
        [1.00, 1.00, 1.00, 1.0],
        [0.96, 1.00, 0.94, 1.0],
        [1.00, 0.97, 0.88, 1.0],
        [0.92, 0.94, 0.84, 1.0],
    ];
    let handles: [_; LEAF_ARCHETYPE_COUNT] = std::array::from_fn(|index| {
        meshes.add(bevy_mesh(&archetypes[index], Some(tints[index]), None))
    });
    for leaf in leaves {
        let joint = skeleton.axis_to_joint[leaf.axis as usize];
        let mut transform = leaf_transform(leaf);
        transform.translation -= skeleton.origins[joint];
        let rest_rotation = transform.rotation;
        commands.spawn((
            Mesh3d(handles[usize::from(leaf.archetype)].clone()),
            MeshMaterial3d(
                materials[leaf_exposure_bin(leaf.light_exposure)]
                    [leaf_pigment_bin(leaf.age, leaf.variation)]
                .clone(),
            ),
            TransmittedShadowReceiver,
            transform,
            ChildOf(skeleton.joints[joint]),
            LeafFlutter {
                rest_rotation,
                phase: leaf.variation,
                amplitude: (0.035 + leaf.age * 0.040).clamp(0.035, 0.075),
            },
        ));
    }
}

fn spawn_pads(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &[[Handle<LeafMaterial>; LEAF_PIGMENT_MATERIAL_COUNT]; LEAF_EXPOSURE_MATERIAL_COUNT],
    archetypes: [MotuMesh; 2],
    pads: Vec<FoliagePad>,
    skeleton: &WindSkeleton,
) {
    let tints = [[0.92, 1.0, 0.87, 1.0], [0.82, 0.94, 0.77, 1.0]];
    let handles: [_; 2] = std::array::from_fn(|index| {
        meshes.add(bevy_mesh(&archetypes[index], Some(tints[index]), None))
    });
    for pad in pads {
        let joint = skeleton.axis_to_joint[pad.axis as usize];
        let mut transform = pad_transform(pad);
        transform.translation -= skeleton.origins[joint];
        commands.spawn((
            Mesh3d(handles[usize::from(pad.archetype)].clone()),
            MeshMaterial3d(
                materials[leaf_exposure_bin(pad.light_exposure)]
                    [leaf_pigment_bin(pad.mean_age, pad.variation)]
                .clone(),
            ),
            TransmittedShadowReceiver,
            transform,
            ChildOf(skeleton.joints[joint]),
        ));
    }
}

fn skeleton_plan(graph: &AxisGraph, seed: u64) -> SkeletonPlan {
    assert!(!graph.axes.is_empty(), "a wind skeleton requires one axis");
    let mut selected_axes: Vec<_> = graph
        .axes
        .iter()
        .enumerate()
        .filter_map(|(index, axis)| (axis.alive && axis.order <= 2).then_some(index))
        .take(MAX_WIND_JOINTS)
        .collect();
    if selected_axes.is_empty() {
        selected_axes.push(0);
    }
    if selected_axes.len() < MAX_WIND_JOINTS {
        selected_axes.extend(
            graph
                .axes
                .iter()
                .enumerate()
                .filter_map(|(index, axis)| (axis.alive && axis.order == 3).then_some(index))
                .take(MAX_WIND_JOINTS - selected_axes.len()),
        );
    }

    let mut selected_lookup = vec![None; graph.axes.len()];
    for (joint, axis) in selected_axes.iter().copied().enumerate() {
        selected_lookup[axis] = Some(joint);
    }
    let axis_to_joint = (0..graph.axes.len())
        .map(|axis| nearest_selected_ancestor(graph, &selected_lookup, axis).unwrap_or(0))
        .collect();
    let parent_joints: Vec<_> = selected_axes
        .iter()
        .copied()
        .map(|axis| {
            graph.axes[axis].parent.and_then(|parent| {
                nearest_selected_ancestor(graph, &selected_lookup, parent as usize)
            })
        })
        .collect();
    let origins: Vec<_> = selected_axes
        .iter()
        .map(|axis| convert(graph.axes[*axis].points_metres[0]))
        .collect();
    let mut phases = vec![0.0; selected_axes.len()];
    for joint in 0..selected_axes.len() {
        let inherited = parent_joints[joint].map_or(0.0, |parent| phases[parent]);
        let offset = (wind_hash(seed, selected_axes[joint]) - 0.5)
            * (0.16 + f32::from(graph.axes[selected_axes[joint]].order) * 0.05);
        phases[joint] = inherited + offset;
    }
    SkeletonPlan {
        selected_axes,
        axis_to_joint,
        parent_joints,
        origins,
        phases,
    }
}

fn nearest_selected_ancestor(
    graph: &AxisGraph,
    selected_lookup: &[Option<usize>],
    mut axis: usize,
) -> Option<usize> {
    loop {
        if let Some(joint) = selected_lookup[axis] {
            return Some(joint);
        }
        axis = graph.axes[axis].parent? as usize;
    }
}

fn spawn_wind_skeleton(
    commands: &mut Commands,
    inverse_bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    graph: &AxisGraph,
    seed: u64,
    tree_root: Entity,
) -> WindSkeleton {
    let plan = skeleton_plan(graph, seed);
    let mut joints = Vec::with_capacity(plan.selected_axes.len());
    for joint in 0..plan.selected_axes.len() {
        let axis_index = plan.selected_axes[joint];
        let axis = graph.axes[axis_index];
        let parent_origin =
            plan.parent_joints[joint].map_or(Vec3::ZERO, |parent| plan.origins[parent]);
        let local_translation = plan.origins[joint] - parent_origin;
        let axis_variation = (wind_hash(seed ^ 0x57_49_4e_44, axis_index) - 0.5) * 0.45;
        let bend_axis = Quat::from_rotation_y(axis_variation) * Vec3::Z;
        let flexibility = match axis.order {
            0 => 0.003,
            1 => 0.011,
            2 => 0.026,
            _ => 0.043,
        } * (0.82 + axis.exposure * 0.24);
        let entity = commands
            .spawn((
                Name::new(format!("Wind joint {axis_index}")),
                Transform::from_translation(local_translation),
                Visibility::default(),
                WindJoint {
                    rest_translation: local_translation,
                    bend_axis,
                    phase: plan.phases[joint],
                    flexibility,
                },
            ))
            .id();
        if let Some(parent) = plan.parent_joints[joint] {
            commands.entity(entity).insert(ChildOf(joints[parent]));
        } else {
            commands.entity(entity).insert(ChildOf(tree_root));
        }
        joints.push(entity);
    }
    let bindposes: Vec<Mat4> = plan
        .origins
        .iter()
        .map(|origin| Mat4::from_translation(-*origin))
        .collect();
    WindSkeleton {
        joints,
        inverse_bindposes: inverse_bindposes.add(SkinnedMeshInverseBindposes::from(bindposes)),
        selected_axes: plan.selected_axes,
        axis_to_joint: plan.axis_to_joint,
        origins: plan.origins,
        parent_joints: plan.parent_joints,
    }
}

fn skin_weights(source: &MotuMesh, graph: &AxisGraph, skeleton: &WindSkeleton) -> SkinWeights {
    let mut joints = Vec::with_capacity(source.vertices.len());
    let mut weights = Vec::with_capacity(source.vertices.len());
    for point in source.vertices.iter().copied() {
        let (joint, fraction) = skeleton
            .selected_axes
            .iter()
            .copied()
            .enumerate()
            .map(|(joint, axis)| {
                let (distance, fraction) = closest_axis_fraction(point, graph.axes[axis]);
                (joint, distance, fraction)
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(joint, _, fraction)| (joint, fraction))
            .expect("wind skeleton has at least one joint");
        let parent = skeleton.parent_joints[joint].unwrap_or(joint);
        let child_weight = if parent == joint {
            1.0
        } else {
            fraction * fraction * (3.0 - 2.0 * fraction)
        };
        joints.push([parent as u16, joint as u16, 0, 0]);
        weights.push([1.0 - child_weight, child_weight, 0.0, 0.0]);
    }
    SkinWeights { joints, weights }
}

fn closest_axis_fraction(point: motu::Vec3, axis: Axis) -> (f32, f32) {
    axis.points_metres
        .windows(2)
        .enumerate()
        .map(|(segment, points)| {
            let delta = points[1] - points[0];
            let local = if delta.length_squared() > f32::EPSILON {
                ((point - points[0]).dot(delta) / delta.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let distance = (point - points[0].lerp(points[1], local)).length_squared();
            let fraction = (segment as f32 + local) / (axis.points_metres.len() - 1) as f32;
            (distance, fraction)
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .expect("axes contain at least two points")
}

fn wind_hash(seed: u64, index: usize) -> f32 {
    let mut value = seed ^ (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value >> 40) as f32 / (1_u32 << 24) as f32
}

fn apply_wind(
    settings: Res<Settings>,
    mut joints: Query<(&WindJoint, &mut Transform), Without<LeafFlutter>>,
    mut leaves: Query<(&LeafFlutter, &mut Transform), Without<WindJoint>>,
) {
    let cycle = settings.wind_phase * std::f32::consts::TAU;
    let gust = (cycle.mul_add(0.43, 0.7)).sin();
    for (joint, mut transform) in &mut joints {
        let coherent = (cycle + joint.phase).sin().mul_add(
            0.78,
            (cycle.mul_add(1.83, joint.phase * 0.62)).sin() * 0.14 + gust * 0.08,
        );
        transform.translation = joint.rest_translation;
        transform.rotation = Quat::from_axis_angle(
            joint.bend_axis,
            settings.wind_strength * joint.flexibility * coherent,
        );
    }
    for (leaf, mut transform) in &mut leaves {
        let flutter = (cycle.mul_add(5.2, leaf.phase)).sin();
        let twist = (cycle.mul_add(3.7, leaf.phase * 1.31)).sin();
        transform.rotation = leaf.rest_rotation
            * Quat::from_rotation_x(settings.wind_strength * leaf.amplitude * flutter)
            * Quat::from_rotation_z(settings.wind_strength * leaf.amplitude * 0.38 * twist);
    }
}

fn spawn_stage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    commands.spawn((
        Name::new("Review ground"),
        ReviewGround,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(120.0, 120.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.21, 0.18, 0.12),
            perceptual_roughness: 0.98,
            reflectance: 0.015,
            ..default()
        })),
    ));
}

fn spawn_lighting(
    commands: &mut Commands,
    settings: &Settings,
    mediums: &mut Assets<ScatteringMedium>,
) {
    commands.spawn((
        Name::new("Atmosphere"),
        Atmosphere {
            ground_albedo: Vec3::splat(0.10),
            ..Atmosphere::earth(mediums.add(ScatteringMedium::earth(128, 128)))
        },
    ));
    commands.spawn((
        Name::new("Sun"),
        ReviewSun,
        DirectionalLight {
            color: Color::WHITE,
            illuminance: 92_000.0,
            shadow_maps_enabled: true,
            contact_shadows_enabled: true,
            ..default()
        },
        Transform::default().looking_to(settings.light.direction(), Vec3::Y),
    ));
}

fn capture(
    mut commands: Commands,
    target: Res<CaptureTarget>,
    mut progress: ResMut<CaptureProgress>,
    mut exit: MessageWriter<AppExit>,
) {
    if progress.requested {
        progress.frames_since_request += 1;
        let written = fs::metadata(&target.path).is_ok_and(|file| file.len() > 0);
        if written && progress.frames_since_request > FLUSH_FRAMES {
            info!("wrote {}", target.path.display());
            exit.write(AppExit::Success);
        } else if progress.frames_since_request > CAPTURE_TIMEOUT_FRAMES {
            error!("screenshot was never written to {}", target.path.display());
            exit.write(AppExit::error());
        }
        return;
    }
    progress.frames += 1;
    if progress.frames < SETTLE_FRAMES {
        return;
    }
    commands
        .spawn(Screenshot::image(target.image.clone()))
        .observe(save_to_disk(target.path.clone()));
    progress.requested = true;
}

fn texture_image(texture: BotanicalTexture, repeat: bool, srgb: bool) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        texture.rgba,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        },
        address_mode_v: if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        },
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

fn bevy_mesh(source: &MotuMesh, tint: Option<[f32; 4]>, skin: Option<&SkinWeights>) -> Mesh {
    let positions: Vec<[f32; 3]> = source
        .vertices
        .iter()
        .map(|vertex| [vertex.x, vertex.z, vertex.y])
        .collect();
    let normals: Vec<[f32; 3]> = source
        .normals
        .iter()
        .map(|normal| [normal.x, normal.z, normal.y])
        .collect();
    let uv: Vec<[f32; 2]> = source.uv.iter().map(|uv| [uv.x, uv.y]).collect();
    let indices = source
        .triangles
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|triangle| [triangle[0], triangle[2], triangle[1]])
        .collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    if let Some(tint) = tint {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![tint; source.vertices.len()]);
    }
    mesh.insert_indices(Indices::U32(indices));
    mesh.generate_tangents()
        .expect("tree laboratory meshes have valid positions, normals, and UVs");
    if let Some(skin) = skin {
        assert_eq!(skin.joints.len(), source.vertices.len());
        assert_eq!(skin.weights.len(), source.vertices.len());
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(skin.joints.clone()),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, skin.weights.clone());
    }
    mesh
}

fn bevy_wood_mesh(source: &MotuMesh, bark: &[BarkVertex], skin: Option<&SkinWeights>) -> Mesh {
    assert_eq!(source.vertices.len(), bark.len());
    let mut mesh = bevy_mesh(source, None, skin);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        bark.iter()
            .copied()
            .map(bark_vertex_colour)
            .collect::<Vec<_>>(),
    );
    mesh
}

fn bark_vertex_colour(vertex: BarkVertex) -> [f32; 4] {
    let maturity = smoothstep(vertex.maturity);
    let young = Vec3::new(1.03, 1.04, 1.01);
    let mature = Vec3::new(0.99, 0.98, 0.95);
    let colour = young.lerp(mature, maturity);
    [colour.x, colour.y, colour.z, vertex.maturity]
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn leaf_transform(leaf: LeafOrgan) -> Transform {
    let direction = convert(leaf.direction).normalize_or(Vec3::X);
    let normal = convert(leaf.normal).normalize_or(Vec3::Y);
    let transverse = normal.cross(direction).normalize_or(Vec3::Z);
    Transform {
        translation: convert(leaf.blade_base_metres),
        rotation: Quat::from_mat3(&Mat3::from_cols(direction, transverse, normal)),
        scale: Vec3::new(leaf.length_metres, leaf.width_metres, leaf.length_metres),
    }
}

fn shoot_tip_transform(tip: ShootTipOrgan) -> Transform {
    let direction = convert(tip.direction).normalize_or(Vec3::X);
    let reference = if direction.dot(Vec3::Y).abs() < 0.88 {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let transverse = direction.cross(reference).normalize_or(Vec3::Z);
    let normal = direction.cross(transverse).normalize_or(Vec3::Y);
    Transform {
        translation: convert(tip.base_metres),
        rotation: Quat::from_mat3(&Mat3::from_cols(direction, transverse, normal))
            * Quat::from_rotation_x(tip.variation),
        scale: Vec3::new(tip.length_metres, tip.radius_metres, tip.radius_metres),
    }
}

fn pad_transform(pad: FoliagePad) -> Transform {
    let direction = convert(pad.direction).normalize_or(Vec3::X);
    let normal = convert(pad.normal).normalize_or(Vec3::Y);
    let transverse = direction.cross(normal).normalize_or(Vec3::Z);
    let extents = Vec3::new(
        pad.half_extents_metres.x,
        pad.half_extents_metres.y,
        pad.half_extents_metres.z,
    );
    Transform {
        translation: convert(pad.centre_metres),
        rotation: Quat::from_mat3(&Mat3::from_cols(direction, normal, transverse)),
        scale: Vec3::new(
            extents.x.max(0.35),
            extents.y.max(0.24),
            extents.z.max(0.30),
        ),
    }
}

fn convert(vector: motu::Vec3) -> Vec3 {
    Vec3::new(vector.x, vector.z, vector.y)
}

fn parse(arguments: impl Iterator<Item = String>) -> Result<Option<Settings>, String> {
    let mut arguments = arguments.peekable();
    let mut seed = 42_u64;
    let mut lod = ReviewLod::default();
    let mut view = ReviewView::default();
    let mut light = ReviewLight::default();
    let mut foliage = true;
    let mut fine_shoots = true;
    let mut wind_phase = 0.0_f32;
    let mut wind_strength = 0.0_f32;
    let mut screenshot = None;
    let mut capture_ui = false;
    while let Some(argument) = arguments.next() {
        let value = |arguments: &mut std::iter::Peekable<_>| {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} requires a value"))
        };
        match argument.as_str() {
            "--seed" => {
                seed = value(&mut arguments)?
                    .parse()
                    .map_err(|_| "--seed must be an unsigned integer".to_owned())?;
            }
            "--lod" => lod = ReviewLod::parse(&value(&mut arguments)?)?,
            "--view" => view = ReviewView::parse(&value(&mut arguments)?)?,
            "--light" => light = ReviewLight::parse(&value(&mut arguments)?)?,
            "--wind-phase" => {
                wind_phase = value(&mut arguments)?
                    .parse()
                    .map_err(|_| "--wind-phase must be a number from 0 to 1".to_owned())?;
            }
            "--wind-strength" => {
                wind_strength = value(&mut arguments)?
                    .parse()
                    .map_err(|_| "--wind-strength must be a number from 0 to 1".to_owned())?;
            }
            "--screenshot" => screenshot = Some(PathBuf::from(value(&mut arguments)?)),
            "--capture-ui" => capture_ui = true,
            "--wood-only" => foliage = false,
            "--scaffold-only" => {
                foliage = false;
                fine_shoots = false;
            }
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            _ => return Err(format!("unknown option {argument:?}; use --help")),
        }
    }
    if !wind_phase.is_finite() || !(0.0..=1.0).contains(&wind_phase) {
        return Err("--wind-phase must be a number from 0 to 1".to_owned());
    }
    if !wind_strength.is_finite() || !(0.0..=1.0).contains(&wind_strength) {
        return Err("--wind-strength must be a number from 0 to 1".to_owned());
    }
    if capture_ui && screenshot.is_none() {
        return Err("--capture-ui requires --screenshot <PATH>".to_owned());
    }
    Ok(Some(Settings {
        seed,
        recipe: BotanicalRecipe::default(),
        lod,
        view,
        light,
        foliage,
        fine_shoots,
        wind_phase,
        wind_strength,
        screenshot,
        capture_ui,
    }))
}

fn print_help() {
    println!(
        "tree-lab [OPTIONS]\n\n\
         With no screenshot path, opens the interactive Tree Studio.\n\n\
         --seed <N>             deterministic prototype seed [42]\n\
         --lod <near|middle|far>\n\
                                tree representation [near]\n\
         --view <NAME>          whole, whole-quarter, crown, detail, leaf, tip, root, scar, epicormic, or junction [whole]\n\
         --light <front|back|grazing>\n\
                                review-light direction [front]\n\
         --wind-phase <0..1>    deterministic point in the wind cycle [0]\n\
         --wind-strength <0..1> branch motion and leaf flutter amount [0]\n\
         --wood-only            omit foliage\n\
         --scaffold-only        omit foliage and fine shoots\n\
         --screenshot <PATH>    render one headless PNG and exit\n\
         --capture-ui           include the Tree Studio HUD in that PNG"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings<'a>(values: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        values.iter().map(|value| (*value).to_owned())
    }

    #[test]
    fn parses_headless_review_command() {
        let settings = parse(strings(&[
            "--seed",
            "666",
            "--lod",
            "middle",
            "--view",
            "leaf",
            "--light",
            "back",
            "--wind-phase",
            "0.25",
            "--wind-strength",
            "0.7",
            "--screenshot",
            "tree.png",
        ]))
        .expect("valid command")
        .expect("settings");
        assert_eq!(settings.seed, 666);
        assert_eq!(settings.lod, ReviewLod::Middle);
        assert_eq!(settings.view, ReviewView::Leaf);
        assert_eq!(settings.light, ReviewLight::Back);
        assert!(settings.foliage);
        assert!(settings.fine_shoots);
        assert_eq!(settings.wind_phase.to_bits(), 0.25_f32.to_bits());
        assert_eq!(settings.wind_strength.to_bits(), 0.7_f32.to_bits());
        assert_eq!(settings.screenshot, Some(PathBuf::from("tree.png")));
        assert!(!settings.capture_ui);
    }

    #[test]
    fn parses_generated_far_impostor_lod() {
        let settings = parse(strings(&["--lod", "far"]))
            .expect("valid command")
            .expect("settings");
        assert_eq!(settings.lod, ReviewLod::Far);
    }

    #[test]
    fn parses_grazing_review_light() {
        assert_eq!(
            ReviewLight::parse("grazing").expect("valid review light"),
            ReviewLight::Grazing
        );
    }

    #[test]
    fn parses_whole_tree_quarter_view() {
        assert_eq!(
            ReviewView::parse("whole-quarter").expect("valid review view"),
            ReviewView::WholeQuarter
        );
    }

    #[test]
    fn parses_epicormic_review_view() {
        assert_eq!(
            ReviewView::parse("epicormic").expect("valid review view"),
            ReviewView::Epicormic
        );
    }

    #[test]
    fn parses_junction_review_view() {
        assert_eq!(
            ReviewView::parse("junction").expect("valid review view"),
            ReviewView::Junction
        );
    }

    #[test]
    fn scaffold_capture_omits_foliage_and_fine_shoots() {
        let settings = parse(strings(&[
            "--scaffold-only",
            "--screenshot",
            "scaffold.png",
        ]))
        .expect("valid command")
        .expect("settings");
        assert!(!settings.foliage);
        assert!(!settings.fine_shoots);
        assert_eq!(settings.wind_strength.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn leaf_transform_keeps_width_in_the_blade_plane() {
        let leaf = LeafOrgan {
            axis: 0,
            blade_base_metres: motu::Vec3::new(0.0, 0.0, 0.0),
            direction: motu::Vec3::new(1.0, 0.0, 0.0),
            normal: motu::Vec3::new(0.0, 0.0, 1.0),
            length_metres: 0.24,
            width_metres: 0.08,
            archetype: 0,
            age: 0.5,
            light_exposure: 0.5,
            variation: 0.0,
        };
        let transform = leaf_transform(leaf);
        assert_eq!(transform.scale, Vec3::new(0.24, 0.08, 0.24));
        assert!(
            (transform.rotation * Vec3::Z)
                .normalize()
                .dot(convert(leaf.normal))
                > 0.999
        );
        assert!(
            (transform.rotation * Vec3::Y)
                .dot(convert(leaf.normal))
                .abs()
                < 1.0e-5
        );
    }

    #[test]
    fn shoot_tip_transform_aligns_the_shared_archetype_to_growth() {
        let tip = ShootTipOrgan {
            axis: 0,
            base_metres: motu::Vec3::new(1.0, 2.0, 3.0),
            direction: motu::Vec3::new(0.0, 1.0, 0.0),
            length_metres: 0.04,
            radius_metres: 0.01,
            state: ShootTipState::ActiveBud,
            variation: 0.7,
        };
        let transform = shoot_tip_transform(tip);
        assert_eq!(transform.translation, convert(tip.base_metres));
        assert_eq!(transform.scale, Vec3::new(0.04, 0.01, 0.01));
        assert!(
            (transform.rotation * Vec3::X)
                .normalize()
                .dot(convert(tip.direction))
                > 0.999
        );
        assert_eq!(ReviewView::parse("tip"), Ok(ReviewView::Tip));
    }

    #[test]
    fn leaf_exposure_uses_three_bounded_shared_material_bins() {
        assert_eq!(leaf_exposure_bin(0.0), 0);
        assert_eq!(leaf_exposure_bin(0.379), 0);
        assert_eq!(leaf_exposure_bin(0.38), 1);
        assert_eq!(leaf_exposure_bin(0.68), 1);
        assert_eq!(leaf_exposure_bin(0.681), 2);
        assert_eq!(leaf_exposure_bin(1.0), 2);
    }

    #[test]
    fn upper_cuticle_response_is_bounded_and_strengthens_with_exposure() {
        let profiles: [LeafOptics; LEAF_EXPOSURE_MATERIAL_COUNT] = std::array::from_fn(leaf_optics);
        assert!(
            profiles
                .windows(2)
                .all(|pair| pair[0].clearcoat < pair[1].clearcoat)
        );
        assert!(
            profiles
                .windows(2)
                .all(|pair| pair[0].clearcoat_roughness > pair[1].clearcoat_roughness)
        );
        assert!(profiles.iter().all(|profile| {
            (0.20..=0.42).contains(&profile.clearcoat)
                && (0.42..=0.58).contains(&profile.clearcoat_roughness)
        }));
        assert_eq!(leaf_optics(usize::MAX), profiles[1]);
    }

    #[test]
    fn leaf_pigment_variation_is_bounded_and_age_gates_warm_tissue() {
        let tau = std::f32::consts::TAU;
        assert_eq!(leaf_pigment_bin(0.0, 0.0), 0);
        assert_eq!(leaf_pigment_bin(1.0, tau * 0.259), 0);
        assert_eq!(leaf_pigment_bin(0.2, tau * 0.90), 1);
        assert_eq!(leaf_pigment_bin(0.42, tau * 0.90), 1);
        assert_eq!(leaf_pigment_bin(0.421, tau * 0.741), 2);
        assert_eq!(leaf_pigment_bin(1.0, tau * 0.999), 2);
        assert_eq!(leaf_pigment_bin(1.0, tau), 0);
    }

    #[test]
    fn bark_maturity_stays_neutral_and_bounded() {
        let young = bark_vertex_colour(BarkVertex {
            radius_metres: 0.005,
            maturity: 0.0,
        });
        let mature = bark_vertex_colour(BarkVertex {
            radius_metres: 0.58,
            maturity: 1.0,
        });
        assert!(young[1] > young[0]);
        assert!(mature[0] > mature[1]);
        assert!(mature[1] > mature[2]);
        assert!(young[..3].iter().sum::<f32>() > mature[..3].iter().sum::<f32>());
        assert!(young[..3].iter().all(|value| (0.75..=1.15).contains(value)));
        assert!(
            mature[..3]
                .iter()
                .all(|value| (0.75..=1.15).contains(value))
        );
        assert_eq!(young[3].to_bits(), 0.0_f32.to_bits());
        assert_eq!(mature[3].to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn rejects_wind_controls_outside_the_normalized_range() {
        let error = parse(strings(&[
            "--wind-strength",
            "1.1",
            "--screenshot",
            "tree.png",
        ]))
        .expect_err("unbounded wind must fail");
        assert!(error.contains("0 to 1"));
    }

    #[test]
    fn skeleton_is_bounded_and_preserves_ancestor_order() {
        let prototype = generate_botanical_prototype(42, BotanicalRecipe::default())
            .expect("botanical prototype");
        let plan = skeleton_plan(&prototype.graph, 42);
        assert!(!plan.selected_axes.is_empty());
        assert!(plan.selected_axes.len() <= MAX_WIND_JOINTS);
        assert_eq!(plan.axis_to_joint.len(), prototype.graph.axes.len());
        assert!(
            plan.axis_to_joint
                .iter()
                .all(|joint| *joint < plan.selected_axes.len())
        );
        assert!(
            plan.parent_joints
                .iter()
                .enumerate()
                .all(|(joint, parent)| parent.is_none_or(|parent| parent < joint))
        );
        assert!(plan.phases.iter().all(|phase| phase.is_finite()));
    }

    #[test]
    fn closest_axis_sample_reports_base_and_tip() {
        let prototype = generate_botanical_prototype(42, BotanicalRecipe::default())
            .expect("botanical prototype");
        let axis = prototype.graph.axes[0];
        let (_, base) = closest_axis_fraction(axis.points_metres[0], axis);
        let (_, tip) = closest_axis_fraction(axis.points_metres[4], axis);
        assert!(base <= f32::EPSILON);
        assert!((tip - 1.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn defaults_to_an_interactive_run_without_a_screenshot() {
        let settings = parse(strings(&["--seed", "42"]))
            .expect("interactive command")
            .expect("settings");
        assert_eq!(settings.seed, 42);
        assert_eq!(settings.recipe, BotanicalRecipe::default());
        assert_eq!(settings.screenshot, None);
        assert!(!settings.capture_ui);
    }

    #[test]
    fn ui_capture_requires_and_preserves_a_screenshot_target() {
        let error = parse(strings(&["--capture-ui"]))
            .expect_err("a UI capture without an image target must fail");
        assert!(error.contains("requires --screenshot"));

        let settings = parse(strings(&[
            "--screenshot",
            "tree-studio.png",
            "--capture-ui",
        ]))
        .expect("UI capture command")
        .expect("settings");
        assert!(settings.capture_ui);
        assert_eq!(settings.screenshot, Some(PathBuf::from("tree-studio.png")));
    }
}
