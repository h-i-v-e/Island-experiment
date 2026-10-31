//! Editable settings are separate from the displayed planet. A worker owns a
//! copied configuration and returns owned terrain; failed jobs preserve it.
use super::{
    InitialOrbit, Orbit, Scene,
    command::{BedView, Command, GenerationMode, RiverDisplay, View},
    initial_orbit, orbit_transform,
};
use bevy::{
    camera::Viewport,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use bevy_egui::{EguiContexts, egui};
use planet_rs::{
    NoiseKind, NoiseLayer, PlanetError, PlanetOptions, PlanetTerrain, TerrainOptions,
    TerrainSource, generate_terrain,
};
use std::{mem, sync::Arc, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct Settings {
    pub planet: PlanetOptions,
    pub terrain: TerrainOptions,
    pub mode: GenerationMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self::from(&Command::default())
    }
}

impl From<&Command> for Settings {
    fn from(command: &Command) -> Self {
        Self {
            planet: command.options,
            terrain: command.terrain,
            mode: command.generation,
        }
    }
}

impl Settings {
    pub fn generate(&self) -> Result<PlanetTerrain, PlanetError> {
        let terrain = if self.mode == GenerationMode::Sphere {
            TerrainOptions {
                impacts: planet_rs::ImpactOptions::default(),
                relief: 0.0,
                land_bias: 0.0,
                hydraulic_passes: 0,
                thermal_passes: 0,
                river_depth: 0.0,
                refinement_passes: 0,
                river_refinement_passes: 0,
                smoothing_passes: 0,
                connect_seas: false,
                ..self.terrain
            }
        } else {
            self.terrain
        };
        generate_terrain(self.planet, terrain)
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        self.terrain.validate().map_err(|e| e.to_string())?;
        if !(4..=8192).contains(&self.planet.vertex_count)
            || !self.planet.radius.is_finite()
            || self.planet.radius <= 0.0
            || !self.planet.sweep_direction.is_finite()
            || !self.planet.sweep_direction.length_squared().is_finite()
            || self.planet.sweep_direction.length_squared() < 1.0e-20
        {
            return Err(String::from(
                "Use 4–8192 starting points and a positive radius.",
            ));
        }
        Ok(())
    }
}

struct Snapshot {
    settings: Settings,
    terrain: Arc<PlanetTerrain>,
}
struct Job {
    task: Task<Result<PlanetTerrain, PlanetError>>,
    settings: Settings,
    started: Instant,
}
#[derive(Clone, Copy)]
enum Action {
    Generate,
    Previous,
}

#[derive(Resource, Default)]
pub(super) struct Refresh {
    pub terrain: bool,
    pub water: bool,
}

#[derive(Resource)]
pub(super) struct Panel {
    draft: Settings,
    seed: String,
    applied: Settings,
    previous: Option<Snapshot>,
    // Own the generated terrain until its render snapshot can be installed.
    pending: Option<(Snapshot, Action)>,
    job: Option<Job>,
    action: Option<Action>,
    hidden: bool,
    status: String,
    sea_coverage: Option<f64>,
    error: Option<String>,
    viewport: Option<(egui::Rect, f32)>,
    view: Option<View>,
    last_view: View,
}

impl Panel {
    pub fn new(settings: &Settings, hidden: bool) -> Self {
        Self {
            draft: *settings,
            seed: settings.planet.seed.to_string(),
            applied: *settings,
            previous: None,
            pending: None,
            job: None,
            action: None,
            hidden,
            status: String::from("Ready"),
            sea_coverage: None,
            error: None,
            viewport: None,
            view: None,
            last_view: View::Front,
        }
    }

    fn settings(&self) -> Result<Settings, String> {
        let seed = self
            .seed
            .parse::<u64>()
            .map_err(|_| String::from("Seed must be an unsigned whole number."))?;
        let settings = Settings {
            planet: PlanetOptions {
                seed,
                ..self.draft.planet
            },
            ..self.draft
        };
        settings.validate()?;
        Ok(settings)
    }

    pub(super) fn restore_draft(&mut self, draft: &Settings, view: View) {
        self.draft = *draft;
        self.seed = draft.planet.seed.to_string();
        self.last_view = view;
    }
    pub(super) fn remembered_generation(&self) -> (Settings, Option<Settings>, bool, View) {
        (
            self.applied,
            self.settings().ok(),
            self.hidden,
            self.last_view,
        )
    }

    fn show_snapshot(&mut self, scene: &mut Scene, snapshot: Snapshot) {
        let previous = Snapshot {
            settings: self.applied,
            terrain: mem::replace(&mut scene.terrain, snapshot.terrain),
        };
        scene.seed = snapshot.settings.planet.seed;
        scene.generation = snapshot.settings.mode;
        self.applied = snapshot.settings;
        self.previous = Some(previous);
    }

    fn busy(&self) -> bool {
        self.job.is_some() || self.pending.is_some()
    }

    pub(super) fn pending_terrain(&self) -> Option<(&PlanetTerrain, GenerationMode)> {
        self.pending
            .as_ref()
            .map(|(snapshot, _)| (snapshot.terrain.as_ref(), snapshot.settings.mode))
    }

    pub(super) fn activate_pending(&mut self, scene: &mut Scene) {
        if let Some((snapshot, action)) = self.pending.take() {
            self.show_snapshot(scene, snapshot);
            self.sea_coverage = Some(scene.terrain.sea_area_fraction());
            self.status = match action {
                Action::Generate => format!("Showing seed {} · render ready", scene.seed),
                Action::Previous => {
                    String::from("Showing previous result · click again to compare")
                }
            };
            info!("Activated seed {} with its rendered chunks", scene.seed);
        }
    }

    pub(super) fn render_failed(&mut self, error: &str) {
        if let Some((snapshot, action)) = self.pending.take() {
            if matches!(action, Action::Previous) {
                self.previous = Some(snapshot);
            }
            self.error = Some(error.to_owned());
            self.status = String::from("Render preparation failed · current planet kept");
        }
    }
}

pub(super) fn update_generation(mut panel: ResMut<Panel>, mut refresh: ResMut<Refresh>) {
    if let Some(job) = &mut panel.job
        && let Some(result) = block_on(poll_once(&mut job.task))
    {
        let settings = job.settings;
        let elapsed = job.started.elapsed();
        panel.job = None;
        match result {
            Ok(terrain) => {
                info!(
                    "Generated seed {}: {} vertices in {:.2?}",
                    settings.planet.seed,
                    terrain.elevations.len(),
                    elapsed
                );
                panel.pending = Some((
                    Snapshot {
                        settings,
                        terrain: Arc::new(terrain),
                    },
                    Action::Generate,
                ));
                panel.error = None;
                panel.status = format!(
                    "Generated in {:.1}s · preparing render",
                    elapsed.as_secs_f64()
                );
                *refresh = Refresh {
                    terrain: true,
                    water: true,
                };
            }
            Err(error) => {
                panel.error = Some(error.to_string());
                panel.status = String::from("Generation failed · current planet kept");
            }
        }
    }
    let Some(action) = panel.action.take() else {
        return;
    };
    if panel.busy() {
        return;
    }
    match action {
        Action::Generate => match panel.settings() {
            Ok(settings) => {
                let task = AsyncComputeTaskPool::get().spawn(async move { settings.generate() });
                panel.job = Some(Job {
                    task,
                    settings,
                    started: Instant::now(),
                });
                panel.error = None;
            }
            Err(error) => panel.error = Some(error),
        },
        Action::Previous => {
            if let Some(previous) = panel.previous.take() {
                panel.draft = previous.settings;
                panel.seed = previous.settings.planet.seed.to_string();
                panel.pending = Some((previous, Action::Previous));
                panel.status = String::from("Preparing previous result for display…");
                panel.error = None;
                *refresh = Refresh {
                    terrain: true,
                    water: true,
                };
            }
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct WeatherPanel<'w> {
    environment: Option<ResMut<'w, super::environment::EnvironmentSettings>>,
    forest_options: Option<ResMut<'w, super::forest::ForestSettings>>,
    forests: Option<ResMut<'w, super::forest::Forests>>,
    simulation: Option<ResMut<'w, super::weather::WeatherControls>>,
    fields: Option<Res<'w, super::weather::WeatherFrame>>,
    solar_controls: Option<ResMut<'w, super::solar::SolarControls>>,
    solar_frame: Option<Res<'w, super::solar::SolarFrame>>,
    preferences: Option<Res<'w, super::preferences::PreferencesStore>>,
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn draw(
    mut contexts: EguiContexts,
    mut panel: ResMut<Panel>,
    mut scene: ResMut<Scene>,
    mut refresh: ResMut<Refresh>,
    keys: Res<ButtonInput<KeyCode>>,
    mut explorer: ResMut<super::explore::Explorer>,
    mut streaming: ResMut<super::streaming::StreamingSettings>,
    stats: Res<super::streaming::StreamingStats>,
    mut chunks: ResMut<super::streaming::ChunkWorld>,
    mut detail_settings: Option<ResMut<super::details::DetailSettings>>,
    detail_stats: Option<Res<super::details::Details>>,
    mut lighting: Option<ResMut<super::lighting::LightingSettings>>,
    mut ocean: Option<ResMut<super::ocean::OceanSettings>>,
    ocean_stats: Option<Res<super::ocean::OceanWorld>>,
    mut water: Option<ResMut<super::water::WaterSettings>>,
    weather: WeatherPanel,
) {
    let WeatherPanel {
        mut environment,
        mut forest_options,
        mut forests,
        mut simulation,
        fields,
        mut solar_controls,
        solar_frame,
        preferences,
    } = weather;
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("planet viewport"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    if explorer.walking() {
        egui::Area::new(egui::Id::new("first person controls"))
            .fixed_pos([16.0, 16.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height((ctx.viewport_rect().height() - 48.0).max(100.0))
                        .show(ui, |ui| {
                            ui.strong(format!(
                                "First person · planet radius {:.0} m",
                                scene.terrain.topology.radius
                            ));
                            ui.label("WASD: move · Mouse: look · Shift: run · Space: jump");
                            ui.label("Esc: return to orbit · Tab: release mouse");
                            if !explorer.captured() {
                                ui.label("Mouse released · view paused for screenshots.");
                                ui.label("Click the world to resume looking around.");
                                if ui.button("Return to orbit").clicked() {
                                    explorer.return_to_orbit = true;
                                }
                                streaming_controls(ui, &mut streaming, &stats, &mut chunks);
                                if let Some(options) = ocean.as_mut() {
                                    ocean_controls(ui, options, ocean_stats.as_deref());
                                }
                                if let Some(options) = water.as_mut() {
                                    water_controls(ui, options);
                                }
                                if let Some(options) = environment.as_mut() {
                                    environment_controls(
                                        ui,
                                        options,
                                        solar_controls.as_ref().is_some_and(|s| s.enabled),
                                    );
                                    if let Some(simulation) = simulation.as_deref_mut() {
                                        weather_controls(
                                            ui,
                                            simulation,
                                            fields.as_deref(),
                                            solar_controls.as_ref().is_some_and(|s| s.enabled),
                                        );
                                    }
                                }
                                if let Some(solar) = solar_controls.as_deref_mut() {
                                    crate::solar::controls(
                                        ui,
                                        solar,
                                        solar_frame.as_deref(),
                                        scene.terrain.topology.radius,
                                    );
                                }
                                if let Some(options) = lighting.as_mut() {
                                    lighting_controls(ui, options);
                                }
                                if let Some(options) = detail_settings.as_mut() {
                                    detail_controls(ui, options, detail_stats.as_deref());
                                }
                                if let (Some(options), Some(world)) =
                                    (forest_options.as_deref_mut(), forests.as_deref_mut())
                                {
                                    forest_controls(ui, options, world);
                                }
                            }
                        });
                });
            });
        let center = ctx.viewport_rect().center();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("crosshair"),
        ));
        if explorer.captured() {
            painter.line_segment(
                [center - egui::vec2(5.0, 0.0), center + egui::vec2(5.0, 0.0)],
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            );
            painter.line_segment(
                [center - egui::vec2(0.0, 5.0), center + egui::vec2(0.0, 5.0)],
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            );
        }
        panel.viewport = Some((ctx.viewport_rect(), ctx.pixels_per_point()));
        return;
    }
    if keys.just_pressed(KeyCode::F1) {
        panel.hidden = !panel.hidden;
    }
    if panel.hidden {
        egui::Area::new(egui::Id::new("show controls"))
            .fixed_pos([16.0, 16.0])
            .show(ctx, |ui| {
                if ui.button("Parameters · F1").clicked() {
                    panel.hidden = false;
                }
            });
    } else {
        let sea = *panel
            .sea_coverage
            .get_or_insert_with(|| scene.terrain.sea_area_fraction())
            * 100.0;
        egui::Panel::left("planet parameters").default_size(345.0).size_range(320.0..=440.0).resizable(true).show(&mut viewport_ui, |ui| {
            ui.heading("Planet experiment");
            ui.label(format!("Showing seed {}", scene.seed));
            ui.small(format!("{} terrain vertices · {} water triangles", scene.terrain.elevations.len(), scene.terrain.river_surface.triangles.len()));
            ui.small(format!("Estimated sea area: {sea:.1}%"));
            if let Some(preferences)=preferences.as_ref() {
                if let Some(error)=preferences.error.as_deref() {ui.colored_label(egui::Color32::YELLOW,format!("Settings could not be saved: {error}"));}
                else {ui.small("Settings remembered between launches");}
            }
            ui.separator();
            generation_buttons(ui, &mut panel);
            ui.separator();
            display_controls(ui, &mut scene, &mut refresh, &mut panel);
            streaming_controls(ui, &mut streaming, &stats, &mut chunks);
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui, |ui| {
                settings_controls(ui, &mut panel);
                if let Some(options) = detail_settings.as_mut() {
                    detail_controls(ui, options, detail_stats.as_deref());
                }
                if let (Some(options),Some(world))=(forest_options.as_deref_mut(),forests.as_deref_mut()) {forest_controls(ui,options,world);}
                if let Some(options) = lighting.as_mut() {lighting_controls(ui, options);}
                if let Some(options)=ocean.as_mut() {ocean_controls(ui,options,ocean_stats.as_deref());}
                if let Some(options)=water.as_mut() {water_controls(ui,options);}
                if let Some(options)=environment.as_mut() {environment_controls(ui,options,solar_controls.as_ref().is_some_and(|s|s.enabled));}
                if let Some(options)=simulation.as_deref_mut() { weather_controls(ui,options,fields.as_deref(),solar_controls.as_ref().is_some_and(|s|s.enabled)); }
                if let Some(solar)=solar_controls.as_deref_mut() {crate::solar::controls(ui,solar,solar_frame.as_deref(),scene.terrain.topology.radius);}
                ui.add_space(10.0);
                ui.small("Sea level stays at the original sphere radius.\nOcean cuts run only at LOD 2 and LOD 1.");
                ui.small("Click the planet to explore on foot.\nDrag to orbit; scroll to zoom.\nHome: reset view · F1: hide controls · Esc: exit");
            });
        });
    }
    panel.viewport = Some((
        viewport_ui.available_rect_before_wrap(),
        ctx.pixels_per_point(),
    ));
}

