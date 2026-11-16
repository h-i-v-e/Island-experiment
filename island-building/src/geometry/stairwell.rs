//! Interior partitions meet reused external walls without covering their windows.
use super::Builder;
use crate::{
    BuildingRecipe, DoorDescriptor, MaterialSlot, StaircaseLayout, StairwellEnclosure,
    StairwellSide, collision::BoxCollider,
};

fn plan(stairs: &StaircaseLayout) -> Option<StairwellEnclosure> {
    stairs.stairwell.as_ref().map(|s| {
        s.enclosure
            .unwrap_or_else(|| StairwellEnclosure::original(stairs))
    })
}

fn wall_box(
    stairs: &StaircaseLayout,
    side: StairwellSide,
    range: [f32; 2],
    z: [f32; 2],
    projection: f32,
) -> BoxCollider {
    let room = stairs.stairwell.as_ref().expect("enclosed wall");
    let plan = plan(stairs).expect("enclosed wall");
    let [a, b] = range;
    let [low, high] = z;
    let t = room.wall_thickness;
    let (min, max) = match side {
        StairwellSide::Near => (
            [a, plan.min[1] - t - projection, low],
            [b, plan.min[1] + projection, high],
        ),
        StairwellSide::Far => (
            [a, plan.max[1] - projection, low],
            [b, plan.max[1] + t + projection, high],
        ),
        StairwellSide::Left => (
            [plan.min[0] - t - projection, a, low],
            [plan.min[0] + projection, b, high],
        ),
        StairwellSide::Right => (
            [plan.max[0] - projection, a, low],
            [plan.max[0] + t + projection, b, high],
        ),
    };
    stairs.rectangle(min, max)
}

pub(crate) fn walls(recipe: &BuildingRecipe, stairs: &StaircaseLayout) -> Vec<BoxCollider> {
    let Some(room) = &stairs.stairwell else {
        return Vec::new();
    };
    let plan = plan(stairs).expect("enclosed wall");
    let t = room.wall_thickness;
    let mut walls = Vec::new();
    for side in StairwellSide::ALL {
        if plan.shared_sides[side.index()] {
            continue;
        }
        let [low, high] = if side.across() {
            [
                plan.min[0] - if plan.shared_sides[3] { 0. } else { t },
                plan.max[0] + if plan.shared_sides[1] { 0. } else { t },
            ]
        } else {
            [plan.min[1], plan.max[1]]
        };
        if side != plan.door_side {
            walls.push(wall_box(
                stairs,
                side,
                [low, high],
                [0., room.bounds.max[2]],
                0.,
            ));
            continue;
        }
        let left = plan.door_offset - room.door_width * 0.5;
        let right = left + room.door_width;
        for range in [[low, left], [right, high]] {
            walls.push(wall_box(stairs, side, range, [0., room.bounds.max[2]], 0.));
        }
        for storey in 0..recipe.storeys {
            let top = if storey + 1 == recipe.storeys {
                room.bounds.max[2]
            } else {
                // This cap sits beneath the next doorway. Keep its upward face
                // below the floorboards instead of coplanar with their surface.
                recipe.floor_height(storey + 1) - super::DOOR_FRAME_FLOOR_CLEARANCE
            };
            walls.push(wall_box(
                stairs,
                side,
                [left, right],
                [recipe.floor_height(storey) + room.door_height, top],
                0.,
            ));
        }
    }
    walls
}

pub(crate) fn doors(recipe: &BuildingRecipe, stairs: &StaircaseLayout) -> Vec<DoorDescriptor> {
    let Some(room) = &stairs.stairwell else {
        return Vec::new();
    };
    let plan = plan(stairs).expect("enclosed doorway");
    let normal = plan.door_side.outward();
    let tangent = plan.door_side.tangent();
    let [across, along] = plan.surface(plan.door_side, plan.door_offset);
    let origin = stairs.point(0., 0., 0.);
    let world_tangent = stairs.point(tangent[0], tangent[1], 0.);
    let angle = (world_tangent[1] - origin[1]).atan2(world_tangent[0] - origin[0]);
    room.doorways
        .iter()
        .map(|doorway| {
            let edge = -room.door_width * 0.5 + 0.003;
            let depth = room.wall_thickness - 0.001;
            let hinge = stairs.point(
                across + tangent[0] * edge + normal[0] * depth,
                along + tangent[1] * edge + normal[1] * depth,
                recipe.floor_height(doorway.storey) + 0.006,
            );
            super::door_leaf_at(
                format!("stairwell-door-{}", doorway.storey),
                [room.door_width, room.door_height],
                hinge,
                [angle, -std::f32::consts::FRAC_PI_2],
                recipe.staircase.door_style,
                false,
                0.5,
            )
        })
        .collect()
}

pub(crate) fn generate(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    stairs: &StaircaseLayout,
    colliders: &mut Vec<BoxCollider>,
    leaves: &mut Vec<DoorDescriptor>,
) {
    let Some(room) = &stairs.stairwell else {
        return;
    };
    for wall in walls(recipe, stairs) {
        builder.cube(MaterialSlot::Interior, wall.min, wall.max);
        colliders.push(wall);
    }
    let plan = plan(stairs).expect("enclosed doorway");
    let left = plan.door_offset - room.door_width * 0.5;
    let right = left + room.door_width;
    for doorway in &room.doorways {
        let z = recipe.floor_height(doorway.storey);
        let bottom = z + super::DOOR_FRAME_FLOOR_CLEARANCE;
        for (range, heights) in [
            ([left - 0.055, left], [bottom, z + room.door_height + 0.055]),
            (
                [right, right + 0.055],
                [bottom, z + room.door_height + 0.055],
            ),
            (
                [left, right],
                [z + room.door_height, z + room.door_height + 0.055],
            ),
        ] {
            let trim = wall_box(stairs, plan.door_side, range, heights, 0.025);
            builder.cube(MaterialSlot::Trim, trim.min, trim.max);
        }
    }
    leaves.extend(doors(recipe, stairs));
}
