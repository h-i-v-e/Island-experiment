//! Engine-facing in-memory baking for the approved runtime material set.

#![allow(clippy::missing_errors_doc)]

use std::{collections::BTreeMap, fmt, sync::OnceLock};

use sha2::{Digest, Sha256};

use super::{
    LinearRgb, NormalConvention, RecipeParameterValues, TEXTURE_ALGORITHM_VERSION, TextureError,
    TextureRecipe, TextureSet, generate_texture_set_with_parameters,
};

/// Explicit colours selected and owned by an engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeMaterialInputs {
    pub dirt_colour: LinearRgb,
    pub stone_colour: LinearRgb,
    pub sand_colour: LinearRgb,
}

impl RuntimeMaterialInputs {
    #[must_use]
    pub const fn new(
        dirt_colour: LinearRgb,
        stone_colour: LinearRgb,
        sand_colour: LinearRgb,
    ) -> Self {
        Self {
            dirt_colour,
            stone_colour,
            sand_colour,
        }
    }

    fn validate(self) -> Result<(), RuntimeMaterialBakeError> {
        for (name, colour) in [
            ("dirt_colour", self.dirt_colour),
            ("stone_colour", self.stone_colour),
            ("sand_colour", self.sand_colour),
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
        if recipe.parameters.contains_key("sand_colour") {
            values.insert_colour("sand_colour", self.sand_colour);
        }
        values
    }
}

/// Stable identifiers for the material recipes consumed by engine terrain
/// shaders.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum IslandMaterialKind {
    Dirt = 0,
    ForestFloor = 1,
    Rock = 2,
    RiverBed = 3,
    Beach = 4,
    FallenStones = 5,
}

impl IslandMaterialKind {
    pub const ALL: [Self; 6] = [
        Self::Dirt,
        Self::ForestFloor,
        Self::Rock,
        Self::RiverBed,
        Self::Beach,
        Self::FallenStones,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dirt => "dirt",
            Self::ForestFloor => "forest_floor",
            Self::Rock => "rock",
            Self::RiverBed => "river_bed",
            Self::Beach => "beach",
            Self::FallenStones => "fallen_stones",
        }
    }
}

/// Fixed-shape selection avoids allocating a list for the common all-material
/// request while still allowing tools to bake a subset.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaterialSelection {
    pub dirt: bool,
    pub forest_floor: bool,
    pub rock: bool,
    pub river_bed: bool,
    pub beach: bool,
    pub fallen_stones: bool,
}

impl MaterialSelection {
    pub const ALL: Self = Self {
        dirt: true,
        forest_floor: true,
        rock: true,
        river_bed: true,
        beach: true,
        fallen_stones: true,
    };

    #[must_use]
    pub const fn includes(self, kind: IslandMaterialKind) -> bool {
        match kind {
            IslandMaterialKind::Dirt => self.dirt,
            IslandMaterialKind::ForestFloor => self.forest_floor,
            IslandMaterialKind::Rock => self.rock,
            IslandMaterialKind::RiverBed => self.river_bed,
            IslandMaterialKind::Beach => self.beach,
            IslandMaterialKind::FallenStones => self.fallen_stones,
        }
    }
}

impl Default for MaterialSelection {
    fn default() -> Self {
        Self::ALL
    }
}

/// Content revision for the embedded runtime recipes and texture algorithm.
/// Engines use this only to invalidate disposable baked-texture caches.
#[must_use]
pub fn runtime_material_revision() -> [u64; 2] {
    static REVISION: OnceLock<[u64; 2]> = OnceLock::new();
    *REVISION.get_or_init(|| {
        let mut hasher = Sha256::new();
        hasher.update(b"motu-runtime-material-revision-v1");
        hasher.update(TEXTURE_ALGORITHM_VERSION.to_le_bytes());
        for kind in IslandMaterialKind::ALL {
            hasher.update([kind as u8]);
            hasher.update(recipe_json(kind).as_bytes());
        }
        let digest: [u8; 32] = hasher.finalize().into();
        let mut low = [0_u8; 8];
        low.copy_from_slice(&digest[..8]);
        let mut high = [0_u8; 8];
        high.copy_from_slice(&digest[8..16]);
        [u64::from_le_bytes(low), u64::from_le_bytes(high)]
    })
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
    static DIRT: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    static FOREST_FLOOR: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    static ROCK: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    static RIVER_BED: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    static BEACH: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    static FALLEN_STONES: OnceLock<Result<TextureRecipe, String>> = OnceLock::new();
    match material {
        IslandMaterialKind::Dirt => &DIRT,
        IslandMaterialKind::ForestFloor => &FOREST_FLOOR,
        IslandMaterialKind::Rock => &ROCK,
        IslandMaterialKind::RiverBed => &RIVER_BED,
        IslandMaterialKind::Beach => &BEACH,
        IslandMaterialKind::FallenStones => &FALLEN_STONES,
    }
}

