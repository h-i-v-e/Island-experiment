//! Exhaustive typed recipe inspector.

#![allow(clippy::too_many_lines)]

use bevy_egui::egui;
use motu::procedural_textures::{
    AlbedoBlend, ColourMap, DomainWarpSettings, GradientStop, HeightBlend, LayerMask,
    MaterialLayer, MaterialModel, NormalConvention, RemapPoint, ScalarRemap, ScalarSource,
    SourceKind, TextureRecipe,
    recipe::{OcclusionCombine, OutputProfile as RecipeOutputProfile},
};

/// Draws either the base recipe or the selected layer and reports mutations.
pub fn draw(
    ui: &mut egui::Ui,
    recipe: &mut TextureRecipe,
    selected_layer_id: Option<&str>,
) -> bool {
    let Some(layer_id) = selected_layer_id else {
        return draw_base(ui, recipe);
    };
    let Some(index) = recipe.layers.iter().position(|layer| layer.id == layer_id) else {
        ui.colored_label(
            egui::Color32::YELLOW,
            "The selected layer no longer exists.",
        );
        return false;
    };
    let (earlier_layers, selected_and_after) = recipe.layers.split_at_mut(index);
    let layer = &mut selected_and_after[0];
    draw_layer(ui, layer, earlier_layers)
}

fn draw_base(ui: &mut egui::Ui, recipe: &mut TextureRecipe) -> bool {
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
            let mut direct_x = recipe.normal_convention == NormalConvention::DirectX;
            if ui.checkbox(&mut direct_x, "DirectX normal Y").changed() {
                recipe.normal_convention = if direct_x {
                    NormalConvention::DirectX
                } else {
                    NormalConvention::OpenGl
                };
                changed = true;
            }
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
            changed |= colour_row(ui, "Base colour", &mut recipe.albedo.base_color);
            changed |= colour_row(ui, "Warm colour", &mut recipe.albedo.warm_color);
            changed |= palette_editor(ui, &mut recipe.albedo.palette);
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
    };
    let mut selected = current;
    egui::ComboBox::from_label("Kind")
        .selected_text(["Layered noise", "Cracked stone", "Rounded stones"][selected])
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, 0, "Layered noise");
            ui.selectable_value(&mut selected, 1, "Cracked stone");
            ui.selectable_value(&mut selected, 2, "Rounded stones");
        });
    if selected != current {
        *material = material_default(selected);
        changed = true;
    }
    match material {
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
            changed |= f32_row(ui, "Warp amplitude", warp_amplitude, 0.01);
            changed |= f32_row(ui, "Crack width", crack_width, 0.001);
            changed |= f32_row(ui, "Shoulder width", shoulder_width, 0.001);
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
            changed |= f32_row(ui, "Pebble size", stone_radius, 0.01);
            changed |= f32_row(ui, "Cell jitter", cell_jitter, 0.01);
            changed |= f32_row(ui, "Warp amplitude", warp_amplitude, 0.01);
            changed |= f32_row(ui, "Dome roundness", anisotropy, 0.01);
            changed |= f32_row(ui, "Stone height", stone_height, 0.001);
            changed |= f32_row(ui, "Stone variation", stone_variation, 0.001);
            changed |= f32_row(ui, "Gap height", gap_height, 0.001);
            changed |= f32_row(ui, "Sand amplitude", sand_amplitude, 0.001);
            changed |= f32_row(ui, "Edge softness", edge_softness, 0.001);
        }
    }
    changed
}

fn material_default(kind: usize) -> MaterialModel {
    match kind {
        0 => MaterialModel::default(),
        1 => serde_json::from_value(serde_json::json!({ "kind": "cracked_stone" })).unwrap(),
        2 => serde_json::from_value(serde_json::json!({ "kind": "rounded_stones" })).unwrap(),
        _ => unreachable!("material selector has exactly three variants"),
    }
}

fn draw_layer(
    ui: &mut egui::Ui,
    layer: &mut MaterialLayer,
    earlier_layers: &[MaterialLayer],
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
            changed |= colour_map_editor(ui, &mut layer.outputs.albedo.colour_map);
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
        Some(LayerMask::Noise { .. }) => 2,
        Some(LayerMask::Layer { .. }) => 3,
    };
    let mut selected = current;
    egui::ComboBox::from_label("Mask kind")
        .selected_text(["None", "Own scalar", "Noise", "Earlier layer"][selected])
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, 0, "None");
            ui.selectable_value(&mut selected, 1, "Own scalar");
            ui.selectable_value(&mut selected, 2, "Noise");
            ui.selectable_value(&mut selected, 3, "Earlier layer");
        });
    let mut changed = false;
    if selected != current {
        *mask = match selected {
            0 => None,
            1 => Some(LayerMask::Own),
            2 => Some(LayerMask::Noise {
                source: ScalarSource::default(),
                remap: ScalarRemap::default(),
            }),
            3 => Some(LayerMask::Layer {
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

fn colour_map_editor(ui: &mut egui::Ui, map: &mut ColourMap) -> bool {
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
                        colour: [0.2; 3],
                    },
                    GradientStop {
                        position: 1.0,
                        colour: [0.8; 3],
                    },
                ],
            }
        };
    }
    match map {
        ColourMap::Ramp { first, second } => {
            changed |= colour_row(ui, "First", first);
            changed |= colour_row(ui, "Second", second);
        }
        ColourMap::Gradient { stops } => {
            let mut remove = None;
            for (index, stop) in stops.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    changed |= f32_row(ui, "At", &mut stop.position, 0.01);
                    changed |= ui.color_edit_button_rgb(&mut stop.colour).changed();
                    if ui.small_button("×").clicked() {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove {
                stops.remove(index);
                changed = true;
            }
            if stops.len() < 32 && ui.button("+ Gradient stop").clicked() {
                stops.push(GradientStop {
                    position: 0.5,
                    colour: [0.5; 3],
                });
                changed = true;
            }
        }
    }
    changed
}

fn palette_editor(ui: &mut egui::Ui, colours: &mut Vec<[f32; 3]>) -> bool {
    let mut changed = false;
    let mut remove = None;
    for (index, colour) in colours.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            changed |= ui.color_edit_button_rgb(colour).changed();
            if ui.small_button("×").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        colours.remove(index);
        changed = true;
    }
    if ui.button("+ Palette colour").clicked() {
        colours.push([0.5; 3]);
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

fn colour_row(ui: &mut egui::Ui, label: &str, value: &mut [f32; 3]) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.color_edit_button_rgb(value).changed()
    })
    .inner
}

fn f32_row(ui: &mut egui::Ui, label: &str, value: &mut f32, speed: f64) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(speed)).changed()
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
