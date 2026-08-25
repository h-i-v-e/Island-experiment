//! The on-screen interface: a menu panel, a generation strip and a frame-rate
//! readout, each anchored to its own part of the screen rather than stacked in
//! one column.
//!
//! The panel edits a draft of the generator's inputs and hands the whole draft
//! over at once, on the Regenerate button, because a build takes seconds to
//! minutes and a slider dragged across its range would otherwise ask for
//! hundreds of them. The island on screen stays up until the new one lands. A
//! preset is the same handoff with the draft filled in for you.
//!
//! Everything here is drawn with `bevy_egui`, and neither plugin is installed
//! under `--screenshot`, so no capture can carry any of it whatever the panel's
//! toggle was left at.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_egui::{
    EguiContexts, EguiPlugin, EguiPostUpdateSet, EguiPrimaryContextPass, egui,
    input::EguiWantsInput,
};
use motu::IslandOptions;

use crate::{
    budget::RenderBudget,
    camera::{CameraMode, UiFocus, WALK_KEY},
    capture::DebugView,
    hash::mix,
    island_gen::{GenerationSettings, GenerationStatus, IslandReady, Regenerate},
    options::{self, Group, PARAMETERS, TERRAIN_SIZE_FLAG, TERRAIN_SIZE_RANGE},
    presets::PRESETS,
    weather::Weather,
};

/// Shows and hides the menu panel. The toggle button in the corner does the
/// same thing for a mouse.
pub const HUD_KEY: KeyCode = KeyCode::KeyH;

/// How often the frame-rate readout takes a new number. Twice a second is
/// slow enough to read and fast enough to still be a frame rate.
const FPS_INTERVAL: f32 = 0.5;

/// The gap every anchored area keeps from the edge it is anchored to, and from
/// the one above it. One number, so the four corners agree.
const MARGIN: f32 = 12.0;
const PANEL_WIDTH: f32 = 344.0;
/// How far down the panel starts: under the toggle button, which is anchored to
/// the same corner and stays on screen when the panel does not.
const TOGGLE_STRIP: f32 = 34.0;
/// Room kept under the scrolling body for the panel's own footer — the built
/// line, any failure, the two buttons and the key reminder — and for the
/// frame-rate block in the corner below it. The body scrolls inside whatever
/// the window leaves between them, so the controls that act on a draft are
/// never the part that runs off the bottom.
const FOOTER_HEIGHT: f32 = 118.0;
const FRAME_RATE_HEIGHT: f32 = 76.0;
/// The body keeps at least this much, however short the window is.
const BODY_MINIMUM_HEIGHT: f32 = 140.0;
/// Two preset buttons to a row, wide enough for the longest name.
const PRESET_COLUMNS: usize = 2;
const PRESET_BUTTON_WIDTH: f32 = 152.0;

/// Distinguishes the randomized seed from the crate's other hashed values.
const SEED_SALT: u64 = 0x1f0b_75c2_e4a9_3d68;

/// The palette, which is the sea and the sky the panel is drawn over: a deep
/// water ground, a shoal-blue accent, and text at the brightness of foam rather
/// than at paper white, which over a bright horizon is glare.
const GROUND: egui::Color32 = egui::Color32::from_rgb(9, 17, 24);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(21, 34, 44);
const RAISED: egui::Color32 = egui::Color32::from_rgb(31, 51, 65);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(74, 148, 178);
const TEXT: egui::Color32 = egui::Color32::from_rgb(196, 214, 224);
const DIM_TEXT: egui::Color32 = egui::Color32::from_rgb(132, 155, 168);
/// A rejected parameter set is the one thing on screen that has to be read
/// before anything else is touched.
const FAILURE_COLOUR: egui::Color32 = egui::Color32::from_rgb(230, 120, 90);
/// How much of the scene shows through a panel. Opaque enough to read a slider
/// against a bright sky, open enough that the island is still there behind it.
const PANEL_ALPHA: u8 = 226;
const STRIP_ALPHA: u8 = 210;
const CORNER: u8 = 6;

