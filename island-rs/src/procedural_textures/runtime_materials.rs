//! Engine-facing in-memory baking for the approved runtime material set.

#![allow(clippy::missing_errors_doc)]

use std::{collections::BTreeMap, fmt, sync::OnceLock};

use sha2::{Digest, Sha256};

use super::{
    LinearRgb, NormalConvention, RecipeParameterValues, TextureError, TextureRecipe, TextureSet,
    generate_texture_set_with_parameters,
};

/// Explicit colours selected and owned by an engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeMaterialInputs {
    pub dirt_colour: LinearRgb,
    pub stone_colour: LinearRgb,
}

impl RuntimeMaterialInputs {
    #[must_use]
    pub const fn new(dirt_colour: LinearRgb, stone_colour: LinearRgb) -> Self {
        Self {
            dirt_colour,
            stone_colour,
        }
    }

    fn validate(self) -> Result<(), RuntimeMaterialBakeError> {
        for (name, colour) in [
            ("dirt_colour", self.dirt_colour),
            ("stone_colour", self.stone_colour),
        ] {
            if !colour.is_valid() {
                return Err(RuntimeMaterialBakeError::InvalidInputColour {
                    name,
                    value: colour,
                });
            }
        }
        Ok(())
    }

    fn parameters_for(self, recipe: &TextureRecipe) -> RecipeParameterValues {
        let mut values = RecipeParameterValues::new();
        if recipe.parameters.contains_key("dirt_colour") {
            values.insert_colour("dirt_colour", self.dirt_colour);
        }
        if recipe.parameters.contains_key("stone_colour") {
            values.insert_colour("stone_colour", self.stone_colour);
        }
        values
    }
}

/// Stable identifiers for the material recipes consumed by engine terrain
/// shaders.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum IslandMaterialKind {
    Rock = 0,
    RiverBed = 1,
    ForestFloor = 2,
    FallenStones = 3,
}

impl IslandMaterialKind {
    pub const ALL: [Self; 4] = [
        Self::Rock,
        Self::RiverBed,
        Self::ForestFloor,
        Self::FallenStones,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rock => "rock",
            Self::RiverBed => "river_bed",
            Self::ForestFloor => "forest_floor",
            Self::FallenStones => "fallen_stones",
        }
    }
}

/// Fixed-shape selection avoids allocating a list for the common all-material
/// request while still allowing tools to bake a subset.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaterialSelection {
    pub rock: bool,
    pub river_bed: bool,
    pub forest_floor: bool,
    pub fallen_stones: bool,
}

impl MaterialSelection {
    pub const ALL: Self = Self {
        rock: true,
        river_bed: true,
        forest_floor: true,
        fallen_stones: true,
    };

    #[must_use]
    pub const fn includes(self, kind: IslandMaterialKind) -> bool {
        match kind {
            IslandMaterialKind::Rock => self.rock,
            IslandMaterialKind::RiverBed => self.river_bed,
            IslandMaterialKind::ForestFloor => self.forest_floor,
            IslandMaterialKind::FallenStones => self.fallen_stones,
        }
    }
}

impl Default for MaterialSelection {
    fn default() -> Self {
        Self::ALL
    }
}

/// Resolution, normal convention, and subset requested by an engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeMaterialBakeOptions {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub normal_convention: NormalConvention,
    pub materials: MaterialSelection,
}

impl Default for RuntimeMaterialBakeOptions {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            normal_convention: NormalConvention::OpenGl,
            materials: MaterialSelection::ALL,
        }
    }
}

/// Canonical identity of one complete runtime request and its resolved maps.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialBakeIdentity {
    pub hash: String,
}

/// Owned maps returned to an engine. No filesystem encoding is performed.
#[derive(Clone, Debug, PartialEq)]
pub struct IslandMaterialTextures {
    pub materials: BTreeMap<IslandMaterialKind, TextureSet>,
    pub identity: MaterialBakeIdentity,
}

/// Failure while loading an embedded recipe or baking one selected material.
#[derive(Debug)]
pub enum RuntimeMaterialBakeError {
    InvalidInputColour {
        name: &'static str,
        value: LinearRgb,
    },
    InvalidResolution {
        axis: &'static str,
        value: u32,
    },
    EmptySelection,
    EmbeddedRecipe {
        material: IslandMaterialKind,
        message: String,
    },
    Bake {
        material: IslandMaterialKind,
        source: TextureError,
    },
}

