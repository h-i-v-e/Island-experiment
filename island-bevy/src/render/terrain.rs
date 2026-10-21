//! Solid ground: the chunked terrain surface and the settled river rocks.
//!
//! The terrain arrives as a fixed grid of chunks, each carrying the same square
//! at three levels of detail, and every one of those is its own entity. That is
//! what gives the frustum something to cull — an island-wide mesh is drawn
//! whole or not at all — and what lets a chunk behind the camera cost nothing
//! while the one under it is at full detail.
//!
//! The three levels of one chunk hand over through [`VisibilityRange`], which
//! Bevy dithers across the margin two of them share, so a level change has no
//! frame it happens on. Every level of one chunk stands at the same point —
//! `chunk::origin`, with the vertices relative to it — because the culling
//! stage and the shader both read the crossfade distance off the entity's
//! translation, and geometry left in world space puts every chunk at the
//! island's centre for both.

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use motu::ISLAND_WORLD_METRES;

use crate::{
    budget::BudgetItem,
    capture::{DebugView, TerrainComparison},
    chunk::{self, TerrainChunk},
    convert::island_to_world,
    island_gen::{
        GeneratedIsland, IslandEntity, IslandReady, PreparedMaterialTextures, PreparedMeshes,
    },
    material_textures,
    surface::{RockExtension, RockMaterial, TerrainExtension, TerrainMaterial},
};

/// Metres at which a chunk hands LOD 0 over to LOD 1 and LOD 1 over to LOD 2,
/// and the run each pair dithers across.
///
/// The levels differ by how far their surfaces stand apart, not by how many
/// vertices they have, and on the reference island a LOD 1 chunk boundary sits
/// 0.46 m from the LOD 0 surface on average and a LOD 2 boundary 1.60 m from
/// LOD 1. Held against the 1857 pixels per radian of a 2560-wide frame, the
/// first is under a pixel past 850 m and the second under a pixel past 3.0 km,
/// which is further than the island's own diagonal from any pose. The near
/// handover is therefore set just past where LOD 1 stops being resolvable, and
/// the far one past the whole island: at `overview`, the furthest pose, only
/// the last corner of the terrain square reaches LOD 2, and that corner is
/// seabed under the sea plane. The shader's own metre-scale detail layer is
/// already gone by 600 m, so nothing inside the near range depends on it.
///
/// The gap between the two also keeps a LOD 0 chunk from ever touching a LOD 2
/// one: 2400 m minus 1050 m is 1350 m, and two neighbouring chunk centres are
/// at most about 350 m apart. Only two levels can ever meet at a seam, which is
/// what the skirt in `chunk` is sized against.
const NEAR_METRES: f32 = 2_400.0;
const NEAR_DITHER: f32 = 80.0;
const MID_METRES: f32 = 5_000.0;
const MID_DITHER: f32 = 160.0;
/// Where the coarsest level itself stops. The far clip stands at forty island
/// widths, so this is past anything the camera can be flown to and still see
/// ground at; it exists because a range has to have an end, not as a cull.
const FAR_METRES: f32 = ISLAND_WORLD_METRES * 30.0;
const FAR_DITHER: f32 = ISLAND_WORLD_METRES * 10.0;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_terrain.run_if(on_message::<IslandReady>));
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut terrains: ResMut<Assets<TerrainMaterial>>,
    mut rocks: ResMut<Assets<RockMaterial>>,
    island: Res<GeneratedIsland>,
    mut prepared: ResMut<PreparedMeshes>,
    mut prepared_materials: ResMut<PreparedMaterialTextures>,
    view: Res<DebugView>,
    comparison: Res<TerrainComparison>,
) {
    let island = &island.0;
    let Some(material_textures) = prepared_materials.textures.take() else {
        error!("the arriving island has no complete runtime material texture group");
        return;
    };
    let atlases = match material_textures::upload(
        &mut images,
        prepared_materials.inputs,
        material_textures,
    ) {
        Ok(atlases) => atlases,
        Err(error) => {
            error!("could not upload runtime terrain materials: {error}");
            return;
        }
    };

    // RGB is the island-space full-detail normal; alpha is terrain AO. A
    // single linear image avoids spending separate texture/sampler slots on
    // maps that always share the same atlas coordinates.
    let neutral_surface = images.add(surface_image(1, 1, vec![128, 128, 255, 255]));
    let surface_maps = island.surface_atlases.each_ref().map(|maps| {
        maps.as_ref().map(|maps| {
            let rgba = maps
                .normal_rgb
                .chunks_exact(3)
                .zip(&maps.occlusion)
                .flat_map(|(normal, &occlusion)| [normal[0], normal[1], normal[2], occlusion])
                .collect();
            images.add(surface_image(maps.width, maps.height, rgba))
        })
    });

    // The extension writes base colour, roughness and reflectance outright, so
    // what the base material still decides is only how the surface is drawn:
    // opaque, single-sided, shadow casting and receiving. One handle per level
    // of detail rather than one in total: every chunk of a level shares its
    // handle and batches with the rest, and the level is the one thing a
    // fragment cannot work out for itself.
    let divisions =
        u8::try_from(chunk::DIVISIONS).expect("the terrain grid fits in the shader uniform");
    let chunk_metres = ISLAND_WORLD_METRES / f32::from(divisions);
    let ground: Vec<Handle<TerrainMaterial>> = (0..chunk::TIERS)
        .map(|level| {
            let surface_map = level
                .checked_sub(1)
                .and_then(|index| surface_maps.get(index))
                .and_then(Option::as_ref);
            let mut extension = TerrainExtension::new(
                island.options.max_height,
                ISLAND_WORLD_METRES,
                chunk_metres,
                u32::try_from(level).unwrap_or(0),
                &atlases,
                surface_map.unwrap_or(&neutral_surface).clone(),
                comparison.surface_maps && surface_map.is_some(),
            );
            // Asset events are flushed after Update. Seed the current view at
            // construction so a newly spawned diagnostic island cannot draw
            // one ordinary-shading frame before `capture` observes its Added
            // event on the next Update.
            extension.settings.debug_view = view.flag();
            terrains.add(TerrainMaterial {
                base: StandardMaterial::default(),
                extension,
            })
        })
        .collect();
    let mut spawned = 0;
    for (index, chunk) in island.terrain_chunks.iter().enumerate() {
        let Some(rendered) = prepared.terrain.get_mut(index) else {
            warn!("missing prepared meshes for terrain chunk {index}");
            continue;
        };
        spawned += spawn_chunk(
            &mut commands,
            &mut meshes,
            &ground,
            chunk,
            rendered,
            comparison.forced_lod,
        );
    }
    if spawned == 0 {
        warn!("island has no terrain chunks");
    } else {
        info!(
            "terrain: {spawned} chunk entities over {} chunks, {} vertices at LOD 0, {} at LOD 1, \
             {} at LOD 2",
            island.terrain_chunks.len(),
            island.tier_vertices(0),
            island.tier_vertices(1),
            island.tier_vertices(2),
        );
    }

    let stone = rocks.add(RockMaterial {
        base: StandardMaterial::default(),
        extension: RockExtension::default(),
    });
    if let Some(mesh) = prepared.river_rocks.take() {
        commands.spawn((
            Name::new("River rocks"),
            IslandEntity,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(stone),
            Transform::default(),
        ));
    }
}

