//! The on-screen interface, laid out as a full-screen HUD: a command bar
//! across the top, the archetype rail on the left, the sculpt inspector on the
//! right, and telemetry, key hints and view toggles along the bottom.
//!
//! The inspector edits a draft of the generator's inputs and hands the whole
//! draft over at once, on the Generate button, because a build takes seconds
//! to minutes and a slider dragged across its range would otherwise ask for
//! hundreds of them. The island on screen stays up until the new one lands. An
//! archetype card is the same handoff with the draft filled in for you.
//!
//! Everything here is drawn with `bevy_egui`, and neither plugin is installed
//! under `--screenshot`, so no capture can carry any of it whatever the HUD's
//! toggle was left at.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::{
    asset::RenderAssetUsages,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    image::{CompressedImageFormats, ImageSampler, ImageType},
    prelude::*,
};
use bevy_egui::{
    EguiContexts, EguiPlugin, EguiPostUpdateSet, EguiPrimaryContextPass, EguiTextureHandle,
    EguiUserTextures, egui, input::EguiWantsInput,
};
use motu::IslandOptions;

use crate::{
    budget::RenderBudget,
    camera::{CameraMode, UiFocus, WALK_KEY},
    capture::DebugView,
    hash::mix,
    island_gen::{GenerationSettings, GenerationStatus, IslandReady, Regenerate},
    options::{self, Group, PARAMETERS, Parameter, TERRAIN_SIZE_FLAG, TERRAIN_SIZE_RANGE},
    presets::{PRESETS, Preset},
    weather::Weather,
};

/// Shows and hides the HUD. The ☰ button in the command bar does the same
/// thing for a mouse, and stays behind as a lone chip while the HUD is down.
pub const HUD_KEY: KeyCode = KeyCode::KeyH;

/// How often the frame-rate readout takes a new number. Twice a second is
/// slow enough to read and fast enough to still be a frame rate.
const FPS_INTERVAL: f32 = 0.5;

/// The gap every anchored area keeps from the edge it is anchored to.
const MARGIN: f32 = 12.0;
/// How far down the two side panels start: under the command bar.
const BAR_CLEARANCE: f32 = 56.0;
/// Room the side panels leave above the bottom strips.
const BOTTOM_CLEARANCE: f32 = 44.0;
/// The side panels keep at least this much body, however short the window.
const BODY_MINIMUM_HEIGHT: f32 = 140.0;
/// Below this window width the centre hint strip would collide with the
/// telemetry, so it is the one strip that gives way.
const HINTS_MINIMUM_WIDTH: f32 = 900.0;

/// The archetype rail: two cards to a row, and no wider than that row.
const CARD_COLUMNS: usize = 2;
const CARD_WIDTH: f32 = 96.0;
const THUMBNAIL_HEIGHT: f32 = 61.0;
const CARD_HEIGHT: f32 = THUMBNAIL_HEIGHT + 17.0;
const CARD_SPACING: f32 = 8.0;
#[allow(clippy::cast_precision_loss)]
const RAIL_WIDTH: f32 = CARD_COLUMNS as f32 * CARD_WIDTH + CARD_SPACING;

/// The sculpt inspector, and the parts of a parameter row inside it. Wide
/// enough that four tab titles land in one row.
const INSPECTOR_WIDTH: f32 = 292.0;
const SLIDER_WIDTH: f32 = 156.0;
const VALUE_WIDTH: f32 = 52.0;
const STEPPER: f32 = 17.0;

/// Distinguishes the randomized seed from the crate's other hashed values.
const SEED_SALT: u64 = 0x1f0b_75c2_e4a9_3d68;

/// The palette: deep navy panels over the sea, off-white text a stop under
/// paper so a panel against a bright horizon does not glare, cyan for
/// selection and focus, and amber kept for the one primary action.
const GROUND: egui::Color32 = egui::Color32::from_rgb(7, 14, 21);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(15, 27, 37);
const RAISED: egui::Color32 = egui::Color32::from_rgb(25, 42, 55);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(76, 196, 228);
const AMBER: egui::Color32 = egui::Color32::from_rgb(226, 160, 55);
const AMBER_BRIGHT: egui::Color32 = egui::Color32::from_rgb(242, 180, 80);
const AMBER_TEXT: egui::Color32 = egui::Color32::from_rgb(31, 20, 4);
const TEXT: egui::Color32 = egui::Color32::from_rgb(226, 234, 241);
const DIM_TEXT: egui::Color32 = egui::Color32::from_rgb(126, 148, 163);
/// A rejected parameter set is the one thing on screen that has to be read
/// before anything else is touched.
const FAILURE_COLOUR: egui::Color32 = egui::Color32::from_rgb(230, 120, 90);
/// How much of the scene shows through a panel. Opaque enough to read a slider
/// against a bright sky, open enough that the island is still there behind it.
const PANEL_ALPHA: u8 = 212;
const STRIP_ALPHA: u8 = 196;
const PANEL_CORNER: u8 = 8;
const CONTROL_CORNER: u8 = 4;

/// The preset captures from `docs/captures/showcase`, downscaled into
/// `assets/preset-thumbs` and embedded here, keyed by the preset's name.
/// `every_preset_has_a_thumbnail` holds the two tables together.
const THUMBNAILS: [(&str, &[u8]); 10] = [
    (
        "The Spires",
        include_bytes!("../assets/preset-thumbs/the-spires.png"),
    ),
    (
        "Lone Cone",
        include_bytes!("../assets/preset-thumbs/lone-cone.png"),
    ),
    (
        "Stone Tower",
        include_bytes!("../assets/preset-thumbs/stone-tower.png"),
    ),
    (
        "Snowcap Massif",
        include_bytes!("../assets/preset-thumbs/snowcap-massif.png"),
    ),
    (
        "Gullied Ridges",
        include_bytes!("../assets/preset-thumbs/gullied-ridges.png"),
    ),
    (
        "Uncut Dome",
        include_bytes!("../assets/preset-thumbs/uncut-dome.png"),
    ),
    (
        "River Country",
        include_bytes!("../assets/preset-thumbs/river-country.png"),
    ),
    (
        "Silted Shore",
        include_bytes!("../assets/preset-thumbs/silted-shore.png"),
    ),
    (
        "Tidal Flats",
        include_bytes!("../assets/preset-thumbs/tidal-flats.png"),
    ),
    (
        "Bare Atoll",
        include_bytes!("../assets/preset-thumbs/bare-atoll.png"),
    ),
];

