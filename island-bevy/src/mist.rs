//! Local mist, as fog volumes placed off the island's own data.
//!
//! Two kinds, and neither is hand-placed. Valley mist is found in the height
//! grid the generator hands over for walking on: the island is divided into
//! coarse cells, and a cell whose lowest ground stands inland, stands low, and
//! stands well under the ground that rings it is a hollow — which is where the
//! drainage runs and where mist actually collects on a still morning. Waterfall
//! mist comes from the drops the river pass already found, one volume per fall
//! at its foot, scaled by how hard it hits, which is the same measurement the
//! spray cloud and the wet rock around it are built from.
//!
//! What both are held to is the review's own acceptance: mist adds depth
//! without hiding terrain. A valley volume is a pool in a hollow with the ridge
//! beside it clear, not a sheet over the island, so the count is capped, the
//! depth is a couple of dozen metres and nothing is placed above the elevation
//! the island's own valleys stop at.

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::FogVolume,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use motu::ISLAND_WORLD_METRES;

use crate::{
    convert::island_to_world,
    hash::{mix, unit},
    island_gen::{GeneratedIsland, HEIGHT_GRID, IslandEntity, IslandReady, RiverDrop},
    weather::Weather,
};

/// Coarse cells across the island square that valley mist is looked for in. At
/// twenty-four the cell is 83 m, which is about as wide as the generator's own
/// valley floors and narrow enough that a volume sits in one rather than over
/// the ridge beside it.
const VALLEY_CELLS: usize = 24;
/// The most valley volumes one island carries. Every one is a raymarched box,
/// and a dozen pools is already more mist than the island has hollows worth
/// filling.
const VALLEY_LIMIT: usize = 12;
/// Metres above the sea a hollow's floor has to stand to be inland at all, and
/// the highest floor mist is placed on. Above this the island is ridge and
/// summit; morning mist sits in the catchments, not on the tops.
const VALLEY_FLOOR_METRES: f32 = 3.0;
const VALLEY_CEILING_METRES: f32 = 110.0;
/// Metres the ground around a cell has to rise over its floor for the cell to
/// be a hollow rather than a slope. Below this every seaward apron on the
/// island would qualify.
const VALLEY_RELIEF_METRES: f32 = 22.0;
/// Metres deep a valley volume is, and how far under the floor it starts. Mist
/// lies in the hollow and a little way into it, so the ground at the bottom is
/// inside the volume rather than under its face.
const VALLEY_DEPTH_METRES: f32 = 26.0;
const VALLEY_SINK_METRES: f32 = 6.0;
/// How far past its own cell a valley volume reaches. Neighbouring hollows
/// overlap slightly, which is what keeps a run of them reading as one valley
/// rather than as a row of boxes.
const VALLEY_SPREAD: f32 = 1.35;

/// The most fall volumes one island carries, and the least a fall may throw to
/// get one. The default island cuts nineteen drops and the eroded three, so the
/// cap only ever bites on the first.
const FALL_LIMIT: usize = 10;
const FALL_MINIMUM_STRENGTH: f32 = 0.15;
/// Metres across and above the foot a fall's own volume reaches at full
/// strength, and the share of that a weak fall keeps. The spray cloud in
/// `spray` throws droplets a few metres; this is the haze standing in the same
/// air, so it is a little wider and a little taller than the fall itself.
const FALL_RADIUS_METRES: f32 = 9.0;
const FALL_RISE_METRES: f32 = 6.0;
const FALL_WEAK_SHARE: f32 = 0.45;

/// Absorption and scattering of the mist itself, per metre per unit of density.
///
/// Absorption is almost nothing against the component's own default of 0.3:
/// water droplets in air scatter light, they do not swallow it, and a volume
/// that absorbs as much as it scatters reads as a dark box wherever the sun
/// does not reach into it. These two together are also what a look's density
/// has to be read against — the optical depth across a volume is density times
/// their sum times the metres the ray crosses, so a hundred-metre valley pool
/// is thick at a density of 0.02 and opaque at anything near one.
const MIST_ABSORPTION: f32 = 0.02;
const MIST_SCATTERING: f32 = 0.55;
/// How much of the scattering goes towards the viewer rather than away. High,
/// because the low sun a mist look puts behind the island is exactly the
/// arrangement that makes a valley glow.
const MIST_ASYMMETRY: f32 = 0.72;

