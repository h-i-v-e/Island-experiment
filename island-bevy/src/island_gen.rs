//! Off-thread island generation and the handoff resource every renderer reads.

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use motu::{ISLAND_WORLD_METRES, Island, IslandOptions, Mesh, Vec3};

use crate::{cache, options};

/// The named generation variants, as (name, overrides). `None` is a variant
/// that changes nothing, so the generator's own defaults stay the only place
/// the unmodified values are written down.
const VARIANTS: [(&str, Option<Variant>); 2] = [
    ("default", None),
    // Deeper hydraulic carving over much gentler coastal falloff: more incised
    // channels inland and wider, shallower shores.
    (
        "eroded",
        Some(Variant {
            hydraulic_erosion_strength: 4.0,
            coastal_slope_multiplier: 0.25,
        }),
    ),
];

/// The variant `--variant` generates with when it is not given. Also the name
/// `camera` resolves a view's shared pose under.
pub const DEFAULT_VARIANT: &str = VARIANTS[0].0;

/// Ground samples along each edge of the height grid [`IslandData`] carries.
///
/// The island is two kilometres across, so 512 samples put one every 3.9 m and
/// cost 1 MiB in the cache entry — a few per cent of an entry that already runs
/// to tens of megabytes. Bilinear interpolation between samples at that spacing
/// smooths cliff faces into walkable ramps and holds every valley and ridge the
/// eye reads while standing on them, which is all walk mode asks of it.
pub const HEIGHT_GRID: u32 = 512;

/// The option overrides one named variant applies.
#[derive(Clone, Copy, Debug)]
struct Variant {
    hydraulic_erosion_strength: f32,
    coastal_slope_multiplier: f32,
}

/// Looks a `--variant` name up and applies its overrides in place, leaving
/// every other option as the caller left it.
pub fn apply_variant(name: &str, options: &mut IslandOptions) -> Result<(), String> {
    let (_, overrides) = VARIANTS
        .iter()
        .find(|(variant, _)| *variant == name)
        .ok_or_else(|| format!("unknown variant {name:?}; expected one of {}", variant_names()))?;
    if let Some(variant) = overrides {
        options.hydraulic_erosion_strength = variant.hydraulic_erosion_strength;
        options.coastal_slope_multiplier = variant.coastal_slope_multiplier;
    }
    Ok(())
}

/// The variant names in table order, for help text and parse errors.
#[must_use]
pub fn variant_names() -> String {
    VARIANTS.map(|(name, _)| name).join(", ")
}

/// Parameters the next island generates from. Set from the command line before
/// the app starts and replaced by every rebuild the HUD asks for.
#[derive(Resource, Clone, Copy, Debug)]
pub struct GenerationSettings {
    pub seed: u64,
    pub options: IslandOptions,
    /// False under `--no-cache`, for the session rather than for one run. A
    /// fresh entry is still written, so the next ordinary run finds one.
    pub cache_reads: bool,
}

/// What the generator is doing, which is what the HUD reports and what keeps
/// two runs from overlapping.
#[derive(Resource, Default)]
pub struct GenerationStatus {
    /// Seconds the run in flight has been going, or `None` between runs.
    pub elapsed: Option<f32>,
    /// The inputs the island on screen was built from, once one exists.
    pub built: Option<(u64, IslandOptions)>,
    /// How long that build took.
    pub took: Option<f32>,
    /// The last failure, kept until a run succeeds. Only ever set once an
    /// island is already on screen; a first generation that fails is fatal.
    pub failure: Option<String>,
}

impl GenerationStatus {
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.elapsed.is_some()
    }
}

/// Asks for a rebuild from these inputs. Ignored while a run is in flight, so
/// the island on screen is never torn down for a build that cannot start.
#[derive(Message, Clone, Copy, Debug)]
pub struct Regenerate {
    pub seed: u64,
    pub options: IslandOptions,
}

/// A new island has just landed in [`GeneratedIsland`]. Renderer plugins spawn
/// their geometry on this rather than on the resource being added, which only
/// ever happens on the first island.
#[derive(Message)]
pub struct IslandReady;