/// The inspector's four sections, one on screen at a time.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Form,
    Hydraulics,
    Rivers,
    Biome,
}

impl Tab {
    const ALL: [Self; 4] = [Self::Form, Self::Hydraulics, Self::Rivers, Self::Biome];

    const fn title(self) -> &'static str {
        match self {
            Self::Form => "FORM",
            Self::Hydraulics => "HYDRAULICS",
            Self::Rivers => "RIVERS",
            Self::Biome => "BIOME",
        }
    }
}

#[derive(Resource)]
struct Hud {
    visible: bool,
    /// The values the controls edit. Nothing generates from them until
    /// Generate, or an archetype card, is pressed.
    seed: u64,
    options: IslandOptions,
    /// The argument list that reproduces the island on screen, rebuilt when one
    /// lands rather than from the draft, which has usually moved on. The copy
    /// button puts it on the clipboard.
    command_line: String,
    /// Smoothed frames per second, and the countdown to the next reading.
    fps: f64,
    next_reading: f32,
    tab: Tab,
    archetypes_open: bool,
}

/// One egui texture per preset, in `PRESETS` order, with the handle that keeps
/// it alive. `None` where the embedded capture failed to decode, which leaves
/// that card labelled but blank rather than the HUD gone.
#[derive(Resource, Default)]
struct PresetThumbnails {
    entries: Vec<Option<(egui::TextureId, Handle<Image>)>>,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((EguiPlugin::default(), FrameTimeDiagnosticsPlugin::default()))
            .add_systems(Startup, (install, load_thumbnails))
            .add_systems(
                Update,
                (
                    toggle,
                    close_for_walking,
                    read_frame_rate,
                    report_command_line,
                ),
            )
            // The style is settled before anything is drawn with it, and the
            // toggle chip is drawn whether or not the HUD it opens is.
            .add_systems(
                EguiPrimaryContextPass,
                (
                    apply_theme,
                    draw_command_bar,
                    draw_generation_strip,
                    draw_archetypes,
                    draw_inspector,
                    draw_toggle_chip,
                    draw_telemetry,
                    draw_hints,
                    draw_view_cluster,
                )
                    .chain(),
            )
            // After the pass that decides it, so the camera reads the answer to
            // the frame just drawn on the next one.
            .add_systems(
                PostUpdate,
                hand_input_over.after(EguiPostUpdateSet::ProcessOutput),
            );
    }
}

/// The draft opens on whatever the command line asked for, so the HUD and the
/// island agree from the first frame.
fn install(mut commands: Commands, settings: Res<GenerationSettings>) {
    commands.insert_resource(Hud {
        visible: true,
        seed: settings.seed,
        options: settings.options,
        command_line: options::command_line(settings.seed, &settings.options),
        fps: 0.0,
        next_reading: 0.0,
        tab: Tab::Form,
        archetypes_open: true,
    });
}

/// Decodes the embedded captures into textures once, at startup. A capture
/// that will not decode costs its card the picture and nothing else.
fn load_thumbnails(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut textures: ResMut<EguiUserTextures>,
) {
    let entries = PRESETS
        .iter()
        .map(|preset| {
            let bytes = THUMBNAILS
                .iter()
                .find(|(name, _)| *name == preset.name)
                .map(|(_, bytes)| *bytes)?;
            let image = Image::from_buffer(
                bytes,
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
        })
        .collect();
    commands.insert_resource(PresetThumbnails { entries });
}

fn toggle(keys: Res<ButtonInput<KeyCode>>, ui: Res<UiFocus>, mut hud: ResMut<Hud>) {
    if keys.just_pressed(HUD_KEY) && !ui.keyboard {
        hud.visible = !hud.visible;
    }
}

fn read_frame_rate(time: Res<Time>, diagnostics: Res<DiagnosticsStore>, mut hud: ResMut<Hud>) {
    hud.next_reading -= time.delta_secs();
    if hud.next_reading > 0.0 {
        return;
    }
    hud.next_reading = FPS_INTERVAL;
    if let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed)
    {
        hud.fps = fps;
    }
}

fn report_command_line(
    mut ready: MessageReader<IslandReady>,
    settings: Res<GenerationSettings>,
    mut hud: ResMut<Hud>,
) {
    if ready.read().next().is_some() {
        hud.command_line = options::command_line(settings.seed, &settings.options);
    }
}

/// The HUD owns the pointer whenever it is under it and the keyboard whenever
/// a field is being typed into, and says so here rather than acting on it, so
/// the camera stays the one place movement is decided. Being on screen at all
/// is the third claim, and the one walking answers by giving its captured
/// cursor back.
fn hand_input_over(wants: Res<EguiWantsInput>, hud: Res<Hud>, mut ui: ResMut<UiFocus>) {
    ui.pointer = wants.wants_any_pointer_input();
    ui.keyboard = wants.wants_any_keyboard_input();
    ui.shown = hud.visible;
}

/// Walking captures the cursor, so a HUD left open behind it could never be
/// clicked. Taking to foot closes it, and `H` opens it again and takes the
/// cursor back for as long as it is up: on foot the HUD is a pause screen.
fn close_for_walking(mode: Res<CameraMode>, mut hud: ResMut<Hud>) {
    if mode.is_changed() && *mode == CameraMode::Walk {
        hud.visible = false;
    }
}

