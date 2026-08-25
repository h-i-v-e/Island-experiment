//! Shader materials for every generated surface: the ground and rock the island
//! is built from, the two waters that stand over them, and the mist a fall
//! throws into the air above both.
//!
//! All five extend `StandardMaterial` rather than replacing it, so the whole
//! lighting stack — shadow cascades, the depth and motion-vector prepasses,
//! screen-space occlusion, contact shadows and aerial perspective — keeps
//! working; only the forward stage is ours, and only the spray replaces the
//! vertex half of it. They are five extensions rather than one behind a mode
//! switch because they answer different questions; what they have in common is
//! the noise library, and that is shared as WGSL.
//!
//! The two water extensions read the opaque depth prepass to recover how far
//! the view ray runs through water before it reaches the bottom. That is only
//! sound because they blend: a blended surface is never written to the prepass,
//! so what a water fragment finds there is always the solid ground under it.
//! The spray blends for the same reason and gains the same exemption, which is
//! also why its vertex stage may move a droplet without a prepass to keep in
//! step with.
//!
//! Four of the five settings blocks end in fields the app writes rather than
//! the material: the seconds the waters and the spray animate from, and the
//! diagnostic channel `--debug-view` switches the output to. `capture` owns
//! both and writes them in every frame; the values here are only what a
//! material carries before it does.

use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{ShaderRef, load_shader_library},
};

/// The terrain surface: the generator's material triple per vertex, every band,
/// detail layer and surface response per pixel.
pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;
/// The merged river-rock body.
pub type RockMaterial = ExtendedMaterial<StandardMaterial, RockExtension>;
/// The open sea: depth absorption, waves, Fresnel and surf.
pub type OceanMaterial = ExtendedMaterial<StandardMaterial, OceanExtension>;
/// The generator's river surface: flow, bed, banks and drops.
pub type RiverMaterial = ExtendedMaterial<StandardMaterial, RiverExtension>;
/// The droplet cloud at the foot of a fall.
pub type SprayMaterial = ExtendedMaterial<StandardMaterial, SprayExtension>;

/// Metres of world space past which the terrain's metre-scale relief is gone.
/// Beyond this the layer is under a pixel wide and only feeds the temporal
/// resolve with noise; the overview pose sits at 1.6 km, well outside it.
const TERRAIN_DETAIL_RANGE: f32 = 600.0;
/// How far a detail gradient may bend the mesh normal. Strong enough that a
/// cliff answers the sun, weak enough that a slope does not sparkle.
const TERRAIN_NORMAL_STRENGTH: f32 = 0.30;
/// Metres above the sea plane that the shoreline stays damp over.
const TERRAIN_WET_BAND: f32 = 3.0;

/// The bodies are centimetres across, so their detail is only ever resolved
/// from a few metres away.
const ROCK_DETAIL_RANGE: f32 = 25.0;
const ROCK_NORMAL_STRENGTH: f32 = 0.28;
const ROCK_ROUGHNESS: f32 = 0.86;
const ROCK_ROUGHNESS_SPREAD: f32 = 0.22;

/// Extinction per metre of sea water along the view ray. What has to saturate
/// is a ray crossing the generator's own shelf, which is one to three metres
/// deep across the whole square and then stops — not the hundred metres an open
/// ocean would offer. At this coefficient two metres of water under a shallow
/// view is already six sevenths opaque, and what still shows through it is
/// seabed the terrain shader has taken to the same deep tone by then, so the
/// square's edge has nothing left to show. A handspan of water against a beach
/// still carries three quarters of its own sand.
const OCEAN_ABSORPTION: f32 = 0.55;
/// Metres at which the sea's longest wave layer has faded out. Past it the
/// crests are under a pixel and only feed the temporal resolve with noise.
const OCEAN_WAVE_RANGE: f32 = 4_000.0;
const OCEAN_WAVE_STRENGTH: f32 = 1.6;
const OCEAN_ROUGHNESS: f32 = 0.13;
/// Depth the surf band is cut off at, and the far shorter run the surface
/// itself fades out over as the bottom reaches it. Depth is only half of what
/// places the surf — the shader also asks how much ground is left before the
/// waterline — because a band in depth alone fills every cove on a shelf this
/// flat.
const OCEAN_FOAM_DEPTH: f32 = 1.4;
const OCEAN_SHORE_DEPTH: f32 = 0.35;
/// Where the sea starts handing the frame back to the sky, and how long it
/// takes. Both stand well past the two-kilometre island: fading any earlier
/// would let the seabed and the empty ocean disagree again.
const OCEAN_HAZE_START: f32 = 7_000.0;
const OCEAN_HAZE_RANGE: f32 = 48_000.0;

