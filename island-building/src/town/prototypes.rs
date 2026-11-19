use super::{TownError, TownOutput, TownPrototype};
use crate::{BuildingPrototype, BuildingRecipe, MaterialSlot, collision::BoxCollider};
use std::path::Path;

#[derive(Default)]
pub(super) struct PrototypePool {
    slots: Vec<PrototypeSlot>,
}

pub(super) struct PrototypeSlot {
    pub recipe: BuildingRecipe,
    pub foundations: Vec<BoxCollider>,
    building: Option<BuildingPrototype>,
}

impl PrototypePool {
    pub fn prepare(&mut self, mut recipe: BuildingRecipe, shared: bool) -> usize {
        // One material serves all terrain-fitted stone, independent of entry ID.
        recipe.materials.foundation_recipe = None;
        if shared && let Some(index) = self.slots.iter().position(|slot| slot.recipe == recipe) {
            return index;
        }
        let index = self.slots.len();
        self.slots.push(PrototypeSlot {
            foundations: crate::terrain_support::foundation_footprints(&recipe),
            recipe,
            building: None,
        });
        index
    }

    pub fn get_mut(&mut self, index: usize) -> &mut PrototypeSlot {
        &mut self.slots[index]
    }

    pub fn finish(mut self, output: &mut TownOutput) {
        let mut prototype_indices = vec![None; self.slots.len()];
        for instance in &mut output.instances {
            let slot = &mut self.slots[instance.prototype_index];
            instance.prototype_index = *prototype_indices[instance.prototype_index]
                .get_or_insert_with(|| {
                    let mut prototype = slot.building.take().expect("used prototype was generated");
                    if output.support_material.is_none()
                        && let Some(index) = prototype
                            .materials
                            .iter()
                            .position(|material| material.slot == MaterialSlot::Foundation)
                    {
                        output.support_material = Some(prototype.materials.remove(index));
                    }
                    crate::terrain_support::remove_supports(&mut prototype);
                    prototype
                        .materials
                        .retain(|material| material.slot != MaterialSlot::Foundation);
                    let index = output.prototypes.len();
                    output.prototypes.push(TownPrototype {
                        entry_id: instance.entry_id.clone(),
                        building: prototype,
                    });
                    index
                });
        }
    }
}

impl PrototypeSlot {
    pub fn resolved_recipe(&mut self, base: &Path, id: &str) -> Result<&BuildingRecipe, TownError> {
        if self.building.is_none() {
            self.building = Some(crate::generate_with_base(&self.recipe, base).map_err(
                |error| TownError::Building {
                    entry: id.into(),
                    reason: error.to_string(),
                },
            )?);
        }
        Ok(&self
            .building
            .as_ref()
            .expect("prototype generated above")
            .recipe)
    }
}
