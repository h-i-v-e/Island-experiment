use bevy_egui::egui;

use super::{DIM, inspector::scalar, units::Dimensions};
use crate::{
    BuildingRecipe, DoorStyle, Facade, HingeSide, OpeningDefaults, OpeningGeneration, OpeningKind,
    OpeningOverride, OpeningRecipe, WindowPanes,
};

pub(super) fn show(ui: &mut egui::Ui, draft: &mut BuildingRecipe, serial: u64, units: Dimensions) {
    ui.heading("Doors & windows");
    defaults(ui, &mut draft.opening_defaults, units);
    let mut automatic = draft.opening_generation.is_some();
    if ui
        .checkbox(&mut automatic, "Generate openings automatically")
        .changed()
    {
        if automatic {
            draft.opening_generation = Some(OpeningGeneration::default());
        } else {
            if let Ok(resolved) = crate::openings::resolve(draft) {
                let openings = resolved.openings.clone();
                draft.openings = openings;
            }
            draft.opening_generation = None;
        }
    }
    if draft.opening_generation.is_some() {
        automatic_controls(ui, draft, units);
    } else {
        manual_controls(ui, draft, serial, units);
    }
}

fn defaults(ui: &mut egui::Ui, sizes: &mut OpeningDefaults, units: Dimensions) {
    egui::CollapsingHeader::new("Building-wide defaults")
        .default_open(true)
        .show(ui, |ui| {
            ui.label("Windows");
            units.length(ui, "Window width", &mut sizes.window.width, 0);
            units.length(ui, "Window height", &mut sizes.window.height, 1);
            units.coordinate(ui, "Window sill", &mut sizes.window.sill, 1, 0.);
            pane_counts(ui, &mut sizes.window_panes);
            ui.separator();
            ui.label("Doors & internal doorways");
            units.length(ui, "Door width", &mut sizes.door.width, 0);
            units.length(ui, "Door height", &mut sizes.door.height, 1);
            ui.label(
                egui::RichText::new(
                    "Doors meet the floor. Individual openings can override these sizes.",
                )
                .small()
                .color(DIM),
            );
        });
}

fn automatic_controls(ui: &mut egui::Ui, draft: &mut BuildingRecipe, units: Dimensions) {
    let settings = draft.opening_generation.as_mut().expect("automatic layout");
    facade_selection(ui, "Entrance walls", &mut settings.door_facades);
    facade_selection(ui, "Window walls", &mut settings.window_facades);
    units.length(ui, "Window spacing", &mut settings.window_spacing, 0);
    scalar(
        ui,
        "Corner clearance",
        &mut settings.corner_clearance,
        0.025,
        " m",
    );
    ui.label(egui::RichText::new("One entrance per selected wall. Windows redistribute as the building resizes, leaving space for corners, fireplaces and partition ends. Edits below override generated values.").small().color(DIM));
    let slots = match crate::openings::generated_slots(draft) {
        Ok(slots) => slots,
        Err(error) => {
            ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
            return;
        }
    };
    ui.label(format!("{} generated slots", slots.len()));
    let lengths = Facade::ALL.map(|facade| facade.length(draft));
    for (index, opening) in slots.iter().enumerate() {
        generated_controls(ui, draft, opening, index, units, lengths);
    }
    let settings = draft.opening_generation.as_mut().expect("automatic layout");
    let inactive = settings
        .overrides
        .iter()
        .filter(|o| !slots.iter().any(|s| s.id == o.id))
        .count();
    if inactive > 0 {
        ui.label(format!(
            "{inactive} overrides retained for slots absent at this building size."
        ));
    }
    if !settings.overrides.is_empty() && ui.button("Reset all opening overrides").clicked() {
        settings.overrides.clear();
    }
    ui.label(egui::RichText::new("Turn off automatic generation to keep this layout and add or remove openings manually.").small().color(DIM));
}

fn facade_selection(ui: &mut egui::Ui, label: &str, selection: &mut Vec<Facade>) {
    ui.label(label);
    ui.horizontal(|ui| {
        for facade in Facade::ALL {
            let mut enabled = selection.contains(&facade);
            if ui
                .toggle_value(&mut enabled, format!("{facade:?}"))
                .changed()
            {
                if enabled {
                    selection.push(facade);
                } else {
                    selection.retain(|v| *v != facade);
                }
            }
        }
    });
}