/// Extinction per metre of fresh water. Well under half the sea's, because a
/// channel is about a metre deep and its bed is the thing worth seeing.
const RIVER_ABSORPTION: f32 = 0.22;
/// Metres per second the surface travels on the flat, and what a full grade
/// adds. The flat figure is an ordinary walking pace; the drop figure is what
/// turns a steep face into falling water rather than a tilted pond.
const RIVER_FLOW_SPEED: f32 = 1.1;
const RIVER_GRADE_SPEED: f32 = 7.0;
/// Metres of bank distance the surface fades in over. The generator's narrowest
/// channel is two metres wide, so this has to stay well inside one.
const RIVER_BANK_METRES: f32 = 0.30;
/// Grade at which the surface starts to break white: about eleven degrees.
const RIVER_FOAM_GRADE: f32 = 0.20;
/// The world-space chop is centimetres across, so it is only ever resolved from
/// close in.
const RIVER_DETAIL_RANGE: f32 = 60.0;
const RIVER_WAVE_STRENGTH: f32 = 2.2;

pub struct SurfaceMaterialsPlugin;

impl Plugin for SurfaceMaterialsPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "noise.wgsl");
        load_shader_library!(app, "debug.wgsl");
        embedded_asset!(app, "terrain.wgsl");
        embedded_asset!(app, "rock.wgsl");
        embedded_asset!(app, "ocean.wgsl");
        embedded_asset!(app, "river.wgsl");
        embedded_asset!(app, "spray.wgsl");
        app.add_plugins((
            MaterialPlugin::<TerrainMaterial>::default(),
            MaterialPlugin::<RockMaterial>::default(),
            MaterialPlugin::<OceanMaterial>::default(),
            MaterialPlugin::<RiverMaterial>::default(),
            MaterialPlugin::<SprayMaterial>::default(),
        ));
    }
}

/// Uniform block shared with `terrain.wgsl`.
#[derive(Clone, Copy, Debug, Reflect, ShaderType)]
pub struct TerrainSettings {
    /// Metres of elevation standing for the generator's normalized height 1.
    pub max_height: f32,
    pub detail_range: f32,
    pub normal_strength: f32,
    pub wet_band: f32,
    /// Metres one chunk of the terrain grid is across. Only the chunk
    /// diagnostic reads it, and it is what lets the shader recover which square
    /// a fragment stands in from its world position alone.
    pub chunk_metres: f32,
    /// The level of detail this material draws. The ground is one material per
    /// level rather than one in total, because a chunk cannot tell the shader
    /// anything itself — a shared handle carries no per-instance value — and
    /// the chunk diagnostic has to name the level it is looking at.
    pub lod_level: u32,
    /// The diagnostic channel the fragment output is switched to, or zero.
    /// Written by `capture`; `debug.wgsl` spells the numbering.
    pub debug_view: u32,
}

/// Terrain extension bindings. The macro blend arrives on the mesh instead:
/// `Mesh::ATTRIBUTE_COLOR` carries the generator's raw material triple, and
/// elevation and slope are read off world position and normal in the shader.
#[derive(Asset, AsBindGroup, Clone, Debug, Reflect)]
pub struct TerrainExtension {
    #[uniform(100)]
    pub settings: TerrainSettings,
}

impl TerrainExtension {
    /// `max_height` is the generator's normalized maximum elevation, and
    /// `chunk_metres` how wide one square of the terrain grid is.
    #[must_use]
    pub fn new(max_height: f32, world_metres: f32, chunk_metres: f32, lod_level: u32) -> Self {
        Self {
            settings: TerrainSettings {
                max_height: (max_height * world_metres).max(1.0),
                detail_range: TERRAIN_DETAIL_RANGE,
                normal_strength: TERRAIN_NORMAL_STRENGTH,
                wet_band: TERRAIN_WET_BAND,
                chunk_metres: chunk_metres.max(1.0),
                lod_level,
                debug_view: 0,
            },
        }
    }
}

impl MaterialExtension for TerrainExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://island_bevy/terrain.wgsl".into()
    }
}

/// Uniform block shared with `rock.wgsl`.
#[derive(Clone, Copy, Debug, Reflect, ShaderType)]
pub struct RockSettings {
    pub detail_range: f32,
    pub normal_strength: f32,
    pub roughness: f32,
    pub roughness_spread: f32,
}

/// Rock extension bindings. `Mesh::ATTRIBUTE_COLOR` carries a deterministic
/// per-body albedo tint, which is as close to per-instance as a merged mesh
/// allows.
#[derive(Asset, AsBindGroup, Clone, Debug, Reflect)]
pub struct RockExtension {
    #[uniform(100)]
    settings: RockSettings,
}

impl Default for RockExtension {
    fn default() -> Self {
        Self {
            settings: RockSettings {
                detail_range: ROCK_DETAIL_RANGE,
                normal_strength: ROCK_NORMAL_STRENGTH,
                roughness: ROCK_ROUGHNESS,
                roughness_spread: ROCK_ROUGHNESS_SPREAD,
            },
        }
    }
}

