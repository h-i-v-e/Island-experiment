//! Free-flying observer camera.
//!
//! WASD moves on the view plane, Space and Shift move along world up and down,
//! holding the left mouse button drags the ground along under the cursor,
//! holding the right mouse button looks around with the cursor grabbed, the
//! scroll wheel zooms along the view direction, R returns to the opening view,
//! and Escape releases the cursor.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use motu::ISLAND_WORLD_METRES;

use crate::lighting::SKY_COLOUR;

/// The opening view, far enough out at 700 m to frame the whole island.
const HOME_EYE: Vec3 = Vec3::new(1_150.0, 700.0, 1_150.0);
const HOME_TARGET: Vec3 = Vec3::new(0.0, 60.0, 0.0);
const FIELD_OF_VIEW: f32 = 50.0_f32.to_radians();
const LOOK_SENSITIVITY: f32 = 0.0022;
const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;
const MOVE_SPEED: f32 = 220.0;
const ZOOM_PER_LINE: f32 = 60.0;
const ZOOM_PER_PIXEL: f32 = 1.0;
/// Panning scales with the height it reads as the distance to the ground, so at
/// sea level the drag would stall.
const PAN_MIN_HEIGHT: f32 = 15.0;
/// Only reached if the primary window has gone; the spawn resolution.
const FALLBACK_VIEWPORT_HEIGHT: f32 = 720.0;
const FAR_CLIP: f32 = ISLAND_WORLD_METRES * 6.0;
/// The open sea has to be flat sky colour by the time it reaches the horizon,
/// so the fog closes a kilometre inside the far clip.
const FOG_START: f32 = ISLAND_WORLD_METRES * 2.75;
const FOG_END: f32 = FAR_CLIP - ISLAND_WORLD_METRES * 0.5;

#[derive(Component)]
pub struct FlyCamera {
    yaw: f32,
    pitch: f32,
    looking: bool,
}

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (grab_cursor, look, pan, fly, zoom, reset));
    }
}

/// The pose the camera opens on and returns to; `spawn_camera` and `reset` both
/// have to read it from here.
fn home_pose() -> Transform {
    Transform::from_translation(HOME_EYE).looking_at(HOME_TARGET, Vec3::Y)
}

/// The angles `look` steers with, recovered from a rotation it did not build.
fn heading(rotation: Quat) -> (f32, f32) {
    let (yaw, pitch, _) = rotation.to_euler(EulerRot::YXZ);
    (yaw, pitch)
}

fn spawn_camera(mut commands: Commands) {
    let transform = home_pose();
    let (yaw, pitch) = heading(transform.rotation);
    commands.spawn((
        Name::new("Fly camera"),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: FIELD_OF_VIEW,
            near: 0.5,
            far: FAR_CLIP,
            ..default()
        }),
        DistanceFog {
            color: SKY_COLOUR,
            falloff: FogFalloff::Linear {
                start: FOG_START,
                end: FOG_END,
            },
            ..default()
        },
        transform,
        FlyCamera {
            yaw,
            pitch,
            looking: false,
        },
    ));
}

fn grab_cursor(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut cameras: Query<&mut FlyCamera>,
) {
    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };
    let release = keys.just_pressed(KeyCode::Escape) || mouse.just_released(MouseButton::Right);
    let looking = if mouse.just_pressed(MouseButton::Right) {
        true
    } else if release {
        false
    } else {
        camera.looking
    };
    if looking == camera.looking {
        return;
    }
    camera.looking = looking;
    if let Ok(mut cursor) = cursors.single_mut() {
        cursor.grab_mode = if looking {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        };
        cursor.visible = !looking;
    }
}

fn look(motion: Res<AccumulatedMouseMotion>, mut cameras: Query<(&mut FlyCamera, &mut Transform)>) {
    for (mut camera, mut transform) in &mut cameras {
        if !camera.looking || motion.delta == Vec2::ZERO {
            continue;
        }
        camera.yaw -= motion.delta.x * LOOK_SENSITIVITY;
        camera.pitch =
            (camera.pitch - motion.delta.y * LOOK_SENSITIVITY).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, camera.yaw, camera.pitch, 0.0);
    }
}

