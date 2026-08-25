//! Observer camera in its two movement modes, the named poses it opens on, and
//! the view stack the scene is rendered through. F switches between the modes
//! and R returns to the pose `--view` selected, and to flying.
//!
//! The two modes take the mouse on opposite terms, because they are used for
//! opposite things. Flying is an editor view of the island: the cursor stays
//! free and the scene is steered with the buttons — the right one turns the
//! island on a turntable around whatever ground it was pointing at, the left
//! one drags the ground along under the cursor, the wheel dollies along the
//! view direction — with WASD on the view plane and Space and Shift moving
//! along world up and down.
//!
//! Walking is a person on the ground, so it follows the conventions a person
//! already has for that: entering it captures and hides the cursor and the
//! mouse looks from then on with no button held, WASD moves along the ground
//! relative to where the view points, Shift sprints, Space jumps and the wheel
//! does nothing. Escape hands the cursor back and a click in the scene takes it
//! again; so does opening the parameter panel, which cannot be clicked through
//! a captured cursor.

use std::f32::consts::{FRAC_PI_2, PI};

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

use crate::{island_gen::GeneratedIsland, screenshot::CaptureTarget};

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
const VIEWS: [View; 7] = [
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
    // A diagnostic pose rather than a subject: far enough out that the terrain
    // grid's LOD 0 to LOD 1 handover falls across the middle of the island
    // instead of past its far corner. From here the near chunk centres stand
    // 1.5 km off and the far ones 3.9 km, so the frontier — and any crack it
    // could open — runs through the frame. The six poses above are unchanged
    // and remain the baseline every capture is read against.
    View {
        name: "chunk-seam",
        pose: ViewPose {
            eye: Vec3::new(1_900.0, 400.0, 1_900.0),
            target: Vec3::new(0.0, 60.0, 0.0),
        },
        variants: &[],
    },
];

/// The view `--view` opens on when it is not given.
pub const DEFAULT_VIEW: &str = VIEWS[0].name;

const FIELD_OF_VIEW: f32 = 50.0_f32.to_radians();
const LOOK_SENSITIVITY: f32 = 0.0022;
const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;
/// Flying speed at full tilt, reached through [`FLY_RAMP_SECONDS`] rather than
/// on the first frame the key is down.
///
/// The island is two kilometres across and the default pose stands 1.6 km off
/// it, so the speed has to cross that in a few seconds. 220 m/s did, and at
/// sixty frames a second it also moved 3.7 m on a single frame's press, which
/// is what made the keys feel like they lurched. 140 m/s still crosses the
/// island in fourteen seconds and covers 2.3 m on that frame; the ramp takes
/// most of the rest.
const MOVE_SPEED: f32 = 140.0;
/// How long the fly keys take to reach [`MOVE_SPEED`] from rest, and to fall
/// back to rest from it. The velocity is carried between frames and moved
/// towards what the keys ask for at a fixed `MOVE_SPEED / FLY_RAMP_SECONDS`,
/// which is 560 m/s².
///
/// A quarter of a second is short enough that a held key is at full speed
/// before the eye has followed it and long enough that a tap is a nudge: one
/// frame's press reaches 9 m/s and coasts to a stop inside 0.16 m, against the
/// 3.7 m the same press used to cover. A linear ramp rather than an exponential
/// one because it actually arrives — at both ends, so a release stops the camera
/// instead of leaving it drifting.
const FLY_RAMP_SECONDS: f32 = 0.25;
/// The least the flying camera may stand over the ground under it, read off the
/// same height grid walking stands on. The sea plane is not ground: over open
/// water, where that grid runs below zero, the floor is this far over sea level
/// instead of this far over the sea bed.
const FLY_CLEARANCE: f32 = 2.0;
const ZOOM_PER_LINE: f32 = 60.0;
const ZOOM_PER_PIXEL: f32 = 1.0;
/// Panning scales with the height it reads as the distance to the ground, so at
/// sea level the drag would stall.
const PAN_MIN_HEIGHT: f32 = 15.0;
/// Only reached if the primary window has gone; the spawn resolution.
const FALLBACK_VIEWPORT_HEIGHT: f32 = 720.0;
/// The same, for the width the orbit drag is measured against.
const FALLBACK_VIEWPORT_WIDTH: f32 = 1_280.0;
/// How much orbit a drag across the whole window is worth. Half a window width
/// turns the island a quarter turn, so the radians a pixel carries are this
/// over the window width — a full sweep of the window is 180°, which is far
/// enough to walk round a subject without letting go and short enough that a
/// small correction stays small.
const ORBIT_PER_WINDOW: f32 = PI;
/// How far along the view direction the pivot search looks before giving up,
/// as a multiple of the island square. Two of them reaches past the far corner
/// from any pose in the table, including `chunk-seam` at 2.7 km out.
const ORBIT_PIVOT_RANGE: f32 = ISLAND_WORLD_METRES * 2.0;
/// The coarse march step the pivot search takes along that ray. The height grid
/// is bilinear over a 3.9 m lattice, so a step near that size cannot walk over
/// a feature the grid itself can hold.
const ORBIT_PIVOT_STEP: f32 = 4.0;
/// Bisections run on the step that crossed the surface. Six of them take a 4 m
/// straddle under 7 cm, which is finer than the grid the height came from.
const ORBIT_PIVOT_BISECTIONS: u32 = 6;
/// Where the pivot goes when the ray never meets the surface — pointed at the
/// sky, or grazing so flat that it runs out of range first. A point this far
/// ahead, dropped to the surface under it, which keeps the turntable in front
/// of the camera and on the ground rather than at infinity.
const ORBIT_FALLBACK_DISTANCE: f32 = 400.0;
/// The band over the pivot's horizontal plane the orbit may carry the eye
/// through, short of the vertical at one end and of the plane itself at the
/// other: over the top the view would flip, and under the plane it would look
/// up through the ground it is turning around.
///
/// A drag that starts outside this band — from a beach looking up at a summit,
/// where the pivot stands over the camera — is not snapped into it, which would
/// throw the eye hundreds of metres on the press. It may only move towards the
/// band, and once inside stays there for the rest of the drag.
const ORBIT_MIN_ELEVATION: f32 = 5.0_f32.to_radians();
const ORBIT_MAX_ELEVATION: f32 = 85.0_f32.to_radians();
/// Under this the eye and its pivot are the same point and the orbit has no
/// angles to turn.
const ORBIT_MIN_DISTANCE: f32 = 0.01;
/// The sea runs past the island to the horizon, and only the atmosphere fades
/// it out. The far plane therefore stands far enough away that what the frustum
/// cuts off is already the colour of the sky, and inside the sea plane's own
/// 100 km half extent so that edge never enters the frame.
const FAR_CLIP: f32 = ISLAND_WORLD_METRES * 40.0;
/// Direct sunlight is the metering point, opened up a stop and a half: that
/// value places an eighteen per cent grey card at mid tone, and forest, wet
/// rock and dark sand all sit well below one.
pub const EXPOSURE: Exposure = Exposure {
    ev100: Exposure::EV100_SUNLIGHT - 1.5,
};
/// Contact shadows only have to seat rocks and trunks on the ground they stand
/// on; a longer ray buys nothing at these scales and costs screen-space steps.
const CONTACT_SHADOW_LENGTH: f32 = 2.0;
const CONTACT_SHADOW_THICKNESS: f32 = 0.4;
/// Bloom carries sun and water glitter only. The island must not glow, so the
/// intensity stays well under the natural preset's own 0.15.
const BLOOM_INTENSITY: f32 = 0.05;