fn generated_controls(
    ui: &mut egui::Ui,
    draft: &mut BuildingRecipe,
    before: &OpeningRecipe,
    index: usize,
    units: Dimensions,
    lengths: [f32; 4],
) {
    let settings = draft.opening_generation.as_ref().expect("automatic layout");
    let existing = settings.overrides.iter().find(|o| o.id == before.id);
    let mut disabled = existing.is_some_and(|o| o.disabled);
    let mut after = before.clone();
    let mut reset = false;
    ui.push_id(&before.id, |ui| {
        egui::CollapsingHeader::new(format!("{} · {:?}", before.id, before.kind))
            .default_open(index == 0)
            .show(ui, |ui| {
                ui.checkbox(&mut disabled, "Suppress this opening");
                ui.add_enabled_ui(!disabled, |ui| {
                    opening_controls(
                        ui,
                        &mut after,
                        draft.opening_defaults,
                        units,
                        lengths,
                        true,
                        draft.storeys,
                    );
                });
                if existing.is_some() {
                    reset = ui.small_button("Reset to generated values").clicked();
                }
            });
    });
    let settings = draft.opening_generation.as_mut().expect("automatic layout");
    if reset {
        settings.overrides.retain(|o| o.id != before.id);
    } else {
        record_override(settings, before, &after, disabled);
    }
}

fn record_override(
    settings: &mut OpeningGeneration,
    before: &OpeningRecipe,
    after: &OpeningRecipe,
    disabled: bool,
) {
    let existing = settings.overrides.iter().position(|o| o.id == before.id);
    if before == after && existing.is_some_and(|i| settings.overrides[i].disabled == disabled) {
        return;
    }
    if before == after && existing.is_none() && !disabled {
        return;
    }
    let index = existing.unwrap_or_else(|| {
        settings.overrides.push(OpeningOverride {
            id: before.id.clone(),
            ..Default::default()
        });
        settings.overrides.len() - 1
    });
    let patch = &mut settings.overrides[index];
    patch.disabled = disabled;
    if before.facade != after.facade {
        patch.facade = Some(after.facade);
    }
    if before.offset.to_bits() != after.offset.to_bits() {
        patch.offset = Some(after.offset);
    }
    if before.hinge != after.hinge {
        patch.hinge = Some(after.hinge);
    }
    if before.swing_degrees.to_bits() != after.swing_degrees.to_bits() {
        patch.swing_degrees = Some(after.swing_degrees);
    }
    if before.door_style != after.door_style {
        patch.door_style = Some(after.door_style);
    }
    if before.window_panes != after.window_panes {
        patch.window_panes = after.window_panes;
    }
    if after.use_default_dimensions {
        patch.width = None;
        patch.height = None;
        patch.sill = None;
    } else if before.use_default_dimensions {
        patch.width = Some(after.width);
        patch.height = Some(after.height);
        patch.sill = Some(after.sill);
    } else {
        if before.width.to_bits() != after.width.to_bits() {
            patch.width = Some(after.width);
        }
        if before.height.to_bits() != after.height.to_bits() {
            patch.height = Some(after.height);
        }
        if before.sill.to_bits() != after.sill.to_bits() {
            patch.sill = Some(after.sill);
        }
    }
}

fn manual_controls(ui: &mut egui::Ui, draft: &mut BuildingRecipe, serial: u64, units: Dimensions) {
    if ui
        .button("Use building-wide sizes for all openings")
        .clicked()
    {
        for opening in &mut draft.openings {
            opening.use_default_dimensions = true;
        }
    }
    let lengths = Facade::ALL.map(|facade| facade.length(draft));
    let mut remove = None;
    for (index, opening) in draft.openings.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            egui::CollapsingHeader::new(format!("{} · {:?}", opening.id, opening.kind))
                .default_open(index == 0)
                .show(ui, |ui| {
                    opening_controls(
                        ui,
                        opening,
                        draft.opening_defaults,
                        units,
                        lengths,
                        false,
                        draft.storeys,
                    );
                    if ui.small_button("Remove opening").clicked() {
                        remove = Some(index);
                    }
                });
        });
    }
    if let Some(index) = remove {
        draft.openings.remove(index);
    }
    ui.horizontal(|ui| {
        for (kind, label) in [
            (OpeningKind::Window, "+ Window"),
            (OpeningKind::Door, "+ Door"),
        ] {
            if ui.button(label).clicked() {
                let size = draft.opening_defaults.for_kind(kind);
                draft.openings.push(OpeningRecipe {
                    storey: 0,
                    id: format!("opening-{serial}-{}", draft.openings.len()),
                    facade: Facade::North,
                    kind,
                    offset: 0.,
                    width: size.width,
                    height: size.height,
                    sill: size.sill,
                    use_default_dimensions: true,
                    window_panes: None,
                    hinge: HingeSide::Left,
                    swing_degrees: 105.,
                    door_style: DoorStyle::Plank,
                });
            }
        }
    });
}

