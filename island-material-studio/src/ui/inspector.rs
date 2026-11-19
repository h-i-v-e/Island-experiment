//! Exhaustive typed recipe inspector.

#![allow(clippy::too_many_lines)]

use std::collections::BTreeMap;

use bevy_egui::egui;
use motu::procedural_textures::{
    AlbedoBlend, CRACKED_STONE_MAX_WARP_AMPLITUDE, ColourMap, ColourParameterReference,
    ColourValue, DomainWarpSettings, GradientStop, HeightBlend, LayerMask, LinearRgb,
    MaterialLayer, MaterialModel, ParameterDefinition, ROUNDED_STONES_MAX_RADIUS,
    ROUNDED_STONES_MAX_WARP_AMPLITUDE, RemapPoint, ScalarRemap, ScalarSource, SourceKind,
    TextureRecipe,
    parameters::{MAX_RECIPE_PARAMETERS, is_valid_parameter_name, rename_recipe_parameter},
    recipe::{OcclusionCombine, OutputProfile as RecipeOutputProfile},
};

pub(super) struct ParameterNameEdit {
    original: Option<String>,
    name: String,
    focus: bool,
}

/// Draws either the base recipe or the selected layer and reports mutations.
pub(super) fn draw(
    ui: &mut egui::Ui,
    recipe: &mut TextureRecipe,
    selected_layer_id: Option<&str>,
    parameter_name_edit: &mut Option<ParameterNameEdit>,
) -> bool {
    let Some(layer_id) = selected_layer_id else {
        return draw_base(ui, recipe, parameter_name_edit);
    };
    let Some(index) = recipe.layers.iter().position(|layer| layer.id == layer_id) else {
        ui.colored_label(
            egui::Color32::YELLOW,
            "The selected layer no longer exists.",
        );
        return false;
    };
    let parameters = &recipe.parameters;
    let (earlier_layers, selected_and_after) = recipe.layers.split_at_mut(index);
    let layer = &mut selected_and_after[0];
    ui.push_id(("layer inspector", layer_id), |ui| {
        draw_layer(ui, layer, earlier_layers, parameters)
    })
    .inner
}

fn draw_base(
    ui: &mut egui::Ui,
    recipe: &mut TextureRecipe,
    parameter_name_edit: &mut Option<ParameterNameEdit>,
) -> bool {
    let mut changed = false;
    ui.heading("Base material");
    changed |= text_row(ui, "Name", &mut recipe.name);
    changed |= u64_row(ui, "Seed", &mut recipe.seed);
    changed |= u32_row(ui, "Width", &mut recipe.width, 1.0);
    changed |= u32_row(ui, "Height", &mut recipe.height, 1.0);
    changed |= f32_row(
        ui,
        "Tile width (m)",
        &mut recipe.physical_tile_width_m,
        0.01,
    );
    changed |= f32_row(
        ui,
        "Tile height (m)",
        &mut recipe.physical_tile_height_m,
        0.01,
    );

    egui::CollapsingHeader::new("Base generator")
        .default_open(true)
        .show(ui, |ui| {
            changed |= material_editor(ui, &mut recipe.material);
        });
    egui::CollapsingHeader::new("Recipe parameters")
        .default_open(true)
        .show(ui, |ui| {
            changed |= parameter_editor(ui, recipe, parameter_name_edit);
        });
    egui::CollapsingHeader::new("Displacement and normals")
        .default_open(true)
        .show(ui, |ui| {
            changed |= f32_row(ui, "Minimum (m)", &mut recipe.displacement.minimum_m, 0.001);
            changed |= f32_row(ui, "Maximum (m)", &mut recipe.displacement.maximum_m, 0.001);
            changed |= f32_row(ui, "Base (m)", &mut recipe.displacement.base_m, 0.001);
            changed |= ui
                .checkbox(
                    &mut recipe.displacement.displacement_map,
                    "Displacement map",
                )
                .changed();
            changed |= f32_row(ui, "Normal scale", &mut recipe.normal_scale, 0.01);
        });
    egui::CollapsingHeader::new("Occlusion")
        .default_open(false)
        .show(ui, |ui| {
            changed |= u8_row(ui, "Directions", &mut recipe.occlusion.directions, 1.0);
            changed |= u8_row(ui, "Samples", &mut recipe.occlusion.samples, 1.0);
            changed |= f32_row(ui, "Radius", &mut recipe.occlusion.radius, 0.01);
            changed |= f32_row(ui, "Maximum radius", &mut recipe.occlusion.max_radius, 0.01);
            changed |= f32_row(
                ui,
                "Cavity strength",
                &mut recipe.occlusion.cavity_strength,
                0.01,
            );
            changed |= f32_row(
                ui,
                "Horizon strength",
                &mut recipe.occlusion.horizon_strength,
                0.01,
            );
            changed |= f32_row(ui, "Power", &mut recipe.occlusion.power, 0.01);
            let weighted = matches!(
                recipe.occlusion.combine,
                OcclusionCombine::WeightedMinimum { .. }
            );
            let mut use_weighted = weighted;
            if ui.checkbox(&mut use_weighted, "Weighted minimum").changed() {
                recipe.occlusion.combine = if use_weighted {
                    OcclusionCombine::WeightedMinimum {
                        cavity_weight: 0.5,
                        horizon_weight: 0.5,
                    }
                } else {
                    OcclusionCombine::Multiply
                };
                changed = true;
            }
            if let OcclusionCombine::WeightedMinimum {
                cavity_weight,
                horizon_weight,
            } = &mut recipe.occlusion.combine
            {
                changed |= f32_row(ui, "Cavity weight", cavity_weight, 0.01);
                changed |= f32_row(ui, "Horizon weight", horizon_weight, 0.01);
            }
        });
    egui::CollapsingHeader::new("Albedo")
        .default_open(false)
        .show(ui, |ui| {
            changed |= colour_value_row(
                ui,
                "Base colour",
                &mut recipe.albedo.base_color,
                &recipe.parameters,
            );
            changed |= colour_value_row(
                ui,
                "Warm colour",
                &mut recipe.albedo.warm_color,
                &recipe.parameters,
            );
            changed |= palette_editor(ui, &mut recipe.albedo.palette, &recipe.parameters);
            changed |= f32_row(ui, "Variation", &mut recipe.albedo.variation, 0.01);
            changed |= f32_row(
                ui,
                "Crack darkening",
                &mut recipe.albedo.crack_darkening,
                0.01,
            );
            changed |= f32_row(
                ui,
                "Shoulder variation",
                &mut recipe.albedo.shoulder_variation,
                0.01,
            );
            changed |= f32_row(
                ui,
                "Mineral density",
                &mut recipe.albedo.mineral_density,
                0.01,
            );
            changed |= f32_row(
                ui,
                "Mineral brightness",
                &mut recipe.albedo.mineral_brightness,
                0.01,
            );
            changed |= f32_row(
                ui,
                "Occlusion influence",
                &mut recipe.albedo.occlusion_influence,
                0.01,
            );
        });
    egui::CollapsingHeader::new("Output profiles")
        .default_open(false)
        .show(ui, |ui| {
            changed |= profile_toggle(ui, recipe, RecipeOutputProfile::Separate, "Separate maps");
            changed |= profile_toggle(
                ui,
                recipe,
                RecipeOutputProfile::MotuUnityTerrain,
                "Motu Unity terrain",
            );
        });
    changed
}

