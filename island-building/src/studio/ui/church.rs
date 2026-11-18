use super::{DIM, units::Dimensions};
use crate::{BuildingRecipe, ChurchTowerStyle};
use bevy_egui::egui;

pub(super) fn show(ui: &mut egui::Ui, r: &mut BuildingRecipe, d: Dimensions) {
    ui.heading("A stone Gothic church");
    ui.add(egui::TextEdit::singleline(&mut r.name).hint_text("Church name"));
    ui.horizontal(|ui| {
        ui.label("Seed");
        ui.add(egui::DragValue::new(&mut r.seed));
    });
    ui.label(egui::RichText::new("Connected nave, crossing, transepts and chancel. Unfurnished interior, pointed windows and an openable south entrance.").small().color(DIM));
    d.length(ui, "Transept span", &mut r.width, 0);
    d.length(ui, "Church length", &mut r.depth, 0);
    d.length(ui, "Wall height", &mut r.wall_height, 1);
    let c = r.church.as_mut().expect("church inspector");
    d.length(ui, "Nave width", &mut c.nave_width, 0);
    d.length(ui, "Transept depth", &mut c.transept_depth, 0);
    d.length(ui, "Crossing offset", &mut c.crossing_offset, 0);
    d.length(ui, "Window bay spacing", &mut c.bay_spacing, 0);
    ui.separator();
    ui.strong("Towers & flying buttresses");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut c.tower_style, ChurchTowerStyle::Twin, "Twin towers");
        ui.selectable_value(
            &mut c.tower_style,
            ChurchTowerStyle::SingleFront,
            "Single front tower",
        );
    });
    d.length(ui, "Tower width", &mut c.tower_width, 0);
    d.length(ui, "Tower height", &mut c.tower_height, 1);
    ui.checkbox(&mut c.flying_buttresses, "Flying buttresses");
    if c.flying_buttresses {
        d.length(ui, "Buttress reach", &mut c.buttress_reach, 0);
    }
    d.length(ui, "Roof rise", &mut r.roof.rise, 1);
    ui.horizontal(|ui| {
        ui.label("Roof overhang");
        ui.add(
            egui::DragValue::new(&mut r.roof.overhang)
                .speed(0.02)
                .range(0.0..=1.5)
                .suffix(" m"),
        );
    });
    ui.separator();
    ui.strong("Pointed windows & entrance");
    d.length(ui, "Window width", &mut r.opening_defaults.window.width, 0);
    d.length(
        ui,
        "Window height",
        &mut r.opening_defaults.window.height,
        1,
    );
    d.length(ui, "Window sill", &mut r.opening_defaults.window.sill, 1);
    d.length(ui, "Door width", &mut r.opening_defaults.door.width, 0);
    d.length(
        ui,
        "Door spring height",
        &mut r.opening_defaults.door.height,
        1,
    );
    ui.label("Two pointed timber leaves fill the entrance arch. Spring height is the start of its curved head.");
    ui.horizontal(|ui| {
        ui.label("Wall thickness");
        ui.add(
            egui::DragValue::new(&mut r.wall_thickness)
                .speed(0.01)
                .suffix(" m"),
        );
    });
    ui.checkbox(&mut r.foundation.stairs, "Show entrance stairs");
    ui.separator();
    ui.label("LOD0: interior, clear glazing, tracery and stone ribs. LOD1: opaque glazing and simpler masonry. LOD2: baked exterior bays and reduced buttress arches.");
}
