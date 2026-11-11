use super::super::{PendingAction, StudioState, Tab};
use super::{ACCENT, DIM, PANEL, choose_export, dialogs, request_action};
use crate::{
    BuildingRecipe, Facade, HingeSide, OpeningKind, OpeningRecipe, PartitionAxis, PartitionRecipe,
    RoofCovering, RoofKind, WallStyle,
};
use bevy_egui::egui;

pub(super) fn show(viewport: &mut egui::Ui, context: &egui::Context, state: &mut StudioState) {
    egui::Panel::left("building-inspector")
        .exact_size(354.)
        .frame(
            egui::Frame::new()
                .fill(PANEL.gamma_multiply(0.98))
                .inner_margin(14),
        )
        .show(viewport, |ui| {
            presets(ui, state);
            ui.separator();
            ui.label(egui::RichText::new("RECIPE").small().color(ACCENT));
            tabs(ui, &mut state.tab);
            ui.separator();
            let mut draft = state.document.recipe().clone();
            egui::ScrollArea::vertical().show(ui, |ui| match state.tab {
                Tab::Structure => structure(ui, &mut draft),
                Tab::Layout => layout(ui, &mut draft, state),
                Tab::Openings => openings(ui, &mut draft, state.serial),
                Tab::Roof => roof(ui, &mut draft),
                Tab::Chimney => super::chimney::show(ui, &mut draft, state),
                Tab::Materials => materials(ui, &mut draft, state),
                Tab::Bake => bake(ui, &mut draft, state),
            });
            // Only recipe edits start a gesture. Toolbar presses must leave
            // history enabled until their release event can activate Undo/Redo.
            if draft != *state.document.recipe() && context.input(|i| i.pointer.primary_down()) {
                state.document.begin_gesture();
            }
            state.document.edit(|recipe| *recipe = draft);
            if !context.input(|i| i.pointer.primary_down()) {
                state.document.end_gesture();
            }
        });
}

fn tabs(ui: &mut egui::Ui, tab: &mut Tab) {
    ui.horizontal_wrapped(|ui| {
        for (candidate, label) in [
            (Tab::Structure, "Structure"),
            (Tab::Layout, "Layout"),
            (Tab::Openings, "Openings"),
            (Tab::Roof, "Roof"),
            (Tab::Chimney, "Chimney"),
            (Tab::Materials, "Materials"),
            (Tab::Bake, "LOD / Bake"),
        ] {
            ui.selectable_value(tab, candidate, label);
        }
    });
}

fn presets(ui: &mut egui::Ui, state: &mut StudioState) {
    ui.label(egui::RichText::new("STARTING PLACES").small().color(ACCENT));
    ui.horizontal(|ui| {
        for (name, label) in [
            ("hut", "Hut"),
            ("cottage", "Cottage"),
            ("workshop", "Workshop"),
        ] {
            let response = ui.allocate_response(egui::vec2(96., 76.), egui::Sense::click());
            thumbnail(ui, &response, name, label);
            if response.clicked() {
                request_action(state, PendingAction::New(name.into()));
            }
        }
    });
}

fn thumbnail(ui: &egui::Ui, response: &egui::Response, name: &str, label: &str) {
    let rect = response.rect;
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        5.,
        if response.hovered() {
            egui::Color32::from_rgb(25, 42, 55)
        } else {
            egui::Color32::from_rgb(15, 27, 37)
        },
    );
    let p = |x: f32, y: f32| egui::pos2(rect.left() + x, rect.top() + y);
    let wall = match name {
        "workshop" => egui::Color32::from_rgb(112, 133, 121),
        "cottage" => egui::Color32::from_rgb(185, 155, 115),
        _ => egui::Color32::from_rgb(166, 116, 76),
    };
    painter.add(egui::Shape::convex_polygon(
        vec![p(17., 31.), p(55., 23.), p(55., 51.), p(17., 58.)],
        wall,
        egui::Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![p(55., 23.), p(80., 35.), p(80., 57.), p(55., 51.)],
        wall.gamma_multiply(0.7),
        egui::Stroke::NONE,
    ));
    let roof = if name == "workshop" {
        vec![p(12., 29.), p(54., 16.), p(84., 31.), p(54., 27.)]
    } else {
        vec![
            p(12., 31.),
            p(36., 8.),
            p(83., 31.),
            p(56., 35.),
            p(36., 19.),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        roof,
        egui::Color32::from_rgb(66, 85, 92),
        egui::Stroke::new(1., ACCENT.gamma_multiply(0.4)),
    ));
    painter.rect_filled(
        egui::Rect::from_two_pos(p(27., 39.), p(36., 55.)),
        0.,
        egui::Color32::from_rgb(52, 42, 33),
    );
    painter.rect_filled(
        egui::Rect::from_two_pos(p(63., 37.), p(73., 44.)),
        0.,
        ACCENT.gamma_multiply(0.4),
    );
    painter.text(
        p(48., 66.),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.),
        egui::Color32::from_rgb(226, 234, 241),
    );
}