fn material_editor(ui: &mut egui::Ui, material: &mut MaterialModel) -> bool {
    let mut changed = false;
    let current = match material {
        MaterialModel::LayeredNoise { .. } => 0,
        MaterialModel::CrackedStone { .. } => 1,
        MaterialModel::RoundedStones { .. } => 2,
        MaterialModel::Masonry(_) => 3,
        MaterialModel::WoodenPlanks(_) => 4,
        MaterialModel::SlateRoof(_) => 5,
        MaterialModel::ThatchRoof(_) => 6,
    };
    let mut selected = current;
    egui::ComboBox::from_label("Kind")
        .selected_text(
            [
                "Layered noise",
                "Cracked stone",
                "Rounded stones",
                "Coursed masonry",
                "Wooden planks",
                "Slate roof",
                "Thatch roof",
            ][selected],
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, 0, "Layered noise");
            ui.selectable_value(&mut selected, 1, "Cracked stone");
            ui.selectable_value(&mut selected, 2, "Rounded stones");
            ui.selectable_value(&mut selected, 3, "Coursed masonry");
            ui.selectable_value(&mut selected, 4, "Wooden planks");
            ui.selectable_value(&mut selected, 5, "Slate roof");
            ui.selectable_value(&mut selected, 6, "Thatch roof");
        });
    if selected != current {
        *material = material_default(selected);
        changed = true;
    }
    match material {
        MaterialModel::SlateRoof(settings) => {
            changed |= u32_row(ui, "Courses down slope (U)", &mut settings.courses, 1.0);
            changed |= u32_row(
                ui,
                "Slates across ridge (V)",
                &mut settings.slates_per_course,
                1.0,
            );
            changed |= f32_row(ui, "Seam width (m)", &mut settings.seam_width_m, 0.0005);
            changed |= f32_row(ui, "Edge bevel (m)", &mut settings.edge_bevel_m, 0.0005);
            changed |= f32_row(
                ui,
                "Overlap height (m)",
                &mut settings.overlap_height_m,
                0.0005,
            );
            changed |= f32_row(
                ui,
                "Slate cleavage (m)",
                &mut settings.surface_relief_m,
                0.0001,
            );
            changed |= f32_row_range(
                ui,
                "Slate colour variation",
                &mut settings.colour_variation,
                0.01,
                0.0..=1.0,
            );
        }
        MaterialModel::ThatchRoof(settings) => {
            changed |= u32_row(ui, "Bundles across ridge (V)", &mut settings.bundles, 1.0);
            changed |= u32_row(
                ui,
                "Strands per bundle",
                &mut settings.strands_per_bundle,
                1.0,
            );
            changed |= u32_row(ui, "Cut ends along slope (U)", &mut settings.courses, 1.0);
            changed |= f32_row(
                ui,
                "Straw relief (m)",
                &mut settings.strand_relief_m,
                0.0001,
            );
            changed |= f32_row(
                ui,
                "Packing relief (m)",
                &mut settings.bundle_relief_m,
                0.0005,
            );
            changed |= f32_row(
                ui,
                "Cut-tip relief (m)",
                &mut settings.course_relief_m,
                0.0005,
            );
            changed |= f32_row_range(
                ui,
                "Straw colour variation",
                &mut settings.colour_variation,
                0.01,
                0.0..=1.0,
            );
            ui.label("Straw fibres follow the roof slope along U.");
        }
        MaterialModel::WoodenPlanks(settings) => {
            changed |= u32_row(ui, "Boards across U", &mut settings.planks, 1.0);
            changed |= u32_row(
                ui,
                "End joints (0 = continuous)",
                &mut settings.joints_per_plank,
                1.0,
            );
            changed |= u32_row(ui, "Growth rings per board", &mut settings.grain_scale, 1.0);
            changed |= ui
                .checkbox(&mut settings.regular_stagger, "Regular half-board stagger")
                .changed();
            changed |= f32_row(ui, "Seam width (m)", &mut settings.seam_width_m, 0.0005);
            changed |= f32_row(ui, "Bevel width (m)", &mut settings.bevel_width_m, 0.0005);
            changed |= f32_row(ui, "Seam depth (m)", &mut settings.seam_depth_m, 0.0005);
            changed |= f32_row(
                ui,
                "Grain relief (m)",
                &mut settings.grain_strength_m,
                0.00005,
            );
            changed |= f32_row(
                ui,
                "Board relief variation (m)",
                &mut settings.plank_variation_m,
                0.00005,
            );
            changed |= f32_row_range(
                ui,
                "Board colour variation",
                &mut settings.colour_variation,
                0.01,
                0.0..=1.0,
            );
            ui.label("Longitudinal grain and board length run along V.");
        }
        MaterialModel::Masonry(settings) => {
            changed |= u32_row(ui, "Stones per course", &mut settings.cells_x, 1.0);
            changed |= u32_row(ui, "Courses (even)", &mut settings.cells_y, 2.0);
            changed |= f32_row_range(
                ui,
                "Joint variation",
                &mut settings.joint_jitter,
                0.01,
                0.0..=0.2,
            );
            changed |= f32_row(ui, "Mortar width (m)", &mut settings.mortar_width, 0.001);
            changed |= f32_row(ui, "Stone bevel (m)", &mut settings.bevel_width, 0.001);
            changed |= f32_row(ui, "Mortar depth (m)", &mut settings.mortar_depth, 0.001);
            changed |= f32_row(
                ui,
                "Stone relief variation (m)",
                &mut settings.stone_variation,
                0.001,
            );
            changed |= f32_row(
                ui,
                "Chipped edges (m)",
                &mut settings.edge_roughness,
                0.0005,
            );
            changed |= f32_row(
                ui,
                "Surface grain (m)",
                &mut settings.surface_amplitude,
                0.0005,
            );
            changed |= f32_row_range(
                ui,
                "Stone colour variation",
                &mut settings.colour_variation,
                0.01,
                0.0..=1.0,
            );
        }
        MaterialModel::LayeredNoise {
            frequency,
            amplitude,
            octaves,
            lacunarity,
            gain,
            offset,
        } => {
            changed |= f32_row(ui, "Frequency", frequency, 0.1);
            changed |= f32_row(ui, "Amplitude", amplitude, 0.01);
            changed |= u8_row(ui, "Octaves", octaves, 1.0);
            changed |= f32_row(ui, "Lacunarity", lacunarity, 0.01);
            changed |= f32_row(ui, "Gain", gain, 0.01);
            changed |= f32_row(ui, "Offset", offset, 0.01);
        }
        MaterialModel::CrackedStone {
            cells_x,
            cells_y,
            cell_jitter,
            warp_amplitude,
            crack_width,
            shoulder_width,
            crack_depth,
            slab_variation,
            fracture_probability,
            fracture_depth,
            surface_amplitude,
            broad_variation,
        } => {
            changed |= u32_row(ui, "Cells X", cells_x, 1.0);
            changed |= u32_row(ui, "Cells Y", cells_y, 1.0);
            changed |= f32_row(ui, "Cell jitter", cell_jitter, 0.01);
            changed |= f32_row_range(
                ui,
                "Warp amplitude",
                warp_amplitude,
                0.01,
                0.0..=f64::from(CRACKED_STONE_MAX_WARP_AMPLITUDE),
            );
            changed |= f32_row_range(
                ui,
                "Crack width",
                crack_width,
                0.001,
                f64::EPSILON..=f64::from(f32::MAX),
            );
            changed |= f32_row_range(
                ui,
                "Shoulder width",
                shoulder_width,
                0.001,
                f64::from(*crack_width)..=f64::from(f32::MAX),
            );
            changed |= f32_row(ui, "Crack depth", crack_depth, 0.001);
            changed |= f32_row(ui, "Slab variation", slab_variation, 0.001);
            changed |= f32_row(ui, "Fracture probability", fracture_probability, 0.01);
            changed |= f32_row(ui, "Fracture depth", fracture_depth, 0.001);
            changed |= f32_row(ui, "Surface amplitude", surface_amplitude, 0.001);
            changed |= f32_row(ui, "Broad variation", broad_variation, 0.001);
        }
        MaterialModel::RoundedStones {
            cells_x,
            cells_y,
            stone_radius,
            cell_jitter,
            warp_amplitude,
            anisotropy,
            stone_height,
            stone_variation,
            gap_height,
            sand_amplitude,
            edge_softness,
        } => {
            changed |= u32_row(ui, "Cells X", cells_x, 1.0);
            changed |= u32_row(ui, "Cells Y", cells_y, 1.0);
            changed |= f32_row_range(
                ui,
                "Pebble size",
                stone_radius,
                0.01,
                f64::EPSILON..=f64::from(ROUNDED_STONES_MAX_RADIUS),
            );
            changed |= f32_row(ui, "Cell jitter", cell_jitter, 0.01);
            changed |= f32_row_range(
                ui,
                "Warp amplitude",
                warp_amplitude,
                0.01,
                0.0..=f64::from(ROUNDED_STONES_MAX_WARP_AMPLITUDE),
            );
            changed |= f32_row(ui, "Dome roundness", anisotropy, 0.01);
            changed |= f32_row(ui, "Stone height", stone_height, 0.001);
            changed |= f32_row(ui, "Stone variation", stone_variation, 0.001);
            changed |= f32_row_range(
                ui,
                "Gap height",
                gap_height,
                0.001,
                f64::from(f32::MIN)..=f64::from(*stone_height),
            );
            changed |= f32_row(ui, "Sand amplitude", sand_amplitude, 0.001);
            changed |= f32_row_range(
                ui,
                "Edge softness",
                edge_softness,
                0.001,
                f64::EPSILON..=f64::from(f32::MAX),
            );
        }
    }
    changed
}

