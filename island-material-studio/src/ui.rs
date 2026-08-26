//! Fixed three-panel egui composition and document-lifecycle commands.

#![allow(clippy::struct_excessive_bools, clippy::too_many_lines)]

pub(crate) mod diagnostics;
pub(crate) mod inspector;
pub(crate) mod layer_stack;
pub(crate) mod preview;
pub(crate) mod toolbar;

use std::path::{Path, PathBuf};

use bevy::{app::AppExit, prelude::*, window::WindowCloseRequested};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use motu::procedural_textures::editor_protocol;

use crate::{
    app::DocumentResource,
    bake::BakeState,
    document::{DocumentError, StudioDocument},
    preview::{PreviewAssets, PreviewResolution, PreviewState},
    preview_scene::{LitPreviewControls, LitPreviewTarget},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreviewTab {
    #[default]
    Albedo,
    Height,
    Normal,
    Occlusion,
    PackedMask,
    LayerRaw,
    LayerRemapped,
    LayerMask,
    Lit,
}

impl PreviewTab {
    pub const ALL: [(Self, &'static str); 9] = [
        (Self::Albedo, "A"),
        (Self::Height, "H"),
        (Self::Normal, "N"),
        (Self::Occlusion, "AO"),
        (Self::PackedMask, "Mask"),
        (Self::LayerRaw, "Layer raw"),
        (Self::LayerRemapped, "Layer mapped"),
        (Self::LayerMask, "Layer mask"),
        (Self::Lit, "Lit"),
    ];
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TileMode {
    #[default]
    One,
    TwoByTwo,
}

impl TileMode {
    #[must_use]
    pub const fn count(self) -> u32 {
        match self {
            Self::One => 1,
            Self::TwoByTwo => 2,
        }
    }
}

#[derive(Clone, Debug)]
pub enum UiAction {
    New,
    Open,
    Save,
    SaveAs,
    Revert,
    Quit,
    Undo,
    Redo,
    Validate,
    Preview,
    Bake,
    SetAutoPreview(bool),
    SetPreviewResolution(PreviewResolution),
    SelectLayer(Option<String>),
    FocusPointer(String),
    MapTab(PreviewTab),
    TileMode(TileMode),
    ResetMapView,
    LitChanged,
}

#[derive(Clone, Copy)]
pub(crate) struct DocumentStatus<'a> {
    pub name: &'a str,
    pub path: Option<&'a str>,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}

#[derive(Resource)]
pub struct UiState {
    pub auto_preview: bool,
    pub pending_resolution: PreviewResolution,
    pub tab: PreviewTab,
    pub tile_mode: TileMode,
    pub nearest_filtering: bool,
    pub fit: bool,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub hover_pixel: Option<[u32; 2]>,
    pub status: String,
    pub focused_pointer: Option<String>,
    path_prompt: Option<PathPrompt>,
    pending_destructive: Option<DeferredAction>,
    external_conflict: bool,
    recipe_gesture_active: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            auto_preview: true,
            pending_resolution: PreviewResolution::Medium,
            tab: PreviewTab::Albedo,
            tile_mode: TileMode::One,
            nearest_filtering: false,
            fit: true,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            hover_pixel: None,
            status: "Ready".into(),
            focused_pointer: None,
            path_prompt: None,
            pending_destructive: None,
            external_conflict: false,
            recipe_gesture_active: false,
        }
    }
}

#[derive(Clone, Debug)]
struct PathPrompt {
    purpose: PathPurpose,
    value: String,
}

#[derive(Clone, Copy, Debug)]
enum PathPurpose {
    Open,
    SaveAs,
}

#[derive(Clone, Debug)]
enum DeferredAction {
    New,
    Open(PathBuf),
    Revert,
    Quit,
}

pub struct StudioUiPlugin;

impl Plugin for StudioUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiState>()
            .init_resource::<diagnostics::DiagnosticsState>()
            .add_systems(EguiPrimaryContextPass, draw_studio)
            .add_systems(Update, guard_window_close);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_studio(
    mut contexts: EguiContexts,
    mut document: ResMut<DocumentResource>,
    mut state: ResMut<UiState>,
    mut diagnostics: ResMut<diagnostics::DiagnosticsState>,
    mut preview_state: ResMut<PreviewState>,
    preview_assets: Res<PreviewAssets>,
    target: Option<Res<LitPreviewTarget>>,
    mut lit_controls: ResMut<LitPreviewControls>,
    mut bake: ResMut<BakeState>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let mut viewport_ui = egui::Ui::new(
        context.clone(),
        "studio viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(context.viewport_rect()),
    );
    if let Some(success) = bake.take_success() {
        document
            .0
            .record_successful_bake(success.recipe_hash.clone());
        state.status = format!("Bake complete: {}", success.manifest_path.display());
        bake.last_success = Some(success);
    }

    let source_path = document
        .0
        .source_path()
        .map(|path| path.display().to_string());
    let document_status = DocumentStatus {
        name: &document.0.recipe().name,
        path: source_path.as_deref(),
        dirty: document.0.is_dirty(),
        can_undo: document.0.history().can_undo(),
        can_redo: document.0.history().can_redo(),
    };
    let mut actions = Vec::new();
    egui::Panel::top("studio toolbar").show(&mut viewport_ui, |ui| {
        actions.extend(toolbar::draw(
            context,
            ui,
            &mut state,
            document_status,
            &preview_state,
            &mut bake,
        ));
    });

    let mut edited_recipe = document.0.recipe_snapshot();
    let mut selected_layer = document.0.selected_layer_id().map(str::to_owned);
    let mut recipe_changed = false;
    egui::Panel::left("layer stack")
        .default_size(260.0)
        .min_size(210.0)
        .show(&mut viewport_ui, |ui| {
            ui.heading("Base / Layers");
            recipe_changed |= layer_stack::draw(ui, &mut edited_recipe, &mut selected_layer);
        });
    egui::Panel::right("preview panel")
        .default_size(500.0)
        .min_size(330.0)
        .show(&mut viewport_ui, |ui| {
            actions.extend(preview::draw(
                ui,
                &mut state,
                &preview_state,
                &preview_assets,
                target.as_deref(),
                &mut lit_controls,
            ));
        });
    egui::Panel::bottom("studio status")
        .resizable(true)
        .default_size(112.0)
        .show(&mut viewport_ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                actions.extend(diagnostics::draw(
                    ui,
                    &diagnostics.entries,
                    document.0.recipe(),
                ));
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(&state.status);
                    ui.separator();
                    ui.label(&preview_state.status);
                    ui.separator();
                    ui.label(format!("Revision {}", document.0.revision()));
                    if let Some(pointer) = &state.focused_pointer {
                        ui.monospace(pointer);
                    }
                });
            });
        });
    egui::CentralPanel::default().show(&mut viewport_ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            recipe_changed |= inspector::draw(ui, &mut edited_recipe, selected_layer.as_deref());
        });
    });

    let pointer_down = context.input(|input| input.pointer.primary_down());
    if recipe_changed {
        if pointer_down && !state.recipe_gesture_active {
            document.0.begin_gesture();
            state.recipe_gesture_active = true;
        }
        document.0.edit(|recipe| *recipe = edited_recipe);
        document.0.select_layer(selected_layer.as_deref());
        refresh_diagnostics(&document.0, &mut diagnostics);
        queue_preview(&document.0, &mut preview_state, false);
    } else if selected_layer.as_deref() != document.0.selected_layer_id() {
        document.0.select_layer(selected_layer.as_deref());
        queue_preview(&document.0, &mut preview_state, false);
    }
    if state.recipe_gesture_active && !pointer_down {
        document.0.end_gesture();
        state.recipe_gesture_active = false;
    }

    if !context.egui_wants_keyboard_input()
        && context.input(|input| input.key_pressed(egui::Key::Delete))
        && let Some(selected) = document.0.selected_layer_id().map(str::to_owned)
    {
        match document.0.remove_layer(&selected) {
            Ok(_) => {
                refresh_diagnostics(&document.0, &mut diagnostics);
                queue_preview(&document.0, &mut preview_state, false);
            }
            Err(error) => report_document_error(error, &mut state),
        }
    }

    if state.nearest_filtering != preview_state.nearest_filtering {
        preview_state.nearest_filtering = state.nearest_filtering;
        queue_preview(&document.0, &mut preview_state, true);
    }
    for action in actions {
        handle_action(
            action,
            &mut document.0,
            &mut state,
            &mut diagnostics,
            &mut preview_state,
            &mut bake,
            &mut exit,
        );
    }
    draw_path_prompt(
        context,
        &mut document.0,
        &mut state,
        &mut diagnostics,
        &mut preview_state,
        &mut exit,
    );
    draw_dirty_prompt(
        context,
        &mut document.0,
        &mut state,
        &mut diagnostics,
        &mut preview_state,
        &mut exit,
    );
    draw_conflict_prompt(context, &mut document.0, &mut state);
}

