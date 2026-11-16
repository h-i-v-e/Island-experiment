//! Attached ground-floor rooms with shared materials and joined envelopes.
mod assemble;
mod atlas;
mod clip;
mod index;
mod junction;
mod node;
mod validation;

use crate::{
    BuildingError, BuildingPrototype, BuildingRecipe, Facade, WindowPanes, collision::BoxCollider,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    #[default]
    LeanTo,
    Wing,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExtensionRecipe {
    pub id: String,
    pub kind: ExtensionKind,
    pub facade: Facade,
    pub offset: f32,
    /// Width along the host wall; depth projects out from its outside face.
    pub width: f32,
    pub depth: f32,
    /// Lower eaves for a lean-to; both eaves for a gabled wing.
    pub wall_height: f32,
    pub roof_rise: f32,
    pub windows: bool,
    pub window_panes: Option<WindowPanes>,
}

impl Default for ExtensionRecipe {
    fn default() -> Self {
        Self {
            id: "extension".into(),
            kind: ExtensionKind::LeanTo,
            facade: Facade::North,
            offset: 0.,
            width: 3.,
            depth: 3.,
            wall_height: 2.4,
            roof_rise: 0.4,
            windows: true,
            window_panes: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExtensionLayout {
    pub id: String,
    pub kind: ExtensionKind,
    pub facade: Facade,
    pub bounds: BoxCollider,
    pub room_bounds: BoxCollider,
    pub connection: BoxCollider,
    #[serde(default)]
    pub window_centres: Vec<[f32; 3]>,
}

pub(crate) use validation::covered as covered_window;
pub(crate) use validation::diagnostics;

pub(crate) fn generate(
    recipe: &BuildingRecipe,
    base: &Path,
) -> Result<BuildingPrototype, BuildingError> {
    assemble::generate(recipe, base)
}

pub(crate) fn footprint(recipe: &BuildingRecipe) -> BoxCollider {
    footprints(recipe)
        .reduce(|mut a, b| {
            for axis in 0..2 {
                a.min[axis] = a.min[axis].min(b.min[axis]);
                a.max[axis] = a.max[axis].max(b.max[axis]);
            }
            a
        })
        .expect("main footprint")
}

pub(crate) fn footprints(recipe: &BuildingRecipe) -> impl Iterator<Item = BoxCollider> + '_ {
    let root = BoxCollider {
        min: [-recipe.width * 0.5, -recipe.depth * 0.5, 0.],
        max: [recipe.width * 0.5, recipe.depth * 0.5, 0.],
    };
    std::iter::once(root)
        .chain(recipe.extensions.iter().map(|extension| {
            let placement = node::placement(recipe, extension);
            placement.bounds(extension.width, extension.depth, recipe.wall_thickness)
        }))
        .map(|mut bounds| {
            for axis in 0..2 {
                bounds.min[axis] -= recipe.foundation.outset;
                bounds.max[axis] += recipe.foundation.outset;
            }
            bounds
        })
}
