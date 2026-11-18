use super::{DIM, inspector::scalar};
use crate::{BuildingRecipe, Facade, StairAxis, StaircaseRecipe};
use bevy_egui::egui;

pub(super) fn show(ui: &mut egui::Ui, recipe: &mut BuildingRecipe) {
    ui.horizontal(|ui| {
        ui.label("Storeys (ground included)");
        ui.add(egui::DragValue::new(&mut recipe.storeys).range(1..=6));
    });
    if recipe.storeys == 1 {
        return;
    }
    ui.label(format!(
        "Roof wall line {:.2} m · floor spacing {:.2} m",
        recipe.eaves_height(),
        recipe.floor_height(1)
    ));
    egui::CollapsingHeader::new("Internal staircase")
        .default_open(true)
        .show(ui, |ui| staircase_controls(ui, &mut recipe.staircase));
}

fn staircase_controls(ui: &mut egui::Ui, stairs: &mut StaircaseRecipe) {
    ui.checkbox(&mut stairs.enclosed, "Enclose stairwell");
    if stairs.enclosed {
        ui.label("One openable door per floor · shared door dimensions");
        ui.horizontal(|ui| {
            ui.label("Stair doors");
            ui.selectable_value(&mut stairs.door_style, crate::DoorStyle::Plank, "Plank");
            ui.selectable_value(
                &mut stairs.door_style,
                crate::DoorStyle::FrameAndPanel,
                "Frame & panel",
            );
        });
    }
    wall_selector(ui, &mut stairs.wall);
    scalar(ui, "Flight width", &mut stairs.width, 0.025, " m");
    scalar(ui, "Tread going", &mut stairs.going, 0.01, " m");
    scalar(ui, "Maximum riser", &mut stairs.target_riser, 0.005, " m");
    scalar(ui, "Landing depth", &mut stairs.landing_depth, 0.025, " m");
    if stairs.wall.is_none() {
        automatic_placement(ui, stairs);
    }
    ui.label(egui::RichText::new("Choose a wall to keep stairs against it. The stair position along that wall adjusts to fit doors and landings.").small().color(DIM));
}

fn wall_selector(ui: &mut egui::Ui, wall: &mut Option<Facade>) {
    let selected = wall.map_or_else(|| "Automatic".into(), |wall| format!("{wall:?}"));
    egui::ComboBox::from_label("Staircase wall")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(wall, None, "Automatic");
            for facade in Facade::ALL {
                ui.selectable_value(wall, Some(facade), format!("{facade:?}"));
            }
        });
}

fn automatic_placement(ui: &mut egui::Ui, stairs: &mut StaircaseRecipe) {
    ui.horizontal(|ui| {
        ui.label("Flight axis");
        for (axis, name) in [
            (StairAxis::Auto, "Auto"),
            (StairAxis::X, "X"),
            (StairAxis::Y, "Y"),
        ] {
            ui.selectable_value(&mut stairs.axis, axis, name);
        }
    });
    let mut automatic = stairs.position.is_none();
    if ui
        .checkbox(&mut automatic, "Position automatically")
        .changed()
    {
        stairs.position = if automatic { None } else { Some([0.; 2]) };
    }
    if let Some(position) = &mut stairs.position {
        scalar(ui, "Centre X", &mut position[0], 0.05, " m");
        scalar(ui, "Centre Y", &mut position[1], 0.05, " m");
    }
}
