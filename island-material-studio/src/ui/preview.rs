//! 2D map tabs, tiled image inspection, and the lit Bevy preview controls.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::too_many_lines
)]

use bevy_egui::egui;
use motu::procedural_textures::{FloatImage, PreviewMaps, TextureDimensions};

use crate::{
    preview::{PreviewAssets, PreviewState},
    preview_scene::{LitPreviewControls, LitPreviewTarget, PreviewShape},
};

use super::{PreviewTab, TileMode, UiAction, UiState};

/// Draw the right-hand preview panel.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    state: &mut UiState,
    preview: &PreviewState,
    assets: &PreviewAssets,
    target: Option<&LitPreviewTarget>,
    controls: &mut LitPreviewControls,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.heading("Preview");
    draw_tabs(ui, state, assets, target, &mut actions);
    ui.separator();
    if state.tab == PreviewTab::Lit {
        draw_lit(ui, state, target, controls, &mut actions);
    } else {
        draw_2d(ui, state, preview, assets, &mut actions);
    }
    actions
}

fn draw_tabs(
    ui: &mut egui::Ui,
    state: &mut UiState,
    assets: &PreviewAssets,
    target: Option<&LitPreviewTarget>,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal_wrapped(|ui| {
        for (tab, label) in PreviewTab::ALL {
            let available = tab == PreviewTab::Lit
                || tab_image(assets, tab).is_some()
                || (tab == PreviewTab::LayerRaw && assets.layer_raw.is_some())
                || (tab == PreviewTab::LayerRemapped && assets.layer_remapped.is_some())
                || (tab == PreviewTab::LayerMask && assets.layer_mask.is_some());
            if ui
                .add_enabled(
                    available || tab == PreviewTab::Lit && target.is_some(),
                    egui::Button::selectable(state.tab == tab, label),
                )
                .clicked()
            {
                state.tab = tab;
                actions.push(UiAction::MapTab(tab));
            }
        }
    });
}

fn draw_2d(
    ui: &mut egui::Ui,
    state: &mut UiState,
    preview: &PreviewState,
    assets: &PreviewAssets,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Tiles");
        if ui
            .selectable_value(&mut state.tile_mode, TileMode::One, "1×1")
            .changed()
        {
            actions.push(UiAction::TileMode(state.tile_mode));
        }
        if ui
            .selectable_value(&mut state.tile_mode, TileMode::TwoByTwo, "2×2")
            .changed()
        {
            actions.push(UiAction::TileMode(state.tile_mode));
        }
        ui.checkbox(&mut state.nearest_filtering, "Nearest")
            .on_hover_text("Nearest filtering is useful for pixel inspection");
        ui.label(&preview.status);
    });
    ui.horizontal(|ui| {
        if ui.button("Fit").clicked() {
            state.fit = true;
            actions.push(UiAction::ResetMapView);
        }
        if ui.button("Reset view").clicked() {
            state.fit = true;
            state.zoom = 1.0;
            state.pan = egui::Vec2::ZERO;
            actions.push(UiAction::ResetMapView);
        }
        ui.add(
            egui::Slider::new(&mut state.zoom, 0.05..=32.0)
                .logarithmic(true)
                .text("Zoom"),
        );
        if state.fit {
            ui.small("fit");
        }
    });

    let Some((texture_id, dimensions)) = tab_image(assets, state.tab) else {
        ui.centered_and_justified(|ui| {
            ui.label("This map is unavailable until a valid preview is generated.");
        });
        return;
    };
    let viewport_size = egui::vec2(
        ui.available_width().max(180.0),
        ui.available_height().clamp(180.0, 640.0),
    );
    let (viewport, response) = ui.allocate_exact_size(viewport_size, egui::Sense::click_and_drag());
    if response.dragged_by(egui::PointerButton::Middle)
        || (response.dragged_by(egui::PointerButton::Primary)
            && ui.input(|input| input.key_down(egui::Key::Space)))
    {
        state.pan += response.drag_delta();
        state.fit = false;
    }
    if response.hovered() {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            state.zoom = (state.zoom * (scroll * 0.002).exp()).clamp(0.05, 32.0);
            state.fit = false;
        }
    }
    let tiles = state.tile_mode.count() as f32;
    let image_size = egui::vec2(dimensions.width as f32, dimensions.height as f32);
    let fit_scale = (viewport.width() / (image_size.x * tiles).max(1.0))
        .min(viewport.height() / (image_size.y * tiles).max(1.0))
        .clamp(0.05, 32.0);
    let scale = if state.fit { fit_scale } else { state.zoom };
    let tiled_size = image_size * scale * tiles;
    let origin = viewport.center() - tiled_size * 0.5 + state.pan;
    let painter = ui.painter_at(viewport);
    let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    for tile_y in 0..state.tile_mode.count() {
        for tile_x in 0..state.tile_mode.count() {
            let offset = egui::vec2(
                tile_x as f32 * image_size.x * scale,
                tile_y as f32 * image_size.y * scale,
            );
            let rect = egui::Rect::from_min_size(origin + offset, image_size * scale);
            painter.image(texture_id, rect, uv, egui::Color32::WHITE);
        }
    }
    painter.rect_stroke(
        viewport,
        egui::CornerRadius::ZERO,
        egui::Stroke::new(1.0, egui::Color32::DARK_GRAY),
        egui::StrokeKind::Inside,
    );
    let pixel = response
        .interact_pointer_pos()
        .and_then(|position| pixel_at(position, origin, scale, dimensions));
    state.hover_pixel = pixel;
    ui.horizontal_wrapped(|ui| {
        if let Some([x, y]) = pixel {
            ui.label(format!("Pixel ({x}, {y})"));
            if let Some(maps) = assets.maps.as_deref() {
                ui.monospace(pixel_readout(maps, state.tab, x, y));
            }
        } else {
            ui.label("Hover a map pixel for values");
        }
    });
    if let Some(error) = &preview.error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    }
}

