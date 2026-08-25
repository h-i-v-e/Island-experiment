//! Standalone procedural texture baker.
//!
//! Recipe parsing and generation stay in `island-rs`; this binary only owns
//! command-line concerns, JSON loading, and reporting.  That keeps the same
//! deterministic generator available to in-process callers.

use std::{env, error::Error, fs, path::PathBuf, process, time::Instant};

use motu::procedural_textures::encoding::{
    TextureSetImages, write_texture_set as write_encoded_texture_set,
};
use motu::{OutputOptions, OutputProfile, TextureRecipe, generate_texture_set};
use serde_json::Value;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
struct Command {
    recipe: Option<PathBuf>,
    output: Option<PathBuf>,
    profile: OutputProfile,
    force: bool,
    seed: Option<u64>,
    width: Option<u32>,
    height: Option<u32>,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            recipe: None,
            output: None,
            profile: OutputProfile::Separate,
            force: false,
            seed: None,
            width: None,
            height: None,
        }
    }
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(error) => {
            eprintln!("island-texture-baker: {error}");
            process::exit(2);
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments = env::args().skip(1);
    let command = match parse(arguments)? {
        ParseResult::Help => {
            print_help();
            return Ok(());
        }
        ParseResult::Version => {
            println!("island-texture-baker {VERSION}");
            return Ok(());
        }
        ParseResult::Command(command) => command,
    };

    let recipe_path = command.recipe.as_deref().ok_or("--recipe is required")?;
    let output = command.output.as_deref().ok_or("--output is required")?;
    let recipe_bytes = fs::read(recipe_path)
        .map_err(|error| format!("could not read recipe {}: {error}", recipe_path.display()))?;
    let recipe = load_recipe(&recipe_bytes, &command)?;

    let started = Instant::now();
    let textures = generate_texture_set(&recipe)?;
    let options = OutputOptions {
        profile: command.profile,
        force: command.force,
    };
    let images = TextureSetImages::from_texture_set(&textures);
    let manifest = write_encoded_texture_set(&images, output, &options)?;
    println!(
        "baked {} ({}x{}, seed {}) in {:.2?}",
        manifest.name,
        manifest.dimensions.width,
        manifest.dimensions.height,
        manifest.metadata.seed,
        started.elapsed()
    );
    for map in &manifest.maps {
        println!("  {}  sha256={}", map.file, map.sha256);
    }
    Ok(())
}

fn load_recipe(bytes: &[u8], command: &Command) -> Result<TextureRecipe, Box<dyn Error>> {
    let mut document: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("recipe is not valid UTF-8 JSON: {error}"))?;
    apply_overrides(&mut document, command)?;
    Ok(serde_json::from_value(document)?)
}

fn apply_overrides(document: &mut Value, command: &Command) -> Result<(), Box<dyn Error>> {
    let object = document
        .as_object_mut()
        .ok_or("recipe root must be a JSON object")?;
    if let Some(seed) = command.seed {
        object.insert("seed".into(), Value::from(seed));
    }

    if let Some(width) = command.width {
        set_dimension(object, "width", width)?;
    }
    if let Some(height) = command.height {
        set_dimension(object, "height", height)?;
    }
    Ok(())
}

