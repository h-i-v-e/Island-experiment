//! Renders a procedurally generated island from the `motu` generator.

// Bevy systems receive their parameters by value; the lint fires on every one.
#![allow(clippy::needless_pass_by_value)]

mod cache;
mod camera;
mod convert;
mod hash;
mod island_gen;
mod lighting;
mod screenshot;
mod surface;
mod terrain;
mod vegetation;
mod water;

use std::{env, path::PathBuf, process};

use bevy::{prelude::*, window::WindowResolution};
use motu::IslandOptions;

use crate::{
    camera::{FlyCameraPlugin, ViewPose},
    island_gen::{GenerationSettings, IslandGenPlugin},
    lighting::LightingPlugin,
    screenshot::ScreenshotPlugin,
    surface::SurfaceMaterialsPlugin,
    terrain::TerrainPlugin,
    vegetation::VegetationPlugin,
    water::WaterPlugin,
};

#[derive(Debug)]
struct Command {
    seed: u64,
    options: IslandOptions,
    view: ViewPose,
    screenshot: Option<PathBuf>,
    no_cache: bool,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            seed: 666,
            options: IslandOptions {
                terrain_size: 1024,
                ..IslandOptions::default()
            },
            view: ViewPose::default(),
            screenshot: None,
            no_cache: false,
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
    // The atmosphere covers the whole background, so the clear colour is only
    // ever seen if the GPU cannot run it.
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(command.view)
    .insert_resource(GenerationSettings {
        seed: command.seed,
        options: command.options,
        cache_reads: !command.no_cache,
    })
    .add_plugins((
        IslandGenPlugin,
        // Ground and water both draw through it, so the material registry is
        // registered once here rather than by whichever of them runs first.
        SurfaceMaterialsPlugin,
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

/// A view's pose can depend on the variant and the two options may arrive in
/// either order, so both names are collected first and the pose is resolved
/// once every argument has been read.
fn parse(arguments: impl Iterator<Item = String>) -> Result<Option<Command>, String> {
    let mut command = Command::default();
    let mut view = String::from(camera::DEFAULT_VIEW);
    let mut variant = String::from(island_gen::DEFAULT_VARIANT);
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
            "--variant" => {
                variant = value(&mut arguments)?;
                island_gen::apply_variant(&variant, &mut command.options)?;
            }
            "--view" => view = value(&mut arguments)?,
            "--screenshot" => command.screenshot = Some(PathBuf::from(value(&mut arguments)?)),
            "--no-cache" => command.no_cache = true,
            _ => return Err(format!("unknown option {argument:?}; use --help for usage")),
        }
    }
    command.view = ViewPose::named(&view, &variant)?;
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
         Options are applied in the order given, so a later one wins.\n\
         \n\
         Options:\n\
           --seed <N>              Generation seed [default: 666]\n\
           --terrain-size <N>      Delaunay seed-point count [default: 1024]\n\
                                   first generation takes about 30 s, then the cache\n\
                                   makes repeat launches fast; use 128 or 256 to iterate\n\
           --max-height <HEIGHT>   Normalized maximum elevation [default: 0.2]\n\
           --water-ratio <RATIO>   Water coverage [default: 0.6]\n\
           --variant <NAME>        Named generation variant [default: default]\n\
           --view <NAME>           Camera pose to open on and to reset to [default: overview]\n\
                                   The river and stream views carry a pose per variant,\n\
                                   so --variant moves them onto that island's channels\n\
           --screenshot <PATH>     Capture one PNG once the island has settled, then exit\n\
           --no-cache              Generate even when a cached island matches these\n\
                                   inputs; the entry is rewritten either way\n\
           -h, --help              Print help\n\
         \n\
         Variants: {}\n\
         Views: {}",
        island_gen::variant_names(),
        ViewPose::names()
    );
}

#[cfg(test)]
mod tests {
    use super::{ViewPose, parse};

    fn command(arguments: &[&str]) -> Result<super::Command, String> {
        parse(arguments.iter().copied().map(String::from)).map(Option::unwrap)
    }

    #[test]
    fn parses_render_options() {
        let command = command(&[
            "--seed",
            "42",
            "--terrain-size",
            "128",
            "--screenshot",
            "out.png",
        ])
        .unwrap();
        assert_eq!(command.seed, 42);
        assert_eq!(command.options.terrain_size, 128);
        assert_eq!(
            command.screenshot.as_deref(),
            Some(std::path::Path::new("out.png"))
        );
    }

    #[test]
    fn caching_is_on_unless_it_is_turned_off() {
        assert!(!command(&[]).unwrap().no_cache);
        assert!(command(&["--no-cache"]).unwrap().no_cache);
    }

    #[test]
    fn rejects_unknown_options() {
        let error = command(&["--wat"]).unwrap_err();
        assert!(error.contains("unknown option"));
    }

    #[test]
    fn selects_a_named_view() {
        assert_eq!(command(&[]).unwrap().view, ViewPose::default());
        let command = command(&["--view", "river-level4"]).unwrap();
        assert_eq!(
            command.view,
            ViewPose::named("river-level4", "default").unwrap()
        );
        assert_ne!(command.view, ViewPose::default());
    }

    #[test]
    fn rejects_unknown_views() {
        let error = command(&["--view", "summit"]).unwrap_err();
        assert!(error.contains("unknown view"));
        assert!(error.contains("overview"));
    }

    /// A view whose subject the variant moves takes its own pose; a view that
    /// frames the island as a whole keeps the shared one. Neither option has to
    /// come first.
    #[test]
    fn a_variant_moves_the_river_views_only() {
        for view in ["river-region", "river-ground", "river-level4", "stream"] {
            let shared = command(&["--view", view]).unwrap().view;
            let eroded = command(&["--view", view, "--variant", "eroded"])
                .unwrap()
                .view;
            assert_ne!(shared, eroded, "{view} kept its shared pose");
            let reversed = command(&["--variant", "eroded", "--view", view])
                .unwrap()
                .view;
            assert_eq!(eroded, reversed);
        }
        for view in ["overview", "mountain"] {
            let shared = command(&["--view", view]).unwrap().view;
            let eroded = command(&["--view", view, "--variant", "eroded"])
                .unwrap()
                .view;
            assert_eq!(shared, eroded, "{view} grew a variant pose");
        }
    }

    /// The variant sets the erosion strength, and an option spelled out after
    /// it still wins.
    #[test]
    fn applies_a_named_variant_in_order() {
        let defaults = command(&[]).unwrap().options;
        let eroded = command(&["--variant", "eroded"]).unwrap().options;
        assert!(eroded.hydraulic_erosion_strength > defaults.hydraulic_erosion_strength);
        assert!(eroded.coastal_slope_multiplier < defaults.coastal_slope_multiplier);
        assert_eq!(eroded.terrain_size, defaults.terrain_size);

        let overridden = command(&["--variant", "eroded", "--max-height", "0.3"])
            .unwrap()
            .options;
        assert!((overridden.max_height - 0.3).abs() < f32::EPSILON);
        assert!(
            (overridden.hydraulic_erosion_strength - eroded.hydraulic_erosion_strength).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn rejects_unknown_variants() {
        let error = command(&["--variant", "flooded"]).unwrap_err();
        assert!(error.contains("unknown variant"));
        assert!(error.contains("eroded"));
    }
}
