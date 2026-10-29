//! A finished spherical mesh with orbit controls; no generation animation.
#![allow(clippy::needless_pass_by_value)]

mod capture;
mod chunk_cache;
mod chunk_policy;
mod collision;
mod command;
mod details;
mod environment;
mod explore;
mod forest;
mod lighting;
mod lod_material;
mod ocean;
mod panel;
mod solar;
mod streaming;
mod surface;
mod terrain_material;
mod water;
mod weather;

use std::{
    fs::File,
    io::{BufWriter, Write},
    process,
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{
    app::{PluginGroupBuilder, ScheduleRunnerPlugin},
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    pbr::wireframe::{Wireframe, WireframeConfig, WireframePlugin},
    prelude::*,
    window::{ExitCondition, WindowResolution},
    winit::WinitPlugin,
};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext, input::EguiWantsInput};
use command::{BedView, Command, GenerationMode, RiverDisplay, View};
use planet_rs::PlanetTerrain;

#[derive(Resource, Clone)]
struct Scene {
    terrain: Arc<PlanetTerrain>,
    seed: u64,
    generation: GenerationMode,
    bed: BedView,
    river_display: RiverDisplay,
    no_ocean: bool,
    wireframe: bool,
    smooth: bool,
}

#[derive(Component)]
struct TerrainChunk;

#[derive(Component, Clone, Copy)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
    focus: Vec3,
    frame: Quat,
    minimum: f32,
    maximum: f32,
}

#[derive(Resource)]
struct InitialOrbit(Orbit);

#[derive(bevy::ecs::system::SystemParam)]
struct ControlsInput<'w> {
    buttons: Res<'w, ButtonInput<MouseButton>>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    motion: Res<'w, AccumulatedMouseMotion>,
    scroll: Res<'w, AccumulatedMouseScroll>,
    initial_view: Res<'w, InitialOrbit>,
    wants: Res<'w, EguiWantsInput>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("planet-bevy: {error}");
        process::exit(1);
    }
}