#[derive(Resource)]
struct Hud {
    visible: bool,
    /// The values the controls edit. Nothing generates from them until
    /// Regenerate, or a preset, is pressed.
    seed: u64,
    options: IslandOptions,
    /// The argument list that reproduces the island on screen, rebuilt when one
    /// lands rather than from the draft, which has usually moved on. The panel
    /// no longer shows it; the copy button puts it on the clipboard.
    command_line: String,
    /// Smoothed frames per second, and the countdown to the next reading.
    fps: f64,
    next_reading: f32,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((EguiPlugin::default(), FrameTimeDiagnosticsPlugin::default()))
            .add_systems(Startup, install)
            .add_systems(
                Update,
                (toggle, close_for_walking, read_frame_rate, report_command_line),
            )
            // The style is settled before anything is drawn with it, and the
            // toggle is drawn whether or not the panel it opens is.
            .add_systems(
                EguiPrimaryContextPass,
                (
                    apply_theme,
                    draw_toggle,
                    draw_panel,
                    draw_generation_strip,
                    draw_frame_rate,
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

/// The draft opens on whatever the command line asked for, so the panel and the
/// island agree from the first frame.
fn install(mut commands: Commands, settings: Res<GenerationSettings>) {
    commands.insert_resource(Hud {
        visible: true,
        seed: settings.seed,
        options: settings.options,
        command_line: options::command_line(settings.seed, &settings.options),
        fps: 0.0,
        next_reading: 0.0,
    });
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

/// The panel owns the pointer whenever it is under it and the keyboard whenever
/// a field is being typed into, and says so here rather than acting on it, so
/// the camera stays the one place movement is decided. Being on screen at all
/// is the third claim, and the one walking answers by giving its captured
/// cursor back. The toggle button claims the pointer the same way the panel
/// does, so a click on it is never also a click into the scene.
fn hand_input_over(wants: Res<EguiWantsInput>, hud: Res<Hud>, mut ui: ResMut<UiFocus>) {
    ui.pointer = wants.wants_any_pointer_input();
    ui.keyboard = wants.wants_any_keyboard_input();
    ui.shown = hud.visible;
}

/// Walking captures the cursor, so a panel left open behind it could never be
/// clicked. Taking to foot closes it, and `H` opens it again and takes the
/// cursor back for as long as it is up: on foot the panel is a pause screen.
fn close_for_walking(mode: Res<CameraMode>, mut hud: ResMut<Hud>) {
    if mode.is_changed() && *mode == CameraMode::Walk {
        hud.visible = false;
    }
}

/// The letter on the key. `KeyCode` names a letter key `KeyH`, and the panel is
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

/// The dark translucent look, installed once on the context that draws it.
///
/// egui's own dark theme is grey on grey with white text and square corners.
/// What replaces it is the two colours the viewer is always over — deep water
/// and shoal blue — a text colour a stop under white so a panel held against a
/// bright horizon does not glare, and enough alpha that the island stays
/// visible through everything drawn on top of it.
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
    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = translucent(GROUND, PANEL_ALPHA);
    visuals.window_fill = translucent(GROUND, PANEL_ALPHA);
    visuals.window_stroke = egui::Stroke::new(1.0, translucent(ACCENT, 90));
    visuals.window_corner_radius = egui::CornerRadius::same(CORNER);
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;
    visuals.selection.bg_fill = translucent(ACCENT, 150);
    visuals.selection.stroke = egui::Stroke::new(1.0, TEXT);
    visuals.hyperlink_color = ACCENT;
    // Four widget states, from the frame around a label to the button under the
    // finger: the fill climbs towards the accent and the outline with it, so a
    // control reads as reachable before it is reached.
    visuals.widgets.noninteractive.bg_fill = translucent(SURFACE, 160);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, translucent(ACCENT, 45));
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, DIM_TEXT);
    visuals.widgets.inactive.bg_fill = translucent(SURFACE, 210);
    visuals.widgets.inactive.weak_bg_fill = translucent(SURFACE, 210);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, translucent(ACCENT, 60));
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.bg_fill = translucent(RAISED, 235);
    visuals.widgets.hovered.weak_bg_fill = translucent(RAISED, 235);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.active.bg_fill = translucent(ACCENT, 200);
    visuals.widgets.active.weak_bg_fill = translucent(ACCENT, 200);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(CORNER / 2);
    }
    // One spacing everywhere, and sliders wide enough that the flag they are
    // labelled with still fits beside them.
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.slider_width = 130.0;
    style.spacing.indent = 12.0;
}

