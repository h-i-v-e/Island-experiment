//! Off-thread island generation and the handoff resource every renderer reads.

use std::time::Instant;

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use motu::{
    GenerationMethod, ISLAND_WORLD_METRES, Island, IslandOptions, Mesh, River, RiverNode, Vec2,
    Vec3,
};

use crate::{
    cache,
    chunk::{self, ChunkTier, TerrainChunk},
    convert::{self, island_to_world},
    math::smoothstep,
    options,
};

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

/// How far from a river's own water edge the ground still reads as damp, and
/// how far above the water beside it.
///
/// Twelve metres is about as wide as the flood plain the generator carves; three
/// metres clears its deepest channel, so a bank is damp to its lip and no
/// further and a terrace standing over one is dry.
const RIVER_WET_METRES: f32 = 12.0;
const RIVER_WET_RISE_METRES: f32 = 3.0;

/// What counts as a drop: the least the water surface may fall over one run of
/// steep segments, and the least grade a segment of that run may have.
///
/// The generator calls any segment steeper than a grade of 0.04 a waterfall,
/// which on a mountain channel is most of it. What the water surface has to
/// pick out is narrower than that: the handful of faces the water actually
/// leaves its bed on. Both thresholds therefore stand well above the
/// generator's, and a run is only kept once its segments together clear the
/// height — a fall carved as four short steps is one drop, not four.
const DROP_METRES: f32 = 0.75;
const DROP_GRADE: f32 = 0.5;
/// The least one segment of a run may fall. Below this the surface has not
/// stepped at all: the generator's own water-surface offset is two centimetres.
const DROP_SEGMENT_METRES: f32 = 0.10;
/// The fall height a drop's strength is measured against. The tallest fall on
/// the reference island is about twelve metres, and everything a drop scales —
/// plunge foam, spray, wet rock — is already at full weight well under that.
const DROP_REFERENCE_METRES: f32 = 8.0;
/// Metres upstream of a lip the water is already drawing down towards it,
/// metres downstream of a foot the plunge is still turning over, and metres
/// around a fall that its spray keeps wet.
const DROP_APPROACH_METRES: f32 = 6.0;
const DROP_PLUNGE_METRES: f32 = 14.0;
const DROP_SPRAY_METRES: f32 = 7.0;
/// How the plunge reaches: tighter across the channel than along it, and
/// tighter again upstream of the foot, because what the fall throws forward is
/// what has to still be turning over downstream.
const DROP_PLUNGE_LATERAL: f32 = 1.7;
const DROP_PLUNGE_UPSTREAM: f32 = 2.5;
/// How far past the channel's own half width a falling sheet reaches, the run
/// its edge fades out over, and the narrowest sheet a drop can carry.
const DROP_FACE_WIDTH: f32 = 1.15;
const DROP_FACE_FADE: f32 = 1.55;
const DROP_FACE_MINIMUM_METRES: f32 = 0.9;
/// What [`DropField::fall`] carries at a lip. Off a face it is exactly zero, so
/// the floor is what separates the top of a sheet from no sheet at all.
/// `river.wgsl` spells the same number and the two are only correct together.
const DROP_FALL_FLOOR: f32 = 0.2;

/// The option overrides one named variant applies.
#[derive(Clone, Copy, Debug)]
struct Variant {
    hydraulic_erosion_strength: f32,
    coastal_slope_multiplier: f32,
}

