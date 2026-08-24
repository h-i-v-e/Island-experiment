//! Free-flying observer camera, the named poses it opens on, and the view
//! stack the scene is rendered through.
//!
//! WASD moves on the view plane, Space and Shift move along world up and down,
//! holding the left mouse button drags the ground along under the cursor,
//! holding the right mouse button looks around with the cursor grabbed, the
//! scroll wheel zooms along the view direction, R returns to the pose `--view`
//! selected, and Escape releases the cursor.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    camera::{Exposure, Hdr},
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    light::AtmosphereEnvironmentMapLight,
    pbr::{AtmosphereSettings, ContactShadows, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use motu::ISLAND_WORLD_METRES;

/// The named capture poses. The first entry is the default. Everything below
/// `overview` was framed on seed 666 at terrain size 1024, which is where the
/// generated relief these poses point at lives.
///
/// At 1024 most of the generated channels run at or below the waterline, and
/// only the dozen that still fall towards it carry a water surface to frame.
/// The `default` river poses therefore sit on the south-west catchment, where
/// the island's largest channel drops from 12 m through two falls into the bay
/// at (-400, 400) and a second channel reaches the same bay 140 m west of it.
///
/// The `eroded` variant keeps three reaches above the sea instead of twelve, so
/// every view that frames running water carries a pose of its own for it. Those
/// poses all sit on the south-east reach around (810, 530), the only eroded one
/// with settled stones and a fall. It carries about 40 m of running water
/// against the `default` channel's 100, so the wider views close in on it.
const VIEWS: [View; 6] = [
    // The whole island, far enough out at 700 m to hold it inside the frame.
    View {
        name: "overview",
        pose: ViewPose {
            eye: Vec3::new(1_150.0, 700.0, 1_150.0),
            target: Vec3::new(0.0, 60.0, 0.0),
        },
        variants: &[],
    },
    // The main massif, whose summit stands at roughly (-50, 355, -60).
    View {
        name: "mountain",
        pose: ViewPose {
            eye: Vec3::new(520.0, 400.0, 560.0),
            target: Vec3::new(-40.0, 200.0, -70.0),
        },
        variants: &[],
    },
    // The south-west catchment from over its bay: the gully the main channel
    // falls down, the inlet the second one reaches, and the ground between them.
    View {
        name: "river-region",
        pose: ViewPose {
            eye: Vec3::new(-625.0, 145.0, 512.0),
            target: Vec3::new(-435.0, 12.0, 372.0),
        },
        // Eroded: the south-east reach from off its cove, with the channel, the
        // fall at its foot and the cove it drains into all inside one frame.
        variants: &[(
            "eroded",
            ViewPose {
                eye: Vec3::new(905.0, 85.0, 618.0),
                target: Vec3::new(808.0, 8.0, 530.0),
            },
        )],
    },
    // Gameplay distance up the same catchment's lower channel, from over the bay.
    View {
        name: "river-ground",
        pose: ViewPose {
            eye: Vec3::new(-445.0, 33.0, 443.0),
            target: Vec3::new(-387.0, 5.5, 386.0),
        },
        // Eroded: up the cove to the channel, which is the one approach whose
        // foreground is water rather than the bare apron.
        variants: &[(
            "eroded",
            ViewPose {
                eye: Vec3::new(855.0, 33.0, 585.0),
                target: Vec3::new(809.0, 6.0, 535.0),
            },
        )],
    },
    // Near-ground from the bay onto the mouth the channel falls out of, close
    // enough that water, bank, rock and sand materials each have to hold up.
    View {
        name: "river-level4",
        pose: ViewPose {
            eye: Vec3::new(-408.0, 9.0, 412.0),
            target: Vec3::new(-397.0, 3.0, 398.0),
        },
        // Eroded: the drop into the cove, where gravel apron, bedrock wall,
        // grass edge, fresh water and sea all meet within one frame.
        variants: &[(
            "eroded",
            ViewPose {
                eye: Vec3::new(836.0, 11.0, 563.0),
                target: Vec3::new(816.0, 4.5, 548.0),
            },
        )],
    },
    // Standing height in the shallow stony reach of the catchment's second
    // channel, looking up it to the fall that feeds it. The settled stones are
    // 6-22 cm, so nothing further out than about ten metres resolves them.
    View {
        name: "stream",
        pose: ViewPose {
            eye: Vec3::new(-531.5, 6.3, 339.0),
            target: Vec3::new(-524.0, 5.8, 324.0),
        },
        // Eroded: the stone-strewn reach above the fall, looking down it to
        // the cove.
        variants: &[(
            "eroded",
            ViewPose {
                eye: Vec3::new(803.5, 8.1, 524.0),
                target: Vec3::new(811.0, 3.8, 537.0),
            },
        )],
    },
];

/// The view `--view` opens on when it is not given.
pub const DEFAULT_VIEW: &str = VIEWS[0].name;

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
/// The sea runs past the island to the horizon, and only the atmosphere fades
/// it out. The far plane therefore stands far enough away that what the frustum
/// cuts off is already the colour of the sky, and inside the sea plane's own
/// 100 km half extent so that edge never enters the frame.
const FAR_CLIP: f32 = ISLAND_WORLD_METRES * 40.0;
/// Direct sunlight is the metering point, opened up a stop and a half: that
/// value places an eighteen per cent grey card at mid tone, and forest, wet
/// rock and dark sand all sit well below one.
const EXPOSURE: Exposure = Exposure {
    ev100: Exposure::EV100_SUNLIGHT - 1.5,
};
/// Contact shadows only have to seat rocks and trunks on the ground they stand
/// on; a longer ray buys nothing at these scales and costs screen-space steps.
const CONTACT_SHADOW_LENGTH: f32 = 2.0;
const CONTACT_SHADOW_THICKNESS: f32 = 0.4;
/// Bloom carries sun and water glitter only. The island must not glow, so the
/// intensity stays well under the natural preset's own 0.15.
const BLOOM_INTENSITY: f32 = 0.05;

/// One entry in the preset table: the pose the view frames by default, and the
/// poses that replace it whole under the generation variants that move its
/// subject somewhere else.
struct View {
    name: &'static str,
    pose: ViewPose,
    variants: &'static [(&'static str, ViewPose)],
}

