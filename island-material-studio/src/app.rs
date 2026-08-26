//! Bevy application composition and top-level document ownership.

use std::{fs, path::PathBuf, time::Duration};

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::{ExitCondition, WindowResolution},
};
use bevy_egui::{EguiGlobalSettings, EguiPlugin, PrimaryEguiContext};

use crate::{
    bake::BakePlugin,
    document::StudioDocument,
    preview::{PreviewPlugin, PreviewState},
    preview_scene::LitPreviewPlugin,
    settings::StudioSettings,
    ui::{PreviewTab, StudioUiPlugin, UiState},
};

pub const WINDOW_SIZE: UVec2 = UVec2::new(1440, 900);

/// Bevy resource wrapper for the UI-thread-owned document.
#[derive(Resource)]
pub struct DocumentResource(pub StudioDocument);

/// Startup options shared by the normal binary and visual acceptance runs.
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    pub recipe_path: Option<PathBuf>,
    pub window_size: Option<UVec2>,
    pub screenshot_path: Option<PathBuf>,
    pub preview_tab: Option<String>,
}

#[derive(Resource)]
struct PersistedSettings {
    path: Option<PathBuf>,
    last_saved: StudioSettings,
    timer: Timer,
}

#[derive(Resource)]
struct AcceptanceCapture {
    path: PathBuf,
    ready_frames: u16,
    requested: bool,
}

/// Builds and runs the desktop editor.
pub fn run(options: RunOptions) -> AppExit {
    let settings_path = StudioSettings::default_path("Island Material Studio");
    let settings = settings_path
        .as_deref()
        .map(StudioSettings::load_or_default)
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!("Could not load studio settings: {error}");
            None
        })
        .unwrap_or_default();
    let document = options
        .recipe_path
        .as_deref()
        .map_or_else(|| Ok(StudioDocument::new_default()), StudioDocument::open);
    let document = match document {
        Ok(document) => document,
        Err(error) => {
            eprintln!("Could not open startup recipe: {error}");
            StudioDocument::new_default()
        }
    };
    let window_size = options
        .window_size
        .unwrap_or(UVec2::from_array(settings.window_size));
    let mut preview_state = PreviewState::default();
    preview_state.auto_preview = settings.auto_preview;
    preview_state.resolution = preview_resolution(settings.preview_resolution);
    preview_state.nearest_filtering = settings.nearest_filter;
    let mut ui_state = UiState::default();
    ui_state.auto_preview = settings.auto_preview;
    ui_state.pending_resolution = preview_state.resolution;
    ui_state.tab = preview_tab(
        options
            .preview_tab
            .as_deref()
            .unwrap_or(&settings.selected_map),
    );
    ui_state.nearest_filtering = settings.nearest_filter;
    // Keep startup generation coherent with the persisted preview controls.
    preview_state.status = "Preview has not been generated".into();

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Procedural Material Studio".into(),
            resolution: WindowResolution::new(window_size.x, window_size.y),
            ..default()
        }),
        // The UI resolves dirty-document close requests explicitly.
        exit_condition: ExitCondition::DontExit,
        ..default()
    }))
    .add_plugins((
        EguiPlugin {
            // bevy_egui does not support bindless textures on Metal. Opt out
            // up front instead of requesting them and logging a fallback.
            bindless_mode_array_size: None,
            ..default()
        },
        PreviewPlugin,
        LitPreviewPlugin,
        BakePlugin,
        StudioUiPlugin,
    ))
    .insert_resource(ClearColor(Color::srgb(0.025, 0.032, 0.04)))
    .insert_resource(EguiGlobalSettings {
        // The app owns a window camera and an off-screen lit-preview camera.
        // Automatic selection can attach the primary UI to the latter.
        auto_create_primary_context: false,
        ..default()
    })
    .insert_resource(DocumentResource(document))
    .insert_resource(preview_state)
    .insert_resource(ui_state)
    .insert_resource(PersistedSettings {
        path: settings_path,
        last_saved: settings,
        timer: Timer::new(Duration::from_secs(1), TimerMode::Repeating),
    })
    .add_systems(Startup, (setup_primary_camera, queue_initial_preview))
    .add_systems(Update, persist_settings);
    if let Some(path) = options.screenshot_path {
        if path.is_file()
            && let Err(error) = fs::remove_file(&path)
        {
            eprintln!("Could not replace acceptance screenshot: {error}");
        }
        app.insert_resource(AcceptanceCapture {
            path,
            ready_frames: 0,
            requested: false,
        })
        .add_systems(Update, capture_when_ready);
    }
    app.run()
}

