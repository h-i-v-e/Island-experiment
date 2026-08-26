//! Validation diagnostics and JSON-pointer navigation.

use bevy::prelude::Resource;
use bevy_egui::egui;
use motu::procedural_textures::{Diagnostic, MaterialLayer, TextureRecipe};

use super::UiAction;

/// Diagnostics retained by the UI between validation requests.
#[derive(Clone, Debug, Default, Resource)]
pub struct DiagnosticsState {
    pub entries: Vec<Diagnostic>,
    pub revision: u64,
}

/// Draws validation issues and returns selection/focus intents.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    diagnostics: &[Diagnostic],
    recipe: &TextureRecipe,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.heading("Diagnostics");
    if diagnostics.is_empty() {
        ui.colored_label(egui::Color32::LIGHT_GREEN, "Recipe is valid");
        return actions;
    }
    ui.label(format!("{} issue(s)", diagnostics.len()));
    for diagnostic in diagnostics {
        let colour = match diagnostic.severity {
            "error" => egui::Color32::LIGHT_RED,
            "warning" => egui::Color32::YELLOW,
            _ => egui::Color32::LIGHT_BLUE,
        };
        let label = format!(
            "{}  {}: {}",
            diagnostic.severity, diagnostic.code, diagnostic.message
        );
        if ui
            .selectable_label(false, egui::RichText::new(label).color(colour))
            .on_hover_text(&diagnostic.pointer)
            .clicked()
        {
            let layer = layer_for_pointer(&diagnostic.pointer, &recipe.layers);
            if let Some(layer_id) = layer {
                actions.push(UiAction::SelectLayer(Some(layer_id)));
            }
            actions.push(UiAction::FocusPointer(diagnostic.pointer.clone()));
        }
        ui.label(egui::RichText::new(&diagnostic.pointer).small().monospace());
    }
    actions
}

fn layer_for_pointer(pointer: &str, layers: &[MaterialLayer]) -> Option<String> {
    let mut segments = pointer.trim_start_matches('/').split('/');
    if segments.next()? != "layers" {
        return None;
    }
    let index = segments.next()?.parse::<usize>().ok()?;
    layers.get(index).map(|layer| layer.id.clone())
}
