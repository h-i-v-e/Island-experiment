use std::{env, error::Error, path::PathBuf, process, time::Instant};

use motu::{CoastalStripOptions, Island, IslandOptions, write_png};

#[derive(Debug)]
struct Command {
    seed: u64,
    width: u32,
    height: u32,
    output: PathBuf,
    options: IslandOptions,
    coastal_strip: bool,
    coast: CoastalStripOptions,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            seed: 666,
            width: 1024,
            height: 1024,
            output: PathBuf::from("test.png"),
            options: IslandOptions::default(),
            coastal_strip: false,
            coast: CoastalStripOptions::default(),
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("island: {error}");
        process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let Some(command) = parse(env::args().skip(1))? else {
        print_help();
        return Ok(());
    };
    let started = Instant::now();
    let island = if command.coastal_strip {
        Island::generate_coastal_strip(command.seed, command.options, command.coast)?
    } else {
        Island::generate(command.seed, command.options)?
    };
    let raster = island.render(command.width, command.height);
    write_png(
        &command.output,
        raster.width(),
        raster.height(),
        raster.pixels(),
    )?;
    println!(
        "wrote {} ({}x{}, seed {}, {} terrain vertices, {} rivers) in {:.2?}",
        command.output.display(),
        command.width,
        command.height,
        command.seed,
        island.terrain().vertex_count(),
        island.rivers().len(),
        started.elapsed()
    );
    Ok(())
}