fn structure(ui: &mut egui::Ui, draft: &mut BuildingRecipe) {
    ui.heading("A place to arrive");
    ui.label(egui::RichText::new("Structure and identity").color(DIM));
    ui.add(egui::TextEdit::singleline(&mut draft.name).hint_text("Building name"));
    ui.horizontal(|ui| {
        ui.label("Seed");
        ui.add(egui::DragValue::new(&mut draft.seed));
        if ui.small_button("+1").clicked() {
            draft.seed = draft.seed.wrapping_add(1);
        }
    });
    scalar(ui, "Width", &mut draft.width, 0.1, " m");
    scalar(ui, "Depth", &mut draft.depth, 0.1, " m");
    scalar(ui, "Wall height", &mut draft.wall_height, 0.05, " m");
    scalar(ui, "Wall thickness", &mut draft.wall_thickness, 0.01, " m");
    scalar(
        ui,
        "Floor thickness",
        &mut draft.floor_thickness,
        0.01,
        " m",
    );
    wall_style(ui, draft);
    ui.separator();
    ui.label(egui::RichText::new("Finished structural surfaces, clear windows and hinged doors. No furniture is generated.").small().color(DIM));
}

fn wall_style(ui: &mut egui::Ui, draft: &mut BuildingRecipe) {
    ui.separator();
    ui.label("Wall construction");
    let previous = draft.walls.style;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut draft.walls.style, WallStyle::Plain, "Plain");
        ui.selectable_value(
            &mut draft.walls.style,
            WallStyle::WattleAndDaub,
            "Wattle & daub",
        );
    });
    if draft.walls.style != previous {
        draft.materials.wall = draft.walls.style.default_colour();
        draft.materials.wall_recipe = None;
    }
    if draft.walls.style == WallStyle::WattleAndDaub {
        scalar(ui, "Timber width", &mut draft.walls.beam_width, 0.005, " m");
        scalar(
            ui,
            "Timber projection",
            &mut draft.walls.frame_projection,
            0.001,
            " m",
        );
        scalar(ui, "Bay spacing", &mut draft.walls.bay_width, 0.05, " m");
        ui.label(
            egui::RichText::new(
                "Timber stands proud of the plaster. Frame colour and material are in Materials.",
            )
            .small()
            .color(DIM),
        );
    }
}

fn layout(ui: &mut egui::Ui, draft: &mut BuildingRecipe, state: &StudioState) {
    ui.heading("Rooms & routes");
    ui.label(
        egui::RichText::new("Full cross-walls with open internal doorways")
            .small()
            .color(DIM),
    );
    let mut remove = None;
    for (index, partition) in draft.partitions.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(&partition.id);
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
                partition_controls(ui, partition);
            });
        });
    }
    if let Some(index) = remove {
        draft.partitions.remove(index);
    }
    if ui.button("+ Add partition").clicked() {
        draft.partitions.push(PartitionRecipe {
            id: format!("partition-{}-{}", state.serial, draft.partitions.len()),
            axis: PartitionAxis::X,
            offset: 0.,
            door_offset: 0.,
            door_width: 1.1,
            door_height: 2.2,
        });
    }
    if let Some(prototype) = &state.last_good {
        ui.label(format!(
            "{} rooms · connected to entrance",
            prototype.layout.room_count
        ));
    }
    ui.label(egui::RichText::new("Parallel partitions are supported. Crossing partitions are diagnosed before generation.").small().color(DIM));
}

fn partition_controls(ui: &mut egui::Ui, partition: &mut PartitionRecipe) {
    ui.horizontal(|ui| {
        ui.label("Axis");
        ui.selectable_value(&mut partition.axis, PartitionAxis::X, "X");
        ui.selectable_value(&mut partition.axis, PartitionAxis::Y, "Y");
    });
    scalar(ui, "Wall offset", &mut partition.offset, 0.05, " m");
    scalar(ui, "Door offset", &mut partition.door_offset, 0.05, " m");
    scalar(ui, "Door width", &mut partition.door_width, 0.05, " m");
    scalar(ui, "Door height", &mut partition.door_height, 0.05, " m");
}