/// Replaces the fields owned by a named variant, leaving every other option as
/// the caller left it. Resetting the owned fields explicitly matters even when
/// a variant writes the same value as `base`: a later variant must still undo
/// that write according to command-line order.
pub fn replace_variant(
    name: &str,
    base: &IslandOptions,
    options: &mut IslandOptions,
) -> Result<(), String> {
    let overrides = VARIANTS
        .iter()
        .find(|(variant, _)| *variant == name)
        .map(|(_, overrides)| *overrides)
        .ok_or_else(|| {
            format!(
                "unknown variant {name:?}; expected one of {}",
                variant_names()
            )
        })?;

    options.hydraulic_erosion_strength = base.hydraulic_erosion_strength;
    options.coastal_slope_multiplier = base.coastal_slope_multiplier;
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
    pub method: GenerationMethod,
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
    pub built: Option<Regenerate>,
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
    pub method: GenerationMethod,
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
    /// The terrain surface, cut into a fixed grid of chunks and carried at
    /// every level of detail the generator publishes. Each chunk vertex has the
    /// generator's material triple — bedrock hardness, loose cover, sea
    /// proximity — and the renderer's own proximity to running water beside it:
    /// 1 at a channel's own water edge, 0 at [`RIVER_WET_METRES`] out from it or
    /// [`RIVER_WET_RISE_METRES`] above it, and never less than what a nearby
    /// fall's spray leaves. The wetness is measured here because the generator
    /// publishes channels, not a distance to them.
    pub terrain_chunks: Vec<TerrainChunk>,
    pub river_mesh: Mesh,
    pub river_rock_mesh: Mesh,
    /// Every fall the channels carry. The water surface, the rock beside it and
    /// the spray over it are all placed from these.
    pub river_drops: Vec<RiverDrop>,
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
    /// lazy decoration pass and the terrain slicing off the main thread.
    fn new(island: &Island) -> (Self, DropIndex) {
        let decorations = island.decorations();
        let started = Instant::now();
        let river_drops = river_drops(island);
        let drops = DropIndex::new(&river_drops);
        let banks = WetBanks::new(island);
        let terrain_chunks = terrain_chunks(island, &banks, &drops);
        let tallest = river_drops
            .iter()
            .map(|drop| drop.metres())
            .fold(0.0, f32::max);
        let vertices: usize = terrain_chunks.iter().map(TerrainChunk::vertices).sum();
        info!(
            "terrain chunks: {} of {} levels, {vertices} vertices in all, \
             measured against {} above-sea channel segments and {} drops \
             (tallest {tallest:.1} m) in {:.0} ms",
            terrain_chunks.len(),
            chunk::TIERS,
            banks.segments.len(),
            river_drops.len(),
            started.elapsed().as_secs_f32() * 1_000.0,
        );
        let data = Self {
            options: island.options(),
            terrain_chunks,
            river_mesh: island.river_mesh().clone(),
            river_rock_mesh: island.river_rock_mesh().clone(),
            river_drops,
            trees: decorations.trees().to_vec(),
            bushes: decorations.bushes().to_vec(),
            heights: island.height_map(HEIGHT_GRID, HEIGHT_GRID),
            rivers: u32::try_from(island.rivers().len()).unwrap_or(u32::MAX),
        };
        (data, drops)
    }

    /// Vertices across every chunk at one level of detail, which is what the
    /// renderer draws when the whole island stands in that level's range.
    #[must_use]
    pub fn tier_vertices(&self, level: usize) -> usize {
        self.terrain_chunks
            .iter()
            .filter_map(|chunk| chunk.tiers.get(level))
            .map(|tier| tier.mesh.vertices.len())
            .sum()
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

/// The terrain grid, sliced and measured once on the generation task.
///
/// The two per-vertex fields are taken on the chunk rather than on the whole
/// island and then cut up, and answer the same either way: the material triple
/// is sampled through each vertex's own UV, which the slicer interpolates along
/// with the position, and the wetness is a function of the vertex position
/// alone. A vertex the slicer left where it found it therefore carries exactly
/// what it carried before the cut, and one the slicer created on a chunk edge
/// carries what the surface there always said.
///
/// The skirt is hung after both are measured and copies them from the vertex it
/// hangs from, so an apron shades as the ground above it does rather than as
/// ground twenty-four metres lower — which, beside a channel, would read as wet.
fn terrain_chunks(island: &Island, banks: &WetBanks, drops: &DropIndex) -> Vec<TerrainChunk> {
    let depth = chunk::skirt_depth();
    let mut levels: Vec<Vec<Mesh>> = (0..chunk::TIERS)
        .map(|level| island.lod(level).map_or_else(Vec::new, chunk::sliced))
        .collect();
    let mut chunks = Vec::with_capacity((chunk::DIVISIONS * chunk::DIVISIONS) as usize);
    for row in 0..chunk::DIVISIONS {
        for column in 0..chunk::DIVISIONS {
            let index = (row * chunk::DIVISIONS + column) as usize;
            let bounds = chunk::bounds(column, row);
            let sides = chunk::interior_sides(column, row);
            let mut tiers = Vec::with_capacity(chunk::TIERS);
            let (mut surface_low, mut surface_high) = (f32::MAX, f32::MIN);
            for level in &mut levels {
                let mut mesh = level.get_mut(index).map(std::mem::take).unwrap_or_default();
                for vertex in &mesh.vertices {
                    surface_low = surface_low.min(vertex.z);
                    surface_high = surface_high.max(vertex.z);
                }
                let mut materials = island.material_values_for(&mesh);
                let mut river_wetness = banks.measure(&mesh, drops);
                for source in chunk::skirt(&mut mesh, bounds, depth, sides) {
                    let source = source as usize;
                    materials.push(materials[source]);
                    river_wetness.push(river_wetness[source]);
                }
                tiers.push(ChunkTier {
                    mesh,
                    materials,
                    river_wetness,
                });
            }
            chunks.push(TerrainChunk {
                column,
                row,
                surface_low,
                surface_high,
                tiers: tiers
                    .try_into()
                    .expect("one tier is built per level of detail"),
            });
        }
    }
    chunks
}

/// One fall on one channel, in the generator's normalized island space: x and y
/// across the square, z the water surface.
///
/// Drops are derived from the river node profile rather than from
/// `Island::river_emitters`. The emitter pass finds every sharp crease on the
/// river mesh, which on this generator is as often angular channel topology as
/// it is a lip, and a crease cannot say which side of a fall it is on. A node
/// profile can: the nodes carry the water surface along the whole reach, so a
/// run of segments that falls far enough and steeply enough is one fall, its
/// first node is the lip and its last the foot. The same profile hands over the
/// fall's height and the channel's own width there, which is what scales
/// everything the renderer builds on top of it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RiverDrop {
    /// The last node the water is still in its bed at, and the first one it is
    /// back in a bed at.
    pub lip: Vec3,
    pub foot: Vec3,
    /// The channel's heading at the foot, unit length. Taken from the reach
    /// past the foot rather than from the two ends, which a vertical fall
    /// leaves standing on top of each other.
    pub direction: Vec2,
    /// The generator's own channel half width at the foot.
    pub half_width: f32,
}

impl RiverDrop {
    /// Metres the water falls.
    #[must_use]
    pub fn metres(self) -> f32 {
        (self.lip.z - self.foot.z).max(0.0) * ISLAND_WORLD_METRES
    }

    /// How hard this fall hits, from nothing to one.
    #[must_use]
    pub fn strength(self) -> f32 {
        (self.metres() / DROP_REFERENCE_METRES).clamp(0.0, 1.0)
    }

    /// The half extent of everything this drop reaches, which is what the
    /// lattice registers it over.
    fn reach(self) -> f32 {
        normalized(
            DROP_APPROACH_METRES
                .max(DROP_PLUNGE_METRES)
                .max(DROP_SPRAY_METRES * (0.55 + self.strength())),
        ) + self.half_width * DROP_FACE_FADE
    }
}

/// What one water-surface vertex knows about the nearest drop. Each channel
/// rides in one component of the river mesh's colour attribute, which the
/// generator leaves free.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DropField {
    /// 1 at a lip, 0 at [`DROP_APPROACH_METRES`] upstream of one.
    pub approach: f32,
    /// 0 off a falling face, [`DROP_FALL_FLOOR`] at its lip and 1 at its foot,
    /// so one channel carries both whether the water is falling and how far it
    /// has already fallen.
    pub fall: f32,
    /// 1 at a foot, 0 at [`DROP_PLUNGE_METRES`] downstream of one.
    pub plunge: f32,
    /// The height of whichever drop the three channels above came from,
    /// against [`DROP_REFERENCE_METRES`].
    pub strength: f32,
}