fn draw_lit(
    ui: &mut egui::Ui,
    _state: &mut UiState,
    target: Option<&LitPreviewTarget>,
    controls: &mut LitPreviewControls,
    actions: &mut Vec<UiAction>,
) {
    let Some(target) = target else {
        ui.centered_and_justified(|ui| ui.label("Lit preview is not available yet."));
        return;
    };
    let available = ui.available_size();
    let size = egui::vec2(available.x.max(180.0), available.y.clamp(180.0, 520.0));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    ui.painter_at(rect).image(
        target.image.texture_id,
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    let before = controls.clone();
    crate::preview_scene::interact(&response, controls);
    if controls_changed(&before, controls) {
        actions.push(UiAction::LitChanged);
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Shape");
        if ui
            .selectable_value(&mut controls.shape, PreviewShape::Sphere, "Sphere")
            .changed()
        {
            actions.push(UiAction::LitChanged);
        }
        if ui
            .selectable_value(&mut controls.shape, PreviewShape::Plane, "Plane")
            .changed()
        {
            actions.push(UiAction::LitChanged);
        }
        if ui.button("Reset view").clicked() {
            controls.reset_view();
            actions.push(UiAction::LitChanged);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for (value, label) in [
            (&mut controls.albedo, "Albedo"),
            (&mut controls.normal, "Normal"),
            (&mut controls.occlusion, "AO"),
            (&mut controls.height, "Parallax"),
        ] {
            if ui.checkbox(value, label).changed() {
                actions.push(UiAction::LitChanged);
            }
        }
    });
    let mut changed = false;
    changed |= ui
        .add(egui::Slider::new(&mut controls.tiling, 0.25..=8.0).text("Tiling"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut controls.roughness, 0.089..=1.0).text("Roughness"))
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut controls.light_azimuth_degrees, 0.0..=360.0)
                .text("Light azimuth"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut controls.light_elevation_degrees, 1.0..=89.0)
                .text("Light elevation"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut controls.light_intensity, 0.0..=20_000.0)
                .text("Light intensity"),
        )
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut controls.ambient_strength, 0.0..=2_000.0).text("Ambient"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut controls.height_scale, 0.0..=4.0).text("Height scale"))
        .changed();
    if changed {
        actions.push(UiAction::LitChanged);
    }
}

