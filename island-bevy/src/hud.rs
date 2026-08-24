//! The parameter panel and the frame-rate readout.
//!
//! The panel edits a draft of the generator's inputs and hands the whole draft
//! over at once, on the Regenerate button, because a build takes seconds to
//! minutes and a slider dragged across its range would otherwise ask for
//! hundreds of them. The island on screen stays up until the new one lands.
//!
//! Both are drawn with `bevy_egui`, and neither plugin is installed under
//! `--screenshot`, so no capture can carry either of them whatever the panel's
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
    camera::{CameraMode, UiHasInput, WALK_KEY},
    hash::mix,
    island_gen::{GenerationSettings, GenerationStatus, IslandReady, Regenerate},
    options::{self, Group, PARAMETERS, TERRAIN_SIZE_FLAG, TERRAIN_SIZE_RANGE},
};

/// Shows and hides the parameter panel.
pub const HUD_KEY: KeyCode = KeyCode::KeyH;

/// How often the frame-rate readout takes a new number. Twice a second is
/// slow enough to read and fast enough to still be a frame rate.
const FPS_INTERVAL: f32 = 0.5;
const PANEL_WIDTH: f32 = 380.0;
const PANEL_MARGIN: f32 = 12.0;
/// Room kept under the parameters for the status line, the button, the command
/// line and the key reminder. The parameters scroll inside whatever the window
/// leaves above it, so the controls that act on them are never the part that
/// runs off the bottom.
const FOOTER_HEIGHT: f32 = 190.0;
/// The parameters keep at least this much, however short the window is.
const PARAMETERS_MINIMUM_HEIGHT: f32 = 120.0;
/// The command line runs to fifteen flags and wraps to several lines, so it
/// scrolls in a box of its own rather than setting the panel's height.
const COMMAND_LINE_HEIGHT: f32 = 70.0;

/// Distinguishes the randomized seed from the crate's other hashed values.
const SEED_SALT: u64 = 0x1f0b_75c2_e4a9_3d68;
/// A rejected parameter set is the one thing on the panel that has to be read
/// before anything else is touched.
const FAILURE_COLOUR: egui::Color32 = egui::Color32::from_rgb(230, 120, 90);

#[derive(Resource)]
struct Hud {
    visible: bool,
    /// The values the controls edit. Nothing generates from them until
    /// Regenerate is pressed.
    seed: u64,
    options: IslandOptions,
    /// The argument list that reproduces the island on screen, rebuilt when one
    /// lands rather than from the draft, which has usually moved on.
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
            .add_systems(Update, (toggle, read_frame_rate, report_command_line))
            .add_systems(EguiPrimaryContextPass, (draw_panel, draw_frame_rate))
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

fn toggle(keys: Res<ButtonInput<KeyCode>>, ui: Res<UiHasInput>, mut hud: ResMut<Hud>) {
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
/// the camera stays the one place movement is decided.
fn hand_input_over(wants: Res<EguiWantsInput>, mut ui: ResMut<UiHasInput>) {
    ui.pointer = wants.wants_any_pointer_input();
    ui.keyboard = wants.wants_any_keyboard_input();
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

fn draw_panel(
    mut contexts: EguiContexts,
    mut hud: ResMut<Hud>,
    status: Res<GenerationStatus>,
    mode: Res<CameraMode>,
    mut requests: MessageWriter<Regenerate>,
) {
    if !hud.visible {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let running = status.is_running();
    let height = (context.content_rect().height() - 2.0 * PANEL_MARGIN - FOOTER_HEIGHT)
        .max(PARAMETERS_MINIMUM_HEIGHT);
    egui::Window::new("Island parameters")
        .anchor(egui::Align2::RIGHT_TOP, [-PANEL_MARGIN, PANEL_MARGIN])
        .default_width(PANEL_WIDTH)
        .resizable(false)
        .collapsible(false)
        .show(context, |ui| {
            egui::ScrollArea::vertical()
                .max_height(height)
                .show(ui, |ui| draw_parameters(ui, &mut hud));

            ui.separator();
            ui.label(status_line(&status));
            if let Some(failure) = &status.failure {
                ui.colored_label(FAILURE_COLOUR, failure);
            }
            if ui
                .add_enabled(!running, egui::Button::new("Regenerate"))
                .clicked()
            {
                requests.write(Regenerate {
                    seed: hud.seed,
                    options: hud.options,
                });
            }

            ui.separator();
            draw_command_line(ui, &hud.command_line);

            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "{} hides this panel  ·  {} switches to {}",
                    key_name(HUD_KEY),
                    key_name(WALK_KEY),
                    match *mode {
                        CameraMode::Fly => "walking",
                        CameraMode::Walk => "flying",
                    }
                ))
                .small()
                .weak(),
            );
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
        ui.add_space(6.0);
        ui.label(egui::RichText::new(group.title()).strong());
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

fn draw_command_line(ui: &mut egui::Ui, line: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("on screen").small().weak());
        if ui.small_button("copy").clicked() {
            ui.ctx().copy_text(String::from(line));
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("command line")
        .max_height(COMMAND_LINE_HEIGHT)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(line).small().monospace().weak());
        });
}

fn status_line(status: &GenerationStatus) -> String {
    if let Some(elapsed) = status.elapsed {
        return format!("generating... {elapsed:.0} s");
    }
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

/// Bottom left, small and dimmed, and clear of the loading notice in the
/// opposite corner. Shown whether or not the panel is.
fn draw_frame_rate(mut contexts: EguiContexts, hud: Res<Hud>) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    egui::Area::new(egui::Id::new("frame rate"))
        .anchor(egui::Align2::LEFT_BOTTOM, [PANEL_MARGIN, -PANEL_MARGIN])
        .movable(false)
        .interactable(false)
        .show(context, |ui| {
            // A backing wash rather than a bright colour: the readout has to
            // stay legible over both the sky and the water without becoming
            // something the eye goes to.
            egui::Frame::NONE
                .fill(egui::Color32::from_black_alpha(70))
                .corner_radius(3.0)
                .inner_margin(4.0)
                .show(ui, |ui| {
                    // The area is only as wide as what is in it, so the label
                    // has to be told to set that width rather than wrap to it.
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("{:.0} fps", hud.fps))
                                .small()
                                .color(egui::Color32::from_white_alpha(150)),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
}