#[allow(clippy::too_many_arguments)]
fn handle_action(
    action: UiAction,
    document: &mut StudioDocument,
    state: &mut UiState,
    diagnostics: &mut diagnostics::DiagnosticsState,
    preview: &mut PreviewState,
    bake: &mut BakeState,
    exit: &mut MessageWriter<AppExit>,
) {
    match action {
        UiAction::New => destructive_or_defer(document, state, DeferredAction::New),
        UiAction::Open => {
            state.path_prompt = Some(PathPrompt {
                purpose: PathPurpose::Open,
                value: document
                    .source_path()
                    .and_then(Path::parent)
                    .map_or_else(String::new, |path| path.display().to_string()),
            });
        }
        UiAction::Save => match document.save() {
            Ok(()) => state.status = "Recipe saved".into(),
            Err(DocumentError::NoSourcePath) => {
                state.path_prompt = Some(PathPrompt {
                    purpose: PathPurpose::SaveAs,
                    value: format!("{}.json", document.recipe().name),
                });
            }
            Err(error) => report_document_error(error, state),
        },
        UiAction::SaveAs => {
            state.path_prompt = Some(PathPrompt {
                purpose: PathPurpose::SaveAs,
                value: document.source_path().map_or_else(
                    || format!("{}.json", document.recipe().name),
                    |path| path.display().to_string(),
                ),
            });
        }
        UiAction::Revert => destructive_or_defer(document, state, DeferredAction::Revert),
        UiAction::Quit => destructive_or_defer(document, state, DeferredAction::Quit),
        UiAction::Undo => {
            if document.undo() {
                refresh_diagnostics(document, diagnostics);
                queue_preview(document, preview, false);
            }
        }
        UiAction::Redo => {
            if document.redo() {
                refresh_diagnostics(document, diagnostics);
                queue_preview(document, preview, false);
            }
        }
        UiAction::Validate => {
            refresh_diagnostics(document, diagnostics);
            state.status = if diagnostics.entries.is_empty() {
                "Recipe is valid".into()
            } else {
                format!("Recipe has {} issue(s)", diagnostics.entries.len())
            };
        }
        UiAction::Preview => queue_preview(document, preview, true),
        UiAction::Bake => match bake.request(document.recipe_snapshot()) {
            Ok(()) => state.status = "Final bake queued".into(),
            Err(error) => state.status = error,
        },
        UiAction::SetAutoPreview(enabled) => preview.auto_preview = enabled,
        UiAction::SetPreviewResolution(resolution) => {
            preview.resolution = resolution;
            queue_preview(document, preview, true);
        }
        UiAction::SelectLayer(id) => {
            document.select_layer(id.as_deref());
            queue_preview(document, preview, false);
        }
        UiAction::FocusPointer(pointer) => state.focused_pointer = Some(pointer),
        UiAction::MapTab(_)
        | UiAction::TileMode(_)
        | UiAction::ResetMapView
        | UiAction::LitChanged => {}
    }
    let _ = exit;
}

