//! Free-flying observer camera.
//!
//! WASD moves on the view plane, Space and Shift move along world up and down,
//! holding the right mouse button looks around with the cursor grabbed, the
//! scroll wheel scales the travel speed, and Escape releases the cursor.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use motu::ISLAND_WORLD_METRES;

const LOOK_SENSITIVITY: f32 = 0.0022;
const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;
const MINIMUM_SPEED: f32 = 5.0;
const MAXIMUM_SPEED: f32 = 4_000.0;
const SPEED_STEP: f32 = 1.18;
const BOOST: f32 = 4.0;

#[derive(Component)]
pub struct FlyCamera {
    speed: f32,
    yaw: f32,
    pitch: f32,
    looking: bool,
}

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (grab_cursor, look, fly, adjust_speed));
    }
}

fn spawn_camera(mut commands: Commands) {
    let eye = Vec3::new(1_150.0, 700.0, 1_150.0);
    let transform = Transform::from_translation(eye).looking_at(Vec3::new(0.0, 60.0, 0.0), Vec3::Y);
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    commands.spawn((
        Name::new("Fly camera"),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 50.0_f32.to_radians(),
            near: 0.5,
            far: ISLAND_WORLD_METRES * 6.0,
            ..default()
        }),
        transform,
        FlyCamera {
            speed: 220.0,
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

fn fly(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut cameras: Query<(&FlyCamera, &mut Transform)>,
) {
    for (camera, mut transform) in &mut cameras {
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
        let boost = if keys.pressed(KeyCode::ControlLeft) {
            BOOST
        } else {
            1.0
        };
        transform.translation += direction * camera.speed * boost * time.delta_secs();
    }
}

fn adjust_speed(scroll: Res<AccumulatedMouseScroll>, mut cameras: Query<&mut FlyCamera>) {
    if scroll.delta.y == 0.0 {
        return;
    }
    for mut camera in &mut cameras {
        camera.speed =
            (camera.speed * SPEED_STEP.powf(scroll.delta.y)).clamp(MINIMUM_SPEED, MAXIMUM_SPEED);
    }
}
