mod chimney;
pub(super) mod dialogs;
mod inspector;
use super::{PendingAction, StudioState, camera::CameraControls};
use crate::interaction::BuildingInstance;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

const PANEL: egui::Color32 = egui::Color32::from_rgb(7, 14, 21);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(76, 196, 228);
const DIM: egui::Color32 = egui::Color32::from_rgb(126, 148, 163);

#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub(super) fn draw(
    mut contexts: EguiContexts,
    mut state: ResMut<StudioState>,
    mut controls: ResMut<CameraControls>,
    mut instances: Query<&mut BuildingInstance>,
    mut themed: Local<bool>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    if !*themed {
        install_theme(context);
        *themed = true;
    }
    dialogs::poll(&mut state);
    shortcuts(context, &mut state);
    let mut viewport_ui = egui::Ui::new(
        context.clone(),
        egui::Id::new("building-viewport"),
        egui::UiBuilder::new()
            .max_rect(context.viewport_rect())
            .layer_id(egui::LayerId::background()),
    );
    toolbar(&mut viewport_ui, &mut state);
    status(&mut viewport_ui, &state);
    inspector::show(&mut viewport_ui, context, &mut state);
    inspection(context, &mut state, &mut controls, &mut instances);
    dirty_modal(context, &mut state);
}

fn install_theme(context: &egui::Context) {
    context.set_theme(egui::ThemePreference::Dark);
    context.all_styles_mut(|style| {
        style.visuals.override_text_color = Some(egui::Color32::from_rgb(226, 234, 241));
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = PANEL.gamma_multiply(0.95);
        style.visuals.window_stroke = egui::Stroke::new(1., ACCENT.gamma_multiply(0.25));
        style.visuals.window_corner_radius = egui::CornerRadius::same(8);
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
        style.visuals.slider_trailing_fill = true;
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(15, 27, 37);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(25, 42, 55);
        style.visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.7);
        style.spacing.item_spacing = egui::vec2(8., 7.);
    });
}

fn brand_bar(ui: &mut egui::Ui, state: &StudioState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("ISLAND  /  BUILDING STUDIO")
                .color(ACCENT)
                .strong()
                .size(15.),
        );
        ui.separator();
        let title = state
            .document
            .source_path()
            .and_then(|path| path.file_name())
            .map_or_else(|| "Untitled recipe".into(), |name| name.to_string_lossy());
        ui.label(format!(
            "{title}{}",
            if state.document.is_dirty() {
                "  •"
            } else {
                ""
            }
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new("METRES · Z-UP RECIPE · THREE LODS")
                    .small()
                    .color(DIM),
            );
        });
    });
}

fn toolbar(viewport: &mut egui::Ui, state: &mut StudioState) {
    egui::Panel::top("building-toolbar")
        .frame(egui::Frame::new().fill(PANEL).inner_margin(12))
        .show(viewport, |ui| {
            brand_bar(ui, state);
            ui.horizontal(|ui| {
                ui.menu_button("New", |ui| {
                    for (name, label) in [
                        ("hut", "Coastal hut"),
                        ("cottage", "Island cottage"),
                        ("workshop", "Harbour workshop"),
                    ] {
                        if ui.button(label).clicked() {
                            request_action(state, PendingAction::New(name.into()));
                            ui.close();
                        }
                    }
                });
                if ui.button("Open…").clicked() {
                    dialogs::start(state, dialogs::Intent::Open);
                }
                if ui.button("Save").clicked() {
                    save(state, false, false);
                }
                if ui.button("Save As…").clicked() {
                    save(state, true, false);
                }
                if ui
                    .add_enabled(
                        state.document.source_path().is_some(),
                        egui::Button::new("Revert"),
                    )
                    .clicked()
                {
                    request_action(state, PendingAction::Revert);
                }
                ui.separator();
                if ui
                    .add_enabled(state.document.can_undo(), egui::Button::new("Undo"))
                    .clicked()
                {
                    state.document.undo();
                }
                if ui
                    .add_enabled(state.document.can_redo(), egui::Button::new("Redo"))
                    .clicked()
                {
                    state.document.redo();
                }
                if ui.button("Validate").clicked() {
                    match crate::validate_recipe(state.document.recipe()) {
                        Ok(()) => {
                            state.error = None;
                            state.status =
                                "Recipe valid · entrance and room connectivity checked".into();
                        }
                        Err(error) => state.error = Some(error.to_string()),
                    }
                }
                ui.separator();
                if ui
                    .add_enabled(
                        state.export_task.is_none(),
                        egui::Button::new("Bake / Export…")
                            .fill(egui::Color32::from_rgb(25, 67, 78)),
                    )
                    .clicked()
                {
                    choose_export(state);
                }
                if state.running.is_some() || state.export_task.is_some() {
                    ui.spinner();
                }
            });
        });
}

