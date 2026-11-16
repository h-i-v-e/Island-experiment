use super::{DIM, units::Dimensions};
use crate::{BuildingRecipe, ExtensionKind, ExtensionRecipe, Facade};
use bevy_egui::egui;

pub(super) fn show(ui: &mut egui::Ui, draft: &mut BuildingRecipe, serial: u64, units: Dimensions) {
    ui.heading("Lean-tos & wings");
    ui.label(egui::RichText::new("Attached ground-floor rooms share materials with the house. Their interiors connect across the shared wall; covered windows are omitted.").small().color(DIM));
    let lengths = Facade::ALL.map(|f| f.length(draft));
    let default_panes = draft.opening_defaults.window_panes;
    let mut remove = None;
    for (index, extension) in draft.extensions.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            egui::CollapsingHeader::new(format!("{} · {:?}", extension.id, extension.kind))
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Stable ID");
                        ui.text_edit_singleline(&mut extension.id);
                    });
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut extension.kind, ExtensionKind::LeanTo, "Lean-to");
                        ui.selectable_value(
                            &mut extension.kind,
                            ExtensionKind::Wing,
                            "Gabled wing",
                        );
                    });
                    ui.horizontal(|ui| {
                        for facade in Facade::ALL {
                            ui.selectable_value(
                                &mut extension.facade,
                                facade,
                                format!("{facade:?}"),
                            );
                        }
                    });
                    units.length(ui, "Width along house", &mut extension.width, 0);
                    units.length(ui, "Outward depth", &mut extension.depth, 0);
                    units.coordinate(
                        ui,
                        "Centre along host wall",
                        &mut extension.offset,
                        0,
                        -lengths[extension.facade.index()] * 0.5,
                    );
                    units.length(ui, "Eaves height", &mut extension.wall_height, 1);
                    units.length(ui, "Roof rise", &mut extension.roof_rise, 1);
                    ui.checkbox(&mut extension.windows, "Generate exterior windows");
                    ui.label("Windows use the building-wide dimensions and pane layout.");
                    let mut custom_panes = extension.window_panes.is_some();
                    ui.checkbox(&mut custom_panes, "Override window panes");
                    if custom_panes {
                        super::openings::pane_counts(
                            ui,
                            extension.window_panes.get_or_insert(default_panes),
                        );
                    } else {
                        extension.window_panes = None;
                    }
                    if ui.small_button("Remove extension").clicked() {
                        remove = Some(index);
                    }
                });
        });
    }
    if let Some(index) = remove {
        draft.extensions.remove(index);
    }
    ui.add_enabled_ui(draft.extensions.len() < 8, |ui| {
        ui.horizontal(|ui| {
            for (kind, label) in [
                (ExtensionKind::LeanTo, "+ Lean-to"),
                (ExtensionKind::Wing, "+ Wing"),
            ] {
                if ui.button(label).clicked() {
                    let facade = [Facade::East, Facade::West, Facade::North, Facade::South]
                        .into_iter()
                        .find(|f| {
                            !draft.extensions.iter().any(|e| e.facade == *f)
                                && draft.chimney.as_ref().is_none_or(|c| c.facade != *f)
                        })
                        .unwrap_or(Facade::East);
                    draft.extensions.push(ExtensionRecipe {
                        id: format!("extension-{serial}-{}", draft.extensions.len()),
                        kind,
                        facade,
                        width: lengths[facade.index()].min(3.),
                        wall_height: if kind == ExtensionKind::LeanTo {
                            (draft.wall_height - 0.4).max(2.3)
                        } else {
                            draft.wall_height
                        },
                        roof_rise: if kind == ExtensionKind::LeanTo {
                            (draft.wall_height - (draft.wall_height - 0.4).max(2.3)).clamp(0.1, 0.4)
                        } else {
                            draft.roof.rise
                        },
                        ..Default::default()
                    });
                }
            }
        });
    });
    ui.label(egui::RichText::new("Move entrances and chimneys clear of connections. Wings have gabled roofs; lean-to roofs fall away from the house. Both use level floors and foundations sized for the complete footprint on 30° terrain.").small().color(DIM));
}
