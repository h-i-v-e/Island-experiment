//! Interactive HUD and camera controls for the tree laboratory.

#![allow(
    clippy::struct_excessive_bools,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use bevy::{
    asset::RenderAssetUsages,
    camera::Exposure,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    image::{CompressedImageFormats, ImageSampler, ImageType},
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    light::{Atmosphere, AtmosphereEnvironmentMapLight},
    prelude::*,
};
use bevy_egui::{
    EguiContexts, EguiPlugin, EguiPrimaryContextPass, EguiTextureHandle, EguiUserTextures, egui,
    input::EguiWantsInput,
};
use island_tree::{BotanicalRecipe, BotanicalSpecies};

use super::{
    RegenerateTree, ReviewCamera, ReviewFrames, ReviewGround, ReviewLight, ReviewLod, ReviewSun,
    ReviewView, Settings, TreeBuildStatus, TreeMetrics, regenerate_tree,
};

const FPS_INTERVAL: f32 = 0.5;
const MARGIN: f32 = 12.0;
const BAR_CLEARANCE: f32 = 76.0;
const BOTTOM_CLEARANCE: f32 = 56.0;
const INSPECTOR_WIDTH: f32 = 318.0;
const DEFAULT_WIND_STRENGTH: f32 = 0.35;
const HERO_CARD_WIDTH: f32 = 188.0;
const SHOWCASE_WIDTH: f32 = HERO_CARD_WIDTH;
const HERO_THUMBNAIL_HEIGHT: f32 = 120.0;
const HERO_CARD_HEIGHT: f32 = HERO_THUMBNAIL_HEIGHT + 18.0;
const CONTROL_CORNER: u8 = 4;
const SHOWCASE_VIEWS: [ReviewView; 3] = [ReviewView::Whole, ReviewView::Crown, ReviewView::Detail];

#[derive(Clone, Copy, Debug, PartialEq)]
struct HeroPreset {
    name: &'static str,
    character: &'static str,
    seed: u64,
    recipe: BotanicalRecipe,
    thumbnail: &'static [u8],
}

impl HeroPreset {
    fn matches(self, studio: &StudioState) -> bool {
        studio.seed == Some(self.seed)
            && studio.recipe == self.recipe
            && studio.lod == ReviewLod::Near
            && studio.foliage
            && studio.fine_shoots
    }

    fn apply(self, studio: &mut StudioState) {
        studio.seed_text = self.seed.to_string();
        studio.seed = Some(self.seed);
        studio.recipe = self.recipe;
        studio.lod = ReviewLod::Near;
        studio.foliage = true;
        studio.fine_shoots = true;
    }
}

const HERO_PRESETS: [HeroPreset; 3] = [
    HeroPreset {
        name: "Pōhutukawa",
        character: "Broad, wind-shaped coastal canopy · seed 42",
        seed: 42,
        recipe: BotanicalRecipe::for_species(BotanicalSpecies::Pohutukawa),
        thumbnail: include_bytes!("../assets/showcase/pohutukawa.png"),
    },
    HeroPreset {
        name: "Nīkau",
        character: "Layered native palm crown · seed 42",
        seed: 42,
        recipe: BotanicalRecipe::for_species(BotanicalSpecies::Nikau),
        thumbnail: include_bytes!("../assets/showcase/nikau.png"),
    },
    HeroPreset {
        name: "Harakeke",
        character: "Mature overlapping flax fans · seed 42",
        seed: 42,
        recipe: BotanicalRecipe::for_species(BotanicalSpecies::Harakeke),
        thumbnail: include_bytes!("../assets/showcase/harakeke.png"),
    },
];

#[derive(Resource)]
struct HeroThumbnails {
    entries: [Option<(egui::TextureId, Handle<Image>)>; HERO_PRESETS.len()],
}

type SunQuery<'world, 'state> = Query<
    'world,
    'state,
    (&'static mut DirectionalLight, &'static mut Transform),
    (With<ReviewSun>, Without<ReviewCamera>),
>;

#[derive(Clone, Copy, Debug, PartialEq)]
struct DraftFingerprint {
    seed: u64,
    recipe: BotanicalRecipe,
    lod: ReviewLod,
    foliage: bool,
    fine_shoots: bool,
}

const PANEL: egui::Color32 = egui::Color32::from_rgb(7, 14, 21);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(15, 27, 37);
const RAISED: egui::Color32 = egui::Color32::from_rgb(25, 42, 55);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(76, 196, 228);
const TEXT: egui::Color32 = egui::Color32::from_rgb(226, 234, 241);
const DIM_TEXT: egui::Color32 = egui::Color32::from_rgb(126, 148, 163);
const FAILURE: egui::Color32 = egui::Color32::from_rgb(230, 120, 90);

pub(super) struct TreeStudioPlugin;

impl Plugin for TreeStudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            EguiPlugin {
                bindless_mode_array_size: None,
                ..default()
            },
            FrameTimeDiagnosticsPlugin::default(),
        ))
        .add_message::<RegenerateTree>()
        .add_systems(Startup, (install, load_hero_thumbnails))
        .add_systems(
            Update,
            (
                regenerate_tree,
                animate_wind,
                inspect_camera,
                toggle_hud,
                read_frame_rate,
                fade_generation_notice,
            ),
        )
        .add_systems(EguiPrimaryContextPass, (apply_theme, draw_hud).chain());
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StudioTab {
    #[default]
    Form,
    Branching,
    Foliage,
    Lighting,
    Biome,
}

impl StudioTab {
    const ALL: [Self; 5] = [
        Self::Form,
        Self::Branching,
        Self::Foliage,
        Self::Lighting,
        Self::Biome,
    ];