const fn recipe_json(material: IslandMaterialKind) -> &'static str {
    match material {
        IslandMaterialKind::Dirt => include_str!("../../texture-recipes/Dirt.json"),
        IslandMaterialKind::ForestFloor => {
            include_str!("../../texture-recipes/ForestFloor.json")
        }
        IslandMaterialKind::Rock => include_str!("../../texture-recipes/cracked-stone.json"),
        IslandMaterialKind::RiverBed => {
            include_str!("../../texture-recipes/rounded-river-stones.json")
        }
        IslandMaterialKind::Beach => include_str!("../../texture-recipes/Beach.json"),
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
    hasher.update(b"motu-runtime-material-bake-v3");
    for channel in inputs
        .dirt_colour
        .channels()
        .into_iter()
        .chain(inputs.stone_colour.channels())
        .chain(inputs.sand_colour.channels())
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

    #[test]
    fn embedded_runtime_material_revision_is_stable_and_nonzero() {
        let first = runtime_material_revision();
        assert_eq!(first, runtime_material_revision());
        assert_ne!(first, [0, 0]);
    }

    fn inputs() -> RuntimeMaterialInputs {
        RuntimeMaterialInputs::new(
            LinearRgb::new(0.12, 0.07, 0.03),
            LinearRgb::new(0.35, 0.34, 0.30),
            LinearRgb::new(0.34, 0.28, 0.09),
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
        assert!(
            result.materials.values().all(|textures| {
                textures.metadata.normal_convention == NormalConvention::DirectX
            })
        );
        assert_eq!(result.identity.hash.len(), 64);
    }

    #[test]
    fn invalid_engine_colour_is_rejected_before_baking() {
        let error = bake_island_materials(
            &RuntimeMaterialInputs::new(
                LinearRgb::new(f32::NAN, 0.2, 0.3),
                LinearRgb::new(0.4, 0.4, 0.4),
                LinearRgb::new(0.3, 0.25, 0.1),
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
                LinearRgb::new(0.5, 0.4, 0.2),
            ),
            &RuntimeMaterialBakeOptions {
                width: Some(64),
                height: Some(64),
                normal_convention: NormalConvention::DirectX,
                materials: MaterialSelection {
                    dirt: false,
                    forest_floor: false,
                    rock: false,
                    river_bed: false,
                    beach: false,
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
                LinearRgb::new(0.5, 0.4, 0.2),
            ),
            &RuntimeMaterialBakeOptions {
                width: Some(64),
                height: Some(64),
                normal_convention: NormalConvention::DirectX,
                materials: MaterialSelection {
                    dirt: false,
                    forest_floor: true,
                    rock: false,
                    river_bed: false,
                    beach: false,
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
                LinearRgb::new(0.5, 0.4, 0.2),
            ),
            &RuntimeMaterialBakeOptions {
                width: Some(64),
                height: Some(64),
                normal_convention: NormalConvention::DirectX,
                materials: MaterialSelection {
                    dirt: false,
                    forest_floor: false,
                    rock: true,
                    river_bed: true,
                    beach: false,
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

    #[test]
    fn beach_uses_the_engine_sand_colour() {
        let sand = LinearRgb::new(0.05, 0.35, 0.75);
        let result = bake_island_materials(
            &RuntimeMaterialInputs::new(
                LinearRgb::new(0.1, 0.1, 0.1),
                LinearRgb::new(0.2, 0.2, 0.2),
                sand,
            ),
            &RuntimeMaterialBakeOptions {
                width: Some(64),
                height: Some(64),
                normal_convention: NormalConvention::DirectX,
                materials: MaterialSelection {
                    dirt: false,
                    forest_floor: false,
                    rock: false,
                    river_bed: false,
                    beach: true,
                    fallen_stones: false,
                },
            },
        )
        .expect("beach runtime bake");
        assert!(
            result.materials[&IslandMaterialKind::Beach]
                .albedo
                .pixels()
                .iter()
                .all(|pixel| pixel[2] > pixel[1] && pixel[1] > pixel[0]),
            "beach albedo must retain the supplied sand hue"
        );
    }

    #[test]
    fn sand_colour_changes_the_runtime_bake_identity() {
        let options = RuntimeMaterialBakeOptions {
            width: Some(32),
            height: Some(32),
            normal_convention: NormalConvention::DirectX,
            materials: MaterialSelection {
                dirt: false,
                forest_floor: false,
                rock: false,
                river_bed: false,
                beach: true,
                fallen_stones: false,
            },
        };
        let first = bake_island_materials(
            &RuntimeMaterialInputs::new(
                LinearRgb::new(0.1, 0.1, 0.1),
                LinearRgb::new(0.2, 0.2, 0.2),
                LinearRgb::new(0.3, 0.25, 0.1),
            ),
            &options,
        )
        .expect("first beach runtime bake");
        let second = bake_island_materials(
            &RuntimeMaterialInputs::new(
                LinearRgb::new(0.1, 0.1, 0.1),
                LinearRgb::new(0.2, 0.2, 0.2),
                LinearRgb::new(0.6, 0.5, 0.2),
            ),
            &options,
        )
        .expect("second beach runtime bake");

        assert_ne!(first.identity, second.identity);
    }
}
