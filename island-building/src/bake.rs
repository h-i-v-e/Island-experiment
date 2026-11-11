//! Deterministic lighting-free exterior charts with physically scaled material samples.
// Dimensions are validated <=2048; encoder conversions round and clamp to byte range.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]
use crate::{
    BakedAtlas, BuildingRecipe, Facade, MaterialSlot, MaterialTexture, OpeningKind, RoofCovering,
    RoofKind,
};

mod doors;
mod roof_filter;
use roof_filter::FilteredRoof;

struct Sources<'a> {
    materials: &'a [MaterialTexture],
    roof: Option<FilteredRoof>,
    thatch_end: Option<FilteredRoof>,
    frames: [Vec<crate::geometry::timber_frame::Member>; 4],
}
impl<'a> Sources<'a> {
    fn new(recipe: &BuildingRecipe, materials: &'a [MaterialTexture]) -> Self {
        let roof = (recipe.roof.covering != RoofCovering::StandingSeam)
            .then(|| materials.iter().find(|m| m.slot == MaterialSlot::Roof))
            .flatten()
            .map(|texture| {
                let (width, height) = chart_dimensions(recipe, 4);
                let width_pixels = recipe.atlas_size as f32 * 0.9 / 4.;
                let height_pixels =
                    recipe.atlas_size as f32 * 0.9 / crate::lod::atlas_rows(recipe) as f32;
                FilteredRoof::new(texture, [width / width_pixels, height / height_pixels])
            });
        let thatch_end = materials
            .iter()
            .find(|m| m.slot == MaterialSlot::ThatchEnd)
            .map(|texture| {
                let ([width, height], _) = crate::lod::thatch_end_chart(recipe);
                let pixels = recipe.atlas_size as f32 * 0.9;
                FilteredRoof::new(
                    texture,
                    [
                        width / (pixels / 4.),
                        height / (pixels / crate::lod::atlas_rows(recipe) as f32),
                    ],
                )
            });
        Self {
            materials,
            roof,
            thatch_end,
            frames: Facade::ALL.map(|facade| crate::geometry::timber_frame::layout(recipe, facade)),
        }
    }
}