/// The letter on the key. `KeyCode` names a letter key `KeyH`, and the HUD is
/// the one place the binding is read rather than matched on, so it is also the
/// one place that spelling has to be undone.
fn key_name(key: KeyCode) -> String {
    format!("{key:?}").trim_start_matches("Key").to_string()
}

/// A number to a fixed width, so a value that changes does not shift the label
/// beside it. Small values need the digits; large ones have none to spare.
fn decimals(maximum: f32) -> usize {
    if maximum <= 1.0 {
        3
    } else if maximum <= 20.0 {
        2
    } else {
        1
    }
}

/// What the inspector calls a parameter and what its tooltip says it does,
/// against the flag that reproduces it. The tooltip carries the flag too, so
/// the friendly name never hides the spelling the command line takes. A flag
/// the match does not know wears its own dashless spelling and no description,
/// so a new parameter arrives labelled.
fn describe(parameter: &Parameter) -> (&'static str, &'static str) {
    match parameter.flag {
        "--max-height" => (
            "RELIEF",
            "How tall the island stands relative to its width.",
        ),
        "--water-ratio" => (
            "SEA LEVEL",
            "How much of the world is sea. Higher drowns more of the land.",
        ),
        "--slope-multiplier" => ("SLOPE", "How steeply the interior rises."),
        "--coastal-slope-multiplier" => ("COAST", "How steeply the land falls away to the shore."),
        "--hydraulic-erosion-strength" => (
            "EROSION",
            "How aggressively running water carves gullies into the flanks.",
        ),
        "--hydraulic-deposition-strength" => (
            "DEPOSITION",
            "How much of the carved sediment settles back onto lower ground.",
        ),
        "--hydraulic-deposition-slope-degrees" => (
            "SETTLE ANGLE",
            "The steepest ground sediment will settle on, in degrees.",
        ),
        "--river-source-catchment-hectares" => (
            "SOURCE CATCHMENT",
            "How much land must drain through a point before a river starts \
             there. Smaller means more rivers.",
        ),
        "--river-source-steep-multiplier" => (
            "STEEP PENALTY",
            "How much more catchment a river needs to start on steep ground.",
        ),
        "--river-source-elevation-boost" => (
            "LOWLAND BOOST",
            "How much more readily rivers start on low ground.",
        ),
        "--river-source-width-metres" => (
            "SOURCE WIDTH",
            "How wide a river is where it starts, in metres.",
        ),
        "--river-maximum-width-metres" => (
            "MAX WIDTH",
            "The widest a river grows on its way to the sea, in metres.",
        ),
        "--river-source-depth-metres" => (
            "SOURCE DEPTH",
            "How deep a river runs where it starts, in metres.",
        ),
        "--river-maximum-depth-metres" => (
            "MAX DEPTH",
            "The deepest a river cuts on its way to the sea, in metres.",
        ),
        _ => (parameter.label(), ""),
    }
}

/// The tooltip a parameter row wears: what it does, then the flag that spells
/// it on the command line.
fn tooltip(description: &str, flag: &str) -> String {
    if description.is_empty() {
        flag.to_string()
    } else {
        format!("{description}\n{flag}")
    }
}

/// The preset the draft currently is, if it is one: the same comparison a card
/// uses to light up, read again by the command bar to name the world.
fn matching_preset(hud: &Hud) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| {
        hud.seed == preset.seed && hud.options == preset.options(hud.options.terrain_size)
    })
}

/// The dark translucent look, installed once on the context that draws it.
fn apply_theme(mut contexts: EguiContexts, mut installed: Local<bool>) {
    if *installed {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    // Pinned dark, and written into both of egui's themes, so what the host
    // reports as its preference cannot put a light panel over the sea.
    context.set_theme(egui::ThemePreference::Dark);
    context.all_styles_mut(theme);
    *installed = true;
}

fn theme(style: &mut egui::Style) {
    use egui::{FontFamily, FontId, TextStyle};
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(12.5, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(10.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(11.5, FontFamily::Monospace),
    );
    // Values read as figures, not prose.
    style.drag_value_text_style = TextStyle::Monospace;

    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = translucent(GROUND, PANEL_ALPHA);
    visuals.window_fill = translucent(GROUND, PANEL_ALPHA);
    visuals.window_stroke = hairline();
    visuals.window_corner_radius = egui::CornerRadius::same(PANEL_CORNER);
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;
    visuals.selection.bg_fill = translucent(ACCENT, 130);
    visuals.selection.stroke = egui::Stroke::new(1.0, TEXT);
    visuals.hyperlink_color = ACCENT;
    visuals.slider_trailing_fill = true;
    // Four widget states, from the frame around a label to the button under
    // the finger: the fill climbs towards the accent and the outline with it,
    // so a control reads as reachable before it is reached.
    visuals.widgets.noninteractive.bg_fill = translucent(SURFACE, 150);
    visuals.widgets.noninteractive.bg_stroke = hairline();
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, DIM_TEXT);
    visuals.widgets.inactive.bg_fill = translucent(SURFACE, 200);
    visuals.widgets.inactive.weak_bg_fill = translucent(SURFACE, 200);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, translucent(ACCENT, 40));
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.bg_fill = translucent(RAISED, 230);
    visuals.widgets.hovered.weak_bg_fill = translucent(RAISED, 230);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, translucent(ACCENT, 180));
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.active.bg_fill = translucent(ACCENT, 190);
    visuals.widgets.active.weak_bg_fill = translucent(ACCENT, 190);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(CONTROL_CORNER);
    }
    style.spacing.item_spacing = egui::vec2(6.0, 5.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.slider_width = SLIDER_WIDTH;
    style.spacing.slider_rail_height = 3.0;
    style.spacing.indent = 12.0;
}

