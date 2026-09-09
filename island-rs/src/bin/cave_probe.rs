//! Reproducible cave corpus/performance runner; writes only an explicit output.
use motu::{
    FernOptions, ForestOptions, GenerationMethod, ISLAND_WORLD_METRES, Island, IslandOptions,
    ReedOptions,
    caves::{CaveOptions, CaveSet},
};
use std::{env, error::Error, path::PathBuf, time::Instant};

fn main() {
    if let Err(error) = run() {
        eprintln!("island-cave-probe: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut seed = 0_u64;
    let mut count = 1_u64;
    let mut size = 1024;
    let mut output = None::<PathBuf>;
    let mut snapshot = None::<PathBuf>;
    let mut cave_options = None::<PathBuf>;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!(
                "island-cave-probe [--seed N] [--count N] [--terrain-size N] [--output DIRECTORY] [--snapshot FILE] [--cave-options JSON]"
            );
            return Ok(());
        }
        let value = args.next().ok_or("option requires a value")?;
        match arg.as_str() {
            "--seed" => seed = value.parse()?,
            "--count" => count = value.parse()?,
            "--terrain-size" => size = value.parse()?,
            "--output" => output = Some(value.into()),
            "--snapshot" => snapshot = Some(value.into()),
            "--cave-options" => cave_options = Some(value.into()),
            _ => return Err(format!("unknown option {arg}").into()),
        }
    }
    if count == 0 || count > 100 {
        return Err("count must be 1..100".into());
    }
    if let Some(path) = &output {
        std::fs::create_dir_all(path)?;
    }
    if let Some(snapshot) = snapshot {
        return inspect_snapshot(&snapshot, cave_options.as_deref(), output.as_deref());
    }
    if cave_options.is_some() {
        return Err("--cave-options requires --snapshot".into());
    }
    for offset in 0..count {
        let seed = seed.checked_add(offset).ok_or("seed overflow")?;
        let started = Instant::now();
        let island = Island::generate_with_caves(
            seed,
            IslandOptions {
                terrain_size: size,
                ..IslandOptions::default()
            },
            ForestOptions::default(),
            ReedOptions::default(),
            FernOptions::default(),
            CaveOptions {
                enabled: 1,
                ..CaveOptions::default()
            },
            GenerationMethod::Cpu,
        )?;
        let generation_seconds = started.elapsed().as_secs_f64();
        let caves = island.caves();
        let triangles: usize = caves
            .caves
            .iter()
            .flat_map(|c| c.chunks.iter().chain(std::iter::once(&c.entrance_collider)))
            .map(|m| m.triangles.len() / 3)
            .sum();
        let chunks: usize = caves.caves.iter().map(|c| c.chunks.len() + 1).sum();
        println!(
            "{}",
            serde_json::json!({"seed":seed,"terrain_size":size,"generation_seconds":generation_seconds,
            "caves":caves.caves.len(),"chunks":chunks,"triangles":triangles,"stats":caves.stats})
        );
        if let Some(path) = &output {
            island.save(path.join(format!("{seed}.motusnapshot")))?;
            // Layout and diagnostic exports are convenient to inspect without Unity.
            std::fs::write(
                path.join(format!("{seed}.caves.json")),
                serde_json::to_vec(caves)?,
            )?;
        }
    }
    Ok(())
}

// Inspect placement against the exact cached final terrain, without changing the
// user's snapshot or rerunning erosion and vegetation generation.
fn inspect_snapshot(
    path: &std::path::Path,
    override_path: Option<&std::path::Path>,
    output: Option<&std::path::Path>,
) -> Result<(), Box<dyn Error>> {
    let island = Island::load(path)?;
    let options = override_path
        .map(|p| -> Result<CaveOptions, Box<dyn Error>> {
            Ok(serde_json::from_slice(&std::fs::read(p)?)?)
        })
        .transpose()?
        .unwrap_or(island.caves().options);
    println!(
        "{}",
        serde_json::json!({"seed":island.seed(),"cached":island.caves().stats,"options":options})
    );
    let started = Instant::now();
    let caves = CaveSet::generate(island.seed(), island.terrain(), options, |point| {
        island.rivers().iter().any(|river| {
            river.nodes.windows(2).any(|pair| {
                let start = pair[0].position.truncate();
                let edge = pair[1].position.truncate() - start;
                let blend = ((point - start).dot(edge) / edge.length_squared().max(1.0e-12))
                    .clamp(0.0, 1.0);
                point.distance(start + edge * blend)
                    < (options.chamber_width + 20.0) / ISLAND_WORLD_METRES
            })
        })
    })?;
    println!(
        "{}",
        serde_json::json!({"seed":island.seed(),"cave_seconds":started.elapsed().as_secs_f64(),"regenerated":caves.stats})
    );
    if let Some(output) = output {
        std::fs::write(
            output.join(format!("{}.caves.json", island.seed())),
            serde_json::to_vec(&caves)?,
        )?;
    }
    Ok(())
}
