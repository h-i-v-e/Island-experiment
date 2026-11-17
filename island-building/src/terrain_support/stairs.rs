use super::{
    FittedStair, PlacementPose, StairTarget, SupportError, SupportOptions, burial, enclosing,
    sample_rectangle,
};
use crate::{
    BuildingRecipe, MaterialSlot, StairRun,
    collision::BoxCollider,
    geometry::{
        Builder,
        foundation::{self, StairLayout},
        slabs,
    },
};

pub(super) fn generate(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    pose: PlacementPose,
    sample: &impl Fn(f32, f32) -> Option<f32>,
    options: SupportOptions,
    target: Option<StairTarget<'_>>,
) -> Result<(Vec<BoxCollider>, Vec<FittedStair>), SupportError> {
    let mut colliders = Vec::new();
    let mut layouts = Vec::new();
    for stairs in foundation::stairs(recipe) {
        let target = target.filter(|target| target.door_id == stairs.door_id);
        let (shapes, steps, step_rise) =
            fitted_shapes(recipe, stairs, pose, sample, options, target)?;
        let bounds = enclosing(&shapes).expect("entrance landing");
        // Caps and bodies share one inseparable material/component. Union their
        // solids so there are no duplicate faces at their exact shared planes.
        slabs::joined_boxes(builder, MaterialSlot::Foundation, &shapes);
        colliders.extend(shapes);
        layouts.push(FittedStair {
            door_id: stairs.door_id.into(),
            steps,
            step_rise,
            step_run: foundation::STEP_RUN,
            bounds,
        });
    }
    Ok((colliders, layouts))
}

fn fitted_shapes(
    recipe: &BuildingRecipe,
    stairs: StairLayout<'_>,
    pose: PlacementPose,
    sample: &impl Fn(f32, f32) -> Option<f32>,
    options: SupportOptions,
    target: Option<StairTarget<'_>>,
) -> Result<(Vec<BoxCollider>, u16, f32), SupportError> {
    let cannot_reach = || SupportError::StairsCannotReachTerrain(stairs.door_id.into());
    let (steps, rise) = if let Some(target) = target {
        let [low, high] = target.grade;
        let street = f32::midpoint(low, high);
        let rise =
            (pose.origin[2] + stairs.landing_height - street) / (f32::from(target.steps) + 1.);
        // The visible last tread must be climbable across the full entrance width.
        if !(1..=options.max_stair_steps).contains(&target.steps)
            || !(0.06..=foundation::STEP_RISE).contains(&rise)
            || rise + street - high < 0.035
            || rise + street - low > 0.22
        {
            return Err(cannot_reach());
        }
        (target.steps, rise)
    } else {
        (options.max_stair_steps, foundation::STEP_RISE)
    };
    let mut shapes = Vec::new();
    let mut landing = stairs.landing_bounds;
    landing.max[2] = stairs.landing_height;
    let landing_ground = add_step(
        &mut shapes,
        recipe,
        stairs,
        landing,
        true,
        pose,
        sample,
        options,
    )?;
    if target.is_some() && landing.max[2] < landing_ground[1] + 0.01 {
        return Err(cannot_reach());
    }
    for step in 1..=steps {
        let body = flight_box(
            stairs,
            f32::from(step - 1) * foundation::STEP_RUN,
            f32::from(step) * foundation::STEP_RUN,
            stairs.landing_height - f32::from(step) * rise,
        );
        let ground = add_step(
            &mut shapes,
            recipe,
            stairs,
            body,
            false,
            pose,
            sample,
            options,
        )?;
        if target.is_some() {
            if body.max[2] < ground[1] + 0.01 {
                return Err(cannot_reach());
            }
            if step == steps {
                return Ok((shapes, step, rise));
            }
        } else if body.max[2] <= ground[0] + 0.001 {
            // The last naturally fitted tread is buried, limiting the visible riser.
            return Ok((shapes, step, rise));
        }
    }
    Err(cannot_reach())
}

#[allow(clippy::too_many_arguments)]
fn add_step(
    shapes: &mut Vec<BoxCollider>,
    recipe: &BuildingRecipe,
    stairs: StairLayout<'_>,
    mut body: BoxCollider,
    landing: bool,
    pose: PlacementPose,
    sample: &impl Fn(f32, f32) -> Option<f32>,
    options: SupportOptions,
) -> Result<[f32; 2], SupportError> {
    let cap = tread_bounds(stairs, body, landing);
    let ground = sample_rectangle(cap, pose, sample, options.sample_spacing)?;
    body.min[2] = (ground[0] - burial(recipe)).min(cap.min[2] - 0.02);
    body.max[2] = cap.min[2];
    shapes.extend([body, cap]);
    Ok(ground)
}

fn flight_box(stairs: StairLayout<'_>, start: f32, end: f32, top: f32) -> BoxCollider {
    let [dx, dy] = stairs.direction;
    let point = |side: f32, run: f32| {
        [
            stairs.flight_origin[0] + dx * run - dy * side * stairs.width,
            stairs.flight_origin[1] + dy * run + dx * side * stairs.width,
            top,
        ]
    };
    let a = point(-0.5, start);
    let b = point(0.5, end);
    BoxCollider {
        min: std::array::from_fn(|i| a[i].min(b[i])),
        max: std::array::from_fn(|i| a[i].max(b[i])),
    }
}

fn tread_bounds(stairs: StairLayout<'_>, body: BoxCollider, landing: bool) -> BoxCollider {
    let mut cap = body;
    cap.min[2] = cap.max[2] - foundation::TREAD_THICKNESS;
    for axis in 0..2 {
        cap.min[axis] -= foundation::TREAD_OVERHANG;
        cap.max[axis] += foundation::TREAD_OVERHANG;
        if landing || stairs.orientation == StairRun::AlongWall {
            let normal = stairs.facade.normal()[axis];
            if normal > 0.5 {
                cap.min[axis] = body.min[axis];
            }
            if normal < -0.5 {
                cap.max[axis] = body.max[axis];
            }
        }
    }
    cap
}
