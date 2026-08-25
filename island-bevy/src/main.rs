//! Renders a procedurally generated island from the `motu` generator.

// Bevy systems receive their parameters by value; the lint fires on every one.
#![allow(clippy::needless_pass_by_value)]

mod app;
mod environment;
mod hash;
mod island;
mod math;
mod render;
mod shaders;

// Keep the original crate-level module paths stable while the source tree is
// grouped by ownership. Internal callers can continue to use `crate::camera`,
// `crate::surface`, and so on without treating folder names as API.
pub(crate) use app::{camera, hud, options, presets, screenshot};
pub(crate) use environment::{clouds, lighting, mist, spray, vegetation, weather};
pub(crate) use island::{cache, chunk, convert, island_gen};
pub(crate) use render::{budget, capture, surface, terrain, water};

use std::{env, path::PathBuf, process, time::Duration};

use bevy::{
    app::ScheduleRunnerPlugin,
    prelude::*,
    window::{ExitCondition, WindowResolution},
    winit::WinitPlugin,
};
use motu::{GenerationMethod, IslandOptions};

use crate::{
    budget::BudgetPlugin,
    camera::{FlyCameraPlugin, ViewPose},
    capture::{CapturePlugin, DebugView},
    clouds::CloudPlugin,
    hud::HudPlugin,
    island_gen::{GenerationSettings, IslandGenPlugin},
    lighting::LightingPlugin,
    mist::MistPlugin,
    screenshot::ScreenshotPlugin,
    spray::SprayPlugin,
    surface::SurfaceMaterialsPlugin,
    terrain::TerrainPlugin,
    vegetation::VegetationPlugin,
    water::WaterPlugin,
    weather::{Weather, WeatherPlugin},
};

/// The size the viewer's window opens at, in logical pixels.
///
/// A capture no longer opens one and states its own size instead, so this is
/// the only place the two can disagree; `screenshot` holds them to the same
/// aspect ratio, which is what keeps a `--view` framing the same thing on
/// screen as in its capture.
pub const WINDOW_RESOLUTION: UVec2 = UVec2::new(1280, 720);