fn queue_initial_preview(document: Res<DocumentResource>, mut preview: ResMut<PreviewState>) {
    preview.request(
        document.0.recipe_snapshot(),
        document.0.revision(),
        document.0.selected_layer_id().map(str::to_owned),
        true,
    );
}

fn setup_primary_camera(mut commands: Commands) {
    commands.spawn((Camera2d, PrimaryEguiContext));
}

fn persist_settings(
    time: Res<Time>,
    window: Single<&Window>,
    document: Res<DocumentResource>,
    ui: Res<UiState>,
    preview: Res<PreviewState>,
    mut persisted: ResMut<PersistedSettings>,
) {
    persisted.timer.tick(time.delta());
    if !persisted.timer.just_finished() {
        return;
    }
    let mut settings = StudioSettings {
        window_size: [window.physical_width(), window.physical_height()],
        preview_resolution: preview.resolution.pixels(),
        auto_preview: preview.auto_preview,
        nearest_filter: ui.nearest_filtering,
        selected_map: preview_tab_name(ui.tab).into(),
        ..persisted.last_saved.clone()
    };
    if let Some(path) = document.0.source_path() {
        settings.remember_recent(path);
    }
    if settings == persisted.last_saved {
        return;
    }
    let Some(path) = persisted.path.as_deref() else {
        return;
    };
    match settings.save(path) {
        Ok(()) => persisted.last_saved = settings,
        Err(error) => eprintln!("Could not save studio settings: {error}"),
    }
}

fn capture_when_ready(
    mut commands: Commands,
    preview: Res<crate::preview::PreviewAssets>,
    mut capture: ResMut<AcceptanceCapture>,
    mut exit: MessageWriter<AppExit>,
) {
    if capture.requested {
        if capture.path.is_file() {
            exit.write(AppExit::Success);
        }
        return;
    }
    if preview.maps.is_none() {
        return;
    }
    capture.ready_frames += 1;
    if capture.ready_frames < 45 {
        return;
    }
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(capture.path.clone()));
    capture.requested = true;
}

const fn preview_resolution(pixels: u32) -> crate::preview::PreviewResolution {
    match pixels {
        128 => crate::preview::PreviewResolution::Small,
        512 => crate::preview::PreviewResolution::Large,
        _ => crate::preview::PreviewResolution::Medium,
    }
}

fn preview_tab(name: &str) -> PreviewTab {
    match name {
        "height" => PreviewTab::Height,
        "normal" => PreviewTab::Normal,
        "occlusion" => PreviewTab::Occlusion,
        "packed_mask" => PreviewTab::PackedMask,
        "layer_raw" => PreviewTab::LayerRaw,
        "layer_remapped" => PreviewTab::LayerRemapped,
        "layer_mask" => PreviewTab::LayerMask,
        "lit" => PreviewTab::Lit,
        _ => PreviewTab::Albedo,
    }
}

const fn preview_tab_name(tab: PreviewTab) -> &'static str {
    match tab {
        PreviewTab::Albedo => "albedo",
        PreviewTab::Height => "height",
        PreviewTab::Normal => "normal",
        PreviewTab::Occlusion => "occlusion",
        PreviewTab::PackedMask => "packed_mask",
        PreviewTab::LayerRaw => "layer_raw",
        PreviewTab::LayerRemapped => "layer_remapped",
        PreviewTab::LayerMask => "layer_mask",
        PreviewTab::Lit => "lit",
    }
}
