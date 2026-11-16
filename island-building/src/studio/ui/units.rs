use super::{DIM, StudioState};
use crate::{MaterialSlot, studio::Tab, units::MaterialGrid};
use bevy_egui::egui;
use std::path::Path;

const SURFACES: [(MaterialSlot, &str); 9] = [
    (MaterialSlot::Wall, "Exterior walls"),
    (MaterialSlot::Floor, "Floor boards"),
    (MaterialSlot::Ceiling, "Ceiling boards"),
    (MaterialSlot::Timber, "Frame timber"),
    (MaterialSlot::Roof, "Roof"),
    (MaterialSlot::Foundation, "Foundation / stairs"),
    (MaterialSlot::Chimney, "Chimney"),
    (MaterialSlot::StoneSlab, "Stone slabs"),
    (MaterialSlot::Door, "Door material"),
];

pub(in crate::studio) struct EditorUnits {
    enabled: bool,
    surfaces: [MaterialSlot; 5],
    cache: Option<(u64, u64, MaterialSlot, Result<MaterialGrid, String>)>,
}

impl Default for EditorUnits {
    fn default() -> Self {
        Self {
            enabled: true,
            surfaces: [
                MaterialSlot::Wall,
                MaterialSlot::Wall,
                MaterialSlot::Wall,
                MaterialSlot::Roof,
                MaterialSlot::Chimney,
            ],
            cache: None,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Dimensions {
    pub grid: Option<MaterialGrid>,
    pub surface: Option<MaterialSlot>,
}

pub(super) fn panel(ui: &mut egui::Ui, state: &mut StudioState) -> Dimensions {
    let index = match state.tab {
        Tab::Structure => 0,
        Tab::Layout => 1,
        Tab::Openings => 2,
        Tab::Roof => 3,
        Tab::Chimney => 4,
        Tab::Materials | Tab::Grub | Tab::Bake => return Dimensions::default(),
    };
    let editor = &mut state.units;
    ui.horizontal(|ui| {
        ui.label("Dimensions");
        ui.selectable_value(&mut editor.enabled, true, "Material units");
        ui.selectable_value(&mut editor.enabled, false, "Metres");
    });
    if !editor.enabled {
        return Dimensions::default();
    }
    let slot = &mut editor.surfaces[index];
    let church = state.document.recipe().church.is_some();
    let surface_label = |surface, default| match (surface, church) {
        (MaterialSlot::Floor, true) => "Stone floor",
        (MaterialSlot::Ceiling, true) => "Stone ceiling",
        _ => default,
    };
    egui::ComboBox::from_label("Grid surface")
        .selected_text(surface_label(
            *slot,
            SURFACES.iter().find(|(s, _)| s == slot).unwrap().1,
        ))
        .show_ui(ui, |ui| {
            for (surface, label) in SURFACES {
                if surface != MaterialSlot::Door
                    || state.document.recipe().materials.door_recipe.is_some()
                {
                    ui.selectable_value(slot, surface, surface_label(surface, label));
                }
            }
        });
    let slot = *slot;
    let revision = state.document.revision();
    let reload = ui.small_button("Reload grid").clicked();
    if editor
        .cache
        .as_ref()
        .is_none_or(|(r, serial, s, _)| (*r, *serial, *s) != (revision, state.serial, slot))
        || reload
    {
        let result = MaterialGrid::load(
            state.document.recipe(),
            state.document.base_directory().unwrap_or(Path::new(".")),
            slot,
        )
        .map_err(|error| error.to_string());
        editor.cache = Some((revision, state.serial, slot, result));
    }
    match &editor.cache.as_ref().unwrap().3 {
        Ok(grid) => {
            for (i, axis) in ["U", "V"].into_iter().enumerate() {
                ui.label(
                    egui::RichText::new(format!(
                        "1 {axis} = {:.3} m · {}",
                        grid.metres[i], grid.labels[i]
                    ))
                    .small()
                    .color(DIM),
                );
            }
            ui.label(
                egui::RichText::new("Edits snap to whole units. Snap rounds an existing value.")
                    .small()
                    .color(DIM),
            );
            ui.label(
                egui::RichText::new("Thickness and small construction clearances stay in metres.")
                    .small()
                    .color(DIM),
            );
            ui.separator();
            Dimensions {
                grid: Some(*grid),
                surface: Some(slot),
            }
        }
        Err(error) => {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Grid unavailable: {error}"),
            );
            ui.label("Dimensions remain editable in metres.");
            Dimensions::default()
        }
    }
}

impl Dimensions {
    pub(super) fn length(
        self,
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        axis: usize,
    ) -> bool {
        self.distance(ui, label, value, axis, 0., true)
    }

    pub(super) fn coordinate(
        self,
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        axis: usize,
        origin: f32,
    ) -> bool {
        self.distance(ui, label, value, axis, origin, false)
    }

    fn distance(
        self,
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        axis: usize,
        origin: f32,
        positive: bool,
    ) -> bool {
        let before = value.to_bits();
        let Some(grid) = self.grid else {
            let mut distance = *value - origin;
            let original = distance;
            super::inspector::scalar(ui, label, &mut distance, 0.025, " m");
            if distance.to_bits() != original.to_bits() {
                *value = distance + origin;
            }
            return value.to_bits() != before;
        };
        let step = grid.metres[axis];
        let mut units = (*value - origin) / step;
        let original_units = units.to_bits();
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut units)
                    .speed(0.1)
                    .max_decimals(3)
                    .update_while_editing(false)
                    .suffix(if axis == 0 { " U" } else { " V" }),
            );
            let snap = ui.small_button("Snap").clicked();
            if units.to_bits() != original_units || snap {
                *value = units
                    .round()
                    .max(if positive { 1. } else { f32::NEG_INFINITY })
                    * step
                    + origin;
            }
            ui.label(label)
                .on_hover_text(format!("{value:.3} m · {}", grid.labels[axis]));
        });
        ui.label(
            egui::RichText::new(format!("{value:.3} m"))
                .small()
                .color(DIM),
        );
        value.to_bits() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingRecipe, document::StudioDocument};

    fn frame(
        context: &egui::Context,
        value: &mut f32,
        events: Vec<egui::Event>,
    ) -> (egui::Pos2, egui::Pos2) {
        let mut output = context.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_max_size(egui::vec2(700., 300.));
                Dimensions {
                    grid: Some(MaterialGrid {
                        metres: [0.2, 2.4],
                        labels: ["board width", "board length"],
                    }),
                    surface: Some(MaterialSlot::Floor),
                }
                .length(ui, "Width", value, 0);
            },
        );
        output.textures_delta.clear();
        let snap = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text() == "Snap"
                {
                    Some(text.pos + text.galley.size() * 0.5)
                } else {
                    None
                }
            })
            .expect("Snap button");
        let number = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.pos.x < snap.x
                    && text.pos.y < 30.
                {
                    Some(text.pos + text.galley.size() * 0.5)
                } else {
                    None
                }
            })
            .expect("unit input");
        (snap, number)
    }

    #[test]
    fn viewing_keeps_legacy_dimensions_and_snap_is_an_undoable_edit() {
        let context = egui::Context::default();
        let mut document = StudioDocument::new(BuildingRecipe::hut());
        document.edit(|r| r.width = 6.13);
        let mut value = document.recipe().width;
        let (point, _) = frame(&context, &mut value, vec![]);
        assert_eq!(
            value.to_bits(),
            6.13_f32.to_bits(),
            "viewing never changes an off-grid recipe"
        );
        for pressed in [true, false] {
            frame(
                &context,
                &mut value,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!((value - 6.2).abs() < 0.00001);
        document.edit(|r| r.width = value);
        document.undo();
        assert_eq!(document.recipe().width.to_bits(), 6.13_f32.to_bits());
        document.redo();
        assert!((document.recipe().width - 6.2).abs() < 0.00001);
    }
    #[test]
    fn keyboard_entry_keeps_unfinished_text_and_snaps_once_on_commit() {
        let context = egui::Context::default();
        let mut value = 6.13;
        let (_, point) = frame(&context, &mut value, vec![]);
        for pressed in [true, false] {
            frame(
                &context,
                &mut value,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        frame(&context, &mut value, vec![]);
        frame(&context, &mut value, vec![egui::Event::Text("29.2".into())]);
        assert_eq!(
            value.to_bits(),
            6.13_f32.to_bits(),
            "typing waits for commit"
        );
        frame(
            &context,
            &mut value,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(
            (value - 5.8).abs() < 0.00001,
            "29.2 rounds to 29 boards: {value}"
        );
    }
}