impl DropField {
    /// The four channels in the order the colour attribute carries them.
    #[must_use]
    pub const fn to_array(self) -> [f32; 4] {
        [self.approach, self.fall, self.plunge, self.strength]
    }
}

/// Every drop on one island, on a uniform lattice over the island square.
///
/// The same arrangement the bank wetness uses, and for the same reason: a
/// vertex reads one cell instead of the whole list, which is what keeps a pass
/// over a two-million-vertex terrain to milliseconds without a thread pool.
pub struct DropIndex {
    drops: Lattice<RiverDrop>,
}

/// Values registered over the uniform cells their reach intersects.
///
/// Both river drops and wet bank segments use the same index shape; only the
/// value and the function that reports its bounds differ.
struct Lattice<T> {
    values: Vec<T>,
    cells: Vec<Vec<u32>>,
    span: usize,
}

impl<T> Lattice<T> {
    fn over(values: Vec<T>, cell_metres: f32, bounds: impl Fn(&T) -> (Vec2, Vec2)) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let span = (1.0 / normalized(cell_metres)).ceil().max(1.0) as usize;
        let mut cells = vec![Vec::new(); span * span];
        for (index, value) in values.iter().enumerate() {
            let (low, high) = bounds(value);
            let (left, top) = (cell_index(low.x, span), cell_index(low.y, span));
            let (right, bottom) = (cell_index(high.x, span), cell_index(high.y, span));
            for row in top..=bottom {
                for column in left..=right {
                    #[allow(clippy::cast_possible_truncation)]
                    cells[row * span + column].push(index as u32);
                }
            }
        }
        Self {
            values,
            cells,
            span,
        }
    }

    fn nearby(&self, point: Vec2) -> impl Iterator<Item = &T> {
        let row = cell_index(point.y, self.span);
        let column = cell_index(point.x, self.span);
        self.cells[row * self.span + column]
            .iter()
            .map(|&index| &self.values[index as usize])
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn len(&self) -> usize {
        self.values.len()
    }
}

impl DropIndex {
    #[must_use]
    pub fn new(drops: &[RiverDrop]) -> Self {
        Self {
            drops: Lattice::over(drops.to_vec(), DROP_PLUNGE_METRES, |drop| {
                let reach = drop.reach();
                (
                    drop.lip.truncate().min(drop.foot.truncate()) - reach,
                    drop.lip.truncate().max(drop.foot.truncate()) + reach,
                )
            }),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.drops.is_empty()
    }

    /// The drops that can reach a normalized island-space point.
    fn nearby(&self, point: Vec2) -> impl Iterator<Item = RiverDrop> + '_ {
        self.drops.nearby(point).copied()
    }

    /// What a water-surface vertex carries. The three proximities are taken
    /// separately, because a short reach between two falls stands downstream of
    /// one and upstream of the next at the same time; the strength comes from
    /// whichever drop claimed the vertex hardest, so a metre-high step beside a
    /// ten-metre fall cannot quieten it.
    #[must_use]
    pub fn field(&self, vertex: Vec3) -> DropField {
        let mut field = DropField::default();
        let mut claimed = 0.0_f32;
        for drop in self.nearby(vertex.truncate()) {
            let approach = drop.approach_at(vertex);
            let fall = drop.fall_at(vertex);
            let plunge = drop.plunge_at(vertex);
            field.approach = field.approach.max(approach);
            field.fall = field.fall.max(fall);
            field.plunge = field.plunge.max(plunge);
            let claim = approach.max(fall).max(plunge);
            if claim > claimed {
                claimed = claim;
                field.strength = drop.strength();
            }
        }
        field
    }

    /// How wet a solid surface — bank or boulder — stands from the spray of the
    /// nearest fall.
    #[must_use]
    pub fn spray(&self, vertex: Vec3) -> f32 {
        self.nearby(vertex.truncate())
            .map(|drop| drop.spray_at(vertex))
            .fold(0.0, f32::max)
    }
}

impl RiverDrop {
    /// Metres from the fall's own axis, which is the segment between the lip
    /// and the foot seen from above. A vertical fall leaves that segment a
    /// point, and the distance is then simply the distance to it.
    fn axis_metres(self, point: Vec2) -> f32 {
        segment_distance(point, self.lip.truncate(), self.foot.truncate()) * ISLAND_WORLD_METRES
    }

