//! Occupied masonry bases shared by standalone and terrain-fitted churches.
use super::plan::Plan;
use crate::{BuildingRecipe, collision::BoxCollider};

/// Zero-depth XY regions at their actual structural bearing heights. Floor
/// foundations meet the floor underside; isolated outer piers have their own
/// pads meeting the pier underside instead of filling the cruciform recesses.
pub(crate) fn footprints(recipe: &BuildingRecipe) -> Vec<BoxCollider> {
    let plan = Plan::new(recipe);
    let floor_top = -recipe.floor_thickness - 0.002;
    plan.floors()
        .map(|floor| floor.bounds(floor_top, floor_top))
        .chain(plan.buttresses().map(|layout| BoxCollider {
            min: layout.pier.min,
            max: [layout.pier.max[0], layout.pier.max[1], layout.pier.min[2]],
        }))
        .map(|mut bounds| {
            for axis in 0..2 {
                bounds.min[axis] -= recipe.foundation.outset;
                bounds.max[axis] += recipe.foundation.outset;
            }
            bounds
        })
        .collect()
}