fn water_controls(ui: &mut egui::Ui, settings: &mut super::water::WaterSettings) {
    egui::CollapsingHeader::new("Sea & river effects").show(ui, |ui| {
        ui.add(egui::Slider::new(&mut settings.distortion, 0.0..=1.5).text("Surface distortion"));
        ui.add(egui::Slider::new(&mut settings.absorption, 0.0..=4.0).text("Absorption"));
        ui.add(egui::Slider::new(&mut settings.reflections, 0.0..=1.0).text("Scene reflections"));
        ui.add(egui::Slider::new(&mut settings.river_ripples, 0.0..=0.5).text("River ripples"));
        ui.add(egui::Slider::new(&mut settings.whitewater, 0.0..=1.0).text("River whitewater"));
        ui.add(
            egui::Slider::new(&mut settings.waterfall_speed, 0.0..=20.0)
                .text("Falling water (m/s)"),
        );
        ui.add(egui::Slider::new(&mut settings.foam_strength, 0.0..=2.0).text("Persistent foam"));
        ui.add(
            egui::Slider::new(&mut settings.foam_lifetime, 0.1..=30.0).text("Foam lifetime (s)"),
        );
        ui.add(egui::Slider::new(&mut settings.mist, 0.0..=2.0).text("Waterfall mist"));
        ui.checkbox(&mut settings.underwater, "Underwater absorption & haze");
    });
}