/// What a look's glow is divided by to reach `FogVolume::light_intensity`.
///
/// The volumetric pass multiplies its in-scattering by the camera's exposure
/// and then composites the result into the frame the main pass wrote, which has
/// not been exposed yet — the tone mapper is what applies exposure, and it runs
/// later. Every other term in this scene is raw radiance under a hundred
/// thousand lux of sunlight, so the mist arrives a whole exposure short of it
/// and can only ever darken what it stands in front of: out-scattering removes
/// background, and nothing measurable comes back. Dividing the light term by
/// the same exposure puts it back in the units the rest of the frame is in, and
/// the look's glow is then the share of that physical in-scattering it wants —
/// a share, because mist returning its full physical radiance under this sun is
/// several times brighter than anything else in the frame.
fn light_intensity(glow: f32) -> f32 {
    glow / crate::camera::EXPOSURE.exposure().max(f32::MIN_POSITIVE)
}

/// Samples along each edge of the shared density volume. A fog volume is a box
/// of uniform density otherwise, and a box is exactly what it looks like: hard
/// faces standing over the terrain wherever the mist is thick enough to see.
/// This is what gives one an edge that fades and an inside that is not flat.
const DENSITY_SAMPLES: u32 = 32;
/// Lattice cells and octaves of the noise inside the envelope, and the least
/// density the middle of a volume keeps. Mist that thins to nothing in its own
/// centre reads as two clouds rather than one.
const DENSITY_CELLS: u32 = 3;
const DENSITY_OCTAVES: u32 = 3;
const DENSITY_FLOOR: f32 = 0.45;
/// How far in from a face the envelope has reached full density, as a share of
/// the half extent. A quarter leaves most of the box at full strength and still
/// takes every face to zero.
const DENSITY_MARGIN: f32 = 0.5;
/// Distinguishes the density volume from the crate's other hashed values.
const DENSITY_SALT: u64 = 0x7b25_e0a4_16cf_98d3;

pub struct MistPlugin;

impl Plugin for MistPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_density)
            .add_systems(Update, place_mist);
    }
}

/// The density volume every fog volume is shaped by. One texture for all of
/// them: what differs between two pools of mist is where they are and how thick
/// they are, not the shape of the noise inside them.
#[derive(Resource)]
struct MistDensity(Handle<Image>);

fn build_density(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(MistDensity(images.add(density_volume())));
}

