use super::units::Dimensions;
use super::{
    DIM, dialogs,
    inspector::{material_path, scalar},
};
use crate::{BuildingRecipe, ChimneyRecipe, Facade, studio::StudioState};
use bevy_egui::egui;

pub(super) fn show(
    ui: &mut egui::Ui,
    draft: &mut BuildingRecipe,
    state: &mut StudioState,
    dimensions: Dimensions,
) {
    ui.heading("Chimney & fireplace");
    ui.label(
        egui::RichText::new("One masonry stack with an unlit fireplace on each floor.")
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
    let lengths = Facade::ALL.map(|f| f.length(draft));
    let roof_top = crate::geometry::chimney::stack_bounds(draft)
        .zip(draft.chimney.as_ref())
        .map_or(0., |(bounds, c)| bounds.max[2] - c.height_above_roof);
    if let Some(chimney) = &mut draft.chimney {
        placement(ui, chimney, dimensions, lengths);
        firebox(ui, chimney, dimensions);
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
        dimensions.length(ui, "Stack width", &mut chimney.stack_width, 0);
        dimensions.length(ui, "Stack depth", &mut chimney.stack_depth, 0);
        let mut top = roof_top + chimney.height_above_roof;
        if dimensions.coordinate(ui, "Stack top from floor", &mut top, 1, 0.) {
            chimney.height_above_roof = top - roof_top;
        }
        ui.label(format!("{:.3} m above roof", chimney.height_above_roof));
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
        egui::RichText::new("Clear the selected JSON to restore Rough continuous stone.")
            .small()
            .color(DIM),
    );
}

fn placement(
    ui: &mut egui::Ui,
    chimney: &mut ChimneyRecipe,
    dimensions: Dimensions,
    lengths: [f32; 4],
) {
    ui.separator();
    egui::ComboBox::from_label("Wall")
        .selected_text(format!("{:?}", chimney.facade))
        .show_ui(ui, |ui| {
            for facade in Facade::ALL {
                ui.selectable_value(&mut chimney.facade, facade, format!("{facade:?}"));
            }
        });
    let mut left = chimney.offset - chimney.width * 0.5;
    let left_changed = dimensions.coordinate(
        ui,
        "Left edge from wall corner",
        &mut left,
        0,
        -lengths[chimney.facade.index()] * 0.5,
    );
    let width_changed = dimensions.length(ui, "Fireplace width", &mut chimney.width, 0);
    if left_changed || width_changed {
        chimney.offset = left + chimney.width * 0.5;
    }
    dimensions.length(ui, "Breast depth", &mut chimney.depth, 0);
}

fn firebox(ui: &mut egui::Ui, chimney: &mut ChimneyRecipe, dimensions: Dimensions) {
    ui.separator();
    ui.label("Open firebox · no fire or smoke");
    dimensions.length(ui, "Firebox width", &mut chimney.firebox_width, 0);
    dimensions.length(ui, "Firebox height", &mut chimney.firebox_height, 1);
    dimensions.length(ui, "Firebox depth", &mut chimney.firebox_depth, 0);
}