/// Tags every entity spawned from a generated island, so a rebuild clears the
/// whole set in one pass. The sea plane carries no tag: it is the same plane
/// under every island.
#[derive(Component)]
pub struct IslandEntity;

/// Everything the renderer reads off a generated island, and nothing else.
///
/// The generator's `Island` is dropped as soon as this is built, which is what
/// lets the same data come off disk as easily as out of a generation run: these
/// are plain `motu` values with public fields, where `Island` is opaque and its
/// own save file only stores the seed to regenerate from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IslandData {
    /// The options the island was generated with. The shaders read scale off
    /// them; nothing regenerates from them.
    pub options: IslandOptions,
    /// The LOD 0 terrain surface, and the generator's material triple for one
    /// of its vertices each: bedrock hardness, loose cover, sea proximity.
    pub terrain: Mesh,
    pub materials: Vec<Vec3>,
    pub river_mesh: Mesh,
    pub river_rock_mesh: Mesh,
    /// Decoration points, `(u, v, height)` in normalized island space.
    pub trees: Vec<Vec3>,
    pub bushes: Vec<Vec3>,
    /// Normalized ground elevation on a [`HEIGHT_GRID`] square lattice, row by
    /// row from `v == 0`. Nothing renders it; walk mode stands on it.
    pub heights: Vec<f32>,
    /// Channel count. Nothing renders it; the ready line reports it.
    pub rivers: u32,
}

impl IslandData {
    /// Built on the generation task, which is also what moves the generator's
    /// lazy decoration pass off the main thread.
    fn new(island: &Island) -> Self {
        let terrain = island.lod(0).cloned().unwrap_or_default();
        let decorations = island.decorations();
        Self {
            options: island.options(),
            materials: island.material_values_for(&terrain),
            terrain,
            river_mesh: island.river_mesh().clone(),
            river_rock_mesh: island.river_rock_mesh().clone(),
            trees: decorations.trees().to_vec(),
            bushes: decorations.bushes().to_vec(),
            heights: island.height_map(HEIGHT_GRID, HEIGHT_GRID),
            rivers: u32::try_from(island.rivers().len()).unwrap_or(u32::MAX),
        }
    }

    /// Ground height in metres under a world-space XZ position, bilinear over
    /// the stored grid.
    ///
    /// Positions off the island square clamp to its edge rather than falling
    /// away, and a grid of any other size — which only an entry written under
    /// another [`HEIGHT_GRID`] could carry — reads as sea level throughout.
    // Every cast below runs over a value already clamped into the grid, so
    // none of them can truncate, lose a sign or lose a digit that matters.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    #[must_use]
    pub fn ground_height(&self, x: f32, z: f32) -> f32 {
        let span = HEIGHT_GRID as usize;
        if self.heights.len() != span * span {
            return 0.0;
        }
        let last = (HEIGHT_GRID - 1) as f32;
        let u = (x / ISLAND_WORLD_METRES + 0.5).clamp(0.0, 1.0) * last;
        let v = (z / ISLAND_WORLD_METRES + 0.5).clamp(0.0, 1.0) * last;
        let (along, down) = (u.fract(), v.fract());
        let (column, row) = (u as usize, v as usize);
        let (right, below) = ((column + 1).min(span - 1), (row + 1).min(span - 1));
        let sample = |row: usize, column: usize| self.heights[row * span + column];
        let near = sample(row, column).lerp(sample(row, right), along);
        let far = sample(below, column).lerp(sample(below, right), along);
        near.lerp(far, down) * ISLAND_WORLD_METRES
    }
}

/// The finished island. Replaced whole on every rebuild.
#[derive(Resource)]
pub struct GeneratedIsland(pub IslandData);

#[derive(Component)]
struct GenerationTask(Task<Result<IslandData, String>>);

#[derive(Component)]
struct LoadingNotice;

