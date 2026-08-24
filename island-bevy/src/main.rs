//! Renders a procedurally generated island from the `motu` generator.

// Bevy systems receive their parameters by value; the lint fires on every one.
#![allow(clippy::needless_pass_by_value)]

mod camera;
mod convert;
mod island_gen;
mod lighting;
mod screenshot;
mod terrain;
mod vegetation;
mod water;

use std::{env, path::PathBuf, process};

use bevy::{prelude::*, window::WindowResolution};
use motu::IslandOptions;

use crate::{
    camera::FlyCameraPlugin,
    island_gen::{GenerationSettings, IslandGenPlugin},
    lighting::{LightingPlugin, SKY_COLOUR},
    screenshot::ScreenshotPlugin,
    terrain::TerrainPlugin,
    vegetation::VegetationPlugin,
    water::WaterPlugin,
};

#[derive(Debug)]
struct Command {
    seed: u64,
    options: IslandOptions,
    screenshot: Option<PathBuf>,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            seed: 666,
            options: IslandOptions {
                terrain_size: 256,
                ..IslandOptions::default()
            },
            screenshot: None,
        }
    }
}

fn main() {
    let command = match parse(env::args().skip(1)) {
        Ok(Some(command)) => command,
        Ok(None) => {
            print_help();
            return;
        }
        Err(error) => {
            eprintln!("island-bevy: {error}");
            process::exit(2);
        }
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: String::from("Motu island"),
            resolution: WindowResolution::new(1280, 720),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(SKY_COLOUR))
    .insert_resource(GenerationSettings {
        seed: command.seed,
        options: command.options,
    })
    .add_plugins((
        IslandGenPlugin,
        TerrainPlugin,
        WaterPlugin,
        VegetationPlugin,
        FlyCameraPlugin,
        LightingPlugin,
    ));
    if let Some(path) = command.screenshot {
        app.add_plugins(ScreenshotPlugin { path });
    }

    if let AppExit::Error(code) = app.run() {
        process::exit(i32::from(code.get()));
    }
}

fn parse(arguments: impl Iterator<Item = String>) -> Result<Option<Command>, String> {
    let mut command = Command::default();
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        let value = |arguments: &mut std::iter::Peekable<_>| {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} requires a value"))
        };
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--seed" => command.seed = parse_value(&argument, &value(&mut arguments)?)?,
            "--terrain-size" => {
                command.options.terrain_size = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--max-height" => {
                command.options.max_height = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--water-ratio" => {
                command.options.water_ratio = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--screenshot" => command.screenshot = Some(PathBuf::from(value(&mut arguments)?)),
            _ => return Err(format!("unknown option {argument:?}; use --help for usage")),
        }
    }
    Ok(Some(command))
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
        "island-bevy - renders the deterministic Motu island in Bevy\n\
         \n\
         Usage: island-bevy [OPTIONS]\n\
         \n\
         Options:\n\
           --seed <N>              Generation seed [default: 666]\n\
           --terrain-size <N>      Delaunay seed-point count [default: 256]\n\
                                   1024 matches the generator default but takes about 30 s\n\
           --max-height <HEIGHT>   Normalized maximum elevation [default: 0.2]\n\
           --water-ratio <RATIO>   Water coverage [default: 0.6]\n\
           --screenshot <PATH>     Capture one PNG once the island has settled, then exit\n\
           -h, --help              Print help"
    );
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_render_options() {
        let command = parse(
            [
                "--seed",
                "42",
                "--terrain-size",
                "128",
                "--screenshot",
                "out.png",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap()
        .unwrap();
        assert_eq!(command.seed, 42);
        assert_eq!(command.options.terrain_size, 128);
        assert_eq!(
            command.screenshot.as_deref(),
            Some(std::path::Path::new("out.png"))
        );
    }

    #[test]
    fn rejects_unknown_options() {
        let error = parse([String::from("--wat")].into_iter()).unwrap_err();
        assert!(error.contains("unknown option"));
    }
}