impl MaterialExtension for RockExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://island_bevy/rock.wgsl".into()
    }
}

/// Uniform block shared with `ocean.wgsl`.
#[derive(Clone, Copy, Debug, Reflect, ShaderType)]
pub struct OceanSettings {
    pub absorption: f32,
    pub wave_range: f32,
    pub wave_strength: f32,
    pub roughness: f32,
    pub foam_depth: f32,
    pub shore_depth: f32,
    pub haze_start: f32,
    pub haze_range: f32,
    /// Seconds every wave layer drifts and evolves by. Written by `capture`.
    pub water_time: f32,
    /// The diagnostic channel the fragment output is switched to, or zero.
    pub debug_view: u32,
}

/// Ocean extension bindings. The sea is one quad whose UVs are a meaningless
/// stretch, so every pattern is read off world position instead, and the depth
/// it stands over is read off the prepass.
#[derive(Asset, AsBindGroup, Clone, Debug, Reflect)]
pub struct OceanExtension {
    #[uniform(100)]
    pub settings: OceanSettings,
}

impl Default for OceanExtension {
    fn default() -> Self {
        Self {
            settings: OceanSettings {
                absorption: OCEAN_ABSORPTION,
                wave_range: OCEAN_WAVE_RANGE,
                wave_strength: OCEAN_WAVE_STRENGTH,
                roughness: OCEAN_ROUGHNESS,
                foam_depth: OCEAN_FOAM_DEPTH,
                shore_depth: OCEAN_SHORE_DEPTH,
                haze_start: OCEAN_HAZE_START,
                haze_range: OCEAN_HAZE_RANGE,
                water_time: 0.0,
                debug_view: 0,
            },
        }
    }
}

impl MaterialExtension for OceanExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://island_bevy/ocean.wgsl".into()
    }
}

/// Uniform block shared with `river.wgsl`.
#[derive(Clone, Copy, Debug, Reflect, ShaderType)]
pub struct RiverSettings {
    /// Metres of world space the generator's normalized island unit stands for.
    pub world_metres: f32,
    pub absorption: f32,
    pub flow_speed: f32,
    pub grade_speed: f32,
    pub bank_metres: f32,
    pub foam_grade: f32,
    pub detail_range: f32,
    pub wave_strength: f32,
    /// Seconds the surface has been running for. Written by `capture`.
    pub water_time: f32,
    /// The diagnostic channel the fragment output is switched to, or zero.
    pub debug_view: u32,
}

/// River extension bindings. `Mesh::ATTRIBUTE_UV_0` carries the generator's own
/// channel parametrisation — downstream distance in V, bank distance in U, both
/// in normalized island units — which is why the world scale has to cross too.
#[derive(Asset, AsBindGroup, Clone, Debug, Reflect)]
pub struct RiverExtension {
    #[uniform(100)]
    pub settings: RiverSettings,
}

impl RiverExtension {
    #[must_use]
    pub fn new(world_metres: f32) -> Self {
        Self {
            settings: RiverSettings {
                world_metres: world_metres.max(1.0),
                absorption: RIVER_ABSORPTION,
                flow_speed: RIVER_FLOW_SPEED,
                grade_speed: RIVER_GRADE_SPEED,
                bank_metres: RIVER_BANK_METRES,
                foam_grade: RIVER_FOAM_GRADE,
                detail_range: RIVER_DETAIL_RANGE,
                wave_strength: RIVER_WAVE_STRENGTH,
                water_time: 0.0,
                debug_view: 0,
            },
        }
    }
}

impl MaterialExtension for RiverExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://island_bevy/river.wgsl".into()
    }
}

/// Uniform block shared with `spray.wgsl`.
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct SpraySettings {
    /// Seconds every droplet is on its arc from. Written by `capture`.
    pub water_time: f32,
    /// The diagnostic channel in force, or zero. Written by `capture`; the
    /// spray answers every one of them by leaving the frame.
    pub debug_view: u32,
}

/// Spray extension bindings. Every droplet is a quad whose attributes are its
/// own launch: `Mesh::ATTRIBUTE_POSITION` is where it leaves the water,
/// `ATTRIBUTE_NORMAL` how fast and which way, `ATTRIBUTE_UV_0` which corner of
/// the quad the vertex is, and `ATTRIBUTE_COLOR` the phase, size, life and
/// brightness that make it its own.
#[derive(Asset, AsBindGroup, Clone, Debug, Default, Reflect)]
pub struct SprayExtension {
    #[uniform(100)]
    pub settings: SpraySettings,
}

impl MaterialExtension for SprayExtension {
    fn vertex_shader() -> ShaderRef {
        "embedded://island_bevy/spray.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://island_bevy/spray.wgsl".into()
    }
}
