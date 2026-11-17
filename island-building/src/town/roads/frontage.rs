use super::{corridor, network::StreetLayout, strip_ends};
use crate::{
    BuildingRecipe, OpeningKind, OpeningRecipe,
    geometry::foundation,
    terrain_support::{FittedSupports, PlacementPose},
    town::{ClearanceRect, TownFrontage, TownInput, TownSettings, placement},
};
use motu::{
    Vec2,
    roads::{RoadNetwork, RoadStrip},
};

pub(in crate::town) struct Access {
    pub frontage: TownFrontage,
    pub clearance: ClearanceRect,
}

#[derive(Clone, Copy)]
pub(in crate::town) struct Lot {
    pub position: [f32; 2],
    pub yaw: f32,
    pub street_edge: Vec2,
}

pub(in crate::town) fn entrance(building: &BuildingRecipe) -> Option<&OpeningRecipe> {
    entrances(building).next()
}

fn entrances(building: &BuildingRecipe) -> impl Iterator<Item = &OpeningRecipe> {
    building
        .openings
        .iter()
        .filter(|door| door.kind == OpeningKind::Door && door.storey == 0)
}

pub(in crate::town) fn lot(
    roads: &RoadNetwork,
    building: &BuildingRecipe,
    door: &OpeningRecipe,
    settings: &TownSettings,
    [strip_index, side, along, setback]: [f32; 4],
) -> Option<Lot> {
    let config = settings.roads?;
    let strip = pick_strip(&roads.strips, strip_index)?;
    let (a, b) = strip_ends(strip);
    let outward = (b - a).normalize().perp() * if side < 0.5 { -1. } else { 1. };
    let facing = -outward;
    let angle = facing.y.atan2(facing.x);
    let [x, y, _] = door.facade.normal();
    let yaw = angle - y.atan2(x);
    // Keep a setback for the full landing and fitted steps. Vary the available
    // setback as well as the along-street position to fit steeper building lots.
    let min_setback = config.shoulder + building.foundation.outset + 1.35;
    let distance = min_setback + (config.max_frontage_distance - min_setback).max(0.) * setback;
    let edge = a.lerp(b, 0.05 + along * 0.9) + outward * (strip.half_width - 0.02);
    let door_xy = edge + outward * distance;
    let point = door.facade.point(building, door.offset, 0., 0.);
    let rotate = Vec2::from_angle(yaw);
    let position = door_xy - rotate.rotate(Vec2::new(point[0], point[1]));
    Some(Lot {
        position: position.to_array(),
        yaw,
        street_edge: edge,
    })
}