#[derive(Clone, Copy)]
struct Sample {
    albedo: [u8; 4],
    normal: [u8; 4],
    ao: u8,
    roughness: u8,
    metallic: u8,
    height: f32,
}
/// Bake the same eight padded charts used by the minimal exterior mesh.
#[must_use]
pub(crate) fn bake(r: &BuildingRecipe, materials: &[MaterialTexture]) -> BakedAtlas {
    let sources = Sources::new(r, materials);
    let size = r.atlas_size;
    let count = (size * size * 4) as usize;
    let rows = crate::lod::atlas_rows(r);
    let mut result = BakedAtlas {
        size,
        albedo: vec![0; count],
        normal: vec![0; count],
        masks: vec![0; count],
    };
    for y in 0..size {
        for x in 0..size {
            let column = (x * 4 / size) as usize;
            let row = (y * rows / size) as usize;
            let chart = row * 4 + column;
            let u = ((x as f32 / size as f32 * 4. - column as f32 - 0.05) / 0.9).clamp(0., 1.);
            let v = (0.95 - (y as f32 / size as f32 * rows as f32 - row as f32)) / 0.9;
            let v = v.clamp(0., 1.);
            let mut s = sample(r, &sources, chart, u, v);
            let du = 4. / (size as f32 * 0.9);
            let dv = rows as f32 / (size as f32 * 0.9);
            // New covering relief is carried by the filtered source normals;
            // these charts have no additional baked height to differentiate.
            if !matches!(chart, 4 | 5) || r.roof.covering == RoofCovering::StandingSeam {
                let a = sample(r, &sources, chart, (u - du).max(0.), v).height;
                let c = sample(r, &sources, chart, (u + du).min(1.), v).height;
                let d = sample(r, &sources, chart, u, (v - dv).max(0.)).height;
                let e = sample(r, &sources, chart, u, (v + dv).min(1.)).height;
                if (a - c).abs() + (d - e).abs() > 0.0001 {
                    let (width, height) = chart_dimensions(r, chart);
                    let nx = (a - c) / (2. * du * width);
                    let ny = (d - e) / (2. * dv * height);
                    let len = (1. + nx * nx + ny * ny).sqrt();
                    s.normal = [encode(nx / len), encode(ny / len), encode(1. / len), 255];
                }
            }
            // Atlas V decreases with physical height/depth, opposite to the source
            // material UVs. Mirror tangent-space green to preserve the world normal.
            s.normal[1] = 255 - s.normal[1];
            let i = ((y * size + x) * 4) as usize;
            result.albedo[i..i + 4].copy_from_slice(&s.albedo);
            result.normal[i..i + 4].copy_from_slice(&s.normal);
            result.masks[i..i + 4].copy_from_slice(&[s.ao, s.roughness, s.metallic, 255]);
        }
    }
    result
}
fn encode(v: f32) -> u8 {
    ((v * 0.5 + 0.5) * 255.).round().clamp(0., 255.) as u8
}
fn rgba(c: [f32; 3]) -> [u8; 4] {
    [srgb(c[0]), srgb(c[1]), srgb(c[2]), 255]
}
fn srgb(v: f32) -> u8 {
    let encoded = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (encoded * 255.).round().clamp(0., 255.) as u8
}
fn plain(colour: [f32; 3], height: f32, roughness: u8) -> Sample {
    Sample {
        albedo: rgba(colour),
        normal: [128, 128, 255, 255],
        ao: 255,
        roughness,
        metallic: 0,
        height,
    }
}
fn texture(
    materials: &[MaterialTexture],
    slot: MaterialSlot,
    x: f32,
    y: f32,
    fallback: [f32; 3],
) -> Sample {
    let Some(m) = materials.iter().find(|m| m.slot == slot) else {
        return plain(fallback, 0., 220);
    };
    let u = (x / m.physical_size[0]).rem_euclid(1.);
    let v = (y / m.physical_size[1]).rem_euclid(1.);
    let px = (u * m.size as f32) as u32 % m.size;
    let py = (v * m.size as f32) as u32 % m.size;
    let i = ((py * m.size + px) * 4) as usize;
    Sample {
        albedo: m.albedo[i..i + 4].try_into().expect("four channels"),
        normal: m.normal[i..i + 4].try_into().expect("four channels"),
        ao: m.masks[i],
        roughness: m.masks[i + 1],
        metallic: m.masks[i + 2],
        height: 0.,
    }
}
fn sample(r: &BuildingRecipe, sources: &Sources<'_>, chart: usize, u: f32, v: f32) -> Sample {
    let materials = sources.materials;
    if chart >= 8 && r.chimney.is_some() {
        return chimney_sample(r, materials, chart, u, v);
    }
    if chart >= 4 {
        return match chart {
            4 | 5 => {
                let flip = chart == 5 && r.roof.covering != RoofCovering::StandingSeam;
                let mut result = roof_sample(r, sources, if flip { 1. - u } else { u }, v);
                if flip {
                    result.normal[0] = 255 - result.normal[0];
                }
                result
            }
            7 if sources.thatch_end.is_some() => {
                let ([width, height], inset) = crate::lod::thatch_end_chart(r);
                sources
                    .thatch_end
                    .as_ref()
                    .expect("end source checked")
                    .sample(u * width, v * height - inset)
            }
            _ => plain(r.materials.floor, 0., 230),
        };
    }
    let facade = Facade::ALL[chart];
    let along = (u - 0.5) * facade.length(r);
    let z = v * (r.wall_height + r.roof.rise + r.floor_thickness) - r.floor_thickness;
    let mut sample = wall_sample(r, sources, chart, [along, z]);
    for o in r.openings.iter().filter(|o| o.facade == facade) {
        let dx = (along - o.offset).abs();
        let inside = dx <= o.width * 0.5 && z >= o.sill && z <= o.sill + o.height;
        let border =
            dx <= o.width * 0.5 + 0.07 && z >= o.sill - 0.07 && z <= o.sill + o.height + 0.07;
        if border {
            sample = plain(r.materials.trim, 0.035, 190);
            sample.ao = 235;
        }
        if inside {
            sample = match o.kind {
                OpeningKind::Window => {
                    let mut s = plain(
                        [
                            r.materials.glass[0] * 0.20,
                            r.materials.glass[1] * 0.23,
                            r.materials.glass[2] * 0.26,
                        ],
                        -0.025,
                        70,
                    );
                    let mullion = dx < 0.022 || (z - (o.sill + o.height * 0.52)).abs() < 0.018;
                    if mullion {
                        s = plain(r.materials.trim, 0.025, 190);
                    }
                    s
                }
                OpeningKind::Door => {
                    let local_x = along - o.offset + o.width * 0.5;
                    doors::sample(r, materials, o, [local_x, z - o.sill])
                }
            };
        }
    }
    sample
}

