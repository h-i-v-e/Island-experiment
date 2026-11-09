//! The shared Rust procedural-material evaluator supplies building surface maps.
use crate::{BuildingError, BuildingRecipe, MaterialSlot, MaterialTexture};
use motu::procedural_textures::{
    AlbedoSettings, DisplacementSettings, MaterialModel, NormalConvention, OcclusionRecipeSettings,
    TextureRecipe, generate_texture_set,
    parameters::{ColourValue, LinearRgb},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

pub(crate) fn generate(
    r: &BuildingRecipe,
    base: &Path,
) -> Result<Vec<MaterialTexture>, BuildingError> {
    [
        (
            MaterialSlot::Wall,
            r.materials.wall,
            r.materials.wall_recipe.as_deref(),
        ),
        (
            MaterialSlot::Roof,
            r.materials.roof,
            r.materials.roof_recipe.as_deref(),
        ),
        (
            MaterialSlot::Door,
            r.materials.door,
            r.materials.door_recipe.as_deref(),
        ),
    ]
    .into_iter()
    .filter(|(slot, _, path)| *slot != MaterialSlot::Door || path.is_some())
    .map(|(slot, colour, path)| material(r, base, slot, colour, path))
    .collect()
}
fn material(
    r: &BuildingRecipe,
    base: &Path,
    slot: MaterialSlot,
    colour: [f32; 3],
    path: Option<&str>,
) -> Result<MaterialTexture, BuildingError> {
    // Preview recipes request 128; final bakes retain more source detail under a fixed budget.
    let size = r.atlas_size.min(512);
    let (mut recipe, source_hash) = if let Some(path) = path {
        let bytes =
            std::fs::read(base.join(path)).map_err(|e| BuildingError::new(path, e.to_string()))?;
        let recipe: TextureRecipe =
            serde_json::from_slice(&bytes).map_err(|e| BuildingError::new(path, e.to_string()))?;
        (recipe, hash_bytes(&bytes))
    } else {
        let recipe = builtin(r.seed, colour, slot);
        let bytes = serde_json::to_vec(&recipe)
            .map_err(|e| BuildingError::new("material", e.to_string()))?;
        (recipe, hash_bytes(&bytes))
    };
    recipe.width = size;
    recipe.height = size;
    let textures = generate_texture_set(&recipe, NormalConvention::OpenGl)
        .map_err(|e| BuildingError::new(format!("{slot:?}"), e.to_string()))?;
    let albedo = textures
        .albedo
        .pixels()
        .iter()
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect();
    let normal = textures
        .normal
        .pixels()
        .iter()
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect();
    let masks = textures
        .occlusion
        .pixels()
        .iter()
        .flat_map(|ao| {
            [
                *ao,
                if slot == MaterialSlot::Roof { 180 } else { 220 },
                0,
                255,
            ]
        })
        .collect();
    Ok(MaterialTexture {
        slot,
        size,
        albedo,
        normal,
        masks,
        physical_size: [recipe.physical_tile_width_m, recipe.physical_tile_height_m],
        source_hash,
    })
}
fn builtin(seed: u64, colour: [f32; 3], slot: MaterialSlot) -> TextureRecipe {
    TextureRecipe {
        name: format!("building_{slot:?}"),
        seed: seed
            ^ if slot == MaterialSlot::Wall {
                0x5bda_71e3
            } else {
                0x14af_8c2d
            },
        parameters: BTreeMap::new(),
        width: 128,
        height: 128,
        physical_tile_width_m: 2.,
        physical_tile_height_m: 2.,
        material: MaterialModel::LayeredNoise {
            frequency: 12.,
            amplitude: 0.0015,
            octaves: 3,
            lacunarity: 2.,
            gain: 0.5,
            offset: 0.,
        },
        layers: Vec::new(),
        normal_scale: 0.6,
        displacement: DisplacementSettings::default(),
        occlusion: OcclusionRecipeSettings {
            directions: 4,
            samples: 3,
            ..Default::default()
        },
        albedo: AlbedoSettings {
            base_color: ColourValue::Literal(LinearRgb::new(colour[0], colour[1], colour[2])),
            warm_color: ColourValue::Literal(LinearRgb::new(
                colour[0] * 0.93,
                colour[1] * 0.93,
                colour[2] * 0.93,
            )),
            variation: 0.12,
            mineral_density: 0.,
            mineral_brightness: 0.,
            occlusion_influence: 0.,
            ..Default::default()
        },
        output_profiles: vec![motu::procedural_textures::recipe::OutputProfile::Separate],
    }
}
pub(crate) fn prototype_hash(
    recipe: &BuildingRecipe,
    materials: &[MaterialTexture],
) -> Result<String, BuildingError> {
    let mut portable = recipe.clone();
    portable.materials.wall_recipe = None;
    portable.materials.roof_recipe = None;
    portable.materials.door_recipe = None;
    let mut hash = Sha256::new();
    hash.update(b"island-building:geometry3:material2:bake2");
    hash.update(
        serde_json::to_vec(&portable).map_err(|e| BuildingError::new("hash", e.to_string()))?,
    );
    for material in materials {
        hash.update(material.source_hash.as_bytes());
    }
    Ok(format!("{:x}", hash.finalize()))
}
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Returns a palette colour for render parts without generated maps.
#[must_use]
pub fn colour(recipe: &BuildingRecipe, slot: MaterialSlot) -> [f32; 4] {
    let m = &recipe.materials;
    let c = match slot {
        MaterialSlot::Wall => m.wall,
        MaterialSlot::Roof => m.roof,
        MaterialSlot::Floor => m.floor,
        MaterialSlot::Interior => m.interior,
        MaterialSlot::Trim => m.trim,
        MaterialSlot::Door => m.door,
        MaterialSlot::Hardware => [0.48, 0.44, 0.34],
        MaterialSlot::WindowProxy => [m.glass[0] * 0.20, m.glass[1] * 0.23, m.glass[2] * 0.26],
        MaterialSlot::Glass => return m.glass,
        MaterialSlot::Atlas => [1.; 3],
    };
    [c[0], c[1], c[2], 1.]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn door_material_is_optional_for_old_recipes_and_changes_the_baked_exterior() {
        let mut legacy = serde_json::to_value(BuildingRecipe::hut()).unwrap();
        legacy["materials"]
            .as_object_mut()
            .unwrap()
            .remove("door_recipe");
        let mut recipe: BuildingRecipe = serde_json::from_value(legacy).unwrap();
        assert!(recipe.materials.door_recipe.is_none());
        recipe.atlas_size = 128;
        let plain = crate::generate(&recipe).unwrap();
        recipe.materials.door_recipe = Some("wooden-planks.json".into());
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../island-rs/texture-recipes");
        let wood = crate::generate_with_base(&recipe, &base).unwrap();
        let door = wood
            .materials
            .iter()
            .find(|material| material.slot == MaterialSlot::Door)
            .unwrap();
        assert!(
            door.physical_size
                .into_iter()
                .zip([1.2, 2.4])
                .all(|(actual, expected)| (actual - expected).abs() < 1.0e-6)
        );
        assert_ne!(wood.hash, plain.hash);
        assert_ne!(wood.atlas.albedo, plain.atlas.albedo);
        assert_eq!(wood.lods[1], plain.lods[1]);
        assert!(
            wood.lods[1]
                .parts
                .iter()
                .any(|part| part.material == MaterialSlot::Door)
        );
    }
}