/// egui's colours are premultiplied, so an alpha applied to one has to scale
/// its channels with it.
fn translucent(colour: egui::Color32, alpha: u8) -> egui::Color32 {
    let [red, green, blue, _] = colour.to_srgba_unmultiplied();
    egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

/// The one-pixel edge every panel and chip shares: a cyan so dim it reads as
/// a hairline rather than a glow.
fn hairline() -> egui::Stroke {
    egui::Stroke::new(1.0, translucent(ACCENT, 56))
}

/// The frame every anchored area is drawn in: one fill, one hairline, one
/// radius, so the bar, the panels and the strips are recognisably one set.
fn panel_frame(alpha: u8) -> egui::Frame {
    egui::Frame::NONE
        .fill(translucent(GROUND, alpha))
        .stroke(hairline())
        .corner_radius(PANEL_CORNER)
        .inner_margin(12.0)
}

/// A section or panel title: small caps spelled with capitals, tracked out a
/// little, a stop dimmer than body text.
fn caps(text: &str, size: f32, colour: egui::Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .extra_letter_spacing(1.1)
        .color(colour)
}

// -------------------------------------------------------------------------
// Command bar
// -------------------------------------------------------------------------

/// The strip along the top: identity on the left, the two actions on the
/// right, and the name of the archetype the draft matches — or the fact it is
/// nobody's — laid centred over the same layer. One layer, deliberately: a
/// second area over the bar would drop behind its translucent fill the first
/// time the bar was clicked, egui raising whatever was clicked last.
fn draw_command_bar(
    mut contexts: EguiContexts,
    mut hud: ResMut<Hud>,
    status: Res<GenerationStatus>,
    mut requests: MessageWriter<Regenerate>,
) {
    if !hud.visible {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let width = context.content_rect().width();
    let running = status.is_running();
    egui::Area::new(egui::Id::new("command bar"))
        .anchor(egui::Align2::LEFT_TOP, [0.0, 0.0])
        .show(context, |ui| {
            egui::Frame::NONE
                .fill(translucent(GROUND, 232))
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.set_width(width - 28.0);
                    ui.horizontal(|ui| {
                        let toggle =
                            egui::Button::new(egui::RichText::new("☰").size(14.0)).frame(false);
                        if ui
                            .add(toggle)
                            .on_hover_text(format!("{} hides the HUD", key_name(HUD_KEY)))
                            .clicked()
                        {
                            hud.visible = false;
                        }
                        ui.label(
                            egui::RichText::new("MOTU")
                                .size(14.0)
                                .strong()
                                .extra_letter_spacing(1.5)
                                .color(TEXT),
                        );
                        ui.label(caps("// WORLD FORGE", 11.0, DIM_TEXT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            draw_generate(ui, &hud, running, &mut requests);
                        });
                    });
                    let rect = ui.max_rect();
                    let name = matching_preset(&hud).map_or("CUSTOM ISLAND", |preset| preset.name);
                    ui.put(
                        egui::Rect::from_center_size(rect.center(), egui::vec2(320.0, 18.0)),
                        egui::Label::new(caps(&name.to_uppercase(), 12.0, TEXT).strong()),
                    );
                    // The bar's one edge: a hairline along its foot.
                    let edge = rect.expand2(egui::vec2(14.0, 8.0));
                    ui.painter()
                        .hline(edge.x_range(), edge.bottom(), hairline());
                });
        });
}

/// The primary action, and the one amber thing on screen. While a build runs
/// it holds its ground, disabled, and counts the seconds.
fn draw_generate(
    ui: &mut egui::Ui,
    hud: &Hud,
    running: bool,
    requests: &mut MessageWriter<Regenerate>,
) {
    // Scoped, so the amber stays on this one button rather than leaking into
    // whatever the row draws after it.
    let clicked = ui
        .scope(|ui| {
            let visuals = &mut ui.style_mut().visuals;
            visuals.widgets.inactive.weak_bg_fill = AMBER;
            visuals.widgets.hovered.weak_bg_fill = AMBER_BRIGHT;
            visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, AMBER_BRIGHT);
            visuals.widgets.active.weak_bg_fill = AMBER;
            let label = if running {
                "GENERATING…"
            } else {
                "GENERATE  ⏵"
            };
            let button = egui::Button::new(
                egui::RichText::new(label)
                    .size(11.5)
                    .strong()
                    .extra_letter_spacing(1.0)
                    .color(AMBER_TEXT),
            )
            .min_size(egui::vec2(112.0, 26.0));
            ui.add_enabled(!running, button)
                .on_hover_text("rebuild the island from the draft")
                .clicked()
        })
        .inner;
    if clicked {
        requests.write(Regenerate {
            seed: hud.seed,
            options: hud.options,
        });
    }
}

// -------------------------------------------------------------------------
// Archetype rail
// -------------------------------------------------------------------------

/// The curated islands as picture cards, two to a row down the left edge. A
/// click fills the draft in and asks for the rebuild in the same frame: the
/// point of an archetype is that it is one press, and the cache makes a second
/// visit to one instant.
fn draw_archetypes(
    mut contexts: EguiContexts,
    mut hud: ResMut<Hud>,
    thumbnails: Res<PresetThumbnails>,
    status: Res<GenerationStatus>,
    mut requests: MessageWriter<Regenerate>,
) {
    if !hud.visible {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let running = status.is_running();
    let body_height =
        (context.content_rect().height() - BAR_CLEARANCE - MARGIN - BOTTOM_CLEARANCE - 76.0)
            .max(BODY_MINIMUM_HEIGHT);
    egui::Window::new("archetypes")
        .anchor(egui::Align2::LEFT_TOP, [MARGIN, BAR_CLEARANCE])
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .frame(panel_frame(PANEL_ALPHA))
        .show(context, |ui| {
            ui.set_width(RAIL_WIDTH);
            ui.horizontal(|ui| {
                ui.label(caps("ISLAND ARCHETYPES", 11.0, TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let chevron = if hud.archetypes_open { "⏷" } else { "⏵" };
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(chevron).size(10.0)).frame(false),
                        )
                        .clicked()
                    {
                        hud.archetypes_open = !hud.archetypes_open;
                    }
                });
            });
            if hud.archetypes_open {
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .show(ui, |ui| {
                        draw_cards(ui, &mut hud, &thumbnails, running, &mut requests);
                    });
                ui.add_space(2.0);
                ui.separator();
                ui.label(caps(&format!("{} PRESETS", PRESETS.len()), 9.5, DIM_TEXT));
            }
        });
}

