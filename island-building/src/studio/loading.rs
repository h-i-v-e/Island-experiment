//! Keep linked-recipe file reads outside the UI frame.
use super::{StudioDocument, StudioState};
use bevy::tasks::{IoTaskPool, futures::check_ready};
use std::path::PathBuf;

pub(super) struct OpenedDocument {
    revision: u64,
    serial: u64,
    result: Result<StudioDocument, String>,
}

pub(super) fn start(state: &mut StudioState, path: PathBuf) {
    if state.load_task.is_some() {
        return;
    }
    let revision = state.document.revision();
    let serial = state.serial;
    state.load_task = Some(IoTaskPool::get().spawn(async move {
        OpenedDocument {
            revision,
            serial,
            result: StudioDocument::open(&path).map_err(|e| e.to_string()),
        }
    }));
    state.status = "Opening recipe and linked materials…".into();
}

pub(super) fn poll(state: &mut StudioState) {
    if let Some(opened) = state.load_task.as_mut().and_then(check_ready) {
        state.load_task = None;
        finish(state, opened);
    }
}

fn finish(state: &mut StudioState, opened: OpenedDocument) {
    if opened.revision != state.document.revision() || opened.serial != state.serial {
        state.error=Some("Recipe changed while opening the file; current edits were retained. Open the file again when ready.".into());
        return;
    }
    match opened.result {
        Ok(document) => {
            state.document = document;
            state.anchors = super::anchors::Editor::default();
            state.error = None;
            state.request_preview();
        }
        Err(error) => {
            state.error = Some(error);
            state.status = "Could not open recipe · current draft retained".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingRecipe, studio::Settings};

    #[test]
    fn a_completed_load_never_overwrites_edits_made_while_it_was_running() {
        let mut state = StudioState::new(
            StudioDocument::new(BuildingRecipe::hut()),
            &Settings::default(),
        );
        let opened = OpenedDocument {
            revision: state.document.revision(),
            serial: state.serial,
            result: Ok(StudioDocument::new(BuildingRecipe::cottage())),
        };
        state.document.edit(|r| r.name = "Keep this draft".into());
        finish(&mut state, opened);
        assert_eq!(state.document.recipe().name, "Keep this draft");
        assert!(
            state
                .error
                .as_ref()
                .unwrap()
                .contains("edits were retained")
        );
    }

    #[test]
    fn successful_and_failed_loads_preserve_their_document_boundaries() {
        let mut state = StudioState::new(
            StudioDocument::new(BuildingRecipe::hut()),
            &Settings::default(),
        );
        let opened = OpenedDocument {
            revision: state.document.revision(),
            serial: state.serial,
            result: Ok(StudioDocument::new(BuildingRecipe::cottage())),
        };
        finish(&mut state, opened);
        assert_eq!(state.document.recipe().name, "Island cottage");
        let failed = OpenedDocument {
            revision: state.document.revision(),
            serial: state.serial,
            result: Err("missing linked material".into()),
        };
        finish(&mut state, failed);
        assert_eq!(state.document.recipe().name, "Island cottage");
        assert_eq!(state.error.as_deref(), Some("missing linked material"));
    }
}
