use super::{DIM, inspector::scalar};
use crate::{BuildingRecipe, StairAxis};
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
    egui::CollapsingHeader::new("Internal staircase").default_open(true).show(ui,|ui| {
        let stairs=&mut recipe.staircase;
        ui.checkbox(&mut stairs.enclosed,"Enclose stairwell");
        if stairs.enclosed {
            ui.label("One openable door per floor · shared door dimensions");
            ui.horizontal(|ui| {
                ui.label("Stair doors");
                ui.selectable_value(&mut stairs.door_style,crate::DoorStyle::Plank,"Plank");
                ui.selectable_value(&mut stairs.door_style,crate::DoorStyle::FrameAndPanel,"Frame & panel");
            });
        }
        ui.horizontal(|ui| {
            ui.label("Run along");
            for (axis,name) in [(StairAxis::Auto,"Auto"),(StairAxis::X,"X"),(StairAxis::Y,"Y")] {ui.selectable_value(&mut stairs.axis,axis,name);}
        });
        scalar(ui,"Flight width",&mut stairs.width,0.025," m");
        scalar(ui,"Tread going",&mut stairs.going,0.01," m");
        scalar(ui,"Maximum riser",&mut stairs.target_riser,0.005," m");
        scalar(ui,"Landing depth",&mut stairs.landing_depth,0.025," m");
        let mut automatic=stairs.position.is_none();
        if ui.checkbox(&mut automatic,"Position automatically").changed() {stairs.position=if automatic{None}else{Some([0.;2])};}
        if let Some(position)=&mut stairs.position {
            scalar(ui,"Centre X",&mut position[0],0.05," m");
            scalar(ui,"Centre Y",&mut position[1],0.05," m");
        }
        ui.label(egui::RichText::new("Automatic stairs hug an external wall, keeping the chimney wall clear. Entrance doors move along their walls when needed. Timber U-stairs include turn and floor landings and guardrails; upper floors and joists leave a real stairwell.").small().color(DIM));
    });
}