fn status(viewport: &mut egui::Ui, state: &StudioState) {
    egui::Panel::bottom("building-status")
        .frame(egui::Frame::new().fill(PANEL).inner_margin(10))
        .show(viewport, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ACCENT, "●");
                ui.label(&state.status);
                ui.separator();
                ui.colored_label(
                    DIM,
                    "RMB orbit / look · scroll zoom · WASD walk · E door · Esc leave walk",
                );
            });
            if let Some(error) = &state.error {
                ui.colored_label(egui::Color32::from_rgb(238, 148, 105), error);
            }
        });
}

fn camera_buttons(ui: &mut egui::Ui, state: &StudioState, controls: &mut CameraControls) {
    let prototype = state.last_good.as_ref().map(std::sync::Arc::clone);
    ui.horizontal(|ui| {
        if ui
            .selectable_label(!controls.walkthrough, "Orbit")
            .clicked()
        {
            controls.walkthrough = false;
        }
        if ui
            .selectable_label(controls.walkthrough, "Walkthrough")
            .clicked()
            && let Some(prototype) = &prototype
        {
            controls.enter_walkthrough(prototype);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for (name, label) in [
            ("exterior", "Overview"),
            ("entrance", "Entrance"),
            ("interior", "Interior"),
            ("south", "S"),
            ("east", "E"),
            ("north", "N"),
            ("west", "W"),
            ("roof-end", "Roof end"),
            ("window", "Window"),
            ("roof-under", "Roof under"),
            ("eaves", "Eaves"),
        ] {
            if ui.small_button(label).clicked()
                && let Some(prototype) = &prototype
            {
                controls.frame(prototype, name);
            }
        }
        if ui
            .add_enabled(
                prototype
                    .as_ref()
                    .is_some_and(|prototype| prototype.recipe.chimney.is_some()),
                egui::Button::new("Fireplace").small(),
            )
            .clicked()
            && let Some(prototype) = &prototype
        {
            controls.frame(prototype, "fireplace");
        }
    });
}

fn instance_status(
    ui: &mut egui::Ui,
    state: &StudioState,
    instances: &mut Query<&mut BuildingInstance>,
) {
    if let Some(root) = state.root
        && let Ok(mut instance) = instances.get_mut(root)
    {
        ui.separator();
        let ids: Vec<_> = instance.doors.keys().cloned().collect();
        for id in ids {
            let open = instance.doors[&id].target.abs() > 0.01;
            if ui
                .button(format!("{} {id}", if open { "Close" } else { "Open" }))
                .clicked()
            {
                instance.toggle_door(&id);
            }
        }
        ui.separator();
        let level = instance.current_lod;
        let source = &instance.prototype.source;
        ui.colored_label(
            ACCENT,
            format!(
                "LOD{level}  ·  {} triangles",
                source.lods[level].triangle_count()
                    + if level == 0 {
                        source
                            .doors
                            .iter()
                            .map(|d| d.mesh.triangle_count() + d.hardware_mesh.triangle_count())
                            .sum::<usize>()
                    } else {
                        0
                    }
            ),
        );
        ui.label(format!(
            "{} material draws · {:.1} MiB textures",
            source.lods[level].parts.len()
                + if level == 0 {
                    source.doors.len()
                        + source
                            .doors
                            .iter()
                            .filter(|door| !door.hardware_mesh.indices.is_empty())
                            .count()
                } else {
                    0
                },
            f64::from(u32::try_from(instance.prototype.texture_bytes).unwrap_or(u32::MAX))
                / 1_048_576.
        ));
        ui.label(format!(
            "{} rooms · {} colliders",
            source.layout.room_count,
            source.colliders.len()
        ));
    }
}

fn inspection(
    context: &egui::Context,
    state: &mut StudioState,
    controls: &mut CameraControls,
    instances: &mut Query<&mut BuildingInstance>,
) {
    egui::Window::new("INSPECTION")
        .anchor(egui::Align2::RIGHT_TOP, [-18., 110.])
        .default_width(232.)
        .resizable(false)
        .collapsible(false)
        .show(context, |ui| {
            camera_buttons(ui, state, controls);
            ui.separator();
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.automatic, true, "Auto");
                for level in 0..3 {
                    if ui
                        .selectable_label(
                            !state.automatic && state.lod == level,
                            format!("LOD{level}"),
                        )
                        .clicked()
                    {
                        state.automatic = false;
                        state.lod = level;
                    }
                }
            });
            ui.label(
                egui::RichText::new("Near openings and occupants pin LOD0")
                    .small()
                    .color(DIM),
            );
            instance_status(ui, state, instances);
            ui.label(format!(
                "Build {:.0} ms · bake {:.0} ms",
                state.build_seconds * 1000.,
                state.bake_seconds * 1000.
            ));
            ui.separator();
            ui.checkbox(&mut state.collision_overlay, "Collision overlay");
            ui.checkbox(&mut state.wireframe, "Wireframe");
            if ui
                .checkbox(&mut state.cutaway, "Roof / ceiling cutaway")
                .changed()
            {
                state.recompile = true;
                controls.walkthrough = false;
            }
            ui.add(egui::Slider::new(&mut state.light, 0.0..=40_000.).text("Sun"));
            ui.add(egui::Slider::new(&mut state.ambient, 0.0..=1500.).text("Fill"));
            ui.add(egui::Slider::new(&mut state.room_light, 0.0..=160_000.).text("Room"));
            ui.add(egui::Slider::new(&mut state.exposure, 5.0..=15.).text("Exposure"));
        });
}
fn request_action(state: &mut StudioState, action: PendingAction) {
    if state.file_dialog.is_some() {
        return;
    }
    if state.document.is_dirty() {
        state.pending_action = Some(action);
    } else {
        state.apply_action(action);
    }
}
fn save(state: &mut StudioState, save_as: bool, continue_pending: bool) {
    if state.file_dialog.is_some() {
        return;
    }
    if save_as || state.document.source_path().is_none() {
        dialogs::start(state, dialogs::Intent::SaveAs { continue_pending });
    } else {
        let result = state.document.save().map_err(|error| error.to_string());
        dialogs::finish_save(state, result, continue_pending);
    }
}