    /// The water drawing down towards the lip: a disc upstream of it, gated to
    /// the surface standing at or above the lip's own. The channel is the only
    /// thing the river mesh covers, so the disc needs no width of its own.
    fn approach_at(self, vertex: Vec3) -> f32 {
        let above = (vertex.z - self.lip.z) * ISLAND_WORLD_METRES;
        let distance = self.lip.truncate().distance(vertex.truncate()) * ISLAND_WORLD_METRES;
        smoothstep(-0.25, 0.05, above) * (1.0 - (distance / DROP_APPROACH_METRES).min(1.0))
    }

    /// The falling face: between the two water surfaces, inside the channel's
    /// own width, carrying how far down the face the point stands.
    fn fall_at(self, vertex: Vec3) -> f32 {
        let height = self.metres();
        if height <= 0.0 {
            return 0.0;
        }
        let above_foot = (vertex.z - self.foot.z) * ISLAND_WORLD_METRES;
        let below_lip = (self.lip.z - vertex.z) * ISLAND_WORLD_METRES;
        let vertical = smoothstep(-0.05, 0.35, above_foot) * smoothstep(-0.05, 0.35, below_lip);
        if vertical <= 0.0 {
            return 0.0;
        }
        let radius =
            (self.half_width * ISLAND_WORLD_METRES * DROP_FACE_WIDTH).max(DROP_FACE_MINIMUM_METRES);
        let across = 1.0
            - smoothstep(
                radius,
                radius * DROP_FACE_FADE,
                self.axis_metres(vertex.truncate()),
            );
        let descent = (below_lip / height).clamp(0.0, 1.0);
        across * vertical * (DROP_FALL_FLOOR + (1.0 - DROP_FALL_FLOOR) * descent)
    }

    /// The receiving water: an ellipse around the foot, drawn out downstream
    /// and cut short upstream, gated to the surface standing at or below the
    /// foot's own.
    fn plunge_at(self, vertex: Vec3) -> f32 {
        let above = (vertex.z - self.foot.z) * ISLAND_WORLD_METRES;
        let offset = (vertex.truncate() - self.foot.truncate()) * ISLAND_WORLD_METRES;
        let along = offset.dot(self.direction);
        let lateral = offset.perp_dot(self.direction) * DROP_PLUNGE_LATERAL;
        let along = if along >= 0.0 {
            along
        } else {
            along * DROP_PLUNGE_UPSTREAM
        };
        let distance = along.hypot(lateral);
        (1.0 - smoothstep(0.05, 0.60, above)) * (1.0 - (distance / DROP_PLUNGE_METRES).min(1.0))
    }

    /// What the fall throws onto the ground and stone around it: strongest at
    /// the foot, reaching up the face and a little past the lip, and gone a
    /// couple of metres under the receiving water, which the water itself is
    /// already answering for.
    fn spray_at(self, vertex: Vec3) -> f32 {
        let strength = self.strength();
        let reach = DROP_SPRAY_METRES * (0.55 + strength);
        let near = 1.0 - (self.axis_metres(vertex.truncate()) / reach).min(1.0);
        if near <= 0.0 {
            return 0.0;
        }
        let height = self.metres();
        let rise = (vertex.z - self.foot.z) * ISLAND_WORLD_METRES;
        let band =
            smoothstep(-2.5, -0.5, rise) * (1.0 - smoothstep(height + 0.5, height + 3.5, rise));
        near * band * (0.55 + 0.45 * strength)
    }
}

/// Metres from a point to a segment, all in normalized island space.
fn segment_distance(point: Vec2, from: Vec2, to: Vec2) -> f32 {
    closest_on_segment(point, from, to).0.distance(point)
}