fn material_default(kind: usize) -> MaterialModel {
    match kind {
        0 => MaterialModel::default(),
        1 => serde_json::from_value(serde_json::json!({ "kind": "cracked_stone" })).unwrap(),
        2 => serde_json::from_value(serde_json::json!({ "kind": "rounded_stones" })).unwrap(),
        3 => serde_json::from_value(serde_json::json!({ "kind": "masonry" })).unwrap(),
        4 => serde_json::from_value(serde_json::json!({ "kind": "wooden_planks" })).unwrap(),
        5 => serde_json::from_value(serde_json::json!({ "kind": "slate_roof" })).unwrap(),
        6 => serde_json::from_value(serde_json::json!({ "kind": "thatch_roof" })).unwrap(),
        _ => unreachable!("material selector has exactly seven variants"),
    }
}

fn draw_layer(
    ui: &mut egui::Ui,
    layer: &mut MaterialLayer,
    earlier_layers: &[MaterialLayer],
    parameters: &BTreeMap<String, ParameterDefinition>,
) -> bool {
    let mut changed = false;
    ui.heading("Layer inspector");
    changed |= text_row(ui, "Name", &mut layer.name);
    changed |= text_row(ui, "Stable ID", &mut layer.id);
    changed |= ui.checkbox(&mut layer.enabled, "Enabled").changed();
    egui::CollapsingHeader::new("Source")
        .default_open(true)
        .show(ui, |ui| changed |= source_editor(ui, &mut layer.source));
    egui::CollapsingHeader::new("Remap / curve")
        .default_open(true)
        .show(ui, |ui| changed |= remap_editor(ui, &mut layer.remap));
    egui::CollapsingHeader::new("Mask")
        .default_open(false)
        .show(ui, |ui| {
            changed |= mask_editor(ui, &mut layer.mask, earlier_layers);
        });
    egui::CollapsingHeader::new("Height output")
        .default_open(true)
        .show(ui, |ui| {
            changed |= ui
                .checkbox(&mut layer.outputs.height.enabled, "Enabled")
                .changed();
            changed |= height_blend_editor(ui, &mut layer.outputs.height.blend);
            changed |= f32_row(
                ui,
                "Strength (m)",
                &mut layer.outputs.height.strength_m,
                0.001,
            );
        });
    egui::CollapsingHeader::new("Albedo output")
        .default_open(true)
        .show(ui, |ui| {
            changed |= ui
                .checkbox(&mut layer.outputs.albedo.enabled, "Enabled")
                .changed();
            changed |= albedo_blend_editor(ui, &mut layer.outputs.albedo.blend);
            changed |= f32_row(ui, "Strength", &mut layer.outputs.albedo.strength, 0.01);
            changed |= colour_map_editor(ui, &mut layer.outputs.albedo.colour_map, parameters);
            changed |= f32_row(
                ui,
                "Hue influence",
                &mut layer.outputs.albedo.hue_influence,
                0.01,
            );
            changed |= f32_row(
                ui,
                "Saturation influence",
                &mut layer.outputs.albedo.saturation_influence,
                0.01,
            );
            changed |= f32_row(
                ui,
                "Value influence",
                &mut layer.outputs.albedo.value_influence,
                0.01,
            );
        });
    changed
}