/// Dragging pulls the ground along with the cursor. The right button owns the
/// mouse whenever it is down, so a drag only pans outside look mode, and the
/// motion stays in the horizontal plane whatever the camera is pitched at.
fn pan(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<(&FlyCamera, &mut Transform)>,
) {
    if !mouse.pressed(MouseButton::Left) || motion.delta == Vec2::ZERO {
        return;
    }
    let viewport_height = windows
        .single()
        .map_or(FALLBACK_VIEWPORT_HEIGHT, Window::height);
    for (camera, mut transform) in &mut cameras {
        if camera.looking {
            continue;
        }
        // Roll is always zero, so the camera's own right vector is already
        // horizontal; forward has to be flattened, and collapses looking
        // straight down, where up carries the heading instead.
        let right = *transform.right();
        let forward = ground_axis(*transform.forward())
            .or_else(|| ground_axis(*transform.up()))
            .unwrap_or(Vec3::NEG_Z);
        // Height above the sea stands in for the distance to the ground, which
        // holds the terrain under the cursor closely enough without a ray cast.
        let height = transform.translation.y.max(PAN_MIN_HEIGHT);
        let metres_per_pixel = 2.0 * height * (FIELD_OF_VIEW * 0.5).tan() / viewport_height;
        transform.translation +=
            (forward * motion.delta.y - right * motion.delta.x) * metres_per_pixel;
    }
}

/// The horizontal heading of a camera axis, or `None` where it points straight
/// up or down and carries no heading at all.
fn ground_axis(direction: Vec3) -> Option<Vec3> {
    Vec3::new(direction.x, 0.0, direction.z).try_normalize()
}

fn fly(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut cameras: Query<&mut Transform, With<FlyCamera>>,
) {
    for mut transform in &mut cameras {
        let forward = *transform.forward();
        let right = *transform.right();
        let mut direction = Vec3::ZERO;
        if keys.pressed(KeyCode::KeyW) {
            direction += forward;
        }
        if keys.pressed(KeyCode::KeyS) {
            direction -= forward;
        }
        if keys.pressed(KeyCode::KeyD) {
            direction += right;
        }
        if keys.pressed(KeyCode::KeyA) {
            direction -= right;
        }
        if keys.pressed(KeyCode::Space) {
            direction += Vec3::Y;
        }
        if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            direction -= Vec3::Y;
        }
        let Some(direction) = direction.try_normalize() else {
            continue;
        };
        transform.translation += direction * MOVE_SPEED * time.delta_secs();
    }
}

/// Scrolling up dollies along the view direction. Line and pixel deltas arrive
/// on wildly different scales, so each carries its own step.
fn zoom(scroll: Res<AccumulatedMouseScroll>, mut cameras: Query<&mut Transform, With<FlyCamera>>) {
    if scroll.delta.y == 0.0 {
        return;
    }
    let step = match scroll.unit {
        MouseScrollUnit::Line => ZOOM_PER_LINE,
        MouseScrollUnit::Pixel => ZOOM_PER_PIXEL,
    };
    let distance = scroll.delta.y * step;
    for mut transform in &mut cameras {
        let forward = *transform.forward();
        transform.translation += forward * distance;
    }
}

/// The stored angles have to come back with the pose, or the next look resumes
/// from wherever the camera had been left. Whether the cursor is grabbed is a
/// separate question the reset does not answer.
fn reset(keys: Res<ButtonInput<KeyCode>>, mut cameras: Query<(&mut FlyCamera, &mut Transform)>) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    for (mut camera, mut transform) in &mut cameras {
        *transform = home_pose();
        let (yaw, pitch) = heading(transform.rotation);
        camera.yaw = yaw;
        camera.pitch = pitch;
    }
}
