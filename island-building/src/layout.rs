//! Stable resolved structural descriptors shared by all LODs.
use crate::BuildingRecipe;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BuildingLayout {
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub footprint: [f32; 2],
    pub room_count: usize,
    pub entrance: [f32; 3],
}
impl BuildingLayout {
    #[must_use]
    pub fn contains(&self, point: [f32; 3], margin: f32) -> bool {
        (0..3).all(|i| {
            point[i] >= self.bounds_min[i] - margin && point[i] <= self.bounds_max[i] + margin
        })
    }
}
pub(crate) fn resolve(recipe: &BuildingRecipe) -> BuildingLayout {
    let opening = recipe
        .openings
        .iter()
        .find(|o| o.kind == crate::OpeningKind::Door)
        .expect("validated exterior door");
    BuildingLayout {
        bounds_min: [
            -recipe.width * 0.5 - recipe.roof.overhang,
            -recipe.depth * 0.5 - recipe.roof.overhang,
            -recipe.floor_thickness,
        ],
        bounds_max: [
            recipe.width * 0.5 + recipe.roof.overhang,
            recipe.depth * 0.5 + recipe.roof.overhang,
            recipe.wall_height + recipe.roof.rise + recipe.roof.thickness,
        ],
        footprint: [recipe.width, recipe.depth],
        room_count: recipe.partitions.len() + 1,
        entrance: opening.facade.point(recipe, opening.offset, 0., 0.),
    }
}