fn set_dimension(
    root: &mut serde_json::Map<String, Value>,
    key: &str,
    value: u32,
) -> Result<(), Box<dyn Error>> {
    // Recipes written with the initial schema use top-level width/height;
    // accepting a nested `dimensions` object makes the CLI tolerant of the
    // common engine-facing representation without weakening deserialization.
    if root.contains_key(key) {
        root.insert(key.into(), Value::from(value));
    } else {
        root.entry("dimensions")
            .or_insert_with(|| Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .ok_or("recipe dimensions must be a JSON object")?
            .insert(key.into(), Value::from(value));
    }
    Ok(())
}

enum ParseResult {
    Help,
    Version,
    Command(Command),
}

fn parse(arguments: impl Iterator<Item = String>) -> Result<ParseResult, String> {
    let mut command = Command {
        profile: OutputProfile::Separate,
        ..Command::default()
    };
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        let value = |arguments: &mut std::iter::Peekable<_>| {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} requires a value"))
        };
        match argument.as_str() {
            "-h" | "--help" => return Ok(ParseResult::Help),
            "-V" | "--version" => return Ok(ParseResult::Version),
            "--recipe" => command.recipe = Some(PathBuf::from(value(&mut arguments)?)),
            "--output" | "-o" => command.output = Some(PathBuf::from(value(&mut arguments)?)),
            "--profile" => {
                command.profile = value(&mut arguments)?
                    .parse::<OutputProfile>()
                    .map_err(|error| error.to_string())?;
            }
            "--force" => command.force = true,
            "--seed" => command.seed = Some(parse_value(&argument, &value(&mut arguments)?)?),
            "--width" => command.width = Some(parse_value(&argument, &value(&mut arguments)?)?),
            "--height" => {
                command.height = Some(parse_value(&argument, &value(&mut arguments)?)?);
            }
            _ => return Err(format!("unknown option {argument:?}; use --help for usage")),
        }
    }
    if command.recipe.is_none() {
        return Err("--recipe is required; use --help for usage".into());
    }
    if command.output.is_none() {
        return Err("--output is required; use --help for usage".into());
    }
    if command.width == Some(0) || command.height == Some(0) {
        return Err("image width and height must be greater than zero".into());
    }
    Ok(ParseResult::Command(command))
}

fn parse_value<T>(option: &str, input: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    input
        .parse()
        .map_err(|error| format!("invalid value for {option}: {error}"))
}

fn print_help() {
    println!(
        "island-texture-baker {VERSION}\n\n\
         Usage: island-texture-baker --recipe <FILE> --output <DIR> [OPTIONS]\n\n\
         Required:\n\
           --recipe <FILE>       UTF-8 JSON texture recipe\n\
           -o, --output <DIR>    Destination directory\n\n\
         Options:\n\
           --profile <PROFILE>   separate (default) or motu_unity_terrain\n\
           --seed <N>            Override recipe seed\n\
           --width <PX>          Override recipe width\n\
           --height <PX>         Override recipe height\n\
           --force               Replace an existing generated set\n\
           -h, --help            Print help\n\
           -V, --version         Print version"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_arguments_and_overrides() {
        let ParseResult::Command(command) = parse(
            [
                "--recipe",
                "stone.json",
                "--output",
                "out",
                "--seed",
                "42",
                "--width",
                "128",
                "--profile",
                "motu_unity_terrain",
                "--force",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap() else {
            panic!("expected command");
        };
        assert_eq!(command.seed, Some(42));
        assert_eq!(command.width, Some(128));
        assert_eq!(command.profile, OutputProfile::MotuUnityTerrain);
        assert!(command.force);
    }

    #[test]
    fn help_and_version_do_not_require_paths() {
        assert!(matches!(
            parse(["--help".into()].into_iter()),
            Ok(ParseResult::Help)
        ));
        assert!(matches!(
            parse(["--version".into()].into_iter()),
            Ok(ParseResult::Version)
        ));
    }

    #[test]
    fn applies_top_level_and_nested_dimension_overrides() {
        let mut top_level = serde_json::json!({"seed": 1, "width": 2, "height": 3});
        let command = Command {
            width: Some(8),
            height: Some(9),
            ..Command::default()
        };
        apply_overrides(&mut top_level, &command).unwrap();
        assert_eq!(top_level["width"], 8);
        assert_eq!(top_level["height"], 9);

        let mut nested = serde_json::json!({"seed": 1, "dimensions": {"width": 2, "height": 3}});
        apply_overrides(&mut nested, &command).unwrap();
        assert_eq!(nested["dimensions"]["width"], 8);
        assert_eq!(nested["dimensions"]["height"], 9);
    }
}
