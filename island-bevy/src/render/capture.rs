//! What a capture pins down: the clock both water shaders run on, and the
//! diagnostic channel the terrain and water fragment stages can be switched to.
//!
//! Both are uniforms the app owns and writes into the material extensions, not
//! values the shaders read off the render world's own globals. A capture has to
//! answer the same way twice, and `globals.time` cannot: it is wall-clock, so it
//! carries however long generation took and however fast the frames before the
//! capture happened to run. The clock here is a resource one system advances, so
//! `--screenshot` holds it at a constant and every crest, streak and foam edge
//! in the frame stands where it stood in the last capture. An ordinary run
//! advances it by the frame delta and animates exactly as it did before.
//!
//! The diagnostic channel is the same kind of value from the other direction: a
//! number nothing in the scene decides, written into the same blocks, switching
//! the surfaces that carry a channel over to it and leaving the rest shaded.

use bevy::{asset::AssetEvent, prelude::*};

use crate::surface::{OceanMaterial, RiverMaterial, SprayMaterial, TerrainMaterial};

/// Seconds the water clock is held at under `--screenshot`.
///
/// Both waters carry one lattice axis that only time moves — the sea's at 0.055
/// units per second, a river's at 0.09 — and this value puts both of them
/// mid-cell rather than on a lattice plane, where the field would be its own
/// unevolved slice. Every drifting layer has travelled several wavelengths by
/// then as well, so a capture catches waves, streaks and surf mid-phase rather
/// than the still, undrifted field the shaders start from.
pub const FROZEN_SECONDS: f32 = 27.5;

/// Seconds before the shared animation clock returns to zero.
///
/// Twenty thousand seconds is a common multiple of the spray lifetimes and the
/// water shaders' five-hundred-second blended phases. Cloud drift uses the
/// clock's separate high-precision elapsed value and wraps spatially in its
/// rotated projection plane. Keeping the shader value below this bound
/// preserves sub-frame precision and keeps the time axes handed to the lattice
/// hashes inside their useful range, however long the viewer itself stays open.
pub const WATER_CLOCK_WRAP_SECONDS: f32 = 20_000.0;

/// The bounded seconds the water shaders take their motion from, followed by
/// high-precision elapsed seconds for CPU-side cloud projection. The latter is
/// reduced spatially before it reaches a transform and never enters a shader.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct WaterClock(pub f32, f64);

impl WaterClock {
    fn at(seconds: f32) -> Self {
        Self(seconds, f64::from(seconds))
    }

    /// Elapsed session time before the shader clock's bounded wrap.
    #[must_use]
    pub fn elapsed_seconds(self) -> f64 {
        self.1
    }
}

/// One diagnostic channel, or ordinary shading.
///
/// Each name stands for one channel of one surface, and a surface that has no
/// channel for the selected view shades normally, so a terrain channel is still
/// seen through the water standing over it and a water channel still stands on
/// shaded ground. `debug.wgsl` spells the same list on the shader side and the
/// two are only correct read together.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DebugView {
    /// Every surface shaded as it always is.
    #[default]
    Off,
    /// Terrain: the generator's material triple, as red, green and blue.
    Weights,
    /// Terrain: the per-vertex proximity to running water.
    Wetness,
    /// Terrain: how far off level the surface stands.
    Slope,
    /// River: the downstream heading in red and blue, its speed in green.
    Flow,
    /// River: the surface grade in red, the whitening a running reach takes
    /// from it in green.
    Grade,
    /// Sea and river: the optical depth the absorption is taken over.
    Depth,
    /// River: which of the four water states the surface is in — calm blue,
    /// running green, plunge orange, falling white.
    State,
    /// River: the ordinary surface with every foam contribution removed, which
    /// is where a fall has to still read as a body of water.
    Foamless,
    /// Terrain: which chunk of the grid the ground belongs to and which level
    /// of detail is drawing it — green for LOD 0, amber for LOD 1, red for
    /// LOD 2, with the chunk's own square shaded in two tones so the seams
    /// between them can be read.
    Chunks,
}

impl DebugView {
    /// In the order `--debug-view`, the help text and the HUD list them.
    pub const ALL: [Self; 10] = [
        Self::Off,
        Self::Weights,
        Self::Wetness,
        Self::Slope,
        Self::Flow,
        Self::Grade,
        Self::Depth,
        Self::State,
        Self::Foamless,
        Self::Chunks,
    ];