    const fn title(self) -> &'static str {
        match self {
            Self::Form => "FORM",
            Self::Branching => "BRANCH",
            Self::Foliage => "FOLIAGE",
            Self::Lighting => "LIGHT",
            Self::Biome => "BIOME",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LightTone {
    #[default]
    Neutral,
    Warm,
    Cool,
}

impl LightTone {
    const ALL: [Self; 3] = [Self::Neutral, Self::Warm, Self::Cool];

    const fn label(self) -> &'static str {
        match self {
            Self::Neutral => "Neutral daylight",
            Self::Warm => "Warm coastal sun",
            Self::Cool => "Cool overcast",
        }
    }

    fn colour(self) -> Color {
        match self {
            Self::Neutral => Color::WHITE,
            Self::Warm => Color::srgb(1.0, 0.82, 0.66),
            Self::Cool => Color::srgb(0.76, 0.86, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BiomeLook {
    #[default]
    CoastalHeadland,
    TemperateGrove,
    DryRidge,
}

impl BiomeLook {
    const ALL: [Self; 3] = [Self::CoastalHeadland, Self::TemperateGrove, Self::DryRidge];

    const fn label(self) -> &'static str {
        match self {
            Self::CoastalHeadland => "Coastal headland",
            Self::TemperateGrove => "Temperate grove",
            Self::DryRidge => "Wind-scoured ridge",
        }
    }

    fn ground_colour(self) -> Color {
        match self {
            Self::CoastalHeadland => Color::srgb(0.21, 0.18, 0.12),
            Self::TemperateGrove => Color::srgb(0.12, 0.19, 0.10),
            Self::DryRidge => Color::srgb(0.31, 0.24, 0.15),
        }
    }

    const fn sky_ground_albedo(self) -> f32 {
        match self {
            Self::CoastalHeadland => 0.10,
            Self::TemperateGrove => 0.07,
            Self::DryRidge => 0.16,
        }
    }
}

#[derive(Resource)]
struct StudioState {
    visible: bool,
    inspection_open: bool,
    tab: StudioTab,
    seed_text: String,
    seed: Option<u64>,
    recipe: BotanicalRecipe,
    lod: ReviewLod,
    foliage: bool,
    fine_shoots: bool,
    animate_wind: bool,
    wind_speed: f32,
    fps: f64,
    next_fps_reading: f32,
    light_tone: LightTone,
    sun_illuminance: f32,
    biome: BiomeLook,
    ground_moisture: f32,
    exposure_compensation: f32,
    sky_fill: f32,
    last_requested: Option<DraftFingerprint>,
}

impl StudioState {
    fn new(settings: &Settings) -> Self {
        Self {
            visible: true,
            inspection_open: true,
            tab: StudioTab::Form,
            seed_text: settings.seed.to_string(),
            seed: Some(settings.seed),
            recipe: settings.recipe,
            lod: settings.lod,
            foliage: settings.foliage,
            fine_shoots: settings.fine_shoots,
            animate_wind: settings.wind_strength > 0.0,
            wind_speed: 0.12,
            fps: 0.0,
            next_fps_reading: 0.0,
            light_tone: LightTone::Neutral,
            sun_illuminance: 92_000.0,
            biome: BiomeLook::CoastalHeadland,
            ground_moisture: 0.18,
            exposure_compensation: 2.4,
            sky_fill: 1.8,
            last_requested: None,
        }
    }

    fn reparse_seed(&mut self) {
        self.seed = self.seed_text.parse().ok();
    }

    fn is_dirty(&self, settings: &Settings) -> bool {
        self.seed != Some(settings.seed)
            || self.recipe != settings.recipe
            || self.lod != settings.lod
            || self.foliage != settings.foliage
            || self.fine_shoots != settings.fine_shoots
    }

    fn request(&self, live: &Settings) -> Option<RegenerateTree> {
        Some(RegenerateTree(Settings {
            seed: self.seed?,
            recipe: self.recipe,
            lod: self.lod,
            view: live.view,
            light: live.light,
            foliage: self.foliage,
            fine_shoots: self.fine_shoots,
            wind_phase: live.wind_phase,
            wind_strength: live.wind_strength,
            screenshot: None,
            capture_ui: false,
        }))
    }

    fn fingerprint(&self) -> Option<DraftFingerprint> {
        Some(DraftFingerprint {
            seed: self.seed?,
            recipe: self.recipe,
            lod: self.lod,
            foliage: self.foliage,
            fine_shoots: self.fine_shoots,
        })
    }

    fn next_request(&mut self, live: &Settings) -> Option<RegenerateTree> {
        if !self.is_dirty(live) {
            return None;
        }
        let fingerprint = self.fingerprint()?;
        if self.last_requested == Some(fingerprint) {
            return None;
        }
        self.last_requested = Some(fingerprint);
        self.request(live)
    }

    fn request_now(&mut self, live: &Settings) -> Option<RegenerateTree> {
        let fingerprint = self.fingerprint()?;
        self.last_requested = Some(fingerprint);
        self.request(live)
    }

    fn randomize_seed(&mut self) {
        let mut value = self.seed.unwrap_or(42).wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        self.seed_text = value.to_string();
        self.seed = Some(value);
    }

    fn set_wind_animation(&mut self, settings: &mut Settings, enabled: bool) {
        self.animate_wind = enabled;
        if enabled && settings.wind_strength <= f32::EPSILON {
            settings.wind_strength = DEFAULT_WIND_STRENGTH;
        }
    }
}

fn request_tree_rebuild(
    studio: &mut StudioState,
    settings: &Settings,
    status: &mut TreeBuildStatus,
    requests: &mut MessageWriter<RegenerateTree>,
) {
    if let Some(request) = studio.request_now(settings) {
        status.error = None;
        status.generating = true;
        status.notice_seconds = 0.0;
        requests.write(request);
    }
}

fn install(mut commands: Commands, settings: Res<Settings>) {
    commands.insert_resource(StudioState::new(&settings));
}

fn load_hero_thumbnails(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut textures: ResMut<EguiUserTextures>,
) {
    let entries = HERO_PRESETS.map(|preset| {
        let image = Image::from_buffer(
            preset.thumbnail,
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::linear(),
            RenderAssetUsages::RENDER_WORLD,
        )
        .ok()?;
        let handle = images.add(image);
        let id = textures.add_image(EguiTextureHandle::Strong(handle.clone()));
        Some((id, handle))
    });
    commands.insert_resource(HeroThumbnails { entries });
}

fn apply_theme(mut contexts: EguiContexts, mut installed: Local<bool>) {
    if *installed {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    context.set_theme(egui::ThemePreference::Dark);
    context.all_styles_mut(|style| {
        style.visuals.dark_mode = true;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.panel_fill = translucent(PANEL, 224);
        style.visuals.window_fill = translucent(PANEL, 224);
        style.visuals.window_stroke = hairline();
        style.visuals.window_corner_radius = egui::CornerRadius::same(8);
        style.visuals.window_shadow = egui::Shadow::NONE;
        style.visuals.selection.bg_fill = translucent(ACCENT, 130);
        style.visuals.slider_trailing_fill = true;
        style.visuals.widgets.inactive.bg_fill = translucent(SURFACE, 220);
        style.visuals.widgets.inactive.weak_bg_fill = translucent(SURFACE, 220);
        style.visuals.widgets.hovered.bg_fill = translucent(RAISED, 235);
        style.visuals.widgets.hovered.weak_bg_fill = translucent(RAISED, 235);
        style.visuals.widgets.active.bg_fill = translucent(ACCENT, 190);
        style.visuals.widgets.active.weak_bg_fill = translucent(ACCENT, 190);
        for widget in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(4);
        }
        style.spacing.item_spacing = egui::vec2(7.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.slider_width = 176.0;
    });
    *installed = true;
}

fn draw_hud(
    mut contexts: EguiContexts,
    mut studio: ResMut<StudioState>,
    mut settings: ResMut<Settings>,
    frames: Res<ReviewFrames>,
    thumbnails: Res<HeroThumbnails>,
    metrics: Res<TreeMetrics>,
    mut status: ResMut<TreeBuildStatus>,
    mut cameras: Query<(&mut ReviewCamera, &mut Transform), Without<ReviewSun>>,
    mut suns: SunQuery,
    mut exposures: Query<&mut Exposure, With<ReviewCamera>>,
    mut sky_lights: Query<&mut AtmosphereEnvironmentMapLight, With<ReviewCamera>>,
    grounds: Query<&MeshMaterial3d<StandardMaterial>, With<ReviewGround>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut atmospheres: Query<&mut Atmosphere>,
    mut requests: MessageWriter<RegenerateTree>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    draw_telemetry(context, &studio, &metrics);
    if !studio.visible {
        egui::Area::new("open tree studio".into())
            .anchor(egui::Align2::LEFT_TOP, [MARGIN, MARGIN])
            .show(context, |ui| {
                panel_frame(196)
                    .inner_margin(egui::Margin::symmetric(5, 2))
                    .show(ui, |ui| {
                        if ui.button("☰  TREE STUDIO").clicked() {
                            studio.visible = true;
                        }
                    });
            });
        draw_generation_strip(context, &studio, &status, &metrics);
        return;
    }

    draw_command_bar(
        context,
        &mut studio,
        &mut settings,
        &frames,
        &mut cameras,
        &mut status,
        &mut requests,
    );
    draw_inspection_selector(
        context,
        &mut studio,
        &mut settings,
        &frames,
        &thumbnails,
        &mut status,
        &mut cameras,
        &mut requests,
    );
    draw_inspector(
        context,
        &mut studio,
        &mut settings,
        &metrics,
        &status,
        &mut suns,
        &mut exposures,
        &mut sky_lights,
        &grounds,
        &mut materials,
        &mut atmospheres,
    );
    draw_controls(context);
    let pointer_down = context.input(|input| input.pointer.primary_down());
    if !pointer_down && let Some(request) = studio.next_request(&settings) {
        status.error = None;
        status.generating = true;
        status.notice_seconds = 0.0;
        requests.write(request);
    }
    draw_generation_strip(context, &studio, &status, &metrics);
}

fn draw_command_bar(
    context: &egui::Context,
    studio: &mut StudioState,
    settings: &mut Settings,
    frames: &ReviewFrames,
    cameras: &mut Query<(&mut ReviewCamera, &mut Transform), Without<ReviewSun>>,
    status: &mut TreeBuildStatus,
    requests: &mut MessageWriter<RegenerateTree>,
) {
    let width = (context.content_rect().width() - 24.0).max(420.0);
    egui::Window::new("tree studio command bar")
        .anchor(egui::Align2::LEFT_TOP, [12.0, 12.0])
        .title_bar(false)
        .fixed_size([width, 52.0])
        .collapsible(false)
        .frame(panel_frame(238))
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.label(caps("ISLAND TREE STUDIO", 15.0, TEXT).strong());
                ui.separator();
                ui.label(caps(
                    settings.recipe.species.scientific_name(),
                    11.0,
                    ACCENT,
                ));
                if status.error.is_none() {
                    ui.label(caps("LIVE · AUTO", 10.0, DIM_TEXT));
                }
                if let Some(error) = &status.error {
                    ui.colored_label(FAILURE, error);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Reset").clicked() {
                        let leaves_isolated_frond = settings.view == ReviewView::Frond;
                        studio.recipe = BotanicalRecipe::for_species(studio.recipe.species);
                        studio.lod = ReviewLod::Near;
                        studio.foliage = true;
                        studio.fine_shoots = true;
                        settings.view = ReviewView::Whole;
                        let frame = frames.get(settings.view);
                        if let Ok((mut camera, mut transform)) = cameras.single_mut() {
                            camera.target = frame.target;
                            *transform = frame.transform;
                        }
                        if leaves_isolated_frond {
                            request_tree_rebuild(studio, settings, status, requests);
                        }
                    }
                });
            });
        });
}

