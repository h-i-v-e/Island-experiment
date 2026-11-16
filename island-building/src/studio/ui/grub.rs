use super::DIM;
use crate::GrubFilter;
use bevy_egui::egui;

pub(super) fn show(ui: &mut egui::Ui, filter: &mut GrubFilter) {
    ui.heading("Grub & moss");
    ui.checkbox(&mut filter.enabled, "Enable grub filter");
    ui.label(
        egui::RichText::new("Broad patches combined with fine breakup; moss favours upward faces.")
            .small()
            .color(DIM),
    );
    ui.add_enabled_ui(filter.enabled, |ui| {
        ui.add(egui::Slider::new(&mut filter.grime, 0.0..=1.0).text("Grime strength"));
        ui.add(egui::Slider::new(&mut filter.moss, 0.0..=1.0).text("Moss strength"));
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut filter.height_falloff)
                    .speed(0.05)
                    .range(0.1..=20.)
                    .suffix(" m"),
            );
            ui.label("Height falloff");
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut filter.patch_size)
                    .speed(0.05)
                    .range(0.1..=20.)
                    .suffix(" m"),
            );
            ui.label("Broad patch size");
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut filter.fine_patch_size)
                    .speed(0.01)
                    .range(0.025..=20.)
                    .suffix(" m"),
            );
            ui.label("Fine patch size");
        });
        ui.add(egui::Slider::new(&mut filter.fine_mix, 0.0..=1.0).text("Fine contribution"));
        for (label, colour) in [
            ("Dirt colour", &mut filter.dirt_colour),
            ("Moss colour", &mut filter.moss_colour),
        ] {
            ui.horizontal(|ui| {
                ui.color_edit_button_rgb(colour);
                ui.label(label);
            });
        }
    });
    ui.separator();
    ui.label("Height blend · building floor");
    ui.label(format!(
        "1 m below: {:.0}% · floor: 50% · 1 m above: {:.0}%",
        filter.height_influence(-1.) * 100.,
        filter.height_influence(1.) * 100.
    ));
    ui.label(egui::RichText::new("Only foundations, stairs, thatch, exterior daub and exterior church stone are weathered. Timber and shared surfaces stay clean, including the mixed LOD2 wall atlas. The midpoint is the building's floor, independent of terrain; percentages show neutral noise.").small().color(DIM));
}