fn source_editor(ui: &mut egui::Ui, source: &mut ScalarSource) -> bool {
    let mut changed = enum_combo(
        ui,
        "Kind",
        &mut source.kind,
        &[
            (SourceKind::Value, "Value"),
            (SourceKind::Fbm, "fBM"),
            (SourceKind::Billow, "Billow"),
            (SourceKind::Ridged, "Ridged"),
            (SourceKind::CellularDistance, "Cellular distance"),
            (SourceKind::CellularDistanceToEdge, "Cellular edge"),
            (SourceKind::CellularValue, "Cellular value"),
        ],
    );
    changed |= u32_row(ui, "Frequency", &mut source.frequency, 1.0);
    if source.kind.is_fractal() {
        changed |= u8_row(ui, "Octaves", &mut source.octaves, 1.0);
        changed |= f32_row(ui, "Lacunarity", &mut source.lacunarity, 0.01);
        changed |= f32_row(ui, "Gain", &mut source.gain, 0.01);
    }
    if source.kind.is_cellular() {
        changed |= f32_row(ui, "Cell jitter", &mut source.cellular_jitter, 0.01);
    }
    changed |= f32_row(ui, "Offset X", &mut source.offset[0], 0.01);
    changed |= f32_row(ui, "Offset Y", &mut source.offset[1], 0.01);
    changed |= u64_row(ui, "Seed domain", &mut source.seed_domain);
    let mut warp_enabled = source.domain_warp.is_some();
    if ui.checkbox(&mut warp_enabled, "Domain warp").changed() {
        source.domain_warp = warp_enabled.then(DomainWarpSettings::default);
        changed = true;
    }
    if let Some(warp) = &mut source.domain_warp {
        changed |= f32_row(ui, "Warp amplitude", &mut warp.amplitude, 0.01);
        changed |= u32_row(ui, "Warp frequency", &mut warp.frequency, 1.0);
        changed |= u8_row(ui, "Warp octaves", &mut warp.octaves, 1.0);
        changed |= f32_row(ui, "Warp lacunarity", &mut warp.lacunarity, 0.01);
        changed |= f32_row(ui, "Warp gain", &mut warp.gain, 0.01);
        changed |= u64_row(ui, "Warp seed domain", &mut warp.seed_domain);
    }
    changed
}

