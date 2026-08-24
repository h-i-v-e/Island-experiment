//! Off-thread island generation and the handoff resource every renderer reads.

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use motu::{Island, IslandOptions, Mesh, Vec3};

use crate::cache;

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

/// Parameters the generator runs with, inserted before the app starts.
#[derive(Resource, Clone, Copy, Debug)]
pub struct GenerationSettings {
    pub seed: u64,
    pub options: IslandOptions,
    /// False under `--no-cache`. A fresh entry is still written, so the next
    /// ordinary run finds one.
    pub cache_reads: bool,
}

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
            rivers: u32::try_from(island.rivers().len()).unwrap_or(u32::MAX),
        }
    }
}

/// The finished island. Renderer plugins spawn their geometry on the frame this
/// resource is added.
#[derive(Resource)]
pub struct GeneratedIsland(pub IslandData);

#[derive(Component)]
struct GenerationTask(Task<Result<IslandData, String>>);

#[derive(Component)]
struct LoadingNotice;

pub struct IslandGenPlugin;

impl Plugin for IslandGenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (start_generation, spawn_loading_notice))
            .add_systems(PreUpdate, poll_generation);
    }
}

fn start_generation(mut commands: Commands, settings: Res<GenerationSettings>) {
    let seed = settings.seed;
    let options = settings.options;
    let cache_reads = settings.cache_reads;
    info!(
        "generating island: seed {seed}, terrain size {}",
        options.terrain_size
    );
    // The cache read runs on the task as well: an entry is tens of megabytes,
    // and the first frame should not wait on that any more than on generation.
    let task =
        AsyncComputeTaskPool::get().spawn(async move { island_data(seed, options, cache_reads) });
    commands.spawn((Name::new("Island generation"), GenerationTask(task)));
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

/// Generation runs in `PreUpdate` so the resource insertion is flushed before
/// the renderer plugins look for it in `Update`.
fn poll_generation(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut GenerationTask)>,
    notices: Query<Entity, With<LoadingNotice>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = block_on(poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).despawn();
        for notice in &notices {
            commands.entity(notice).despawn();
        }
        match result {
            Ok(data) => {
                info!(
                    "island ready: {} terrain vertices, {} rivers, {} trees, {} bushes",
                    data.terrain.vertices.len(),
                    data.rivers,
                    data.trees.len(),
                    data.bushes.len()
                );
                commands.insert_resource(GeneratedIsland(data));
            }
            Err(error) => {
                error!("island generation failed: {error}");
                exit.write(AppExit::error());
            }
        }
    }
}