/// Filter distant terrain without exhausting the native eight-texel gutters.
/// Only the first two downsampled levels are generated: their four-texel
/// footprints fit those gutters, whereas a complete chain mixes chart interiors.
/// Packed normal channels are averaged linearly and renormalized in the shader.
fn surface_image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    let mip_levels = append_surface_mips(
        image
            .data
            .as_mut()
            .expect("a new image retains its pixel data"),
        width as usize,
        height as usize,
    );
    image.texture_descriptor.mip_level_count = u32::from(mip_levels);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        lod_max_clamp: f32::from(mip_levels - 1),
        ..default()
    });
    image
}

/// Appends two box-filtered levels in wgpu's level-major order. Integer source
/// bounds include the last row/column of an odd cropped atlas, and a dimension
/// already at one stays at one while the other dimension shrinks.
fn append_surface_mips(rgba: &mut Vec<u8>, mut width: usize, mut height: usize) -> u8 {
    let mut offset = 0;
    let mut levels = 1;
    while levels < 3 && (width > 1 || height > 1) {
        let next_width = (width / 2).max(1);
        let next_height = (height / 2).max(1);
        let next_offset = rgba.len();
        rgba.reserve(next_width * next_height * 4);
        for y in 0..next_height {
            for x in 0..next_width {
                let mut total = [0_u32; 4];
                let mut samples = 0;
                for source_y in y * height / next_height..(y + 1) * height / next_height {
                    for source_x in x * width / next_width..(x + 1) * width / next_width {
                        let pixel = offset + (source_y * width + source_x) * 4;
                        for (sum, &channel) in total.iter_mut().zip(&rgba[pixel..pixel + 4]) {
                            *sum += u32::from(channel);
                        }
                        samples += 1;
                    }
                }
                rgba.extend(
                    total.map(|sum| u8::try_from((sum + samples / 2) / samples).unwrap_or(255)),
                );
            }
        }
        offset = next_offset;
        width = next_width;
        height = next_height;
        levels += 1;
    }
    levels
}

/// One chunk's levels, or none at all where the generator left the square
/// empty. Returns how many entities were spawned.
fn spawn_chunk(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    ground: &[Handle<TerrainMaterial>],
    chunk: &TerrainChunk,
    rendered: &mut [Option<Mesh>; chunk::TIERS],
    forced_lod: Option<usize>,
) -> usize {
    let centre = chunk::origin(chunk);
    let origin = island_to_world(centre.x, centre.y, centre.z);
    let mut spawned = 0;
    for (level, mesh) in rendered.iter_mut().enumerate() {
        if forced_lod.is_some_and(|selected| selected != level) {
            continue;
        }
        let Some(mesh) = mesh.take() else {
            continue;
        };
        let vertices = u32::try_from(mesh.count_vertices()).unwrap_or(u32::MAX);
        let mut entity = commands.spawn((
            Name::new(format!(
                "Terrain chunk {},{} LOD {level}",
                chunk.column, chunk.row
            )),
            IslandEntity,
            BudgetItem::terrain(vertices),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(ground[level].clone()),
            Transform::from_translation(origin),
        ));
        if forced_lod.is_none() {
            entity.insert(tier(level));
        }
        spawned += 1;
    }
    spawned
}

