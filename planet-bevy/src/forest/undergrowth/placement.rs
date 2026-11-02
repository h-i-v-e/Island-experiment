//! Generate once on the forest worker; all support queries borrow its ground BVH.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{Built, Kind, Placed};
use crate::{
    Scene,
    collision::Ground,
    command::BedView,
    forest::placement::{Plant, mean_temperature},
};
use bevy::prelude::*;
use std::{collections::HashMap, f32::consts::TAU};

const CELL: f32 = 4.0;
#[derive(Default)]
struct Occupied(HashMap<[i32; 3], Vec<(Vec3, f32)>>);
impl Occupied {
    fn insert(&mut self, point: Vec3, radius: f32) {
        self.0
            .entry((point / CELL).floor().as_ivec3().to_array())
            .or_default()
            .push((point, radius));
    }
    fn blocked(&self, point: Vec3, radius: f32) -> bool {
        let cell = (point / CELL).floor().as_ivec3();
        (-1..=1).any(|x| {
            (-1..=1).any(|y| {
                (-1..=1).any(|z| {
                    self.0
                        .get(&(cell + IVec3::new(x, y, z)).to_array())
                        .is_some_and(|items| {
                            items.iter().any(|&(other, r)| {
                                point.distance_squared(other) < (radius + r).powi(2)
                            })
                        })
                })
            })
        })
    }
}
pub(super) fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
fn unit(key: u64) -> f32 {
    (mix(key) >> 40) as f32 / 16_777_216.0
}
fn sample(scene: &Scene, ground: &Ground, direction: Vec3) -> Option<(Vec3, Vec3)> {
    let (point, normal, material) = ground.forest_sample(
        &scene.terrain,
        direction.normalize(),
        scene.bed == BedView::Original,
    )?;
    let radius = scene.terrain.topology.radius as f32;
    ((point.length() - 1.0) * radius > 1.0
        && normal.dot(point.normalize()) > 0.84
        && material[1] > 0.20
        && material[2] < 0.025)
        .then_some((point, normal))
}
pub(in crate::forest) fn build(scene: &Scene, ground: &Ground, trees: &[Plant]) -> Built {
    let radius = scene.terrain.topology.radius as f32;
    let log_models: Vec<_> = (0..8)
        .map(|i| motu::generate_fallen_log(mix(scene.seed ^ (i + 0x6c6f_6773))))
        .collect();
    let fern_models = (0..4)
        .map(|i| motu::generate_fern(mix(scene.seed ^ (i + 0x6665_726e))))
        .collect();
    let mut props = Vec::new();
    let mut obstacles = Occupied::default();
    for tree in trees {
        let scale = tree.transform.scale.x * radius / motu::ISLAND_WORLD_METRES;
        obstacles.insert(
            tree.root * radius + tree.root.normalize() * 0.35,
            0.35 * scale + 0.15,
        );
    }
    for (i, tree) in trees.iter().enumerate() {
        let key = mix(scene.seed ^ i as u64 ^ 0x6465_6164_776f_6f64);
        if unit(key) > 0.09 || props.len() >= 6000 {
            continue;
        }
        let up = tree.root.normalize();
        let side = up.any_orthonormal_vector();
        for attempt in 0..4 {
            let key = mix(key ^ attempt);
            let variant = (mix(key) % log_models.len() as u64) as usize;
            let model = &log_models[variant];
            let offset = unit(key ^ 1) * TAU;
            let centre = tree.root
                + (side * offset.cos() + up.cross(side) * offset.sin())
                    * ((3.0 + unit(key ^ 2) * 7.0) / radius);
            let yaw = unit(key ^ 3) * TAU;
            let axis = side * yaw.cos() + up.cross(side) * yaw.sin();
            let Some(transform) = fit_log(scene, ground, centre, axis, model, &obstacles) else {
                continue;
            };
            for step in 0..=16 {
                let local = Vec3::new(
                    (step as f32 / 16.0 - 0.5) * model.length_metres,
                    0.0,
                    model.radius_metres,
                ) / motu::ISLAND_WORLD_METRES;
                obstacles.insert(
                    transform.transform_point(local) * radius,
                    model.radius_metres + 0.22,
                );
            }
            props.push(Placed {
                root: transform.translation,
                transform,
                kind: Kind::Log,
                variant,
            });
            break;
        }
    }
    place_ferns(scene, ground, trees, &obstacles, &mut props);
    Built {
        props,
        fern_models,
        log_models,
    }
}
fn place_ferns(
    scene: &Scene,
    ground: &Ground,
    trees: &[Plant],
    obstacles: &Occupied,
    props: &mut Vec<Placed>,
) {
    let radius = scene.terrain.topology.radius as f32;
    let logs = props.len();
    let mut spacing = Occupied::default();
    for (i, tree) in trees.iter().enumerate() {
        let up = tree.root.normalize();
        let temperature = mean_temperature(up, (tree.root.length() - 1.0) * radius, scene.seed);
        if temperature < 1.0 {
            continue;
        }
        let key = mix(scene.seed ^ i as u64 ^ 0x6665_726e_6265_6473);
        let thinning = (54_000.0 / (trees.len() as f32 * 3.25)).min(1.0);
        if unit(key) < 0.35 || unit(key ^ 0x6275_6467_6574) > thinning {
            continue;
        }
        let side = up.any_orthonormal_vector();
        let tree_scale = (tree.transform.scale.x * radius / motu::ISLAND_WORLD_METRES).sqrt();
        for clump in 0..5 {
            if props.len() - logs >= 60_000 {
                break;
            }
            let key = mix(key ^ clump);
            let yaw = unit(key) * TAU;
            let distance = (0.75 + unit(key ^ 1) * 1.4) * tree_scale;
            let position =
                tree.root + (side * yaw.cos() + up.cross(side) * yaw.sin()) * (distance / radius);
            let Some((point, normal)) = sample(scene, ground, position) else {
                continue;
            };
            if obstacles.blocked(point * radius, 0.22) || spacing.blocked(point * radius, 0.32) {
                continue;
            }
            let length = 0.45 + unit(key ^ 2) * 0.70;
            // Reject fronds bridging a cliff or dipping through a nearby bank.
            let clear = [side, -side].into_iter().all(|direction| {
                sample(scene, ground, point + direction * (length * 0.65 / radius))
                    .is_some_and(|(probe, _)| ((probe - point).dot(up) * radius).abs() < 0.28)
            });
            if !clear {
                continue;
            }
            spacing.insert(point * radius, 0.32);
            let root = point - up * (0.015 / radius);
            let rotation = Quat::from_rotation_arc(Vec3::Z, normal.lerp(up, 0.35).normalize())
                * Quat::from_rotation_z(yaw);
            props.push(Placed {
                root,
                transform: Transform::from_translation(root)
                    .with_rotation(rotation)
                    .with_scale(Vec3::splat(motu::ISLAND_WORLD_METRES * length / radius)),
                kind: Kind::Fern,
                variant: (mix(key ^ 3) % 4) as usize,
            });
        }
    }
}

