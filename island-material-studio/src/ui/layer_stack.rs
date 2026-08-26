//! Ordered layer selection and structural operations.

use bevy_egui::egui;
use motu::procedural_textures::{LayerMask, MaterialLayer, TextureRecipe, recipe::MAX_LAYERS};

/// Draws the ordered stack and returns whether the recipe changed.
pub fn draw(
    ui: &mut egui::Ui,
    recipe: &mut TextureRecipe,
    selected_layer_id: &mut Option<String>,
) -> bool {
    let mut changed = false;
    let mut operation = None;
    let layer_count = recipe.layers.len();
    egui::ScrollArea::vertical()
        .id_salt("material layer stack")
        .show(ui, |ui| {
            for (index, layer) in recipe.layers.iter_mut().enumerate() {
                let selected = selected_layer_id.as_deref() == Some(layer.id.as_str());
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut layer.enabled, "").changed();
                        if ui
                            .selectable_label(selected, format!("{}. {}", index + 1, layer.name))
                            .clicked()
                        {
                            *selected_layer_id = Some(layer.id.clone());
                        }
                        if layer.outputs.height.enabled {
                            ui.small("H").on_hover_text("Contributes height");
                        }
                        if layer.outputs.albedo.enabled {
                            ui.small("A").on_hover_text("Contributes albedo");
                        }
                    });
                    if selected {
                        ui.horizontal(|ui| {
                            if ui.small_button("↑").clicked() && index > 0 {
                                operation = Some(LayerOperation::MoveUp(index));
                            }
                            if ui.small_button("↓").clicked() && index + 1 < layer_count {
                                operation = Some(LayerOperation::MoveDown(index));
                            }
                            if ui.small_button("Duplicate").clicked() {
                                operation = Some(LayerOperation::Duplicate(index));
                            }
                            if ui.small_button("Delete").clicked() {
                                operation = Some(LayerOperation::Delete(index));
                            }
                        });
                    }
                });
            }
        });
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("+ Add layer").clicked() {
            operation = Some(LayerOperation::Add);
        }
        if ui.button("Base settings").clicked() {
            *selected_layer_id = None;
        }
    });
    if let Some(operation) = operation {
        changed |= apply_operation(recipe, selected_layer_id, operation);
    }
    changed
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayerOperation {
    Add,
    Duplicate(usize),
    MoveUp(usize),
    MoveDown(usize),
    Delete(usize),
}

fn apply_operation(
    recipe: &mut TextureRecipe,
    selection: &mut Option<String>,
    operation: LayerOperation,
) -> bool {
    match operation {
        LayerOperation::Add => {
            if recipe.layers.len() >= MAX_LAYERS {
                return false;
            }
            let layer = MaterialLayer {
                id: unique_layer_id(recipe, "layer"),
                name: format!("Layer {}", recipe.layers.len() + 1),
                ..MaterialLayer::default()
            };
            *selection = Some(layer.id.clone());
            recipe.layers.push(layer);
        }
        LayerOperation::Duplicate(index) => {
            if recipe.layers.len() >= MAX_LAYERS {
                return false;
            }
            let Some(source) = recipe.layers.get(index) else {
                return false;
            };
            // Recipes are deliberately small and history already snapshots the
            // whole document. Cloning one layer keeps every tagged variant
            // intact while assigning a fresh stable identity below.
            let mut duplicate = source.clone();
            duplicate.id = unique_layer_id(recipe, &format!("{}-copy", source.id));
            duplicate.name = format!("{} copy", source.name);
            *selection = Some(duplicate.id.clone());
            recipe.layers.insert(index + 1, duplicate);
        }
        LayerOperation::MoveUp(index) => {
            let mut candidate = recipe.layers.clone();
            candidate.swap(index, index - 1);
            if has_forward_reference(&candidate) {
                return false;
            }
            recipe.layers = candidate;
        }
        LayerOperation::MoveDown(index) => {
            let mut candidate = recipe.layers.clone();
            candidate.swap(index, index + 1);
            if has_forward_reference(&candidate) {
                return false;
            }
            recipe.layers = candidate;
        }
        LayerOperation::Delete(index) => {
            let removed_id = recipe.layers.remove(index).id;
            for layer in &mut recipe.layers {
                if matches!(
                    layer.mask.as_ref(),
                    Some(LayerMask::Layer { layer_id, .. }) if layer_id == &removed_id
                ) {
                    layer.mask = Some(LayerMask::Own);
                }
            }
            *selection = recipe
                .layers
                .get(index.min(recipe.layers.len().saturating_sub(1)))
                .map(|layer| layer.id.clone());
        }
    }
    true
}

fn has_forward_reference(layers: &[MaterialLayer]) -> bool {
    layers.iter().enumerate().any(|(index, layer)| {
        let Some(LayerMask::Layer { layer_id, .. }) = &layer.mask else {
            return false;
        };
        !layers[..index]
            .iter()
            .any(|earlier| earlier.id == *layer_id)
    })
}

fn unique_layer_id(recipe: &TextureRecipe, preferred: &str) -> String {
    let stem = preferred
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    let stem = if stem.is_empty() { "layer" } else { &stem };
    (1_u32..=u32::try_from(MAX_LAYERS).expect("layer limit fits u32") + 1)
        .map(|suffix| {
            if suffix == 1 {
                stem.to_owned()
            } else {
                format!("{stem}-{suffix}")
            }
        })
        .find(|candidate| recipe.layers.iter().all(|layer| layer.id != *candidate))
        .expect("the bounded layer stack always has a free numeric suffix")
}

#[cfg(test)]
mod tests {
    use motu::procedural_textures::TextureRecipe;

    use super::*;

    fn recipe() -> TextureRecipe {
        serde_json::from_str(include_str!(
            "../../../island-rs/texture-recipes/cracked-stone.json"
        ))
        .unwrap()
    }

    #[test]
    fn duplicate_assigns_a_stable_unique_id() {
        let mut recipe = recipe();
        let mut selected = Some(recipe.layers[0].id.clone());
        assert!(apply_operation(
            &mut recipe,
            &mut selected,
            LayerOperation::Duplicate(0)
        ));
        assert_eq!(recipe.layers.len(), 2);
        assert_ne!(recipe.layers[0].id, recipe.layers[1].id);
        assert_eq!(selected.as_deref(), Some(recipe.layers[1].id.as_str()));
    }

    #[test]
    fn deletion_repairs_selection() {
        let mut recipe = recipe();
        apply_operation(&mut recipe, &mut None, LayerOperation::Duplicate(0));
        let mut selected = Some(recipe.layers[0].id.clone());
        apply_operation(&mut recipe, &mut selected, LayerOperation::Delete(0));
        assert_eq!(selected.as_deref(), Some(recipe.layers[0].id.as_str()));
    }
}
