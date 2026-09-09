//! Reproducible cave corpus/performance runner; writes only an explicit output.
use motu::{
    FernOptions, ForestOptions, GenerationMethod, ISLAND_WORLD_METRES, Island, IslandOptions,
    ReedOptions,
    caves::{CaveNetworkOptions, CaveOptions, CaveSet, CaveWalkOptions},
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
    let mut branches = 2;
    let mut walk = CaveWalkOptions {
        enabled: 1,
        ..CaveWalkOptions::default()
    };
    let mut output = None::<PathBuf>;
    let mut snapshot = None::<PathBuf>;
    let mut cave_options = None::<PathBuf>;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!(
                "island-cave-probe [--seed N] [--count N] [--terrain-size N] [--branches N] [--walk 0|1] [--end-probability P] [--branch-probability P] [--output DIRECTORY] [--snapshot FILE] [--cave-options JSON]"
            );
            return Ok(());
        }
        let value = args.next().ok_or("option requires a value")?;
        match arg.as_str() {
            "--seed" => seed = value.parse()?,
            "--count" => count = value.parse()?,
            "--terrain-size" => size = value.parse()?,
            "--branches" => branches = value.parse()?,
            "--walk" => walk.enabled = value.parse()?,
            "--end-probability" => walk.end_probability = value.parse()?,
            "--branch-probability" => walk.branch_probability = value.parse()?,
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
        eprintln!("cave-probe,seed,{seed}");
        let started = Instant::now();
        let island = Island::generate_with_cave_walks(
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
            CaveNetworkOptions {
                maximum_branches: branches,
                ..CaveNetworkOptions::default()
            },
            walk,
            GenerationMethod::Cpu,
        )?;
        let generation_seconds = started.elapsed().as_secs_f64();
        let caves = island.caves();
        let mut report = generation_report(&island, size, generation_seconds)?;
        if let Some(path) = &output {
            let snapshot_path = path.join(format!("{seed}.motusnapshot"));
            let save_started = Instant::now();
            island.save(&snapshot_path)?;
            report["snapshot_save_seconds"] = save_started.elapsed().as_secs_f64().into();
            report["snapshot_bytes"] = std::fs::metadata(&snapshot_path)?.len().into();
            // Layout and diagnostic exports are convenient to inspect without Unity.
            std::fs::write(
                path.join(format!("{seed}.caves.json")),
                serde_json::to_vec(caves)?,
            )?;
        }
        println!("{report}");
    }
    Ok(())
}

fn generation_report(
    island: &Island,
    size: u32,
    generation_seconds: f64,
) -> Result<serde_json::Value, bincode::Error> {
    let caves = island.caves();
    let seed = island.seed();
    let meshes = || {
        caves
            .caves
            .iter()
            .flat_map(|c| c.chunks.iter().chain(std::iter::once(&c.entrance_collider)))
    };
    let triangles: usize = meshes().map(|m| m.triangles.len() / 3).sum();
    let vertices: usize = meshes().map(|m| m.vertices.len()).sum();
    let mesh_buffer_bytes: usize = meshes()
        .map(|m| {
            std::mem::size_of_val(m.vertices.as_slice())
                + std::mem::size_of_val(m.normals.as_slice())
                + std::mem::size_of_val(m.uv.as_slice())
                + std::mem::size_of_val(m.triangles.as_slice())
        })
        .sum();
    let surface_samples: usize = caves.caves.iter().map(|c| c.surface.heights.len()).sum();
    let render_triangles: usize = caves
        .caves
        .iter()
        .flat_map(|c| &c.chunks)
        .map(|m| m.triangles.len() / 3)
        .sum();
    let chunks = meshes().count();
    Ok(serde_json::json!({
        "seed":seed, "terrain_size":size, "generation_seconds":generation_seconds,
        "cave_revision":motu::caves::CAVE_REVISION, "profiling":cfg!(feature = "profiling"),
        "caves":caves.caves.len(), "branches":caves.caves.iter().map(|c| c.branches.len()).sum::<usize>(),
        "network_options":caves.network_options, "walk_options":caves.walk_options, "chunks":chunks, "triangles":triangles,
        "render_triangles":render_triangles, "vertices":vertices,
        "mesh_buffer_bytes":mesh_buffer_bytes, "surface_samples":surface_samples,
        "surface_buffer_bytes":surface_samples * std::mem::size_of::<f32>(),
        "cave_serialized_bytes":bincode::serialized_size(caves)?, "stats":caves.stats
    }))
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
    let caves = CaveSet::generate_wandering(
        island.seed(),
        island.terrain(),
        options,
        island.caves().network_options,
        island.caves().walk_options,
        |point| {
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
        },
    )?;
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