/// The image stack [`spawn_camera`] attaches, as a capture's own metadata names
/// it. Written out beside the camera rather than beside the capture, so a
/// component added or dropped there is one line from the list that reports it.
pub const RENDER_FEATURES: [&str; 7] = [
    "hdr",
    "aces-fitted",
    "atmosphere",
    "taa",
    "ssao",
    "contact-shadows",
    "bloom",
];

/// Switches between flying and walking.
pub const WALK_KEY: KeyCode = KeyCode::KeyF;
/// Eye height over the ground on foot.
const EYE_HEIGHT: f32 = 1.8;
/// On foot. Faster than a person walks, because the island is two kilometres
/// across and a real 1.5 m/s crossed it in twenty minutes.
const WALK_SPEED: f32 = 4.5;
/// Shift on foot, at twice the walk.
const SPRINT_SPEED: f32 = 9.0;
/// How high a jump carries the eye, and the gravity it falls back under. The
/// launch speed follows from the two, so the apex is the number to change.
const JUMP_APEX: f32 = 1.0;
const GRAVITY: f32 = 9.81;
/// How deep the water may be underfoot. A step that would cross into anything
/// deeper is refused, and ground already below this is stood on at this depth
/// rather than sunk into, which puts the eye 0.8 m over the sea: chest deep.
/// The generated shelf runs one to three metres down across the whole square,
/// so the sea itself is what stops a walk at the shoreline.
const WADE_DEPTH: f32 = 1.0;

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
    pub eye: Vec3,
    pub target: Vec3,
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
    /// The cursor is captured and the mouse is steering the camera. Flying,
    /// this lasts as long as the right button is held and the mouse orbits;
    /// walking, it is the ordinary state, the mouse looks, and
    /// [`WalkState::released`] is what interrupts it.
    looking: bool,
    /// The ground point the flying orbit is turning around, picked on the frame
    /// the right button went down and held until it comes back up. `None`
    /// whenever no orbit is in progress, which is what the release resync is
    /// keyed on.
    orbit: Option<Vec3>,
    /// What flying is carrying this frame, in world space. Held between frames
    /// so [`FLY_RAMP_SECONDS`] has something to ramp, and cleared whenever the
    /// camera is put somewhere rather than flown there.
    velocity: Vec3,
    walk: WalkState,
}

/// What walking carries between frames. Reset whole every time walking is
/// entered, so a mode left mid-jump or mid-Escape does not resume that way.
#[derive(Default)]
struct WalkState {
    /// Escape handed the cursor back, and a click in the scene takes it again.
    released: bool,
    /// A jump is in the air, rising at `rise` metres per second.
    airborne: bool,
    rise: f32,
}

/// Which of the two movement modes the camera is in.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraMode {
    #[default]
    Fly,
    Walk,
}

/// What an on-screen panel is claiming this frame. Always present and left
/// clear when nothing draws one, which is what keeps the camera off the HUD's
/// drags and typing without the camera knowing what draws it.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct UiFocus {
    /// The pointer is over the panel, or the panel is dragging with it.
    pub pointer: bool,
    /// A panel field is taking typed input.
    pub keyboard: bool,
    /// A panel is on screen. Walking gives the cursor back for as long as one
    /// is, since a captured cursor can never arrive at it.
    pub shown: bool,
}

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewPose>()
            .init_resource::<CameraMode>()
            .init_resource::<UiFocus>()
            .add_systems(Startup, spawn_camera)
            // Chained: the mode is settled before the cursor is decided for it,
            // the cursor before the look and the orbit it drives, and the two
            // ground clamps last of all, because walking, jumping, flying,
            // orbiting, panning, dollying and the mode switch all move the eye.
            // One clamp runs per mode, so the pair is a single tail on the
            // chain and neither can undo the other.
            //
            // `look` and `orbit` take the same mouse motion and each runs in
            // one mode only, so they can never both answer a drag.
            .add_systems(
                Update,
                (
                    switch_mode,
                    reset,
                    grab_cursor,
                    look,
                    orbit,
                    pan,
                    fly,
                    walk,
                    jump,
                    zoom,
                    stand_on_ground,
                    clear_the_ground,
                )
                    .chain(),
            );
    }
}

