//! Resolve enclosure sides before geometry so rendering, doors and collision agree.
use super::{
    StaircaseLayout, StairwellDoorway, StairwellEnclosure, StairwellLayout, StairwellSide,
};
use crate::BuildingRecipe;

pub(super) fn resolve(
    recipe: &BuildingRecipe,
    stairs: &StaircaseLayout,
    unit: f32,
) -> Option<StairwellLayout> {
    let thickness = super::enclosure_thickness(recipe);
    let x = recipe.width * 0.5 - recipe.wall_thickness;
    let y = recipe.depth * 0.5 - recipe.wall_thickness;
    let corner_a = stairs.coordinates([-x, -y, 0.]);
    let corner_b = stairs.coordinates([x, y, 0.]);
    let exterior_min = [corner_a[0].min(corner_b[0]), corner_a[1].min(corner_b[1])];
    let exterior_max = [corner_a[0].max(corner_b[0]), corner_a[1].max(corner_b[1])];
    let close = |gap: f32| gap < unit - 0.00001 || gap.abs() < 0.00001;
    let shared = [
        close(-thickness - exterior_min[1]),
        close(exterior_max[0] - stairs.span() - thickness),
        close(exterior_max[1] - stairs.length() - thickness),
        close(-thickness - exterior_min[0]),
    ];
    let mut plan = StairwellEnclosure {
        min: [
            if shared[3] { exterior_min[0] } else { 0. },
            if shared[0] { exterior_min[1] } else { 0. },
        ],
        max: [
            if shared[1] {
                exterior_max[0]
            } else {
                stairs.span()
            },
            if shared[2] {
                exterior_max[1]
            } else {
                stairs.length()
            },
        ],
        shared_sides: shared,
        merge_unit: unit,
        door_side: StairwellSide::Near,
        door_offset: stairs.span() * 0.5,
    };
    let dimensions = recipe.opening_defaults.door;
    let (side, offset) = door_position(recipe, stairs, plan, dimensions.width)?;
    plan.door_side = side;
    plan.door_offset = offset;
    let height =
        recipe.floor_height(recipe.storeys - 1) + crate::geometry::top_storey_ceiling_top(recipe);
    let outer_min = [
        plan.min[0] - if shared[3] { 0. } else { thickness },
        plan.min[1] - if shared[0] { 0. } else { thickness },
    ];
    let outer_max = [
        plan.max[0] + if shared[1] { 0. } else { thickness },
        plan.max[1] + if shared[2] { 0. } else { thickness },
    ];
    let [across, along] = plan.surface(side, offset);
    let normal = side.outward();
    Some(StairwellLayout {
        bounds: stairs.rectangle(
            [outer_min[0], outer_min[1], 0.],
            [outer_max[0], outer_max[1], height],
        ),
        enclosure: Some(plan),
        wall_thickness: thickness,
        door_width: dimensions.width,
        door_height: dimensions.height,
        doorways: (0..recipe.storeys)
            .map(|storey| {
                let floor = recipe.floor_height(storey);
                StairwellDoorway {
                    storey,
                    centre: stairs.point(
                        across + normal[0] * thickness * 0.5,
                        along + normal[1] * thickness * 0.5,
                        floor + dimensions.height * 0.5,
                    ),
                    room_approach: stairs.point(
                        across + normal[0] * (thickness + 0.65),
                        along + normal[1] * (thickness + 0.65),
                        floor + 0.025,
                    ),
                    stair_approach: stairs.point(
                        across - normal[0] * 0.5,
                        along - normal[1] * 0.5,
                        floor + 0.025,
                    ),
                }
            })
            .collect(),
    })
}

fn door_position(
    recipe: &BuildingRecipe,
    stairs: &StaircaseLayout,
    plan: StairwellEnclosure,
    width: f32,
) -> Option<(StairwellSide, f32)> {
    let landing_centre = (plan.min[1] + stairs.landing_depth) * 0.5;
    [
        (
            StairwellSide::Near,
            plan.min[0],
            plan.max[0],
            stairs.span() * 0.5,
        ),
        (
            StairwellSide::Right,
            plan.min[1],
            stairs.landing_depth,
            landing_centre,
        ),
        (
            StairwellSide::Left,
            plan.min[1],
            stairs.landing_depth,
            landing_centre,
        ),
        (StairwellSide::Far, plan.min[0], 0., plan.min[0] * 0.5),
        (
            StairwellSide::Far,
            stairs.span(),
            plan.max[0],
            (stairs.span() + plan.max[0]) * 0.5,
        ),
    ]
    .into_iter()
    .filter(|(side, ..)| !plan.shared_sides[side.index()])
    .find_map(|(side, low, high, preferred)| {
        let low = low + width * 0.5 + 0.08;
        let high = high - width * 0.5 - 0.08;
        if low > high {
            return None;
        }
        let offset = preferred.clamp(low, high);
        door_clear(recipe, stairs, plan, side, offset, width).then_some((side, offset))
    })
}

fn door_clear(
    recipe: &BuildingRecipe,
    stairs: &StaircaseLayout,
    plan: StairwellEnclosure,
    side: StairwellSide,
    offset: f32,
    width: f32,
) -> bool {
    let normal = side.outward();
    let tangent = side.tangent();
    let [across, along] = plan.surface(side, offset);
    let thickness = super::enclosure_thickness(recipe);
    let edge = -width * 0.5 + 0.003;
    let hinge = [
        across + tangent[0] * edge + normal[0] * (thickness - 0.001),
        along + tangent[1] * edge + normal[1] * (thickness - 0.001),
    ];
    let inside = |p: [f32; 3], margin: f32| {
        p[0].abs() <= recipe.width * 0.5 - recipe.wall_thickness - margin
            && p[1].abs() <= recipe.depth * 0.5 - recipe.wall_thickness - margin
    };
    if !inside(
        stairs.point(
            across + normal[0] * (thickness + 0.65),
            along + normal[1] * (thickness + 0.65),
            0.,
        ),
        0.3,
    ) {
        return false;
    }
    (0..=32_u16).all(|i| {
        let (sin, cos) = (f32::from(i) / 32. * std::f32::consts::FRAC_PI_2).sin_cos();
        [0., width - 0.006].into_iter().all(|length| {
            [0., 0.055].into_iter().all(|depth| {
                let point = std::array::from_fn::<_, 2, _>(|j| {
                    hinge[j]
                        + (tangent[j] * cos + normal[j] * sin) * length
                        + (tangent[j] * sin - normal[j] * cos) * depth
                });
                inside(stairs.point(point[0], point[1], 0.), 0.025)
            })
        })
    })
}
