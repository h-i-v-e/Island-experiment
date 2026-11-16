//! Padded, lighting-free stone/roof/window bays for the minimal church exterior.
use super::windows;
use crate::{BakedAtlas, BuildingRecipe, MaterialSlot, MaterialTexture, geometry::Builder};

fn uv(chart: usize, u: f32, v: f32) -> [f32; 2] {
    [
        (chart % 4) as f32 * 0.25 + (0.05 + 0.9 * u) * 0.25,
        (chart / 4) as f32 * 0.5 + (0.95 - 0.9 * v) * 0.5,
    ]
}
pub(super) fn face(b: &mut Builder, chart: usize, p: [[f32; 3]; 4]) {
    b.quad(
        MaterialSlot::Atlas,
        p,
        [
            uv(chart, 0., 0.),
            uv(chart, 1., 0.),
            uv(chart, 1., 1.),
            uv(chart, 0., 1.),
        ],
    );
}
pub(super) fn triangle(b: &mut Builder, chart: usize, p: [[f32; 3]; 3]) {
    b.triangle(
        MaterialSlot::Atlas,
        p,
        [uv(chart, 0., 0.), uv(chart, 1., 0.), uv(chart, 0.5, 1.)],
    );
}
pub(super) fn cube(b: &mut Builder, chart: usize, min: [f32; 3], max: [f32; 3]) {
    let [a, c, z] = min;
    let [d, e, f] = max;
    for p in [
        [[a, c, z], [d, c, z], [d, c, f], [a, c, f]],
        [[d, e, z], [a, e, z], [a, e, f], [d, e, f]],
        [[d, c, z], [d, e, z], [d, e, f], [d, c, f]],
        [[a, e, z], [a, c, z], [a, c, f], [a, e, f]],
        [[a, c, f], [d, c, f], [d, e, f], [a, e, f]],
        [[a, e, z], [d, e, z], [d, c, z], [a, c, z]],
    ] {
        face(b, chart, p);
    }
}

pub(super) fn bake(r: &BuildingRecipe, materials: &[MaterialTexture]) -> BakedAtlas {
    let size = r.atlas_size;
    let count = (size * size * 4) as usize;
    let mut atlas = BakedAtlas {
        size,
        albedo: vec![0; count],
        normal: vec![0; count],
        masks: vec![0; count],
    };
    let c = r.church.as_ref().expect("church formula");
    let pattern = super::tracery::Pattern::new(r.opening_defaults.window);
    let tower = super::plan::Plan::new(r).at_height(c.tower_height - r.wall_height);
    let tower_pattern = super::tracery::Pattern::new(tower.window);
    for y in 0..size {
        for x in 0..size {
            let column = x * 4 / size;
            let row = y * 2 / size;
            let chart = row * 4 + column;
            let u = ((x as f32 / size as f32 * 4. - column as f32 - 0.05) / 0.9).clamp(0., 1.);
            let v = ((0.95 - (y as f32 / size as f32 * 2. - row as f32)) / 0.9).clamp(0., 1.);
            let (slot, width, height) = match chart {
                1 => (MaterialSlot::Roof, 2., 2.),
                2 => (MaterialSlot::Wall, c.bay_spacing, r.wall_height),
                3 => (MaterialSlot::Wall, c.nave_width, r.wall_height),
                7 => (
                    MaterialSlot::Wall,
                    c.tower_width / (c.tower_width / c.bay_spacing).floor().max(1.),
                    tower.height,
                ),
                5 => (MaterialSlot::Floor, 2., 2.),
                6 => (MaterialSlot::Foundation, 2., 2.),
                _ => (MaterialSlot::Wall, 2., 2.),
            };
            let mut pixel = sample(materials, slot, u * width, v * height);
            if chart == 4 {
                for channel in &mut pixel.0[..3] {
                    *channel = (f32::from(*channel) * 1.12).min(255.) as u8;
                }
            }
            if matches!(chart, 2 | 3 | 7) {
                let (w, h, sill) = if chart == 7 {
                    (tower.window.width, tower.window.height, tower.window.sill)
                } else if chart == 2 {
                    let s = r.opening_defaults.window;
                    (s.width, s.height, s.sill)
                } else {
                    let d = &r.openings[0];
                    (d.width, d.height + d.width * 0.866_025_4, 0.)
                };
                let px = (u - 0.5) * width;
                let z = v * height;
                let half = w * 0.5;
                let inside = px.abs() <= half && z >= sill && z <= windows::arch(px, half, sill, h);
                let surround = px.abs() <= half + 0.17
                    && z >= sill - 0.16
                    && z <= windows::arch(px, half + 0.17, sill, h + 0.17 * 3_f32.sqrt());
                if surround {
                    pixel.0 = [187, 177, 153, 255];
                    pixel.1 = [128, 128, 255, 255];
                }
                if inside {
                    if chart == 3 {
                        pixel = timber(r, materials, px + w * 0.5, z);
                        let seam = (width * 4. / size as f32).max(0.008) * 0.6;
                        if px.abs() < seam {
                            for value in &mut pixel.0[..3] {
                                *value /= 3;
                            }
                        }
                    } else {
                        let mullion = if chart == 7 { tower_pattern } else { pattern }.stone_at(
                            px,
                            z,
                            0.25 * (width * 4. / size as f32).max(height * 2. / size as f32),
                        );
                        pixel.0 = if mullion {
                            [187, 177, 153, 255]
                        } else {
                            [47, 70, 78, 255]
                        };
                        pixel.1 = [128, 128, 255, 255];
                        pixel.2 = [220, if mullion { 210 } else { 100 }, 0, 255];
                    }
                }
            }
            pixel.1[1] = 255 - pixel.1[1];
            let i = ((y * size + x) * 4) as usize;
            atlas.albedo[i..i + 4].copy_from_slice(&pixel.0);
            atlas.normal[i..i + 4].copy_from_slice(&pixel.1);
            atlas.masks[i..i + 4].copy_from_slice(&pixel.2);
        }
    }
    atlas
}
fn timber(
    r: &BuildingRecipe,
    materials: &[MaterialTexture],
    x: f32,
    z: f32,
) -> ([u8; 4], [u8; 4], [u8; 4]) {
    if materials.iter().any(|m| m.slot == MaterialSlot::Door) {
        return sample(materials, MaterialSlot::Door, x, z);
    }
    let colour = r.materials.door.map(|v| {
        let encoded = if v <= 0.003_130_8 {
            v * 12.92
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        };
        (encoded * 255.).round().clamp(0., 255.) as u8
    });
    (
        [colour[0], colour[1], colour[2], 255],
        [128, 128, 255, 255],
        [255, 190, 0, 255],
    )
}
fn sample(
    materials: &[MaterialTexture],
    slot: MaterialSlot,
    x: f32,
    y: f32,
) -> ([u8; 4], [u8; 4], [u8; 4]) {
    let m = materials
        .iter()
        .find(|m| m.slot == slot)
        .expect("church material");
    let px = ((x / m.physical_size[0]).rem_euclid(1.) * m.size as f32) as u32 % m.size;
    let py = ((y / m.physical_size[1]).rem_euclid(1.) * m.size as f32) as u32 % m.size;
    let i = ((py * m.size + px) * 4) as usize;
    (
        m.albedo[i..i + 4].try_into().expect("RGBA"),
        m.normal[i..i + 4].try_into().expect("RGBA"),
        m.masks[i..i + 4].try_into().expect("RGBA"),
    )
}
