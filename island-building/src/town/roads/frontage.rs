use super::{corridor, network::StreetLayout, strip_ends};
use crate::{
    BuildingRecipe, OpeningKind, OpeningRecipe, StairRun,
    collision::BoxCollider,
    geometry::foundation,
    terrain_support::{FittedSupports, PlacementPose, StairTarget},
    town::{ClearanceRect, TownFrontage, TownInput, TownSettings, placement},
};
use motu::{
    Vec2,
    roads::{RoadNetwork, RoadStrip},
};
use std::collections::VecDeque;

const EDGE_INSET: f32 = 0.02;
pub(in crate::town) const PAVED_TREAD_OVERLAP: f32 = 0.15;
const TOE_PAST_EDGE: f32 = PAVED_TREAD_OVERLAP - EDGE_INSET;
const FRONTAGE_STEP: f32 = 0.4;
// The NavMesh allows 45° slopes, or one metre of rise per metre of run.
const MAX_WALKABLE_GRADIENT: f32 = 1.0;

pub(in crate::town) struct Access {
    pub frontage: TownFrontage,
    pub clearance: ClearanceRect,
}

#[derive(Clone, Copy)]
pub(in crate::town) struct Lot {
    pub position: [f32; 2],
    pub yaw: f32,
    pub street_edge: Vec2,
    pub road_steps: Option<u16>,
}

#[derive(Default)]
pub(in crate::town) struct Lots {
    blocks: Vec<VecDeque<Lot>>,
    cursor: usize,
}

impl Lots {
    pub fn start_at(&mut self, seed: u64) {
        self.blocks.retain(|block| !block.is_empty());
        if !self.blocks.is_empty() {
            let index = seed % self.blocks.len() as u64;
            self.cursor = usize::try_from(index).expect("block index fits usize");
        }
    }
}

impl Iterator for Lots {
    type Item = Lot;