/// One named capture pose. Selected by `--view` and `--variant` together, it is
/// both the pose the camera opens on and the one `R` returns to.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ViewPose {
    eye: Vec3,
    target: Vec3,
}

impl Default for ViewPose {
    fn default() -> Self {
        VIEWS[0].pose
    }
}

impl ViewPose {
    /// Looks a `--view` name up in the preset table and resolves it against a
    /// `--variant` name, falling back to the view's shared pose. An unknown
    /// variant resolves to that shared pose; `island_gen` is what rejects it.
    pub fn named(view: &str, variant: &str) -> Result<Self, String> {
        let entry = VIEWS
            .iter()
            .find(|entry| entry.name == view)
            .ok_or_else(|| format!("unknown view {view:?}; expected one of {}", Self::names()))?;
        Ok(entry
            .variants
            .iter()
            .find(|(name, _)| *name == variant)
            .map_or(entry.pose, |&(_, pose)| pose))
    }

    /// The preset names in table order, for help text and parse errors.
    #[must_use]
    pub fn names() -> String {
        VIEWS
            .iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn transform(self) -> Transform {
        Transform::from_translation(self.eye).looking_at(self.target, Vec3::Y)
    }
}

#[derive(Component)]
pub struct FlyCamera {
    yaw: f32,
    pitch: f32,
    looking: bool,
}

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewPose>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, (grab_cursor, look, pan, fly, zoom, reset));
    }
}

/// The angles `look` steers with, recovered from a rotation it did not build.
fn heading(rotation: Quat) -> (f32, f32) {
    let (yaw, pitch, _) = rotation.to_euler(EulerRot::YXZ);
    (yaw, pitch)
}

fn spawn_camera(mut commands: Commands, pose: Res<ViewPose>) {
    let transform = pose.transform();
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
        // Sunlight arrives in the hundred-thousand-lux range, so the whole
        // chain has to stay in high dynamic range until the tone mapper.
        Hdr,
        EXPOSURE,
        Tonemapping::AcesFitted,
        // Draws the sky and lays aerial perspective over everything the opaque
        // pass has already drawn; the entity carrying the planet is in
        // `lighting`.
        AtmosphereSettings::default(),
        // The same atmosphere as an environment map, which is what fills
        // crevices now that there is no uniform ambient term.
        AtmosphereEnvironmentMapLight::default(),
        // Temporal anti-aliasing needs multisampling off, and repays it by
        // resolving the stochastic occlusion and contact shadow passes.
        Msaa::Off,
        TemporalAntiAliasing::default(),
        ScreenSpaceAmbientOcclusion::default(),
        ContactShadows {
            length: CONTACT_SHADOW_LENGTH,
            thickness: CONTACT_SHADOW_THICKNESS,
            ..default()
        },
        Bloom {
            intensity: BLOOM_INTENSITY,
            ..Bloom::NATURAL
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
fn reset(
    keys: Res<ButtonInput<KeyCode>>,
    pose: Res<ViewPose>,
    mut cameras: Query<(&mut FlyCamera, &mut Transform)>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    for (mut camera, mut transform) in &mut cameras {
        *transform = pose.transform();
        let (yaw, pitch) = heading(transform.rotation);
        camera.yaw = yaw;
        camera.pitch = pitch;
    }
}