fn ocean_controls(
    ui: &mut egui::Ui,
    settings: &mut super::ocean::OceanSettings,
    stats: Option<&super::ocean::OceanWorld>,
) {
    egui::CollapsingHeader::new("Ocean").show(ui, |ui| {
        ui.checkbox(&mut settings.waves, "Geometric waves");
        ui.checkbox(&mut settings.paused, "Freeze animation");
        ui.add(egui::Slider::new(&mut settings.amplitude, 0.0..=3.0).text("Wave height scale"));
        ui.add(egui::Slider::new(&mut settings.speed, 0.0..=4.0).text("Wave speed"));
        ui.add(
            egui::Slider::new(&mut settings.source_drift, 0.0..=2.0)
                .text("Manual source drift (m/s)"),
        );
        ui.add(egui::Slider::new(&mut settings.choppiness, 0.0..=1.0).text("Choppiness"));
        ui.add(egui::Slider::new(&mut settings.foam, 0.0..=2.0).text("Foam"));
        ui.add(egui::Slider::new(&mut settings.roughness, 0.04..=0.8).text("Roughness"));
        ui.add(egui::Slider::new(&mut settings.refraction, 0.0..=1.0).text("Refraction"));
        ui.add(
            egui::Slider::new(&mut settings.fine_radius, 16.0..=256.0).text("Fine ring radius (m)"),
        );
        ui.add(egui::Slider::new(&mut settings.spacing, 0.5..=4.0).text("Fine spacing (m)"));
        ui.horizontal(|ui| {
            ui.label("Map");
            ui.selectable_value(&mut settings.debug, 0, "Shaded");
            ui.selectable_value(&mut settings.debug, 1, "Depth");
            ui.selectable_value(&mut settings.debug, 2, "Land distance");
            ui.selectable_value(&mut settings.debug, 3, "Wave drivers");
        });
        if let Some(stats) = stats {
            ui.small(&stats.status);
            ui.small(format!(
                "{} vertices · {} rings · {}² coast maps",
                stats.vertices, stats.rings, stats.map_size
            ));
        }
    });
}

fn lighting_controls(ui: &mut egui::Ui, settings: &mut super::lighting::LightingSettings) {
    egui::CollapsingHeader::new("Lighting & ambient occlusion").show(ui, |ui| {
        ui.checkbox(&mut settings.shadows, "Sun shadows");
        ui.add(
            egui::Slider::new(&mut settings.shadow_distance, 20.0..=3000.0)
                .logarithmic(true)
                .text("Shadow range (m)"),
        );
        ui.add(
            egui::Slider::new(&mut settings.shadow_bias, 0.0..=0.25)
                .logarithmic(true)
                .text("Shadow bias (m)"),
        );
        ui.checkbox(&mut settings.ao, "Depth-buffer ambient occlusion");
        ui.add(egui::Slider::new(&mut settings.ao_intensity, 0.0..=2.5).text("AO strength"));
        ui.add(
            egui::Slider::new(&mut settings.ao_radius, 0.01..=20.0)
                .logarithmic(true)
                .text("AO radius (m)"),
        );
        ui.horizontal(|ui| {
            ui.label("Samples");
            for samples in [6, 10, 12] {
                ui.selectable_value(&mut settings.ao_samples, samples, samples.to_string());
            }
        });
        ui.checkbox(&mut settings.half_resolution, "Half-resolution AO");
        ui.checkbox(&mut settings.ao_only, "Show AO only");
    });
}

fn detail_controls(
    ui: &mut egui::Ui,
    options: &mut super::details::DetailSettings,
    stats: Option<&super::details::Details>,
) {
    egui::CollapsingHeader::new("Surface detail").show(ui, |ui| {
        ui.checkbox(&mut options.grass, "Fur grass");
        ui.checkbox(&mut options.stones, "Stones");
        ui.checkbox(&mut options.boulders, "Large boulders");
        ui.add(egui::Slider::new(&mut options.height, 0.02..=0.5).text("Grass height (m)"));
        ui.add(egui::Slider::new(&mut options.density, 2.0..=32.0).text("Tufts per metre"));
        ui.add(egui::Slider::new(&mut options.rock_density, 0.2..=2.0).text("Rock density"));
        if let Some(stats) = stats {
            ui.small(format!(
                "{} grass patches · {} stones · {} boulders",
                stats.grass_patches, stats.stones, stats.boulders
            ));
        }
    });
}

fn forest_controls(
    ui: &mut egui::Ui,
    options: &mut super::forest::ForestSettings,
    world: &mut super::forest::Forests,
) {
    egui::CollapsingHeader::new("Forests").show(ui, |ui| {
        ui.checkbox(&mut options.enabled, "Forests");
        ui.add(egui::Slider::new(&mut options.distance, 0.0..=400.0).text("Detailed trees (m)"))
            .on_hover_text("Set to 0 to show only the coarse trunks and canopies.");
        ui.small(format!(
            "{} trees · {} detailed · {} simplified",
            world.trees,
            world.detailed,
            world.simplified_trees()
        ));
        if ui
            .add_enabled(
                options.enabled && world.trees > 0,
                egui::Button::new("Inspect forest"),
            )
            .clicked()
        {
            world.inspect = true;
        }
        ui.small("Tree detail follows distance in every view.");
    });
}

fn streaming_controls(
    ui: &mut egui::Ui,
    settings: &mut super::streaming::StreamingSettings,
    stats: &super::streaming::StreamingStats,
    chunks: &mut super::streaming::ChunkWorld,
) {
    egui::CollapsingHeader::new("Chunks & streaming")
        .default_open(true)
        .show(ui, |ui| {
            ui.small(&stats.status);
            ui.label(format!(
                "{} / {} visible · {} triangles",
                stats.visible, stats.total, stats.triangles
            ));
            ui.small(format!(
                "Horizon hidden: {} · outside view: {}",
                stats.hidden_horizon, stats.hidden_frustum
            ));
            ui.small(format!("LOD 0/1/2/3: {:?}", stats.lods));
            ui.small(format!("{} patches using baked surface maps", stats.mapped));
            ui.small(format!(
                "Rivers: {} mesh / {} texture patches",
                stats.river_meshes, stats.river_textures
            ));
            ui.small(format!(
                "{} MiB resident · {} loading",
                stats.resident_bytes / (1024 * 1024),
                stats.loading
            ));
            ui.horizontal(|ui| {
                ui.checkbox(&mut settings.horizon, "Horizon culling");
                ui.checkbox(&mut settings.colours, "Chunk colours");
            });
            ui.checkbox(&mut settings.surface_maps, "LOD normal + occlusion maps");
            ui.horizontal(|ui| {
                ui.label("Surface:");
                ui.selectable_value(&mut settings.surface_debug, 0, "Shaded");
                ui.selectable_value(&mut settings.surface_debug, 1, "Normals");
                ui.selectable_value(&mut settings.surface_debug, 2, "Occlusion");
            });
            ui.horizontal(|ui| {
                ui.label("LOD:");
                ui.selectable_value(&mut settings.forced_lod, None, "Auto");
                for lod in 0..4 {
                    ui.selectable_value(&mut settings.forced_lod, Some(lod), lod.to_string());
                }
            });
            ui.add(
                egui::Slider::new(&mut settings.target_pixels, 0.5..=8.0)
                    .text("LOD error (pixels)"),
            );
            ui.add(
                egui::Slider::new(&mut settings.budget_mb, 32..=512).text("Detail budget (MiB)"),
            );
            if ui
                .add(egui::Slider::new(&mut settings.level, 0..=3).text("Chunk depth"))
                .changed()
            {
                chunks.rebuild = true;
            }
            ui.small("Coarse fallback stays loaded. Detailed chunks stream nearby.");
        });
}

