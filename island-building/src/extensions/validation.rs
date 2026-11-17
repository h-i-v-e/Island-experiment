use super::{ExtensionRecipe, node};
use crate::{BuildingRecipe, Diagnostic, OpeningKind, collision::BoxCollider};
use std::collections::BTreeSet;

pub(crate) fn diagnostics(recipe: &BuildingRecipe) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    if recipe.extensions.is_empty() {
        return errors;
    }
    if recipe.church.is_some() || recipe.extensions.len() > 8 {
        errors.push(error(
            "extensions",
            "Use up to eight extensions on a rectangular building.",
        ));
    }
    if recipe
        .openings
        .iter()
        .any(|o| o.id.starts_with("extension-door-"))
    {
        errors.push(error(
            "extensions",
            "extension-door- IDs are reserved for generated internal doors.",
        ));
    }
    let mut ids = BTreeSet::new();
    for (index, extension) in recipe.extensions.iter().enumerate() {
        if extension.id.trim().is_empty() || extension.id.len() > 64 || !ids.insert(&extension.id) {
            errors.push(error(
                "extensions",
                "Extension IDs must be unique and contain 1 to 64 characters.",
            ));
        }
        for (field, value, min, max) in [
            (
                "width",
                extension.width,
                (recipe.wall_thickness * 2. + 0.8).max(1.5),
                30.,
            ),
            ("depth", extension.depth, 1.5, 20.),
            (
                "wall height",
                extension.wall_height,
                2.3,
                recipe.wall_height,
            ),
            ("roof rise", extension.roof_rise, 0.1, 6.),
            ("offset", extension.offset, -40., 40.),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                errors.push(error(
                    &extension.id,
                    &format!("Extension {field} must be between {min} and {max} metres."),
                ));
            }
        }
        if extension.offset.abs() + extension.width * 0.5 > extension.facade.length(recipe) * 0.5 {
            errors.push(error(
                &extension.id,
                "Keep the attachment within the ends of its host wall.",
            ));
        }
        if extension.window_panes.is_some_and(|p| !p.is_valid()) {
            errors.push(error(
                &extension.id,
                "Pane rows and columns must each be between 1 and 32.",
            ));
        }
        for other in &recipe.extensions[..index] {
            if extension.facade == other.facade
                && (extension.offset - other.offset).abs()
                    < (extension.width + other.width) * 0.5 + 0.1
            {
                errors.push(error(
                    &extension.id,
                    "Extensions on the same wall must leave a clear gap between them.",
                ));
            }
        }
    }
    errors
}

pub(super) fn connections(recipe: &BuildingRecipe) -> Result<(), crate::BuildingError> {
    let mut errors = Vec::new();
    for extension in &recipe.extensions {
        if recipe.openings.iter().any(|opening| {
            opening.kind == OpeningKind::Door
                && opening.facade == extension.facade
                && overlaps(extension, opening.offset, opening.width + 0.3)
        }) {
            errors.push(error(
                &extension.id,
                "Move the exterior entrance clear of this extension, or choose another host wall.",
            ));
        }
        if recipe.chimney.as_ref().is_some_and(|c| {
            c.facade == extension.facade && overlaps(extension, c.offset, c.width + 0.3)
        }) {
            errors.push(error(
                &extension.id,
                "Move the chimney clear of the extension connection.",
            ));
        }
        let bounds = node::placement(recipe, extension).bounds(
            extension.width,
            extension.depth,
            recipe.wall_thickness,
        );
        if crate::geometry::foundation::stairs(recipe).any(|s| intersects_xy(bounds, s.bounds)) {
            errors.push(error(
                &extension.id,
                "The extension crosses the entrance stairs; move it or change the stair direction.",
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(crate::BuildingError {
            diagnostics: errors,
        })
    }
}

pub(crate) fn covered(recipe: &BuildingRecipe, opening: &crate::OpeningRecipe) -> bool {
    recipe.extensions.iter().any(|e| {
        e.facade == opening.facade
            && overlaps(e, opening.offset, opening.width + 0.14)
            && opening.sill + recipe.floor_height(opening.storey)
                < e.wall_height + e.roof_rise + recipe.roof.effective_thickness()
    })
}

fn overlaps(extension: &ExtensionRecipe, offset: f32, width: f32) -> bool {
    (extension.offset - offset).abs() < (extension.width + width) * 0.5
}

fn intersects_xy(a: BoxCollider, b: BoxCollider) -> bool {
    (0..2).all(|i| a.min[i] < b.max[i] - 0.001 && a.max[i] > b.min[i] + 0.001)
}

fn error(element: &str, message: &str) -> Diagnostic {
    Diagnostic {
        element: element.into(),
        message: message.into(),
    }
}
