//! Unattended capture: render until the island has settled, write one PNG and
//! the metadata beside it, then exit. Used to verify the renderer without a
//! person at the keyboard, and to compare one renderer change against the last.
//!
//! Nothing is on screen while it runs. The camera is pointed at
//! [`CaptureTarget`]'s offscreen image instead of at a window surface, `main`
//! leaves both the primary window and winit's event loop out of the run, and
//! the frames are driven by the schedule runner. A capture therefore cannot
//! raise a window over whatever is in front of it, and cannot come back solid
//! black either: there is no compositor in the path to decline to keep a
//! surface current.
//!
//! The settle is counted in frames alone. It used to hold for a frame count and
//! a wall-clock second, which made the capture depend on how fast those frames
//! happened to run; the frozen water clock in `capture` removes the last reason
//! for the scene to care what time it is at all.
//!
//! Every frame-indexed sequence in the render stack repeats over
//! [`CAPTURE_FRAME_PERIOD`] frames and the capture waits for a multiple of it,
//! so the temporal jitter, the occlusion noise, the contact-shadow noise and the
//! dithered vegetation LOD all stand where they stood last time.
//!
//! The temporal resolve is the one thing that cannot be held that way, and two
//! captures of one command are close rather than identical because of it: its
//! history is the whole run of frames rather than this one. Rendering offscreen
//! took most of that away with the compositor, which used to decline to present
//! frames and leave a mark on the history that decayed without quite going.
//! Three captures of `--view stream --terrain-size 128`, compared each against
//! each on an M3 Pro: between 0.04 and 0.07 per cent of pixels differ and none
//! by more than eight steps in 255, where a windowed pair of the same command
//! used to differ on five per cent and by up to seventeen. What is left is a
//! residual no warm-up length shortens, so the count below is set for the
//! pipeline compiles rather than against it.

use std::{ffi::OsString, fs, path::PathBuf, time::Instant};

use bevy::{
    camera::RenderTarget,
    diagnostic::FrameCount,
    ecs::system::SystemParam,
    prelude::*,
    render::{
        render_resource::{TextureFormat, TextureUsages},
        renderer::RenderAdapterInfo,
        view::screenshot::{Screenshot, save_to_disk},
    },
};
use motu::{GenerationMethod, IslandOptions};

use crate::{
    budget::RenderBudget,
    camera::{self, ViewPose},
    capture::{DebugView, WaterClock},
    island_gen::{GeneratedIsland, GenerationSettings},
    options,
    weather::Weather,
};

/// Frames held after the island appears, before the capture is asked for.
///
/// Everything the frame is built from has to have settled by then. The
/// atmosphere, occlusion, contact shadow and bloom pipelines all compile on
/// first use, the meshes have to reach the GPU, and the temporal resolve carries
/// at least a sixty-seventh of each frame into its history, so a still scene is
/// inside a thousandth of its converged value by three hundred. Five multiples
/// of [`CAPTURE_FRAME_PERIOD`] leaves room for the compiles and for the slowest
/// frames of a two-million-vertex island, and comes to about the five seconds
/// the settle used to spend waiting on a clock.
///
/// Longer buys nothing measurable: 768 frames left two captures of one command
/// no closer than 320 did, because what is left over is not a transient a count
/// outlasts.
const SETTLE_FRAMES: u32 = 320;
/// Frames the render stack's own frame-indexed sequences repeat over, and so the
/// multiple of the frame counter a capture is taken on.
///
/// Four of them index by frame number: the temporal jitter walks eight Halton
/// offsets, its history ping-pongs between two textures, the occlusion pass
/// steps through sixty-four noise offsets and the contact shadows read
/// thirty-two layers of a blue-noise volume. Sixty-four frames return every one
/// of them to where it started. Generation takes as long as it takes, so the
/// frame the island lands on is not the same twice; waiting for a multiple of
/// this is what stands two captures at the same point in all four anyway.
const CAPTURE_FRAME_PERIOD: u32 = 64;
/// Frames after the island lands before the frame-time clock starts.
///
/// The pipelines compile on their first use and the meshes are still crossing
/// to the GPU, so the opening frames of a settle run to hundreds of
/// milliseconds and say nothing about the frame the capture ends on. One
/// [`CAPTURE_FRAME_PERIOD`] is past all of it, and leaves four more to average
/// over.
const TIMING_START_FRAMES: u32 = CAPTURE_FRAME_PERIOD;
/// Frames to wait after the file appears, covering the tail of the write.
const FLUSH_FRAMES: u32 = 5;
/// Frames after the request before the capture is treated as failed.
const CAPTURE_TIMEOUT_FRAMES: u32 = 1_800;

