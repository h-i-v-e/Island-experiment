//! Rank both axes together by distance to usable exterior walls before fitting entrances.
use super::{StairAxis, layout, placement_offsets};
use crate::{BuildingRecipe, Facade, WallStyle};

pub(super) struct Candidate {
    pub axis: StairAxis,
    pub position: [f32; 2],
    clearance: [f32; 2],
}

pub(super) fn candidates(recipe: &BuildingRecipe) -> Vec<Candidate> {
    let mut result = Vec::new();
    for axis in [StairAxis::Y, StairAxis::X] {
        if recipe.staircase.axis != StairAxis::Auto && recipe.staircase.axis != axis {
            continue;
        }
        axis_candidates(recipe, axis, &mut result);
    }
    result.sort_by(|a, b| {
        a.clearance[0]
            .total_cmp(&b.clearance[0])
            .then_with(|| a.clearance[1].total_cmp(&b.clearance[1]))
    });
    result
}

fn axis_candidates(recipe: &BuildingRecipe, axis: StairAxis, result: &mut Vec<Candidate>) {
    let shape = layout(recipe, axis, [0.; 2], 0., false);
    let half =
        std::array::from_fn::<_, 2, _>(|i| (shape.bounds.max[i] - shape.bounds.min[i]) * 0.5);
    let projection = if recipe.walls.style == WallStyle::WattleAndDaub {
        recipe.walls.frame_projection
    } else {
        0.
    };
    let margin = recipe.wall_thickness + projection + 0.04;
    let max = [
        recipe.width * 0.5 - margin - half[0],
        recipe.depth * 0.5 - margin - half[1],
    ];
    if max.iter().any(|v| *v < 0.) {
        return;
    }
    let min = max.map(|r| -r);
    let automatic = recipe.staircase.position.is_none();
    let positions = recipe.staircase.position.into_iter().chain(
        automatic
            .then(|| placement_offsets(min[0], max[0]))
            .into_iter()
            .flatten()
            .flat_map(|x| {
                placement_offsets(min[1], max[1])
                    .into_iter()
                    .map(move |y| [x, y])
            }),
    );
    for position in positions {
        if (0..2).any(|i| position[i] < min[i] - 0.00001 || position[i] > max[i] + 0.00001) {
            continue;
        }
        let mut gaps = Facade::ALL.map(|facade| {
            if recipe.chimney.as_ref().is_some_and(|c| c.facade == facade) {
                return f32::INFINITY;
            }
            let normal = facade.normal();
            let i = usize::from(normal[1].abs() > 0.5);
            let gap = [recipe.width, recipe.depth][i] * 0.5
                - recipe.wall_thickness
                - projection
                - half[i]
                - normal[i] * position[i];
            // Treat floating-point variants of the same construction clearance as ties.
            (gap * 10000.).round() / 10000.
        });
        gaps.sort_by(f32::total_cmp);
        result.push(Candidate {
            axis,
            position,
            clearance: [gaps[0], gaps[0] + gaps[1]],
        });
    }
}