// The random stream supplies exactly 24 bits and the strip count is at most 4096.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn pick_strip(strips: &[RoadStrip], fraction: f32) -> Option<&RoadStrip> {
    strips.get((fraction * strips.len() as f32) as usize)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::town) fn central_approach(
    layout: &StreetLayout,
    building: &BuildingRecipe,
    supports: &FittedSupports,
    pose: PlacementPose,
    input: &TownInput,
    settings: &TownSettings,
    occupied: &[ClearanceRect],
) -> Option<(usize, Access)> {
    for door in entrances(building) {
        let door_world = world_door(building, door, pose);
        for strip in &layout.roads.strips {
            let (a, b) = strip_ends(strip);
            let (node, root, neighbour) = if layout.roots.contains(&Some(strip.start)) {
                (strip.start, a, b)
            } else if layout.roots.contains(&Some(strip.end)) {
                (strip.end, b, a)
            } else {
                continue;
            };
            let tangent = (neighbour - root).normalize();
            let across = tangent.perp();
            let radius = strip.half_width - 0.02;
            let offset = (door_world - root)
                .dot(across)
                .clamp(-radius * 0.8, radius * 0.8);
            let target =
                root - tangent * (radius * radius - offset * offset).sqrt() + across * offset;
            if let Some(access) = approach(
                &layout.roads,
                building,
                door,
                supports,
                pose,
                target,
                input,
                settings,
                occupied,
            ) {
                return Some((node, access));
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub(in crate::town) fn approach(
    roads: &RoadNetwork,
    building: &BuildingRecipe,
    door: &OpeningRecipe,
    supports: &FittedSupports,
    pose: PlacementPose,
    target: Vec2,
    input: &TownInput,
    settings: &TownSettings,
    occupied: &[ClearanceRect],
) -> Option<Access> {
    let config = settings.roads?;
    let door_world = world_door(building, door, pose);
    let delta = target - door_world;
    let [nx, ny, _] = door.facade.normal();
    let facing = Vec2::from_angle(pose.yaw).rotate(Vec2::new(nx, ny));
    if delta.length() > config.max_frontage_distance || delta.normalize_or_zero().dot(facing) < 0.8
    {
        return None;
    }
    let (toe, step_height) = ground_connection(building, door, supports, pose);
    let height = roads.surface_height(target)?;
    let run = toe.distance(target);
    if !(0.35..=config.max_frontage_distance).contains(&run) {
        return None;
    }
    let half_width = (door.width * 0.5).clamp(0.45, 0.75);
    let access = corridor(toe, target, half_width, 0.05);
    if placement::touches_river(&access, input, settings.river_margin)
        || placement::protected(&access, input)
        || occupied
            .iter()
            .any(|block| placement::overlaps(block, &access, 0.))
        || !walkable(
            roads,
            input,
            settings,
            toe,
            target,
            step_height,
            height,
            half_width,
        )
    {
        return None;
    }
    Some(Access {
        frontage: TownFrontage {
            door_id: door.id.clone(),
            street_edge: [target.x, target.y, height],
        },
        clearance: access,
    })
}

fn world_door(building: &BuildingRecipe, door: &OpeningRecipe, pose: PlacementPose) -> Vec2 {
    let world = pose.world_point(door.facade.point(building, door.offset, 0., 0.));
    Vec2::new(world[0], world[1])
}

fn ground_connection(
    building: &BuildingRecipe,
    door: &OpeningRecipe,
    supports: &FittedSupports,
    pose: PlacementPose,
) -> (Vec2, f32) {
    if let Some(stair) = supports
        .stairs
        .iter()
        .find(|stair| stair.door_id == door.id)
        && let Some(layout) = foundation::stairs(building).find(|stairs| stairs.door_id == door.id)
    {
        let run = f32::from(stair.steps) * stair.step_run + layout.tread_overhang + 0.035;
        let local = std::array::from_fn::<_, 2, _>(|axis| {
            layout.flight_origin[axis] + layout.direction[axis] * run
        });
        let height = layout.landing_height - f32::from(stair.steps - 1) * stair.step_rise;
        let world = pose.world_point([local[0], local[1], height]);
        return (Vec2::new(world[0], world[1]), world[2]);
    }
    let outset = building.foundation.outset + 0.02;
    let world = pose.world_point(door.facade.point(building, door.offset, door.sill, -outset));
    (Vec2::new(world[0], world[1]), world[2])
}

// Town validation bounds the approach to 12m, keeping this sample count small.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_arguments
)]
fn walkable(
    roads: &RoadNetwork,
    input: &TownInput,
    settings: &TownSettings,
    toe: Vec2,
    target: Vec2,
    step_height: f32,
    street_height: f32,
    half_width: f32,
) -> bool {
    let length = toe.distance(target);
    let samples = (length / settings.sample_spacing.min(0.2)).ceil() as u16;
    let step = length / f32::from(samples);
    let sideways = (target - toe).normalize().perp() * half_width;
    let max_gradient = settings.max_slope_degrees.to_radians().tan().min(0.33);
    let mut previous = [step_height; 3];
    for index in 0..=samples {
        let centre = toe.lerp(target, f32::from(index) / f32::from(samples));
        let tolerance = if index == 0 {
            0.23
        } else {
            max_gradient * step + 0.015
        };
        for (track, offset) in [-sideways, Vec2::ZERO, sideways].into_iter().enumerate() {
            let point = centre + offset;
            let Some(natural) = input.terrain.sample(point.x, point.y) else {
                return false;
            };
            let height = roads.graded_height(point, natural);
            if height <= input.sea_level + settings.min_dry_height
                || (height - previous[track]).abs() > tolerance
            {
                return false;
            }
            if index == samples && track == 1 && (height - street_height).abs() > 0.05 {
                return false;
            }
            if track > 0 && (height - previous[track - 1]).abs() > max_gradient * half_width + 0.02
            {
                return false;
            }
            previous[track] = height;
        }
    }
    true
}