#[allow(clippy::too_many_lines)] // One declarative match keeps CLI flags and their destination fields together.
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
            "--width" => command.width = parse_value(&argument, &value(&mut arguments)?)?,
            "--height" => command.height = parse_value(&argument, &value(&mut arguments)?)?,
            "-o" | "--output" => command.output = PathBuf::from(value(&mut arguments)?),
            "--terrain-shape" => {
                command.coastal_strip = match value(&mut arguments)?.as_str() {
                    "island" => false,
                    "coastal-strip" => true,
                    other => {
                        return Err(format!(
                            "unknown terrain shape {other:?}; expected island or coastal-strip"
                        ));
                    }
                };
            }
            "--coastal-section-index" => {
                command.coast.section_index = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--coastal-section-count" => {
                command.coast.section_count = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--coastal-seam-segments" => {
                command.coast.seam_segments = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--coastal-seam-blend-width" => {
                command.coast.seam_blend_width = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--coastal-seam-ridge-height" => {
                command.coast.seam_ridge_height = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--terrain-size" | "--seed-points" => {
                command.options.terrain_size = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--water-ratio" => {
                command.options.water_ratio = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--max-height" => {
                command.options.max_height = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--seabed-depth-distance-ratio" => {
                command.options.seabed_depth_distance_ratio =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--initial-soil-depth-metres" => {
                command.options.initial_soil_depth_metres =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--hydraulic-erosion-strength" => {
                command.options.hydraulic_erosion_strength =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--hydraulic-deposition-strength" => {
                command.options.hydraulic_deposition_strength =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--hydraulic-deposition-slope" => {
                command.options.hydraulic_deposition_slope_degrees =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--river-source-catchment-hectares" => {
                command.options.river_source_catchment_hectares =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--river-source-steep-catchment-multiplier" => {
                command.options.river_source_steep_catchment_multiplier =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--river-source-elevation-boost" => {
                command.options.river_source_elevation_boost =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            _ => return Err(format!("unknown option {argument:?}; use --help for usage")),
        }
    }
    if command.width == 0 || command.height == 0 {
        return Err("image width and height must be greater than zero".into());
    }
    if command.coastal_strip {
        command.coast.validate(command.options)?;
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
        "island - deterministic procedural island generator\n\
         \n\
         Usage: island [OPTIONS]\n\
         \n\
         Options:\n\
           --seed <N>             Generation seed [default: 666]\n\
           --width <PX>           Output width [default: 1024]\n\
           --height <PX>          Output height [default: 1024]\n\
           -o, --output <PATH>    PNG destination [default: test.png]\n\
           --terrain-shape <SHAPE> island or coastal-strip [default: island]\n\
           --coastal-section-index <N> Canonical section index [default: 0]\n\
           --coastal-section-count <N> Repeat period in sections [default: 8]\n\
           --coastal-seam-segments <N> Fixed edge intervals [default: 32]\n\
           --coastal-seam-blend-width <F> Normalized seam collar [default: 0.08]\n\
           --coastal-seam-ridge-height <F> Extra normalized seam height [default: 0.02]\n\
           --seed-points <N>      Delaunay seed-point count [default: 1024]\n\
           --terrain-size <N>     Alias for --seed-points\n\
           --water-ratio <RATIO>  Water coverage [default: 0.6]\n\
           --max-height <HEIGHT>  Normalized maximum elevation [default: 0.2]\n\
           --seabed-depth-distance-ratio <RATIO>\n\
                                  Final seabed drop / distance from land [default: 0.1]\n\
           --initial-soil-depth-metres <METRES>\n\
                                  Initial loose soil on land [default: 0]\n\
           --hydraulic-erosion-strength <S>\n\
                                  Hydraulic erosion multiplier from 0 to 8 [default: 1]\n\
           --hydraulic-deposition-strength <S>\n\
                                  Gentle-slope deposition rate from 0 to 4 [default: 1.5]\n\
           --hydraulic-deposition-slope <DEGREES>\n\
                                  Angle where deposition reaches zero [default: 12]\n\
           --river-source-catchment-hectares <HECTARES>\n\
                                  Upstream drainage area required [default: 0.05]\n\
           --river-source-steep-catchment-multiplier <S>\n\
                                  Near-vertical source penalty [default: 4]\n\
           --river-source-elevation-boost <FACTOR>\n\
                                  Extra sea-level catchment multiplier [default: 9]\n\
           -h, --help             Print help"
    );
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn coastal_sections_are_explicit_and_validate_the_repeat_identity() {
        let command = parse(
            [
                "--terrain-shape",
                "coastal-strip",
                "--coastal-section-count",
                "4",
                "--coastal-section-index",
                "3",
                "--coastal-seam-segments",
                "33",
                "--coastal-seam-blend-width",
                "0.1",
                "--coastal-seam-ridge-height",
                "0",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap()
        .unwrap();
        assert!(command.coastal_strip);
        assert_eq!(command.coast.section_count, 4);
        assert_eq!(command.coast.section_index, 3);
        assert_eq!(command.coast.seam_segments, 33);
        for arguments in [
            vec!["--terrain-shape", "cylinder"],
            vec![
                "--terrain-shape",
                "coastal-strip",
                "--coastal-section-count",
                "0",
            ],
            vec![
                "--terrain-shape",
                "coastal-strip",
                "--coastal-section-index",
                "8",
            ],
        ] {
            assert!(parse(arguments.into_iter().map(str::to_owned)).is_err());
        }
        assert!(!parse(std::iter::empty()).unwrap().unwrap().coastal_strip);
    }

    #[test]
    fn parses_generation_options() {
        let command = parse(
            [
                "--seed",
                "42",
                "--width",
                "320",
                "--water-ratio",
                "0.7",
                "--initial-soil-depth-metres",
                "2.5",
                "--seabed-depth-distance-ratio",
                "0.25",
                "--hydraulic-erosion-strength",
                "1.5",
                "--hydraulic-deposition-strength",
                "2.5",
                "--hydraulic-deposition-slope",
                "15",
                "--river-source-catchment-hectares",
                "0.75",
                "--river-source-steep-catchment-multiplier",
                "5.25",
                "--river-source-elevation-boost",
                "8.5",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap()
        .unwrap();
        assert!((command.options.initial_soil_depth_metres - 2.5).abs() < f32::EPSILON);
        assert!((command.options.seabed_depth_distance_ratio - 0.25).abs() < f32::EPSILON);
        assert_eq!(command.seed, 42);
        assert_eq!(command.width, 320);
        assert!((command.options.water_ratio - 0.7).abs() < f32::EPSILON);
        assert!((command.options.hydraulic_erosion_strength - 1.5).abs() < f32::EPSILON);
        assert!((command.options.hydraulic_deposition_strength - 2.5).abs() < f32::EPSILON);
        assert!((command.options.hydraulic_deposition_slope_degrees - 15.0).abs() < f32::EPSILON);
        assert!((command.options.river_source_catchment_hectares - 0.75).abs() < f32::EPSILON);
        assert!(
            (command.options.river_source_steep_catchment_multiplier - 5.25).abs() < f32::EPSILON
        );
        assert!((command.options.river_source_elevation_boost - 8.5).abs() < f32::EPSILON);
    }

    #[test]
    fn rejects_unknown_options() {
        let error = parse([String::from("--wat")].into_iter()).unwrap_err();
        assert!(error.contains("unknown option"));
    }
}
