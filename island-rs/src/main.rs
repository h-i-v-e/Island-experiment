use std::{env, error::Error, path::PathBuf, process, time::Instant};

use motu::{Island, IslandOptions, write_png};

#[derive(Debug)]
struct Command {
    seed: u64,
    width: u32,
    height: u32,
    output: PathBuf,
    options: IslandOptions,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            seed: 666,
            width: 1024,
            height: 1024,
            output: PathBuf::from("test.png"),
            options: IslandOptions::default(),
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
    let island = Island::generate(command.seed, command.options)?;
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
            "--terrain-size" | "--seed-points" => {
                command.options.terrain_size = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--water-ratio" => {
                command.options.water_ratio = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--max-height" => {
                command.options.max_height = parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--coastal-erosion-strength" => {
                command.options.coastal_erosion_strength =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--beach-formation-strength" => {
                command.options.beach_formation_strength =
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
            "--cliff-render-strength" => {
                command.options.cliff_render_strength =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--river-lod2-threshold" => {
                command.options.river_lod2_source_threshold =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--river-lod1-threshold" => {
                command.options.river_lod1_source_threshold =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--river-broad-threshold" => {
                command.options.river_broad_source_threshold =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--river-land-threshold" => {
                command.options.river_land_source_threshold =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            "--river-final-threshold" => {
                command.options.river_final_source_threshold =
                    parse_value(&argument, &value(&mut arguments)?)?;
            }
            _ => return Err(format!("unknown option {argument:?}; use --help for usage")),
        }
    }
    if command.width == 0 || command.height == 0 {
        return Err("image width and height must be greater than zero".into());
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
           --seed-points <N>      Delaunay seed-point count [default: 1024]\n\
           --terrain-size <N>     Alias for --seed-points\n\
           --water-ratio <RATIO>  Water coverage [default: 0.6]\n\
           --max-height <HEIGHT>  Normalized maximum elevation [default: 0.2]\n\
           --coastal-erosion-strength <S>\n\
                                  Wave erosion and rocky coast strength [default: 1]\n\
           --beach-formation-strength <S>\n\
                                  Sheltered sediment deposition strength [default: 1]\n\
           --hydraulic-erosion-strength <S>\n\
                                  Hydraulic erosion multiplier from 0 to 8 [default: 1]\n\
           --hydraulic-deposition-strength <S>\n\
                                  Gentle-slope deposition rate from 0 to 4 [default: 1.5]\n\
           --hydraulic-deposition-slope <DEGREES>\n\
                                  Angle where deposition reaches zero [default: 12]\n\
           --cliff-render-strength <S>\n\
                                  Accepted for compatibility; currently ignored\n\
           --river-lod2-threshold <SD>    Coarse river source threshold [default: 0.35]\n\
           --river-lod1-threshold <SD>    Medium river source threshold [default: 0.65]\n\
           --river-broad-threshold <SD>   Broad LOD 0 threshold [default: 1.0]\n\
           --river-land-threshold <SD>    Land-refined threshold [default: 1.3]\n\
           --river-final-threshold <SD>   Final-detail threshold [default: 1.6]\n\
           -h, --help             Print help"
    );
}

#[cfg(test)]
mod tests {
    use super::parse;

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
                "--coastal-erosion-strength",
                "2.5",
                "--beach-formation-strength",
                "3.0",
                "--hydraulic-erosion-strength",
                "1.5",
                "--hydraulic-deposition-strength",
                "2.5",
                "--hydraulic-deposition-slope",
                "15",
                "--river-final-threshold",
                "2.25",
                "--cliff-render-strength",
                "1.75",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap()
        .unwrap();
        assert_eq!(command.seed, 42);
        assert_eq!(command.width, 320);
        assert!((command.options.water_ratio - 0.7).abs() < f32::EPSILON);
        assert!((command.options.coastal_erosion_strength - 2.5).abs() < f32::EPSILON);
        assert!((command.options.beach_formation_strength - 3.0).abs() < f32::EPSILON);
        assert!((command.options.hydraulic_erosion_strength - 1.5).abs() < f32::EPSILON);
        assert!((command.options.hydraulic_deposition_strength - 2.5).abs() < f32::EPSILON);
        assert!((command.options.hydraulic_deposition_slope_degrees - 15.0).abs() < f32::EPSILON);
        assert!((command.options.river_final_source_threshold - 2.25).abs() < f32::EPSILON);
        assert!((command.options.cliff_render_strength - 1.75).abs() < f32::EPSILON);
    }

    #[test]
    fn rejects_unknown_options() {
        let error = parse([String::from("--wat")].into_iter()).unwrap_err();
        assert!(error.contains("unknown option"));
    }
}
