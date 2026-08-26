//! Command bar and bake controls for the material studio.

#![allow(clippy::too_many_lines)]

use bevy_egui::egui;
use motu::procedural_textures::OutputProfile;

use crate::{
    bake::BakeState,
    preview::{PreviewResolution, PreviewState},
};

use super::{DocumentStatus, UiAction, UiState};

/// Draw the fixed command bar.  Commands are returned to the application
/// layer so file dialogs and dirty-document policy stay outside egui.
pub(crate) fn draw(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    state: &mut UiState,
    document: DocumentStatus<'_>,
    preview: &PreviewState,
    bake: &mut BakeState,
) -> Vec<UiAction> {
    let mut actions = keyboard_actions(ctx);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Material Studio").strong());
        ui.separator();
        command_button(ui, "New", "Ctrl/Cmd+N", UiAction::New, &mut actions);
        command_button(ui, "Open", "Ctrl/Cmd+O", UiAction::Open, &mut actions);
        command_button(ui, "Save", "Ctrl/Cmd+S", UiAction::Save, &mut actions);
        command_button(
            ui,
            "Save As",
            "Ctrl/Cmd+Shift+S",
            UiAction::SaveAs,
            &mut actions,
        );
        command_button(
            ui,
            "Revert",
            "Reload saved file",
            UiAction::Revert,
            &mut actions,
        );
        ui.separator();
        if ui
            .add_enabled(document.can_undo, egui::Button::new("Undo"))
            .on_hover_text("Undo the last recipe transaction (Ctrl/Cmd+Z)")
            .clicked()
        {
            actions.push(UiAction::Undo);
        }
        if ui
            .add_enabled(document.can_redo, egui::Button::new("Redo"))
            .on_hover_text("Redo the last recipe transaction (Ctrl/Cmd+Shift+Z)")
            .clicked()
        {
            actions.push(UiAction::Redo);
        }
        ui.separator();
        if ui
            .checkbox(&mut state.auto_preview, "Auto preview")
            .on_hover_text("Generate a preview 300 ms after a committed edit")
            .changed()
        {
            actions.push(UiAction::SetAutoPreview(state.auto_preview));
        }
        egui::ComboBox::from_id_salt("preview-resolution")
            .selected_text(format!("{} px", preview.resolution.pixels()))
            .show_ui(ui, |ui| {
                for resolution in [
                    PreviewResolution::Small,
                    PreviewResolution::Medium,
                    PreviewResolution::Large,
                ] {
                    if ui
                        .selectable_value(
                            &mut state.pending_resolution,
                            resolution,
                            format!("{} px", resolution.pixels()),
                        )
                        .changed()
                    {
                        actions.push(UiAction::SetPreviewResolution(resolution));
                    }
                }
            });
        if ui.button("Validate").clicked() {
            actions.push(UiAction::Validate);
        }
        if ui
            .button("Preview")
            .on_hover_text("Generate now (F5 or Ctrl/Cmd+Enter)")
            .clicked()
        {
            actions.push(UiAction::Preview);
        }
        if ui.button("Bake").clicked() {
            actions.push(UiAction::Bake);
        }
        ui.separator();
        let dirty = if document.dirty { "*" } else { "" };
        ui.label(format!("{}{}", document.name, dirty))
            .on_hover_text(document.path.unwrap_or("Unsaved document"));
        if preview.is_running() {
            ui.spinner();
        }
    });

    ui.collapsing("Bake options", |ui| {
        ui.horizontal(|ui| {
            ui.label("Output directory");
            ui.text_edit_singleline(&mut bake.output_directory);
        });
        egui::ComboBox::from_label("Profile")
            .selected_text(match bake.profile {
                OutputProfile::Separate => "Separate maps",
                OutputProfile::MotuUnityTerrain => "Motu Unity terrain",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut bake.profile, OutputProfile::Separate, "Separate maps");
                ui.selectable_value(
                    &mut bake.profile,
                    OutputProfile::MotuUnityTerrain,
                    "Motu Unity terrain",
                );
            });
        ui.checkbox(
            &mut bake.overwrite,
            "Replace an existing generated set (explicit overwrite)",
        );
        if ui
            .add_enabled(
                !bake.is_running(),
                egui::Button::new("Run full-resolution bake"),
            )
            .clicked()
        {
            actions.push(UiAction::Bake);
        }
        ui.label(&bake.status);
        if let Some(error) = &bake.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    });
    actions
}

fn command_button(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    action: UiAction,
    actions: &mut Vec<UiAction>,
) {
    if ui.button(label).on_hover_text(hint).clicked() {
        actions.push(action);
    }
}

fn keyboard_actions(ctx: &egui::Context) -> Vec<UiAction> {
    if ctx.egui_wants_keyboard_input() {
        return Vec::new();
    }
    ctx.input(|input| {
        let command = input.modifiers.command || input.modifiers.ctrl;
        let shift = input.modifiers.shift;
        let mut actions = Vec::new();
        if command && input.key_pressed(egui::Key::N) {
            actions.push(UiAction::New);
        }
        if command && input.key_pressed(egui::Key::O) {
            actions.push(UiAction::Open);
        }
        if command && input.key_pressed(egui::Key::S) {
            actions.push(if shift {
                UiAction::SaveAs
            } else {
                UiAction::Save
            });
        }
        if command && input.key_pressed(egui::Key::Z) {
            actions.push(if shift {
                UiAction::Redo
            } else {
                UiAction::Undo
            });
        }
        if input.key_pressed(egui::Key::F5) || (command && input.key_pressed(egui::Key::Enter)) {
            actions.push(UiAction::Preview);
        }
        actions
    })
}