fn wall_sample(
    recipe: &BuildingRecipe,
    sources: &Sources<'_>,
    chart: usize,
    point: [f32; 2],
) -> Sample {
    let plaster = texture(
        sources.materials,
        MaterialSlot::Wall,
        point[0],
        point[1],
        recipe.materials.wall,
    );
    let footprint = [
        Facade::ALL[chart].length(recipe) * 4. / (recipe.atlas_size as f32 * 0.9),
        (recipe.wall_height + recipe.roof.rise + recipe.floor_thickness)
            * crate::lod::atlas_rows(recipe) as f32
            / (recipe.atlas_size as f32 * 0.9),
    ];
    let mut total = 0_f32;
    let mut strongest = 0_f32;
    let mut selected = None;
    for member in &sources.frames[chart] {
        let coverage = member.coverage(point, footprint);
        total += coverage;
        if coverage > strongest {
            strongest = coverage;
            selected = Some(member);
        }
    }
    let Some(member) = selected else {
        return plaster;
    };
    let uv = member.uv(point);
    let direction = member.direction;
    let mut sample = texture(
        sources.materials,
        MaterialSlot::Timber,
        uv[0],
        uv[1],
        recipe.materials.timber,
    );
    let x = f32::from(sample.normal[0]) / 127.5 - 1.;
    let y = f32::from(sample.normal[1]) / 127.5 - 1.;
    sample.normal[0] = encode(x * direction[1] + y * direction[0]);
    sample.normal[1] = encode(-x * direction[0] + y * direction[1]);
    sample.height = recipe.walls.frame_projection;
    blend_surface(plaster, sample, total.min(1.))
}

fn blend_surface(base: Sample, top: Sample, coverage: f32) -> Sample {
    let mix =
        |a: u8, b: u8| (f32::from(a) * (1. - coverage) + f32::from(b) * coverage).round() as u8;
    let mut normal = std::array::from_fn::<_, 3, _>(|i| {
        (f32::from(base.normal[i]) * (1. - coverage) + f32::from(top.normal[i]) * coverage) / 127.5
            - 1.
    });
    let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt();
    normal = normal.map(|v| v / length.max(0.0001));
    Sample {
        albedo: std::array::from_fn(|i| {
            if i == 3 {
                255
            } else {
                srgb(linear(base.albedo[i]) * (1. - coverage) + linear(top.albedo[i]) * coverage)
            }
        }),
        normal: [encode(normal[0]), encode(normal[1]), encode(normal[2]), 255],
        ao: mix(base.ao, top.ao),
        roughness: mix(base.roughness, top.roughness),
        metallic: mix(base.metallic, top.metallic),
        height: top.height * coverage,
    }
}