/// The closest point on a segment and its parameter from `from` to `to`.
fn closest_on_segment(point: Vec2, from: Vec2, to: Vec2) -> (Vec2, f32) {
    let along = to - from;
    let length = along.length_squared();
    let travelled = if length > f32::EPSILON {
        ((point - from).dot(along) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (from + along * travelled, travelled)
}

/// Every fall on every channel of one island.
///
/// A run of consecutive steep segments is one drop rather than several: the
/// generator cuts a face into node-length steps, and a fall read as four
/// separate metre-high drops would carry four lips, four feet and four plunges
/// stacked inside one sheet.
fn river_drops(island: &Island) -> Vec<RiverDrop> {
    let options = island.options();
    drops_of(
        island.rivers(),
        normalized(options.river_source_width_metres),
        normalized(options.river_maximum_width_metres),
    )
}

/// [`river_drops`] over the channels alone, which is what the tests can build.
fn drops_of(rivers: &[River], source: f32, maximum: f32) -> Vec<RiverDrop> {
    let mut drops = Vec::new();
    for river in rivers {
        let widths = river.target_half_widths(source, maximum);
        let nodes = &river.nodes;
        let mut segment = 0;
        while segment + 1 < nodes.len() {
            if !steps_down(nodes[segment], nodes[segment + 1]) {
                segment += 1;
                continue;
            }
            let lip = segment;
            while segment + 1 < nodes.len() && steps_down(nodes[segment], nodes[segment + 1]) {
                segment += 1;
            }
            let drop = RiverDrop {
                lip: surface_point(nodes[lip]),
                foot: surface_point(nodes[segment]),
                direction: heading(nodes, lip, segment),
                half_width: widths[segment],
            };
            // The lip has to stand above the sea, because a step between two
            // reaches that have already reached it is the coastline rather
            // than a fall. Where the lip is above and the foot is not, the
            // channel is dropping into the sea, and that is a fall like any
            // other: the sheet and its spray are real, and only the plunge is
            // missing, because the water it lands in is not this surface.
            if drop.metres() >= DROP_METRES && drop.lip.z * ISLAND_WORLD_METRES > DROP_METRES * 0.5
            {
                drops.push(drop);
            }
        }
    }
    drops
}

/// Whether the water surface steps down between two nodes rather than running
/// down between them.
fn steps_down(from: RiverNode, to: RiverNode) -> bool {
    let fall = (from.surface - to.surface) * ISLAND_WORLD_METRES;
    let run = from.position.truncate().distance(to.position.truncate()) * ISLAND_WORLD_METRES;
    fall >= DROP_SEGMENT_METRES && fall >= run * DROP_GRADE
}

/// One node as the point its water surface stands at.
fn surface_point(node: RiverNode) -> Vec3 {
    Vec3::new(node.position.x, node.position.y, node.surface)
}

/// Which way the channel runs at a fall's foot. The reach past the foot answers
/// first, because the lip and the foot of a vertical fall stand on top of each
/// other and the segment between them has no heading at all.
fn heading(nodes: &[RiverNode], lip: usize, foot: usize) -> Vec2 {
    let foot_point = nodes[foot].position.truncate();
    if let Some(next) = nodes.get(foot + 1)
        && let Some(direction) = (next.position.truncate() - foot_point).try_normalize()
    {
        return direction;
    }
    (foot_point - nodes[lip].position.truncate())
        .try_normalize()
        .unwrap_or(Vec2::X)
}

/// One stretch of above-sea river as the wetness pass reads it: the two ends of
/// a centreline segment in normalized island XY, the water surface at each, and
/// the channel's own half width there.
#[derive(Clone, Copy)]
struct WetSegment {
    from: Vec2,
    to: Vec2,
    from_surface: f32,
    to_surface: f32,
    from_half_width: f32,
    to_half_width: f32,
}

impl WetSegment {
    /// How wet this one segment leaves a terrain vertex. Distance runs from the
    /// water's edge rather than from the centreline, so a wide channel wets its
    /// banks no harder than a narrow one does, and a vertex standing well over
    /// the water beside it is dry however close it is horizontally.
    fn wetness(&self, vertex: Vec3) -> f32 {
        let point = vertex.truncate();
        let (closest, travelled) = closest_on_segment(point, self.from, self.to);
        let half_width = self.from_half_width.lerp(self.to_half_width, travelled);
        let edge = (closest.distance(point) - half_width).max(0.0);
        let rise = (vertex.z - self.from_surface.lerp(self.to_surface, travelled)).max(0.0);
        let near = 1.0 - (edge / normalized(RIVER_WET_METRES)).min(1.0);
        let low = 1.0 - (rise / normalized(RIVER_WET_RISE_METRES)).min(1.0);
        near * low
    }

    /// The half extent of everything this segment can wet, which is what the
    /// lattice registers it over.
    fn reach(&self) -> f32 {
        self.from_half_width.max(self.to_half_width) + normalized(RIVER_WET_METRES)
    }
}

/// Metres as the fraction of the island square the generator works in.
fn normalized(metres: f32) -> f32 {
    metres / ISLAND_WORLD_METRES
}

/// Every above-sea channel segment on one island, on a uniform lattice over the
/// island square.
///
/// Each cell holds every segment close enough to wet a point inside it, so a
/// vertex reads one cell and nothing else. Cells are a wetness range across,
/// which leaves most of them empty on an island whose channels run through a
/// few valleys, and that is what keeps a pass over two million vertices to
/// milliseconds without a thread pool.
struct WetBanks {
    segments: Lattice<WetSegment>,
}

impl WetBanks {
    fn new(island: &Island) -> Self {
        Self::over(wet_segments(island))
    }

    /// The lattice over a segment list, which is what the tests can build.
    fn over(segments: Vec<WetSegment>) -> Self {
        Self {
            segments: Lattice::over(segments, RIVER_WET_METRES, |segment| {
                let reach = segment.reach();
                (
                    segment.from.min(segment.to) - reach,
                    segment.from.max(segment.to) + reach,
                )
            }),
        }
    }

    /// One proximity per terrain vertex, in the mesh's own order. Ground beside
    /// a fall takes the wetter of the two answers: a plunge pool soaks its
    /// surround well past the channel's own bank, and reads as one damp
    /// hollow rather than as a band with a fall standing outside it.
    fn measure(&self, terrain: &Mesh, drops: &DropIndex) -> Vec<f32> {
        if self.segments.is_empty() && drops.is_empty() {
            return vec![0.0; terrain.vertices.len()];
        }
        terrain
            .vertices
            .iter()
            .map(|&vertex| {
                self.segments
                    .nearby(vertex.truncate())
                    .map(|segment| segment.wetness(vertex))
                    .fold(drops.spray(vertex), f32::max)
            })
            .collect()
    }
}

/// The lattice cell a normalized coordinate falls in. Anything off the island
/// square holds the edge cell, which is where the terrain square ends anyway.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn cell_index(coordinate: f32, span: usize) -> usize {
    #[allow(clippy::cast_precision_loss)]
    let last = (span - 1) as f32;
    (coordinate * last.max(1.0)).clamp(0.0, last) as usize
}

/// Every above-sea part of every channel segment. A reach that crosses the
/// waterline is clipped at the crossing so its final bank remains wet right up
/// to the mouth; only the already-submerged part is left to sea proximity.
fn wet_segments(island: &Island) -> Vec<WetSegment> {
    let options = island.options();
    let source = normalized(options.river_source_width_metres);
    let maximum = normalized(options.river_maximum_width_metres);
    let mut segments = Vec::new();
    for river in island.rivers() {
        let widths = river.target_half_widths(source, maximum);
        for (index, pair) in river.nodes.windows(2).enumerate() {
            let (from, to) = (pair[0], pair[1]);
            if let Some(segment) = wet_segment(from, to, widths[index], widths[index + 1]) {
                segments.push(segment);
            }
        }
    }
    segments
}

/// The above-sea share of one channel segment, if it has one.
fn wet_segment(
    from: RiverNode,
    to: RiverNode,
    from_half_width: f32,
    to_half_width: f32,
) -> Option<WetSegment> {
    if from.surface <= 0.0 && to.surface <= 0.0 {
        return None;
    }
    let mut segment = WetSegment {
        from: from.position.truncate(),
        to: to.position.truncate(),
        from_surface: from.surface,
        to_surface: to.surface,
        from_half_width,
        to_half_width,
    };
    if from.surface <= 0.0 || to.surface <= 0.0 {
        let crossing = from.surface / (from.surface - to.surface);
        let point = from.position.lerp(to.position, crossing).truncate();
        let half_width = from_half_width.lerp(to_half_width, crossing);
        if from.surface <= 0.0 {
            segment.from = point;
            segment.from_surface = 0.0;
            segment.from_half_width = half_width;
        } else {
            segment.to = point;
            segment.to_surface = 0.0;
            segment.to_half_width = half_width;
        }
    }
    Some(segment)
}

/// The finished island. Replaced whole on every rebuild.
#[derive(Resource)]
pub struct GeneratedIsland(pub IslandData);

/// Render meshes prepared on the generation task and consumed once by the
/// spawn systems. Keeping them separate leaves [`IslandData`] as the plain,
/// cacheable generator payload while the expensive attribute conversion stays
/// off the main thread.
#[derive(Resource)]
pub struct PreparedMeshes {
    pub terrain: Vec<[Option<bevy::prelude::Mesh>; chunk::TIERS]>,
    pub river: Option<bevy::prelude::Mesh>,
    pub river_rocks: Option<bevy::prelude::Mesh>,
}

struct PreparedIsland {
    data: IslandData,
    meshes: PreparedMeshes,
}

impl PreparedIsland {
    fn new(data: IslandData, drops: &DropIndex) -> Self {
        let started = Instant::now();
        let terrain = data
            .terrain_chunks
            .iter()
            .map(|chunk| {
                let centre = chunk::origin(chunk);
                let origin = island_to_world(centre.x, centre.y, centre.z);
                std::array::from_fn(|level| {
                    let tier = &chunk.tiers[level];
                    convert::terrain_mesh(&tier.mesh, &tier.materials, &tier.river_wetness, origin)
                })
            })
            .collect();
        let river = convert::river_mesh(&data.river_mesh, drops);
        let river_rocks = convert::rock_mesh(&data.river_rock_mesh, drops);
        info!(
            "render meshes prepared off-thread in {:.0} ms",
            started.elapsed().as_secs_f32() * 1_000.0
        );
        Self {
            data,
            meshes: PreparedMeshes {
                terrain,
                river,
                river_rocks,
            },
        }
    }
}

#[derive(Component)]
struct GenerationTask(Task<Result<PreparedIsland, String>>);

/// What an arriving island replaces: everything the island before it spawned.
type Replaced = With<IslandEntity>;

pub struct IslandGenPlugin;

impl Plugin for IslandGenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GenerationStatus>()
            .add_message::<Regenerate>()
            .add_message::<IslandReady>()
            .add_systems(Startup, start_first_generation)
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
        method,
        cache_reads,
    } = settings;
    info!(
        "generating {method} island: seed {seed}, terrain size {}",
        options.terrain_size,
    );
    // The cache read runs on the task as well: an entry is tens of megabytes,
    // and no frame should wait on that any more than on generation.
    let task = AsyncComputeTaskPool::get().spawn(async move {
        island_data(seed, options, method, cache_reads)
            .map(|(data, drops)| PreparedIsland::new(data, &drops))
    });
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
    settings.method = request.method;
    let requested = *settings;
    start(&mut commands, requested, &mut status);
}