fn opening_controls(
    ui: &mut egui::Ui,
    opening: &mut OpeningRecipe,
    defaults: OpeningDefaults,
    units: Dimensions,
    lengths: [f32; 4],
    generated: bool,
    storeys: u8,
) {
    if generated {
        ui.label(format!("Storey {} (0 = ground)", opening.storey));
    } else if storeys > 1 || opening.storey > 0 {
        ui.horizontal(|ui| {
            ui.label("Storey (0 = ground)");
            ui.add(egui::DragValue::new(&mut opening.storey).range(0..=storeys.saturating_sub(1)));
        });
    }
    let authored_size = [opening.width, opening.height, opening.sill];
    if generated {
        ui.label(&opening.id);
    } else {
        ui.horizontal(|ui| {
            ui.label("Stable ID");
            ui.text_edit_singleline(&mut opening.id);
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut opening.kind, OpeningKind::Door, "Door");
            ui.selectable_value(&mut opening.kind, OpeningKind::Window, "Window");
        });
    }
    ui.horizontal(|ui| {
        for facade in Facade::ALL {
            ui.selectable_value(&mut opening.facade, facade, format!("{facade:?}"));
        }
    });
    if opening.use_default_dimensions {
        let size = defaults.for_kind(opening.kind);
        opening.width = size.width;
        opening.height = size.height;
        opening.sill = size.sill;
    }
    let mut custom = !opening.use_default_dimensions;
    ui.checkbox(&mut custom, "Override dimensions");
    opening.use_default_dimensions = !custom;
    let mut left = opening.offset - opening.width * 0.5;
    let left_changed = units.coordinate(
        ui,
        if storeys > 1 && opening.kind == OpeningKind::Door {
            "Preferred left edge from wall corner"
        } else {
            "Left edge from wall corner"
        },
        &mut left,
        0,
        -lengths[opening.facade.index()] * 0.5,
    );
    let mut width_changed = false;
    ui.add_enabled_ui(custom, |ui| {
        width_changed = units.length(ui, "Width", &mut opening.width, 0);
        units.length(ui, "Height", &mut opening.height, 1);
        units.coordinate(ui, "Sill", &mut opening.sill, 1, 0.);
    });
    if left_changed || width_changed {
        if let Some(grid) = units.grid {
            left = crate::units::snap(left, grid.metres[0], -lengths[opening.facade.index()] * 0.5);
        }
        opening.offset = left + opening.width * 0.5;
    }
    if opening.kind == OpeningKind::Door {
        door_controls(ui, opening);
        if storeys > 1 {
            ui.label("Moves along this wall when needed to clear the stairs.");
        }
    } else {
        window_controls(ui, opening, defaults.window_panes);
    }
    if opening.use_default_dimensions {
        [opening.width, opening.height, opening.sill] = authored_size;
    }
}

fn window_controls(ui: &mut egui::Ui, opening: &mut OpeningRecipe, defaults: WindowPanes) {
    let mut custom = opening.window_panes.is_some();
    ui.checkbox(&mut custom, "Override window panes");
    if custom {
        pane_counts(ui, opening.window_panes.get_or_insert(defaults));
    } else {
        opening.window_panes = None;
        ui.label(format!(
            "{} columns × {} rows · building default",
            defaults.columns, defaults.rows
        ));
    }
}