fn environment_controls(
    ui: &mut egui::Ui,
    settings: &mut super::environment::EnvironmentSettings,
    solar_enabled: bool,
) {
    egui::CollapsingHeader::new("Atmosphere & clouds").show(ui, |ui| {
        ui.checkbox(&mut settings.atmosphere, "Atmosphere");
        ui.add(egui::Slider::new(&mut settings.atmosphere_height,20.0..=2000.0).logarithmic(true).text("Air height (m)"));
        ui.add(egui::Slider::new(&mut settings.haze,0.0..=5.0).text("Haze"));
        ui.separator();
        ui.checkbox(&mut settings.clouds,"Volumetric clouds");
        ui.checkbox(&mut settings.clear_terrain,"Keep cloud base above mountains");
        ui.add(egui::Slider::new(&mut settings.coverage,0.0..=1.0).text("Coverage"));
        ui.add(egui::Slider::new(&mut settings.density,0.0..=8.0).text("Density"));
        ui.add(egui::Slider::new(&mut settings.cloud_base,1.0..=1500.0).text("Minimum cloud base (m)"));
        ui.add(egui::Slider::new(&mut settings.cloud_thickness,5.0..=1000.0).text("Thickness (m)"));
        ui.add(egui::Slider::new(&mut settings.cloud_scale,10.0..=2000.0).logarithmic(true).text("Formation size (m)"));
        ui.add(egui::Slider::new(&mut settings.wind_speed,0.0..=40.0).text("Manual cloud drift (m/s)"));
        ui.add(egui::Slider::new(&mut settings.shadow_strength,0.0..=1.0).text("Cloud shadows"));
        ui.horizontal(|ui| {
            ui.label("Cloud quality");
            for (value,label) in [(0,"Low"),(1,"Balanced"),(2,"High")] {ui.selectable_value(&mut settings.quality,value,label);}
        });
        ui.checkbox(&mut settings.temporal,"Temporal reconstruction");
        ui.checkbox(&mut settings.paused,"Pause weather");
        ui.separator();
        ui.add_enabled(!solar_enabled, egui::Slider::new(&mut settings.sun_azimuth,-180.0..=180.0).text("Sun azimuth"));
        ui.add_enabled(!solar_enabled, egui::Slider::new(&mut settings.sun_elevation,-90.0..=90.0).text("Sun elevation"));
        if solar_enabled {ui.small("Sun position follows the Solar system controls.");}
        ui.small("Daylight depends on your location on the planet. Air height expands to include the cloud layer.");
    });
}

fn generation_buttons(ui: &mut egui::Ui, panel: &mut Panel) {
    let valid = panel.settings();
    let busy = panel.busy();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!busy && valid.is_ok(), egui::Button::new("Generate"))
            .clicked()
        {
            panel.action = Some(Action::Generate);
        }
        if ui
            .add_enabled(!busy && valid.is_ok(), egui::Button::new("New seed"))
            .on_hover_text("Advance the seed and generate a new planet.")
            .clicked()
        {
            panel.seed = panel
                .seed
                .parse::<u64>()
                .unwrap_or(42)
                .wrapping_add(1)
                .to_string();
            panel.action = Some(Action::Generate);
        }
        if ui
            .add_enabled(
                !busy && panel.previous.is_some(),
                egui::Button::new("Previous"),
            )
            .on_hover_text("Swap the displayed planet and settings with the previous result.")
            .clicked()
        {
            panel.action = Some(Action::Previous);
        }
    });
    ui.horizontal(|ui| {
        if ui
            .button("Quick preview")
            .on_hover_text(
                "Set 1024 starting points, 3 refinements, and 1 river refinement. Then Generate.",
            )
            .clicked()
        {
            panel.draft.planet.vertex_count = 1024;
            panel.draft.terrain.refinement_passes = 3;
            panel.draft.terrain.river_refinement_passes = 1;
        }
        if ui
            .button("Full detail")
            .on_hover_text(
                "Set 4096 starting points, 5 refinements, and 3 river refinements. Then Generate.",
            )
            .clicked()
        {
            panel.draft.planet.vertex_count = 4096;
            panel.draft.terrain.refinement_passes = 5;
            panel.draft.terrain.river_refinement_passes = 3;
        }
        if ui.button("Defaults").clicked() {
            panel.draft = Settings::from(&Command::default());
            panel.seed = panel.draft.planet.seed.to_string();
        }
    });
    generation_status(ui, panel, &valid);
}

fn generation_status(ui: &mut egui::Ui, panel: &Panel, valid: &Result<Settings, String>) {
    // Reserve the status rows so the first draft edit cannot move a slider
    // out from under the pointer during a drag.
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 44.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            if let Some(job) = &panel.job {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "Generating · {:.0}s",
                        job.started.elapsed().as_secs_f64()
                    ));
                });
                ui.ctx().request_repaint();
            } else if panel.pending.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&panel.status);
                });
                ui.ctx().request_repaint();
            } else {
                ui.label(&panel.status);
            }
            let error = valid.as_ref().err().or(panel.error.as_ref());
            let dirty = valid.as_ref().is_ok_and(|settings| {
                *settings
                    != panel
                        .pending
                        .as_ref()
                        .map_or(panel.applied, |(snapshot, _)| snapshot.settings)
            });
            let (text, colour) = if let Some(error) = error {
                (error.as_str(), egui::Color32::LIGHT_RED)
            } else if dirty {
                (
                    "Settings changed · Generate to apply",
                    egui::Color32::LIGHT_BLUE,
                )
            } else {
                (" ", egui::Color32::GRAY)
            };
            ui.add(egui::Label::new(egui::RichText::new(text).color(colour)).truncate())
                .on_hover_text(text);
        },
    );
}