/// egui's colours are premultiplied, so an alpha applied to one has to scale
/// its channels with it.
fn translucent(colour: egui::Color32, alpha: u8) -> egui::Color32 {
    let [red, green, blue, _] = colour.to_srgba_unmultiplied();
    egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

/// The frame every anchored area is drawn in: one fill, one hairline, one
/// radius, so the panel, the strip and the readout are recognisably one set.
fn panel_frame(alpha: u8) -> egui::Frame {
    egui::Frame::NONE
        .fill(translucent(GROUND, alpha))
        .stroke(egui::Stroke::new(1.0, translucent(ACCENT, 90)))
        .corner_radius(CORNER)
        .inner_margin(10.0)
}

/// The one control that is on screen whatever else is, so the panel can be
/// opened and closed with nothing but the mouse. Top left, above the panel it
/// opens and in the same place whether that panel is up or not.
fn draw_toggle(mut contexts: EguiContexts, mut hud: ResMut<Hud>) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let label = if hud.visible { "close" } else { "island" };
    egui::Area::new(egui::Id::new("hud toggle"))
        .anchor(egui::Align2::LEFT_TOP, [MARGIN, MARGIN])
        .show(context, |ui| {
            egui::Frame::NONE
                .fill(translucent(GROUND, STRIP_ALPHA))
                .stroke(egui::Stroke::new(1.0, translucent(ACCENT, 90)))
                .corner_radius(CORNER)
                .inner_margin(egui::Margin::symmetric(3, 1))
                .show(ui, |ui| {
                    // An area is only as wide as what is in it, and a button
                    // told nothing wraps to that width instead of setting it,
                    // which stacks the icon over the word.
                    let button = egui::Button::new(
                        egui::RichText::new(format!("☰  {label}")).color(TEXT),
                    )
                    .wrap_mode(egui::TextWrapMode::Extend)
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE);
                    if ui
                        .add(button)
                        .on_hover_text(format!("{} shows and hides this", key_name(HUD_KEY)))
                        .clicked()
                    {
                        hud.visible = !hud.visible;
                    }
                });
        });
}

/// Everything that edits a draft, in one column under the toggle: the presets,
/// the parameters and the two looks scroll together, and the footer that acts
/// on the draft stays pinned under them.
fn draw_panel(
    mut contexts: EguiContexts,
    mut hud: ResMut<Hud>,
    status: Res<GenerationStatus>,
    mode: Res<CameraMode>,
    mut debug_view: ResMut<DebugView>,
    mut weather: ResMut<Weather>,
    mut requests: MessageWriter<Regenerate>,
) {
    if !hud.visible {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let running = status.is_running();
    let top = MARGIN + TOGGLE_STRIP;
    let body_height = (context.content_rect().height()
        - top
        - MARGIN
        - FOOTER_HEIGHT
        - FRAME_RATE_HEIGHT)
        .max(BODY_MINIMUM_HEIGHT);
    egui::Window::new("island menu")
        .anchor(egui::Align2::LEFT_TOP, [MARGIN, top])
        .default_width(PANEL_WIDTH)
        .max_width(PANEL_WIDTH)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .frame(panel_frame(PANEL_ALPHA))
        .show(context, |ui| {
            ui.set_width(PANEL_WIDTH);
            egui::ScrollArea::vertical()
                .max_height(body_height)
                .show(ui, |ui| {
                    egui::CollapsingHeader::new(heading("Showcase"))
                        .default_open(true)
                        .show(ui, |ui| draw_presets(ui, &mut hud, running, &mut requests));
                    // Open by default with the presets: the two are the panel,
                    // and between them they are taller than any window, which
                    // is what the scroll around them is for. `Look` is two
                    // dropdowns that change nothing about the island, so it is
                    // the one section that starts folded away.
                    egui::CollapsingHeader::new(heading("Parameters"))
                        .default_open(true)
                        .show(ui, |ui| draw_parameters(ui, &mut hud));
                    egui::CollapsingHeader::new(heading("Look"))
                        .default_open(false)
                        .show(ui, |ui| {
                            draw_weather(ui, &mut weather);
                            draw_debug_view(ui, &mut debug_view);
                        });
                });

            ui.separator();
            draw_footer(ui, &mut hud, &status, running, &mut requests);

            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(match *mode {
                    CameraMode::Fly => format!(
                        "{} hides this  ·  {} walks",
                        key_name(HUD_KEY),
                        key_name(WALK_KEY)
                    ),
                    // Walking is holding its cursor here for as long as the
                    // panel is up, so hiding it is also how the walk resumes.
                    CameraMode::Walk => format!(
                        "{} hides this and looks again  ·  {} flies",
                        key_name(HUD_KEY),
                        key_name(WALK_KEY)
                    ),
                })
                .small()
                .color(DIM_TEXT),
            );
        });
}

