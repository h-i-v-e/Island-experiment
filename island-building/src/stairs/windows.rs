//! Resolve window elevations after the fixed stair and entrance placement.
use crate::{
    BuildingRecipe, Facade, OpeningKind, OpeningRecipe, StaircaseLayout, StairwellSide,
    collision::BoxCollider,
};
use std::borrow::Cow;

const LANDING_GAP: f32 = 0.02;
const EPSILON: f32 = 0.00001;

pub(crate) fn fit(recipe: &mut Cow<'_, BuildingRecipe>, stairs: &StaircaseLayout) {
    // Plan against the original opening list before mutating its elevations or
    // removing entries. Only changed windows need an owned edit record.
    let changes: Vec<_> = recipe
        .openings
        .iter()
        .enumerate()
        .filter(|(_, opening)| opening.kind == OpeningKind::Window)
        .filter_map(|(index, opening)| {
            let sill = resolve_sill(recipe, opening, stairs);
            (!sill.is_some_and(|s| (s - opening.sill).abs() < EPSILON)).then_some((index, sill))
        })
        .collect();
    if changes.is_empty() {
        return;
    }
    let openings = &mut recipe.to_mut().openings;
    for (index, sill) in changes.into_iter().rev() {
        if let Some(sill) = sill {
            openings[index].sill = sill;
        } else {
            openings.remove(index);
        }
    }
}

fn resolve_sill(
    recipe: &BuildingRecipe,
    opening: &OpeningRecipe,
    stairs: &StaircaseLayout,
) -> Option<f32> {
    if !visible_from_stairs(stairs, opening.facade) {
        return Some(opening.sill);
    }
    let frame = crate::geometry::windows::bounds(recipe, opening);
    let base = recipe.floor_height(opening.storey);
    let drop = opening.sill - frame.min[2];
    let mut sill = opening.sill;
    loop {
        let delta = base + sill - opening.sill;
        let required = stairs
            .landings
            .iter()
            .chain(&stairs.turn_landings)
            .filter(|landing| {
                beside_window(recipe, opening.facade, frame, **landing)
                    && landing.max[2] + LANDING_GAP > frame.min[2] + delta + EPSILON
                    && landing.min[2] - LANDING_GAP < frame.max[2] + delta - EPSILON
            })
            .map(|landing| landing.max[2] + LANDING_GAP + drop - base)
            .fold(sill, f32::max);
        if required <= sill + EPSILON {
            break;
        }
        sill = required;
        if sill + opening.height > recipe.wall_height - 0.12 + EPSILON {
            return None;
        }
    }
    if sill > opening.sill + EPSILON
        && recipe.openings.iter().any(|other| {
            other.id != opening.id
                && other.storey == opening.storey
                && other.facade == opening.facade
                && (other.offset - opening.offset).abs()
                    < (other.width + opening.width) * 0.5 + 0.14
                && other.sill < sill + opening.height + 0.14 - EPSILON
                && sill < other.sill + other.height + 0.14 - EPSILON
        })
    {
        return None;
    }
    Some(sill)
}

fn visible_from_stairs(stairs: &StaircaseLayout, facade: Facade) -> bool {
    stairs.stairwell.as_ref().is_none_or(|room| {
        room.enclosure.is_some_and(|plan| {
            let origin = stairs.point(0., 0., 0.);
            let normal = facade.normal();
            StairwellSide::ALL.into_iter().any(|side| {
                let n = side.outward();
                let tip = stairs.point(n[0], n[1], 0.);
                plan.shared_sides[side.index()]
                    && (tip[0] - origin[0]) * normal[0] + (tip[1] - origin[1]) * normal[1] > 0.5
            })
        })
    })
}

fn beside_window(
    recipe: &BuildingRecipe,
    facade: Facade,
    frame: BoxCollider,
    landing: BoxCollider,
) -> bool {
    let normal = facade.normal();
    let axis = usize::from(normal[1].abs() > 0.5);
    let along = 1 - axis;
    let edge = if normal[axis] > 0. {
        landing.max[axis]
    } else {
        -landing.min[axis]
    };
    let wall = [recipe.width, recipe.depth][axis] * 0.5 - recipe.wall_thickness;
    wall - edge <= crate::geometry::internal_stairs::WALL_GUARD_CLEARANCE + EPSILON
        && frame.min[along] < landing.max[along] - EPSILON
        && frame.max[along] > landing.min[along] + EPSILON
}