/// The angles `look` steers with, recovered from a rotation it did not build.
fn heading(rotation: Quat) -> (f32, f32) {
    let (yaw, pitch, _) = rotation.to_euler(EulerRot::YXZ);
    (yaw, pitch)
}

fn spawn_camera(mut commands: Commands, pose: Res<ViewPose>, capture: Option<Res<CaptureTarget>>) {
    let transform = pose.transform();
    let (yaw, pitch) = heading(transform.rotation);
    let mut camera = commands.spawn((
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
            orbit: None,
            velocity: Vec3::ZERO,
            walk: WalkState::default(),
        },
    ));
    // A capture run has no window to render into, so the whole stack above
    // draws into an offscreen image instead. Every other run leaves the
    // component off and targets the primary window, which is the default.
    if let Some(target) = capture {
        camera.insert(target.render_target());
    }
}

/// The one place the cursor is captured and handed back, on whichever of the
/// two schemes the current mode uses. A mode switch decides it afresh: a grab
/// that belonged to walking must not carry into flying, where the right button
/// owns it, and the reverse.
fn grab_cursor(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<CameraMode>,
    ui: Res<UiFocus>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut cameras: Query<&mut FlyCamera>,
) {
    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };
    let switched = mode.is_changed();
    let looking = match *mode {
        // A press that lands on the panel belongs to it; a release always
        // reaches the camera, or a drag that wandered over the panel would
        // leave the cursor grabbed with nothing steering it.
        CameraMode::Fly => {
            if switched {
                false
            } else if mouse.just_pressed(MouseButton::Right) && !ui.pointer {
                true
            } else if keys.just_pressed(KeyCode::Escape)
                || mouse.just_released(MouseButton::Right)
            {
                false
            } else {
                camera.looking
            }
        }
        // On foot the look is the view itself rather than something asked for,
        // so the cursor is captured for as long as walking lasts. Escape hands
        // it back and a click in the scene takes it again; the panel borrows it
        // for as long as it is up, without spending that click.
        CameraMode::Walk => {
            if keys.just_pressed(KeyCode::Escape) {
                camera.walk.released = true;
            } else if mouse.just_pressed(MouseButton::Left) && !ui.pointer && !ui.shown {
                camera.walk.released = false;
            }
            !camera.walk.released && !ui.shown
        }
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

/// Turns the view in place around the eye, which is what the mouse does on
/// foot. Flying steers the same motion into [`orbit`] instead, so this runs in
/// one mode only and the two never share a drag.
fn look(
    motion: Res<AccumulatedMouseMotion>,
    mode: Res<CameraMode>,
    mut cameras: Query<(&mut FlyCamera, &mut Transform)>,
) {
    if *mode == CameraMode::Fly {
        return;
    }
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

/// Flying, a right-button drag turns the island on a turntable instead of
/// turning the view: the ground the camera was pointing at when the button went
/// down stays where it is, and the eye swings around it.
///
/// The pivot is picked once, on the press, and held for the whole drag. One
/// re-read every frame would slide along the surface as the view moved and the
/// turn would wander off whatever it was started on.
///
/// The eye keeps the distance it had and faces the pivot throughout, so the
/// drag is a rotation of the scene and nothing else. The wheel still dollies —
/// it moves the eye along its own view line, which leaves both angles alone and
/// only shortens the arm — because the angles are read back off the eye every
/// frame rather than carried.
///
/// The drag reads the way a hand on the landscape would: dragging right carries
/// it right, dragging down tips it down and lifts the eye over it. That is the
/// direction the in-place look turns as well, so the two modes' mice agree even
/// though only one of them moves the eye.
///
/// Closing an orbit resyncs the stored yaw and pitch off the transform it left,
/// since nothing else has kept them up to date while the eye was being swung,
/// and spends the flying velocity so no momentum survives the drag. It is keyed
/// on the held pivot rather than on the button, so Escape and a switch to
/// walking close one exactly the way a release does.
fn orbit(
    motion: Res<AccumulatedMouseMotion>,
    mode: Res<CameraMode>,
    island: Option<Res<GeneratedIsland>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<(&mut FlyCamera, &mut Transform)>,
) {
    let viewport_width = windows
        .single()
        .map_or(FALLBACK_VIEWPORT_WIDTH, Window::width);
    let radians_per_pixel = ORBIT_PER_WINDOW / viewport_width;
    for (mut camera, mut transform) in &mut cameras {
        if *mode == CameraMode::Walk || !camera.looking {
            if camera.orbit.take().is_some() {
                let (yaw, pitch) = heading(transform.rotation);
                camera.yaw = yaw;
                camera.pitch = pitch;
                camera.velocity = Vec3::ZERO;
            }
            continue;
        }
        let pivot = *camera.orbit.get_or_insert_with(|| {
            pivot_ahead(transform.translation, *transform.forward(), |x, z| {
                surface_height(island.as_deref(), x, z)
            })
        });
        if motion.delta == Vec2::ZERO {
            continue;
        }
        let eye = orbited(
            pivot,
            transform.translation,
            -motion.delta.x * radians_per_pixel,
            motion.delta.y * radians_per_pixel,
        );
        *transform = Transform::from_translation(eye).looking_at(pivot, Vec3::Y);
    }
}

/// The surface the flying camera sees under a place: the generated ground, or
/// the sea plane wherever that ground runs below it. It is the floor
/// [`clear_the_ground`] holds the camera over, so an orbit turns around what is
/// drawn rather than around a sea bed hidden under water. With no island
/// generated yet there is only the sea.
fn surface_height(island: Option<&GeneratedIsland>, x: f32, z: f32) -> f32 {
    island.map_or(0.0, |island| island.0.ground_height(x, z).max(0.0))
}

/// The point a view direction is aimed at: where the ray from `eye` first meets
/// the surface, or — pointed over the horizon, or grazing so flat that it runs
/// out of range first — a point [`ORBIT_FALLBACK_DISTANCE`] ahead dropped onto
/// the surface under it, which keeps the turntable in front of the camera and
/// on the ground instead of at infinity.
///
/// A coarse march with a bisection on the step that crossed, rather than
/// anything cleverer: the surface is a bilinear height field with no overhangs,
/// so the first crossing along the ray is the one that is visible, and the
/// whole search runs once per drag.
fn pivot_ahead(eye: Vec3, forward: Vec3, surface: impl Fn(f32, f32) -> f32) -> Vec3 {
    let direction = forward.try_normalize().unwrap_or(Vec3::NEG_Z);
    let point_at = |distance: f32| eye + direction * distance;
    let clearance = |distance: f32| {
        let point = point_at(distance);
        point.y - surface(point.x, point.z)
    };
    let dropped = |distance: f32| {
        let point = point_at(distance);
        Vec3::new(point.x, surface(point.x, point.z), point.z)
    };
    // Only reachable with the eye already under the surface, which the flying
    // clamp does not allow; there is no first crossing to find from there.
    if clearance(0.0) <= 0.0 {
        return dropped(ORBIT_FALLBACK_DISTANCE);
    }
    let mut over = 0.0;
    let mut under = None;
    let mut distance = ORBIT_PIVOT_STEP;
    while distance <= ORBIT_PIVOT_RANGE {
        if clearance(distance) <= 0.0 {
            under = Some(distance);
            break;
        }
        over = distance;
        distance += ORBIT_PIVOT_STEP;
    }
    let Some(mut under) = under else {
        return dropped(ORBIT_FALLBACK_DISTANCE);
    };
    for _ in 0..ORBIT_PIVOT_BISECTIONS {
        let middle = f32::midpoint(over, under);
        if clearance(middle) <= 0.0 {
            under = middle;
        } else {
            over = middle;
        }
    }
    point_at(under)
}

/// Where a drag leaves the eye: its offset from the pivot turned by the two
/// angles, at the length it already had.
///
/// `azimuth` swings around the pivot's vertical axis and `elevation` above and
/// below its horizontal plane, both in radians. The elevation is held inside
/// [`ORBIT_MIN_ELEVATION`] to [`ORBIT_MAX_ELEVATION`], each end widened to
/// wherever the eye already stands: a drag begun from a beach looking up at a
/// summit starts below the band and one begun looking straight down starts
/// above it, and snapping either into the band would throw the eye hundreds of
/// metres on the press. From outside, the elevation may only move towards the
/// band, and once inside it stays there for the rest of the drag.
fn orbited(pivot: Vec3, eye: Vec3, azimuth: f32, elevation: f32) -> Vec3 {
    let offset = eye - pivot;
    let distance = offset.length();
    if distance < ORBIT_MIN_DISTANCE {
        return eye;
    }
    let turned = offset.x.atan2(offset.z) + azimuth;
    let raised = (offset.y / distance).clamp(-1.0, 1.0).asin();
    // Never the pole itself, whichever end the eye came in at: an eye directly
    // over its pivot has no heading for `looking_at` to build a rotation from.
    let lowest = ORBIT_MIN_ELEVATION.min(raised).max(-PITCH_LIMIT);
    let highest = ORBIT_MAX_ELEVATION.max(raised).min(PITCH_LIMIT);
    let raised = (raised + elevation).clamp(lowest, highest);
    let (up, along) = raised.sin_cos();
    let (across, ahead) = turned.sin_cos();
    pivot + distance * Vec3::new(along * across, up, along * ahead)
}

/// Dragging pulls the ground along with the cursor. The right button owns the
/// mouse whenever it is down, so a drag only pans outside look mode, and the
/// motion stays in the horizontal plane whatever the camera is pitched at.
fn pan(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mode: Res<CameraMode>,
    ui: Res<UiFocus>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<(&FlyCamera, &mut Transform)>,
) {
    // Panning slides the eye over the ground, which is the one thing walking is
    // for, so on foot the drag does nothing.
    if *mode == CameraMode::Walk || ui.pointer {
        return;
    }
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

/// The keys ask for a velocity rather than for a step, and the one being
/// carried moves towards it at a fixed rate. A key released asks for nothing,
/// so the same rate is what brings the camera back to rest; the ramp is
/// therefore in the velocity and not in the keys, and a direction changed
/// mid-flight turns the motion rather than restarting it.
fn fly(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mode: Res<CameraMode>,
    ui: Res<UiFocus>,
    mut cameras: Query<(&mut FlyCamera, &mut Transform)>,
) {
    if *mode == CameraMode::Walk {
        return;
    }
    for (mut camera, mut transform) in &mut cameras {
        let forward = *transform.forward();
        let right = *transform.right();
        let mut direction = Vec3::ZERO;
        // A panel field taking typed input asks for nothing rather than
        // freezing what the camera was already carrying, so a burst of typing
        // lands the camera instead of parking it mid-flight.
        if !ui.keyboard {
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
        }
        let wanted = direction.normalize_or_zero() * MOVE_SPEED;
        let change = wanted - camera.velocity;
        let step = MOVE_SPEED / FLY_RAMP_SECONDS * time.delta_secs();
        camera.velocity += change.normalize_or_zero() * step.min(change.length());
        transform.translation += camera.velocity * time.delta_secs();
    }
}

/// Scrolling up dollies along the view direction. Line and pixel deltas arrive
/// on wildly different scales, so each carries its own step. On foot the wheel
/// does nothing at all: a person does not dolly, and the ground clamp would
/// turn the motion into a stride nobody asked for.
fn zoom(
    scroll: Res<AccumulatedMouseScroll>,
    mode: Res<CameraMode>,
    ui: Res<UiFocus>,
    mut cameras: Query<(&FlyCamera, &mut Transform)>,
) {
    if scroll.delta.y == 0.0 || *mode == CameraMode::Walk {
        return;
    }
    let step = match scroll.unit {
        MouseScrollUnit::Line => ZOOM_PER_LINE,
        MouseScrollUnit::Pixel => ZOOM_PER_PIXEL,
    };
    let distance = scroll.delta.y * step;
    for (camera, mut transform) in &mut cameras {
        // Looking grabs the cursor where it stood, so the panel would go on
        // reporting the pointer as its own for as long as the look lasted. A
        // camera that already owns the mouse keeps the wheel with it.
        if ui.pointer && !camera.looking {
            continue;
        }
        let forward = *transform.forward();
        transform.translation += forward * distance;
    }
}

/// The stored angles have to come back with the pose, or the next look resumes
/// from wherever the camera had been left. A named pose is a flying pose, so
/// the reset also puts the camera back in the air, and `grab_cursor` hands a
/// walk's captured cursor back on the same frame. Reset while already flying
/// leaves the mode untouched, so it cannot interrupt a look in progress.
fn reset(
    keys: Res<ButtonInput<KeyCode>>,
    pose: Res<ViewPose>,
    ui: Res<UiFocus>,
    mut mode: ResMut<CameraMode>,
    mut cameras: Query<(&mut FlyCamera, &mut Transform)>,
) {
    if !keys.just_pressed(KeyCode::KeyR) || ui.keyboard {
        return;
    }
    mode.set_if_neq(CameraMode::Fly);
    for (mut camera, mut transform) in &mut cameras {
        *transform = pose.transform();
        let (yaw, pitch) = heading(transform.rotation);
        camera.yaw = yaw;
        camera.pitch = pitch;
        // A pose is a place, not a heading with speed behind it: whatever the
        // camera was carrying would otherwise fly it straight back off the pose.
        camera.velocity = Vec3::ZERO;
        // Reset under a held right button leaves the drag running, and the
        // ground it was turning around is not in front of the pose it landed
        // on. Dropping the pivot has the next frame of that drag pick one off
        // the new view instead.
        camera.orbit = None;
    }
}

fn switch_mode(
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<UiFocus>,
    mut mode: ResMut<CameraMode>,
    mut cameras: Query<&mut FlyCamera>,
) {
    if !keys.just_pressed(WALK_KEY) || ui.keyboard {
        return;
    }
    *mode = match *mode {
        CameraMode::Fly => CameraMode::Walk,
        CameraMode::Walk => CameraMode::Fly,
    };
    // Every walk starts on its feet with the cursor captured, whatever the last
    // one was left in the middle of, and a flight resumed after one starts from
    // rest rather than from the speed it was taken to foot at.
    for mut camera in &mut cameras {
        camera.walk = WalkState::default();
        camera.velocity = Vec3::ZERO;
    }
    info!("camera mode: {:?}", *mode);
}

/// Walking moves in the horizontal plane at a pace a person keeps, relative to
/// where the view points, with Shift sprinting rather than descending as it
/// does in the air. It runs whether or not a jump is in the air, so a jump can
/// be steered.
///
/// A step that would put the walker in water deeper than [`WADE_DEPTH`] is
/// refused, in the air as much as on the ground. The two axes are then tried on
/// their own, so a shoreline turns the walk along itself instead of stopping it
/// dead.
fn walk(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mode: Res<CameraMode>,
    ui: Res<UiFocus>,
    island: Option<Res<GeneratedIsland>>,
    mut cameras: Query<&mut Transform, With<FlyCamera>>,
) {
    if *mode == CameraMode::Fly || ui.keyboard {
        return;
    }
    let sprinting = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let speed = if sprinting { SPRINT_SPEED } else { WALK_SPEED };
    for mut transform in &mut cameras {
        let forward = ground_axis(*transform.forward())
            .or_else(|| ground_axis(*transform.up()))
            .unwrap_or(Vec3::NEG_Z);
        let right = ground_axis(*transform.right()).unwrap_or(Vec3::X);
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
        let Some(direction) = direction.try_normalize() else {
            continue;
        };
        let step = direction * speed * time.delta_secs();
        let from = transform.translation;
        let wadeable = |to: Vec3| {
            island
                .as_ref()
                .is_none_or(|island| island.0.ground_height(to.x, to.z) >= -WADE_DEPTH)
        };
        for candidate in [
            from + step,
            from + Vec3::new(step.x, 0.0, 0.0),
            from + Vec3::new(0.0, 0.0, step.z),
        ] {
            if wadeable(candidate) {
                transform.translation = candidate;
                break;
            }
        }
    }
}

/// Space leaves the ground. The launch speed is whatever reaches [`JUMP_APEX`]
/// under [`GRAVITY`], so the apex is the only number that has to be believed.
fn jump(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<CameraMode>,
    ui: Res<UiFocus>,
    mut cameras: Query<&mut FlyCamera>,
) {
    if *mode == CameraMode::Fly || ui.keyboard || !keys.just_pressed(KeyCode::Space) {
        return;
    }
    for mut camera in &mut cameras {
        if camera.walk.airborne {
            continue;
        }
        camera.walk.airborne = true;
        camera.walk.rise = (2.0 * GRAVITY * JUMP_APEX).sqrt();
    }
}

/// Settles the eye against the ground under it, whatever moved it this frame.
///
/// On its feet it sits exactly one person's height over the surface: bilinear
/// over a 3.9 m lattice, that surface has no vertical faces to fall down, so
/// following it is the whole of walking and only a jump ever leaves it. In the
/// air the rise integrates under gravity and the same height is the floor it
/// lands back on, which is also what stops a jump onto rising ground from
/// carrying the eye through it.
///
/// Ground below [`WADE_DEPTH`] is stood on at that depth rather than sunk into,
/// so a camera taken to foot over open sea, or carried off the terrain square,
/// stands at the waterline instead of falling away under it.
fn stand_on_ground(
    time: Res<Time>,
    mode: Res<CameraMode>,
    island: Option<Res<GeneratedIsland>>,
    mut cameras: Query<(&mut FlyCamera, &mut Transform)>,
) {
    if *mode == CameraMode::Fly {
        return;
    }
    let Some(island) = island else {
        return;
    };
    for (mut camera, mut transform) in &mut cameras {
        let support = island
            .0
            .ground_height(transform.translation.x, transform.translation.z)
            .max(-WADE_DEPTH);
        let mut feet = transform.translation.y - EYE_HEIGHT;
        if camera.walk.airborne {
            camera.walk.rise -= GRAVITY * time.delta_secs();
            feet += camera.walk.rise * time.delta_secs();
        }
        if !camera.walk.airborne || feet <= support {
            feet = support;
            camera.walk.airborne = false;
            camera.walk.rise = 0.0;
        }
        transform.translation.y = feet + EYE_HEIGHT;
    }
}

/// Keeps the flying camera over the ground, whatever moved it this frame.
///
/// Everything flying does moves the eye: the keys, the pan drag, the wheel and
/// `R`. This runs after all four and reads the one height grid walking already
/// stands on, so the two modes agree on where the ground is and neither can put
/// the eye under it. The floor is [`FLY_CLEARANCE`] over that ground, and never
/// under [`FLY_CLEARANCE`] over sea level: the sea plane is a surface the camera
/// has to clear as well, and the shelf below it is not ground to fly along.
///
/// Downward velocity is spent when the floor is reached rather than carried, or
/// a Shift held against the ground would build up a fall the camera would take
/// the moment it flew out over deeper water.
///
/// A capture run is left alone. Its pose is what `--view` names and what the
/// sidecar records, and a capture whose eye had been lifted off that pose would
/// no longer be the frame its own metadata describes.
fn clear_the_ground(
    mode: Res<CameraMode>,
    island: Option<Res<GeneratedIsland>>,
    capture: Option<Res<CaptureTarget>>,
    mut cameras: Query<(&mut FlyCamera, &mut Transform)>,
) {
    if *mode == CameraMode::Walk || capture.is_some() {
        return;
    }
    let Some(island) = island else {
        return;
    };
    for (mut camera, mut transform) in &mut cameras {
        let ground = island
            .0
            .ground_height(transform.translation.x, transform.translation.z)
            .max(0.0);
        let floor = ground + FLY_CLEARANCE;
        if transform.translation.y < floor {
            transform.translation.y = floor;
            camera.velocity.y = camera.velocity.y.max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use bevy::{
        app::App,
        input::{ButtonInput, mouse::AccumulatedMouseMotion},
        math::{Vec2, Vec3},
        prelude::{IntoScheduleConfigs, KeyCode, MouseButton, Transform, Update},
    };

    use super::{
        CameraMode, FALLBACK_VIEWPORT_WIDTH, FlyCamera, ORBIT_FALLBACK_DISTANCE,
        ORBIT_MAX_ELEVATION, ORBIT_MIN_ELEVATION, ORBIT_PER_WINDOW, ORBIT_PIVOT_RANGE, UiFocus,
        WalkState, grab_cursor, heading, orbit, orbited, pivot_ahead,
    };

    /// The flat sea, which is what a pivot falls back to wherever the generated
    /// ground runs below it.
    fn sea(_x: f32, _z: f32) -> f32 {
        0.0
    }

    /// The angle of an eye over its pivot's horizontal plane, which is the one
    /// the orbit clamps.
    fn elevation(pivot: Vec3, eye: Vec3) -> f32 {
        let offset = eye - pivot;
        (offset.y / offset.length()).asin()
    }

    /// An eye placed at a known angle and arm from a pivot, so a test can state
    /// the geometry it starts from rather than the coordinates.
    fn eye_at(pivot: Vec3, elevation: f32, distance: f32) -> Vec3 {
        pivot + distance * Vec3::new(0.0, elevation.sin(), elevation.cos())
    }

    /// The pivot is the first place the view ray meets the surface, near enough
    /// that the bisection has closed on it rather than on the coarse step.
    #[test]
    fn the_pivot_lands_where_the_view_meets_the_sea() {
        let eye = Vec3::new(0.0, 100.0, 0.0);
        let pivot = pivot_ahead(eye, Vec3::new(0.0, -1.0, -1.0), sea);
        assert!(pivot.y.abs() < 0.1, "{pivot} is not on the sea plane");
        assert!((pivot.z + 100.0).abs() < 0.1, "{pivot} is not 100 m ahead");
    }

    /// Ground standing over the sea is met before the sea behind it, so a view
    /// across a headland turns around the headland.
    #[test]
    fn the_pivot_stops_at_the_first_ground_it_crosses() {
        // A ramp out of the sea, climbing a metre for every two metres north.
        let ramp = |_x: f32, z: f32| (-z * 0.5).max(0.0);
        let eye = Vec3::new(0.0, 100.0, 0.0);
        let pivot = pivot_ahead(eye, Vec3::new(0.0, -1.0, -1.0), ramp);
        // The ray drops a metre per metre and the ramp climbs half of one, so
        // they meet where 100 - t = t / 2.
        assert!((pivot.z + 200.0 / 3.0).abs() < 0.1, "{pivot} missed the ramp");
        assert!((pivot.y - ramp(pivot.x, pivot.z)).abs() < 0.1, "{pivot} is off the ramp");
    }

    /// A ray that never comes down cannot be marched to anything, so the pivot
    /// goes a fixed distance ahead and drops to the surface under it.
    #[test]
    fn a_view_over_the_horizon_pivots_on_the_ground_ahead() {
        let eye = Vec3::new(0.0, 100.0, 0.0);
        let pivot = pivot_ahead(eye, Vec3::new(0.0, 0.5, -1.0), sea);
        assert!(pivot.y.abs() < f32::EPSILON, "{pivot} is not on the sea");
        let ahead = (pivot - eye).length();
        assert!(ahead > ORBIT_FALLBACK_DISTANCE * 0.5, "{ahead} m is not ahead");
        assert!(ahead < ORBIT_FALLBACK_DISTANCE * 1.5, "{ahead} m is too far");
    }

    /// A view flat enough that the surface is further off than the march looks
    /// takes the same fallback rather than running to the end of the ray.
    #[test]
    fn a_grazing_view_gives_up_inside_its_range() {
        let eye = Vec3::new(0.0, 100.0, 0.0);
        let pivot = pivot_ahead(eye, Vec3::new(0.0, -0.001, -1.0), sea);
        assert!((pivot - eye).length() < ORBIT_PIVOT_RANGE * 0.5, "{pivot} is out of range");
    }

    /// The arm is what the wheel changes, never the drag: an orbit turns the
    /// eye around the pivot and leaves the distance to it alone.
    #[test]
    fn the_orbit_keeps_the_distance_it_started_with() {
        let pivot = Vec3::new(10.0, 5.0, -20.0);
        let eye = eye_at(pivot, 0.6, 300.0);
        let turned = orbited(pivot, eye, 0.7, -0.2);
        assert!(((turned - pivot).length() - 300.0).abs() < 0.01);
    }

    /// Dragging right carries the landscape right, which puts the eye round to
    /// its own left. The system negates the horizontal drag to ask for that, so
    /// a negative angle here has to move the eye towards negative X from a
    /// pivot it is due south of.
    #[test]
    fn a_rightward_drag_swings_the_eye_left() {
        let pivot = Vec3::ZERO;
        let eye = eye_at(pivot, ORBIT_MIN_ELEVATION, 100.0);
        let turned = orbited(pivot, eye, -0.2, 0.0);
        assert!(turned.x < -1.0, "{turned} did not swing left");
        assert!(turned.z > 0.0, "{turned} swung past a fifth of a radian");
    }

    /// Neither end of the drag may reach the pole: over the top the view would
    /// flip, and under the floor it would look up through the ground.
    #[test]
    fn the_elevation_stays_inside_its_band() {
        let pivot = Vec3::ZERO;
        let high = orbited(pivot, eye_at(pivot, 1.4, 100.0), 0.0, 0.0);
        let low = orbited(pivot, eye_at(pivot, 0.2, 100.0), 0.0, -1.0);
        assert!(elevation(pivot, high) <= ORBIT_MAX_ELEVATION + 1e-4);
        assert!(elevation(pivot, low) >= ORBIT_MIN_ELEVATION - 1e-4);
    }

    /// An eye that starts outside the band — looking up at a summit from a
    /// beach, where the pivot stands over it — is not snapped into it, which
    /// would throw it hundreds of metres on the press. It may only move
    /// towards the band.
    #[test]
    fn an_eye_below_the_band_is_led_back_rather_than_snapped() {
        let pivot = Vec3::ZERO;
        let eye = eye_at(pivot, -0.5, 100.0);
        let towards = orbited(pivot, eye, 0.0, 0.2);
        assert!((elevation(pivot, towards) + 0.3).abs() < 1e-4);
        let away = orbited(pivot, eye, 0.0, -0.2);
        assert!((elevation(pivot, away) + 0.5).abs() < 1e-4);
    }

    /// An eye already on its pivot has no arm to turn and no angles to read off
    /// one, so the drag leaves it where it is.
    #[test]
    fn an_eye_on_its_pivot_does_not_move() {
        let pivot = Vec3::new(3.0, 4.0, 5.0);
        assert_eq!(orbited(pivot, pivot, 1.0, 1.0), pivot);
    }

    /// The documented feel of the drag: half a window width is a quarter turn.
    #[test]
    fn half_a_window_width_is_a_quarter_turn() {
        let turn = ORBIT_PER_WINDOW / FALLBACK_VIEWPORT_WIDTH * (FALLBACK_VIEWPORT_WIDTH * 0.5);
        assert!((turn - FRAC_PI_2).abs() < 1e-6, "{turn} radians is not a quarter turn");
    }

    /// What the camera is left carrying after a frame, which is what the next
    /// input reads.
    struct Carried {
        yaw: f32,
        pitch: f32,
        pivot: Option<Vec3>,
        velocity: Vec3,
    }

    /// A flying camera with the two systems a right-button drag runs and
    /// nothing else: no window, so the drag is measured against the fallback
    /// width, and no island, so the surface it pivots on is the sea plane.
    fn viewer(eye: Vec3, at: Vec3) -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<CameraMode>()
            .init_resource::<UiFocus>()
            .add_systems(Update, (grab_cursor, orbit).chain());
        let transform = Transform::from_translation(eye).looking_at(at, Vec3::Y);
        let (yaw, pitch) = heading(transform.rotation);
        app.world_mut().spawn((
            transform,
            FlyCamera {
                yaw,
                pitch,
                looking: false,
                orbit: None,
                velocity: Vec3::ZERO,
                walk: WalkState::default(),
            },
        ));
        // The mode resource reads as changed on the frame it is inserted, which
        // is how a mode switch tells `grab_cursor` to decide the cursor afresh.
        // One frame spends that, so the press below is an ordinary press.
        app.update();
        app
    }

    /// One frame of a drag: the button the window would report and the motion
    /// accumulated under it, then the systems that answer them.
    fn frame(app: &mut App, held: bool, motion: Vec2) {
        {
            let mut buttons = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            buttons.clear();
            if held {
                buttons.press(MouseButton::Right);
            } else {
                buttons.release(MouseButton::Right);
            }
        }
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = motion;
        app.update();
    }

    fn state(app: &mut App) -> (Transform, Carried) {
        let mut cameras = app.world_mut().query::<(&Transform, &FlyCamera)>();
        let (transform, camera) = cameras.single(app.world()).expect("one camera");
        (
            *transform,
            Carried {
                yaw: camera.yaw,
                pitch: camera.pitch,
                pivot: camera.orbit,
                velocity: camera.velocity,
            },
        )
    }

    /// The whole drag end to end, through the systems that run it: the press
    /// picks the ground the view was on, the motion swings the eye around that
    /// point at the distance it had, and the release hands the angles back.
    ///
    /// A held button and a moving mouse are not something a test can ask a real
    /// window for, so the frames are driven by hand; what they drive is the
    /// unchanged pair of systems.
    #[test]
    fn a_right_drag_turns_the_camera_around_the_ground_it_was_on() {
        // Down at 45° onto the sea plane, so the pivot is the origin and the
        // arm is the diagonal out to it.
        let eye = Vec3::new(0.0, 400.0, 400.0);
        let mut app = viewer(eye, Vec3::ZERO);

        frame(&mut app, true, Vec2::ZERO);
        let (_, pressed) = state(&mut app);
        let pivot = pressed.pivot.expect("the press picks a pivot");
        assert!(pivot.length() < 1.0, "{pivot} is not the ground looked at");

        // A quarter of the fallback width is 45° of turn; dragged right, which
        // carries the eye round to its own left.
        frame(&mut app, true, Vec2::new(FALLBACK_VIEWPORT_WIDTH * 0.25, 0.0));
        let (turned, held) = state(&mut app);
        assert_eq!(held.pivot, Some(pivot), "the pivot moved mid-drag");
        let arm = turned.translation - pivot;
        assert!(
            (arm.length() - (eye - pivot).length()).abs() < 1.0,
            "{arm} is not the arm the drag started with"
        );
        assert!(arm.x < -1.0, "{arm} did not swing left of the pivot");
        assert!(
            (elevation(pivot, turned.translation) - FRAC_PI_2 * 0.5).abs() < 0.01,
            "the horizontal drag moved the elevation"
        );
        let to_pivot = (pivot - turned.translation).normalize();
        assert!(
            turned.forward().dot(to_pivot) > 0.9999,
            "the camera is not facing what it is turning around"
        );

        // The release drops the pivot, reads the angles back off the pose the
        // orbit left, and spends the velocity so nothing carries into the keys.
        frame(&mut app, false, Vec2::ZERO);
        let (settled, after) = state(&mut app);
        assert_eq!(after.pivot, None, "the drag outlived the button");
        assert_eq!(after.velocity, Vec3::ZERO);
        let (yaw, pitch) = heading(settled.rotation);
        assert!((after.yaw - yaw).abs() < 1e-5, "the yaw is out of step");
        assert!((after.pitch - pitch).abs() < 1e-5, "the pitch is out of step");
    }

    /// Dragging down tips the island down and lifts the eye over it, and the
    /// band is what stops that short of the top rather than letting it flip.
    #[test]
    fn a_downward_drag_lifts_the_eye_and_stops_short_of_the_top() {
        let mut app = viewer(Vec3::new(0.0, 400.0, 400.0), Vec3::ZERO);
        frame(&mut app, true, Vec2::ZERO);
        let (_, pressed) = state(&mut app);
        let pivot = pressed.pivot.expect("the press picks a pivot");

        frame(&mut app, true, Vec2::new(0.0, FALLBACK_VIEWPORT_WIDTH * 0.1));
        let (lifted, _) = state(&mut app);
        assert!(
            elevation(pivot, lifted.translation) > FRAC_PI_2 * 0.5,
            "the eye did not rise over the pivot"
        );

        // Far more drag than the band has room for, twice over, neither frame
        // of which may carry the eye past the pole.
        for _ in 0..2 {
            frame(&mut app, true, Vec2::new(0.0, FALLBACK_VIEWPORT_WIDTH));
            let (high, _) = state(&mut app);
            assert!(
                elevation(pivot, high.translation) <= ORBIT_MAX_ELEVATION + 1e-4,
                "the drag went over the top"
            );
        }
    }
}