/// The range one level of detail is drawn over. Each level starts exactly where
/// the finer one ends, or the dither would leave a gap or a doubling.
fn tier(level: usize) -> VisibilityRange {
    let near = NEAR_METRES..(NEAR_METRES + NEAR_DITHER);
    let mid = MID_METRES..(MID_METRES + MID_DITHER);
    let far = FAR_METRES..(FAR_METRES + FAR_DITHER);
    let (start_margin, end_margin) = match level {
        0 => (0.0..0.0, near),
        1 => (near, mid),
        _ => (mid, far),
    };
    VisibilityRange {
        start_margin,
        end_margin,
        // The entity's own translation, which is the chunk's centre and the
        // same for all three of its levels. The shader reads the crossfade
        // distance off that translation too, so the culling stage and the
        // dither cannot disagree about which level a chunk is at — and a
        // disagreement is a hole, not a blemish.
        use_aabb: false,
    }
}

#[cfg(test)]
mod tests {
    use motu::ISLAND_WORLD_METRES;

    use bevy::image::ImageSampler;

    use super::{FAR_METRES, surface_image, tier};
    use crate::chunk;

    #[test]
    fn surface_mips_preserve_constant_normals_and_occlusion_for_cropped_atlases() {
        let pixel = [54, 170, 243, 112];
        // Cropped atlases can have odd heights or a dimension already at one.
        for (width, height, expected_levels, expected_pixels) in [
            (8, 8, 3_u8, 64 + 16 + 4),
            (7, 5, 3, 35 + 6 + 1),
            (1, 7, 3, 7 + 3 + 1),
            (2, 1, 2, 2 + 1),
            (1, 1, 1, 1),
        ] {
            let image = surface_image(width, height, pixel.repeat((width * height) as usize));
            assert_eq!(
                image.texture_descriptor.mip_level_count,
                u32::from(expected_levels)
            );
            let data = image.data.as_ref().unwrap();
            assert_eq!(data.len(), expected_pixels * 4);
            assert!(data.chunks_exact(4).all(|sample| sample == pixel));
            let ImageSampler::Descriptor(sampler) = &image.sampler else {
                panic!("surface maps require an explicit sampler");
            };
            assert_eq!(
                sampler.lod_max_clamp.to_bits(),
                f32::from(expected_levels - 1).to_bits()
            );
        }
    }

    #[test]
    fn surface_mips_include_the_last_row_and_column_of_odd_atlases() {
        let mut pixels = vec![0; 3 * 3 * 4];
        pixels[8 * 4..].fill(252);
        let image = surface_image(3, 3, pixels);
        assert_eq!(image.texture_descriptor.mip_level_count, 2);
        assert_eq!(&image.data.as_ref().unwrap()[36..], &[28; 4]);
    }

    /// Every level starts where the finer one ends. A gap there would leave a
    /// band of distance with no ground drawn at all, and an overlap would draw
    /// two surfaces into the same pixels.
    #[test]
    fn the_levels_hand_over_without_a_gap() {
        let levels: Vec<_> = (0..chunk::TIERS).map(tier).collect();
        assert!(levels[0].start_margin.start.abs() < f32::EPSILON);
        for pair in levels.windows(2) {
            assert_eq!(pair[0].end_margin, pair[1].start_margin);
        }
        assert!(levels[chunk::TIERS - 1].end_margin.start >= FAR_METRES);
    }

    /// Two levels apart must never meet at a seam: the skirt is sized against
    /// the worst step between neighbouring levels, and a LOD 0 chunk beside a
    /// LOD 2 one could open a wider gap than that. Two chunk centres are a
    /// chunk diagonal apart at most, so the bands only have to be further apart
    /// than that.
    #[test]
    fn the_finest_and_coarsest_levels_cannot_be_neighbours() {
        let divisions = f32::from(u8::try_from(chunk::DIVISIONS).expect("a small grid"));
        let diagonal = ISLAND_WORLD_METRES / divisions * std::f32::consts::SQRT_2;
        let separation = tier(2).start_margin.start - tier(0).end_margin.end;
        assert!(
            separation > diagonal,
            "{separation} m between the bands is not clear of a {diagonal} m chunk diagonal"
        );
    }

    /// Every handover is a dither rather than a step, and every level's two
    /// margins are in the order `VisibilityRange` requires.
    #[test]
    fn every_margin_is_a_crossfade() {
        for level in 0..chunk::TIERS {
            let range = tier(level);
            assert!(
                range.end_margin.end > range.end_margin.start,
                "level {level}"
            );
            assert!(
                range.end_margin.start >= range.start_margin.end,
                "level {level}"
            );
        }
    }
}
