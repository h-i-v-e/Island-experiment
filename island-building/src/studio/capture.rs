use super::{Settings, StudioState};
use bevy::{
    prelude::*,
    render::{
        render_resource::{TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, save_to_disk},
    },
};
use std::path::PathBuf;

#[derive(Resource)]
pub(super) struct CaptureTarget {
    pub image: Handle<Image>,
    path: PathBuf,
    frames: u32,
    requested: bool,
    wait: u32,
}

pub(super) fn install(app: &mut App, settings: &Settings) {
    let mut image = Image::new_target_texture(1600, 1100, TextureFormat::Rgba8UnormSrgb, None);
    image
        .texture_descriptor
        .usage
        .remove(TextureUsages::TEXTURE_BINDING);
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.insert_resource(CaptureTarget {
        image,
        path: settings.screenshot.clone().expect("capture path"),
        frames: 0,
        requested: false,
        wait: 0,
    })
    .add_systems(Update, capture);
}

fn capture(
    mut commands: Commands,
    mut target: ResMut<CaptureTarget>,
    state: Res<StudioState>,
    mut exit: MessageWriter<AppExit>,
) {
    target.wait += 1;
    if target.wait > 2400 {
        error!(
            "Capture timed out: {}",
            state.error.as_deref().unwrap_or(&state.status)
        );
        exit.write(AppExit::error());
        return;
    }
    if target.requested {
        target.frames += 1;
        if target.frames > 8 && std::fs::metadata(&target.path).is_ok_and(|file| file.len() > 0) {
            info!("Wrote {}", target.path.display());
            exit.write(AppExit::Success);
        }
        return;
    }
    if state.last_good.is_none() || state.running.is_some() || state.pending.is_some() {
        return;
    }
    target.frames += 1;
    if target.frames < 120 {
        return;
    }
    commands
        .spawn(Screenshot::image(target.image.clone()))
        .observe(save_to_disk(target.path.clone()));
    target.requested = true;
    target.frames = 0;
}