fn draw_inspection_selector(
    context: &egui::Context,
    studio: &mut StudioState,
    settings: &mut Settings,
    frames: &ReviewFrames,
    thumbnails: &HeroThumbnails,
    status: &mut TreeBuildStatus,
    cameras: &mut Query<(&mut ReviewCamera, &mut Transform), Without<ReviewSun>>,
    requests: &mut MessageWriter<RegenerateTree>,
) {
    let body_height =
        (context.content_rect().height() - BAR_CLEARANCE - BOTTOM_CLEARANCE - 72.0).max(160.0);
    egui::Window::new("tree showcase")
        .anchor(egui::Align2::LEFT_TOP, [MARGIN, BAR_CLEARANCE])
        .default_width(SHOWCASE_WIDTH)
        .max_width(SHOWCASE_WIDTH)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .frame(panel_frame(212))
        .show(context, |ui| {
            ui.set_width(SHOWCASE_WIDTH);
            ui.horizontal(|ui| {
                ui.label(caps("TREE SHOWCASE", 11.0, TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let chevron = if studio.inspection_open { "⏷" } else { "⏵" };
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(chevron).size(10.0)).frame(false),
                        )
                        .clicked()
                    {
                        studio.inspection_open = !studio.inspection_open;
                    }
                });
            });
            if studio.inspection_open {
                ui.add_space(5.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .show(ui, |ui| {
                        for (index, preset) in HERO_PRESETS.into_iter().enumerate() {
                            let thumbnail = thumbnails.entries[index]
                                .as_ref()
                                .map(|(texture, _)| *texture);
                            if hero_card(ui, preset, thumbnail, preset.matches(studio)) {
                                let leaves_isolated_frond = settings.view == ReviewView::Frond
                                    && settings.recipe.species == BotanicalSpecies::Nikau;
                                preset.apply(studio);
                                set_camera_view(ReviewView::Whole, settings, frames, cameras);
                                if leaves_isolated_frond {
                                    request_tree_rebuild(studio, settings, status, requests);
                                }
                            }
                            ui.add_space(6.0);
                        }

                        ui.separator();
                        ui.label(caps("CAMERA", 9.5, DIM_TEXT));
                        ui.horizontal(|ui| {
                            for view in SHOWCASE_VIEWS {
                                let selected = settings.view == view;
                                let colour = if selected { PANEL } else { TEXT };
                                let mut button = egui::Button::new(
                                    egui::RichText::new(view.label()).size(9.0).color(colour),
                                );
                                if selected {
                                    button = button.fill(ACCENT);
                                }
                                if ui.add_sized([58.0, 24.0], button).clicked() {
                                    let leaves_isolated_frond = settings.view == ReviewView::Frond
                                        && settings.recipe.species == BotanicalSpecies::Nikau;
                                    set_camera_view(view, settings, frames, cameras);
                                    if leaves_isolated_frond {
                                        request_tree_rebuild(studio, settings, status, requests);
                                    }
                                }
                            }
                        });
                    });
                ui.add_space(2.0);
                ui.separator();
                ui.label(caps("3 HERO SPECIMENS · 3 CAMERA VIEWS", 8.5, DIM_TEXT));
            }
        });
}