/// The pixel size every capture is rendered and written at.
///
/// It used to be whatever the window turned out to be — 1280x720 asked for and
/// twice that back from a Retina display — so the number a sidecar reported was
/// a property of the screen the run happened to open on. An offscreen image has
/// no scale factor behind it, so the size is stated here and is the size the
/// sidecar reports. The aspect ratio is the one those windows had, and the
/// projection's field of view is vertical, so the framing is unchanged.
const CAPTURE_RESOLUTION: UVec2 = UVec2::new(2560, 1440);
/// The capture image's format, matching the `Bgra8UnormSrgb` a window surface
/// presents in channel width and in transfer function; the two differ only in
/// channel order, which the readback undoes either way.
const CAPTURE_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

/// The offscreen image a capture renders into, present only under
/// `--screenshot`.
///
/// `camera::spawn_camera` aims the camera at it when it is here and leaves the
/// camera on the primary window when it is not, so the one resource is what
/// separates a headless run from every other one.
#[derive(Resource)]
pub struct CaptureTarget {
    image: Handle<Image>,
    size: UVec2,
}

impl CaptureTarget {
    /// The camera component that renders into this image.
    pub fn render_target(&self) -> RenderTarget {
        RenderTarget::Image(self.image.clone().into())
    }
}

pub struct ScreenshotPlugin {
    pub path: PathBuf,
    /// The `--view` and `--variant` names, which the poses and options they
    /// resolved to no longer carry and the metadata has to report.
    pub view: String,
    pub variant: String,
}

#[derive(Resource)]
struct ScreenshotPath(PathBuf);

/// The two names the resolved pose and option set have already forgotten.
#[derive(Resource)]
struct CaptureNames {
    view: String,
    variant: String,
}

#[derive(Resource, Default)]
struct CaptureProgress {
    frames: u32,
    requested: bool,
    frames_since_request: u32,
    /// When the frame-time clock started, and how many frames had passed by
    /// then. Set once, [`TIMING_START_FRAMES`] after the island lands.
    timing_from: Option<(Instant, u32)>,
}

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        // Zero-filled until the first frame renders over it; the capture waits
        // out the settle either way, so nothing ever reads the initial content.
        let mut image = Image::new_target_texture(
            CAPTURE_RESOLUTION.x,
            CAPTURE_RESOLUTION.y,
            CAPTURE_FORMAT,
            None,
        );
        // Nothing samples the target: the camera renders into it and the
        // readback copies out of a texture of its own. Declaring it sampleable
        // anyway costs the driver its attachment compression, and measurably
        // costs the settle for it.
        image
            .texture_descriptor
            .usage
            .remove(TextureUsages::TEXTURE_BINDING);
        let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.insert_resource(CaptureTarget {
            image,
            size: CAPTURE_RESOLUTION,
        })
        .insert_resource(ScreenshotPath(self.path.clone()))
        .insert_resource(CaptureNames {
            view: self.view.clone(),
            variant: self.variant.clone(),
        })
        .init_resource::<CaptureProgress>()
        .add_systems(Startup, clear_previous)
        .add_systems(Update, capture);
    }
}

/// The sidecar path for one capture: the image path with `.txt` after it, so
/// the two sort together and neither can be mistaken for the other's island.
fn sidecar(path: &std::path::Path) -> PathBuf {
    let mut name = OsString::from(path);
    name.push(".txt");
    PathBuf::from(name)
}

/// Stale files from an earlier run would otherwise satisfy the write check, or
/// leave a sidecar standing beside a capture that never landed.
fn clear_previous(path: Res<ScreenshotPath>) {
    for stale in [path.0.clone(), sidecar(&path.0)] {
        if let Err(error) = fs::remove_file(&stale)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            warn!("could not clear {}: {error}", stale.display());
        }
    }
}