fn controls_changed(before: &LitPreviewControls, after: &LitPreviewControls) -> bool {
    before.shape != after.shape
        || before.albedo != after.albedo
        || before.normal != after.normal
        || before.occlusion != after.occlusion
        || before.height != after.height
        || before.tiling != after.tiling
        || before.roughness != after.roughness
        || before.light_azimuth_degrees != after.light_azimuth_degrees
        || before.light_elevation_degrees != after.light_elevation_degrees
        || before.light_intensity != after.light_intensity
        || before.ambient_strength != after.ambient_strength
        || before.height_scale != after.height_scale
        || before.orbit_yaw != after.orbit_yaw
        || before.orbit_pitch != after.orbit_pitch
        || before.camera_distance != after.camera_distance
}

fn tab_image(
    assets: &PreviewAssets,
    tab: PreviewTab,
) -> Option<(egui::TextureId, TextureDimensions)> {
    let image = match tab {
        PreviewTab::Albedo => assets.albedo.as_ref(),
        PreviewTab::Height => assets.height.as_ref(),
        PreviewTab::Normal => assets.normal.as_ref(),
        PreviewTab::Occlusion => assets.occlusion.as_ref(),
        PreviewTab::PackedMask => assets.packed_mask.as_ref(),
        PreviewTab::LayerRaw => assets.layer_raw.as_ref(),
        PreviewTab::LayerRemapped => assets.layer_remapped.as_ref(),
        PreviewTab::LayerMask => assets.layer_mask.as_ref(),
        PreviewTab::Lit => None,
    }?;
    let dimensions = assets.maps.as_ref()?.textures.dimensions;
    Some((image.texture_id, dimensions))
}

fn pixel_at(
    position: egui::Pos2,
    origin: egui::Pos2,
    scale: f32,
    dimensions: TextureDimensions,
) -> Option<[u32; 2]> {
    if scale <= 0.0 {
        return None;
    }
    let x = ((position.x - origin.x) / scale).floor();
    let y = ((position.y - origin.y) / scale).floor();
    if x < 0.0 || y < 0.0 {
        return None;
    }
    let x = x as u32;
    let y = y as u32;
    (x < dimensions.width && y < dimensions.height).then_some([x, y])
}

fn pixel_readout(maps: &PreviewMaps, tab: PreviewTab, x: u32, y: u32) -> String {
    let index = y as usize * maps.textures.dimensions.width as usize + x as usize;
    match tab {
        PreviewTab::Albedo => format!("RGB {:?}", maps.textures.albedo.pixels()[index]),
        PreviewTab::Height => {
            let value = maps.textures.height.pixels()[index];
            let normalized = f32::from(value) / f32::from(u16::MAX);
            format!("R16 {value} ({normalized:.4})")
        }
        PreviewTab::Normal => format!("RGB {:?}", maps.textures.normal.pixels()[index]),
        PreviewTab::Occlusion => format!("AO {}", maps.textures.occlusion.pixels()[index]),
        PreviewTab::PackedMask => maps.packed_mask.as_ref().map_or_else(
            || "Unavailable".into(),
            |mask| format!("RGBA {:?}", mask.pixels()[index]),
        ),
        PreviewTab::LayerRaw => {
            float_readout(maps.selected_layer.as_ref().map(|layer| &layer.raw), index)
        }
        PreviewTab::LayerRemapped => float_readout(
            maps.selected_layer.as_ref().map(|layer| &layer.remapped),
            index,
        ),
        PreviewTab::LayerMask => {
            float_readout(maps.selected_layer.as_ref().map(|layer| &layer.mask), index)
        }
        PreviewTab::Lit => "Lit preview".into(),
    }
}

fn float_readout(image: Option<&FloatImage>, index: usize) -> String {
    image.map_or_else(
        || "Unavailable".into(),
        |image| format!("{:.5}", image.pixels()[index]),
    )
}