    fn next(&mut self) -> Option<Lot> {
        for _ in 0..self.blocks.len() {
            let block = &mut self.blocks[self.cursor];
            let lot = block.pop_front();
            // Test each setback at the current station before advancing. On
            // the next circuit, slide within each block to recover tight gaps.
            if lot.is_none_or(|lot| {
                block
                    .front()
                    .is_none_or(|next| next.street_edge != lot.street_edge)
            }) {
                self.cursor = (self.cursor + 1) % self.blocks.len();
            }
            if lot.is_some() {
                return lot;
            }
        }
        None
    }
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

#[derive(Clone, Copy)]
pub(in crate::town) struct Row {
    pub start: Vec2,
    pub end: Vec2,
    pub half_width: f32,
}

/// Join adjacent straight strips so one footprint can span several grading sections.
pub(in crate::town) fn rows(
    roads: &RoadNetwork,
    ring_count: usize,
    ring_index: &impl Fn(&RoadStrip) -> usize,
) -> Vec<Vec<Row>> {
    grouped_rows(roads, ring_count, ring_index, false)
}

/// A field can use the open side of a street across a junction with a side road.
pub(in crate::town) fn field_rows(roads: &RoadNetwork) -> Vec<Row> {
    grouped_rows(roads, 1, &|_| 0, true)
        .pop()
        .unwrap_or_default()
}

fn grouped_rows(
    roads: &RoadNetwork,
    ring_count: usize,
    ring_index: &impl Fn(&RoadStrip) -> usize,
    through_junctions: bool,
) -> Vec<Vec<Row>> {
    use std::collections::HashMap;
    let mut adjacent = HashMap::<usize, Vec<usize>>::new();
    for (index, strip) in roads.strips.iter().enumerate() {
        adjacent.entry(strip.start).or_default().push(index);
        adjacent.entry(strip.end).or_default().push(index);
    }
    let mut used = vec![false; roads.strips.len()];
    let mut rows = vec![Vec::new(); ring_count];
    for (index, strip) in roads.strips.iter().enumerate() {
        if std::mem::replace(&mut used[index], true) {
            continue;
        }
        let (start, end) = strip_ends(strip);
        let ring = ring_index(strip);
        let direction = (end - start).normalize();
        let mut row = Row {
            start,
            end,
            half_width: strip.half_width,
        };
        for (point, mut node, outward) in [
            (&mut row.start, strip.start, -direction),
            (&mut row.end, strip.end, direction),
        ] {
            loop {
                let neighbours = &adjacent[&node];
                if !through_junctions && neighbours.len() != 2 {
                    break;
                }
                let Some((next, far, far_node)) = neighbours.iter().copied().find_map(|next| {
                    let strip = &roads.strips[next];
                    if used[next]
                        || ring_index(strip) != ring
                        || (strip.half_width - row.half_width).abs() > 0.001
                    {
                        return None;
                    }
                    let (a, b) = strip_ends(strip);
                    let (near, far, far_node) = if strip.start == node {
                        (a, b, strip.end)
                    } else {
                        (b, a, strip.start)
                    };
                    let gap = if through_junctions {
                        (near - *point).perp_dot(outward).abs()
                    } else {
                        near.distance(*point)
                    };
                    (gap <= 0.004 && (far - near).normalize().dot(outward) >= 0.9999)
                        .then_some((next, far, far_node))
                }) else {
                    break;
                };
                used[next] = true;
                *point = far;
                node = far_node;
            }
        }
        rows[ring].push(row);
    }
    rows
}

/// Divide both sides of a street into footprint-wide blocks with setback alternatives.
pub(in crate::town) fn lots(
    rows: &[Row],
    building: &BuildingRecipe,
    door: &OpeningRecipe,
    footprint: BoxCollider,
    settings: &TownSettings,
) -> Lots {
    let Some(config) = settings.roads else {
        return Lots::default();
    };
    let local_door = Vec2::from_slice(&door.facade.point(building, door.offset, 0., 0.)[..2]);
    let normal = Vec2::from_slice(&door.facade.normal()[..2]);
    let direct_stair = foundation::stairs(building)
        .find(|stair| stair.door_id == door.id && stair.orientation == StairRun::Outward);
    let direct = direct_stair.into_iter().flat_map(|stair| {
        [1, 3, 6].into_iter().map(move |steps| {
            let reach = stair.landing + f32::from(steps) * stair.step_run + stair.tread_overhang;
            (reach - TOE_PAST_EDGE, Some(steps))
        })
    });
    let min_setback = config.shoulder_width() + building.foundation.outset + 1.35;
    let max_setback = config.max_frontage_distance;
    let setback = [
        min_setback,
        min_setback + 0.8,
        min_setback + 1.6,
        max_setback,
    ]
    .map(|distance| (distance, None));

    let offsets: Vec<_> = direct
        .filter(|(distance, _)| *distance <= max_setback)
        .chain(
            setback
                .into_iter()
                .filter(|(distance, _)| (min_setback..=max_setback).contains(distance)),
        )
        .collect();
    let mut lots = Lots::default();
    for row in rows {
        row_lots(
            *row,
            local_door,
            normal,
            footprint,
            settings.building_gap,
            &offsets,
            &mut lots.blocks,
        );
    }
    lots
}

fn row_lots(
    row: Row,
    door: Vec2,
    normal: Vec2,
    footprint: BoxCollider,
    gap: f32,
    offsets: &[(f32, Option<u16>)],
    blocks: &mut Vec<VecDeque<Lot>>,
) {
    let tangent = (row.end - row.start).normalize();
    let length = row.start.distance(row.end);
    for side in [-1., 1.] {
        let outward = tangent.perp() * side;
        let yaw = (-outward).to_angle() - normal.to_angle();
        let rotation = Vec2::from_angle(yaw);
        let along_local = Vec2::from_angle(-yaw).rotate(tangent);
        let [lo, hi] = along_bounds(footprint, door, along_local);
        let pitch = hi - lo + gap;
        let mut start = (0.25 - lo).max(0.05);
        let last = (length - hi - 0.25).min(length - 0.05);
        while start <= last + 0.001 {
            let mut candidates = VecDeque::new();
            let mut along = start;
            while along < start + pitch - 0.001 && along <= last + 0.001 {
                let edge = row.start + tangent * along + outward * (row.half_width - EDGE_INSET);
                candidates.extend(offsets.iter().map(|&(setback, road_steps)| Lot {
                    position: (edge + outward * setback - rotation.rotate(door)).to_array(),
                    yaw,
                    street_edge: edge,
                    road_steps,
                }));
                along += FRONTAGE_STEP;
            }
            blocks.push(candidates);
            start += pitch;
        }
    }
}

fn along_bounds(footprint: BoxCollider, door: Vec2, tangent: Vec2) -> [f32; 2] {
    let min = Vec2::from_slice(&footprint.min[..2]) - door;
    let max = Vec2::from_slice(&footprint.max[..2]) - door;
    let lower = (min * tangent).min(max * tangent);
    let upper = (min * tangent).max(max * tangent);
    [lower.element_sum(), upper.element_sum()]
}

pub(in crate::town) fn stair_target<'a>(
    roads: &RoadNetwork,
    door: &'a OpeningRecipe,
    lot: Lot,
) -> Option<StairTarget<'a>> {
    let steps = lot.road_steps?;
    let facing = door_facing(door, lot.yaw);
    let toe = lot.street_edge + facing * TOE_PAST_EDGE;
    let half_width = door.width * 0.5 + 0.15 + foundation::TREAD_OVERHANG;
    let across = facing.perp() * half_width;
    let grade = [-1., -0.5, 0., 0.5, 1.].into_iter().try_fold(
        [f32::INFINITY, f32::NEG_INFINITY],
        |[low, high], offset| {
            let height = roads.surface_height(toe + across * offset)?;
            Some([low.min(height), high.max(height)])
        },
    )?;
    Some(StairTarget {
        door_id: &door.id,
        steps,
        grade,
    })
}