/// Everything the sidecar records off the running app rather than off the
/// command line. Collected as one parameter so the capture system stays inside
/// the argument count the rest of the crate keeps to.
#[derive(SystemParam)]
struct Recorded<'w> {
    names: Res<'w, CaptureNames>,
    settings: Res<'w, GenerationSettings>,
    pose: Res<'w, ViewPose>,
    weather: Res<'w, Weather>,
    clock: Res<'w, WaterClock>,
    debug_view: Res<'w, DebugView>,
    /// Absent only where there is no wgpu backend, which is also where there is
    /// nothing to capture.
    adapter: Option<Res<'w, RenderAdapterInfo>>,
    /// The image the capture was rendered into, and so the size it was written
    /// at. There is no window to ask, and nothing scales this one.
    target: Res<'w, CaptureTarget>,
    /// What the culling stages left standing on the frame before this one. Read
    /// into the log rather than into the sidecar: it is a property of the pose
    /// and the camera, but the sidecar is meant to be byte-identical between two
    /// captures of one command, and a census taken a frame earlier is not.
    budget: Res<'w, RenderBudget>,
}

/// The mean frame time over the settle, once the pipelines have compiled.
///
/// Wall clock over a counted run of frames, which under `--screenshot` is the
/// whole of what the app does: nothing is presented, nothing waits on a
/// vertical blank, and the schedule runner takes frames as fast as the renderer
/// finishes them.
fn report_frame_time(progress: &CaptureProgress) {
    let Some((started, from)) = progress.timing_from else {
        return;
    };
    let frames = progress.frames.saturating_sub(from);
    if frames == 0 {
        return;
    }
    let elapsed = started.elapsed().as_secs_f32();
    info!(
        "settle: {frames} frames in {elapsed:.2} s, {:.1} ms per frame",
        elapsed / f32::from(u16::try_from(frames).unwrap_or(u16::MAX)) * 1_000.0
    );
}

