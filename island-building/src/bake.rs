//! Deterministic lighting-free exterior charts with physically scaled material samples.
// Dimensions are validated <=2048; encoder conversions round and clamp to byte range.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]
use crate::{
    BakedAtlas, BuildingRecipe, Facade, MaterialSlot, MaterialTexture, OpeningKind, RoofKind,
};

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
    let size = r.atlas_size;
    let count = (size * size * 4) as usize;
    let mut result = BakedAtlas {
        size,
        albedo: vec![0; count],
        normal: vec![0; count],
        masks: vec![0; count],
    };
    for y in 0..size {
        for x in 0..size {
            let column = (x * 4 / size) as usize;
            let row = (y * 2 / size) as usize;
            let chart = row * 4 + column;
            let u = ((x as f32 / size as f32 * 4. - column as f32 - 0.05) / 0.9).clamp(0., 1.);
            let v = (0.95 - (y as f32 / size as f32 * 2. - row as f32)) / 0.9;
            let v = v.clamp(0., 1.);
            let mut s = sample(r, materials, chart, u, v);
            let du = 4. / (size as f32 * 0.9);
            let dv = 2. / (size as f32 * 0.9);
            let a = sample(r, materials, chart, (u - du).max(0.), v).height;
            let c = sample(r, materials, chart, (u + du).min(1.), v).height;
            let d = sample(r, materials, chart, u, (v - dv).max(0.)).height;
            let e = sample(r, materials, chart, u, (v + dv).min(1.)).height;
            if (a - c).abs() + (d - e).abs() > 0.0001 {
                let width = if chart < 4 {
                    Facade::ALL[chart].length(r)
                } else {
                    roof_dimensions(r).0
                };
                let height = if chart < 4 {
                    r.wall_height + r.roof.rise + r.floor_thickness
                } else {
                    r.depth
                };
                let nx = (a - c) / (2. * du * width);
                let ny = (d - e) / (2. * dv * height);
                let len = (1. + nx * nx + ny * ny).sqrt();
                s.normal = [encode(nx / len), encode(ny / len), encode(1. / len), 255];
            }
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
fn sample(
    r: &BuildingRecipe,
    materials: &[MaterialTexture],
    chart: usize,
    u: f32,
    v: f32,
) -> Sample {
    if chart >= 4 {
        return match chart {
            4 | 5 => roof_sample(r, materials, u, v),
            _ => plain(r.materials.floor, 0., 230),
        };
    }
    let facade = Facade::ALL[chart];
    let along = (u - 0.5) * facade.length(r);
    let z = v * (r.wall_height + r.roof.rise + r.floor_thickness) - r.floor_thickness;
    let mut sample = texture(materials, MaterialSlot::Wall, along, z, r.materials.wall);
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
                    let mut surface = if r.materials.door_recipe.is_some() {
                        texture(
                            materials,
                            MaterialSlot::Door,
                            local_x,
                            z - o.sill,
                            r.materials.door,
                        )
                    } else {
                        let groove = ((along - o.offset) * 9.).rem_euclid(1.) < 0.045;
                        let factor = if groove { 0.7 } else { 1. };
                        plain(r.materials.door.map(|v| v * factor), 0., 205)
                    };
                    surface.height = -0.012;
                    let knob_x = if o.hinge == crate::HingeSide::Left {
                        o.width - 0.078
                    } else {
                        0.078
                    };
                    let knob_z = 1.02_f32.min((o.height - 0.009) * 0.55) + 0.006;
                    if (local_x - knob_x).hypot(z - knob_z) < 0.025 {
                        surface = plain([0.48, 0.44, 0.34], 0.02, 61);
                        surface.metallic = 230;
                    }
                    surface
                }
            };
        }
    }
    sample
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
fn roof_sample(recipe: &BuildingRecipe, materials: &[MaterialTexture], u: f32, v: f32) -> Sample {
    let (length, width) = roof_dimensions(recipe);
    let mut result = texture(
        materials,
        MaterialSlot::Roof,
        u * length,
        v * (recipe.depth + recipe.roof.overhang * 2.),
        recipe.materials.roof,
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roof_seams_share_geometry_stations_and_remain_in_far_maps() {
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 512;
        let (_, width) = roof_dimensions(&recipe);
        let count = (width / 0.35).ceil();
        let rib = roof_sample(&recipe, &[], 1. / count, 0.5);
        let flat = roof_sample(&recipe, &[], 1.5 / count, 0.5);
        assert!(rib.height > flat.height);
        assert!(rib.ao < flat.ao);
        assert_ne!(rib.albedo, flat.albedo);
        let atlas = bake(&recipe, &[]);
        let size = recipe.atlas_size as usize;
        let roof_row = size * 3 / 4;
        assert!((0..size / 4).any(|x| {
            let i = (roof_row * size + x) * 4;
            atlas.normal[i] != 128 || atlas.normal[i + 1] != 128
        }));
    }
}