/// A section title, in the one place the panel uses one.
fn heading(text: &str) -> egui::RichText {
    egui::RichText::new(text).heading().size(15.0).color(TEXT)
}

/// What the panel does to the island on screen: rebuild it from the draft, or
/// hand the arguments that reproduce it to the clipboard. Under them, what the
/// island on screen was built from, and any failure, which is the one line that
/// has to be read before anything else is touched.
fn draw_footer(
    ui: &mut egui::Ui,
    hud: &mut Hud,
    status: &GenerationStatus,
    running: bool,
    requests: &mut MessageWriter<Regenerate>,
) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!running, egui::Button::new("Regenerate"))
            .clicked()
        {
            requests.write(Regenerate {
                seed: hud.seed,
                options: hud.options,
            });
        }
        if ui
            .button("Copy command")
            .on_hover_text("the full argument list that reproduces the island on screen")
            .clicked()
        {
            ui.ctx().copy_text(hud.command_line.clone());
        }
    });
    ui.label(egui::RichText::new(status_line(status)).small().color(DIM_TEXT));
    if let Some(failure) = &status.failure {
        ui.colored_label(FAILURE_COLOUR, failure);
    }
}

/// The curated islands, two to a row. A click fills the draft in and asks for
/// the rebuild in the same frame: the point of a preset is that it is one
/// press, and the cache makes a second visit to one instant.
fn draw_presets(
    ui: &mut egui::Ui,
    hud: &mut Hud,
    running: bool,
    requests: &mut MessageWriter<Regenerate>,
) {
    ui.label(
        egui::RichText::new("ten islands at the size you are already running")
            .small()
            .color(DIM_TEXT),
    );
    egui::Grid::new("presets")
        .num_columns(PRESET_COLUMNS)
        .spacing([6.0, 6.0])
        .show(ui, |ui| {
            for (index, preset) in PRESETS.iter().enumerate() {
                let selected = hud.seed == preset.seed
                    && hud.options == preset.options(hud.options.terrain_size);
                let button = egui::Button::new(preset.name)
                    .selected(selected)
                    .min_size(egui::vec2(PRESET_BUTTON_WIDTH, 0.0));
                if ui
                    .add_enabled(!running, button)
                    .on_hover_text(preset.character)
                    .clicked()
                {
                    hud.seed = preset.seed;
                    hud.options = preset.options(hud.options.terrain_size);
                    requests.write(Regenerate {
                        seed: hud.seed,
                        options: hud.options,
                    });
                }
                if index % PRESET_COLUMNS == PRESET_COLUMNS - 1 {
                    ui.end_row();
                }
            }
        });
}

fn draw_parameters(ui: &mut egui::Ui, hud: &mut Hud) {
    ui.horizontal(|ui| {
        ui.label("seed");
        ui.add(egui::DragValue::new(&mut hud.seed).speed(1.0));
        if ui.button("-").clicked() {
            hud.seed = hud.seed.wrapping_sub(1);
        }
        if ui.button("+").clicked() {
            hud.seed = hud.seed.wrapping_add(1);
        }
        if ui.button("random").clicked() {
            hud.seed = random_seed();
        }
    });

    for group in Group::ALL {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(group.title()).strong().color(ACCENT));
        // The one parameter that is not an `f32` and so not in the table.
        if group == Group::Terrain {
            ui.add(
                egui::Slider::new(&mut hud.options.terrain_size, TERRAIN_SIZE_RANGE)
                    .logarithmic(true)
                    .text(&TERRAIN_SIZE_FLAG[2..]),
            );
        }
        for parameter in PARAMETERS.iter().filter(|entry| entry.group == group) {
            let value = (parameter.field)(&mut hud.options);
            ui.add(
                egui::Slider::new(value, parameter.minimum..=parameter.maximum)
                    .logarithmic(parameter.logarithmic)
                    .fixed_decimals(decimals(parameter.maximum))
                    .text(parameter.label()),
            );
        }
    }
    // A maximum dragged under its own source would only fail once the generator
    // ran, so it is put back straight away and the slider shows where it
    // actually landed.
    options::reconcile(&mut hud.options);
}

