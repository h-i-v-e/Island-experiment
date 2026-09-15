use std::{env, fs, hint::black_box, path::PathBuf, process, time::Instant};

use motu::{Island, IslandOptions, Mesh};

const DEFAULT_SEEDS: [u64; 5] = [666, 2018, 42, 12_345, 0x5eed_cafe];

fn main() {
    if let Err(error) = run() {
        eprintln!("generation-bench: {error}");
        process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut terrain_size = 1024_u32;
    let mut repetitions = 3_usize;
    let mut seeds = Vec::new();
    let mut snapshot_directory = None::<PathBuf>;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--terrain-size" => {
                terrain_size = parse(&argument, arguments.next())?;
            }
            "--repetitions" => {
                repetitions = parse(&argument, arguments.next())?;
            }
            "--seed" => seeds.push(parse(&argument, arguments.next())?),
            "--snapshot-directory" => {
                snapshot_directory = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or("--snapshot-directory requires a path")?,
                ));
            }
            "-h" | "--help" => {
                println!(
                    "generation-bench [--terrain-size N] [--repetitions N] [--seed N]...\n\
                     [--snapshot-directory PATH]\n\
                     One warm-up precedes measured runs. Optional full snapshots are saved\n\
                     after timing each run; no raster or Unity assets are generated."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown option {argument:?}")),
        }
    }
    if seeds.is_empty() {
        seeds.extend(DEFAULT_SEEDS);
    }
    if repetitions == 0 {
        return Err("--repetitions must be greater than zero".into());
    }
    if let Some(directory) = &snapshot_directory {
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }

    let options = IslandOptions {
        terrain_size,
        ..IslandOptions::default()
    };
    let warmup_size = terrain_size.min(128);
    black_box(Island::generate(
        seeds[0],
        IslandOptions {
            terrain_size: warmup_size,
            ..options
        },
    )?);

    println!("seed,run,workers,milliseconds,vertices,triangles,rivers,geometry_hash");
    for seed in seeds {
        for run in 1..=repetitions {
            let started = Instant::now();
            let island = black_box(Island::generate(seed, options)?);
            let elapsed = started.elapsed();
            let mesh = island.terrain().mesh();
            println!(
                "{seed},{run},{},{:.3},{},{},{},{}",
                rayon::current_num_threads(),
                elapsed.as_secs_f64() * 1000.0,
                mesh.vertices.len(),
                mesh.triangles.len() / 3,
                island.rivers().len(),
                geometry_hash(mesh),
            );
            if let Some(directory) = &snapshot_directory {
                island
                    .save(directory.join(format!("{seed}-{run}.motusnapshot")))
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

fn parse<T>(name: &str, value: Option<String>) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .ok_or_else(|| format!("{name} requires a value"))?
        .parse()
        .map_err(|error| format!("invalid value for {name}: {error}"))
}

fn geometry_hash(mesh: &Mesh) -> u64 {
    mesh.vertices
        .iter()
        .flat_map(motu::Vec3::to_array)
        .map(f32::to_bits)
        .chain(mesh.triangles.iter().copied())
        .fold(0xcbf2_9ce4_8422_2325, |hash, value| {
            (hash ^ u64::from(value)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}