fn display_controls(
    ui: &mut egui::Ui,
    scene: &mut Scene,
    refresh: &mut Refresh,
    panel: &mut Panel,
) {
    ui.horizontal(|ui| {
        let mut sea = !scene.no_ocean;
        let mut rivers = scene.river_display == RiverDisplay::Show;
        ui.checkbox(&mut sea, "Sea");
        ui.checkbox(&mut rivers, "Rivers");
        ui.checkbox(&mut scene.wireframe, "Edges");
        scene.no_ocean = !sea;
        scene.river_display = if rivers {
            RiverDisplay::Show
        } else {
            RiverDisplay::Hide
        };
    });
    ui.horizontal(|ui| {
        refresh.terrain |= ui.checkbox(&mut scene.smooth, "Smooth shading").changed();
        let mut carved = scene.bed == BedView::Carved;
        if ui.checkbox(&mut carved, "Carved beds").changed() {
            scene.bed = if carved {
                BedView::Carved
            } else {
                BedView::Original
            };
            refresh.terrain = true;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("View:");
        for (label, view) in [
            ("Front", View::Front),
            ("Back", View::Back),
            ("Mountains", View::Mountains),
            ("Volcanoes", View::Volcanoes),
            ("Craters", View::Craters),
            ("River", View::River),
            ("Falls", View::Waterfall),
            ("Mouth", View::Mouth),
            ("Sky", View::Sky),
            ("Clouds", View::Clouds),
            ("Forest", View::Forest),
        ] {
            if ui.button(label).clicked() {
                panel.view = Some(view);
            }
        }
    });
}

fn noise_controls(ui: &mut egui::Ui, terrain: &mut TerrainOptions) {
    egui::CollapsingHeader::new("Initial noise layers").default_open(true).show(ui,|ui| {
        ui.horizontal(|ui| {
            if ui.button("Classic blend").clicked() { terrain.noise_layers=NoiseLayer::classic(); }
            if ui.button("Voronoi plates blend").clicked() { terrain.noise_layers=NoiseLayer::plates(); }
        });
        for (i,layer) in terrain.noise_layers.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Layer {}",i+1));
                    egui::ComboBox::from_id_salt(("initial-noise-kind",i)).selected_text(layer.kind.label()).show_ui(ui,|ui| {
                        for kind in [NoiseKind::Fractal,NoiseKind::Ridged,NoiseKind::Voronoi] {
                            ui.selectable_value(&mut layer.kind,kind,kind.label());
                        }
                    });
                });
                ui.add(egui::Slider::new(&mut layer.frequency,0.01..=100.0).logarithmic(true).text("Frequency"))
                    .on_hover_text("Higher frequency makes smaller terrain features.");
                ui.add(egui::Slider::new(&mut layer.weight,0.0..=2.0).text("Height weight"))
                    .on_hover_text("Relative share of the initial height field. Zero turns this layer off; Relief sets the overall height.");
            });
        }
        ui.small("Weights normalize. Set a weight to zero to disable its layer. Apply with Generate.");
        ui.small("Voronoi plates makes broad interiors and warped boundary ridges; it is a terrain pattern, not a plate simulation.");
    });
}

fn tectonic_controls(ui: &mut egui::Ui, terrain: &mut TerrainOptions, radius: f64) {
    egui::CollapsingHeader::new("Tectonic plates").default_open(true).show(ui, |ui| {
        let t = &mut terrain.tectonics;
        ui.add(egui::Slider::new(&mut t.plates, 4..=64).text("Plate count"));
        ui.add(egui::Slider::new(&mut t.steps, 1..=128).text("Drift steps"))
            .on_hover_text("More steps resolve the same drift history more accurately; they do not increase total movement.");
        ui.add(egui::Slider::new(&mut t.drift, 0.0..=1.0).text("Drift distance (radians)"))
            .on_hover_text("Total motion around the sphere. Zero stops plate motion and boundary deformation.");
        ui.add(egui::Slider::new(&mut t.continental_fraction, 0.0..=1.0).text("Continental plate fraction"))
            .on_hover_text("Continental crust starts higher than oceanic crust. This sets the fraction of plate seeds, not a fixed land or sea area.");
        ui.add(egui::Slider::new(&mut t.fragmentation, 0.0..=1.0).text("Continental fragmentation"))
            .on_hover_text("Breaks the regular plate-shaped crust into irregular continental fragments carried by plate motion. Zero restores the original plate-cell crust.");
        ui.add(egui::Slider::new(&mut t.fragment_scale, 0.35..=2.0).text("Continental fragment size"))
            .on_hover_text("Relative size of the continental fragments. Larger values make broader pieces; smaller values make finer coastal structure.");
        ui.add(egui::Slider::new(&mut t.rift_strength, 0.0..=2.0).text("Rift basin strength"))
            .on_hover_text("Sinks branching fault basins where continental crust stretches, forming gulfs and straits. Zero disables the additional basins; the original narrow boundary rifts remain.");
        ui.add(egui::Slider::new(&mut t.rift_branching, 0..=4).text("Rift branching levels"))
            .on_hover_text("Successive branches create smaller fault valleys. Zero disables the additional rift basin network.");
        ui.add(egui::Slider::new(&mut t.boundary_width, 0.05..=1.0).text("Mountain belt width"))
            .on_hover_text("Width of the narrow boundary spine and its surrounding foothills.");
        let width = (4.0*std::f64::consts::PI/f64::from(t.plates)).sqrt()*t.boundary_width*0.12*radius;
        ui.small(format!("Nominal spine half-width: {width:.1} m"));
        ui.add(egui::Slider::new(&mut t.deformation, 0.0..=3.0).text("Push / pull strength"));
        ui.add(egui::Slider::new(&mut t.branching,0.0..=1.0).text("Branching ridge strength"))
            .on_hover_text("Spurs attach at each convex bend apex at full parent height. Strength controls how much height they retain farther out; zero disables them.");
        ui.add(egui::Slider::new(&mut t.branch_depth,0..=4).text("Branching levels"))
            .on_hover_text("Successive generations are shorter, narrower and lower. Zero disables the branch network.");
        ui.add(egui::Slider::new(&mut t.ruggedness, 0.0..=1.0).text("Mountain ruggedness"))
            .on_hover_text("Irregular linked summits, saddles and bends. Zero gives an idealised continuous ridge.");
        ui.add(egui::Slider::new(&mut t.volcanism, 0.0..=3.0).text("Volcanic activity"))
            .on_hover_text("Procedural cone chains along sliding plate boundaries; zero disables them.");
        ui.add(egui::Slider::new(&mut t.volcano_spacing, 0.5..=3.0).text("Volcano spacing"));
        ui.add(egui::Slider::new(&mut t.detail, 0.0..=0.25).text("Noise detail"))
            .on_hover_text("Small contribution from the noise layers below. Zero uses only plate-derived heights.");
        ui.small("Compression raises terrain and hardens rock; separation lowers terrain and weakens rock.");
        ui.small("A simplified drift model with oceanic and continental crust. Apply with Generate.");
    });
}