fn remap_editor(ui: &mut egui::Ui, remap: &mut ScalarRemap) -> bool {
    let mut changed = false;
    changed |= f32_row(ui, "Input minimum", &mut remap.input_min, 0.01);
    changed |= f32_row(ui, "Input maximum", &mut remap.input_max, 0.01);
    changed |= ui.checkbox(&mut remap.invert, "Invert").changed();
    changed |= f32_row(ui, "Contrast", &mut remap.contrast, 0.01);
    changed |= f32_row(ui, "Bias", &mut remap.bias, 0.01);
    changed |= ui.checkbox(&mut remap.clamp, "Clamp 0–1").changed();
    let mut curve_enabled = remap.curve.is_some();
    if ui.checkbox(&mut curve_enabled, "Custom curve").changed() {
        remap.curve = curve_enabled.then(|| {
            vec![
                RemapPoint {
                    position: 0.0,
                    value: 0.0,
                },
                RemapPoint {
                    position: 1.0,
                    value: 1.0,
                },
            ]
        });
        changed = true;
    }
    if let Some(points) = &mut remap.curve {
        let mut remove = None;
        for (index, point) in points.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                changed |= f32_row(ui, "Position", &mut point.position, 0.01);
                changed |= f32_row(ui, "Value", &mut point.value, 0.01);
                if ui.small_button("×").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            points.remove(index);
            changed = true;
        }
        if points.len() < 16 && ui.button("+ Curve point").clicked() {
            points.push(RemapPoint {
                position: 0.5,
                value: 0.5,
            });
            changed = true;
        }
    }
    changed
}

fn mask_editor(
    ui: &mut egui::Ui,
    mask: &mut Option<LayerMask>,
    earlier_layers: &[MaterialLayer],
) -> bool {
    let current = match mask {
        None => 0,
        Some(LayerMask::Own) => 1,
        Some(LayerMask::PreviousHeight { .. }) => 2,
        Some(LayerMask::Noise { .. }) => 3,
        Some(LayerMask::Layer { .. }) => 4,
    };
    let mut selected = current;
    egui::ComboBox::from_label("Mask kind")
        .selected_text(
            [
                "None",
                "Own scalar",
                "Previous height",
                "Noise",
                "Earlier layer",
            ][selected],
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, 0, "None");
            ui.selectable_value(&mut selected, 1, "Own scalar");
            ui.selectable_value(&mut selected, 2, "Previous height");
            ui.selectable_value(&mut selected, 3, "Noise");
            ui.selectable_value(&mut selected, 4, "Earlier layer");
        });
    let mut changed = false;
    if selected != current {
        *mask = match selected {
            0 => None,
            1 => Some(LayerMask::Own),
            2 => Some(LayerMask::PreviousHeight {
                bottom_m: -0.01,
                top_m: 0.01,
                invert: false,
            }),
            3 => Some(LayerMask::Noise {
                source: ScalarSource::default(),
                remap: ScalarRemap::default(),
            }),
            4 => Some(LayerMask::Layer {
                layer_id: earlier_layers
                    .iter()
                    .next_back()
                    .map_or_else(String::new, |layer| layer.id.clone()),
                remap: ScalarRemap::default(),
            }),
            _ => unreachable!(),
        };
        changed = true;
    }
    match mask {
        None | Some(LayerMask::Own) => {}
        Some(LayerMask::PreviousHeight {
            bottom_m,
            top_m,
            invert,
        }) => {
            changed |= f32_row(ui, "Bottom height (m)", bottom_m, 0.001);
            changed |= f32_row(ui, "Top height (m)", top_m, 0.001);
            changed |= ui.checkbox(invert, "Invert (select low areas)").changed();
        }
        Some(LayerMask::Noise { source, remap }) => {
            changed |= source_editor(ui, source);
            changed |= remap_editor(ui, remap);
        }
        Some(LayerMask::Layer { layer_id, remap }) => {
            egui::ComboBox::from_label("Earlier layer")
                .selected_text(if layer_id.is_empty() {
                    "Select…"
                } else {
                    layer_id.as_str()
                })
                .show_ui(ui, |ui| {
                    for layer in earlier_layers {
                        changed |= ui
                            .selectable_value(layer_id, layer.id.clone(), &layer.name)
                            .changed();
                    }
                });
            changed |= remap_editor(ui, remap);
        }
    }
    changed
}

