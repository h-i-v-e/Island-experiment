use super::super::{PendingAction, StudioState, Tab};
use super::units::Dimensions;
use super::{ACCENT, DIM, PANEL, choose_export, dialogs, request_action};
use crate::{
    BuildingRecipe, PartitionAxis, PartitionRecipe, RoofCovering, RoofKind, StairRun, WallStyle,
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
            let church = state.document.recipe().church.is_some();
            if church
                && !matches!(
                    state.tab,
                    Tab::Structure | Tab::Materials | Tab::Grub | Tab::Bake
                )
            {
                state.tab = Tab::Structure;
            }
            tabs(ui, &mut state.tab, church);
            ui.separator();
            let dimensions = super::units::panel(ui, state);
            let mut draft = state.document.recipe().clone();
            egui::ScrollArea::vertical().show(ui, |ui| match state.tab {
                Tab::Structure if church => super::church::show(ui, &mut draft, dimensions),
                Tab::Structure => structure(ui, &mut draft, dimensions),
                Tab::Layout => layout(ui, &mut draft, state, dimensions),
                Tab::Extensions => {
                    super::extensions::show(ui, &mut draft, state.serial, dimensions);
                }
                Tab::Openings => super::openings::show(ui, &mut draft, state.serial, dimensions),
                Tab::Roof => roof(ui, &mut draft, dimensions),
                Tab::Chimney => super::chimney::show(ui, &mut draft, state, dimensions),
                Tab::Materials => materials(ui, &mut draft, state),
                Tab::Grub => super::grub::show(ui, &mut draft.grub_filter),
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

fn tabs(ui: &mut egui::Ui, tab: &mut Tab, church: bool) {
    ui.horizontal_wrapped(|ui| {
        for (candidate, label) in [
            (Tab::Structure, "Structure"),
            (Tab::Layout, "Layout"),
            (Tab::Extensions, "Lean-tos / wings"),
            (Tab::Openings, "Openings"),
            (Tab::Roof, "Roof"),
            (Tab::Chimney, "Chimney"),
            (Tab::Materials, "Materials"),
            (Tab::Grub, "Grub"),
            (Tab::Bake, "LOD / Bake"),
        ] {
            if !church
                || matches!(
                    candidate,
                    Tab::Structure | Tab::Materials | Tab::Grub | Tab::Bake
                )
            {
                ui.selectable_value(tab, candidate, label);
            }
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
    if ui.button("Gothic church · cruciform stone").clicked() {
        request_action(state, PendingAction::New("church".into()));
    }
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

fn structure(ui: &mut egui::Ui, draft: &mut BuildingRecipe, dimensions: Dimensions) {
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
    footprint(ui, draft, dimensions);
    dimensions.length(ui, "Storey wall height", &mut draft.wall_height, 1);
    super::storeys::show(ui, draft);
    scalar(ui, "Wall thickness", &mut draft.wall_thickness, 0.01, " m");
    scalar(
        ui,
        "Floor thickness",
        &mut draft.floor_thickness,
        0.01,
        " m",
    );
    wall_style(ui, draft, dimensions);
    foundation(ui, draft);
    ui.separator();
    ui.label(egui::RichText::new("Finished structural surfaces, clear windows and hinged doors. No furniture is generated.").small().color(DIM));
}

fn footprint(ui: &mut egui::Ui, draft: &mut BuildingRecipe, dimensions: Dimensions) {
    let interior_grid = matches!(
        dimensions.surface,
        Some(crate::MaterialSlot::Floor | crate::MaterialSlot::Ceiling)
    );
    let plinth_grid = dimensions.surface == Some(crate::MaterialSlot::Foundation);
    let inset = if interior_grid {
        2. * draft.wall_thickness
    } else if plinth_grid {
        -2. * draft.foundation.outset
    } else {
        0.
    };
    let mut width = draft.width - inset;
    let mut depth = draft.depth - inset;
    let width_changed = dimensions.length(
        ui,
        if interior_grid {
            "Inside width"
        } else if plinth_grid {
            "Plinth width"
        } else {
            "Width"
        },
        &mut width,
        0,
    );
    let depth_changed = dimensions.length(
        ui,
        if interior_grid {
            "Inside depth"
        } else if plinth_grid {
            "Plinth depth"
        } else {
            "Depth"
        },
        &mut depth,
        usize::from(interior_grid),
    );
    // Avoid round-trip noise simply by viewing an existing recipe.
    if width_changed {
        draft.width = width + inset;
    }
    if depth_changed {
        draft.depth = depth + inset;
    }
    if interior_grid {
        ui.label(format!(
            "Footprint {:.3} × {:.3} m",
            draft.width, draft.depth
        ));
        ui.label(
            egui::RichText::new("Board end joints still follow the joist spacing.")
                .small()
                .color(DIM),
        );
    }
}

fn foundation(ui: &mut egui::Ui, draft: &mut BuildingRecipe) {
    ui.separator();
    ui.label("Foundations & access");
    scalar(
        ui,
        "Foundation outset",
        &mut draft.foundation.outset,
        0.01,
        " m",
    );
    scalar(
        ui,
        "Buried margin",
        &mut draft.foundation.burial_depth,
        0.01,
        " m",
    );
    ui.checkbox(&mut draft.foundation.stairs, "Generate entrance stairs");
    ui.horizontal(|ui| {
        ui.label("Stair direction");
        ui.selectable_value(
            &mut draft.foundation.stair_run,
            StairRun::Outward,
            "Outward",
        );
        ui.selectable_value(
            &mut draft.foundation.stair_run,
            StairRun::AlongWall,
            "Along wall",
        );
    });
    if draft.foundation.stair_run == StairRun::AlongWall {
        ui.label(
            egui::RichText::new("Turns toward the longer wall edge beside each door.")
                .small()
                .color(DIM),
        );
    }
    let depth = -crate::geometry::foundation::bounds(draft).min[2];
    ui.label(format!("30° terrain · {depth:.2} m foundation depth"));
    if let Some(stairs) = crate::geometry::foundation::stairs(draft).next() {
        ui.label(format!(
            "{} steps · {:.2} m reach · 20 cm rise / 24 cm tread",
            stairs.steps, stairs.run
        ));
    }
    ui.label(egui::RichText::new("Place the highest terrain within the foundation at floor level. Stair visibility can be changed per placement in Inspection.").small().color(DIM));
}

fn wall_style(ui: &mut egui::Ui, draft: &mut BuildingRecipe, dimensions: Dimensions) {
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
        if dimensions.surface == Some(crate::MaterialSlot::Timber) {
            dimensions.length(ui, "Timber width", &mut draft.walls.beam_width, 0);
        } else {
            scalar(ui, "Timber width", &mut draft.walls.beam_width, 0.005, " m");
        }
        scalar(
            ui,
            "Timber projection",
            &mut draft.walls.frame_projection,
            0.001,
            " m",
        );
        dimensions.length(ui, "Bay spacing", &mut draft.walls.bay_width, 0);
        ui.label(
            egui::RichText::new(
                "Timber stands proud of the plaster. Frame colour and material are in Materials.",
            )
            .small()
            .color(DIM),
        );
    }
}

fn layout(
    ui: &mut egui::Ui,
    draft: &mut BuildingRecipe,
    state: &StudioState,
    dimensions: Dimensions,
) {
    ui.heading("Rooms & routes");
    ui.label(
        egui::RichText::new("Full cross-walls with open internal doorways")
            .small()
            .color(DIM),
    );
    let size = [draft.width, draft.depth];
    let thickness = draft.wall_thickness;
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
                partition_controls(
                    ui,
                    partition,
                    dimensions,
                    size,
                    thickness,
                    draft.opening_defaults.door,
                    draft.storeys,
                );
            });
        });
    }
    if let Some(index) = remove {
        draft.partitions.remove(index);
    }
    if ui.button("+ Add partition").clicked() {
        draft.partitions.push(PartitionRecipe {
            storey: 0,
            id: format!("partition-{}-{}", state.serial, draft.partitions.len()),
            axis: PartitionAxis::X,
            offset: 0.,
            door_offset: 0.,
            door_width: 1.1,
            door_height: 2.2,
            use_default_door_dimensions: true,
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

fn partition_controls(
    ui: &mut egui::Ui,
    partition: &mut PartitionRecipe,
    dimensions: Dimensions,
    size: [f32; 2],
    thickness: f32,
    defaults: crate::OpeningDimensions,
    storeys: u8,
) {
    if storeys > 1 || partition.storey > 0 {
        ui.horizontal(|ui| {
            ui.label("Storey (0 = ground)");
            ui.add(
                egui::DragValue::new(&mut partition.storey).range(0..=storeys.saturating_sub(1)),
            );
        });
    }
    ui.horizontal(|ui| {
        ui.label("Axis");
        ui.selectable_value(&mut partition.axis, PartitionAxis::X, "X");
        ui.selectable_value(&mut partition.axis, PartitionAxis::Y, "Y");
    });
    let across = usize::from(partition.axis == PartitionAxis::X);
    dimensions.coordinate(
        ui,
        "Wall position from inside edge",
        &mut partition.offset,
        0,
        -size[across] * 0.5 + thickness,
    );
    let authored_size = [partition.door_width, partition.door_height];
    if partition.use_default_door_dimensions {
        partition.door_width = defaults.width;
        partition.door_height = defaults.height;
    }
    let mut custom = !partition.use_default_door_dimensions;
    ui.checkbox(&mut custom, "Override door dimensions");
    partition.use_default_door_dimensions = !custom;
    let mut left = partition.door_offset - partition.door_width * 0.5;
    let width_changed = ui
        .add_enabled_ui(custom, |ui| {
            dimensions.length(ui, "Door width", &mut partition.door_width, 0)
        })
        .inner;
    let left_changed = dimensions.coordinate(
        ui,
        "Door left edge",
        &mut left,
        0,
        -size[1 - across] * 0.5 + thickness,
    );
    if left_changed || width_changed {
        if let Some(grid) = dimensions.grid {
            left = crate::units::snap(left, grid.metres[0], -size[1 - across] * 0.5 + thickness);
        }
        partition.door_offset = left + partition.door_width * 0.5;
    }
    ui.add_enabled_ui(custom, |ui| {
        dimensions.length(ui, "Door height", &mut partition.door_height, 1);
    });
    if partition.use_default_door_dimensions {
        [partition.door_width, partition.door_height] = authored_size;
    }
}

fn roof(ui: &mut egui::Ui, draft: &mut BuildingRecipe, dimensions: Dimensions) {
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
    roof_dimensions(ui, draft, dimensions);
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

fn roof_dimensions(ui: &mut egui::Ui, draft: &mut BuildingRecipe, dimensions: Dimensions) {
    if let Some(grid) = dimensions
        .grid
        .filter(|_| dimensions.surface == Some(crate::MaterialSlot::Roof))
    {
        let run = if draft.roof.kind == RoofKind::Gable {
            draft.width * 0.5
        } else {
            draft.width
        };
        let overhang = draft.roof.overhang
            * if draft.roof.kind == RoofKind::Gable {
                1.
            } else {
                2.
            };
        let mut slope = (run + overhang) * (1. + (draft.roof.rise / run).powi(2)).sqrt();
        if dimensions.length(ui, "Eave-to-top slope", &mut slope, 0) {
            let pitch = grid.metres[0];
            slope = slope.max(((run + overhang) / pitch).floor() * pitch + pitch);
            draft.roof.rise = run * ((slope / (run + overhang)).powi(2) - 1.).sqrt();
        }
        ui.label(format!("Rise {:.3} m", draft.roof.rise));
        let mut length = draft.depth + 2. * draft.roof.overhang;
        if dimensions.length(ui, "Gable-to-gable length", &mut length, 1) {
            draft.depth = length - 2. * draft.roof.overhang;
        }
        ui.label(
            egui::RichText::new("Roof length changes the building depth.")
                .small()
                .color(DIM),
        );
    } else {
        dimensions.length(ui, "Rise", &mut draft.roof.rise, 1);
    }
}

fn materials(ui: &mut egui::Ui, draft: &mut BuildingRecipe, state: &mut StudioState) {
    ui.heading("Surface palette");
    for (label, color) in [
        ("Exterior", &mut draft.materials.wall),
        ("Roof", &mut draft.materials.roof),
        ("Floor", &mut draft.materials.floor),
        ("Foundation / stair bodies", &mut draft.materials.foundation),
        ("Stone slabs", &mut draft.materials.stone_slab),
        ("Ceiling", &mut draft.materials.ceiling),
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
    board_materials(ui, draft, state);
    material_path(
        ui,
        "Stone slabs · treads & hearth",
        &mut draft.materials.stone_slab_recipe,
        state,
        dialogs::MaterialTarget::StoneSlab,
    );
    ui.label(
        egui::RichText::new("Solid stone for step tops, landings and the fireplace hearth.")
            .small()
            .color(DIM),
    );
    material_path(
        ui,
        "Foundation / stair body material",
        &mut draft.materials.foundation_recipe,
        state,
        dialogs::MaterialTarget::Foundation,
    );
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

fn board_materials(ui: &mut egui::Ui, draft: &mut BuildingRecipe, state: &mut StudioState) {
    for (label, path, target) in [
        (
            "Floor material",
            &mut draft.materials.floor_recipe,
            dialogs::MaterialTarget::Floor,
        ),
        (
            "Ceiling material",
            &mut draft.materials.ceiling_recipe,
            dialogs::MaterialTarget::Ceiling,
        ),
    ] {
        material_path(ui, label, path, state, target);
        if path.is_none() {
            ui.label(
                egui::RichText::new(if draft.church.is_some() {
                    "Stone surfaces · built-in material"
                } else {
                    "Wooden boards · built-in material"
                })
                .small()
                .color(DIM),
            );
        }
    }
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
    } else if matches!(
        target,
        dialogs::MaterialTarget::Chimney | dialogs::MaterialTarget::Foundation
    ) {
        ui.colored_label(DIM, "Rough continuous stone · no block joints");
    } else if matches!(target, dialogs::MaterialTarget::StoneSlab) {
        ui.colored_label(DIM, "Solid dressed stone · no block joints");
    }
}