/// What an arriving island replaces: the notice that stood in for the first
/// one, and everything the island before it spawned.
type Replaced = Or<(With<LoadingNotice>, With<IslandEntity>)>;

pub struct IslandGenPlugin;

impl Plugin for IslandGenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GenerationStatus>()
            .add_message::<Regenerate>()
            .add_message::<IslandReady>()
            .add_systems(Startup, (start_first_generation, spawn_loading_notice))
            .add_systems(
                PreUpdate,
                (track_elapsed, poll_generation, accept_requests).chain(),
            );
    }
}

fn start_first_generation(
    mut commands: Commands,
    settings: Res<GenerationSettings>,
    mut status: ResMut<GenerationStatus>,
) {
    start(&mut commands, *settings, &mut status);
}

/// Puts one generation on the async pool. The caller has already established
/// that nothing else is running.
fn start(commands: &mut Commands, settings: GenerationSettings, status: &mut GenerationStatus) {
    let GenerationSettings {
        seed,
        options,
        cache_reads,
    } = settings;
    info!(
        "generating island: seed {seed}, terrain size {}",
        options.terrain_size
    );
    // The cache read runs on the task as well: an entry is tens of megabytes,
    // and no frame should wait on that any more than on generation.
    let task =
        AsyncComputeTaskPool::get().spawn(async move { island_data(seed, options, cache_reads) });
    commands.spawn((Name::new("Island generation"), GenerationTask(task)));
    status.elapsed = Some(0.0);
}

/// Takes the most recent rebuild request. A request that arrives while the
/// generator is busy is dropped rather than queued: the HUD disables its button
/// for exactly that time, and a stale queue would build an island nobody asked
/// for last.
fn accept_requests(
    mut commands: Commands,
    mut requests: MessageReader<Regenerate>,
    mut settings: ResMut<GenerationSettings>,
    mut status: ResMut<GenerationStatus>,
) {
    let Some(request) = requests.read().last() else {
        return;
    };
    if status.is_running() {
        warn!("ignoring a rebuild request: one island is already generating");
        return;
    }
    settings.seed = request.seed;
    settings.options = request.options;
    let requested = *settings;
    start(&mut commands, requested, &mut status);
}

/// Reads the cached geometry if there is any, and otherwise generates it and
/// leaves an entry behind. A cache that cannot be read or written only ever
/// costs time, so nothing here is fatal but a failed generation.
fn island_data(seed: u64, options: IslandOptions, cache_reads: bool) -> Result<IslandData, String> {
    let path = cache::path(cache::key(seed, &options));
    if !cache_reads {
        info!("island cache bypassed: --no-cache");
    } else if let Some(data) = cache::read(&path, seed, &options) {
        info!("island cache hit: {}", path.display());
        return Ok(data);
    } else {
        info!("island cache miss: {}", path.display());
    }
    let data = IslandData::new(&Island::generate(seed, options)?);
    match cache::write(&path, seed, &data) {
        Ok(()) => info!("island cache written: {}", path.display()),
        Err(error) => warn!(
            "could not write island cache to {}: {error}",
            path.display()
        ),
    }
    Ok(data)
}

/// Only the first generation has one: every later build keeps the island it
/// replaces on screen, and the HUD is what reports that one is running.
fn spawn_loading_notice(mut commands: Commands) {
    commands.spawn((
        Name::new("Loading notice"),
        LoadingNotice,
        Text::new("Generating island..."),
        TextFont::from_font_size(26.0),
        TextColor(Color::WHITE),
        // The notice sits over the atmosphere's brightest band, near the
        // horizon, which white on its own does not clear.
        TextShadow::default(),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(24.0),
            top: Val::Px(24.0),
            ..default()
        },
    ));
}

fn track_elapsed(time: Res<Time>, mut status: ResMut<GenerationStatus>) {
    if let Some(elapsed) = status.elapsed.as_mut() {
        *elapsed += time.delta_secs();
    }
}