fn height_blend_editor(ui: &mut egui::Ui, blend: &mut HeightBlend) -> bool {
    let current = match blend {
        HeightBlend::Replace => 0,
        HeightBlend::Add => 1,
        HeightBlend::Subtract => 2,
        HeightBlend::Multiply => 3,
        HeightBlend::Minimum => 4,
        HeightBlend::Maximum => 5,
        HeightBlend::Lerp { .. } => 6,
    };
    let mut selected = current;
    egui::ComboBox::from_label("Blend")
        .selected_text(
            [
                "Replace", "Add", "Subtract", "Multiply", "Minimum", "Maximum", "Lerp",
            ][selected],
        )
        .show_ui(ui, |ui| {
            for (index, label) in [
                "Replace", "Add", "Subtract", "Multiply", "Minimum", "Maximum", "Lerp",
            ]
            .into_iter()
            .enumerate()
            {
                ui.selectable_value(&mut selected, index, label);
            }
        });
    let mut changed = selected != current;
    if changed {
        *blend = match selected {
            0 => HeightBlend::Replace,
            1 => HeightBlend::Add,
            2 => HeightBlend::Subtract,
            3 => HeightBlend::Multiply,
            4 => HeightBlend::Minimum,
            5 => HeightBlend::Maximum,
            6 => HeightBlend::Lerp { amount: 0.5 },
            _ => unreachable!(),
        };
    }
    if let HeightBlend::Lerp { amount } = blend {
        changed |= f32_row(ui, "Lerp amount", amount, 0.01);
    }
    changed
}

fn albedo_blend_editor(ui: &mut egui::Ui, blend: &mut AlbedoBlend) -> bool {
    enum_combo(
        ui,
        "Blend",
        blend,
        &[
            (AlbedoBlend::Replace, "Replace"),
            (AlbedoBlend::Mix, "Mix"),
            (AlbedoBlend::Multiply, "Multiply"),
            (AlbedoBlend::Add, "Add"),
            (AlbedoBlend::Overlay, "Overlay"),
        ],
    )
}

fn colour_map_editor(
    ui: &mut egui::Ui,
    map: &mut ColourMap,
    parameters: &BTreeMap<String, ParameterDefinition>,
) -> bool {
    let current = usize::from(matches!(map, ColourMap::Gradient { .. }));
    let mut selected = current;
    egui::ComboBox::from_label("Colour map")
        .selected_text(["Ramp", "Gradient"][selected])
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, 0, "Ramp");
            ui.selectable_value(&mut selected, 1, "Gradient");
        });
    let mut changed = selected != current;
    if changed {
        *map = if selected == 0 {
            ColourMap::default()
        } else {
            ColourMap::Gradient {
                stops: vec![
                    GradientStop {
                        position: 0.0,
                        colour: [0.2; 3].into(),
                    },
                    GradientStop {
                        position: 1.0,
                        colour: [0.8; 3].into(),
                    },
                ],
            }
        };
    }
    match map {
        ColourMap::Ramp { first, second } => {
            changed |= colour_value_row(ui, "First", first, parameters);
            changed |= colour_value_row(ui, "Second", second, parameters);
        }
        ColourMap::Gradient { stops } => {
            let mut remove = None;
            for (index, stop) in stops.iter_mut().enumerate() {
                ui.push_id(("gradient stop", index), |ui| {
                    ui.horizontal(|ui| {
                        changed |= f32_row(ui, "At", &mut stop.position, 0.01);
                        if ui.small_button("×").clicked() {
                            remove = Some(index);
                        }
                    });
                    changed |= colour_value_row(ui, "Colour", &mut stop.colour, parameters);
                });
            }
            if let Some(index) = remove {
                stops.remove(index);
                changed = true;
            }
            if stops.len() < 32 && ui.button("+ Gradient stop").clicked() {
                stops.push(GradientStop {
                    position: 0.5,
                    colour: [0.5; 3].into(),
                });
                changed = true;
            }
        }
    }
    changed
}

fn palette_editor(
    ui: &mut egui::Ui,
    colours: &mut Vec<ColourValue>,
    parameters: &BTreeMap<String, ParameterDefinition>,
) -> bool {
    let mut changed = false;
    let mut remove = None;
    for (index, colour) in colours.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("Colour {}", index + 1));
            if ui.small_button("×").clicked() {
                remove = Some(index);
            }
        });
        changed |= ui
            .push_id(("palette colour", index), |ui| {
                colour_value_row(ui, "Value", colour, parameters)
            })
            .inner;
    }
    if let Some(index) = remove {
        colours.remove(index);
        changed = true;
    }
    if ui.button("+ Palette colour").clicked() {
        colours.push([0.5; 3].into());
        changed = true;
    }
    changed
}