fn hero_card(
    ui: &mut egui::Ui,
    preset: HeroPreset,
    thumbnail: Option<egui::TextureId>,
    selected: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(HERO_CARD_WIDTH, HERO_CARD_HEIGHT),
        egui::Sense::click(),
    );
    let image_rect =
        egui::Rect::from_min_size(rect.min, egui::vec2(HERO_CARD_WIDTH, HERO_THUMBNAIL_HEIGHT));
    if let Some(texture) = thumbnail {
        egui::Image::new((texture, image_rect.size()))
            .corner_radius(CONTROL_CORNER)
            .paint_at(ui, image_rect);
    }
    let painter = ui.painter();
    let stroke = if selected {
        egui::Stroke::new(1.5, ACCENT)
    } else if response.hovered() {
        egui::Stroke::new(1.0, translucent(ACCENT, 170))
    } else {
        hairline()
    };
    painter.rect_stroke(
        image_rect,
        CONTROL_CORNER,
        stroke,
        egui::StrokeKind::Outside,
    );
    if selected {
        let badge = image_rect.right_top() + egui::vec2(-10.0, 10.0);
        painter.circle_filled(badge, 7.0, ACCENT);
        painter.text(
            badge,
            egui::Align2::CENTER_CENTER,
            "✔",
            egui::FontId::proportional(9.0),
            PANEL,
        );
    }
    painter.text(
        rect.center_bottom(),
        egui::Align2::CENTER_BOTTOM,
        preset.name.to_uppercase(),
        egui::FontId::proportional(9.0),
        if selected { TEXT } else { DIM_TEXT },
    );
    response.on_hover_text(preset.character).clicked()
}

fn set_camera_view(
    view: ReviewView,
    settings: &mut Settings,
    frames: &ReviewFrames,
    cameras: &mut Query<(&mut ReviewCamera, &mut Transform), Without<ReviewSun>>,
) {
    settings.view = view;
    let frame = frames.get(view);
    if let Ok((mut camera, mut transform)) = cameras.single_mut() {
        camera.target = frame.target;
        *transform = frame.transform;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_inspector(
    context: &egui::Context,
    studio: &mut StudioState,
    settings: &mut Settings,
    metrics: &TreeMetrics,
    status: &TreeBuildStatus,
    suns: &mut SunQuery,
    exposures: &mut Query<&mut Exposure, With<ReviewCamera>>,
    sky_lights: &mut Query<&mut AtmosphereEnvironmentMapLight, With<ReviewCamera>>,
    grounds: &Query<&MeshMaterial3d<StandardMaterial>, With<ReviewGround>>,
    materials: &mut Assets<StandardMaterial>,
    atmospheres: &mut Query<&mut Atmosphere>,
) {
    let height = (context.content_rect().height() - BAR_CLEARANCE - BOTTOM_CLEARANCE).max(320.0);
    let body_height = (height - 90.0).max(200.0);
    egui::Window::new("tree studio inspector")
        .anchor(egui::Align2::RIGHT_TOP, [-MARGIN, BAR_CLEARANCE])
        .title_bar(false)
        .fixed_size([INSPECTOR_WIDTH + 24.0, height])
        .min_width(INSPECTOR_WIDTH + 24.0)
        .max_width(INSPECTOR_WIDTH + 24.0)
        .resizable(false)
        .collapsible(false)
        .frame(panel_frame(224))
        .show(context, |ui| {
            ui.set_min_width(INSPECTOR_WIDTH);
            ui.set_max_width(INSPECTOR_WIDTH);
            ui.label(caps("TREE PARAMETERS", 11.0, TEXT));
            ui.add_space(2.0);
            draw_tabs(ui, &mut studio.tab);
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(body_height)
                .show(ui, |ui| match studio.tab {
                    StudioTab::Form => draw_form(ui, studio),
                    StudioTab::Branching => draw_branching(ui, studio),
                    StudioTab::Foliage => draw_foliage(ui, studio, settings),
                    StudioTab::Lighting => {
                        draw_lighting(ui, studio, settings, suns, exposures, sky_lights);
                    }
                    StudioTab::Biome => {
                        draw_biome(ui, studio, grounds, materials, atmospheres);
                    }
                });
            ui.add_space(4.0);
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "seed {} · {} axes · generated in {} ms",
                    settings.seed, metrics.axes, metrics.generation_millis
                ))
                .small()
                .color(DIM_TEXT),
            );
            if let Some(error) = &status.error {
                ui.colored_label(FAILURE, error);
            }
        });
}