fn settings_controls(ui: &mut egui::Ui, panel: &mut Panel) {
    egui::CollapsingHeader::new("Planet & terrain")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Seed");
                ui.add(egui::TextEdit::singleline(&mut panel.seed).desired_width(200.0));
            });
            ui.add(
                egui::Slider::new(&mut panel.draft.planet.vertex_count, 32..=8192)
                    .logarithmic(true)
                    .text("Starting points"),
            );
            let t = &mut panel.draft.terrain;
            ui.add(egui::Slider::new(&mut t.relief, 0.0..=0.45).text("Relief"));
            ui.add(egui::Slider::new(&mut t.land_bias, -0.3..=0.3).text("Land bias"));
            ui.horizontal(|ui| {
                ui.label("Terrain source");
                egui::ComboBox::from_id_salt("terrain-source").selected_text(t.source.label()).show_ui(ui, |ui| {
                    for source in [TerrainSource::Noise, TerrainSource::Tectonics] {
                        ui.selectable_value(&mut t.source, source, source.label());
                    }
                });
            });
            if t.source == TerrainSource::Tectonics { tectonic_controls(ui, t, panel.draft.planet.radius); }
            noise_controls(ui,t);
            ui.horizontal(|ui| {
                ui.label("Radius (metres)");
                ui.add(
                    egui::DragValue::new(&mut panel.draft.planet.radius)
                        .range(0.0001..=1.0e12)
                        .speed(1.0),
                );
            })
            .response
            .on_hover_text(
                "Default 1000 m (1 km). Sets walking scale and export units; orbit appearance stays constant.",
            );
            let mut sphere = panel.draft.mode == GenerationMode::Sphere;
            ui.checkbox(&mut sphere, "Undeformed sphere");
            panel.draft.mode = if sphere {
                GenerationMode::Sphere
            } else {
                GenerationMode::Terrain
            };
        });
    let t = &mut panel.draft.terrain;
    egui::CollapsingHeader::new("Impact craters").show(ui, |ui| {
        ui.checkbox(&mut t.impacts.enabled,"Generate impact craters");
        ui.add_enabled_ui(t.impacts.enabled, |ui| {
            ui.add(egui::Slider::new(&mut t.impacts.per_stage,0..=24).text("Impacts per stage"));
            ui.add(egui::Slider::new(&mut t.impacts.size,0.25..=3.0).text("Crater size"));
            ui.add(egui::Slider::new(&mut t.impacts.strength,0.0..=3.0).text("Impact strength"));
        });
        ui.small("Seeded batches before erosion and each terrain stage. Later impacts are smaller; bowls lower terrain and rims lift it. Apply with Generate.");
    });
    egui::CollapsingHeader::new("Erosion").show(ui, |ui| {
        ui.add(egui::Slider::new(&mut t.erosion_strength, 0.0..=8.0).text("Hydraulic strength"))
            .on_hover_text("Higher strength removes and transports more material per sweep.");
        ui.add(egui::Slider::new(&mut t.hydraulic_passes, 0..=128).text("Hydraulic sweeps"))
            .on_hover_text("Each sweep adds a full rainfall/transport pass. More sweeps deepen erosion and take longer to generate.");
        ui.add(egui::Slider::new(&mut t.thermal_passes, 0..=128).text("Thermal passes"))
            .on_hover_text("Independent slope relaxation. Set this and hydraulic strength to zero to compare without erosion.");
        ui.small("Apply with Generate. For stronger erosion try strength 2–4 and 4–8 sweeps.");
        ui.small("Hydraulic and thermal erosion are independent; river carving is a separate stage.");
    });
    egui::CollapsingHeader::new("Tessellation & smoothing").show(ui, |ui| {
        ui.add(egui::Slider::new(&mut t.refinement_passes, 0..=6).text("Refinement stages"))
            .on_hover_text("All requested stages run without a vertex budget. Adaptive stages select terrain curvature.");
        ui.add(egui::Slider::new(&mut t.smoothing_passes, 0..=12).text("Smoothing passes"));
        ui.add(egui::Slider::new(&mut t.river_refinement_passes, 0..=3).text("River refinements"));
        ui.small(format!("Adaptive spacing cutoff: {:.3} m at this radius. No refinement vertex budget.", panel.draft.planet.radius * 0.0002));
        ui.small(format!("Seabed detail stops at {:.1} m depth; smoothing fades before this depth.", panel.draft.planet.radius * 0.01));
    });
    egui::CollapsingHeader::new("Rivers & seas").show(ui, |ui| {
        ui.add(
            egui::Slider::new(&mut t.river_catchment, 0.000_001..=1.0)
                .logarithmic(true)
                .max_decimals(6)
                .text("River catchment"),
        );
        ui.add(
            egui::Slider::new(&mut t.river_width, 0.0001..=0.03)
                .logarithmic(true)
                .max_decimals(5)
                .text("River width"),
        );
        ui.add(
            egui::Slider::new(&mut t.river_depth, 0.0..=0.02)
                .fixed_decimals(4)
                .text("River depth"),
        );
        ui.add(egui::Slider::new(&mut t.river_taper, 0.0..=1.0).text("Upstream narrowing"))
            .on_hover_text("Narrow headwaters that widen gradually downstream. Set to zero to compare with the previous river widths. Apply with Generate.");
        ui.checkbox(&mut t.connect_seas, "Fill inland seas before river carving")
            .on_hover_text("Keep the largest ocean; fill isolated seas to at least 0.5 m above sea level before the final river carve.");
    });
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn apply_view(
    mut panel: ResMut<Panel>,
    scene: Res<Scene>,
    mut cameras: Query<(&mut Camera, &mut Orbit, &mut Transform)>,
    mut initial: ResMut<InitialOrbit>,
    environment: Option<Res<super::environment::EnvironmentSettings>>,
    solar_frame: Option<Res<super::solar::SolarFrame>>,
    mut forests: Option<ResMut<super::forest::Forests>>,
) {
    let view = panel.view.take();
    if let Some(view) = view {
        panel.last_view = view;
    }
    for (mut camera, mut orbit, mut transform) in &mut cameras {
        if let Some((rect, scale)) = panel.viewport {
            let position = UVec2::new(
                (rect.min.x * scale).round() as u32,
                (rect.min.y * scale).round() as u32,
            );
            let size = UVec2::new(
                (rect.width() * scale).round().max(1.0) as u32,
                (rect.height() * scale).round().max(1.0) as u32,
            );
            let viewport = Viewport {
                physical_position: position,
                physical_size: size,
                ..default()
            };
            if camera
                .viewport
                .as_ref()
                .is_none_or(|old| old.physical_position != position || old.physical_size != size)
            {
                camera.viewport = Some(viewport);
            }
        }
        if let Some(view) = view {
            if view == View::Forest
                && let Some(forests) = forests.as_mut()
            {
                forests.inspect = true;
            }
            let defaults = super::environment::EnvironmentSettings::default();
            let settings = environment.as_deref().unwrap_or(&defaults);
            let mountains = if matches!(view, View::Mountains | View::Volcanoes | View::Craters) {
                let sun = solar_frame
                    .as_deref()
                    .filter(|frame| frame.enabled)
                    .and_then(|frame| frame.state)
                    .map_or_else(
                        || {
                            planet_rs::DVec3::from_array(
                                settings.sun_direction().to_array().map(f64::from),
                            )
                        },
                        |state| state.sun_direction,
                    );
                super::geology_orbit(view, &scene.terrain, sun)
            } else {
                None
            };
            *orbit = mountains
                .or_else(|| super::environment_orbit(view, &scene.terrain, settings))
                .unwrap_or_else(|| initial_orbit(view, &scene.terrain));
            initial.0 = *orbit;
            *transform = orbit_transform(&orbit);
        }
    }
}

