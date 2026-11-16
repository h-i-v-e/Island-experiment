//! Stable resolved structural descriptors shared by all LODs.
use crate::BuildingRecipe;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BuildingLayout {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<crate::ExtensionLayout>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occupied_bounds: Vec<crate::collision::BoxCollider>,
    #[serde(default = "crate::recipe::one_storey")]
    pub storeys: u8,
    #[serde(default)]
    pub floor_heights: Vec<f32>,
    #[serde(default)]
    pub staircase: Option<crate::StaircaseLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub church: Option<crate::ChurchLayout>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub footprint: [f32; 2],
    pub room_count: usize,
    pub entrance: [f32; 3],
}
impl BuildingLayout {
    #[must_use]
    pub fn contains(&self, point: [f32; 3], margin: f32) -> bool {
        if !self.occupied_bounds.is_empty() {
            return self.occupied_bounds.iter().any(|bounds| {
                (0..3).all(|i| {
                    point[i] >= bounds.min[i] - margin && point[i] <= bounds.max[i] + margin
                })
            });
        }
        (0..3).all(|i| {
            point[i] >= self.bounds_min[i] - margin && point[i] <= self.bounds_max[i] + margin
        })
    }
}
pub(crate) fn resolve(
    recipe: &BuildingRecipe,
    staircase: Option<crate::StaircaseLayout>,
) -> BuildingLayout {
    let straw_margin = if recipe.roof.covering == crate::RoofCovering::Thatch {
        0.025
    } else {
        0.
    };
    // Door/window trims project 45 mm even when the roof has no overhang.
    let exterior_margin = (recipe.roof.overhang + straw_margin).max(0.045);
    let depth_margin = exterior_margin
        + if recipe.roof.covering == crate::RoofCovering::Thatch {
            crate::geometry::thatch::RIDGE_PROJECTION
        } else {
            0.
        };
    let opening = recipe
        .openings
        .iter()
        .find(|o| o.kind == crate::OpeningKind::Door)
        .expect("validated exterior door");
    let stair_room = usize::from(staircase.as_ref().is_some_and(|s| s.stairwell.is_some()));
    let mut layout = BuildingLayout {
        extensions: Vec::new(),
        occupied_bounds: Vec::new(),
        storeys: recipe.storeys,
        floor_heights: (0..recipe.storeys)
            .map(|i| recipe.floor_height(i))
            .collect(),
        staircase,
        church: None,
        bounds_min: [
            -recipe.width * 0.5 - exterior_margin,
            -recipe.depth * 0.5 - depth_margin,
            -recipe.floor_thickness,
        ],
        bounds_max: [
            recipe.width * 0.5 + exterior_margin,
            recipe.depth * 0.5 + depth_margin,
            recipe.eaves_height() + recipe.roof.rise + recipe.roof.effective_thickness(),
        ],
        footprint: [recipe.width, recipe.depth],
        room_count: recipe.partitions.len() + usize::from(recipe.storeys) + stair_room,
        entrance: opening.facade.point(recipe, opening.offset, 0., 0.),
    };
    if recipe.roof.covering != crate::RoofCovering::StandingSeam {
        let edge = recipe.width * 0.5 + recipe.roof.overhang;
        layout.bounds_max[2] = [-edge, 0., edge]
            .into_iter()
            .map(|x| crate::geometry::chimney::roof_height(recipe, x))
            .fold(layout.bounds_max[2], f32::max)
            + recipe.roof.covering.detail_height();
    }
    if let Some(stack) = crate::geometry::chimney::stack_bounds(recipe) {
        for axis in 0..2 {
            layout.bounds_min[axis] = layout.bounds_min[axis].min(stack.min[axis] - 0.06);
            layout.bounds_max[axis] = layout.bounds_max[axis].max(stack.max[axis] + 0.06);
        }
        // Shed eaves can rise beyond the wall-line roof height. Include the
        // complete sloped overhang and standing-seam relief in render bounds.
        let edge = recipe.width * 0.5 + recipe.roof.overhang;
        let roof_top = crate::geometry::chimney::roof_height(recipe, -edge)
            .max(crate::geometry::chimney::roof_height(recipe, edge))
            .max(crate::geometry::chimney::roof_height(recipe, 0.))
            + recipe.roof.covering.detail_height();
        layout.bounds_max[2] = layout.bounds_max[2].max(stack.max[2]).max(roof_top);
    }
    for opening in recipe
        .openings
        .iter()
        .filter(|o| o.kind == crate::OpeningKind::Window)
    {
        let bounds = crate::geometry::windows::bounds(recipe, opening);
        let mut bounds = bounds;
        bounds.min[2] += recipe.floor_height(opening.storey);
        bounds.max[2] += recipe.floor_height(opening.storey);
        for axis in 0..3 {
            layout.bounds_min[axis] = layout.bounds_min[axis].min(bounds.min[axis]);
            layout.bounds_max[axis] = layout.bounds_max[axis].max(bounds.max[axis]);
        }
    }
    let foundation = crate::geometry::foundation::bounds(recipe);
    for bounds in std::iter::once(foundation)
        .chain(crate::geometry::foundation::stairs(recipe).map(|stairs| stairs.bounds))
    {
        for axis in 0..3 {
            layout.bounds_min[axis] = layout.bounds_min[axis].min(bounds.min[axis]);
            layout.bounds_max[axis] = layout.bounds_max[axis].max(bounds.max[axis]);
        }
    }
    layout
}