    /// The name `--debug-view` spells this view with.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Weights => "weights",
            Self::Wetness => "wetness",
            Self::Slope => "slope",
            Self::Flow => "flow",
            Self::Grade => "grade",
            Self::Depth => "depth",
            Self::State => "state",
            Self::Foamless => "foamless",
            Self::Chunks => "chunks",
        }
    }

    /// The value the shaders switch on. Zero is ordinary shading, which is what
    /// a material extension carries before anything has written to it.
    #[must_use]
    pub const fn flag(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Weights => 1,
            Self::Wetness => 2,
            Self::Slope => 3,
            Self::Flow => 4,
            Self::Grade => 5,
            Self::Depth => 6,
            Self::State => 7,
            Self::Foamless => 8,
            Self::Chunks => 9,
        }
    }

    /// Looks a `--debug-view` name up. Rejected at parse time rather than
    /// silently shading normally, because a diagnostic capture that quietly
    /// carries the ordinary scene is worse than no capture.
    pub fn named(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|view| view.label() == name)
            .ok_or_else(|| {
                format!(
                    "unknown debug view {name:?}; expected one of {}",
                    Self::names()
                )
            })
    }

    /// The view names in table order, for help text and parse errors.
    #[must_use]
    pub fn names() -> String {
        Self::ALL.map(Self::label).join(", ")
    }
}

/// Installed on every run. Under `--screenshot` the clock opens on
/// [`FROZEN_SECONDS`] and nothing advances it.
pub struct CapturePlugin {
    pub debug_view: DebugView,
    pub frozen: bool,
}

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WaterClock::at(if self.frozen {
            FROZEN_SECONDS
        } else {
            0.0
        }))
        .insert_resource(self.debug_view)
        .add_systems(Update, write_settings);
        if !self.frozen {
            app.add_systems(Update, advance_clock.before(write_settings));
        }
    }
}

fn advance_clock(time: Res<Time>, mut clock: ResMut<WaterClock>) {
    clock.1 += time.delta_secs_f64();
    clock.0 = wrap_clock(clock.0 + time.delta_secs());
}

fn wrap_clock(seconds: f32) -> f32 {
    seconds.rem_euclid(WATER_CLOCK_WRAP_SECONDS)
}

/// Writes the animated values into every water material each frame, and the
/// diagnostic value into terrain only when it changes or a material arrives.
///
/// A frozen clock never changes again after the frame it is inserted on, and
/// the rivers are spawned seconds later, when the island lands, so the animated
/// materials deliberately remain unconditional. Terrain has no animated value:
/// its added-asset messages cover that late-spawn case without marking every
/// LOD material modified and rebuilding its bind group on every frame.
fn write_settings(
    clock: Res<WaterClock>,
    view: Res<DebugView>,
    mut terrain_events: MessageReader<AssetEvent<TerrainMaterial>>,
    mut terrains: ResMut<Assets<TerrainMaterial>>,
    mut oceans: ResMut<Assets<OceanMaterial>>,
    mut rivers: ResMut<Assets<RiverMaterial>>,
    mut sprays: ResMut<Assets<SprayMaterial>>,
) {
    let flag = view.flag();
    if view.is_changed() {
        // The full pass already covers every material named by an added event;
        // drain the reader so those events are not replayed next frame.
        for _ in terrain_events.read() {}
        for (_, material) in terrains.iter_mut() {
            material.extension.settings.debug_view = flag;
        }
    } else {
        for event in terrain_events.read() {
            if let AssetEvent::Added { id } = event
                && let Some(mut material) = terrains.get_mut(*id)
            {
                material.extension.settings.debug_view = flag;
            }
        }
    }
    for (_, material) in oceans.iter_mut() {
        material.extension.settings.water_time = clock.0;
        material.extension.settings.debug_view = flag;
    }
    for (_, material) in rivers.iter_mut() {
        material.extension.settings.water_time = clock.0;
        material.extension.settings.debug_view = flag;
    }
    for (_, material) in sprays.iter_mut() {
        material.extension.settings.water_time = clock.0;
        material.extension.settings.debug_view = flag;
    }
}

#[cfg(test)]
mod tests {
    use super::{DebugView, WATER_CLOCK_WRAP_SECONDS, wrap_clock};

    /// The names are what the flag, the help text, the HUD and the capture
    /// sidecar all spell a view with, and the flags are what the shaders switch
    /// on, so a duplicate in either list would quietly merge two channels.
    #[test]
    fn every_view_is_listed_once() {
        for (index, view) in DebugView::ALL.iter().enumerate() {
            assert!(
                DebugView::ALL[..index]
                    .iter()
                    .all(|earlier| earlier.label() != view.label()
                        && earlier.flag() != view.flag()),
                "{} is listed twice",
                view.label()
            );
            assert_eq!(DebugView::named(view.label()), Ok(*view));
        }
        assert_eq!(DebugView::default().flag(), 0);
    }

    #[test]
    fn rejects_an_unknown_view() {
        let error = DebugView::named("curvature").unwrap_err();
        assert!(error.contains("unknown debug view"), "{error}");
        for view in DebugView::ALL {
            assert!(error.contains(view.label()), "{error}");
        }
    }

    #[test]
    fn water_clock_stays_in_its_precise_interval() {
        assert_eq!(
            wrap_clock(WATER_CLOCK_WRAP_SECONDS).to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            wrap_clock(WATER_CLOCK_WRAP_SECONDS + 0.25).to_bits(),
            0.25_f32.to_bits()
        );
        assert_eq!(
            wrap_clock(-0.25).to_bits(),
            (WATER_CLOCK_WRAP_SECONDS - 0.25).to_bits()
        );
    }
}