fn draw_cards(
    ui: &mut egui::Ui,
    hud: &mut Hud,
    thumbnails: &PresetThumbnails,
    running: bool,
    requests: &mut MessageWriter<Regenerate>,
) {
    egui::Grid::new("archetype cards")
        .num_columns(CARD_COLUMNS)
        .spacing([CARD_SPACING, CARD_SPACING])
        .show(ui, |ui| {
            for (index, preset) in PRESETS.iter().enumerate() {
                let selected = hud.seed == preset.seed
                    && hud.options == preset.options(hud.options.terrain_size);
                let thumbnail = thumbnails
                    .entries
                    .get(index)
                    .and_then(|entry| entry.as_ref().map(|(id, _)| *id));
                if preset_card(ui, preset, thumbnail, selected, !running) {
                    hud.seed = preset.seed;
                    hud.options = preset.options(hud.options.terrain_size);
                    requests.write(Regenerate {
                        seed: hud.seed,
                        options: hud.options,
                    });
                }
                if index % CARD_COLUMNS == CARD_COLUMNS - 1 {
                    ui.end_row();
                }
            }
        });
}

/// One archetype: its capture, its name under it, a cyan edge and check when
/// it is what the draft says. Returns whether it was pressed.
fn preset_card(
    ui: &mut egui::Ui,
    preset: &Preset,
    thumbnail: Option<egui::TextureId>,
    selected: bool,
    enabled: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(CARD_WIDTH, CARD_HEIGHT), egui::Sense::click());
    let image_rect = egui::Rect::from_min_size(rect.min, egui::vec2(CARD_WIDTH, THUMBNAIL_HEIGHT));
    ui.painter()
        .rect_filled(image_rect, CONTROL_CORNER, translucent(SURFACE, 255));
    if let Some(id) = thumbnail {
        egui::Image::new((id, image_rect.size()))
            .corner_radius(CONTROL_CORNER)
            .paint_at(ui, image_rect);
    }
    let painter = ui.painter();
    if selected {
        painter.rect_stroke(
            image_rect,
            CONTROL_CORNER,
            egui::Stroke::new(1.5, ACCENT),
            egui::StrokeKind::Outside,
        );
        let badge = image_rect.right_top() + egui::vec2(-10.0, 10.0);
        painter.circle_filled(badge, 7.0, ACCENT);
        painter.text(
            badge,
            egui::Align2::CENTER_CENTER,
            "✔",
            egui::FontId::proportional(9.0),
            GROUND,
        );
    } else if enabled && response.hovered() {
        painter.rect_stroke(
            image_rect,
            CONTROL_CORNER,
            egui::Stroke::new(1.0, translucent(ACCENT, 170)),
            egui::StrokeKind::Outside,
        );
    } else {
        painter.rect_stroke(
            image_rect,
            CONTROL_CORNER,
            hairline(),
            egui::StrokeKind::Inside,
        );
    }
    painter.text(
        rect.center_bottom(),
        egui::Align2::CENTER_BOTTOM,
        preset.name.to_uppercase(),
        egui::FontId::proportional(8.5),
        if selected { TEXT } else { DIM_TEXT },
    );
    enabled && response.on_hover_text(preset.character).clicked()
}

// -------------------------------------------------------------------------
// Sculpt inspector
// -------------------------------------------------------------------------