impl fmt::Display for RuntimeMaterialBakeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInputColour { name, value } => write!(
                formatter,
                "runtime material input {name} is not finite linear RGB in [0, 1]: {:?}",
                value.channels()
            ),
            Self::InvalidResolution { axis, value } => {
                write!(
                    formatter,
                    "runtime material {axis} must be non-zero ({value})"
                )
            }
            Self::EmptySelection => formatter.write_str("runtime material selection is empty"),
            Self::EmbeddedRecipe { material, message } => write!(
                formatter,
                "embedded {} material recipe is invalid: {message}",
                material.name()
            ),
            Self::Bake { material, source } => {
                write!(
                    formatter,
                    "could not bake {} material: {source}",
                    material.name()
                )
            }
        }
    }
}

impl std::error::Error for RuntimeMaterialBakeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bake { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Bakes the selected runtime material recipes with explicit caller colours
/// and returns complete owned texture sets without touching the filesystem.
pub fn bake_island_materials(
    inputs: &RuntimeMaterialInputs,
    options: &RuntimeMaterialBakeOptions,
) -> Result<IslandMaterialTextures, RuntimeMaterialBakeError> {
    inputs.validate()?;
    for (axis, value) in [("width", options.width), ("height", options.height)] {
        if value == Some(0) {
            return Err(RuntimeMaterialBakeError::InvalidResolution { axis, value: 0 });
        }
    }
    if !IslandMaterialKind::ALL
        .into_iter()
        .any(|kind| options.materials.includes(kind))
    {
        return Err(RuntimeMaterialBakeError::EmptySelection);
    }

    let mut materials = BTreeMap::new();
    for kind in IslandMaterialKind::ALL
        .into_iter()
        .filter(|kind| options.materials.includes(*kind))
    {
        let mut recipe = embedded_recipe(kind)?.clone();
        if let Some(width) = options.width {
            recipe.width = width;
        }
        if let Some(height) = options.height {
            recipe.height = height;
        }
        let parameters = inputs.parameters_for(&recipe);
        let textures =
            generate_texture_set_with_parameters(&recipe, &parameters, options.normal_convention)
                .map_err(|source| RuntimeMaterialBakeError::Bake {
                material: kind,
                source,
            })?;
        materials.insert(kind, textures);
    }

    let identity = MaterialBakeIdentity {
        hash: bake_identity(inputs, options, &materials),
    };
    Ok(IslandMaterialTextures {
        materials,
        identity,
    })
}

fn embedded_recipe(
    material: IslandMaterialKind,
) -> Result<&'static TextureRecipe, RuntimeMaterialBakeError> {
    let parsed = recipe_cell(material).get_or_init(|| {
        serde_json::from_str(recipe_json(material)).map_err(|error| error.to_string())
    });
    parsed
        .as_ref()
        .map_err(|message| RuntimeMaterialBakeError::EmbeddedRecipe {
            material,
            message: message.clone(),
        })
}

fn recipe_cell(material: IslandMaterialKind) -> &'static OnceLock<Result<TextureRecipe, String>> {
    static ROCK: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    static RIVER_BED: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    static FOREST_FLOOR: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    static FALLEN_STONES: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    match material {
        IslandMaterialKind::Rock => &ROCK,
        IslandMaterialKind::RiverBed => &RIVER_BED,
        IslandMaterialKind::ForestFloor => &FOREST_FLOOR,
        IslandMaterialKind::FallenStones => &FALLEN_STONES,
    }
}

const fn recipe_json(material: IslandMaterialKind) -> &'static str {
    match material {
        IslandMaterialKind::Rock => include_str!("../../texture-recipes/cracked-stone.json"),
        IslandMaterialKind::RiverBed => {
            include_str!("../../texture-recipes/rounded-river-stones.json")
        }
        IslandMaterialKind::ForestFloor => {
            include_str!("../../texture-recipes/ForestFloor.json")
        }
        IslandMaterialKind::FallenStones => {
            include_str!("../../texture-recipes/FallenStones.json")
        }
    }
}