#[derive(Debug)]
struct Command {
    seed: u64,
    options: IslandOptions,
    method: GenerationMethod,
    view: ViewPose,
    /// The `--view` and `--variant` names the pose and the options above were
    /// resolved from. Neither resolved value carries its own name, and a
    /// capture's metadata has to report both.
    view_name: String,
    variant_name: String,
    weather: Weather,
    debug_view: DebugView,
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
            method: GenerationMethod::Cpu,
            view: ViewPose::default(),
            view_name: String::from(camera::DEFAULT_VIEW),
            variant_name: String::from(island_gen::DEFAULT_VARIANT),
            weather: Weather::default(),
            debug_view: DebugView::default(),
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
    // A capture run opens nothing at all: no primary window is asked for, so
    // winit's event loop is left out with it and `screenshot`'s offscreen image
    // is what the camera renders into. Nothing is composited, so a capture can
    // neither be raised over the work in front of it nor come back solid black
    // — which is what reading a window surface back used to risk, macOS keeping
    // a surface current only while its window is on screen. `visible: false`,
    // minimizing and `WindowLevel::AlwaysOnBottom` were each tried against that
    // and each produced a valid PNG of black; not having a surface is what
    // settles it.
    let mut plugins = DefaultPlugins.set(if capturing {
        WindowPlugin {
            primary_window: None,
            // With no window to close, the run ends on the AppExit the capture
            // writes for itself and not before.
            exit_condition: ExitCondition::DontExit,
            ..default()
        }
    } else {
        WindowPlugin {
            primary_window: Some(Window {
                title: String::from("Motu island"),
                resolution: WindowResolution::new(WINDOW_RESOLUTION.x, WINDOW_RESOLUTION.y),
                ..default()
            }),
            ..default()
        }
    });
    if capturing {
        plugins = plugins.disable::<WinitPlugin>();
    }
    app.add_plugins(plugins);
    if capturing {
        // The winit runner went with the event loop, so the frames need
        // driving. Zero wait runs them as fast as the renderer will take them,
        // and every part of the settle is counted in frames, so how quickly
        // they arrive is not something the capture can read.
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
    }
    // The atmosphere covers the whole background, so the clear colour is only
    // ever seen if the GPU cannot run it.
    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(command.view)
        .insert_resource(command.weather)
        .insert_resource(GenerationSettings {
            seed: command.seed,
            options: command.options,
            method: command.method,
            cache_reads: !command.no_cache,
        })
        .add_plugins((
            IslandGenPlugin,
            // Counts what the culling stages left standing, which is what the
            // capture log and the panel both report.
            BudgetPlugin,
            // Ground and water both draw through it, so the material registry is
            // registered once here rather than by whichever of them runs first.
            SurfaceMaterialsPlugin,
            TerrainPlugin,
            WaterPlugin,
            SprayPlugin,
            VegetationPlugin,
            FlyCameraPlugin,
            LightingPlugin,
            // The named look, and the two things it builds: the cloud layer and
            // the shadow it hangs off the sun, and the mist in the valleys.
            WeatherPlugin,
            CloudPlugin,
            MistPlugin,
            // A capture freezes the water clock; every other run advances it.
            CapturePlugin {
                debug_view: command.debug_view,
                frozen: capturing,
            },
        ));
    if let Some(path) = command.screenshot {
        // The HUD is left out rather than hidden: nothing that is never built
        // can end up in a frame.
        app.add_plugins(ScreenshotPlugin {
            path,
            view: command.view_name,
            variant: command.variant_name,
        });
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
    // A later variant replaces the earlier variant's own writes without
    // erasing unrelated options the user supplied between them.
    let variant_base = command.options;
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
            options::GENERATION_METHOD_FLAG => {
                command.method = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--variant" => {
                let selected = value(&mut arguments)?;
                island_gen::replace_variant(&selected, &variant_base, &mut command.options)?;
                variant = selected;
            }
            "--view" => view = value(&mut arguments)?,
            "--weather" => command.weather = Weather::named(&value(&mut arguments)?)?,
            "--debug-view" => {
                command.debug_view = DebugView::named(&value(&mut arguments)?)?;
            }
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
    command.view_name = view;
    command.variant_name = variant;
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
  --generation-method <METHOD>
                          Terrain generation method: cpu or gpu [default: cpu]
  --variant <NAME>        Named generation variant [default: default]
  --view <NAME>           Camera pose to open on and to reset to [default: overview]
                          The river and stream views carry a pose per variant,
                          so --variant moves them onto that island's channels
  --weather <NAME>        Named weather look [default: clear]. Sun, haze, cloud
                          and its ground shadow, valley and waterfall mist and
                          the grade, as one set. `clear` is the renderer with
                          none of them, and the baseline the rest are read
                          against; the HUD offers the same list
  --debug-view <NAME>     Switch the terrain and water surfaces to one
                          diagnostic channel [default: off]. The HUD offers the
                          same list; a capture can be taken of any of them
  --screenshot <PATH>     Capture one PNG once the island has settled, then exit
                          Nothing opens: the scene renders into an offscreen
                          2560x1440 image and no HUD is built, so a capture
                          stays out of whatever else is on screen. The water
                          clock is frozen and the settle is a fixed frame count,
                          so one command captures the same frame twice.
                          <PATH>.txt records every input
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
Weather: {}
Debug views: {}

In the viewer, H and the corner button both show and hide the menu panel — ten
showcase presets, every generator parameter, the weather and debug lists, and a
button that copies the arguments for the island on screen — and F switches
between flying and walking. Flying steers with the mouse buttons, leaves the
cursor free and will not go through the ground; walking captures the cursor and
looks with no button held, Shift sprints, Space jumps, and Escape hands the
cursor back until you click. See README.md for the rest of the controls.",
        options::help_lines(),
        island_gen::variant_names(),
        ViewPose::names(),
        Weather::names(),
        DebugView::names()
    );
}

#[cfg(test)]
mod tests {
    use motu::{GenerationMethod, IslandOptions};