/// The right-hand panel: every generator parameter, in four tabs, over the
/// footer that reports the island on screen and copies its command line.
fn draw_inspector(
    mut contexts: EguiContexts,
    mut hud: ResMut<Hud>,
    status: Res<GenerationStatus>,
    mut weather: ResMut<Weather>,
    mut debug_view: ResMut<DebugView>,
) {
    if !hud.visible {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let body_height =
        (context.content_rect().height() - BAR_CLEARANCE - MARGIN - BOTTOM_CLEARANCE - 118.0)
            .max(BODY_MINIMUM_HEIGHT);
    egui::Window::new("sculpt")
        .anchor(egui::Align2::RIGHT_TOP, [-MARGIN, BAR_CLEARANCE])
        .default_width(INSPECTOR_WIDTH)
        .max_width(INSPECTOR_WIDTH)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .frame(panel_frame(PANEL_ALPHA))
        .show(context, |ui| {
            ui.set_width(INSPECTOR_WIDTH);
            ui.label(caps("TERRAIN SCULPT", 11.0, TEXT));
            ui.add_space(2.0);
            draw_tabs(ui, &mut hud);
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(body_height)
                .show(ui, |ui| match hud.tab {
                    Tab::Form => draw_form(ui, &mut hud),
                    Tab::Hydraulics => draw_group(ui, &mut hud, Group::Hydraulics),
                    Tab::Rivers => draw_group(ui, &mut hud, Group::Rivers),
                    Tab::Biome => draw_biome(ui, &mut weather, &mut debug_view),
                });
            ui.add_space(4.0);
            ui.separator();
            draw_footer(ui, &hud, &status);
        });
}

/// Four titles across the panel, the live one underlined in the accent: the
/// quietest thing that still reads as a tab. Tracked tighter than the other
/// capitals, because HYDRAULICS has to land in a quarter of the panel.
fn draw_tabs(ui: &mut egui::Ui, hud: &mut Hud) {
    #[allow(clippy::cast_precision_loss)]
    let count = Tab::ALL.len() as f32;
    let spacing = ui.spacing().item_spacing.x;
    let width = (ui.available_width() - spacing * (count - 1.0)) / count;
    ui.horizontal(|ui| {
        for tab in Tab::ALL {
            let selected = hud.tab == tab;
            let colour = if selected { TEXT } else { DIM_TEXT };
            let title = egui::RichText::new(tab.title())
                .size(9.5)
                .extra_letter_spacing(0.4)
                .color(colour);
            let response = ui.add_sized([width, 20.0], egui::Button::new(title).frame(false));
            if selected {
                ui.painter().hline(
                    response.rect.x_range().shrink(8.0),
                    response.rect.bottom() - 1.0,
                    egui::Stroke::new(2.0, ACCENT),
                );
            }
            if response.clicked() {
                hud.tab = tab;
            }
        }
    });
}

fn draw_form(ui: &mut egui::Ui, hud: &mut Hud) {
    seed_row(ui, &mut hud.seed);
    terrain_size_row(ui, &mut hud.options.terrain_size);
    for parameter in PARAMETERS
        .iter()
        .filter(|entry| entry.group == Group::Terrain)
    {
        parameter_row(ui, parameter, &mut hud.options);
    }
    options::reconcile(&mut hud.options);
}

/// One parameter group as one tab: the Hydraulics and Rivers tables, each
/// under its own title in the tab strip.
fn draw_group(ui: &mut egui::Ui, hud: &mut Hud, group: Group) {
    for parameter in PARAMETERS.iter().filter(|entry| entry.group == group) {
        parameter_row(ui, parameter, &mut hud.options);
    }
    // A maximum dragged under its own source would only fail once the
    // generator ran, so it is put back straight away and the slider shows
    // where it actually landed.
    options::reconcile(&mut hud.options);
}

/// The two looks. Nothing here regenerates: weather moves the sun, the air,
/// the cloud and the mist, the surface view is a uniform the shaders read, and
/// the island under both is the same island.
fn draw_biome(ui: &mut egui::Ui, weather: &mut Weather, debug_view: &mut DebugView) {
    section(ui, "ATMOSPHERE").on_hover_text(
        "Sun, sky, cloud and mist, as one named look. Moves the light and \
         the air, never the island.",
    );
    egui::ComboBox::from_id_salt("weather")
        .selected_text(weather.label())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for look in Weather::all() {
                ui.selectable_value(weather, look, look.label());
            }
        });
    section(ui, "SURFACE VIEW").on_hover_text(
        "Diagnostic channels the surfaces can display instead of their \
         ordinary shading. Off is the island as rendered.",
    );
    egui::ComboBox::from_id_salt("debug view")
        .selected_text(debug_view.label())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for view in DebugView::ALL {
                ui.selectable_value(debug_view, view, view.label());
            }
        });
}

/// A sub-heading inside a tab, ruled off from what came before it.
fn section(ui: &mut egui::Ui, title: &str) -> egui::Response {
    ui.add_space(6.0);
    ui.separator();
    ui.label(caps(title, 9.5, DIM_TEXT))
}

/// One parameter: its name over a row of slider, figure and steppers, each
/// part at a fixed width so fourteen rows land in the same columns.
fn parameter_row(ui: &mut egui::Ui, parameter: &Parameter, options: &mut IslandOptions) {
    let value = (parameter.field)(options);
    let dec = decimals(parameter.maximum);
    let (name, description) = describe(parameter);
    let hover = tooltip(description, parameter.flag);
    ui.add_space(4.0);
    ui.label(caps(name, 9.5, DIM_TEXT)).on_hover_text(&hover);
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(value, parameter.minimum..=parameter.maximum)
                .logarithmic(parameter.logarithmic)
                .show_value(false),
        )
        .on_hover_text(&hover);
        ui.add_sized(
            [VALUE_WIDTH, 18.0],
            egui::DragValue::new(value)
                .range(parameter.minimum..=parameter.maximum)
                .speed(f64::from(parameter.maximum - parameter.minimum) / 300.0)
                .fixed_decimals(dec),
        );
        let (down, up) = stepped(*value, parameter);
        if stepper(ui, "−") {
            *value = down;
        }
        if stepper(ui, "+") {
            *value = up;
        }
    });
}

/// Where one press of each stepper lands: a fiftieth of the range, or a fifth
/// either way on a logarithmic parameter, held inside the slider's bounds.
fn stepped(value: f32, parameter: &Parameter) -> (f32, f32) {
    let (down, up) = if parameter.logarithmic {
        (value / 1.25, value * 1.25)
    } else {
        let step = (parameter.maximum - parameter.minimum) / 50.0;
        (value - step, value + step)
    };
    (
        down.clamp(parameter.minimum, parameter.maximum),
        up.clamp(parameter.minimum, parameter.maximum),
    )
}

fn stepper(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(11.0))
            .min_size(egui::vec2(STEPPER, STEPPER)),
    )
    .clicked()
}

/// The value everything else is hashed from: dragged or typed, or replaced
/// whole by RANDOMIZE beside it. Randomizing moves only the draft — Generate
/// is still the one thing that builds.
fn seed_row(ui: &mut egui::Ui, seed: &mut u64) {
    let hover = "Which island of this shape you get. Same seed, same island, \
                 every time.";
    ui.add_space(4.0);
    ui.label(caps("SEED", 9.5, DIM_TEXT)).on_hover_text(hover);
    ui.horizontal(|ui| {
        ui.add_sized([SLIDER_WIDTH, 18.0], egui::DragValue::new(seed).speed(1.0))
            .on_hover_text(hover);
        // The rest of the row, out to the edge the other rows' steppers reach.
        if ui
            .add_sized(
                [ui.available_width(), 18.0],
                egui::Button::new(caps("RANDOMIZE", 9.0, TEXT)),
            )
            .on_hover_text("Pick a seed nobody chose; Generate builds it")
            .clicked()
        {
            *seed = random_seed();
        }
    });
}

