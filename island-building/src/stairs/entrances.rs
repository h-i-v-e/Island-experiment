//! Translate cached entrance leaves while testing stair placements; retain authoring offsets.
use crate::{BuildingRecipe, DoorDescriptor, OpeningKind, OpeningRecipe, collision::BoxCollider};

struct Entrance<'a> {
    opening: &'a OpeningRecipe,
    leaf: DoorDescriptor,
    hinge: [f32; 3],
    offset: f32,
}

pub(super) struct Entrances<'a>(Vec<Entrance<'a>>);

impl<'a> Entrances<'a> {
    pub fn new(recipe: &'a BuildingRecipe) -> Self {
        Self(
            recipe
                .openings
                .iter()
                .filter(|o| o.kind == OpeningKind::Door)
                .map(|opening| {
                    let leaf = crate::geometry::door_leaf(recipe, opening);
                    Entrance {
                        opening,
                        hinge: leaf.hinge,
                        leaf,
                        offset: opening.offset,
                    }
                })
                .collect(),
        )
    }

    pub fn fit(&mut self, recipe: &BuildingRecipe, shapes: &[BoxCollider]) -> bool {
        for i in 0..self.0.len() {
            let (earlier, remaining) = self.0.split_at_mut(i);
            let entrance = &mut remaining[0];
            let fits = crate::openings::entrance_candidates(recipe, entrance.opening, shapes)
                .into_iter()
                .any(|offset| {
                    let opening = entrance.opening;
                    if earlier.iter().any(|e| {
                        e.opening.facade == opening.facade
                            && (e.offset - offset).abs()
                                < (e.opening.width + opening.width) * 0.5 + 0.14 - 0.00001
                    }) {
                        return false;
                    }
                    let tangent = opening.facade.tangent();
                    entrance.leaf.hinge = std::array::from_fn(|axis| {
                        entrance.hinge[axis] + tangent[axis] * (offset - opening.offset)
                    });
                    let feet =
                        opening
                            .facade
                            .point(recipe, offset, 0.025, recipe.wall_thickness + 0.65);
                    if shapes.iter().any(|s| s.intersects_capsule(feet, 0.28, 1.8)) {
                        return false;
                    }
                    if crate::validation::validate_door_swings(
                        std::slice::from_ref(&entrance.leaf),
                        shapes,
                    )
                    .is_err()
                    {
                        return false;
                    }
                    entrance.offset = offset;
                    true
                });
            if !fits {
                return false;
            }
        }
        true
    }

    pub fn offsets(self) -> Vec<f32> {
        self.0.into_iter().map(|e| e.offset).collect()
    }
}