/// Generation runs in `PreUpdate` so the swap is flushed before the renderer
/// plugins read `IslandReady` in `Update`. Clearing what the arriving island
/// replaces and installing it happen in one command queue, and the plugins
/// spawn from it later the same frame, so no frame is rendered without an
/// island.
fn poll_generation(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut GenerationTask)>,
    replaced: Query<Entity, Replaced>,
    settings: Res<GenerationSettings>,
    mut status: ResMut<GenerationStatus>,
    mut ready: MessageWriter<IslandReady>,
    mut exit: MessageWriter<AppExit>,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = block_on(poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let elapsed = status.elapsed.take();
        match result {
            Ok(data) => {
                info!(
                    "island ready: {} terrain vertices, {} rivers, {} trees, {} bushes",
                    data.terrain.vertices.len(),
                    data.rivers,
                    data.trees.len(),
                    data.bushes.len()
                );
                info!(
                    "island arguments: {}",
                    options::command_line(settings.seed, &settings.options)
                );
                for entity in &replaced {
                    commands.entity(entity).despawn();
                }
                commands.insert_resource(GeneratedIsland(data));
                ready.write(IslandReady);
                status.built = Some((settings.seed, settings.options));
                status.took = elapsed;
                status.failure = None;
            }
            Err(error) => {
                error!("island generation failed: {error}");
                // Nothing to fall back to on the first island, and a viewer
                // with no island in it is not worth leaving open.
                if status.built.is_none() {
                    exit.write(AppExit::error());
                } else {
                    status.failure = Some(error);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use motu::ISLAND_WORLD_METRES;

    use super::{HEIGHT_GRID, IslandData};

    /// Ground rising evenly from west to east, from sea level at the square's
    /// west edge to 200 m at its east one. Every sample then has a height the
    /// test can predict, and interpolation between two of them lands on the
    /// same line rather than only near it.
    fn ramp() -> IslandData {
        let span = HEIGHT_GRID as usize;
        #[allow(clippy::cast_precision_loss)]
        let last = (HEIGHT_GRID - 1) as f32;
        IslandData {
            heights: (0..span * span)
                .map(|index| {
                    #[allow(clippy::cast_precision_loss)]
                    let column = (index % span) as f32;
                    0.1 * column / last
                })
                .collect(),
            ..IslandData::default()
        }
    }

    fn height_at(island: &IslandData, x: f32) -> f32 {
        island.ground_height(x, 0.0)
    }

    #[test]
    fn samples_the_ground_between_grid_points() {
        let island = ramp();
        let half = ISLAND_WORLD_METRES * 0.5;
        assert!(height_at(&island, -half).abs() < 0.01, "the west edge");
        assert!((height_at(&island, half) - 200.0).abs() < 0.01, "the east");
        // Halfway is between two samples, so this only lands if the sample is
        // interpolated rather than snapped to the nearer of them.
        assert!((height_at(&island, 0.0) - 100.0).abs() < 0.01, "the middle");
        assert!((height_at(&island, -half * 0.5) - 50.0).abs() < 0.5);
    }

    /// Walking off the terrain square holds the edge rather than falling away
    /// under it.
    #[test]
    fn off_the_square_holds_the_edge() {
        let island = ramp();
        let far = ISLAND_WORLD_METRES * 10.0;
        assert!(height_at(&island, -far).abs() < 0.01);
        assert!((height_at(&island, far) - 200.0).abs() < 0.01);
        assert!((island.ground_height(0.0, -far) - 100.0).abs() < 0.01);
    }

    /// Only an entry written under another `HEIGHT_GRID` could carry one, and
    /// the format version retires those, but a grid that is not the expected
    /// square reads as sea level rather than indexing off the end of it.
    #[test]
    fn a_grid_of_another_size_reads_as_sea_level() {
        let island = IslandData {
            heights: vec![0.25; 9],
            ..IslandData::default()
        };
        assert!(island.ground_height(0.0, 0.0).abs() < f32::EPSILON);
        assert!(IslandData::default().ground_height(0.0, 0.0).abs() < f32::EPSILON);
    }
}