fn openings(ui: &mut egui::Ui, draft: &mut BuildingRecipe, serial: u64) {
    ui.heading("Doors & windows");
    let mut remove = None;
    for (index, opening) in draft.openings.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            egui::CollapsingHeader::new(format!("{} · {:?}", opening.id, opening.kind))
                .default_open(index == 0)
                .show(ui, |ui| {
                    opening_controls(ui, opening);
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
                let door = kind == OpeningKind::Door;
                draft.openings.push(OpeningRecipe {
                    id: format!("opening-{serial}-{}", draft.openings.len()),
                    facade: Facade::North,
                    kind,
                    offset: 0.,
                    width: 1.15,
                    height: if door { 2.2 } else { 1.1 },
                    sill: if door { 0. } else { 0.95 },
                    hinge: HingeSide::Left,
                    swing_degrees: 105.,
                });
            }
        }
    });
}

fn opening_controls(ui: &mut egui::Ui, opening: &mut OpeningRecipe) {
    ui.horizontal(|ui| {
        ui.label("Stable ID");
        ui.text_edit_singleline(&mut opening.id);
    });
    ui.horizontal(|ui| {
        for facade in Facade::ALL {
            ui.selectable_value(&mut opening.facade, facade, format!("{facade:?}"));
        }
    });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut opening.kind, OpeningKind::Door, "Door");
        ui.selectable_value(&mut opening.kind, OpeningKind::Window, "Window");
    });
    scalar(ui, "Offset", &mut opening.offset, 0.05, " m");
    scalar(ui, "Width", &mut opening.width, 0.05, " m");
    scalar(ui, "Height", &mut opening.height, 0.05, " m");
    scalar(ui, "Sill", &mut opening.sill, 0.05, " m");
    if opening.kind == OpeningKind::Door {
        ui.horizontal(|ui| {
            ui.label("Hinge");
            ui.selectable_value(&mut opening.hinge, HingeSide::Left, "Left");
            ui.selectable_value(&mut opening.hinge, HingeSide::Right, "Right");
        });
        scalar(ui, "Swing", &mut opening.swing_degrees, 1., "°");
    }
}

fn roof(ui: &mut egui::Ui, draft: &mut BuildingRecipe) {
    ui.heading("A sheltered silhouette");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut draft.roof.kind, RoofKind::Gable, "Gable");
        ui.selectable_value(&mut draft.roof.kind, RoofKind::Shed, "Single slope");
    });
    ui.separator();
    ui.label("Roof covering");
    let previous = draft.roof.covering;
    ui.horizontal(|ui| {
        for (covering, label) in [
            (RoofCovering::StandingSeam, "Metal"),
            (RoofCovering::Slate, "Slate"),
            (RoofCovering::Thatch, "Thatch"),
        ] {
            ui.selectable_value(&mut draft.roof.covering, covering, label);
        }
    });
    if draft.roof.covering != previous {
        draft.materials.roof = draft.roof.covering.default_colour();
        draft.materials.roof_recipe = None;
    }
    ui.label(
        egui::RichText::new("Selecting a covering restores its default surface.")
            .small()
            .color(DIM),
    );
    scalar(ui, "Rise", &mut draft.roof.rise, 0.05, " m");
    scalar(ui, "Eaves", &mut draft.roof.overhang, 0.025, " m");
    scalar(ui, "Thickness", &mut draft.roof.thickness, 0.01, " m");
    if draft.roof.covering == RoofCovering::Thatch {
        ui.label(
            egui::RichText::new("Thatch is at least 0.45 m thick.")
                .small()
                .color(DIM),
        );
    }
    ui.label(
        egui::RichText::new(if draft.roof.covering == RoofCovering::Thatch {
            "LOD0 smooths the thatch while retaining its overall dimensions."
        } else {
            "The same roof profile is retained at every LOD."
        })
        .small()
        .color(DIM),
    );
}