#[allow(clippy::too_many_lines)] // Viewer plugin and system registration.
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let Some(command) = Command::parse(std::env::args().skip(1))? else {
        println!("{}", command::HELP);
        return Ok(());
    };
    let started = Instant::now();
    let configuration = panel::Settings::from(&command);
    let chunk_settings = command.streaming;
    let terrain = configuration.generate()?;
    let mesh = &terrain.topology;
    let initial_orbit = environment_orbit(command.view, &terrain, &command.environment)
        .unwrap_or_else(|| initial_orbit(command.view, &terrain));
    println!(
        "Validated seed {}: {} vertices, {} edges, {} triangles; radius {}. Generation + validation: {:.2?}",
        command.options.seed,
        mesh.directions.len(),
        3 * mesh.directions.len() - 6,
        mesh.triangles.len(),
        mesh.radius,
        started.elapsed()
    );
    if let Some(path) = &command.export {
        export_obj(path, &terrain, command.options.seed)?;
        println!("Exported {}", path.display());
    }
    print_terrain_stats(&terrain);
    if command.validate_only {
        return Ok(());
    }
    // Rendering uses a unit sphere, so orbit controls and depth precision work
    // identically for tiny and astronomical physical radii. Export retains units.
    let capturing = command.screenshot.is_some();
    let mut app = App::new();
    app.add_plugins((
        viewer_plugins(capturing),
        EguiPlugin::default(),
        lod_material::LodMaterialPlugin,
        details::DetailPlugin,
        forest::ForestPlugin,
        ocean::OceanPlugin,
        water::WaterPlugin,
        lighting::LightingPlugin,
        environment::EnvironmentPlugin,
        weather::WeatherPlugin,
        solar::SolarPlugin,
        WireframePlugin::default(),
    ))
    .insert_resource(WireframeConfig {
        default_color: Color::srgb(0.07, 0.14, 0.19),
        ..default()
    })
    .insert_resource(bevy_egui::EguiGlobalSettings {
        auto_create_primary_context: false,
        ..default()
    })
    .insert_resource(panel::Panel::new(
        configuration,
        command.panel == command::PanelVisibility::Hidden,
    ))
    .init_resource::<panel::Refresh>()
    .init_resource::<streaming::ChunkWorld>()
    .init_resource::<streaming::StreamingStats>()
    .add_systems(Startup, terrain_material::setup)
    .insert_resource(chunk_settings)
    .insert_resource(command.lighting)
    .insert_resource(command.ocean)
    .insert_resource(command.water)
    .insert_resource(command.environment)
    .insert_resource(command.weather)
    .insert_resource(command.solar)
    .insert_resource(command.forest)
    .insert_resource(forest::Forests::with_inspection(
        command.view == View::Forest,
    ))
    .init_resource::<explore::Explorer>()
    .insert_resource(ClearColor(Color::srgb(0.018, 0.025, 0.045)))
    .insert_resource(Scene {
        terrain: Arc::new(terrain),
        seed: command.options.seed,
        generation: command.generation,
        bed: command.bed,
        river_display: command.rivers,
        no_ocean: command.no_ocean,
        wireframe: command.wireframe,
        smooth: command.generation == GenerationMode::Terrain,
    })
    .insert_resource(InitialOrbit(initial_orbit))
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (
            panel::update_generation,
            controls,
            refresh_scene,
            streaming::prepare_update,
            explore::update,
            streaming::update,
            forest::update,
            details::update,
            ocean::update,
            water::update,
            lighting::update,
            update_edges,
        )
            .chain(),
    )
    .add_systems(
        EguiPrimaryContextPass,
        (panel::draw, panel::apply_view).chain(),
    );
    if let Some(path) = command.screenshot {
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(5)))
            .add_plugins(capture::CapturePlugin { path });
    }
    let result = app.run();
    if result.is_success() {
        Ok(())
    } else {
        Err("viewer failed; see render log".into())
    }
}

fn viewer_plugins(capturing: bool) -> PluginGroupBuilder {
    let mut plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: (!capturing).then(|| Window {
            title: String::from("Planet experiment"),
            resolution: WindowResolution::new(1280, 800),
            ..default()
        }),
        exit_condition: if capturing {
            ExitCondition::DontExit
        } else {
            ExitCondition::OnAllClosed
        },
        ..default()
    });
    if capturing {
        plugins = plugins.disable::<WinitPlugin>();
    }
    plugins
}

fn print_terrain_stats(terrain: &PlanetTerrain) {
    let stats = terrain.stats();
    println!(
        "Terrain: {} land / {} submerged sites, {} river segments. Relative heights: {:.5}..{:.5}. Sea radius: {}",
        stats.land_vertices,
        stats.submerged_vertices,
        stats.river_segments,
        stats.minimum_elevation,
        stats.maximum_elevation,
        terrain.topology.radius
    );
    println!(
        "Refinement vertices: {:?}; carved sites: {}; max cut: {:.6} R; water triangles: {}",
        terrain.refinement_vertices,
        stats.carved_vertices,
        stats.maximum_channel_depth,
        stats.water_triangles
    );
    println!("Ocean: {} connected component(s)", stats.sea_components);
    println!("Waterfalls: {}", terrain.waterfalls.len());
    for pass in &terrain.ocean_passes {
        println!(
            "Ocean {:?}: {} -> {} seas, {} channel(s), {} sites",
            pass.stage, pass.seas_before, pass.seas_after, pass.channels, pass.vertices
        );
    }
}

fn export_obj(path: &std::path::Path, terrain: &PlanetTerrain, seed: u64) -> std::io::Result<()> {
    let mesh = &terrain.topology;
    let mut file = BufWriter::new(File::create(path)?);
    writeln!(
        file,
        "# Planet experiment; seed {seed}; sea radius {}",
        mesh.radius
    )?;
    for vertex in 0..mesh.directions.len() {
        let point = terrain.position(vertex);
        writeln!(file, "v {:.17} {:.17} {:.17}", point.x, point.y, point.z)?;
    }
    for [a, b, c] in &mesh.triangles {
        writeln!(file, "f {} {} {}", a + 1, b + 1, c + 1)?;
    }
    file.flush()
}

