//! Shared bark recipes with independently configurable wood colours.
use super::{
    IslandMaterialKind, LinearRgb, NormalConvention, RecipeParameterValues,
    RuntimeMaterialBakeError, TextureRecipe, TextureSet, generate_texture_set_with_parameters,
};

/// Species selects a shared recipe, independently of the rendered tree variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeBarkSpecies {
    Broadleaf,
    Pine,
    Palm,
}

impl TryFrom<u32> for TreeBarkSpecies {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Broadleaf),
            1 => Ok(Self::Pine),
            2 => Ok(Self::Palm),
            _ => Err(()),
        }
    }
}

/// Bake one species' maps. The caller owns the returned buffers; recipes and
/// optional colour overrides are resolved before generating any pixels.
///
/// # Errors
/// Returns an error for invalid embedded recipes, colours, resolutions or maps.
pub fn bake_tree_bark(
    conifer: bool,
    resolution: u32,
    wood_colour: Option<LinearRgb>,
    normal_convention: NormalConvention,
) -> Result<TextureSet, RuntimeMaterialBakeError> {
    bake_species_bark(
        if conifer {
            TreeBarkSpecies::Pine
        } else {
            TreeBarkSpecies::Broadleaf
        },
        resolution,
        wood_colour,
        normal_convention,
    )
}

/// Bake a species recipe into owned maps while preserving recipe colour controls.
///
/// # Errors
/// Returns an error for invalid embedded recipes, colours, resolutions or maps.
pub fn bake_species_bark(
    species: TreeBarkSpecies,
    resolution: u32,
    wood_colour: Option<LinearRgb>,
    normal_convention: NormalConvention,
) -> Result<TextureSet, RuntimeMaterialBakeError> {
    let json = match species {
        TreeBarkSpecies::Broadleaf => include_str!("../../texture-recipes/PlateBark.json"),
        TreeBarkSpecies::Pine => include_str!("../../texture-recipes/PineBark.json"),
        TreeBarkSpecies::Palm => include_str!("../../texture-recipes/PalmBark.json"),
    };
    let mut recipe: TextureRecipe =
        serde_json::from_str(json).map_err(|error| RuntimeMaterialBakeError::EmbeddedRecipe {
            material: IslandMaterialKind::TreeBark,
            message: error.to_string(),
        })?;
    recipe.width = resolution;
    recipe.height = resolution;
    let mut parameters = RecipeParameterValues::new();
    if let Some(colour) = wood_colour {
        parameters.insert_colour("wood_colour", colour);
    }
    generate_texture_set_with_parameters(&recipe, &parameters, normal_convention).map_err(
        |source| RuntimeMaterialBakeError::Bake {
            material: IslandMaterialKind::TreeBark,
            source,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn species_have_distinct_bark_and_colour_controls_preserve_relief() {
        let plate = bake_species_bark(
            TreeBarkSpecies::Broadleaf,
            32,
            None,
            NormalConvention::DirectX,
        )
        .unwrap();
        let pine =
            bake_species_bark(TreeBarkSpecies::Pine, 32, None, NormalConvention::DirectX).unwrap();
        let palm =
            bake_species_bark(TreeBarkSpecies::Palm, 32, None, NormalConvention::DirectX).unwrap();
        assert_ne!(plate.albedo, pine.albedo);
        assert_ne!(plate.height, pine.height);
        assert_ne!(palm.albedo, pine.albedo);
        assert_ne!(palm.height, plate.height);
        assert_eq!(palm.metadata.name, "PalmBark");
        for (species, original) in [
            (TreeBarkSpecies::Broadleaf, plate),
            (TreeBarkSpecies::Pine, pine),
            (TreeBarkSpecies::Palm, palm),
        ] {
            let recoloured = bake_species_bark(
                species,
                32,
                Some(LinearRgb::new(0.18, 0.06, 0.025)),
                NormalConvention::DirectX,
            )
            .unwrap();
            assert_ne!(original.albedo, recoloured.albedo);
            assert_eq!(original.height, recoloured.height);
            assert_eq!(original.normal, recoloured.normal);
            assert_eq!(original.occlusion, recoloured.occlusion);
        }
    }
}
