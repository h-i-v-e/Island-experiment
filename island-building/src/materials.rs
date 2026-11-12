//! The shared Rust procedural-material evaluator supplies building surface maps.
use crate::{
    BuildingError, BuildingRecipe, MaterialSlot, MaterialTexture, RoofCovering, WallStyle,
};
use motu::procedural_textures::{
    AlbedoSettings, DisplacementSettings, MaterialModel, NormalConvention, OcclusionRecipeSettings,
    RecipeParameterValues, TextureRecipe, generate_texture_set_with_parameters,
    parameters::{ColourValue, LinearRgb},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

mod thatch_ends;

pub(crate) fn generate(
    r: &BuildingRecipe,
    base: &Path,
) -> Result<Vec<MaterialTexture>, BuildingError> {
    let mut materials: Vec<_> = [
        (
            MaterialSlot::Wall,
            r.materials.wall,
            r.materials.wall_recipe.as_deref(),
        ),
        (
            MaterialSlot::Timber,
            r.materials.timber,
            r.materials.timber_recipe.as_deref(),
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
        (
            MaterialSlot::Floor,
            r.materials.floor,
            r.materials.floor_recipe.as_deref(),
        ),
        (
            MaterialSlot::Foundation,
            r.materials.foundation,
            r.materials.foundation_recipe.as_deref(),
        ),
        (
            MaterialSlot::Ceiling,
            r.materials.ceiling,
            r.materials.ceiling_recipe.as_deref(),
        ),
        (
            MaterialSlot::Chimney,
            r.materials.chimney,
            r.materials.chimney_recipe.as_deref(),
        ),
    ]
    .into_iter()
    .filter(|(slot, _, path)| match slot {
        MaterialSlot::Door => path.is_some(),
        MaterialSlot::Timber => r.walls.style == WallStyle::WattleAndDaub || path.is_some(),
        MaterialSlot::Chimney => r.chimney.is_some() || path.is_some(),
        _ => true,
    })
    .map(|(slot, colour, path)| material(r, base, slot, colour, path))
    .collect::<Result<_, _>>()?;
    if r.roof.covering == RoofCovering::Thatch {
        let roof = materials
            .iter()
            .find(|map| map.slot == MaterialSlot::Roof)
            .expect("roof material is always generated");
        let ends = thatch_ends::generate(roof, r.seed);
        materials.extend(ends);
    }
    Ok(materials)
}
fn material(
    r: &BuildingRecipe,
    base: &Path,
    slot: MaterialSlot,
    colour: [f32; 3],
    path: Option<&str>,
) -> Result<MaterialTexture, BuildingError> {
    // Fine straw retains its 1024px source even with a small editing atlas.
    // The other surfaces retain their existing preview/final source budget.
    let size = match (slot, r.roof.covering) {
        (MaterialSlot::Roof, RoofCovering::Thatch) => 1024,
        (MaterialSlot::Roof, RoofCovering::Slate)
        | (MaterialSlot::Floor | MaterialSlot::Ceiling, _) => r.atlas_size.clamp(256, 512),
        _ => r.atlas_size.min(512),
    };
    let mut parameters = RecipeParameterValues::new();
    let colour_parameter = match slot {
        MaterialSlot::Wall if r.walls.style == WallStyle::WattleAndDaub => Some("plaster_colour"),
        MaterialSlot::Timber | MaterialSlot::Floor | MaterialSlot::Ceiling => Some("wood_colour"),
        MaterialSlot::Chimney | MaterialSlot::Foundation => Some("stone_colour"),
        MaterialSlot::Roof if r.roof.covering != RoofCovering::StandingSeam => Some("roof_colour"),
        _ => None,
    };
    if let Some(parameter) = colour_parameter.filter(|_| path.is_none()) {
        parameters.insert_colour(parameter, LinearRgb::from(colour));
    }
    let (mut recipe, source_hash) = if let Some(path) = path {
        let bytes =
            std::fs::read(base.join(path)).map_err(|e| BuildingError::new(path, e.to_string()))?;
        let recipe: TextureRecipe =
            serde_json::from_slice(&bytes).map_err(|e| BuildingError::new(path, e.to_string()))?;
        (recipe, hash_bytes(&bytes))
    } else {
        let recipe = default_material(r, slot, colour)?;
        let bytes = if parameters.is_empty() {
            serde_json::to_vec(&recipe)
        } else {
            serde_json::to_vec(&(&recipe, &parameters))
        }
        .map_err(|e| BuildingError::new("material", e.to_string()))?;
        (recipe, hash_bytes(&bytes))
    };
    recipe.width = size;
    recipe.height = size;
    align_board_length(&mut recipe, r, slot);
    let textures =
        generate_texture_set_with_parameters(&recipe, &parameters, NormalConvention::OpenGl)
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
                match (slot, r.roof.covering) {
                    (MaterialSlot::Roof, RoofCovering::StandingSeam) => 180,
                    (MaterialSlot::Roof, RoofCovering::Slate) => 224,
                    (MaterialSlot::Roof, RoofCovering::Thatch) => 242,
                    _ => 220,
                },
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

fn align_board_length(material: &mut TextureRecipe, building: &BuildingRecipe, slot: MaterialSlot) {
    if matches!(slot, MaterialSlot::Floor | MaterialSlot::Ceiling)
        && let MaterialModel::WoodenPlanks(settings) = &material.material
        && settings.regular_stagger
        && settings.joints_per_plank > 0
        && let Some(grid) = crate::geometry::timber_frame::joists::grid(building)
    {
        // Alternate rows finish over alternate joists; every full board spans two bays.
        let joints = f32::from(u16::try_from(settings.joints_per_plank).unwrap_or(u16::MAX));
        material.physical_tile_height_m = grid.spacing * 2. * joints;
    }
}
fn default_material(
    recipe: &BuildingRecipe,
    slot: MaterialSlot,
    colour: [f32; 3],
) -> Result<TextureRecipe, BuildingError> {
    let bytes: &[u8] = match slot {
        MaterialSlot::Wall if recipe.walls.style == WallStyle::WattleAndDaub => {
            include_bytes!("../../island-rs/texture-recipes/wattle-daub.json")
        }
        MaterialSlot::Timber => {
            include_bytes!("../../island-rs/texture-recipes/wooden-planks.json")
        }
        MaterialSlot::Floor | MaterialSlot::Ceiling => {
            include_bytes!("../../island-rs/texture-recipes/wooden-floorboards.json")
        }
        MaterialSlot::Chimney | MaterialSlot::Foundation => {
            include_bytes!("../../island-rs/texture-recipes/coursed-stone.json")
        }
        MaterialSlot::Roof => match recipe.roof.covering {
            RoofCovering::Slate => {
                include_bytes!("../../island-rs/texture-recipes/slate-roof.json")
            }
            RoofCovering::Thatch => {
                include_bytes!("../../island-rs/texture-recipes/thatch-roof.json")
            }
            RoofCovering::StandingSeam => return Ok(builtin(recipe.seed, colour, slot)),
        },
        _ => return Ok(builtin(recipe.seed, colour, slot)),
    };
    let mut material: TextureRecipe = serde_json::from_slice(bytes)
        .map_err(|error| BuildingError::new("default material", error.to_string()))?;
    material.seed ^= recipe.seed;
    if slot == MaterialSlot::Timber {
        material.name = "Frame timber".into();
        material.physical_tile_width_m = 0.18;
        if let MaterialModel::WoodenPlanks(settings) = &mut material.material {
            settings.planks = 1;
            settings.grain_scale = 3;
            settings.seam_width_m = 0.0005;
            settings.bevel_width_m = 0.001;
            settings.seam_depth_m = 0.0002;
        }
    }
    Ok(material)
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
    portable.materials.chimney_recipe = None;
    portable.materials.timber_recipe = None;
    portable.materials.floor_recipe = None;
    portable.materials.foundation_recipe = None;
    portable.materials.ceiling_recipe = None;
    let mut hash = Sha256::new();
    hash.update(b"island-building:geometry30:material12:bake14");
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
        MaterialSlot::Timber => m.timber,
        MaterialSlot::Roof
        | MaterialSlot::ThatchEnd
        | MaterialSlot::ThatchShell1
        | MaterialSlot::ThatchShell2
        | MaterialSlot::ThatchShell3
        | MaterialSlot::ThatchShell4 => m.roof,
        MaterialSlot::Floor => m.floor,
        MaterialSlot::Foundation | MaterialSlot::Stairs => m.foundation,
        MaterialSlot::Ceiling => m.ceiling,
        MaterialSlot::Interior => m.interior,
        MaterialSlot::Trim => m.trim,
        MaterialSlot::Door => m.door,
        MaterialSlot::Hardware => [0.48, 0.44, 0.34],
        MaterialSlot::Chimney => m.chimney,
        MaterialSlot::Firebox => [0.018, 0.014, 0.012],
        MaterialSlot::WindowProxy => [m.glass[0] * 0.20, m.glass[1] * 0.23, m.glass[2] * 0.26],
        MaterialSlot::Glass => return m.glass,
        MaterialSlot::Atlas => [1.; 3],
    };
    [c[0], c[1], c[2], 1.]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn floor_and_ceiling_boards_have_physical_relief_and_independent_tints() {
        let mut legacy = serde_json::to_value(BuildingRecipe::hut()).unwrap();
        for field in ["ceiling", "floor_recipe", "ceiling_recipe"] {
            legacy["materials"].as_object_mut().unwrap().remove(field);
        }
        let mut recipe: BuildingRecipe = serde_json::from_value(legacy).unwrap();
        recipe.atlas_size = 128;
        let floor = material(
            &recipe,
            Path::new("."),
            MaterialSlot::Floor,
            recipe.materials.floor,
            None,
        )
        .unwrap();
        let ceiling = material(
            &recipe,
            Path::new("."),
            MaterialSlot::Ceiling,
            [0.2, 0.1, 0.04],
            None,
        )
        .unwrap();
        for texture in [&floor, &ceiling] {
            assert!(
                texture
                    .physical_size
                    .iter()
                    .zip([1.2, 2.4])
                    .all(|(a, b)| (a - b).abs() < 0.00001)
            );
        }
        assert_ne!(floor.albedo, ceiling.albedo);
        assert_eq!(
            floor.normal, ceiling.normal,
            "recolouring preserves board relief"
        );
        assert_eq!(floor.masks, ceiling.masks);
        assert!(
            floor
                .normal
                .chunks_exact(4)
                .any(|p| p[0].abs_diff(128) > 5 || p[1].abs_diff(128) > 5)
        );
    }

    #[test]
    fn timber_colour_changes_wood_without_repainting_the_plaster_or_changing_grain_relief() {
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.walls.style = WallStyle::WattleAndDaub;
        let original = generate(&recipe, Path::new(".")).unwrap();
        recipe.materials.timber = [0.3, 0.12, 0.04];
        let changed = generate(&recipe, Path::new(".")).unwrap();
        let timber = |maps: Vec<MaterialTexture>| {
            maps.into_iter()
                .find(|m| m.slot == MaterialSlot::Timber)
                .unwrap()
        };
        assert!(
            original
                .iter()
                .filter(|m| m.slot != MaterialSlot::Timber)
                .all(|m| changed.contains(m))
        );
        let a = timber(original);
        let b = timber(changed);
        assert_ne!(a.albedo, b.albedo);
        assert_ne!(a.source_hash, b.source_hash);
        assert_eq!(a.normal, b.normal);
        assert_eq!(a.masks, b.masks);
    }

    fn chimney_texture(textures: &[MaterialTexture]) -> &MaterialTexture {
        textures
            .iter()
            .find(|texture| texture.slot == MaterialSlot::Chimney)
            .expect("chimney texture")
    }

    #[test]
    fn legacy_building_json_defaults_new_chimney_fields_without_adding_geometry() {
        let mut legacy = serde_json::to_value(BuildingRecipe::hut()).unwrap();
        legacy.as_object_mut().unwrap().remove("chimney");
        for field in ["chimney", "chimney_recipe"] {
            legacy["materials"].as_object_mut().unwrap().remove(field);
        }
        let mut recipe: BuildingRecipe = serde_json::from_value(legacy).unwrap();
        recipe.atlas_size = 128;
        assert!(recipe.chimney.is_none());
        assert!(recipe.materials.chimney_recipe.is_none());
        assert_eq!(
            serde_json::to_value(recipe.materials.chimney).unwrap(),
            serde_json::to_value(crate::MaterialPalette::default().chimney).unwrap()
        );
        let prototype = crate::generate(&recipe).unwrap();
        assert!(
            prototype
                .materials
                .iter()
                .all(|texture| texture.slot != MaterialSlot::Chimney)
        );
        assert!(
            prototype
                .lods
                .iter()
                .flat_map(|lod| &lod.parts)
                .all(|part| !matches!(
                    part.material,
                    MaterialSlot::Chimney | MaterialSlot::Firebox
                ))
        );
    }

    #[test]
    fn default_chimney_colour_changes_albedo_and_identity_but_preserves_surface_relief() {
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        assert!(recipe.chimney.is_some());
        let original = generate(&recipe, Path::new(".")).unwrap();
        recipe.materials.chimney = [0.12, 0.08, 0.045];
        let recoloured = generate(&recipe, Path::new(".")).unwrap();
        let a = chimney_texture(&original);
        let b = chimney_texture(&recoloured);
        assert_ne!(a.albedo, b.albedo);
        assert_ne!(a.source_hash, b.source_hash);
        assert_eq!(a.normal, b.normal);
        assert_eq!(a.masks, b.masks);
        assert!(
            original
                .iter()
                .filter(|texture| texture.slot != MaterialSlot::Chimney)
                .all(|texture| recoloured.contains(texture))
        );
    }

    #[test]
    fn changed_custom_chimney_material_invalidates_identity_and_the_lod2_atlas() {
        let directory = std::env::temp_dir().join(format!(
            "building-chimney-material-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("stone.json");
        let mut texture: TextureRecipe = serde_json::from_slice(include_bytes!(
            "../../island-rs/texture-recipes/coursed-stone.json"
        ))
        .unwrap();
        fs::write(&path, serde_json::to_vec(&texture).unwrap()).unwrap();
        let mut recipe = BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.materials.chimney_recipe = Some("stone.json".into());
        let original = crate::generate_with_base(&recipe, &directory).unwrap();
        let MaterialModel::Masonry(settings) = &mut texture.material else {
            unreachable!()
        };
        settings.cells_x = 6;
        texture.seed += 17;
        fs::write(&path, serde_json::to_vec(&texture).unwrap()).unwrap();
        let changed = crate::generate_with_base(&recipe, &directory).unwrap();
        assert_ne!(
            chimney_texture(&original.materials).source_hash,
            chimney_texture(&changed.materials).source_hash
        );
        assert_ne!(original.hash, changed.hash);
        assert_ne!(original.atlas.albedo, changed.atlas.albedo);
        assert_eq!(original.lods, changed.lods);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn roof_palette_recolours_each_covering_without_changing_surface_relief() {
        for covering in [RoofCovering::Slate, RoofCovering::Thatch] {
            let mut recipe = BuildingRecipe::hut();
            recipe.atlas_size = 128;
            recipe.roof.covering = covering;
            recipe.materials.roof = covering.default_colour();
            let original = material(
                &recipe,
                Path::new("."),
                MaterialSlot::Roof,
                recipe.materials.roof,
                None,
            )
            .unwrap();
            recipe.materials.roof = [0.14, 0.07, 0.025];
            let recoloured = material(
                &recipe,
                Path::new("."),
                MaterialSlot::Roof,
                recipe.materials.roof,
                None,
            )
            .unwrap();
            assert_ne!(original.albedo, recoloured.albedo);
            assert_ne!(original.source_hash, recoloured.source_hash);
            assert_eq!(original.normal, recoloured.normal);
            assert_eq!(original.masks, recoloured.masks);
        }
    }

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