fn draw_tabs(ui: &mut egui::Ui, tab: &mut StudioTab) {
    let width = (ui.available_width() - ui.spacing().item_spacing.x * 4.0) / 5.0;
    ui.horizontal(|ui| {
        for candidate in StudioTab::ALL {
            let selected = *tab == candidate;
            let colour = if selected { TEXT } else { DIM_TEXT };
            let response = ui.add_sized(
                [width, 20.0],
                egui::Button::new(caps(candidate.title(), 8.5, colour)).frame(false),
            );
            if selected {
                ui.painter().hline(
                    response.rect.x_range().shrink(5.0),
                    response.rect.bottom() - 1.0,
                    egui::Stroke::new(2.0, ACCENT),
                );
            }
            if response.clicked() {
                *tab = candidate;
            }
        }
    });
}

fn draw_form(ui: &mut egui::Ui, studio: &mut StudioState) {
    section(ui, "PLANT FAMILY");
    let previous_species = studio.recipe.species;
    egui::ComboBox::from_id_salt("plant family")
        .selected_text(studio.recipe.species.label())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for species in BotanicalSpecies::ALL {
                ui.selectable_value(&mut studio.recipe.species, species, species.label());
            }
        });
    if studio.recipe.species != previous_species {
        studio.recipe = BotanicalRecipe::for_species(studio.recipe.species);
    }

    section(ui, "SEED");
    ui.horizontal(|ui| {
        let changed = ui
            .add_sized(
                [184.0, 20.0],
                egui::TextEdit::singleline(&mut studio.seed_text).font(egui::TextStyle::Monospace),
            )
            .changed();
        if changed {
            studio.reparse_seed();
        }
        if ui
            .add_sized(
                [ui.available_width(), 20.0],
                egui::Button::new(caps("RANDOMIZE", 8.5, TEXT)),
            )
            .clicked()
        {
            studio.randomize_seed();
        }
    });
    if studio.seed.is_none() {
        ui.colored_label(FAILURE, "Enter an unsigned whole-number seed");
    }

    section(ui, "PROPORTIONS");
    let (height_range, radius_range) = match studio.recipe.species {
        BotanicalSpecies::Pohutukawa => (5.0..=14.0, 0.25..=1.35),
        BotanicalSpecies::Nikau => (4.5..=10.0, 0.14..=0.34),
        BotanicalSpecies::Harakeke => (1.2..=3.0, 0.20..=0.55),
    };
    slider_f32(
        ui,
        &mut studio.recipe.trunk_height_metres,
        height_range,
        match studio.recipe.species {
            BotanicalSpecies::Harakeke => "Plant height",
            BotanicalSpecies::Pohutukawa | BotanicalSpecies::Nikau => "Trunk height",
        },
        " m",
    );
    let radius_max = (studio.recipe.trunk_height_metres * 0.19).min(*radius_range.end());
    studio.recipe.trunk_radius_metres = studio.recipe.trunk_radius_metres.min(radius_max);
    slider_f32(
        ui,
        &mut studio.recipe.trunk_radius_metres,
        *radius_range.start()..=radius_max,
        match studio.recipe.species {
            BotanicalSpecies::Harakeke => "Clump radius",
            BotanicalSpecies::Pohutukawa | BotanicalSpecies::Nikau => "Trunk radius",
        },
        " m",
    );

    section(ui, "LEVEL OF DETAIL");
    egui::ComboBox::from_id_salt("tree lod")
        .selected_text(studio.lod.label())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for lod in ReviewLod::ALL {
                ui.selectable_value(&mut studio.lod, lod, lod.label());
            }
        });
    ui.label(
        egui::RichText::new(
            "LOD 0 uses full leaves; LOD 1 uses canopy pads; LOD 2 uses an eight-vertex generated image impostor.",
        )
        .small()
        .color(DIM_TEXT),
    );
}

fn draw_branching(ui: &mut egui::Ui, studio: &mut StudioState) {
    match studio.recipe.species {
        BotanicalSpecies::Pohutukawa => {
            section(ui, "CROWN STRUCTURE");
            ui.add(
                egui::Slider::new(&mut studio.recipe.primary_count, 5..=10).text("Primary limbs"),
            );
            ui.add(
                egui::Slider::new(&mut studio.recipe.secondaries_per_primary, 3..=8)
                    .text("Secondary limbs"),
            );
            ui.add(
                egui::Slider::new(&mut studio.recipe.terminals_per_secondary, 3..=8)
                    .text("Terminal shoots"),
            );
        }
        BotanicalSpecies::Nikau => {
            section(ui, "FROND CROWN");
            ui.add(
                egui::Slider::new(&mut studio.recipe.primary_count, 12..=24).text("Live fronds"),
            );
            ui.label(
                egui::RichText::new(
                    "Fronds follow a deterministic age spiral: young spears rise through the centre while older leaves arch around the crownshaft.",
                )
                .small()
                .color(DIM_TEXT),
            );
        }
        BotanicalSpecies::Harakeke => {
            section(ui, "CLUMP STRUCTURE");
            ui.add(egui::Slider::new(&mut studio.recipe.primary_count, 4..=16).text("Leaf fans"));
            ui.label(
                egui::RichText::new(
                    "Each fan grows from a basal rhizome; overlapping fan planes build the mature clump rather than a radial grass tuft.",
                )
                .small()
                .color(DIM_TEXT),
            );
        }
    }
}