/// The one parameter that is not an `f32`: seed points, on a logarithmic
/// slider, whose steppers halve and double because that is the scale the cost
/// moves on.
fn terrain_size_row(ui: &mut egui::Ui, size: &mut u32) {
    let hover = tooltip(
        "How many points the terrain is triangulated from: the island's \
         resolution and most of its build time.",
        TERRAIN_SIZE_FLAG,
    );
    ui.add_space(4.0);
    ui.label(caps("ISLAND SIZE", 9.5, DIM_TEXT))
        .on_hover_text(&hover);
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(size, TERRAIN_SIZE_RANGE)
                .logarithmic(true)
                .show_value(false),
        )
        .on_hover_text(&hover);
        ui.add_sized(
            [VALUE_WIDTH, 18.0],
            egui::DragValue::new(size)
                .range(TERRAIN_SIZE_RANGE)
                .speed(4.0),
        );
        if stepper(ui, "−") {
            *size = (*size / 2).max(*TERRAIN_SIZE_RANGE.start());
        }
        if stepper(ui, "+") {
            *size = size.saturating_mul(2).min(*TERRAIN_SIZE_RANGE.end());
        }
    });
}

/// What the island on screen was built from, any failure, and the copy button
/// that puts the whole reproducing argument list on the clipboard.
fn draw_footer(ui: &mut egui::Ui, hud: &Hud, status: &GenerationStatus) {
    ui.label(
        egui::RichText::new(status_line(status))
            .small()
            .color(DIM_TEXT),
    );
    if let Some(failure) = &status.failure {
        ui.colored_label(FAILURE_COLOUR, failure);
    }
    if ui
        .add(egui::Button::new(caps("COPY COMMAND", 9.5, TEXT)))
        .on_hover_text("the full argument list that reproduces the island on screen")
        .clicked()
    {
        ui.ctx().copy_text(hud.command_line.clone());
    }
}

// -------------------------------------------------------------------------
// Strips
// -------------------------------------------------------------------------

/// The one control on screen while the HUD is down, so it can be brought back
/// with nothing but the mouse. In the same corner as the bar's ☰, which takes
/// over the job while the HUD is up.
fn draw_toggle_chip(mut contexts: EguiContexts, mut hud: ResMut<Hud>) {
    if hud.visible {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    egui::Area::new(egui::Id::new("hud toggle"))
        .anchor(egui::Align2::LEFT_TOP, [MARGIN, MARGIN])
        .show(context, |ui| {
            egui::Frame::NONE
                .fill(translucent(GROUND, STRIP_ALPHA))
                .stroke(hairline())
                .corner_radius(PANEL_CORNER)
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    let button = egui::Button::new(egui::RichText::new("☰").size(14.0).color(TEXT))
                        .wrap_mode(egui::TextWrapMode::Extend)
                        .frame(false);
                    if ui
                        .add(button)
                        .on_hover_text(format!("{} shows the HUD", key_name(HUD_KEY)))
                        .clicked()
                    {
                        hud.visible = true;
                    }
                });
        });
}

/// Top centre, under the bar, and only while there is something to say: a
/// build in flight — the opening one included — or the failure the last one
/// ended in. A build leaves whatever is on screen there and takes seconds to
/// minutes, so the one thing the strip is for is that the viewer never looks
/// idle while it is working — including with the HUD shut, which is why this
/// is not part of it.
fn draw_generation_strip(mut contexts: EguiContexts, hud: Res<Hud>, status: Res<GenerationStatus>) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let spinning = status.elapsed.is_some();
    let notice = if let Some(elapsed) = status.elapsed {
        (format!("GENERATING ISLAND · {elapsed:.0} S"), TEXT)
    } else if let Some(failure) = &status.failure {
        (format!("GENERATION FAILED · {failure}"), FAILURE_COLOUR)
    } else {
        return;
    };
    let top = if hud.visible { BAR_CLEARANCE } else { MARGIN };
    egui::Area::new(egui::Id::new("generation strip"))
        .anchor(egui::Align2::CENTER_TOP, [0.0, top])
        .movable(false)
        .interactable(false)
        .show(context, |ui| {
            panel_frame(STRIP_ALPHA)
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if spinning {
                            ui.add(egui::Spinner::new().size(12.0).color(ACCENT));
                        }
                        let (text, colour) = notice;
                        ui.add(
                            egui::Label::new(caps(&text, 10.0, colour))
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                });
        });
}

fn status_line(status: &GenerationStatus) -> String {
    match (status.built, status.took) {
        (Some((seed, options)), Some(took)) => format!(
            "seed {seed} · size {} · built in {}",
            options.terrain_size,
            duration(took)
        ),
        (Some((seed, options)), None) => {
            format!("seed {seed} · size {}", options.terrain_size)
        }
        _ => String::from("waiting for the first island"),
    }
}

/// A build that came off the cache lands in tens of milliseconds, and rounding
/// that to seconds would report it as instant rather than as fast.
fn duration(seconds: f32) -> String {
    if seconds < 1.0 {
        format!("{:.0} ms", seconds * 1000.0)
    } else {
        format!("{seconds:.0} s")
    }
}

/// The crate has no random source, on purpose: everything else in it is a hash
/// of something already known. A seed nobody chose is the one value that has
/// to come from outside, so the clock supplies it.
fn random_seed() -> u64 {
    #[allow(clippy::cast_possible_truncation)]
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);
    mix(nanos, SEED_SALT)
}

