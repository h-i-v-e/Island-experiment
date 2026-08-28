//! Typed recipe parameters and deterministic colour-reference resolution.

#![allow(clippy::missing_errors_doc)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::recipe::{ColourMap, TextureRecipe};

/// Maximum number of declared parameters in one recipe.
pub const MAX_RECIPE_PARAMETERS: usize = 32;
/// Maximum UTF-8 byte length of one parameter name.
pub const MAX_PARAMETER_NAME_LEN: usize = 64;

/// A finite linear-RGB colour. Range validation is performed at recipe and
/// request boundaries so deserialization can retain precise diagnostics.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LinearRgb(pub [f32; 3]);

impl LinearRgb {
    #[must_use]
    pub const fn new(red: f32, green: f32, blue: f32) -> Self {
        Self([red, green, blue])
    }

    #[must_use]
    pub const fn channels(self) -> [f32; 3] {
        self.0
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.0
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }
}

impl From<[f32; 3]> for LinearRgb {
    fn from(value: [f32; 3]) -> Self {
        Self(value)
    }
}

impl From<LinearRgb> for [f32; 3] {
    fn from(value: LinearRgb) -> Self {
        value.0
    }
}

/// One typed parameter declared by a recipe.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParameterDefinition {
    Colour {
        default: LinearRgb,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
    },
}

impl ParameterDefinition {
    #[must_use]
    pub const fn default_colour(&self) -> LinearRgb {
        match self {
            Self::Colour { default, .. } => *default,
        }
    }
}

/// One caller-supplied parameter value.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParameterValue {
    Colour { value: LinearRgb },
}

impl ParameterValue {
    #[must_use]
    pub const fn colour(value: LinearRgb) -> Self {
        Self::Colour { value }
    }

    #[must_use]
    pub const fn as_colour(self) -> LinearRgb {
        match self {
            Self::Colour { value } => value,
        }
    }
}

/// Deterministically ordered parameter overrides supplied by a caller.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RecipeParameterValues(BTreeMap<String, ParameterValue>);

impl RecipeParameterValues {
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn insert_colour(
        &mut self,
        name: impl Into<String>,
        colour: LinearRgb,
    ) -> Option<ParameterValue> {
        self.0.insert(name.into(), ParameterValue::colour(colour))
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<ParameterValue> {
        self.0.get(name).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, ParameterValue)> {
        self.0.iter().map(|(name, value)| (name.as_str(), *value))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A colour field bound to a declared parameter.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ColourParameterReference {
    pub parameter: String,
    /// Optional authored colour to tint relative to the parameter's declared
    /// default. This preserves variation between gradient stops while still
    /// allowing one engine-supplied colour to recolour the complete material.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<LinearRgb>,
}

/// Either a literal linear colour or a reference to a recipe parameter.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ColourValue {
    Literal(LinearRgb),
    Parameter(ColourParameterReference),
}

impl ColourValue {
    #[must_use]
    pub const fn literal(colour: [f32; 3]) -> Self {
        Self::Literal(LinearRgb(colour))
    }

    #[must_use]
    pub fn resolved(self) -> Option<LinearRgb> {
        match self {
            Self::Literal(colour) => Some(colour),
            Self::Parameter(_) => None,
        }
    }

    #[must_use]
    pub const fn as_resolved(&self) -> Option<LinearRgb> {
        match self {
            Self::Literal(colour) => Some(*colour),
            Self::Parameter(_) => None,
        }
    }
}

impl From<[f32; 3]> for ColourValue {
    fn from(value: [f32; 3]) -> Self {
        Self::literal(value)
    }
}

/// A validated owned recipe with all parameter references replaced by literal
/// colours. It can safely move into an engine background task.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedTextureRecipe {
    recipe: TextureRecipe,
    parameters: RecipeParameterValues,
    parameter_hash: String,
}

impl ResolvedTextureRecipe {
    #[must_use]
    pub const fn recipe(&self) -> &TextureRecipe {
        &self.recipe
    }

    #[must_use]
    pub const fn parameters(&self) -> &RecipeParameterValues {
        &self.parameters
    }

    #[must_use]
    pub fn parameter_hash(&self) -> &str {
        &self.parameter_hash
    }
}

/// One precise failure while resolving caller-supplied parameter values.
#[derive(Clone, Debug, PartialEq)]
pub enum RecipeParameterError {
    UnknownOverride { name: String },
    InvalidOverrideColour { name: String, colour: LinearRgb },
    UnknownReference { path: String, name: String },
}

impl fmt::Display for RecipeParameterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOverride { name } => {
                write!(
                    formatter,
                    "parameter override {name:?} is not declared by the recipe"
                )
            }
            Self::InvalidOverrideColour { name, colour } => write!(
                formatter,
                "parameter override {name:?} has invalid linear RGB value {:?}",
                colour.channels()
            ),
            Self::UnknownReference { path, name } => {
                write!(formatter, "{path} references undeclared parameter {name:?}")
            }
        }
    }
}

/// All parameter failures found during one resolution pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeParameterErrors(Vec<RecipeParameterError>);

impl RecipeParameterErrors {
    #[must_use]
    pub fn errors(&self) -> &[RecipeParameterError] {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn push(&mut self, error: RecipeParameterError) {
        self.0.push(error);
    }
}

impl fmt::Display for RecipeParameterErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            error.fmt(formatter)?;
        }
        Ok(())
    }
}