fn profile_toggle(
    ui: &mut egui::Ui,
    recipe: &mut TextureRecipe,
    profile: RecipeOutputProfile,
    label: &str,
) -> bool {
    let mut enabled = recipe.output_profiles.contains(&profile);
    if !ui.checkbox(&mut enabled, label).changed() {
        return false;
    }
    if enabled {
        recipe.output_profiles.push(profile);
    } else {
        recipe.output_profiles.retain(|current| *current != profile);
    }
    true
}

fn enum_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    variants: &[(T, &'static str)],
) -> bool {
    let selected = variants
        .iter()
        .find_map(|(variant, name)| (*variant == *value).then_some(*name))
        .unwrap_or("Unknown");
    let mut changed = false;
    egui::ComboBox::from_label(label)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for &(variant, name) in variants {
                changed |= ui.selectable_value(value, variant, name).changed();
            }
        });
    changed
}

fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value).changed()
    })
    .inner
}

fn colour_value_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut ColourValue,
    parameters: &BTreeMap<String, ParameterDefinition>,
) -> bool {
    let current = match value {
        ColourValue::Literal(_) => None,
        ColourValue::Parameter(reference) => Some(reference.parameter.as_str()),
    };
    let mut selected = current;
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(("colour source", label))
            .selected_text(current.unwrap_or("Literal"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut selected, None, "Literal");
                for name in parameters.keys() {
                    ui.selectable_value(&mut selected, Some(name.as_str()), name);
                }
            });
    });
    let mut changed = selected != current;
    if changed {
        *value = if let Some(parameter) = selected.map(str::to_owned) {
            let base = match value {
                ColourValue::Parameter(reference) => reference.base,
                ColourValue::Literal(_) => None,
            };
            ColourValue::Parameter(ColourParameterReference { parameter, base })
        } else {
            let literal = match value {
                ColourValue::Literal(colour) => *colour,
                ColourValue::Parameter(reference) => reference
                    .base
                    .or_else(|| {
                        parameters
                            .get(&reference.parameter)
                            .map(ParameterDefinition::default_colour)
                    })
                    .unwrap_or(LinearRgb::new(0.5, 0.5, 0.5)),
            };
            ColourValue::Literal(literal)
        };
    }

    match value {
        ColourValue::Literal(colour) => {
            changed |= ui.color_edit_button_rgb(&mut colour.0).changed();
        }
        ColourValue::Parameter(reference) => {
            let mut preserve_base = reference.base.is_some();
            if ui
                .checkbox(&mut preserve_base, "Preserve authored variation")
                .changed()
            {
                reference.base = preserve_base.then(|| {
                    parameters.get(&reference.parameter).map_or(
                        LinearRgb::new(0.5, 0.5, 0.5),
                        ParameterDefinition::default_colour,
                    )
                });
                changed = true;
            }
            if let Some(base) = &mut reference.base {
                ui.horizontal(|ui| {
                    ui.label("Authored base");
                    changed |= ui.color_edit_button_rgb(&mut base.0).changed();
                });
            }
        }
    }
    changed
}

fn parameter_editor(
    ui: &mut egui::Ui,
    recipe: &mut TextureRecipe,
    name_edit: &mut Option<ParameterNameEdit>,
) -> bool {
    let mut changed = false;
    let mut remove = None;
    for (name, definition) in &mut recipe.parameters {
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong(name);
            if ui.small_button("Rename").clicked() {
                *name_edit = Some(ParameterNameEdit {
                    original: Some(name.clone()),
                    name: name.clone(),
                    focus: true,
                });
            }
            if ui.small_button("Remove").clicked() {
                remove = Some(name.clone());
            }
        });
        match definition {
            ParameterDefinition::Colour {
                default,
                description,
            } => {
                changed |= ui.color_edit_button_rgb(&mut default.0).changed();
                changed |= text_row(ui, "Description", description);
            }
        }
    }
    if let Some(name) = remove {
        recipe.parameters.remove(&name);
        changed = true;
    }
    if recipe.parameters.len() < MAX_RECIPE_PARAMETERS && ui.button("+ Colour parameter").clicked()
    {
        *name_edit = Some(ParameterNameEdit {
            original: None,
            name: String::new(),
            focus: true,
        });
    }
    changed |= draw_parameter_name_edit(ui, recipe, name_edit);
    changed
}

