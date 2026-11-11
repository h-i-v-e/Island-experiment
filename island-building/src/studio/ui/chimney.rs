use super::{
    DIM, dialogs,
    inspector::{material_path, scalar},
};
use crate::{BuildingRecipe, ChimneyRecipe, Facade, studio::StudioState};
use bevy_egui::egui;

pub(super) fn show(ui: &mut egui::Ui, draft: &mut BuildingRecipe, state: &mut StudioState) {
    ui.heading("Chimney & fireplace");
    ui.label(
        egui::RichText::new("A masonry stack outside and an unlit fireplace inside.")
            .small()
            .color(DIM),
    );
    let mut enabled = draft.chimney.is_some();
    if ui
        .checkbox(&mut enabled, "Include chimney and fireplace")
        .changed()
    {
        draft.chimney = enabled.then(ChimneyRecipe::default);
    }
    if let Some(chimney) = &mut draft.chimney {
        placement(ui, chimney);
        firebox(ui, chimney);
        ui.separator();
        ui.label("Hearth · fixed obstacle");
        scalar(
            ui,
            "Projection beyond breast",
            &mut chimney.hearth_depth,
            0.025,
            " m",
        );
        scalar(ui, "Hearth height", &mut chimney.hearth_height, 0.01, " m");
        ui.separator();
        ui.label("Roof stack");
        scalar(ui, "Stack width", &mut chimney.stack_width, 0.025, " m");
        scalar(ui, "Stack depth", &mut chimney.stack_depth, 0.025, " m");
        scalar(
            ui,
            "Height above roof",
            &mut chimney.height_above_roof,
            0.05,
            " m",
        );
        ui.label(egui::RichText::new("The exterior stack remains at every LOD. The firebox and hearth appear at LOD0; their collision remains resident.").small().color(DIM));
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.color_edit_button_rgb(&mut draft.materials.chimney);
        ui.label("Masonry colour");
    });
    material_path(
        ui,
        "Chimney material",
        &mut draft.materials.chimney_recipe,
        state,
        dialogs::MaterialTarget::Chimney,
    );
    ui.label(
        egui::RichText::new("Clear the selected JSON to restore Coursed stone.")
            .small()
            .color(DIM),
    );
}

fn placement(ui: &mut egui::Ui, chimney: &mut ChimneyRecipe) {
    ui.separator();
    egui::ComboBox::from_label("Wall")
        .selected_text(format!("{:?}", chimney.facade))
        .show_ui(ui, |ui| {
            for facade in Facade::ALL {
                ui.selectable_value(&mut chimney.facade, facade, format!("{facade:?}"));
            }
        });
    scalar(ui, "Offset along wall", &mut chimney.offset, 0.05, " m");
    scalar(ui, "Fireplace width", &mut chimney.width, 0.025, " m");
    scalar(ui, "Breast depth", &mut chimney.depth, 0.025, " m");
}

fn firebox(ui: &mut egui::Ui, chimney: &mut ChimneyRecipe) {
    ui.separator();
    ui.label("Open firebox · no fire or smoke");
    scalar(ui, "Firebox width", &mut chimney.firebox_width, 0.025, " m");
    scalar(
        ui,
        "Firebox height",
        &mut chimney.firebox_height,
        0.025,
        " m",
    );
    scalar(ui, "Firebox depth", &mut chimney.firebox_depth, 0.025, " m");
}
