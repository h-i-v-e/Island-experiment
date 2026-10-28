//! Stable equal-area roots, independent of terrain tessellation and render LOD.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
use crate::{Scene, collision::Ground, command::BedView};
use bevy::prelude::*;
use planet_rs::{DVec3, chunks::CellLocator};
use std::f32::consts::TAU;

pub(super) const VARIANTS: usize = 4;
const SPACING: f32 = 10.0;
const MAX_CANDIDATES: usize = 200_000;
const MAX_TREES: usize = 24_000;

pub(super) struct Plant {
    pub transform: Transform,
    pub root: Vec3,
    pub owner: u32,
    pub variant: usize,
}
pub(super) struct Built {
    pub plants: Vec<Plant>,
    pub prototypes: Vec<motu::TreeMeshes>,
    pub ground: Ground,
    pub upper: f32,
    pub occluder: f32,
    pub pixels: Vec<u8>,
}

pub(super) fn build(scene: Scene, shared: Option<Ground>) -> Built {
    let ground = shared.unwrap_or_else(|| Ground::new(&scene));
    let radius = scene.terrain.topology.radius as f32;
    let count = ((4.0 * std::f32::consts::PI * radius * radius / SPACING.powi(2)) as usize)
        .clamp(32, MAX_CANDIDATES);
    let locator = CellLocator::new(2).expect("fixed forest cell depth");
    let original = scene.bed == BedView::Original;
    let mut plants = Vec::new();
    for index in 0..count {
        let up = candidate(index, count, scene.seed);
        if coverage(up * radius, scene.seed) < 0.60 {
            continue;
        }
        let Some((point, normal, material)) = ground.forest_sample(&scene.terrain, up, original)
        else {
            continue;
        };
        if !suitable(point, normal, material, radius) {
            continue;
        }
        let tangent = up.any_orthonormal_vector();
        let bitangent = up.cross(tangent);
        // Crown clearance from channels, beaches and cliffs, measured along
        // the sphere rather than in a world XZ plane.
        let clear = [tangent, -tangent, bitangent, -bitangent]
            .into_iter()
            .all(|offset| {
                let probe = (point + offset * (3.0 / radius)).normalize();
                ground
                    .forest_sample(&scene.terrain, probe, original)
                    .is_some_and(|(p, n, m)| suitable(p, n, m, radius))
            });
        if !clear {
            continue;
        }
        let key = mix(scene.seed ^ index as u64 ^ 0x7472_6565_726f_6f74);
        let scale = 1.0 + unit(key) * 0.60;
        let rotation =
            Quat::from_rotation_arc(Vec3::Z, up) * Quat::from_rotation_z(unit(mix(key)) * TAU);
        let root = point - up * (0.08 / radius);
        plants.push(Plant {
            root,
            transform: Transform::from_translation(root)
                .with_rotation(rotation)
                .with_scale(Vec3::splat(motu::ISLAND_WORLD_METRES * scale / radius)),
            owner: locator
                .owner(DVec3::from_array(up.to_array().map(f64::from)))
                .unwrap(),
            variant: (mix(key ^ 0x0013) % VARIANTS as u64) as usize,
        });
        if plants.len() == MAX_TREES {
            break;
        }
    }
    let prototypes = [2018, 42, 1234, 98765]
        .into_iter()
        .map(motu::generate_tree)
        .collect();
    let upper = 1.0 + scene.terrain.elevations.iter().copied().fold(0.0, f64::max) as f32;
    let occluder = 1.0 + scene.terrain.elevations.iter().copied().fold(0.0, f64::min) as f32;
    let pixels = super::mask::build(&plants, radius);
    Built {
        plants,
        prototypes,
        ground,
        upper,
        occluder,
        pixels,
    }
}

fn suitable(point: Vec3, normal: Vec3, material: [f32; 4], radius: f32) -> bool {
    let height = (point.length() - 1.0) * radius;
    (4.0..90.0).contains(&height)
        && normal.dot(point.normalize()) >= 0.927_183_87
        && material[1] >= 0.25
        && material[2] < 0.03
}

