//! Renders a procedurally generated island from the `motu` generator.

// Bevy systems receive their parameters by value; the lint fires on every one.
#![allow(clippy::needless_pass_by_value)]

mod cache;
mod camera;
mod convert;
mod hash;
mod hud;
mod island_gen;
mod lighting;
mod options;
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
    hud::HudPlugin,
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

    let capturing = command.screenshot.is_some();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: String::from("Motu island"),
            resolution: WindowResolution::new(1280, 720),
            // A capture run is not the person's own work, so its window never
            // takes the keyboard: whatever was being typed into stays the
            // window typing goes to, and the capture runs beside it.
            //
            // It cannot go further than that. A capture reads the window's own
            // surface back, and macOS only keeps a surface current while the
            // window is composited, so `visible: false`, minimizing it and
            // `WindowLevel::AlwaysOnBottom` each produce a valid PNG of solid
            // black — verified, one at a time, on this machine. Staying
            // unfocused is the most that can be given up and still capture.
            focused: !capturing,
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
        // The HUD is left out rather than hidden: nothing that is never built
        // can end up in a frame.
        app.add_plugins(ScreenshotPlugin { path });
    } else {
        app.add_plugins(HudPlugin);
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
            options::SEED_FLAG => command.seed = parse_value(&argument, &value(&mut arguments)?)?,
            options::TERRAIN_SIZE_FLAG => {
                command.options.terrain_size = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--variant" => {
                variant = value(&mut arguments)?;
                island_gen::apply_variant(&variant, &mut command.options)?;
            }
            "--view" => view = value(&mut arguments)?,
            "--screenshot" => command.screenshot = Some(PathBuf::from(value(&mut arguments)?)),
            "--no-cache" => command.no_cache = true,
            // Every remaining generator parameter is spelled by the table, so
            // the flags the HUD reports are exactly the flags read back here.
            // The lookup comes before the value is taken, or an unrecognised
            // flag would be reported as a missing value instead.
            flag => {
                let Some(parameter) = options::parameter(flag) else {
                    return Err(format!("unknown option {argument:?}; use --help for usage"));
                };
                let scalar = parse_value(&argument, &value(&mut arguments)?)?;
                *(parameter.field)(&mut command.options) = scalar;
            }
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

/// Written out rather than continued line by line, so the indentation in the
/// source is the indentation on the terminal.
fn print_help() {
    println!(
        "island-bevy - renders the deterministic Motu island in Bevy

Usage: island-bevy [OPTIONS]

Options are applied in the order given, so a later one wins.

Options:
  --seed <N>              Generation seed [default: 666]
  --terrain-size <N>      Delaunay seed-point count, 16 to 4096 [default: 1024]
                          first generation takes about 30 s, then the cache
                          makes repeat launches fast; use 128 or 256 to iterate
  --variant <NAME>        Named generation variant [default: default]
  --view <NAME>           Camera pose to open on and to reset to [default: overview]
                          The river and stream views carry a pose per variant,
                          so --variant moves them onto that island's channels
  --screenshot <PATH>     Capture one PNG once the island has settled, then exit
                          The window never takes the keyboard and no HUD is
                          built, so a capture stays out of whatever else is open
  --no-cache              Generate even when a cached island matches these
                          inputs; the entry is rewritten either way. Applies to
                          the HUD's rebuilds as well as to the first island
  -h, --help              Print help

Generator parameters, with the range the HUD offers each over. The ones the
generator validates are held to its own limits and the rest are working
ranges; a value outside either is still accepted here.
{}
Variants: {}
Views: {}

In the viewer, H shows and hides the parameter panel and F switches between
flying and walking. Flying steers with the mouse buttons and leaves the cursor
free; walking captures it and looks with no button held, Shift sprints, Space
jumps, and Escape hands the cursor back until you click. See README.md for the
rest of the controls.",
        options::help_lines(),
        island_gen::variant_names(),
        ViewPose::names()
    );
}

#[cfg(test)]
mod tests {
    use motu::IslandOptions;

    use super::{ViewPose, options, parse};

    fn command(arguments: &[&str]) -> Result<super::Command, String> {
        parse(arguments.iter().copied().map(String::from)).map(Option::unwrap)
    }

    /// The whole point of the reported line: an island found by dragging
    /// sliders has to open again from the command line, every parameter
    /// included and none of them rounded on the way.
    #[test]
    fn the_reported_command_line_parses_back_to_the_same_island() {
        let found = IslandOptions {
            max_height: 0.335,
            water_ratio: 0.72,
            slope_multiplier: 2.05,
            coastal_slope_multiplier: 0.4,
            hydraulic_erosion_strength: 5.5,
            hydraulic_deposition_strength: 0.75,
            hydraulic_deposition_slope_degrees: 27.5,
            river_source_catchment_hectares: 0.125,
            river_source_steep_multiplier: 6.5,
            river_source_elevation_boost: 3.25,
            river_source_width_metres: 4.5,
            river_maximum_width_metres: 22.0,
            river_source_depth_metres: 0.85,
            river_maximum_depth_metres: 3.5,
            terrain_size: 512,
        };
        let line = options::command_line(4242, &found);
        let reopened = command(&line.split(' ').collect::<Vec<_>>()).unwrap();
        assert_eq!(reopened.seed, 4242);
        assert_eq!(reopened.options, found);
    }

    /// An unknown flag is reported as unknown even though it stands where a
    /// generator parameter would, rather than as one missing its value.
    #[test]
    fn rejects_an_unknown_flag_before_taking_its_value() {
        for arguments in [&["--river-source-catchmnt-hectares"][..], &["--wat", "3"]] {
            let error = command(arguments).unwrap_err();
            assert!(error.contains("unknown option"), "{error}");
        }
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