fn materials(ui: &mut egui::Ui, draft: &mut BuildingRecipe, state: &mut StudioState) {
    ui.heading("Surface palette");
    for (label, color) in [
        ("Exterior", &mut draft.materials.wall),
        ("Roof", &mut draft.materials.roof),
        ("Floor", &mut draft.materials.floor),
        ("Interior", &mut draft.materials.interior),
        ("Trim", &mut draft.materials.trim),
        ("Door", &mut draft.materials.door),
        ("Chimney", &mut draft.materials.chimney),
        ("Frame timber", &mut draft.materials.timber),
    ] {
        ui.horizontal(|ui| {
            ui.color_edit_button_rgb(color);
            ui.label(label);
        });
    }
    ui.horizontal(|ui| {
        ui.color_edit_button_rgba_unmultiplied(&mut draft.materials.glass);
        ui.label("Glass tint / opacity");
    });
    ui.separator();
    material_path(
        ui,
        "Wall material",
        &mut draft.materials.wall_recipe,
        state,
        dialogs::MaterialTarget::Wall,
    );
    material_path(
        ui,
        "Roof material",
        &mut draft.materials.roof_recipe,
        state,
        dialogs::MaterialTarget::Roof,
    );
    if draft.materials.roof_recipe.is_none() {
        ui.label(
            egui::RichText::new(match draft.roof.covering {
                RoofCovering::StandingSeam => "Metal roof · palette colour",
                RoofCovering::Slate => "Slate roof · built-in material",
                RoofCovering::Thatch => "Thatch roof · built-in material",
            })
            .small()
            .color(DIM),
        );
    }
    material_path(
        ui,
        "Door material",
        &mut draft.materials.door_recipe,
        state,
        dialogs::MaterialTarget::Door,
    );
    material_path(
        ui,
        "Chimney material",
        &mut draft.materials.chimney_recipe,
        state,
        dialogs::MaterialTarget::Chimney,
    );
    if ui
        .add_enabled(
            state.file_dialog.is_none(),
            egui::Button::new("Use wooden planks on doors"),
        )
        .clicked()
    {
        dialogs::use_wooden_planks(state);
    }
    material_path(
        ui,
        "Frame timber material",
        &mut draft.materials.timber_recipe,
        state,
        dialogs::MaterialTarget::Timber,
    );
    ui.label(egui::RichText::new("Choose Material Studio JSON recipes. Physical tiling and parameters use the shared evaluator; exports snapshot dependencies.").small().color(DIM));
}

fn bake(ui: &mut egui::Ui, draft: &mut BuildingRecipe, state: &mut StudioState) {
    ui.heading("Three representations");
    for label in [
        "LOD0  ·  full accessible interior",
        "LOD1  ·  exterior / opaque windows",
        "LOD2  ·  baked 3D shell",
    ] {
        ui.label(label);
    }
    ui.separator();
    egui::ComboBox::from_label("Final atlas")
        .selected_text(format!("{} px", draft.atlas_size))
        .show_ui(ui, |ui| {
            for size in [128, 256, 512, 1024, 2048] {
                ui.selectable_value(&mut draft.atlas_size, size, format!("{size} × {size}"));
            }
        });
    ui.label(format!(
        "Final atlas memory: {:.1} MiB",
        f64::from(draft.atlas_size) * f64::from(draft.atlas_size) * 12. / 1_048_576.
    ));
    if ui
        .add_enabled(
            state.export_task.is_none(),
            egui::Button::new("Bake full atlas + export package…"),
        )
        .clicked()
    {
        choose_export(state);
    }
    ui.label(
        egui::RichText::new(if state.baked_serial == Some(state.serial) {
            "Final bake matches this revision"
        } else {
            "Final atlas is not baked for this revision"
        })
        .color(DIM),
    );
    ui.label(egui::RichText::new("Preview atlases use 128 px. Export meshes, maps, material dependencies, collision, hinges and a SHA-256 manifest to a new directory.").small().color(DIM));
}

pub(super) fn scalar(ui: &mut egui::Ui, label: &str, value: &mut f32, speed: f32, suffix: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(value).speed(speed).suffix(suffix));
        ui.label(label);
    });
}

pub(super) fn material_path(
    ui: &mut egui::Ui,
    label: &str,
    path: &mut Option<String>,
    state: &mut StudioState,
    target: dialogs::MaterialTarget,
) {
    ui.label(label);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                state.file_dialog.is_none(),
                egui::Button::new("Choose JSON…"),
            )
            .clicked()
        {
            dialogs::start(state, dialogs::Intent::Material(target));
        }
        if path.is_some() && ui.small_button("Clear").clicked() {
            *path = None;
        }
    });
    if let Some(path) = path {
        ui.add(egui::TextEdit::singleline(path).desired_width(f32::INFINITY));
    } else if matches!(target, dialogs::MaterialTarget::Chimney) {
        ui.colored_label(DIM, "Coursed stone");
    }
}