pub(in crate::town) fn stair_access(
    roads: &RoadNetwork,
    door: &OpeningRecipe,
    lot: Lot,
    input: &TownInput,
    settings: &TownSettings,
    blocked: &impl Fn(&ClearanceRect) -> bool,
) -> Option<Access> {
    let facing = door_facing(door, lot.yaw);
    let height = roads.surface_height(lot.street_edge)?;
    let landing = lot.street_edge + facing * (TOE_PAST_EDGE + 0.35);
    let access = corridor(lot.street_edge, landing, door.width * 0.5 + 0.15, 0.05);
    available(&access, input, settings, blocked).then(|| Access {
        frontage: TownFrontage {
            door_id: door.id.clone(),
            street_edge: [lot.street_edge.x, lot.street_edge.y, height],
        },
        clearance: access,
    })
}

fn door_facing(door: &OpeningRecipe, yaw: f32) -> Vec2 {
    let [x, y, _] = door.facade.normal();
    Vec2::from_angle(yaw).rotate(Vec2::new(x, y))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::town) fn central_approach(
    layout: &StreetLayout,
    building: &BuildingRecipe,
    supports: &FittedSupports,
    pose: PlacementPose,
    input: &TownInput,
    settings: &TownSettings,
    blocked: &impl Fn(&ClearanceRect) -> bool,
) -> Option<(usize, Access)> {
    let limit = settings.roads?.max_frontage_distance + 12.;
    let mut best = None;
    let mut best_alignment = 0.8;
    for door in entrances(building) {
        let door_world = world_door(building, door, pose);
        let facing = door_facing(door, pose.yaw);
        for strip in layout.inner_strips() {
            for target in central_edges(&layout.roads, strip, door_world, facing) {
                let alignment = (target - door_world).normalize_or_zero().dot(facing);
                if alignment <= best_alignment {
                    continue;
                }
                if let Some(access) = approach(
                    &layout.roads,
                    building,
                    door,
                    supports,
                    pose,
                    target,
                    input,
                    settings,
                    blocked,
                    limit,
                ) {
                    if alignment > 0.999 {
                        return Some((strip.start, access));
                    }
                    best = Some((strip.start, access));
                    best_alignment = alignment;
                }
            }
        }
    }
    best
}