fn queue_preview(document: &StudioDocument, preview: &mut PreviewState, manual: bool) {
    preview.request(
        document.recipe_snapshot(),
        document.revision(),
        document.selected_layer_id().map(str::to_owned),
        manual,
    );
}

fn refresh_diagnostics(document: &StudioDocument, diagnostics: &mut diagnostics::DiagnosticsState) {
    diagnostics.entries = editor_protocol::validate_diagnostics(document.recipe());
    diagnostics.revision = document.revision();
}

fn destructive_or_defer(document: &StudioDocument, state: &mut UiState, action: DeferredAction) {
    if document.is_dirty() {
        state.pending_destructive = Some(action);
    } else {
        state.pending_destructive = Some(action);
        state.status = "Ready to continue".into();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_path_prompt(
    context: &egui::Context,
    document: &mut StudioDocument,
    state: &mut UiState,
    diagnostics: &mut diagnostics::DiagnosticsState,
    preview: &mut PreviewState,
    exit: &mut MessageWriter<AppExit>,
) {
    let Some(mut prompt) = state.path_prompt.take() else {
        return;
    };
    let mut keep = true;
    egui::Window::new(match prompt.purpose {
        PathPurpose::Open => "Open recipe",
        PathPurpose::SaveAs => "Save recipe as",
    })
    .collapsible(false)
    .resizable(false)
    .show(context, |ui| {
        ui.label("JSON recipe path");
        ui.add_sized([520.0, 24.0], egui::TextEdit::singleline(&mut prompt.value));
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                keep = false;
            }
            if ui.button("Continue").clicked() {
                let path = PathBuf::from(prompt.value.trim());
                match prompt.purpose {
                    PathPurpose::Open => {
                        if document.is_dirty() {
                            state.pending_destructive = Some(DeferredAction::Open(path));
                        } else if let Err(error) = execute_deferred(
                            DeferredAction::Open(path),
                            document,
                            state,
                            diagnostics,
                            preview,
                            exit,
                        ) {
                            report_document_error(error, state);
                        }
                    }
                    PathPurpose::SaveAs => match document.save_as(&path) {
                        Ok(()) => {
                            state.status = format!("Saved {}", path.display());
                            if let Some(action) = state.pending_destructive.take()
                                && let Err(error) = execute_deferred(
                                    action,
                                    document,
                                    state,
                                    diagnostics,
                                    preview,
                                    exit,
                                )
                            {
                                report_document_error(error, state);
                            }
                        }
                        Err(error) => report_document_error(error, state),
                    },
                }
                keep = false;
            }
        });
    });
    if keep {
        state.path_prompt = Some(prompt);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_dirty_prompt(
    context: &egui::Context,
    document: &mut StudioDocument,
    state: &mut UiState,
    diagnostics: &mut diagnostics::DiagnosticsState,
    preview: &mut PreviewState,
    exit: &mut MessageWriter<AppExit>,
) {
    let Some(action) = state.pending_destructive.clone() else {
        return;
    };
    if !document.is_dirty() {
        state.pending_destructive = None;
        if let Err(error) = execute_deferred(action, document, state, diagnostics, preview, exit) {
            report_document_error(error, state);
        }
        return;
    }
    egui::Window::new("Unsaved changes")
        .collapsible(false)
        .resizable(false)
        .show(context, |ui| {
            ui.label("Save this recipe before continuing?");
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    match document.save() {
                        Ok(()) => {
                            state.pending_destructive = None;
                            if let Err(error) = execute_deferred(
                                action.clone(),
                                document,
                                state,
                                diagnostics,
                                preview,
                                exit,
                            ) {
                                report_document_error(error, state);
                            }
                        }
                        Err(DocumentError::NoSourcePath) => {
                            state.path_prompt = Some(PathPrompt {
                                purpose: PathPurpose::SaveAs,
                                value: format!("{}.json", document.recipe().name),
                            });
                        }
                        Err(error) => report_document_error(error, state),
                    }
                }
                if ui.button("Discard").clicked() {
                    state.pending_destructive = None;
                    if let Err(error) = execute_deferred(
                        action.clone(),
                        document,
                        state,
                        diagnostics,
                        preview,
                        exit,
                    ) {
                        report_document_error(error, state);
                    }
                }
                if ui.button("Cancel").clicked() {
                    state.pending_destructive = None;
                }
            });
        });
}

