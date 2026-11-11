//! Native pickers must never block Bevy's UI callback or its main event loop.
use super::{StudioState, request_action};
use crate::studio::PendingAction;
use bevy::tasks::{IoTaskPool, futures::check_ready};
use rfd::AsyncFileDialog;
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub(super) enum MaterialTarget {
    Wall,
    Roof,
    Door,
    Chimney,
}

pub(super) enum Intent {
    Open,
    SaveAs { continue_pending: bool },
    Export,
    Material(MaterialTarget),
}

pub(in crate::studio) struct Completion {
    intent: Intent,
    path: Option<PathBuf>,
    revision: u64,
    serial: u64,
}

pub(super) fn start(state: &mut StudioState, intent: Intent) {
    if state.file_dialog.is_some() {
        return;
    }
    let revision = state.document.revision();
    let serial = state.serial;
    let mut dialog = AsyncFileDialog::new();
    if let Some(directory) = state.document.base_directory() {
        dialog = dialog.set_directory(directory);
    }
    dialog = match &intent {
        Intent::Open => dialog
            .set_title("Open building recipe")
            .add_filter("Building recipe", &["json"]),
        Intent::SaveAs { .. } => dialog
            .set_title("Save building recipe")
            .add_filter("Building recipe", &["json"])
            .set_file_name("building.json"),
        Intent::Export => dialog
            .set_title("Choose a new package directory")
            .set_file_name("building-package"),
        Intent::Material(_) => dialog
            .set_title("Choose procedural material recipe")
            .add_filter("Material recipe", &["json"]),
    };
    state.file_dialog = Some(IoTaskPool::get().spawn(async move {
        // Creating the modal future also dispatches to AppKit's main queue.
        // Both construction and polling therefore happen outside the ECS frame.
        let path = match intent {
            Intent::Open | Intent::Material(_) => dialog.pick_file().await,
            Intent::SaveAs { .. } | Intent::Export => dialog.save_file().await,
        }
        .map(PathBuf::from);
        Completion {
            intent,
            path,
            revision,
            serial,
        }
    }));
    state.status = "Waiting for file selection…".into();
}

pub(super) fn use_wooden_planks(state: &mut StudioState) {
    if state.file_dialog.is_some() {
        return;
    }
    let revision = state.document.revision();
    let serial = state.serial;
    state.file_dialog = Some(IoTaskPool::get().spawn(async move {
        let bundled = std::env::current_exe().ok().and_then(|path| {
            path.parent()?
                .parent()
                .map(|contents| contents.join("Resources/material-recipes/wooden-planks.json"))
        });
        let path = bundled.filter(|path| path.is_file()).unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../island-rs/texture-recipes/wooden-planks.json")
        });
        Completion {
            intent: Intent::Material(MaterialTarget::Door),
            path: Some(path),
            revision,
            serial,
        }
    }));
    state.status = "Selecting wooden planks…".into();
}

pub(super) fn poll(state: &mut StudioState) {
    let Some(completion) = state.file_dialog.as_mut().and_then(check_ready) else {
        return;
    };
    state.file_dialog = None;
    finish(state, completion);
}

fn finish(state: &mut StudioState, completion: Completion) {
    let Some(path) = completion.path else {
        state.status = "File selection cancelled".into();
        return;
    };
    if completion.revision != state.document.revision() || completion.serial != state.serial {
        state.error = Some("Recipe changed during file selection. Select the file again.".into());
        return;
    }
    match completion.intent {
        Intent::Open => request_action(state, PendingAction::Open(path)),
        Intent::SaveAs { continue_pending } => {
            let path = if path.extension().is_none() {
                path.with_extension("json")
            } else {
                path
            };
            let result = state
                .document
                .save_as(&path)
                .map_err(|error| error.to_string());
            finish_save(state, result, continue_pending);
        }
        Intent::Export => state.request_export(path),
        Intent::Material(target) => {
            let path = path.to_string_lossy().into_owned();
            state.document.edit(|recipe| match target {
                MaterialTarget::Wall => recipe.materials.wall_recipe = Some(path),
                MaterialTarget::Roof => recipe.materials.roof_recipe = Some(path),
                MaterialTarget::Door => recipe.materials.door_recipe = Some(path),
                MaterialTarget::Chimney => recipe.materials.chimney_recipe = Some(path),
            });
            // Explicit reselection also refreshes a dependency edited in Material Studio.
            state.request_preview();
            state.status = "Material recipe selected".into();
        }
    }
}