fn central_edges<'a>(
    roads: &'a RoadNetwork,
    strip: &'a RoadStrip,
    door: Vec2,
    facing: Vec2,
) -> impl Iterator<Item = Vec2> + 'a {
    let (start, end) = strip_ends(strip);
    let tangent = (end - start).normalize();
    let across = tangent.perp();
    let along = (door - start).dot(tangent).clamp(0., start.distance(end));
    let side = (door - start).dot(across).signum();
    let edge = start + tangent * along + across * side * (strip.half_width - 0.02);
    // A doorway can look straight at the junction between two strips.
    roads
        .junctions
        .iter()
        .filter(move |junction| junction.node == strip.start || junction.node == strip.end)
        .filter_map(move |junction| junction.ray_entry(door, facing, EDGE_INSET))
        .chain(std::iter::once(edge))
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
    blocked: &impl Fn(&ClearanceRect) -> bool,
    max_distance: f32,
) -> Option<Access> {
    let door_world = world_door(building, door, pose);
    let delta = target - door_world;
    let facing = door_facing(door, pose.yaw);
    if delta.length() > max_distance || delta.normalize_or_zero().dot(facing) < 0.8 {
        return None;
    }
    let (toe, step_height) = ground_connection(building, door, supports, pose);
    let height = roads.surface_height(target)?;
    let run = toe.distance(target);
    if !(0.35..=max_distance).contains(&run) {
        return None;
    }
    let half_width = (door.width * 0.5).clamp(0.45, 0.75);
    let access = corridor(toe, target, half_width, 0.05);
    if !available(&access, input, settings, blocked)
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

fn available(
    access: &ClearanceRect,
    input: &TownInput,
    settings: &TownSettings,
    blocked: &impl Fn(&ClearanceRect) -> bool,
) -> bool {
    !placement::touches_river(access, input, settings.river_margin)
        && !placement::protected(access, input)
        && !blocked(access)
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

// Town validation bounds approaches to 24m, keeping the sample count small.
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
    let mut previous = None;
    for index in 0..=samples {
        let centre = toe.lerp(target, f32::from(index) / f32::from(samples));
        let [Some(left), Some(middle), Some(right)] =
            [-sideways, Vec2::ZERO, sideways].map(|offset| {
                let point = centre + offset;
                let natural = input.terrain.sample(point.x, point.y)?;
                Some(roads.graded_height(point, natural))
            })
        else {
            return false;
        };
        let row = [left, middle, right];
        if row
            .iter()
            .any(|height| *height <= input.sea_level + settings.min_dry_height)
            || (index == samples && (middle - street_height).abs() > 0.05)
        {
            return false;
        }
        if let Some(prior) = previous {
            if !walkable_strip(prior, row, step, half_width) {
                return false;
            }
        } else if row
            .iter()
            .any(|height| (*height - step_height).abs() > 0.23)
        {
            return false;
        }
        previous = Some(row);
    }
    true
}

fn walkable_strip(prior: [f32; 3], row: [f32; 3], step: f32, half_width: f32) -> bool {
    // Use both triangles of each sampled quad to check the full slope.
    // The tolerance covers interpolation and floating-point rounding at 45°.
    let limit = MAX_WALKABLE_GRADIENT + 0.001;
    (0..2).all(|side| {
        let forward_left = (row[side] - prior[side]) / step;
        let forward_right = (row[side + 1] - prior[side + 1]) / step;
        let across_near = (prior[side + 1] - prior[side]) / half_width;
        let across_far = (row[side + 1] - row[side]) / half_width;
        forward_left.hypot(across_far) <= limit && forward_right.hypot(across_near) <= limit
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn door_approach_uses_the_navmesh_slope_limit() {
        use crate::town::HeightGrid;

        let settings = TownSettings {
            max_slope_degrees: 14.,
            ..TownSettings::default()
        };
        let roads = RoadNetwork::default();
        for (along, across, expected) in [
            (0.7, 0., true),
            (1., 0., true),
            (1.04, 0., false),
            (0.7, 0.7, true),
            (0.8, 0.8, false),
        ] {
            let heights = (0_u16..3).flat_map(|y| {
                (0_u16..4).map(move |x| 3. + f32::from(x) * along + (f32::from(y) - 1.) * across)
            });
            let input = TownInput {
                tile_seed: 0,
                sea_level: 0.,
                terrain: HeightGrid {
                    origin: [0., -1.],
                    spacing: 1.,
                    width: 4,
                    height: 3,
                    heights: heights.collect(),
                },
                rivers: vec![],
                bridge_exclusions: Vec::new(),
                disable_bridges: false,
                protected_terrain: vec![],
            };
            assert_eq!(
                walkable(
                    &roads,
                    &input,
                    &settings,
                    Vec2::ZERO,
                    Vec2::new(3., 0.),
                    3.,
                    3. + 3. * along,
                    0.25,
                ),
                expected,
                "along={along}, across={across}"
            );
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // Candidate positions are copied directly from the blocks.
    fn seeded_frontage_search_wraps_before_trying_offset_positions() {
        let lot = |along, setback| Lot {
            position: [along, setback],
            yaw: 0.,
            street_edge: Vec2::new(along, 0.),
            road_steps: None,
        };
        let mut candidates = Lots {
            blocks: vec![
                [lot(0., 1.), lot(0.4, 1.)].into(),
                [lot(6., 1.), lot(6., 2.), lot(6.4, 1.)].into(),
                [lot(12., 1.)].into(),
            ],
            ..Lots::default()
        };
        candidates.start_at(1);
        let positions: Vec<_> = candidates.map(|lot| lot.position).collect();
        assert_eq!(
            positions,
            [
                [6., 1.],
                [6., 2.],
                [12., 1.],
                [0., 1.],
                [6.4, 1.],
                [0.4, 1.]
            ]
        );
    }
}