/// Bottom left, one line, small and dimmed: the frame rate and what the
/// culling stages left for that frame to draw. The census is the same one a
/// capture writes into its log, so a pose found by flying can be read off the
/// screen and looked up again later. Shown whether or not the HUD is.
fn draw_telemetry(mut contexts: EguiContexts, hud: Res<Hud>, budget: Res<RenderBudget>) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let line = format!(
        "{:.0} FPS  ·  {}/{} CHUNKS  ·  {}K VERTS  ·  {}/{} SCATTER",
        hud.fps,
        budget.terrain.drawn_entities,
        budget.terrain.entities,
        budget.terrain.drawn_vertices / 1_000,
        budget.scatter.drawn_entities,
        budget.scatter.entities,
    );
    egui::Area::new(egui::Id::new("telemetry"))
        .anchor(egui::Align2::LEFT_BOTTOM, [MARGIN, -MARGIN])
        .movable(false)
        .interactable(false)
        .show(context, |ui| {
            panel_frame(STRIP_ALPHA)
                .inner_margin(egui::Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(caps(&line, 9.5, DIM_TEXT))
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
}

/// Bottom centre: what the mouse and the two mode keys do right now, keys
/// bright and verbs dim. The real bindings, not decorative ones — flying
/// orbits on the right button and pans on the left. On a narrow window this is
/// the strip that gives way.
fn draw_hints(mut contexts: EguiContexts, hud: Res<Hud>, mode: Res<CameraMode>) {
    if !hud.visible {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    if context.content_rect().width() < HINTS_MINIMUM_WIDTH {
        return;
    }
    let walk = key_name(WALK_KEY);
    let hide = key_name(HUD_KEY);
    let pairs: &[(&str, &str)] = match *mode {
        CameraMode::Fly => &[
            ("LMB", "PAN"),
            ("RMB", "ORBIT"),
            ("WHEEL", "ZOOM"),
            (&walk, "WALK"),
            (&hide, "HIDE HUD"),
        ],
        CameraMode::Walk => &[
            ("WASD", "MOVE"),
            ("SHIFT", "SPRINT"),
            ("SPACE", "JUMP"),
            (&walk, "FLY"),
            (&hide, "RESUME"),
        ],
    };
    egui::Area::new(egui::Id::new("control hints"))
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -MARGIN])
        .movable(false)
        .interactable(false)
        .show(context, |ui| {
            panel_frame(STRIP_ALPHA)
                .inner_margin(egui::Margin::symmetric(12, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        for (index, (key, verb)) in pairs.iter().enumerate() {
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

// -------------------------------------------------------------------------
// View cluster
// -------------------------------------------------------------------------

/// Which icon a view button wears; drawn with the painter, so nothing depends
/// on which glyphs the embedded fonts carry.
#[derive(Clone, Copy)]
enum ViewIcon {
    Mountain,
    Grid,
    Weights,
}

/// Bottom right: three one-press looks at the ground — shaded, chunk grid,
/// material weights — mirroring the Biome tab's surface-view list, which still
/// holds the rest of it. Pressing the lit one puts ordinary shading back.
fn draw_view_cluster(mut contexts: EguiContexts, hud: Res<Hud>, mut debug_view: ResMut<DebugView>) {
    if !hud.visible {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    egui::Area::new(egui::Id::new("view cluster"))
        .anchor(egui::Align2::RIGHT_BOTTOM, [-MARGIN, -MARGIN])
        .show(context, |ui| {
            panel_frame(STRIP_ALPHA)
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (icon, view, name) in [
                            (ViewIcon::Mountain, DebugView::Off, "shaded"),
                            (ViewIcon::Grid, DebugView::Chunks, "chunk grid"),
                            (ViewIcon::Weights, DebugView::Weights, "material weights"),
                        ] {
                            let selected = *debug_view == view;
                            if view_button(ui, icon, selected)
                                .on_hover_text(name)
                                .clicked()
                            {
                                *debug_view = if selected && view != DebugView::Off {
                                    DebugView::Off
                                } else {
                                    view
                                };
                            }
                        }
                    });
                });
        });
}

fn view_button(ui: &mut egui::Ui, icon: ViewIcon, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let fill = if selected {
        translucent(ACCENT, 46)
    } else if response.hovered() {
        translucent(RAISED, 235)
    } else {
        translucent(SURFACE, 190)
    };
    let stroke = if selected {
        egui::Stroke::new(1.0, ACCENT)
    } else {
        hairline()
    };
    let painter = ui.painter();
    painter.rect(rect, CONTROL_CORNER, fill, stroke, egui::StrokeKind::Inside);
    let colour = if selected { TEXT } else { DIM_TEXT };
    let line = egui::Stroke::new(1.3, colour);
    let at = |x: f32, y: f32| rect.min + egui::vec2(x * rect.width(), y * rect.height());
    match icon {
        ViewIcon::Mountain => {
            painter.line(
                vec![
                    at(0.18, 0.72),
                    at(0.42, 0.32),
                    at(0.56, 0.54),
                    at(0.68, 0.4),
                    at(0.84, 0.72),
                ],
                line,
            );
        }
        ViewIcon::Grid => {
            for t in [0.3, 0.5, 0.7] {
                painter.line_segment([at(0.2, t), at(0.8, t)], line);
                painter.line_segment([at(t, 0.2), at(t, 0.8)], line);
            }
        }
        ViewIcon::Weights => {
            painter.circle_stroke(at(0.5, 0.36), 3.4, line);
            painter.circle_stroke(at(0.36, 0.62), 3.4, line);
            painter.circle_stroke(at(0.64, 0.62), 3.4, line);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::{PARAMETERS, PRESETS, THUMBNAILS, describe};

    /// The card table is keyed by preset name, so a preset renamed or added
    /// without a capture would quietly lose its picture.
    #[test]
    fn every_preset_has_a_thumbnail() {
        for preset in &PRESETS {
            let entry = THUMBNAILS.iter().find(|(name, _)| *name == preset.name);
            let (_, bytes) = entry.unwrap_or_else(|| panic!("{} has no thumbnail", preset.name));
            assert!(!bytes.is_empty(), "{} has an empty thumbnail", preset.name);
        }
        assert_eq!(THUMBNAILS.len(), PRESETS.len());
    }

    /// Every slider wears a friendly name — never the raw dashless fallback,
    /// which would mean the mapping missed a flag — and a tooltip that says
    /// what the parameter does, not just how it is spelled.
    #[test]
    fn every_parameter_has_a_name_and_a_description() {
        for parameter in &PARAMETERS {
            let (name, description) = describe(parameter);
            assert!(
                !name.is_empty() && name != parameter.label(),
                "{} has no display name",
                parameter.flag
            );
            assert!(
                !description.is_empty(),
                "{} has no description",
                parameter.flag
            );
        }
    }
}