fn draw_foliage(ui: &mut egui::Ui, studio: &mut StudioState, settings: &mut Settings) {
    section(ui, "LEAF LOAD");
    let (leaf_range, leaf_label) = match studio.recipe.species {
        BotanicalSpecies::Pohutukawa => (8..=64, "Leaves per terminal"),
        BotanicalSpecies::Nikau => (8..=64, "Leaflet pairs per frond"),
        BotanicalSpecies::Harakeke => (9..=18, "Leaves per fan"),
    };
    ui.add(egui::Slider::new(&mut studio.recipe.leaves_per_terminal, leaf_range).text(leaf_label));
    ui.checkbox(&mut studio.foliage, "Show foliage");
    ui.add_enabled_ui(studio.lod == ReviewLod::Near, |ui| {
        ui.checkbox(&mut studio.fine_shoots, "Fine shoots and buds");
    });

    section(ui, "WIND REVIEW");
    let mut animate = studio.animate_wind;
    if ui.checkbox(&mut animate, "Animate wind").changed() {
        studio.set_wind_animation(settings, animate);
    }
    slider_f32(ui, &mut settings.wind_strength, 0.0..=1.0, "Strength", "");
    slider_f32(ui, &mut studio.wind_speed, 0.02..=0.40, "Cycle speed", "");
    ui.add_enabled_ui(!studio.animate_wind, |ui| {
        slider_f32(ui, &mut settings.wind_phase, 0.0..=1.0, "Cycle phase", "");
    });
}

fn draw_lighting(
    ui: &mut egui::Ui,
    studio: &mut StudioState,
    settings: &mut Settings,
    suns: &mut SunQuery,
    exposures: &mut Query<&mut Exposure, With<ReviewCamera>>,
    sky_lights: &mut Query<&mut AtmosphereEnvironmentMapLight, With<ReviewCamera>>,
) {
    let mut changed = false;
    section(ui, "SUN DIRECTION");
    ui.horizontal_wrapped(|ui| {
        for light in ReviewLight::ALL {
            changed |= ui
                .selectable_value(&mut settings.light, light, light.label())
                .changed();
        }
    });

    section(ui, "LIGHT CHARACTER");
    let previous_tone = studio.light_tone;
    egui::ComboBox::from_id_salt("light tone")
        .selected_text(studio.light_tone.label())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for tone in LightTone::ALL {
                ui.selectable_value(&mut studio.light_tone, tone, tone.label());
            }
        });
    changed |= studio.light_tone != previous_tone;
    changed |= ui
        .add(
            egui::Slider::new(&mut studio.sun_illuminance, 20_000.0..=160_000.0)
                .text("Sun intensity")
                .suffix(" lux")
                .logarithmic(true)
                .fixed_decimals(0),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut studio.exposure_compensation, 0.5..=4.0)
                .text("Exposure")
                .suffix(" stops")
                .fixed_decimals(2),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut studio.sky_fill, 0.5..=3.0)
                .text("Sky fill")
                .fixed_decimals(2),
        )
        .changed();
    ui.label(
        egui::RichText::new(
            "Exposure sets overall brightness; sky fill opens the shaded crown without clipping sunlit bark.",
        )
        .small()
        .color(DIM_TEXT),
    );
    if changed {
        apply_lighting_preview(studio, settings, suns, exposures, sky_lights);
    }
}

fn apply_lighting_preview(
    studio: &StudioState,
    settings: &Settings,
    suns: &mut SunQuery,
    exposures: &mut Query<&mut Exposure, With<ReviewCamera>>,
    sky_lights: &mut Query<&mut AtmosphereEnvironmentMapLight, With<ReviewCamera>>,
) {
    if let Ok((mut sun, mut transform)) = suns.single_mut() {
        sun.color = studio.light_tone.colour();
        sun.illuminance = studio.sun_illuminance;
        *transform = Transform::default().looking_to(settings.light.direction(), Vec3::Y);
    }
    if let Ok(mut exposure) = exposures.single_mut() {
        exposure.ev100 = Exposure::EV100_SUNLIGHT - studio.exposure_compensation;
    }
    if let Ok(mut sky_light) = sky_lights.single_mut() {
        sky_light.intensity = studio.sky_fill;
    }
}

fn draw_biome(
    ui: &mut egui::Ui,
    studio: &mut StudioState,
    grounds: &Query<&MeshMaterial3d<StandardMaterial>, With<ReviewGround>>,
    materials: &mut Assets<StandardMaterial>,
    atmospheres: &mut Query<&mut Atmosphere>,
) {
    section(ui, "REVIEW HABITAT");
    let previous_biome = studio.biome;
    egui::ComboBox::from_id_salt("biome look")
        .selected_text(studio.biome.label())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for biome in BiomeLook::ALL {
                ui.selectable_value(&mut studio.biome, biome, biome.label());
            }
        });
    let mut changed = studio.biome != previous_biome;
    changed |= ui
        .add(
            egui::Slider::new(&mut studio.ground_moisture, 0.0..=1.0)
                .text("Ground moisture")
                .fixed_decimals(2),
        )
        .changed();
    ui.label(
        egui::RichText::new(
            "A neutral review habitat changes ground response and sky bounce, not the tree recipe.",
        )
        .small()
        .color(DIM_TEXT),
    );
    if changed {
        apply_biome_preview(studio, grounds, materials, atmospheres);
    }
}

fn apply_biome_preview(
    studio: &StudioState,
    grounds: &Query<&MeshMaterial3d<StandardMaterial>, With<ReviewGround>>,
    materials: &mut Assets<StandardMaterial>,
    atmospheres: &mut Query<&mut Atmosphere>,
) {
    if let Ok(ground) = grounds.single()
        && let Some(mut material) = materials.get_mut(&ground.0)
    {
        material.base_color = studio.biome.ground_colour();
        material.perceptual_roughness = 0.98 - studio.ground_moisture * 0.34;
        material.reflectance = 0.015 + studio.ground_moisture * 0.055;
    }
    if let Ok(mut atmosphere) = atmospheres.single_mut() {
        atmosphere.ground_albedo = Vec3::splat(studio.biome.sky_ground_albedo());
    }
}

