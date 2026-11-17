//! Clean interior daub and the same non-overlapping timber layout as exterior walls.
use super::{Builder, slabs, timber_frame};
use crate::{
    BuildingRecipe, Facade, MaterialSlot, OpeningKind, OpeningRecipe, RoofCovering, RoofKind,
    WallStyle, collision::BoxCollider,
};

#[derive(Clone, Copy)]
pub(crate) struct Doorway {
    /// Absolute coordinate along the wall's longer horizontal axis.
    pub centre: f32,
    pub width: f32,
    pub height: f32,
}

pub(crate) fn axis(bounds: BoxCollider) -> usize {
    usize::from(bounds.max[1] - bounds.min[1] > bounds.max[0] - bounds.min[0])
}

pub(crate) fn solids(bounds: BoxCollider, door: Option<Doorway>) -> [BoxCollider; 3] {
    let Some(door) = door else {
        return [
            bounds,
            BoxCollider {
                min: bounds.min,
                max: bounds.min,
            },
            BoxCollider {
                min: bounds.min,
                max: bounds.min,
            },
        ];
    };
    let along = axis(bounds);
    let mut left = bounds;
    left.max[along] = door.centre - door.width * 0.5;
    let mut right = bounds;
    right.min[along] = door.centre + door.width * 0.5;
    let mut lintel = bounds;
    lintel.min[along] = left.max[along];
    lintel.max[along] = right.min[along];
    lintel.min[2] += door.height;
    [left, right, lintel]
}

pub(crate) fn generate(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    bounds: BoxCollider,
    door: Option<Doorway>,
    colliders: &mut Vec<BoxCollider>,
) {
    let shapes = solids(bounds, door);
    let start = builder.mesh(MaterialSlot::Interior).indices.len();
    slabs::joined_boxes(builder, MaterialSlot::Interior, &shapes);
    let mesh = builder.mesh(MaterialSlot::Interior);
    let along = axis(bounds);
    let keep: Vec<_> = mesh.indices[start..]
        .chunks_exact(3)
        .filter(|tri| {
            let points = [tri[0], tri[1], tri[2]].map(|i| mesh.positions[i as usize]);
            // Floors, ceilings and adjoining walls own the perimeter caps.
            ![
                (2, bounds.min[2]),
                (2, bounds.max[2]),
                (along, bounds.min[along]),
                (along, bounds.max[along]),
            ]
            .iter()
            .any(|&(axis, plane)| points.iter().all(|p| (p[axis] - plane).abs() < 0.00001))
        })
        .flatten()
        .copied()
        .collect();
    mesh.indices.truncate(start);
    mesh.indices.extend(keep);
    let mut frame_bounds = bounds;
    frame_bounds.max[2] = frame_bounds.max[2].min(bounds.min[2] + super::ceiling_height(recipe));
    decorate(builder, recipe, frame_bounds, door);
    colliders.extend(
        shapes
            .into_iter()
            .filter(|b| (0..3).all(|i| b.max[i] > b.min[i])),
    );
}

pub(crate) fn decorate(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    bounds: BoxCollider,
    door: Option<Doorway>,
) {
    if recipe.walls.style != WallStyle::WattleAndDaub {
        return;
    }
    let along = axis(bounds);
    let across = 1 - along;
    // Only the small authoring recipe is copied. Image/mesh buffers remain shared.
    let mut local = recipe.clone();
    local.width = bounds.max[along] - bounds.min[along];
    local.depth = bounds.max[across] - bounds.min[across];
    local.wall_height = bounds.max[2] - bounds.min[2];
    local.wall_thickness = local.depth;
    local.storeys = 1;
    local.roof.kind = RoofKind::Gable;
    local.roof.covering = RoofCovering::StandingSeam;
    local.roof.rise = 0.;
    local.openings.clear();
    if let Some(door) = door {
        local.openings.push(OpeningRecipe {
            id: "interior-frame-opening".into(),
            storey: 0,
            facade: Facade::South,
            kind: OpeningKind::Door,
            offset: door.centre - (bounds.min[along] + bounds.max[along]) * 0.5,
            width: door.width,
            height: door.height,
            sill: 0.,
            use_default_dimensions: false,
            window_panes: None,
            hinge: crate::HingeSide::Left,
            swing_degrees: 90.,
            door_style: crate::DoorStyle::Plank,
        });
    }
    let mut frame = Builder::default();
    timber_frame::interior_panel(&mut frame, &local);
    let mut lod = frame.finish();
    for part in &mut lod.parts {
        for p in &mut part.mesh.positions {
            let [u, v, z] = *p;
            p[along] = (bounds.min[along] + bounds.max[along]) * 0.5 + u;
            p[across] =
                (bounds.min[across] + bounds.max[across]) * 0.5 + if along == 0 { v } else { -v };
            p[2] = bounds.min[2] + z;
        }
        if along == 1 {
            for n in &mut part.mesh.normals {
                let [x, y, z] = *n;
                *n = [-y, x, z];
            }
        }
    }
    builder.append(lod, 0.);
}