fn pane_counts(ui: &mut egui::Ui, panes: &mut WindowPanes) {
    ui.label("Window panes");
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut panes.columns).range(1..=WindowPanes::MAX_PER_AXIS));
        ui.label("columns");
        ui.add(egui::DragValue::new(&mut panes.rows).range(1..=WindowPanes::MAX_PER_AXIS));
        ui.label("rows");
        ui.label(format!(
            "· {} panes",
            u16::from(panes.columns) * u16::from(panes.rows)
        ));
    });
    ui.horizontal(|ui| {
        ui.selectable_value(
            panes,
            WindowPanes {
                columns: 1,
                rows: 1,
            },
            "Single pane",
        );
        ui.selectable_value(
            panes,
            WindowPanes {
                columns: 8,
                rows: 4,
            },
            "Shop grid",
        );
    });
}

fn door_controls(ui: &mut egui::Ui, opening: &mut OpeningRecipe) {
    ui.horizontal(|ui| {
        ui.label("Door style");
        ui.selectable_value(&mut opening.door_style, DoorStyle::Plank, "Plank");
        ui.selectable_value(
            &mut opening.door_style,
            DoorStyle::FrameAndPanel,
            "18th-century panels",
        );
    });
    ui.horizontal(|ui| {
        ui.label("Hinge");
        ui.selectable_value(&mut opening.hinge, HingeSide::Left, "Left");
        ui.selectable_value(&mut opening.hinge, HingeSide::Right, "Right");
    });
    scalar(ui, "Swing", &mut opening.swing_degrees, 1., "°");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewing_manual_or_generated_controls_does_not_change_the_recipe() {
        for automatic in [false, true] {
            let mut recipe = BuildingRecipe::hut();
            recipe.openings[1].use_default_dimensions = true;
            recipe.opening_defaults.window.width = 1.6;
            recipe.opening_defaults.window_panes = WindowPanes {
                columns: 1,
                rows: 1,
            };
            recipe.openings[1].window_panes = Some(WindowPanes {
                columns: 8,
                rows: 4,
            });
            if automatic {
                recipe.opening_generation = Some(OpeningGeneration::default());
            }
            let before = recipe.clone();
            let context = egui::Context::default();
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                show(ui, &mut recipe, 1, Dimensions::default());
            });
            output.textures_delta.clear();
            assert_eq!(recipe, before);
        }
    }

    #[test]
    fn pane_overrides_and_reset_keep_dimension_inheritance() {
        let recipe = BuildingRecipe::procedural(10., 7.);
        let before = crate::openings::generated_slots(&recipe)
            .unwrap()
            .into_iter()
            .find(|o| o.kind == OpeningKind::Window)
            .unwrap();
        let mut after = before.clone();
        after.window_panes = Some(WindowPanes {
            columns: 8,
            rows: 4,
        });
        let mut settings = OpeningGeneration::default();
        record_override(&mut settings, &before, &after, false);
        assert_eq!(settings.overrides[0].window_panes, after.window_panes);
        assert!(settings.overrides[0].width.is_none());
        assert!(settings.overrides[0].height.is_none());
        let before = after.clone();
        after.window_panes = None;
        record_override(&mut settings, &before, &after, false);
        assert!(settings.overrides[0].window_panes.is_none());
        assert!(after.use_default_dimensions);
    }

    #[test]
    fn style_edits_keep_size_inheritance_and_dimension_reset_restores_it() {
        let recipe = BuildingRecipe::procedural(6., 5.);
        let before = crate::openings::generated_slots(&recipe).unwrap().remove(0);
        let mut settings = OpeningGeneration::default();
        let mut after = before.clone();
        after.door_style = DoorStyle::FrameAndPanel;
        record_override(&mut settings, &before, &after, false);
        assert_eq!(
            settings.overrides[0].door_style,
            Some(DoorStyle::FrameAndPanel)
        );
        assert!(settings.overrides[0].width.is_none());
        let before = after.clone();
        after.use_default_dimensions = false;
        after.width = 1.3;
        record_override(&mut settings, &before, &after, false);
        assert_eq!(settings.overrides[0].width, Some(1.3));
        let before = after.clone();
        after.use_default_dimensions = true;
        record_override(&mut settings, &before, &after, false);
        assert!(settings.overrides[0].width.is_none());
        assert!(settings.overrides[0].height.is_none());
        assert_eq!(
            settings.overrides[0].door_style,
            Some(DoorStyle::FrameAndPanel)
        );
    }
}