fn choose_export(state: &mut StudioState) {
    dialogs::start(state, dialogs::Intent::Export);
}

fn dirty_modal(context: &egui::Context, state: &mut StudioState) {
    if state.pending_action.is_none() || state.file_dialog.is_some() {
        return;
    }
    egui::Window::new("Save changes?")
        .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
        .collapsible(false)
        .resizable(false)
        .show(context, |ui| {
            ui.label("This recipe has unsaved changes.");
            ui.horizontal(|ui| {
                if ui.button("Save and continue").clicked() {
                    save(state, false, true);
                }
                if ui.button("Discard changes").clicked()
                    && let Some(action) = state.pending_action.take()
                {
                    state.apply_action(action);
                }
                if ui.button("Cancel").clicked() {
                    state.pending_action = None;
                }
            });
        });
}
fn shortcuts(context: &egui::Context, state: &mut StudioState) {
    if context.egui_wants_keyboard_input() || state.file_dialog.is_some() {
        return;
    }
    context.input(|input| {
        if input.modifiers.command && input.key_pressed(egui::Key::S) {
            save(state, input.modifiers.shift, false);
        }
        if input.modifiers.command && input.key_pressed(egui::Key::Z) {
            if input.modifiers.shift {
                state.document.redo();
            } else {
                state.document.undo();
            }
        }
    });
}