fn slider_f32(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
    suffix: &str,
) {
    ui.add(
        egui::Slider::new(value, range)
            .text(label)
            .suffix(suffix)
            .fixed_decimals(2),
    );
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.separator();
    ui.label(caps(title, 10.5, ACCENT));
}

fn draw_generation_strip(
    context: &egui::Context,
    studio: &StudioState,
    status: &TreeBuildStatus,
    metrics: &TreeMetrics,
) {
    let (text, colour, spinning) = if status.generating {
        (String::from("GENERATING TREE"), TEXT, true)
    } else if let Some(error) = &status.error {
        (format!("GENERATION FAILED · {error}"), FAILURE, false)
    } else if status.notice_seconds > 0.0 {
        (
            format!("TREE UPDATED · {} MS", metrics.generation_millis),
            TEXT,
            false,
        )
    } else {
        return;
    };
    let top = if studio.visible {
        BAR_CLEARANCE
    } else {
        MARGIN
    };
    egui::Area::new("tree generation strip".into())
        .anchor(egui::Align2::CENTER_TOP, [0.0, top])
        .movable(false)
        .interactable(false)
        .show(context, |ui| {
            panel_frame(196)
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if spinning {
                            ui.add(egui::Spinner::new().size(12.0).color(ACCENT));
                        }
                        ui.add(
                            egui::Label::new(caps(&text, 10.0, colour))
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                });
        });
}

fn draw_telemetry(context: &egui::Context, studio: &StudioState, metrics: &TreeMetrics) {
    let triangles = metrics.wood_triangles + metrics.scar_triangles;
    let line = format!(
        "{:.0} FPS  ·  {} AXES  ·  {} LEAVES  ·  {}K TRIS",
        studio.fps,
        metrics.axes,
        metrics.leaves,
        (triangles + 500) / 1_000,
    );
    egui::Area::new("tree telemetry".into())
        .anchor(egui::Align2::LEFT_BOTTOM, [MARGIN, -MARGIN])
        .movable(false)
        .interactable(false)
        .show(context, |ui| {
            panel_frame(196)
                .inner_margin(egui::Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(caps(&line, 9.5, DIM_TEXT))
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
}

fn draw_controls(context: &egui::Context) {
    if context.content_rect().width() < 820.0 {
        return;
    }
    let controls = [
        ("RMB", "ORBIT"),
        ("MMB", "PAN"),
        ("WHEEL", "DOLLY"),
        ("H", "HIDE HUD"),
    ];
    egui::Area::new("tree controls".into())
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -MARGIN])
        .movable(false)
        .interactable(false)
        .show(context, |ui| {
            panel_frame(196)
                .inner_margin(egui::Margin::symmetric(12, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        for (index, (key, verb)) in controls.into_iter().enumerate() {
                            if index > 0 {
                                ui.label(caps("·", 9.5, translucent(DIM_TEXT, 140)));
                            }
                            ui.label(caps(key, 9.5, TEXT));
                            ui.label(caps(verb, 9.5, DIM_TEXT));
                        }
                    });
                });
        });
}

fn read_frame_rate(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    mut studio: ResMut<StudioState>,
) {
    studio.next_fps_reading -= time.delta_secs();
    if studio.next_fps_reading > 0.0 {
        return;
    }
    studio.next_fps_reading = FPS_INTERVAL;
    if let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed)
    {
        studio.fps = fps;
    }
}

fn fade_generation_notice(time: Res<Time>, mut status: ResMut<TreeBuildStatus>) {
    if !status.generating && status.notice_seconds > 0.0 {
        status.notice_seconds = (status.notice_seconds - time.delta_secs()).max(0.0);
    }
}

fn animate_wind(time: Res<Time>, studio: Res<StudioState>, mut settings: ResMut<Settings>) {
    if !studio.animate_wind || settings.wind_strength <= f32::EPSILON {
        return;
    }
    settings.wind_phase =
        (settings.wind_phase + time.delta_secs() * studio.wind_speed).rem_euclid(1.0);
}

fn toggle_hud(
    keys: Res<ButtonInput<KeyCode>>,
    wants_input: Res<EguiWantsInput>,
    mut studio: ResMut<StudioState>,
) {
    if keys.just_pressed(KeyCode::KeyH) && !wants_input.wants_any_keyboard_input() {
        studio.visible = !studio.visible;
    }
}

fn inspect_camera(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    wants_input: Res<EguiWantsInput>,
    mut cameras: Query<(&mut ReviewCamera, &mut Transform)>,
) {
    if wants_input.wants_any_pointer_input() {
        return;
    }
    for (mut camera, mut transform) in &mut cameras {
        let mut offset = transform.translation - camera.target;
        let mut distance = offset.length().max(0.35);
        if mouse.pressed(MouseButton::Right) && motion.delta != Vec2::ZERO {
            let yaw = offset.x.atan2(offset.z) - motion.delta.x * 0.006;
            let pitch = ((offset.y / distance).clamp(-1.0, 1.0).asin() + motion.delta.y * 0.005)
                .clamp(-1.45, 1.45);
            let (vertical, horizontal) = pitch.sin_cos();
            let (across, forward) = yaw.sin_cos();
            offset = distance * Vec3::new(horizontal * across, vertical, horizontal * forward);
        } else if mouse.pressed(MouseButton::Middle) && motion.delta != Vec2::ZERO {
            let scale = distance * 0.0018;
            let shift =
                (-*transform.right() * motion.delta.x + *transform.up() * motion.delta.y) * scale;
            camera.target += shift;
            transform.translation += shift;
        }
        if scroll.delta.y.abs() > f32::EPSILON {
            let sensitivity = match scroll.unit {
                MouseScrollUnit::Line => 0.12,
                MouseScrollUnit::Pixel => 0.002,
            };
            distance = (distance * (-scroll.delta.y * sensitivity).exp()).clamp(0.35, 100.0);
            offset = offset.normalize_or(Vec3::Z) * distance;
        }
        transform.translation = camera.target + offset;
        *transform = transform.looking_at(camera.target, Vec3::Y);
    }
}