fn setup(
    mut commands: Commands,
    target: Option<Res<capture::CaptureTarget>>,
    initial_view: Res<InitialOrbit>,
) {
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            shadow_depth_bias: 0.000_02,
            shadow_normal_bias: 0.4,
            ..default()
        },
        lighting::Sun,
        bevy::light::CascadeShadowConfig::default(),
        Transform::from_xyz(-3.0, 4.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let orbit = initial_view.0;
    let mut camera = commands.spawn((
        Camera3d {
            depth_texture_usages: (bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT
                | bevy::render::render_resource::TextureUsages::TEXTURE_BINDING)
                .into(),
            ..default()
        },
        Msaa::Off,
        bevy::camera::Hdr,
        bevy::core_pipeline::prepass::DepthPrepass,
        bevy::anti_alias::fxaa::Fxaa::default(),
        bevy::pbr::ScreenSpaceTransmission::default(),
        bevy::light::ShadowFilteringMethod::Gaussian,
        lighting::AoSettings::new(&lighting::LightingSettings::default(), 1000.0),
        Projection::Perspective(PerspectiveProjection {
            near: 0.00005,
            far: 50.0,
            ..default()
        }),
        bevy::ui::IsDefaultUiCamera,
        orbit_transform(&orbit),
        orbit,
    ));
    if let Some(target) = &target {
        camera.insert(target.render_target());
    }
    let mut ui_camera = commands.spawn((
        Camera2d,
        Msaa::Off,
        bevy::camera::visibility::RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: bevy::camera::CameraOutputMode::Write {
                blend_state: Some(bevy::render::render_resource::BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        PrimaryEguiContext,
    ));
    if let Some(target) = target {
        ui_camera.insert(target.render_target());
    }
}

#[allow(clippy::cast_possible_truncation)] // Rendering uses normalized f32 positions.
fn initial_orbit(view: View, terrain: &PlanetTerrain) -> Orbit {
    let mut orbit = Orbit {
        yaw: 0.45
            + if view == View::Back {
                std::f32::consts::PI
            } else {
                0.0
            },
        pitch: 0.25,
        distance: 3.5,
        focus: Vec3::ZERO,
        frame: Quat::IDENTITY,
        minimum: 1.15,
        maximum: 12.0,
    };
    if matches!(view, View::Waterfall | View::Mouth)
        && let Some(fall) = terrain
            .waterfalls
            .iter()
            .filter(|fall| view != View::Mouth || fall.lower_surface <= 0.0)
            .max_by(|a, b| {
                let score = |fall: &planet_rs::Waterfall| {
                    (fall.upper_surface - fall.lower_surface)
                        * (1.2
                            + fall
                                .direction
                                .dot(planet_rs::DVec3::new(-3.0, 4.0, 5.0).normalize()))
                };
                score(a).total_cmp(&score(b))
            })
    {
        orbit.focus =
            surface::vector(fall.lip * (1.0 + (fall.upper_surface + fall.lower_surface) * 0.5));
        orbit.frame = Quat::from_rotation_arc(Vec3::Y, surface::vector(fall.lip));
        let direction = orbit.frame.inverse() * surface::vector(fall.direction);
        orbit.yaw = direction.x.atan2(direction.z);
        orbit.pitch = 0.3;
        orbit.distance =
            (surface::vector(fall.lip * fall.half_width).length() * 6.0).clamp(0.015, 0.08);
        orbit.minimum = 0.002;
        orbit.maximum = 0.5;
    }
    if view == View::River {
        let candidate = terrain
            .rivers
            .iter()
            .filter(|river| terrain.elevations[river.from] > 0.008)
            .max_by(|a, b| {
                let score = |river: &planet_rs::RiverSegment| {
                    river.catchment
                        * (1.2
                            + terrain.topology.directions[river.from]
                                .dot(planet_rs::DVec3::new(-3.0, 4.0, 5.0).normalize()))
                };
                score(a).total_cmp(&score(b))
            });
        if let Some(river) = candidate {
            orbit.focus = surface::vector(terrain.normalized_position(river.from));
            orbit.frame = Quat::from_rotation_arc(
                Vec3::Y,
                surface::vector(terrain.topology.directions[river.from]),
            );
            orbit.distance = 0.075;
            orbit.pitch = 0.85;
            orbit.minimum = 0.01;
            orbit.maximum = 0.5;
        }
    }
    if view == View::Grass {
        let light = planet_rs::DVec3::new(-3.0, 4.0, 5.0).normalize();
        let candidate = (0..terrain.elevations.len())
            .filter(|&i| {
                let up = terrain.topology.directions[i];
                terrain.elevations[i] * terrain.topology.radius > 2.0
                    && terrain.elevations[i] * terrain.topology.radius < 75.0
                    && terrain.normals[i].dot(up) > 0.97
                    && terrain.channel_depths[i] < 0.02 / terrain.topology.radius
            })
            .max_by(|&a, &b| {
                let score = |i: usize| {
                    terrain.soil_depth[i].min(0.006)
                        * (1.2 + terrain.topology.directions[i].dot(light))
                };
                score(a).total_cmp(&score(b))
            });
        if let Some(i) = candidate {
            let up = surface::vector(terrain.topology.directions[i]);
            orbit.focus = surface::vector(terrain.normalized_position(i))
                + up * (0.07 / terrain.topology.radius as f32);
            orbit.frame = Quat::from_rotation_arc(Vec3::Y, up);
            let local_light = orbit.frame.inverse() * surface::vector(light);
            orbit.yaw = local_light.x.atan2(local_light.z);
            orbit.pitch = 0.28;
            orbit.distance = 5.0 / terrain.topology.radius as f32;
            orbit.minimum = 0.5 / terrain.topology.radius as f32;
            orbit.maximum = 0.5;
        }
    }
    environment_orbit(view, terrain, &environment::EnvironmentSettings::default())
        .or_else(|| ocean::inspection_orbit(view, terrain))
        .unwrap_or(orbit)
}

#[allow(clippy::cast_possible_truncation)] // Physical metre heights become normalized GPU positions.
fn environment_orbit(
    view: View,
    terrain: &PlanetTerrain,
    settings: &environment::EnvironmentSettings,
) -> Option<Orbit> {
    if !matches!(view, View::Sky | View::Clouds) {
        return None;
    }
    let land = initial_orbit(View::Grass, terrain);
    let up = land.focus.normalize_or_zero();
    let up = if up.length_squared() > 0.5 {
        up
    } else {
        Vec3::Y
    };
    let radius = terrain.topology.radius as f32;
    let highest = terrain.elevations.iter().copied().fold(0.0, f64::max) as f32 * radius;
    let (base, _) = environment::settings::effective_layers(settings, highest);
    let altitude = if view == View::Clouds {
        base + settings.cloud_thickness * 0.45
    } else {
        (land.focus.length() - 1.0) * radius + 1.7
    };
    let eye = up * (1.0 + altitude / radius);
    let sun = settings.sun_direction();
    let tangent = (sun - up * sun.dot(up))
        .try_normalize()
        .unwrap_or_else(|| up.any_orthonormal_vector());
    let forward = (tangent + up * 0.22).normalize();
    let frame = Quat::from_rotation_arc(Vec3::Y, up);
    let local_offset = frame.inverse() * -forward;
    let orbit = Orbit {
        focus: eye + forward * 0.04,
        frame,
        distance: 0.04,
        pitch: local_offset.y.asin(),
        yaw: local_offset.x.atan2(local_offset.z),
        minimum: 0.001,
        maximum: 0.5,
    };
    Some(orbit)
}

fn orbit_transform(orbit: &Orbit) -> Transform {
    Transform::from_translation(
        orbit.focus
            + orbit.frame
                * Vec3::new(
                    orbit.pitch.cos() * orbit.yaw.sin(),
                    orbit.pitch.sin(),
                    orbit.pitch.cos() * orbit.yaw.cos(),
                )
                * orbit.distance,
    )
    .looking_at(orbit.focus, orbit.frame * Vec3::Y)
}

fn controls(
    input: ControlsInput,
    mut scene: ResMut<Scene>,
    mut cameras: Query<(&mut Orbit, &mut Transform)>,
    mut refresh: ResMut<panel::Refresh>,
    mut exit: MessageWriter<AppExit>,
    explorer: Res<explore::Explorer>,
) {
    let keyboard = !input.wants.wants_any_keyboard_input();
    let pointer = !input.wants.wants_any_pointer_input();
    if keyboard && !explorer.walking() && input.keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    if keyboard && !explorer.walking() && input.keys.just_pressed(KeyCode::KeyW) {
        scene.wireframe = !scene.wireframe;
    }
    if keyboard && !explorer.walking() && input.keys.just_pressed(KeyCode::KeyS) {
        scene.smooth = !scene.smooth;
        refresh.terrain = true;
    }
    if keyboard && input.keys.just_pressed(KeyCode::KeyB) {
        scene.bed = if scene.bed == BedView::Carved {
            BedView::Original
        } else {
            BedView::Carved
        };
        refresh.terrain = true;
    }
    if keyboard && input.keys.just_pressed(KeyCode::KeyO) {
        scene.no_ocean = !scene.no_ocean;
    }
    if keyboard && input.keys.just_pressed(KeyCode::KeyR) {
        scene.river_display = if scene.river_display == RiverDisplay::Show {
            RiverDisplay::Hide
        } else {
            RiverDisplay::Show
        };
    }
    for (mut orbit, mut transform) in &mut cameras {
        if explorer.walking() {
            continue;
        }
        if keyboard && input.keys.just_pressed(KeyCode::Home) {
            *orbit = input.initial_view.0;
        }
        if pointer
            && (input.buttons.pressed(MouseButton::Left)
                || input.buttons.pressed(MouseButton::Right))
            && !input.buttons.just_pressed(MouseButton::Left)
            && !input.buttons.just_pressed(MouseButton::Right)
        {
            orbit.yaw -= input.motion.delta.x * 0.005;
            orbit.pitch = (orbit.pitch + input.motion.delta.y * 0.005).clamp(-1.5, 1.5);
        }
        if pointer {
            let sensitivity = if input.scroll.unit == MouseScrollUnit::Line {
                0.12
            } else {
                0.002
            };
            orbit.distance = (orbit.distance * (-input.scroll.delta.y * sensitivity).exp())
                .clamp(orbit.minimum, orbit.maximum);
        }
        *transform = orbit_transform(&orbit);
    }
}

fn refresh_scene(mut refresh: ResMut<panel::Refresh>, mut chunks: ResMut<streaming::ChunkWorld>) {
    if refresh.terrain || refresh.water {
        chunks.rebuild = true;
        refresh.terrain = false;
        refresh.water = false;
    }
}

fn update_edges(
    mut commands: Commands,
    scene: Res<Scene>,
    terrain: Query<(Entity, Has<Wireframe>), With<TerrainChunk>>,
) {
    // Render the same entity/mesh as the terrain, so its selected LOD, loading
    // fallback and visibility automatically apply to the edge pass as well.
    for (entity, enabled) in &terrain {
        if scene.wireframe && !enabled {
            commands.entity(entity).insert(Wireframe);
        } else if !scene.wireframe && enabled {
            commands.entity(entity).remove::<Wireframe>();
        }
    }
}