fn bake_identity(
    inputs: &RuntimeMaterialInputs,
    options: &RuntimeMaterialBakeOptions,
    materials: &BTreeMap<IslandMaterialKind, TextureSet>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"motu-runtime-material-bake-v1");
    for channel in inputs
        .dirt_colour
        .channels()
        .into_iter()
        .chain(inputs.stone_colour.channels())
    {
        hasher.update(channel.to_le_bytes());
    }
    hasher.update(options.width.unwrap_or_default().to_le_bytes());
    hasher.update(options.height.unwrap_or_default().to_le_bytes());
    hasher.update([options.normal_convention as u8]);
    for (kind, textures) in materials {
        hasher.update([*kind as u8]);
        hasher.update(textures.metadata.recipe_hash.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> RuntimeMaterialInputs {
        RuntimeMaterialInputs::new(
            LinearRgb::new(0.12, 0.07, 0.03),
            LinearRgb::new(0.35, 0.34, 0.30),
        )
    }

    #[test]
    fn complete_runtime_set_bakes_in_memory() {
        let result = bake_island_materials(
            &inputs(),
            &RuntimeMaterialBakeOptions {
                width: Some(64),
                height: Some(64),
                normal_convention: NormalConvention::DirectX,
                materials: MaterialSelection::ALL,
            },
        )
        .expect("runtime material bake");
        assert_eq!(result.materials.len(), IslandMaterialKind::ALL.len());
        assert!(result.materials.values().all(|textures| {
            textures.dimensions.width == 64 && textures.dimensions.height == 64
        }));
        assert_eq!(result.identity.hash.len(), 64);
    }

    #[test]
    fn invalid_engine_colour_is_rejected_before_baking() {
        let error = bake_island_materials(
            &RuntimeMaterialInputs::new(
                LinearRgb::new(f32::NAN, 0.2, 0.3),
                LinearRgb::new(0.4, 0.4, 0.4),
            ),
            &RuntimeMaterialBakeOptions::default(),
        )
        .expect_err("invalid input");
        assert!(matches!(
            error,
            RuntimeMaterialBakeError::InvalidInputColour {
                name: "dirt_colour",
                ..
            }
        ));
    }

    #[test]
    fn fallen_stones_keep_engine_dirt_and_stone_colours_separate() {
        let result = bake_island_materials(
            &RuntimeMaterialInputs::new(
                LinearRgb::new(1.0, 0.0, 0.0),
                LinearRgb::new(0.0, 0.0, 1.0),
            ),
            &RuntimeMaterialBakeOptions {
                width: Some(64),
                height: Some(64),
                normal_convention: NormalConvention::DirectX,
                materials: MaterialSelection {
                    rock: false,
                    river_bed: false,
                    forest_floor: false,
                    fallen_stones: true,
                },
            },
        )
        .expect("fallen-stones runtime bake");
        let albedo = &result.materials[&IslandMaterialKind::FallenStones].albedo;
        let dirt_pixels = albedo
            .pixels()
            .iter()
            .filter(|pixel| pixel[0] > pixel[2])
            .count();
        let stone_pixels = albedo
            .pixels()
            .iter()
            .filter(|pixel| pixel[2] > pixel[0])
            .count();

        assert!(dirt_pixels > albedo.len() / 2, "dirt must remain the base");
        assert!(stone_pixels > 0, "stone cells must use the stone colour");
    }

    #[test]
    fn forest_floor_keeps_engine_dirt_as_its_visible_soil() {
        let result = bake_island_materials(
            &RuntimeMaterialInputs::new(
                LinearRgb::new(1.0, 0.0, 1.0),
                LinearRgb::new(0.0, 1.0, 1.0),
            ),
            &RuntimeMaterialBakeOptions {
                width: Some(64),
                height: Some(64),
                normal_convention: NormalConvention::DirectX,
                materials: MaterialSelection {
                    rock: false,
                    river_bed: false,
                    forest_floor: true,
                    fallen_stones: false,
                },
            },
        )
        .expect("forest-floor runtime bake");
        let albedo = &result.materials[&IslandMaterialKind::ForestFloor].albedo;
        let dirt_pixels = albedo
            .pixels()
            .iter()
            .filter(|pixel| pixel[1] == 0 && pixel[0].abs_diff(pixel[2]) <= 1)
            .count();

        assert!(
            dirt_pixels > albedo.len() / 10,
            "visible soil must retain the supplied dirt hue"
        );
    }

    #[test]
    fn rock_and_river_stones_do_not_add_an_authored_hue() {
        let result = bake_island_materials(
            &RuntimeMaterialInputs::new(
                LinearRgb::new(0.25, 0.25, 0.25),
                LinearRgb::new(0.25, 0.25, 0.25),
            ),
            &RuntimeMaterialBakeOptions {
                width: Some(64),
                height: Some(64),
                normal_convention: NormalConvention::DirectX,
                materials: MaterialSelection {
                    rock: true,
                    river_bed: true,
                    forest_floor: false,
                    fallen_stones: false,
                },
            },
        )
        .expect("stone runtime bake");

        for kind in [IslandMaterialKind::Rock, IslandMaterialKind::RiverBed] {
            assert!(
                result.materials[&kind]
                    .albedo
                    .pixels()
                    .iter()
                    .all(|pixel| pixel[0].abs_diff(pixel[1]) <= 1
                        && pixel[1].abs_diff(pixel[2]) <= 1),
                "{} must retain the supplied neutral stone hue",
                kind.name()
            );
        }
    }
}