fn draw_conflict_prompt(
    context: &egui::Context,
    document: &mut StudioDocument,
    state: &mut UiState,
) {
    if !state.external_conflict {
        return;
    }
    egui::Window::new("Recipe changed externally")
        .collapsible(false)
        .resizable(false)
        .show(context, |ui| {
            ui.label("Reload the disk version, Save As, or explicitly overwrite it.");
            ui.horizontal(|ui| {
                if ui.button("Reload").clicked() {
                    match document.revert() {
                        Ok(()) => state.status = "Reloaded external recipe".into(),
                        Err(error) => state.status = error.to_string(),
                    }
                    state.external_conflict = false;
                }
                if ui.button("Save As").clicked() {
                    state.path_prompt = Some(PathPrompt {
                        purpose: PathPurpose::SaveAs,
                        value: format!("{}-copy.json", document.recipe().name),
                    });
                    state.external_conflict = false;
                }
                if ui.button("Overwrite").clicked() {
                    match document.save_overwrite() {
                        Ok(()) => state.status = "External file overwritten explicitly".into(),
                        Err(error) => state.status = error.to_string(),
                    }
                    state.external_conflict = false;
                }
                if ui.button("Cancel").clicked() {
                    state.external_conflict = false;
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn execute_deferred(
    action: DeferredAction,
    document: &mut StudioDocument,
    state: &mut UiState,
    diagnostics: &mut diagnostics::DiagnosticsState,
    preview: &mut PreviewState,
    exit: &mut MessageWriter<AppExit>,
) -> Result<(), DocumentError> {
    match action {
        DeferredAction::New => *document = StudioDocument::new_default(),
        DeferredAction::Open(path) => document.open_replace(path)?,
        DeferredAction::Revert => document.revert()?,
        DeferredAction::Quit => {
            exit.write(AppExit::Success);
            return Ok(());
        }
    }
    refresh_diagnostics(document, diagnostics);
    queue_preview(document, preview, true);
    state.status = "Document loaded".into();
    Ok(())
}

fn report_document_error(error: DocumentError, state: &mut UiState) {
    state.external_conflict = matches!(error, DocumentError::ExternalChange(_));
    state.status = error.to_string();
}

fn guard_window_close(
    mut requests: MessageReader<WindowCloseRequested>,
    document: Res<DocumentResource>,
    mut state: ResMut<UiState>,
    mut exit: MessageWriter<AppExit>,
) {
    if requests.read().next().is_none() {
        return;
    }
    if document.0.is_dirty() {
        state.pending_destructive = Some(DeferredAction::Quit);
    } else {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_modes_have_the_expected_repeat_count() {
        assert_eq!(TileMode::One.count(), 1);
        assert_eq!(TileMode::TwoByTwo.count(), 2);
    }

    #[test]
    fn preview_tabs_cover_every_required_view() {
        assert_eq!(PreviewTab::ALL.len(), 9);
        assert!(
            PreviewTab::ALL
                .iter()
                .any(|(tab, _)| *tab == PreviewTab::Lit)
        );
    }
}