fn draw_parameter_name_edit(
    ui: &mut egui::Ui,
    recipe: &mut TextureRecipe,
    name_edit: &mut Option<ParameterNameEdit>,
) -> bool {
    let Some(edit) = name_edit.as_mut() else {
        return false;
    };
    if let Some(original) = &edit.original {
        if !recipe.parameters.contains_key(original) {
            *name_edit = None;
            return false;
        }
        ui.label(format!("Rename {original}"));
    } else {
        ui.label("New colour parameter");
    }
    let response = ui.add(egui::TextEdit::singleline(&mut edit.name).hint_text("e.g. dirt_colour"));
    if std::mem::take(&mut edit.focus) {
        response.request_focus();
    }
    let unchanged = edit.original.as_deref() == Some(edit.name.as_str());
    let valid = is_valid_parameter_name(&edit.name);
    let duplicate = !unchanged && recipe.parameters.contains_key(&edit.name);
    if !edit.name.is_empty() && !valid {
        ui.label("Start with a lowercase letter. Use up to 64 lowercase letters, digits, or underscores.");
    } else if duplicate {
        ui.label("That parameter name is already in use.");
    }
    let ready = valid && !duplicate && !unchanged;
    let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    let (submit, cancel) = ui
        .horizontal(|ui| {
            let label = if edit.original.is_some() {
                "Apply name"
            } else {
                "Add parameter"
            };
            let submit = ui.add_enabled(ready, egui::Button::new(label)).clicked();
            (submit || ready && enter, ui.button("Cancel").clicked())
        })
        .inner;
    if cancel {
        *name_edit = None;
    } else if submit {
        let edit = name_edit.take().expect("active parameter name edit");
        if let Some(original) = edit.original {
            return rename_recipe_parameter(recipe, &original, &edit.name);
        }
        recipe.parameters.insert(
            edit.name,
            ParameterDefinition::Colour {
                default: LinearRgb::new(0.5, 0.5, 0.5),
                description: String::new(),
            },
        );
        return true;
    }
    false
}

#[cfg(test)]
mod colour_picker_tests {
    use super::*;
    use crate::{
        document::StudioDocument,
        file_io::{canonical_json_bytes, parse_recipe_bytes},
    };
    use std::path::Path;

    #[test]
    fn adding_a_colour_parameter_accepts_an_authored_name() {
        let mut recipe = StudioDocument::default_recipe();
        let mut edit = None;
        let ctx = egui::Context::default();
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    parameter_editor(ui, &mut recipe, &mut edit);
                },
            );
            output.textures_delta.clear();
            output
        };
        let initial = frame(Vec::new());
        let open = text_position(&initial, "+ Colour parameter");
        frame(pointer(open, true));
        frame(pointer(open, false));
        let named = frame(vec![egui::Event::Text("dirt_colour".into())]);
        let add = text_position(&named, "Add parameter");
        frame(pointer(add, true));
        frame(pointer(add, false));
        assert!(edit.is_none());
        assert_eq!(
            recipe.parameters.keys().collect::<Vec<_>>(),
            ["dirt_colour"]
        );
    }

    #[test]
    fn selecting_a_parameter_survives_redraws_and_recipe_serialization() {
        let mut recipe = StudioDocument::default_recipe();
        let dirt = LinearRgb::new(0.31, 0.17, 0.08);
        recipe.parameters.insert(
            "dirt_colour".into(),
            ParameterDefinition::Colour {
                default: dirt,
                description: String::new(),
            },
        );

        let ctx = egui::Context::default();
        let mut values = [
            recipe.albedo.base_color.clone(),
            recipe.albedo.base_color.clone(),
        ];
        let mut slot = 0;
        let mut frame = |events| {
            // The inspector edits a freshly cloned recipe each frame.
            values[slot ^ 1] = values[slot].clone();
            slot ^= 1;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    colour_value_row(ui, "Base colour", &mut values[slot], &recipe.parameters);
                },
            );
            output.textures_delta.clear();
            output
        };
        let initial = frame(Vec::new());
        let literal = text_position(&initial, "Literal");
        frame(pointer(literal, true));
        frame(pointer(literal, false));

        let menu = frame(Vec::new());
        let parameter = text_position(&menu, "dirt_colour");
        frame(pointer(parameter, true));
        frame(pointer(parameter, false));
        let selected_frame = frame(Vec::new());
        text_position(&selected_frame, "dirt_colour");

        recipe.albedo.base_color = values[slot].clone();
        let json = canonical_json_bytes(&recipe).unwrap();
        let reopened = parse_recipe_bytes(Path::new("dirt-test.json"), &json).unwrap();
        assert_eq!(
            reopened.recipe.albedo.base_color,
            ColourValue::Parameter(ColourParameterReference {
                parameter: "dirt_colour".into(),
                base: None,
            })
        );
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn text_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing picker label {label}"))
    }
}

fn f32_row(ui: &mut egui::Ui, label: &str, value: &mut f32, speed: f64) -> bool {
    f32_row_control(ui, label, value, speed, None)
}

fn f32_row_range(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
) -> bool {
    f32_row_control(ui, label, value, speed, Some(range))
}

fn f32_row_control(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    speed: f64,
    range: Option<std::ops::RangeInclusive<f64>>,
) -> bool {
    let previous = *value;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut control = egui::DragValue::new(value).speed(speed);
        if let Some(range) = range {
            control = control.range(range);
        }
        let changed = ui.add(control).changed();
        if changed && !value.is_finite() {
            *value = previous;
            ui.colored_label(egui::Color32::LIGHT_RED, "Finite number required");
            false
        } else {
            changed
        }
    })
    .inner
}

fn u8_row(ui: &mut egui::Ui, label: &str, value: &mut u8, speed: f64) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(speed)).changed()
    })
    .inner
}

fn u32_row(ui: &mut egui::Ui, label: &str, value: &mut u32, speed: f64) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(speed)).changed()
    })
    .inner
}

fn u64_row(ui: &mut egui::Ui, label: &str, value: &mut u64) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(1.0)).changed()
    })
    .inner
}