impl std::error::Error for RecipeParameterErrors {}

/// Resolves all recipe colour references with strict caller override handling.
pub(crate) fn resolve_validated_recipe(
    recipe: &TextureRecipe,
    overrides: &RecipeParameterValues,
) -> Result<ResolvedTextureRecipe, RecipeParameterErrors> {
    let mut errors = RecipeParameterErrors::default();
    let declared = recipe
        .parameters
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    for (name, value) in overrides.iter() {
        if !declared.contains(name) {
            errors.push(RecipeParameterError::UnknownOverride {
                name: name.to_owned(),
            });
        }
        let colour = value.as_colour();
        if !colour.is_valid() {
            errors.push(RecipeParameterError::InvalidOverrideColour {
                name: name.to_owned(),
                colour,
            });
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut resolved = recipe.clone();
    resolve_colour(
        &mut resolved.albedo.base_color,
        "albedo.base_color",
        recipe,
        overrides,
        &mut errors,
    );
    resolve_colour(
        &mut resolved.albedo.warm_color,
        "albedo.warm_color",
        recipe,
        overrides,
        &mut errors,
    );
    for (index, colour) in resolved.albedo.palette.iter_mut().enumerate() {
        resolve_colour(
            colour,
            &format!("albedo.palette[{index}]"),
            recipe,
            overrides,
            &mut errors,
        );
    }
    for (layer_index, layer) in resolved.layers.iter_mut().enumerate() {
        match &mut layer.outputs.albedo.colour_map {
            ColourMap::Ramp { first, second } => {
                resolve_colour(
                    first,
                    &format!("layers[{layer_index}].outputs.albedo.colour_map.first"),
                    recipe,
                    overrides,
                    &mut errors,
                );
                resolve_colour(
                    second,
                    &format!("layers[{layer_index}].outputs.albedo.colour_map.second"),
                    recipe,
                    overrides,
                    &mut errors,
                );
            }
            ColourMap::Gradient { stops } => {
                for (stop_index, stop) in stops.iter_mut().enumerate() {
                    resolve_colour(
                        &mut stop.colour,
                        &format!(
                            "layers[{layer_index}].outputs.albedo.colour_map.stops[{stop_index}].colour"
                        ),
                        recipe,
                        overrides,
                        &mut errors,
                    );
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let parameters = resolved_values(recipe, overrides);
    let parameter_hash = hash_parameters(&parameters);
    Ok(ResolvedTextureRecipe {
        recipe: resolved,
        parameters,
        parameter_hash,
    })
}

fn resolve_colour(
    colour: &mut ColourValue,
    path: &str,
    recipe: &TextureRecipe,
    overrides: &RecipeParameterValues,
    errors: &mut RecipeParameterErrors,
) {
    let ColourValue::Parameter(reference) = colour else {
        return;
    };
    let Some(definition) = recipe.parameters.get(&reference.parameter) else {
        errors.push(RecipeParameterError::UnknownReference {
            path: path.to_owned(),
            name: reference.parameter.clone(),
        });
        return;
    };
    let default = definition.default_colour();
    let supplied = overrides
        .get(&reference.parameter)
        .map_or(default, ParameterValue::as_colour);
    let resolved = reference.base.map_or(supplied, |base| {
        tint_relative_to_default(base, default, supplied)
    });
    *colour = ColourValue::Literal(resolved);
}

fn tint_relative_to_default(base: LinearRgb, default: LinearRgb, supplied: LinearRgb) -> LinearRgb {
    if supplied == default {
        return base;
    }
    LinearRgb(std::array::from_fn(|index| {
        let default_channel = default.0[index];
        let shifted = if default_channel > f32::EPSILON {
            base.0[index] * supplied.0[index] / default_channel
        } else {
            base.0[index] + supplied.0[index] - default_channel
        };
        shifted.clamp(0.0, 1.0)
    }))
}

fn resolved_values(
    recipe: &TextureRecipe,
    overrides: &RecipeParameterValues,
) -> RecipeParameterValues {
    RecipeParameterValues(
        recipe
            .parameters
            .iter()
            .map(|(name, definition)| {
                let value = overrides
                    .get(name)
                    .unwrap_or_else(|| ParameterValue::colour(definition.default_colour()));
                (name.clone(), value)
            })
            .collect(),
    )
}

fn hash_parameters(parameters: &RecipeParameterValues) -> String {
    let encoded = serde_json::to_vec(parameters).expect("parameter maps are serializable");
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_tint_preserves_authored_default_exactly() {
        let base = LinearRgb::new(0.1, 0.3, 0.7);
        let default = LinearRgb::new(0.2, 0.4, 0.8);
        assert_eq!(tint_relative_to_default(base, default, default), base);
    }

    #[test]
    fn parameter_values_serialize_in_name_order() {
        let mut values = RecipeParameterValues::new();
        values.insert_colour("stone_colour", LinearRgb::new(0.4, 0.4, 0.4));
        values.insert_colour("dirt_colour", LinearRgb::new(0.2, 0.1, 0.05));
        assert_eq!(
            serde_json::to_string(&values).expect("serialize"),
            r#"{"dirt_colour":{"kind":"colour","value":[0.2,0.1,0.05]},"stone_colour":{"kind":"colour","value":[0.4,0.4,0.4]}}"#
        );
    }
}