fn capture(
    mut commands: Commands,
    path: Res<ScreenshotPath>,
    frames: Res<FrameCount>,
    island: Option<Res<GeneratedIsland>>,
    mut progress: ResMut<CaptureProgress>,
    mut exit: MessageWriter<AppExit>,
    recorded: Recorded,
) {
    if progress.requested {
        progress.frames_since_request += 1;
        let written = fs::metadata(&path.0).is_ok_and(|file| file.len() > 0);
        if written && progress.frames_since_request > FLUSH_FRAMES {
            info!("wrote {}", path.0.display());
            write_metadata(&path.0, &recorded);
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
    if progress.frames == TIMING_START_FRAMES {
        progress.timing_from = Some((Instant::now(), progress.frames));
    }
    if progress.frames < SETTLE_FRAMES || !frames.0.is_multiple_of(CAPTURE_FRAME_PERIOD) {
        return;
    }
    info!(
        "capturing on frame {}, {} after the island",
        frames.0, progress.frames
    );
    report_frame_time(&progress);
    info!("terrain budget: {}", recorded.budget.terrain.line());
    info!("scatter budget: {}", recorded.budget.scatter.line());
    info!(
        "scatter groups: {} of {} kept",
        recorded.budget.groups.drawn_entities, recorded.budget.groups.entities
    );
    commands
        .spawn(Screenshot::image(recorded.target.image.clone()))
        .observe(save_to_disk(path.0.clone()));
    progress.requested = true;
}

fn write_metadata(path: &std::path::Path, recorded: &Recorded) {
    let metadata = Metadata {
        seed: recorded.settings.seed,
        options: recorded.settings.options,
        method: recorded.settings.method,
        variant: recorded.names.variant.clone(),
        view: recorded.names.view.clone(),
        pose: *recorded.pose,
        weather: *recorded.weather,
        debug_view: *recorded.debug_view,
        water_clock: recorded.clock.0,
        adapter: recorded.adapter.as_ref().map_or_else(
            || String::from("unknown"),
            |info| format!("{} ({})", info.name, info.backend),
        ),
        resolution: recorded.target.size,
    };
    let path = sidecar(path);
    match fs::write(&path, metadata.text()) {
        Ok(()) => info!("wrote {}", path.display()),
        Err(error) => warn!("could not write {}: {error}", path.display()),
    }
}

/// Everything one capture records beside its PNG, in the order it records it.
struct Metadata {
    seed: u64,
    options: IslandOptions,
    method: GenerationMethod,
    variant: String,
    view: String,
    pose: ViewPose,
    weather: Weather,
    debug_view: DebugView,
    water_clock: f32,
    adapter: String,
    resolution: UVec2,
}

impl Metadata {
    /// Plain `key: value` lines in a fixed order, so two captures of the same
    /// command produce byte-identical sidecars and `diff` between two captures
    /// reports what actually differs about them.
    fn text(&self) -> String {
        let moved = options::non_default(&self.options);
        let look = self.weather.look();
        // The stack the camera always carries, then whatever the look adds to
        // it. Under `clear` the look adds nothing, so the line is exactly the
        // one every capture taken before weather existed already recorded.
        let features: Vec<&str> = camera::RENDER_FEATURES
            .into_iter()
            .chain(look.features())
            .collect();
        let lines = [
            format!(
                "crate: {} {}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            ),
            format!("seed: {}", self.seed),
            format!("terrain-size: {}", self.options.terrain_size),
            format!("generation-method: {}", self.method),
            format!("variant: {}", self.variant),
            format!(
                "non-default-options: {}",
                if moved.is_empty() {
                    "none"
                } else {
                    moved.as_str()
                }
            ),
            format!("view: {}", self.view),
            format!("eye: {}", vector(self.pose.eye)),
            format!("target: {}", vector(self.pose.target)),
            format!("weather: {}", self.weather.label()),
            format!("debug-view: {}", self.debug_view.label()),
            format!("sun-direction: {}", vector(look.sun_direction)),
            format!("exposure-ev100: {}", camera::EXPOSURE.ev100),
            format!("water-clock-seconds: {}", self.water_clock),
            format!("warm-up-frames: {SETTLE_FRAMES}"),
            format!("capture-frame-period: {CAPTURE_FRAME_PERIOD}"),
            format!("renderer: {}", features.join(", ")),
            format!("adapter: {}", self.adapter),
            format!("resolution: {}x{}", self.resolution.x, self.resolution.y),
        ];
        let mut text = lines.join("\n");
        text.push('\n');
        text
    }
}

/// `{}` on an f32 is the shortest form that reads back as the same value, which
/// is what lets a pose be copied out of a sidecar into `camera`'s own table.
fn vector(value: Vec3) -> String {
    format!("{}, {}, {}", value.x, value.y, value.z)
}

#[cfg(test)]
mod tests {
    use motu::{GenerationMethod, IslandOptions};

    use super::{
        CAPTURE_FORMAT, CAPTURE_RESOLUTION, DebugView, Metadata, TextureFormat, UVec2, Vec3,
        ViewPose, Weather, sidecar,
    };

    fn metadata() -> Metadata {
        Metadata {
            seed: 666,
            options: IslandOptions {
                terrain_size: 128,
                max_height: 0.35,
                ..IslandOptions::default()
            },
            method: GenerationMethod::Gpu,
            variant: String::from("eroded"),
            view: String::from("stream"),
            pose: ViewPose {
                eye: Vec3::new(-531.5, 6.3, 339.0),
                target: Vec3::new(-524.0, 5.8, 324.0),
            },
            weather: Weather::default(),
            debug_view: DebugView::Flow,
            water_clock: 27.5,
            adapter: String::from("Apple M3 Pro (Metal)"),
            resolution: UVec2::new(2560, 1440),
        }
    }

    /// The sidecar is only worth writing if everything needed to reproduce or
    /// to place the image is on it, one key per line and always in the same
    /// order.
    #[test]
    fn the_sidecar_records_every_input() {
        let text = metadata().text();
        // A value may hold a colon of its own, so only the first one separates
        // a key from it.
        let keys: Vec<&str> = text
            .lines()
            .map(|line| line.split_once(": ").expect("every line is key: value").0)
            .collect();
        assert_eq!(
            keys,
            [
                "crate",
                "seed",
                "terrain-size",
                "generation-method",
                "variant",
                "non-default-options",
                "view",
                "eye",
                "target",
                "weather",
                "debug-view",
                "sun-direction",
                "exposure-ev100",
                "water-clock-seconds",
                "warm-up-frames",
                "capture-frame-period",
                "renderer",
                "adapter",
                "resolution",
            ]
        );
        for expected in [
            "seed: 666",
            "terrain-size: 128",
            "variant: eroded",
            "non-default-options: --max-height 0.35",
            "view: stream",
            "eye: -531.5, 6.3, 339",
            "weather: clear",
            "debug-view: flow",
            "water-clock-seconds: 27.5",
            "warm-up-frames: 320",
            "resolution: 2560x1440",
            "adapter: Apple M3 Pro (Metal)",
        ] {
            assert!(
                text.contains(expected),
                "{expected} is missing from\n{text}"
            );
        }
        // The same inputs have to spell the same file, or a diff between two
        // captures would report the sidecar rather than the capture.
        assert_eq!(text, metadata().text());
    }

    /// The look is what decides both the sun the frame was lit by and what was
    /// in the image stack, so both lines have to follow it. Under `clear` the
    /// stack is exactly the camera's own, which is what keeps a sidecar taken
    /// before weather existed comparable with one taken after.
    #[test]
    fn the_sidecar_follows_the_weather_look() {
        let mut metadata = metadata();
        let clear = metadata.text();
        assert!(clear.contains("weather: clear"), "{clear}");
        assert!(
            clear.contains(&format!(
                "renderer: {}",
                super::camera::RENDER_FEATURES.join(", ")
            )),
            "{clear}"
        );

        metadata.weather = Weather::named("overcast").expect("overcast is in the table");
        let overcast = metadata.text();
        assert!(overcast.contains("weather: overcast"), "{overcast}");
        for feature in [
            "clouds",
            "cloud-shadows",
            "volumetric-fog",
            "colour-grading",
        ] {
            assert!(
                overcast.contains(feature),
                "{feature} is missing\n{overcast}"
            );
        }
        // And it was lit from somewhere else, which is the point of a look.
        assert_ne!(
            clear.lines().find(|line| line.starts_with("sun-direction")),
            overcast
                .lines()
                .find(|line| line.starts_with("sun-direction"))
        );
    }

    /// A default island reports that it is one rather than an empty value.
    #[test]
    fn a_default_island_reports_no_moved_parameters() {
        let mut metadata = metadata();
        metadata.options = IslandOptions::default();
        assert!(metadata.text().contains("non-default-options: none"));
    }

    /// The capture size used to be the window's, so the two could not drift
    /// apart. Now that it is stated, only the aspect ratio holds a pose to
    /// framing the same thing on screen as in its capture — the field of view
    /// is vertical, so nothing else about the projection depends on the size.
    #[test]
    fn the_capture_keeps_the_viewer_s_aspect_ratio() {
        let window = crate::WINDOW_RESOLUTION;
        assert_eq!(
            CAPTURE_RESOLUTION.x * window.y,
            CAPTURE_RESOLUTION.y * window.x,
            "{CAPTURE_RESOLUTION} does not frame what {window} does"
        );
    }

    /// Both halves of the capture's format have to hold, and neither announces
    /// itself: a format the readback cannot convert loses the PNG outright, and
    /// one that is not sRGB writes the tone-mapped frame through the wrong
    /// curve and looks merely dark.
    #[test]
    fn the_capture_format_survives_the_readback() {
        assert!(
            matches!(
                CAPTURE_FORMAT,
                TextureFormat::Rgba8UnormSrgb | TextureFormat::Bgra8UnormSrgb
            ),
            "{CAPTURE_FORMAT:?} is not a format Image::try_into_dynamic reads back"
        );
        assert!(CAPTURE_FORMAT.is_srgb());
    }

    #[test]
    fn the_sidecar_sits_beside_its_image() {
        assert_eq!(
            sidecar(std::path::Path::new("captures/stream.png")),
            std::path::PathBuf::from("captures/stream.png.txt")
        );
    }
}