fn fit_log(
    scene: &Scene,
    ground: &Ground,
    centre: Vec3,
    direction: Vec3,
    model: &motu::FallenLogPrototype,
    obstacles: &Occupied,
) -> Option<Transform> {
    let radius = scene.terrain.topology.radius as f32;
    let half = model.length_metres * 0.5 / radius;
    let (a, _) = sample(scene, ground, centre - direction * half)?;
    let (b, _) = sample(scene, ground, centre + direction * half)?;
    let axis = (b - a).normalize();
    let up = centre.normalize();
    let up = (up - axis * axis.dot(up)).normalize();
    let mut origin = (a + b) * 0.5;
    let mut raise = f32::NEG_INFINITY;
    let mut maximum_gap = f32::NEG_INFINITY;
    for step in 0..=8 {
        let t = step as f32 / 8.0;
        let line = origin + axis * ((t - 0.5) * model.length_metres / radius);
        if obstacles.blocked(
            line * radius + up * model.radius_metres,
            model.radius_metres + 0.22,
        ) {
            return None;
        }
        for side in [-0.7, 0.0, 0.7] {
            let probe = line + up.cross(axis) * (side * model.radius_metres / radius);
            let (support, _) = sample(scene, ground, probe)?;
            let gap = (line - support).dot(up) * radius + model.radius_metres * (0.22 * t);
            raise = raise.max(0.015 - gap);
            maximum_gap = maximum_gap.max(gap);
        }
    }
    // Keep the lowest cylinder within 24 cm of all sampled terrain: no floating
    // shafts or long bridges across hollows. Broken roots can bury naturally.
    if maximum_gap + raise > (model.radius_metres * 0.65).min(0.24) {
        return None;
    }
    // Island rests its axis at 0.72 radii above support. The native model
    // already includes one radius, so bury the remaining 0.28 here as well.
    origin += up * ((raise - model.radius_metres * 0.28) / radius);
    Some(
        Transform::from_translation(origin)
            .with_rotation(Quat::from_mat3(&Mat3::from_cols(axis, up.cross(axis), up)))
            .with_scale(Vec3::splat(motu::ISLAND_WORLD_METRES / radius)),
    )
}