    use super::{DebugView, ViewPose, Weather, options, parse};

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
        let line = options::command_line(4242, &found, GenerationMethod::Gpu);
        let reopened = command(&line.split(' ').collect::<Vec<_>>()).unwrap();
        assert_eq!(reopened.seed, 4242);
        assert_eq!(reopened.options, found);
        assert_eq!(reopened.method, GenerationMethod::Gpu);
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
        assert_eq!(command.method, GenerationMethod::Cpu);
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
    fn rejects_unknown_generation_methods() {
        let error = command(&["--generation-method", "cuda"]).unwrap_err();
        assert!(error.contains("expected cpu or gpu"), "{error}");
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

    /// A named variant describes one coherent set of overrides. Replacing it
    /// with `default` must remove the first variant's writes as well as its
    /// name, and selecting it again must restore exactly the same set.
    #[test]
    fn a_later_variant_replaces_the_earlier_variant() {
        let defaults = command(&[]).unwrap().options;
        let eroded = command(&["--variant", "eroded"]).unwrap().options;

        let restored = command(&["--variant", "eroded", "--variant", "default"]).unwrap();
        assert_eq!(restored.options, defaults);
        assert_eq!(restored.variant_name, "default");

        let selected_again = command(&[
            "--variant",
            "eroded",
            "--variant",
            "default",
            "--variant",
            "eroded",
        ])
        .unwrap();
        assert_eq!(selected_again.options, eroded);
        assert_eq!(selected_again.variant_name, "eroded");
    }

    /// Variant replacement touches only variant-owned fields, while an
    /// explicit value targeting one of those fields still wins or loses by
    /// its position in the argument list.
    #[test]
    fn variant_replacement_preserves_left_to_right_option_order() {
        let defaults = command(&[]).unwrap().options;
        let eroded = command(&["--variant", "eroded"]).unwrap().options;

        let before = command(&["--hydraulic-erosion-strength", "2", "--variant", "eroded"])
            .unwrap()
            .options;
        assert!(
            (before.hydraulic_erosion_strength - eroded.hydraulic_erosion_strength).abs()
                < f32::EPSILON
        );

        let after = command(&["--variant", "eroded", "--hydraulic-erosion-strength", "2"])
            .unwrap()
            .options;
        assert!((after.hydraulic_erosion_strength - 2.0).abs() < f32::EPSILON);

        let replaced = command(&[
            "--variant",
            "eroded",
            "--max-height",
            "0.3",
            "--hydraulic-erosion-strength",
            "2",
            "--variant",
            "default",
        ])
        .unwrap()
        .options;
        assert!((replaced.max_height - 0.3).abs() < f32::EPSILON);
        assert!(
            (replaced.hydraulic_erosion_strength - defaults.hydraulic_erosion_strength).abs()
                < f32::EPSILON
        );
        assert!(
            (replaced.coastal_slope_multiplier - defaults.coastal_slope_multiplier).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn rejects_unknown_variants() {
        let error = command(&["--variant", "flooded"]).unwrap_err();
        assert!(error.contains("unknown variant"));
        assert!(error.contains("eroded"));
    }

    /// Ordinary shading unless a channel was asked for by name.
    #[test]
    fn selects_a_debug_view() {
        assert_eq!(command(&[]).unwrap().debug_view, DebugView::Off);
        for view in DebugView::ALL {
            let command = command(&["--debug-view", view.label()]).unwrap();
            assert_eq!(command.debug_view, view);
        }
    }

    /// A misspelled channel has to stop the run. A capture that quietly carried
    /// the ordinary scene instead would be read as the diagnostic it was asked
    /// for.
    #[test]
    fn rejects_unknown_debug_views() {
        let error = command(&["--debug-view", "curvature"]).unwrap_err();
        assert!(error.contains("unknown debug view"), "{error}");
        assert!(error.contains("weights"), "{error}");
        assert!(error.contains("depth"), "{error}");
    }

    /// Every look is selectable by name, and the one a run opens on when none
    /// is asked for is `clear` — which is the renderer without weather, and so
    /// the baseline every capture taken before it existed is still read
    /// against.
    #[test]
    fn selects_a_named_weather_look() {
        assert_eq!(command(&[]).unwrap().weather, Weather::default());
        assert_eq!(command(&[]).unwrap().weather.label(), "clear");
        for weather in Weather::all() {
            let command = command(&["--weather", weather.label()]).unwrap();
            assert_eq!(command.weather, weather);
        }
        assert_ne!(
            command(&["--weather", "overcast"]).unwrap().weather,
            Weather::default()
        );
    }

    /// A misspelled look has to stop the run with the valid ones listed. A
    /// capture that quietly carried `clear` instead would be read as the look
    /// it was asked for.
    #[test]
    fn rejects_unknown_weather() {
        let error = command(&["--weather", "sunny"]).unwrap_err();
        assert!(error.contains("unknown weather"), "{error}");
        for weather in Weather::all() {
            assert!(error.contains(weather.label()), "{error}");
        }
    }

    /// The capture metadata reports the names, not the pose and the option
    /// overrides they resolved to, so the parser has to keep both.
    #[test]
    fn keeps_the_view_and_variant_names() {
        let defaults = command(&[]).unwrap();
        assert_eq!(defaults.view_name, "overview");
        assert_eq!(defaults.variant_name, "default");
        let named = command(&["--view", "stream", "--variant", "eroded"]).unwrap();
        assert_eq!(named.view_name, "stream");
        assert_eq!(named.variant_name, "eroded");
    }
}
