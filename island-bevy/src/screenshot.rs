//! Unattended capture: render until the island has settled, write one PNG,
//! then exit. Used to verify the renderer without a person at the keyboard.

use std::{fs, path::PathBuf};

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

use crate::island_gen::GeneratedIsland;

/// Frames and seconds to hold after the island appears, so render pipelines
/// finish compiling and the meshes reach the GPU before the capture.
const SETTLE_FRAMES: u32 = 30;
const SETTLE_SECONDS: f32 = 2.0;
/// Frames to wait after the file appears, covering the tail of the write.
const FLUSH_FRAMES: u32 = 5;
/// Frames after the request before the capture is treated as failed.
const CAPTURE_TIMEOUT_FRAMES: u32 = 1_800;

pub struct ScreenshotPlugin {
    pub path: PathBuf,
}

#[derive(Resource)]
struct ScreenshotPath(PathBuf);

#[derive(Resource, Default)]
struct CaptureProgress {
    frames: u32,
    seconds: f32,
    requested: bool,
    frames_since_request: u32,
}

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ScreenshotPath(self.path.clone()))
            .init_resource::<CaptureProgress>()
            .add_systems(Startup, clear_previous)
            .add_systems(Update, capture);
    }
}

/// A stale file from an earlier run would otherwise satisfy the write check.
fn clear_previous(path: Res<ScreenshotPath>) {
    if let Err(error) = fs::remove_file(&path.0)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!("could not clear {}: {error}", path.0.display());
    }
}

fn capture(
    mut commands: Commands,
    path: Res<ScreenshotPath>,
    time: Res<Time>,
    island: Option<Res<GeneratedIsland>>,
    mut progress: ResMut<CaptureProgress>,
    mut exit: MessageWriter<AppExit>,
) {
    if progress.requested {
        progress.frames_since_request += 1;
        let written = fs::metadata(&path.0).is_ok_and(|file| file.len() > 0);
        if written && progress.frames_since_request > FLUSH_FRAMES {
            info!("wrote {}", path.0.display());
            exit.write(AppExit::Success);
        } else if progress.frames_since_request > CAPTURE_TIMEOUT_FRAMES {
            error!("screenshot was never written to {}", path.0.display());
            exit.write(AppExit::error());
        }
        return;
    }
    if island.is_none() {
        return;
    }
    progress.frames += 1;
    progress.seconds += time.delta_secs();
    if progress.frames < SETTLE_FRAMES || progress.seconds < SETTLE_SECONDS {
        return;
    }
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path.0.clone()));
    progress.requested = true;
}