pub(super) fn finish_save(
    state: &mut StudioState,
    result: Result<(), String>,
    continue_pending: bool,
) {
    match result {
        Ok(()) => {
            state.status = "Recipe saved".into();
            state.error = None;
            if continue_pending && let Some(action) = state.pending_action.take() {
                state.apply_action(action);
            }
        }
        Err(error) => state.error = Some(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingRecipe, document::StudioDocument, studio::Settings};

    fn state() -> StudioState {
        StudioState::new(
            StudioDocument::new(BuildingRecipe::hut()),
            &Settings::default(),
        )
    }

    #[test]
    fn material_selection_targets_the_right_surface_and_can_be_undone() {
        for (selected, target) in [
            MaterialTarget::Wall,
            MaterialTarget::Roof,
            MaterialTarget::Door,
            MaterialTarget::Chimney,
        ]
        .into_iter()
        .enumerate()
        {
            let mut state = state();
            let revision = state.document.revision();
            let serial = state.serial;
            finish(
                &mut state,
                Completion {
                    intent: Intent::Material(target),
                    path: Some(PathBuf::from("/tmp/material.json")),
                    revision,
                    serial,
                },
            );
            let material = &state.document.recipe().materials;
            for (index, path) in [
                material.wall_recipe.as_deref(),
                material.roof_recipe.as_deref(),
                material.door_recipe.as_deref(),
                material.chimney_recipe.as_deref(),
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(path, (index == selected).then_some("/tmp/material.json"));
            }
            state.document.undo();
            assert!(state.document.recipe().materials.wall_recipe.is_none());
            assert!(state.document.recipe().materials.roof_recipe.is_none());
            assert!(state.document.recipe().materials.door_recipe.is_none());
            assert!(state.document.recipe().materials.chimney_recipe.is_none());
        }
    }

    #[test]
    fn cancelled_and_stale_selections_preserve_the_current_recipe_and_continuation() {
        let mut state = state();
        let revision = state.document.revision();
        let serial = state.serial;
        state.pending_action = Some(PendingAction::Quit);
        finish(
            &mut state,
            Completion {
                intent: Intent::SaveAs {
                    continue_pending: true,
                },
                path: None,
                revision,
                serial,
            },
        );
        assert!(matches!(state.pending_action, Some(PendingAction::Quit)));
        assert!(!state.exit_requested);
        assert_eq!(state.document.revision(), revision);
        state.document.edit(|recipe| recipe.seed += 1);
        finish(
            &mut state,
            Completion {
                intent: Intent::Material(MaterialTarget::Wall),
                path: Some(PathBuf::from("/tmp/stale.json")),
                revision,
                serial,
            },
        );
        assert!(state.document.recipe().materials.wall_recipe.is_none());
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|error| error.contains("changed"))
        );
        finish_save(&mut state, Err("write failed".into()), true);
        assert!(matches!(state.pending_action, Some(PendingAction::Quit)));
        assert!(!state.exit_requested);
    }

    #[test]
    fn selection_cannot_modify_a_replacement_document_with_the_same_revision() {
        let mut state = state();
        let revision = state.document.revision();
        let serial = state.serial;
        state.apply_action(PendingAction::New("cottage".into()));
        assert_eq!(state.document.revision(), revision);
        finish(
            &mut state,
            Completion {
                intent: Intent::Material(MaterialTarget::Wall),
                path: Some(PathBuf::from("/tmp/previous-document.json")),
                revision,
                serial,
            },
        );
        assert!(state.document.recipe().materials.wall_recipe.is_none());
        assert!(state.error.is_some());
    }
}