fn weather_controls(
    ui: &mut egui::Ui,
    controls: &mut crate::weather::WeatherControls,
    frame: Option<&crate::weather::WeatherFrame>,
    solar_enabled: bool,
) {
    egui::CollapsingHeader::new("Global weather simulation").show(ui, |ui| {
        ui.checkbox(&mut controls.enabled, "Simulated global weather");
        ui.horizontal(|ui| {
            ui.checkbox(&mut controls.paused, "Pause weather");
            if ui.button("Step").clicked() {
                controls.paused = true;
                controls.step = true;
            }
        });
        ui.add(egui::Slider::new(&mut controls.speed, 0.0..=10.0).text("Time speed"));
        egui::ComboBox::from_label("Weather map")
            .selected_text(
                [
                    "Shaded",
                    "Pressure",
                    "Wind speed",
                    "Humidity",
                    "Clouds",
                    "Rain",
                    "Wave energy",
                    "Temperature",
                ][controls.debug.min(7) as usize],
            )
            .show_ui(ui, |ui| {
                for (i, name) in [
                    "Shaded",
                    "Pressure",
                    "Wind speed",
                    "Humidity",
                    "Clouds",
                    "Rain",
                    "Wave energy",
                    "Temperature",
                ]
                .iter()
                .enumerate()
                {
                    ui.selectable_value(&mut controls.debug, u32::try_from(i).unwrap(), *name);
                }
            });
        if controls.debug != 0 {
            ui.small("Blue: low · Red: high. Clouds can be hidden above for a clear map.");
        }
        ui.label("Climate settings (apply below)");
        ui.add(egui::Slider::new(&mut controls.model.humidity, 0.2..=2.0).text("Moisture"));
        ui.add(
            egui::Slider::new(&mut controls.model.solar_heating, 0.2..=2.0).text("Solar heating"),
        );
        let mut hours = controls.model.rotation_period / 3600.0;
        if ui
            .add_enabled(
                !solar_enabled,
                egui::Slider::new(&mut hours, 4.0..=72.0).text("Rotation (hours)"),
            )
            .changed()
        {
            controls.model.rotation_period = hours * 3600.0;
        }
        if solar_enabled {
            ui.small("Rotation follows the Solar system day setting.");
        }
        if ui.button("Apply climate / restart weather").clicked() {
            controls.reset = controls.reset.wrapping_add(1);
        }
        if let Some(frame) = frame {
            if let Some(error) = &frame.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            if controls.enabled && !frame.ready {
                ui.small("Preparing global weather…");
            }
            if let Some(stats) = &frame.stats {
                ui.small(format!(
                    "Weather time {:.1} h · {} ticks",
                    frame.seconds / 3600.0,
                    frame.ticks
                ));
                ui.small(format!(
                    "Wind {:.1} / {:.1} m/s mean/max · Cloud cover ~{:.0}%",
                    stats.mean_wind,
                    stats.maximum_wind,
                    stats.cloud_coverage * 100.0
                ));
                ui.small(format!(
                    "Clear regions {:.0}% · Overcast regions {:.0}%",
                    stats.clear_area * 100.0,
                    stats.overcast_area * 100.0
                ));
                ui.small(format!(
                    "Temperature {:.0}–{:.0} °C · Waves up to {:.2} m",
                    stats.minimum_temperature - 273.15,
                    stats.maximum_temperature - 273.15,
                    stats.maximum_wave_height
                ));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lod_material::LodMaterial, streaming};
    use bevy::mesh::VertexAttributeValues;
    use std::collections::BTreeSet;
    use std::{thread, time::Duration};

    #[test]
    fn tectonic_generate_captures_draft_settings_and_previous_restores_the_source() {
        let mut app = app();
        let original = app.world().resource::<Scene>().terrain.elevations.as_ptr();
        let original_geology = app.world().resource::<Panel>().applied.terrain.tectonics;
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            // The generic UI fixture has only 64 points and no refinement;
            // use enough source samples to resolve the narrower crust fragments.
            panel.draft.planet.vertex_count = 512;
            panel.draft.terrain.source = TerrainSource::Tectonics;
            panel.draft.terrain.river_catchment = 1.0;
            panel.draft.terrain.tectonics.steps = 16;
            panel.draft.terrain.tectonics.branching = 0.6;
            panel.draft.terrain.tectonics.branch_depth = 2;
            panel.draft.terrain.tectonics.volcanism = 0.75;
            panel.draft.terrain.tectonics.fragmentation = 0.9;
            panel.draft.terrain.tectonics.fragment_scale = 0.8;
            panel.draft.terrain.tectonics.rift_strength = 1.2;
            panel.draft.terrain.tectonics.rift_branching = 3;
            panel.draft.terrain.impacts = planet_rs::ImpactOptions {
                enabled: true,
                per_stage: 2,
                ..default()
            };
            panel.action = Some(Action::Generate);
        }
        app.update();
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            panel.draft.terrain.source = TerrainSource::Noise;
            panel.draft.terrain.tectonics.drift = 0.7;
            panel.draft.terrain.tectonics.fragmentation = 0.0;
            panel.draft.terrain.tectonics.rift_strength = 0.0;
        }
        finish(&mut app);
        activate(&mut app);
        let panel = app.world().resource::<Panel>();
        assert_eq!(panel.applied.terrain.source, TerrainSource::Tectonics);
        assert_eq!(panel.applied.planet.vertex_count, 512);
        assert_eq!(panel.applied.terrain.tectonics.steps, 16);
        assert!((panel.applied.terrain.tectonics.branching - 0.6).abs() < f64::EPSILON);
        assert_eq!(panel.applied.terrain.tectonics.branch_depth, 2);
        assert!(panel.applied.terrain.impacts.enabled);
        assert_eq!(panel.applied.terrain.impacts.per_stage, 2);
        assert!((panel.applied.terrain.tectonics.volcanism - 0.75).abs() < f64::EPSILON);
        assert!((panel.applied.terrain.tectonics.fragmentation - 0.9).abs() < f64::EPSILON);
        assert!((panel.applied.terrain.tectonics.fragment_scale - 0.8).abs() < f64::EPSILON);
        assert!((panel.applied.terrain.tectonics.rift_strength - 1.2).abs() < f64::EPSILON);
        assert_eq!(panel.applied.terrain.tectonics.rift_branching, 3);
        assert_eq!(
            panel.applied.terrain.tectonics.drift.to_bits(),
            0.25_f64.to_bits()
        );
        assert!(
            panel.sea_coverage.is_some_and(|f| f > 0.1 && f < 0.99),
            "resolved fragment fixture should retain both land and sea: {:?}",
            panel.sea_coverage
        );
        assert_ne!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
        app.world_mut().resource_mut::<Panel>().action = Some(Action::Previous);
        app.update();
        activate(&mut app);
        assert_eq!(
            app.world().resource::<Panel>().applied.terrain.source,
            TerrainSource::Noise
        );
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
        assert_eq!(
            app.world().resource::<Panel>().applied.terrain.tectonics,
            original_geology
        );
    }

    fn settings() -> Settings {
        Settings {
            planet: PlanetOptions {
                vertex_count: 64,
                ..PlanetOptions::default()
            },
            terrain: TerrainOptions {
                refinement_passes: 0,
                river_refinement_passes: 0,
                hydraulic_passes: 0,
                thermal_passes: 0,
                ..TerrainOptions::default()
            },
            mode: GenerationMode::Terrain,
        }
    }

    fn app() -> App {
        let settings = settings();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Panel::new(&settings, false))
            .init_resource::<Refresh>()
            .insert_resource(Scene {
                terrain: Arc::new(settings.generate().unwrap()),
                seed: settings.planet.seed,
                generation: settings.mode,
                bed: BedView::Carved,
                river_display: RiverDisplay::Show,
                no_ocean: false,
                wireframe: false,
                smooth: true,
            })
            .add_systems(Update, update_generation);
        app
    }

    fn finish(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().resource::<Panel>().job.is_some() {
            assert!(
                Instant::now() < deadline,
                "background generation must complete"
            );
            thread::sleep(Duration::from_millis(1));
            app.update();
        }
    }

    fn activate(app: &mut App) {
        app.world_mut()
            .resource_scope(|world, mut panel: Mut<Panel>| {
                panel.activate_pending(&mut world.resource_mut::<Scene>());
            });
    }

    fn rendering_app() -> App {
        let mut settings = settings();
        settings.mode = GenerationMode::Sphere;
        settings.terrain.impacts.enabled = true;
        let mut app = app();
        app.insert_resource(Panel::new(&settings, false));
        let mut scene = app.world_mut().resource_mut::<Scene>();
        scene.terrain = Arc::new(settings.generate().unwrap());
        scene.generation = settings.mode;
        scene.no_ocean = true;
        let orbit = initial_orbit(View::Front, &scene.terrain);
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<LodMaterial>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<crate::terrain_material::TerrainTextures>()
            .init_resource::<streaming::ChunkWorld>()
            .init_resource::<streaming::StreamingStats>()
            .insert_resource(streaming::StreamingSettings {
                level: 0,
                forced_lod: Some(3),
                horizon: false,
                ..default()
            })
            .add_systems(
                Update,
                (
                    crate::refresh_scene,
                    streaming::prepare_update,
                    streaming::update,
                )
                    .chain()
                    .after(update_generation),
            );
        app.world_mut().spawn((
            Camera::default(),
            Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
            Projection::Perspective(PerspectiveProjection::default()),
            orbit,
        ));
        app
    }

    fn displayed_positions(app: &mut App) -> BTreeSet<[u32; 3]> {
        let mut query = app
            .world_mut()
            .query_filtered::<&Mesh3d, With<crate::TerrainChunk>>();
        let world = app.world();
        let meshes = world.resource::<Assets<Mesh>>();
        query
            .iter(world)
            .flat_map(|handle| {
                let Some(VertexAttributeValues::Float32x3(positions)) = meshes
                    .get(handle.id())
                    .unwrap()
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("rendered terrain must have positions");
                };
                positions.iter().map(|point| point.map(f32::to_bits))
            })
            .collect()
    }

    fn finish_render(app: &mut App) {
        // A new terrain key also rebuilds the fixture's fallback ocean LODs;
        // cold-cache runs need time for that mesh as well as the tiny terrain.
        let deadline = Instant::now() + Duration::from_mins(1);
        loop {
            app.update();
            if app.world().resource::<streaming::ChunkWorld>().settled()
                && !app.world().resource::<Panel>().busy()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "render preparation must complete: status={}, panel_error={:?}",
                app.world().resource::<streaming::StreamingStats>().status,
                app.world().resource::<Panel>().error,
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn generation_and_previous_switch_collision_source_with_the_rendered_meshes() {
        let mut app = rendering_app();
        finish_render(&mut app);
        let original_positions = displayed_positions(&mut app);
        assert!(!original_positions.is_empty());
        for (seed, action) in [
            (43, Action::Generate),
            (42, Action::Previous),
            (43, Action::Previous),
        ] {
            let old_seed = app.world().resource::<Scene>().seed;
            let old_positions = displayed_positions(&mut app);
            {
                let mut panel = app.world_mut().resource_mut::<Panel>();
                panel.seed = seed.to_string();
                panel.action = Some(action);
            }
            let deadline = Instant::now() + Duration::from_secs(15);
            while app.world().resource::<Scene>().seed == old_seed {
                // Until the renderer can switch, collision keeps the old Scene.
                assert_eq!(displayed_positions(&mut app), old_positions);
                app.update();
                assert!(Instant::now() < deadline, "planet must become displayable");
                thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(app.world().resource::<Scene>().seed, seed);
            assert_ne!(displayed_positions(&mut app), old_positions);
            if seed == 42 {
                // Previous now exercises loading the already baked cache.
                assert_eq!(displayed_positions(&mut app), original_positions);
            }
            assert!(!app.world().resource::<Panel>().busy());
        }
    }

    #[test]
    fn noise_edits_regenerate_render_and_collision_at_the_same_seed_and_previous_restores_them() {
        let mut app = rendering_app();
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            panel.draft.mode = GenerationMode::Terrain;
            panel.action = Some(Action::Generate);
        }
        finish_render(&mut app);
        let original_positions = displayed_positions(&mut app);
        let original = app.world().resource::<Scene>().terrain.elevations.as_ptr();
        app.world_mut()
            .resource_mut::<Panel>()
            .draft
            .terrain
            .noise_layers = NoiseLayer::plates();
        app.update();
        assert_eq!(
            displayed_positions(&mut app),
            original_positions,
            "draft edits changed active rendering"
        );
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
        app.world_mut().resource_mut::<Panel>().action = Some(Action::Generate);
        finish_render(&mut app);
        assert_eq!(app.world().resource::<Scene>().seed, 42);
        assert_ne!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
        assert_ne!(
            displayed_positions(&mut app),
            original_positions,
            "new noise reused stale render data"
        );
        assert_eq!(
            app.world().resource::<Panel>().applied.terrain.noise_layers,
            NoiseLayer::plates()
        );
        app.world_mut().resource_mut::<Panel>().action = Some(Action::Previous);
        finish_render(&mut app);
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
        assert_eq!(displayed_positions(&mut app), original_positions);
        assert_eq!(
            app.world().resource::<Panel>().applied.terrain.noise_layers,
            NoiseLayer::classic()
        );
    }

    #[test]
    fn render_failure_keeps_the_active_planet_and_previous_can_be_retried() {
        let mut app = app();
        let original = app.world().resource::<Scene>().terrain.elevations.as_ptr();
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            panel.seed = String::from("43");
            panel.action = Some(Action::Generate);
        }
        app.update();
        finish(&mut app);
        activate(&mut app);
        let generated = app.world().resource::<Scene>().terrain.elevations.as_ptr();
        app.world_mut().resource_mut::<Panel>().action = Some(Action::Previous);
        app.update();
        app.world_mut()
            .resource_mut::<Panel>()
            .render_failed("cache unavailable");
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            generated
        );
        assert!(!app.world().resource::<Panel>().busy());
        assert!(app.world().resource::<Panel>().error.is_some());
        app.world_mut().resource_mut::<Panel>().action = Some(Action::Previous);
        app.update();
        activate(&mut app);
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
    }

    #[test]
    fn seed_editing_preserves_the_full_u64_and_validates_combined_relief() {
        let mut panel = Panel::new(&settings(), false);
        panel.seed = u64::MAX.to_string();
        assert_eq!(panel.settings().unwrap().planet.seed, u64::MAX);
        panel.seed = String::from("unfinished");
        assert!(panel.settings().is_err());
        panel.seed = String::from("42");
        panel.draft.terrain.relief = 0.4;
        panel.draft.terrain.land_bias = 0.1;
        assert!(panel.settings().is_err());
    }

    #[test]
    fn background_generation_keeps_current_until_ready_and_previous_swaps_buffers() {
        let mut app = app();
        let original = app.world().resource::<Scene>().terrain.elevations.as_ptr();
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            panel.seed = String::from("43");
            panel.action = Some(Action::Generate);
        }
        app.update();
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
        assert!(app.world().resource::<Panel>().job.is_some());
        // Draft edits while generating must not change the captured request.
        app.world_mut().resource_mut::<Panel>().seed = String::from("44");
        finish(&mut app);
        assert_eq!(app.world().resource::<Scene>().seed, 42);
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
        assert!(app.world().resource::<Panel>().busy());
        assert!(app.world().resource::<Refresh>().terrain);
        activate(&mut app);
        assert_eq!(app.world().resource::<Scene>().seed, 43);
        let generated = app.world().resource::<Scene>().terrain.elevations.as_ptr();
        assert_ne!(generated, original);
        assert_eq!(app.world().resource::<Panel>().seed, "44");
        app.world_mut().resource_mut::<Panel>().action = Some(Action::Previous);
        app.update();
        assert_eq!(app.world().resource::<Scene>().seed, 43);
        activate(&mut app);
        assert_eq!(app.world().resource::<Scene>().seed, 42);
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
        app.world_mut().resource_mut::<Panel>().action = Some(Action::Previous);
        app.update();
        assert_eq!(app.world().resource::<Scene>().seed, 42);
        activate(&mut app);
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            generated
        );
    }

    #[test]
    fn failed_background_generation_keeps_the_good_planet_and_reports_the_error() {
        let mut app = app();
        let original = app.world().resource::<Scene>().terrain.elevations.as_ptr();
        let bad_seed = (0..100)
            .find(|&seed| {
                planet_rs::generate(PlanetOptions {
                    vertex_count: 4,
                    seed,
                    ..PlanetOptions::default()
                })
                .is_err()
            })
            .unwrap();
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            panel.seed = bad_seed.to_string();
            panel.draft.planet.vertex_count = 4;
            panel.action = Some(Action::Generate);
        }
        app.update();
        assert!(app.world().resource::<Panel>().job.is_some());
        finish(&mut app);
        assert_eq!(
            app.world().resource::<Scene>().terrain.elevations.as_ptr(),
            original
        );
        assert!(app.world().resource::<Panel>().error.is_some());
        assert!(app.world().resource::<Panel>().previous.is_none());
    }
}