/// Reads the cached geometry if there is any, and otherwise generates it and
/// leaves an entry behind. A cache that cannot be read or written only ever
/// costs time, so nothing here is fatal but a failed generation.
fn island_data(
    seed: u64,
    options: IslandOptions,
    method: GenerationMethod,
    cache_reads: bool,
) -> Result<(IslandData, DropIndex), String> {
    let path = cache::path(method, cache::key(seed, &options));
    if !cache_reads {
        info!("island cache bypassed: --no-cache");
    } else if let Some(data) = cache::read(&path, seed, &options) {
        info!("island cache hit: {}", path.display());
        let drops = DropIndex::new(&data.river_drops);
        return Ok((data, drops));
    } else {
        info!("island cache miss: {}", path.display());
    }
    let (data, drops) = IslandData::new(&Island::generate_with_method(seed, options, method)?);
    match cache::write(&path, seed, &data) {
        Ok(()) => info!("island cache written: {}", path.display()),
        Err(error) => warn!(
            "could not write island cache to {}: {error}",
            path.display()
        ),
    }
    Ok((data, drops))
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
            Ok(prepared) => {
                let PreparedIsland { data, meshes } = prepared;
                info!(
                    "island ready: {} chunks over {} terrain vertices at LOD 0, {} rivers, \
                     {} trees, {} bushes",
                    data.terrain_chunks.len(),
                    data.tier_vertices(0),
                    data.rivers,
                    data.trees.len(),
                    data.bushes.len()
                );
                info!(
                    "island arguments: {}",
                    options::command_line(settings.seed, &settings.options, settings.method)
                );
                for entity in &replaced {
                    commands.entity(entity).despawn();
                }
                commands.insert_resource(GeneratedIsland(data));
                commands.insert_resource(meshes);
                ready.write(IslandReady);
                status.built = Some(Regenerate {
                    seed: settings.seed,
                    options: settings.options,
                    method: settings.method,
                });
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
    use motu::{ISLAND_WORLD_METRES, River, RiverNode, Vec2, Vec3};

    use super::{
        DROP_FALL_FLOOR, DROP_PLUNGE_METRES, DropIndex, HEIGHT_GRID, IslandData, RiverDrop,
        WetBanks, WetSegment, closest_on_segment, drops_of, normalized, wet_segment,
    };
    use crate::chunk;

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

    /// A gently domed surface over the island square, as a triangulated grid.
    /// Fine enough that the chunk grid cuts through triangles rather than only
    /// between them, which is the case the slicer has to be right about.
    fn dome(span: usize) -> motu::Mesh {
        #[allow(clippy::cast_precision_loss)]
        let last = (span - 1) as f32;
        let mut mesh = motu::Mesh::default();
        for row in 0..span {
            for column in 0..span {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (column as f32 / last, row as f32 / last);
                let height = 0.03 * (1.0 - (x - 0.5).abs() - (y - 0.5).abs()).max(0.0);
                mesh.vertices.push(Vec3::new(x, y, height));
                mesh.normals.push(Vec3::Z);
                mesh.uv.push(motu::Vec2::new(x, y));
            }
        }
        for row in 0..span - 1 {
            for column in 0..span - 1 {
                let corner = u32::try_from(row * span + column).expect("a small grid");
                let span = u32::try_from(span).expect("a small grid");
                mesh.triangles.extend([
                    corner,
                    corner + 1,
                    corner + span,
                    corner + 1,
                    corner + span + 1,
                    corner + span,
                ]);
            }
        }
        mesh
    }

    /// Measuring the bank wetness chunk by chunk has to answer exactly what
    /// measuring the whole surface answers, or the chunked terrain would carry
    /// a different ground than the island-wide mesh it replaced. It does
    /// because the measurement is a function of the vertex position alone and
    /// the slicer leaves interior vertices where it found them.
    #[test]
    fn the_wetness_a_chunk_measures_is_the_wetness_the_whole_surface_measures() {
        let banks = WetBanks::over(vec![WetSegment {
            from: motu::Vec2::new(0.2, 0.3),
            to: motu::Vec2::new(0.85, 0.72),
            from_surface: 0.004,
            to_surface: 0.012,
            from_half_width: normalized(3.0),
            to_half_width: normalized(6.0),
        }]);
        let drops = DropIndex::new(&[]);
        let surface = dome(65);
        let whole = banks.measure(&surface, &drops);
        assert!(
            whole.iter().any(|&wetness| wetness > 0.1),
            "the channel wets nothing, so the comparison would prove nothing"
        );

        let mut compared = 0;
        for tile in chunk::sliced(&surface) {
            let measured = banks.measure(&tile, &drops);
            for (vertex, wetness) in tile.vertices.iter().zip(&measured) {
                let Some(index) = surface
                    .vertices
                    .iter()
                    .position(|original| original.distance(*vertex) < 1.0e-6)
                else {
                    // A vertex the slicer created on a chunk edge has nothing
                    // to compare against; it carries what the surface there
                    // always said, which is the same function.
                    continue;
                };
                assert!(
                    (wetness - whole[index]).abs() < 1.0e-5,
                    "vertex {vertex:?} reads {wetness} in its chunk and {} whole",
                    whole[index]
                );
                compared += 1;
            }
        }
        assert!(
            compared > surface.vertices.len(),
            "{compared} vertices compared"
        );
    }

    /// One channel running east along `y == 0.5`, given as `(metres travelled,
    /// water surface in metres)` pairs. Everything the drop pass reads is on
    /// the nodes, so a river is exactly this much.
    fn channel(profile: &[(f32, f32)]) -> River {
        River {
            nodes: profile
                .iter()
                .map(|&(along, surface)| RiverNode {
                    vertex: 0,
                    flow: 1,
                    surface: surface / ISLAND_WORLD_METRES,
                    position: Vec3::new(
                        0.25 + along / ISLAND_WORLD_METRES,
                        0.5,
                        surface / ISLAND_WORLD_METRES,
                    ),
                })
                .collect(),
            join: None,
        }
    }

    fn drops(profile: &[(f32, f32)]) -> Vec<RiverDrop> {
        drops_of(&[channel(profile)], normalized(2.0), normalized(14.0))
    }

    #[test]
    fn a_mouth_segment_is_clipped_at_the_waterline() {
        let river = channel(&[(0.0, 1.5), (20.0, -0.5)]);
        let segment = wet_segment(river.nodes[0], river.nodes[1], 0.001, 0.003)
            .expect("the upstream part is above sea");
        assert!((segment.to_surface).abs() < f32::EPSILON);
        assert!((segment.to.x - (0.25 + 15.0 / ISLAND_WORLD_METRES)).abs() < 1.0e-6);
        assert!((segment.to_half_width - 0.0025).abs() < 1.0e-6);
    }

    #[test]
    fn closest_segment_projection_reports_the_same_point_and_parameter() {
        let (closest, travelled) =
            closest_on_segment(Vec2::new(0.25, 1.0), Vec2::ZERO, Vec2::new(1.0, 0.0));
        assert_eq!(closest, Vec2::new(0.25, 0.0));
        assert!((travelled - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn shader_fall_floor_matches_the_baked_drop_field() {
        let expected = format!("const FALL_FLOOR: f32 = {DROP_FALL_FLOOR};");
        assert!(
            include_str!("../shaders/river.wgsl").contains(&expected),
            "missing {expected:?}"
        );
    }

    /// A face the generator cut as four short steps is one fall, not four. Four
    /// drops there would stack four lips, four feet and four plunge pools
    /// inside one sheet.
    #[test]
    fn a_stepped_face_is_one_drop() {
        let stepped = drops(&[
            (0.0, 20.0),
            (2.0, 19.9),
            (2.4, 19.2),
            (2.8, 18.4),
            (3.2, 17.6),
            (3.6, 16.8),
            (6.0, 16.7),
        ]);
        assert_eq!(stepped.len(), 1, "{stepped:?}");
        // The lip is the last node the water was still in its bed at, which is
        // the 19.9 m one: the two metres before it fall a tenth and are a
        // reach, not a step.
        assert!((stepped[0].metres() - 3.1).abs() < 0.05, "{stepped:?}");
        // Downstream of the foot, taken from the reach past it rather than from
        // a lip and a foot that stand almost on top of each other.
        assert!(stepped[0].direction.x > 0.99, "{stepped:?}");
    }

    /// A reach that runs down rather than stepping down carries no drop
    /// however far it descends, and neither does a step too small to see.
    #[test]
    fn only_a_real_step_is_a_drop() {
        assert!(drops(&[(0.0, 20.0), (40.0, 12.0), (80.0, 4.0)]).is_empty());
        assert!(drops(&[(0.0, 20.0), (1.0, 19.5), (2.0, 19.4)]).is_empty());
        // And a fall that has already reached the sea is the coastline.
        assert!(drops(&[(0.0, 0.2), (1.0, -2.0)]).is_empty());
    }

    /// The three proximities have to answer for the three places on a fall a
    /// fragment can stand, and for nowhere else.
    #[test]
    fn the_drop_field_separates_a_lip_a_face_and_a_foot() {
        let drop = RiverDrop {
            lip: Vec3::new(0.5, 0.5, 6.0 / ISLAND_WORLD_METRES),
            foot: Vec3::new(0.5, 0.5, 2.0 / ISLAND_WORLD_METRES),
            direction: Vec2::X,
            half_width: normalized(2.0) * 0.5,
        };
        let index = DropIndex::new(&[drop]);
        let at = |along: f32, height: f32| {
            index.field(Vec3::new(
                0.5 + along / ISLAND_WORLD_METRES,
                0.5,
                height / ISLAND_WORLD_METRES,
            ))
        };

        let lip = at(-0.5, 6.0);
        assert!(lip.approach > 0.80, "{lip:?}");
        let face = at(0.0, 4.0);
        assert!(face.fall > DROP_FALL_FLOOR, "{face:?}");
        assert!(face.plunge < 0.01 && face.approach < 0.01, "{face:?}");
        // At the foot's own level the sheet has not quite ended — that is the
        // last of it meeting the water it lands in — but the plunge is what is
        // there.
        let foot = at(0.5, 2.0);
        assert!(foot.plunge > 0.85, "{foot:?}");
        assert!(foot.fall < foot.plunge * 0.1, "{foot:?}");

        // The face carries how far down it the water has come, so the foot end
        // of a sheet is more aerated than its lip.
        assert!(at(0.0, 2.4).fall > at(0.0, 5.6).fall);

        // And a reach the far side of the island knows nothing about any of it.
        let elsewhere = index.field(Vec3::new(0.9, 0.2, 2.0 / ISLAND_WORLD_METRES));
        assert_eq!(elsewhere, super::DropField::default());
        assert!(index.spray(Vec3::new(0.9, 0.2, 2.0 / ISLAND_WORLD_METRES)) < f32::EPSILON);
    }

    /// The plunge is drawn out downstream and cut short upstream, because what
    /// a fall throws forward is what is still turning over below it.
    #[test]
    fn the_plunge_reaches_downstream_further_than_up() {
        let index = DropIndex::new(&[RiverDrop {
            lip: Vec3::new(0.5, 0.5, 6.0 / ISLAND_WORLD_METRES),
            foot: Vec3::new(0.5, 0.5, 2.0 / ISLAND_WORLD_METRES),
            direction: Vec2::X,
            half_width: normalized(2.0) * 0.5,
        }]);
        let at = |along: f32| {
            index
                .field(Vec3::new(
                    0.5 + along / ISLAND_WORLD_METRES,
                    0.5,
                    2.0 / ISLAND_WORLD_METRES,
                ))
                .plunge
        };
        assert!(at(4.0) > at(-4.0));
        assert!(at(DROP_PLUNGE_METRES + 1.0) < f32::EPSILON);
    }
}