fn translucent(colour: egui::Color32, alpha: u8) -> egui::Color32 {
    let [red, green, blue, _] = colour.to_srgba_unmultiplied();
    egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

fn hairline() -> egui::Stroke {
    egui::Stroke::new(1.0, translucent(ACCENT, 56))
}

fn panel_frame(alpha: u8) -> egui::Frame {
    egui::Frame::NONE
        .fill(translucent(PANEL, alpha))
        .stroke(hairline())
        .corner_radius(8)
        .inner_margin(12.0)
}

fn caps(text: &str, size: f32, colour: egui::Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .extra_letter_spacing(1.0)
        .color(colour)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_seed_does_not_build_a_request() {
        let settings = Settings {
            seed: 42,
            recipe: BotanicalRecipe::default(),
            lod: ReviewLod::Near,
            view: ReviewView::Whole,
            light: ReviewLight::Front,
            foliage: true,
            fine_shoots: true,
            wind_phase: 0.0,
            wind_strength: 0.0,
            screenshot: None,
            capture_ui: false,
        };
        let mut studio = StudioState::new(&settings);
        studio.seed_text = "not a seed".into();
        studio.reparse_seed();
        assert!(studio.request(&settings).is_none());
    }

    #[test]
    fn reset_recipe_is_not_dirty_against_defaults() {
        let settings = Settings {
            seed: 42,
            recipe: BotanicalRecipe::default(),
            lod: ReviewLod::Near,
            view: ReviewView::Whole,
            light: ReviewLight::Front,
            foliage: true,
            fine_shoots: true,
            wind_phase: 0.0,
            wind_strength: 0.0,
            screenshot: None,
            capture_ui: false,
        };
        assert!(!StudioState::new(&settings).is_dirty(&settings));
    }

    #[test]
    fn live_regeneration_requests_each_distinct_draft_once() {
        let settings = Settings {
            seed: 42,
            recipe: BotanicalRecipe::default(),
            lod: ReviewLod::Near,
            view: ReviewView::Whole,
            light: ReviewLight::Front,
            foliage: true,
            fine_shoots: true,
            wind_phase: 0.0,
            wind_strength: 0.0,
            screenshot: None,
            capture_ui: false,
        };
        let mut studio = StudioState::new(&settings);
        studio.recipe.primary_count = 8;
        assert!(studio.next_request(&settings).is_some());
        assert!(studio.next_request(&settings).is_none());
        studio.recipe.primary_count = 7;
        assert!(studio.next_request(&settings).is_some());
    }

    #[test]
    fn immediate_rebuild_does_not_queue_the_same_draft_twice() {
        let settings = Settings {
            seed: 42,
            recipe: BotanicalRecipe::default(),
            lod: ReviewLod::Near,
            view: ReviewView::Whole,
            light: ReviewLight::Front,
            foliage: true,
            fine_shoots: true,
            wind_phase: 0.0,
            wind_strength: 0.0,
            screenshot: None,
            capture_ui: false,
        };
        let mut studio = StudioState::new(&settings);
        studio.recipe.primary_count = 8;

        assert!(studio.request_now(&settings).is_some());
        assert!(studio.next_request(&settings).is_none());
    }

    #[test]
    fn enabling_wind_supplies_visible_strength_without_overwriting_a_choice() {
        let mut settings = Settings {
            seed: 42,
            recipe: BotanicalRecipe::default(),
            lod: ReviewLod::Near,
            view: ReviewView::Whole,
            light: ReviewLight::Front,
            foliage: true,
            fine_shoots: true,
            wind_phase: 0.0,
            wind_strength: 0.0,
            screenshot: None,
            capture_ui: false,
        };
        let mut studio = StudioState::new(&settings);
        studio.set_wind_animation(&mut settings, true);
        assert!(studio.animate_wind);
        assert!((settings.wind_strength - DEFAULT_WIND_STRENGTH).abs() < f32::EPSILON);

        settings.wind_strength = 0.62;
        studio.set_wind_animation(&mut settings, false);
        studio.set_wind_animation(&mut settings, true);
        assert!((settings.wind_strength - 0.62).abs() < f32::EPSILON);
    }

    #[test]
    fn showcase_has_one_embedded_hero_for_each_species() {
        assert_eq!(HERO_PRESETS.len(), BotanicalSpecies::ALL.len());
        for species in BotanicalSpecies::ALL {
            let matches = HERO_PRESETS
                .iter()
                .filter(|preset| preset.recipe.species == species)
                .count();
            assert_eq!(matches, 1, "{species:?} should have exactly one hero");
        }
        for preset in HERO_PRESETS {
            assert!(!preset.name.is_empty());
            assert!(!preset.character.is_empty());
            assert!(!preset.thumbnail.is_empty());
        }
    }

    #[test]
    fn hero_application_restores_a_complete_near_lod_draft() {
        let settings = Settings {
            seed: 7,
            recipe: BotanicalRecipe::for_species(BotanicalSpecies::Nikau),
            lod: ReviewLod::Far,
            view: ReviewView::Detail,
            light: ReviewLight::Front,
            foliage: false,
            fine_shoots: false,
            wind_phase: 0.0,
            wind_strength: 0.0,
            screenshot: None,
            capture_ui: false,
        };
        let mut studio = StudioState::new(&settings);
        let hero = HERO_PRESETS[2];
        hero.apply(&mut studio);

        assert!(hero.matches(&studio));
        let request = studio.request(&settings).expect("hero seed is valid").0;
        assert_eq!(request.seed, hero.seed);
        assert_eq!(request.recipe, hero.recipe);
        assert_eq!(request.lod, ReviewLod::Near);
        assert!(request.foliage);
        assert!(request.fine_shoots);
        assert_eq!(studio.recipe.species, BotanicalSpecies::Harakeke);
    }

    #[test]
    fn showcase_exposes_only_three_general_purpose_camera_views() {
        assert_eq!(SHOWCASE_VIEWS.len(), 3);
        assert_eq!(SHOWCASE_VIEWS[0], ReviewView::Whole);
        assert!(SHOWCASE_VIEWS.contains(&ReviewView::Crown));
        assert!(SHOWCASE_VIEWS.contains(&ReviewView::Detail));
        assert!(!SHOWCASE_VIEWS.contains(&ReviewView::Frond));
    }
}