/// The same list `--weather` takes, so a look found here is captured by name.
/// Nothing regenerates: a look moves the sun, the air, the cloud and the mist,
/// and the island under all of it is the same island.
fn draw_weather(ui: &mut egui::Ui, selected: &mut Weather) {
    egui::ComboBox::from_label("weather")
        .selected_text(selected.label())
        .show_ui(ui, |ui| {
            for look in Weather::all() {
                ui.selectable_value(selected, look, look.label());
            }
        });
}

/// The same list `--debug-view` takes, so a channel found here is captured by
/// name. Nothing regenerates: the selection is a uniform the surfaces read, and
/// the island under it does not change.
fn draw_debug_view(ui: &mut egui::Ui, selected: &mut DebugView) {
    egui::ComboBox::from_label("debug view")
        .selected_text(selected.label())
        .show_ui(ui, |ui| {
            for view in DebugView::ALL {
                ui.selectable_value(selected, view, view.label());
            }
        });
}

/// Top centre, and only while there is something to say: a build in flight, or
/// the failure the last one ended in. A rebuild leaves the island on screen and
/// takes seconds to minutes, so the one thing the strip is for is that the
/// viewer never looks idle while it is working — including with the panel shut,
/// which is why this is not part of it.
fn draw_generation_strip(mut contexts: EguiContexts, status: Res<GenerationStatus>) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let notice = if let Some(elapsed) = status.elapsed {
        (format!("generating island   {elapsed:.0} s"), TEXT)
    } else if let Some(failure) = &status.failure {
        (format!("generation failed   {failure}"), FAILURE_COLOUR)
    } else {
        return;
    };
    egui::Area::new(egui::Id::new("generation strip"))
        .anchor(egui::Align2::CENTER_TOP, [0.0, MARGIN])
        .movable(false)
        .interactable(false)
        .show(context, |ui| {
            panel_frame(STRIP_ALPHA)
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show(ui, |ui| {
                    let (text, colour) = notice;
                    ui.add(
                        egui::Label::new(egui::RichText::new(text).color(colour))
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
}

fn status_line(status: &GenerationStatus) -> String {
    match (status.built, status.took) {
        (Some((seed, options)), Some(took)) => format!(
            "seed {seed} at terrain size {} built in {}",
            options.terrain_size,
            duration(took)
        ),
        (Some((seed, options)), None) => {
            format!("seed {seed} at terrain size {}", options.terrain_size)
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
/// of something already known. A seed nobody chose is the one value that has to
/// come from outside, so the clock supplies it.
fn random_seed() -> u64 {
    #[allow(clippy::cast_possible_truncation)]
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);
    mix(nanos, SEED_SALT)
}

/// Bottom left, small and dimmed: the corner the panel above it stops short of.
/// Shown whether or not the panel is.
/// The frame rate, and under it what the culling stages left for that frame to
/// draw. The census is the same one a capture writes into its log, so a pose
/// found by flying can be read off the screen and looked up again later.
fn draw_frame_rate(mut contexts: EguiContexts, hud: Res<Hud>, budget: Res<RenderBudget>) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    egui::Area::new(egui::Id::new("frame rate"))
        .anchor(egui::Align2::LEFT_BOTTOM, [MARGIN, -MARGIN])
        .movable(false)
        .interactable(false)
        .show(context, |ui| {
            panel_frame(STRIP_ALPHA)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    // The area is only as wide as what is in it, so the label
                    // has to be told to set that width rather than wrap to it.
                    for line in [
                        format!("{:.0} fps", hud.fps),
                        format!(
                            "terrain {}/{} chunks, {} kv",
                            budget.terrain.drawn_entities,
                            budget.terrain.entities,
                            budget.terrain.drawn_vertices / 1_000
                        ),
                        format!(
                            "scatter {}/{} instances",
                            budget.scatter.drawn_entities, budget.scatter.entities
                        ),
                    ] {
                        ui.add(
                            egui::Label::new(egui::RichText::new(line).small().color(DIM_TEXT))
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    }
                });
        });
}