/// A soft blob: noise inside an envelope that reaches zero at all six faces, so
/// the box a fog volume is drawn from is never the shape the mist has.
fn density_volume() -> Image {
    let span = DENSITY_SAMPLES as usize;
    #[allow(clippy::cast_precision_loss)]
    let along = |index: usize| (index as f32 + 0.5) / span as f32;
    let mut data = Vec::with_capacity(span * span * span);
    for layer in 0..span {
        for row in 0..span {
            for column in 0..span {
                let point = Vec3::new(along(column), along(row), along(layer));
                let envelope = fade(point.x) * fade(point.y) * fade(point.z);
                let noise = DENSITY_FLOOR + (1.0 - DENSITY_FLOOR) * blobs(point);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                data.push(((envelope * noise).clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: DENSITY_SAMPLES,
            height: DENSITY_SAMPLES,
            depth_or_array_layers: DENSITY_SAMPLES,
        },
        TextureDimension::D3,
        data,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Nothing scrolls this volume and its own faces are already zero, so the
    // edge is what a sample outside it should find.
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// The envelope along one axis: zero at both faces, one by [`DENSITY_MARGIN`]
/// of the way in.
fn fade(along: f32) -> f32 {
    let inward = along.min(1.0 - along) * 2.0;
    let progress = (inward / DENSITY_MARGIN).clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

/// Tiling three-dimensional value noise, summed. Tiling only so the volume has
/// no discontinuity of its own; the envelope is what actually ends it.
fn blobs(point: Vec3) -> f32 {
    let mut total = 0.0;
    let mut normalization = 0.0;
    let mut amplitude = 1.0;
    let mut period = DENSITY_CELLS;
    for octave in 0..DENSITY_OCTAVES {
        total += amplitude * cell_noise(point, period, mix(u64::from(octave), DENSITY_SALT));
        normalization += amplitude;
        amplitude *= 0.5;
        period *= 2;
    }
    total / normalization
}

fn cell_noise(point: Vec3, period: u32, salt: u64) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let cells = period as f32;
    let scaled = point * cells;
    let base = scaled.floor();
    let blend = scaled - base;
    let smooth = blend * blend * (Vec3::splat(3.0) - 2.0 * blend);
    #[allow(clippy::cast_possible_truncation)]
    let base = base.as_i64vec3();
    let corner = |x: i64, y: i64, z: i64| {
        let wrap = |value: i64| value.rem_euclid(i64::from(period)).cast_unsigned();
        let cell = wrap(x).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ wrap(y).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
            ^ wrap(z).wrapping_mul(0x1656_67b1_9e37_79f9);
        unit(mix(cell, salt))
    };
    let face = |z: i64| {
        let near = corner(base.x, base.y, z).lerp(corner(base.x + 1, base.y, z), smooth.x);
        let far =
            corner(base.x, base.y + 1, z).lerp(corner(base.x + 1, base.y + 1, z), smooth.x);
        near.lerp(far, smooth.y)
    };
    face(base.z).lerp(face(base.z + 1), smooth.z)
}

/// Rebuilds the volumes whenever the island or the look changes. Both matter:
/// a new island moves every hollow, and a look decides whether there is mist in
/// them at all.
fn place_mist(
    mut commands: Commands,
    weather: Res<Weather>,
    mut applied: Local<Option<Weather>>,
    mut ready: MessageReader<IslandReady>,
    island: Option<Res<GeneratedIsland>>,
    density: Res<MistDensity>,
    volumes: Query<Entity, With<FogVolume>>,
) {
    let arrived = ready.read().next().is_some();
    if !arrived && *applied == Some(*weather) {
        return;
    }
    *applied = Some(*weather);
    for volume in &volumes {
        commands.entity(volume).despawn();
    }
    let Some(island) = island else {
        return;
    };
    let look = weather.look();
    if !look.has_mist() {
        return;
    }

    let mist = &look.mist;
    let volume = |thickness: f32| FogVolume {
        fog_color: mist.colour,
        density_factor: thickness,
        density_texture: Some(density.0.clone()),
        absorption: MIST_ABSORPTION,
        scattering: MIST_SCATTERING,
        scattering_asymmetry: MIST_ASYMMETRY,
        light_intensity: light_intensity(mist.glow),
        ..default()
    };

    let mut placed = 0;
    if mist.valley > 0.0 {
        for hollow in hollows(&island.0.heights) {
            commands.spawn((
                Name::new("Valley mist"),
                IslandEntity,
                volume(mist.valley),
                Transform {
                    translation: hollow.centre,
                    scale: hollow.extent,
                    ..default()
                },
            ));
            placed += 1;
        }
    }
    let mut falls = 0;
    if mist.fall > 0.0 {
        for (transform, strength) in fall_volumes(&island.0.river_drops) {
            commands.spawn((
                Name::new("Fall mist"),
                IslandEntity,
                volume(mist.fall * strength),
                transform,
            ));
            falls += 1;
        }
    }
    info!("mist: {placed} valley volumes and {falls} at falls, under {}", look.name);
}

/// One placed volume: where its box sits and how big it is.
struct Hollow {
    centre: Vec3,
    extent: Vec3,
}

/// The island's own hollows, deepest first and capped.
///
/// A cell qualifies on three counts read off the height grid alone: its floor
/// stands above the sea, so it is inland; its floor stands under the elevation
/// the island's valleys stop at; and the ground in the ring of cells around it
/// rises well over that floor, so it is a hollow rather than an open slope. The
/// score is how enclosed it is against how low it lies, which puts the deepest
/// inland catchments first and the shallow coastal dips last.
fn hollows(heights: &[f32]) -> Vec<Hollow> {
    let span = HEIGHT_GRID as usize;
    if heights.len() != span * span {
        return Vec::new();
    }
    // The lowest and highest ground in each coarse cell, in metres.
    let mut floor = vec![f32::MAX; VALLEY_CELLS * VALLEY_CELLS];
    let mut ceiling = vec![f32::MIN; VALLEY_CELLS * VALLEY_CELLS];
    for row in 0..span {
        for column in 0..span {
            let metres = heights[row * span + column] * ISLAND_WORLD_METRES;
            let cell = coarse(row, span) * VALLEY_CELLS + coarse(column, span);
            floor[cell] = floor[cell].min(metres);
            ceiling[cell] = ceiling[cell].max(metres);
        }
    }

    let mut found: Vec<(f32, usize)> = Vec::new();
    for row in 0..VALLEY_CELLS {
        for column in 0..VALLEY_CELLS {
            let cell = row * VALLEY_CELLS + column;
            let low = floor[cell];
            if !(VALLEY_FLOOR_METRES..=VALLEY_CEILING_METRES).contains(&low) {
                continue;
            }
            // The ring around it, its own cell included: a valley floor has
            // ground standing over it on more than one side.
            let mut around = f32::MIN;
            for neighbour_row in row.saturating_sub(1)..=(row + 1).min(VALLEY_CELLS - 1) {
                for neighbour_column in
                    column.saturating_sub(1)..=(column + 1).min(VALLEY_CELLS - 1)
                {
                    around = around.max(ceiling[neighbour_row * VALLEY_CELLS + neighbour_column]);
                }
            }
            let enclosure = around - low;
            if enclosure < VALLEY_RELIEF_METRES {
                continue;
            }
            let lowness = 1.0 - (low / VALLEY_CEILING_METRES).clamp(0.0, 1.0);
            found.push((enclosure * lowness, cell));
        }
    }
    found.sort_by(|left, right| right.0.total_cmp(&left.0));
    found.truncate(VALLEY_LIMIT);

    #[allow(clippy::cast_precision_loss)]
    let cell_metres = ISLAND_WORLD_METRES / VALLEY_CELLS as f32;
    found
        .into_iter()
        .map(|(_, cell)| {
            let (row, column) = (cell / VALLEY_CELLS, cell % VALLEY_CELLS);
            #[allow(clippy::cast_precision_loss)]
            let centre = |index: usize| (index as f32 + 0.5) / VALLEY_CELLS as f32;
            let floor = floor[cell];
            let ground = island_to_world(centre(column), centre(row), 0.0);
            Hollow {
                centre: Vec3::new(
                    ground.x,
                    floor - VALLEY_SINK_METRES + VALLEY_DEPTH_METRES * 0.5,
                    ground.z,
                ),
                extent: Vec3::new(
                    cell_metres * VALLEY_SPREAD,
                    VALLEY_DEPTH_METRES,
                    cell_metres * VALLEY_SPREAD,
                ),
            }
        })
        .collect()
}

/// Which coarse cell a height-grid index falls in.
fn coarse(index: usize, span: usize) -> usize {
    (index * VALLEY_CELLS / span.max(1)).min(VALLEY_CELLS - 1)
}

/// One volume per fall worth one, strongest first and capped, each as its own
/// transform and the share of the look's density it carries.
fn fall_volumes(drops: &[RiverDrop]) -> Vec<(Transform, f32)> {
    let mut ranked: Vec<&RiverDrop> = drops
        .iter()
        .filter(|drop| drop.strength() >= FALL_MINIMUM_STRENGTH)
        .collect();
    ranked.sort_by(|left, right| right.strength().total_cmp(&left.strength()));
    ranked.truncate(FALL_LIMIT);
    ranked
        .into_iter()
        .map(|drop| {
            let strength = drop.strength();
            let share = FALL_WEAK_SHARE + (1.0 - FALL_WEAK_SHARE) * strength;
            let foot = island_to_world(drop.foot.x, drop.foot.y, drop.foot.z);
            let radius = FALL_RADIUS_METRES * share;
            // Tall enough to stand up the face of the fall, and centred so the
            // water at the foot is inside the box rather than under it.
            let height = FALL_RISE_METRES * share + drop.metres();
            (
                Transform {
                    translation: Vec3::new(foot.x, foot.y + height * 0.35, foot.z),
                    scale: Vec3::new(radius * 2.0, height, radius * 2.0),
                    ..default()
                },
                share,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use motu::ISLAND_WORLD_METRES;

    use super::{
        HEIGHT_GRID, VALLEY_CEILING_METRES, VALLEY_FLOOR_METRES, VALLEY_LIMIT, hollows,
    };

    /// A grid with one narrow gully cut across an otherwise high plateau. The
    /// gully is the only hollow on it, so that is where every volume has to go.
    fn gully() -> Vec<f32> {
        let span = HEIGHT_GRID as usize;
        #[allow(clippy::cast_precision_loss)]
        (0..span * span)
            .map(|index| {
                let column = (index % span) as f32 / span as f32;
                // 180 m plateau, with a channel 20 m above the sea down the
                // middle fifteenth of the square.
                let metres = if (column - 0.5).abs() < 0.033 { 20.0 } else { 180.0 };
                metres / ISLAND_WORLD_METRES
            })
            .collect()
    }

    /// Mist goes in the hollow and nowhere else. This is the whole claim the
    /// placement makes: it is read off the island's own drainage rather than
    /// written down beside it.
    #[test]
    fn mist_finds_the_gully_and_not_the_plateau() {
        let found = hollows(&gully());
        assert!(!found.is_empty(), "the gully carries no mist");
        assert!(found.len() <= VALLEY_LIMIT);
        for hollow in &found {
            // Every volume straddles the channel down the middle of the square.
            assert!(
                hollow.centre.x.abs() < ISLAND_WORLD_METRES * 0.06,
                "a volume at x {}",
                hollow.centre.x
            );
            // And sits over the gully floor rather than over the plateau.
            assert!(hollow.centre.y < 40.0, "a volume at {} m", hollow.centre.y);
        }
    }

    /// Ground with nothing standing over it is not a hollow, however low it is,
    /// and ground above the island's own valleys is not one however enclosed.
    #[test]
    fn open_ground_and_high_ground_carry_none() {
        let span = HEIGHT_GRID as usize;
        let flat = vec![10.0 / ISLAND_WORLD_METRES; span * span];
        assert!(hollows(&flat).is_empty());

        // The same gully, lifted so its floor stands above the ceiling.
        #[allow(clippy::cast_precision_loss)]
        let alpine: Vec<f32> = gully()
            .iter()
            .map(|height| height + (VALLEY_CEILING_METRES + 20.0) / ISLAND_WORLD_METRES)
            .collect();
        assert!(hollows(&alpine).is_empty());

        // And a drowned one, whose floor is under the sea.
        let drowned: Vec<f32> = gully()
            .iter()
            .map(|height| height - (VALLEY_FLOOR_METRES + 25.0) / ISLAND_WORLD_METRES)
            .collect();
        assert!(hollows(&drowned).is_empty());

        // A grid that is not the expected square places nothing at all.
        assert!(hollows(&[0.1; 9]).is_empty());
    }
}