fn linear(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Length of a roof chart along the slope and its horizontal width.
fn roof_dimensions(recipe: &BuildingRecipe) -> (f32, f32) {
    let (width, slope) = if recipe.roof.kind == RoofKind::Gable {
        (
            recipe.width * 0.5 + recipe.roof.overhang,
            recipe.roof.rise / (recipe.width * 0.5),
        )
    } else {
        (
            recipe.width + recipe.roof.overhang * 2.,
            recipe.roof.rise / recipe.width,
        )
    };
    (width * (1. + slope * slope).sqrt(), width)
}
fn roof_depth(recipe: &BuildingRecipe) -> f32 {
    recipe.depth
        + recipe.roof.overhang * 2.
        + if recipe.roof.covering == RoofCovering::Thatch {
            crate::geometry::thatch::RIDGE_PROJECTION * 2.
        } else {
            0.
        }
}

fn roof_sample(recipe: &BuildingRecipe, sources: &Sources<'_>, u: f32, v: f32) -> Sample {
    let (length, width) = roof_dimensions(recipe);
    let x = u * length;
    let y = v * roof_depth(recipe);
    let mut result = sources.roof.as_ref().map_or_else(
        || {
            texture(
                sources.materials,
                MaterialSlot::Roof,
                x,
                y,
                recipe.materials.roof,
            )
        },
        |roof| roof.sample(x, y),
    );
    if recipe.roof.covering != RoofCovering::StandingSeam {
        return result;
    }
    // These stations are exactly roof_seams()'s geometry positions. Integrating
    // their subpixel width keeps narrow ribs visible without distant aliasing.
    let count = (width / 0.35).ceil();
    let station = u * count;
    let nearest = station.round();
    if nearest > 0. && nearest < count {
        let distance = (station - nearest).abs() * width / count;
        let pixel_width = width * 4. / (recipe.atlas_size as f32 * 0.9);
        let coverage = ((0.012 + pixel_width * 0.5 - distance) / pixel_width).clamp(0., 1.);
        result.height = 0.018 * coverage;
        result.ao = (f32::from(result.ao) * (1. - 0.08 * coverage)).round() as u8;
        result.roughness = (f32::from(result.roughness) + 8. * coverage)
            .min(255.)
            .round() as u8;
        for channel in &mut result.albedo[..3] {
            *channel = (f32::from(*channel) * (1. - 0.04 * coverage)).round() as u8;
        }
    }
    result
}

fn chart_dimensions(recipe: &BuildingRecipe, chart: usize) -> (f32, f32) {
    if chart == 7 && recipe.roof.covering == RoofCovering::Thatch {
        let ([width, height], _) = crate::lod::thatch_end_chart(recipe);
        return (width, height);
    }
    if chart < 4 {
        return (
            Facade::ALL[chart].length(recipe),
            recipe.wall_height + recipe.roof.rise + recipe.floor_thickness,
        );
    }
    if chart < 6 {
        return (roof_dimensions(recipe).0, roof_depth(recipe));
    }
    if let Some(bounds) = crate::geometry::chimney::exterior_bounds(recipe).filter(|_| chart >= 8) {
        if chart < 12 {
            let width = if matches!(Facade::ALL[chart - 8], Facade::South | Facade::North) {
                bounds.max[0] - bounds.min[0]
            } else {
                bounds.max[1] - bounds.min[1]
            };
            return (width, bounds.max[2] - bounds.min[2]);
        }
        return (
            bounds.max[0] - bounds.min[0] + 0.12,
            bounds.max[1] - bounds.min[1] + 0.12,
        );
    }
    (recipe.width, recipe.depth)
}
fn chimney_sample(
    recipe: &BuildingRecipe,
    materials: &[MaterialTexture],
    chart: usize,
    u: f32,
    v: f32,
) -> Sample {
    let bounds =
        crate::geometry::chimney::exterior_bounds(recipe).expect("chimney chart only when enabled");
    if chart < 12 {
        let z = bounds.min[2] + v * (bounds.max[2] - bounds.min[2]);
        let margin = if z > bounds.max[2] - 0.1 { 0.06 } else { 0. };
        let min = [bounds.min[0] - margin, bounds.min[1] - margin];
        let max = [bounds.max[0] + margin, bounds.max[1] + margin];
        let along = match Facade::ALL[chart - 8] {
            Facade::South => min[0] + u * (max[0] - min[0]),
            Facade::East => min[1] + u * (max[1] - min[1]),
            Facade::North => -max[0] + u * (max[0] - min[0]),
            Facade::West => -max[1] + u * (max[1] - min[1]),
        };
        return texture(
            materials,
            MaterialSlot::Chimney,
            along,
            z,
            recipe.materials.chimney,
        );
    }
    let x = bounds.min[0] - 0.06 + u * (bounds.max[0] - bounds.min[0] + 0.12);
    let y = bounds.min[1] - 0.06 + v * (bounds.max[1] - bounds.min[1] + 0.12);
    let c = recipe.chimney.as_ref().expect("enabled chimney");
    let wall = 0.1_f32.min(c.stack_width.min(c.stack_depth) * 0.2);
    if x > bounds.min[0] + wall
        && x < bounds.max[0] - wall
        && y > bounds.min[1] + wall
        && y < bounds.max[1] - wall
    {
        let colour = crate::materials::colour(recipe, MaterialSlot::Firebox);
        return plain([colour[0], colour[1], colour[2]], -0.06, 240);
    }
    texture(
        materials,
        MaterialSlot::Chimney,
        x,
        y,
        recipe.materials.chimney,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distant_thatch_ends_bake_the_cut_straw_into_the_reserved_chart() {
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.chimney = None;
        recipe.roof.covering = RoofCovering::Thatch;
        let end = MaterialTexture {
            slot: MaterialSlot::ThatchEnd,
            size: 1,
            albedo: vec![83, 121, 45, 255],
            normal: vec![128, 128, 255, 255],
            masks: vec![225, 250, 0, 255],
            physical_size: [0.16; 2],
            source_hash: "cut-straw-chart".into(),
        };
        let atlas = bake(&recipe, &[end]);
        let offset = (96 * 128 + 112) * 4;
        assert_eq!(&atlas.albedo[offset..offset + 4], &[83, 121, 45, 255]);
        assert_eq!(&atlas.masks[offset..offset + 4], &[225, 250, 0, 255]);
    }
    #[test]
    fn atlas_green_channel_preserves_world_normals_when_chart_v_is_mirrored() {
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        let materials = [
            MaterialSlot::Wall,
            MaterialSlot::Roof,
            MaterialSlot::Chimney,
        ]
        .map(|slot| MaterialTexture {
            slot,
            size: 1,
            albedo: vec![128, 128, 128, 255],
            normal: vec![128, 180, 240, 255],
            masks: vec![255, 220, 0, 255],
            physical_size: [2., 2.],
            source_hash: "normal-parity".into(),
        });
        let atlas = bake(&recipe, &materials);
        // Interior of chimney chart8 has no relief-height override. Its UV V
        // reverses the source bitangent, so positive near G must become negative.
        let offset = (84 * 128 + 16) * 4;
        assert_eq!(atlas.normal[offset], 128);
        assert_eq!(atlas.normal[offset + 1], 75);
        assert_eq!(atlas.normal[offset + 2], 240);
    }
    #[test]
    fn roof_seams_share_geometry_stations_and_remain_in_far_maps() {
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 512;
        let (_, width) = roof_dimensions(&recipe);
        let count = (width / 0.35).ceil();
        let sources = Sources::new(&recipe, &[]);
        let rib = roof_sample(&recipe, &sources, 1. / count, 0.5);
        let flat = roof_sample(&recipe, &sources, 1.5 / count, 0.5);
        assert!(rib.height > flat.height);
        assert!(rib.ao < flat.ao);
        assert_ne!(rib.albedo, flat.albedo);
        let atlas = bake(&recipe, &[]);
        let size = recipe.atlas_size as usize;
        let roof_row = size * 3 / (2 * crate::lod::atlas_rows(&recipe) as usize);
        assert!((0..size / 4).any(|x| {
            let i = (roof_row * size + x) * 4;
            atlas.normal[i] != 128 || atlas.normal[i + 1] != 128
        }));
    }
}