fn candidate(index: usize, count: usize, seed: u64) -> Vec3 {
    let y = 1.0 - 2.0 * (index as f32 + 0.5) / count as f32;
    let azimuth = index as f64 * 2.399_963_229_728_653 + f64::from(unit(mix(seed)) * TAU);
    let radial = (1.0 - y * y).sqrt();
    Vec3::new(
        radial * azimuth.cos() as f32,
        y,
        radial * azimuth.sin() as f32,
    )
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
fn unit(value: u64) -> f32 {
    (value >> 40) as f32 / 16_777_215.0
}
fn noise(point: Vec3, seed: u64) -> f32 {
    let cell = point.floor().as_ivec3();
    // Use point-floor for negative coordinates too: the field wraps all poles.
    let f = point - point.floor();
    let f = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let sample = |x: i32, y: i32, z: i32| {
        unit(mix(seed
            ^ (i64::from(cell.x + x) as u64).wrapping_mul(0x8da6_b343)
            ^ (i64::from(cell.y + y) as u64).wrapping_mul(0xd816_3841)
            ^ (i64::from(cell.z + z) as u64).wrapping_mul(0xcb1a_b31f)))
    };
    let plane = |z| {
        let a = sample(0, 0, z).lerp(sample(1, 0, z), f.x);
        let b = sample(0, 1, z).lerp(sample(1, 1, z), f.x);
        a.lerp(b, f.y)
    };
    plane(0).lerp(plane(1), f.z)
}
pub(super) fn coverage(point: Vec3, seed: u64) -> f32 {
    noise(point / 75.0, seed ^ 0x8c2d_4e7a_51b3_9f06) * 0.70
        + noise(point / 24.0, seed ^ 0x1f3d_5b79_a7c9_e2d4) * 0.30
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_are_repeatable_equal_area_and_radially_upright() {
        for index in 0..1000 {
            let up = candidate(index, 1000, 42);
            assert!((up.length() - 1.0).abs() < 1.0e-6);
            assert_eq!(up, candidate(index, 1000, 42));
            assert_ne!(up, candidate(index, 1000, 43));
            let rotation = Quat::from_rotation_arc(Vec3::Z, up) * Quat::from_rotation_z(1.8);
            assert!((rotation * Vec3::Z).dot(up) > 0.99999);
            assert!((0.0..=1.0).contains(&coverage(up * 1000.0, 42)));
        }
    }

    #[test]
    fn beaches_channels_cliffs_and_snow_are_excluded_on_every_hemisphere() {
        for up in [Vec3::Y, -Vec3::Y, Vec3::X, -Vec3::Z] {
            let point = up * 1.02;
            assert!(suitable(point, up, [0.3, 0.8, 0.0, 0.0], 1000.0));
            assert!(!suitable(up * 1.001, up, [0.3, 0.8, 0.0, 1.0], 1000.0));
            assert!(!suitable(point, up, [0.3, 0.8, 0.2, 0.0], 1000.0));
            assert!(!suitable(
                point,
                up.any_orthonormal_vector(),
                [0.3, 0.8, 0.0, 0.0],
                1000.0
            ));
            assert!(!suitable(up * 1.11, up, [0.3, 0.8, 0.0, 0.0], 1000.0));
        }
    }

    #[test]
    fn placement_samples_the_displayed_mesh_and_interpolates_its_material() {
        let mut scene = crate::collision::test_scene();
        let terrain = std::sync::Arc::get_mut(&mut scene.terrain).unwrap();
        terrain.elevations.fill(0.025);
        terrain.soil_depth.fill(0.003);
        terrain.channel_depths.fill(0.0);
        let ground = Ground::new(&scene);
        for up in [Vec3::Y, -Vec3::Y, Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z] {
            let (point, normal, material) =
                ground.forest_sample(&scene.terrain, up, false).unwrap();
            let radius = ground.radius(&scene.terrain, up).unwrap();
            assert!(point.distance(up * radius) < 1.0e-6);
            assert!(normal.dot(up) > 0.95);
            assert!((material[1] - 0.84375).abs() < 1.0e-5);
            assert_eq!(material[2].to_bits(), 0.0_f32.to_bits());
        }
    }
}
