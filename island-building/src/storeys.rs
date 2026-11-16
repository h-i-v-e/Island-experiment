//! Assemble floor-local shells by moving their buffers into one immutable prototype.
use crate::{
    BuildingLod, BuildingRecipe, DoorDescriptor, MaterialSlot,
    collision::BoxCollider,
    geometry::{self, Builder},
    stairs::StaircaseLayout,
};

pub(crate) struct Structure {
    pub near: BuildingLod,
    pub doors: Vec<DoorDescriptor>,
    pub colliders: Vec<BoxCollider>,
    pub supports: Vec<BoxCollider>,
}

pub(crate) fn recipe_for_storey(recipe: &BuildingRecipe, storey: u8) -> BuildingRecipe {
    let mut local = recipe.clone();
    local.openings.retain(|o| o.storey == storey);
    local.partitions.retain(|p| p.storey == storey);
    for opening in &mut local.openings {
        opening.storey = 0;
    }
    for partition in &mut local.partitions {
        partition.storey = 0;
    }
    local.storeys = 1;
    local.opening_generation = None;
    if storey + 1 < recipe.storeys {
        local.roof.rise = 0.;
        local.roof.covering = crate::RoofCovering::StandingSeam;
    }
    local
}

pub(crate) fn near(
    recipe: &BuildingRecipe,
    roof_pitch: f32,
    stairs: Option<&StaircaseLayout>,
) -> Structure {
    if recipe.storeys == 1 {
        let (near, doors, colliders) = geometry::generate_with_roof_pitch(recipe, roof_pitch);
        return Structure {
            near,
            doors,
            colliders,
            supports: Vec::new(),
        };
    }
    let mut builder = Builder::default();
    geometry::foundation::generate(&mut builder, recipe, false);
    let mut result = Structure {
        near: BuildingLod::default(),
        doors: Vec::new(),
        colliders: vec![geometry::foundation::bounds(recipe)],
        supports: Vec::new(),
    };
    for storey in 0..recipe.storeys {
        let local = recipe_for_storey(recipe, storey);
        let height = recipe.floor_height(storey);
        let mut level = geometry::generate_level(
            &local,
            storey == 0,
            storey + 1 == recipe.storeys,
            roof_pitch,
            stairs,
        );
        builder.append(level.near, height);
        for door in &mut level.doors {
            door.hinge[2] += height;
        }
        result.doors.extend(level.doors);
        result
            .colliders
            .extend(level.colliders.into_iter().map(|b| raised(b, height)));
        result
            .supports
            .extend(level.supports.into_iter().map(|b| raised(b, height)));
    }
    geometry::chimney::near(&mut builder, recipe, &mut result.colliders);
    if let Some(stairs) = stairs {
        geometry::stairwell::generate(
            &mut builder,
            recipe,
            stairs,
            &mut result.colliders,
            &mut result.doors,
        );
        geometry::internal_stairs::generate(
            &mut builder,
            recipe,
            stairs,
            &mut result.colliders,
            &mut result.supports,
        );
    }
    result.near = builder.finish();
    result
}

pub(crate) fn middle(recipe: &BuildingRecipe) -> BuildingLod {
    let mut builder = Builder::default();
    geometry::foundation::generate(&mut builder, recipe, false);
    for storey in 0..recipe.storeys {
        let local = recipe_for_storey(recipe, storey);
        builder.append(
            crate::lod::middle_level(&local, storey + 1 == recipe.storeys),
            recipe.floor_height(storey),
        );
        if storey > 0 {
            exterior_band(&mut builder, recipe, recipe.floor_height(storey));
        }
    }
    geometry::chimney::middle(&mut builder, recipe);
    builder.finish()
}

fn exterior_band(builder: &mut Builder, recipe: &BuildingRecipe, top: f32) {
    for facade in crate::Facade::ALL {
        let timber = recipe.walls.style == crate::WallStyle::WattleAndDaub;
        let half = facade.length(recipe) * 0.5;
        let points = [
            facade.point(recipe, -half, top - recipe.floor_thickness, 0.),
            facade.point(recipe, half, top - recipe.floor_thickness, 0.),
            facade.point(recipe, half, top, 0.),
            facade.point(recipe, -half, top, 0.),
        ];
        builder.quad(
            if timber {
                MaterialSlot::Timber
            } else {
                MaterialSlot::Wall
            },
            points,
            points.map(|p| {
                let t = facade.tangent();
                let uv =
                    crate::units::wall_uv(facade.length(recipe), p[0] * t[0] + p[1] * t[1], p[2]);
                if timber {
                    [-(p[2] - top + recipe.floor_thickness) * 0.5, uv[0]]
                } else {
                    uv
                }
            }),
        );
    }
}

fn raised(mut bounds: BoxCollider, height: f32) -> BoxCollider {
    bounds.min[2] += height;
    bounds.max[2] += height;
    bounds
}

/// Split an axis-aligned slab or beam around multiple complete XY cutouts.
pub(crate) fn cells(bounds: BoxCollider, cuts: &[BoxCollider]) -> Vec<BoxCollider> {
    let mut xs = vec![bounds.min[0], bounds.max[0]];
    let mut ys = vec![bounds.min[1], bounds.max[1]];
    for cut in cuts {
        for value in [cut.min[0], cut.max[0]] {
            if value > bounds.min[0] && value < bounds.max[0] {
                xs.push(value);
            }
        }
        for value in [cut.min[1], cut.max[1]] {
            if value > bounds.min[1] && value < bounds.max[1] {
                ys.push(value);
            }
        }
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    let mut result = Vec::new();
    for x in xs.windows(2) {
        for y in ys.windows(2) {
            let centre = [(x[0] + x[1]) * 0.5, (y[0] + y[1]) * 0.5];
            if cuts
                .iter()
                .any(|c| (0..2).all(|i| centre[i] > c.min[i] && centre[i] < c.max[i]))
            {
                continue;
            }
            result.push(BoxCollider {
                min: [x[0], y[0], bounds.min[2]],
                max: [x[1], y[1], bounds.max[2]],
            });
        }
    }
    result
}
