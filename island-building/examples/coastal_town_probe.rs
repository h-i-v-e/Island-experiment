//! Generate the real seed-42 `CoastlinePlayground` terrain and inspect native towns.
//!
//! ```sh
//! cargo run --manifest-path island-building/Cargo.toml --example coastal_town_probe -- /tmp/coastal-town-probe
//! ```
//! Optional flags: --seed N --section N --water-ratio F --terrain-size N --load SNAPSHOT --save-snapshot
#![allow(clippy::cast_precision_loss)] // Physical grid coordinates use bounded integer indices.
use island_building::town::{
    HeightGrid, RiverPolyline, TownInput, TownOutput, TownRecipe, generate_town,
};
use motu::{CoastalStripOptions, ISLAND_WORLD_METRES, Island, IslandOptions, Vec2};
use serde::Serialize;
use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct Arguments {
    output: PathBuf,
    seed: u64,
    section: u32,
    water_ratio: f32,
    terrain_size: u32,
    load: Option<PathBuf>,
    input: Option<PathBuf>,
    recipe: Option<PathBuf>,
    save_snapshot: bool,
}
impl Arguments {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);
        let output=args.next().ok_or("usage: coastal_town_probe OUTPUT_DIR [--seed N] [--section N] [--water-ratio F] [--terrain-size N] [--load SNAPSHOT] [--save-snapshot]")?.into();
        let mut result = Self {
            output,
            seed: 42,
            section: 0,
            water_ratio: 0.8,
            terrain_size: 1024,
            load: None,
            input: None,
            recipe: None,
            save_snapshot: false,
        };
        while let Some(flag) = args.next() {
            if flag == "--save-snapshot" {
                result.save_snapshot = true;
                continue;
            }
            let value = args.next().ok_or("missing flag value")?;
            match flag.as_str() {
                "--seed" => result.seed = value.parse()?,
                "--section" => result.section = value.parse()?,
                "--water-ratio" => result.water_ratio = value.parse()?,
                "--terrain-size" => result.terrain_size = value.parse()?,
                "--load" => result.load = Some(value.into()),
                "--input" => result.input = Some(value.into()),
                "--recipe" => result.recipe = Some(value.into()),
                _ => return Err(format!("unknown flag: {flag}").into()),
            }
        }
        Ok(result)
    }
    fn town_recipe(&self) -> Result<TownRecipe> {
        if let Some(path) = &self.recipe {
            Ok(serde_json::from_reader(File::open(path)?)?)
        } else {
            Ok(serde_json::from_str(include_str!(
                "../recipes/towns/coastal.json"
            ))?)
        }
    }
    fn options(&self) -> IslandOptions {
        IslandOptions {
            max_height: 400. / ISLAND_WORLD_METRES,
            water_ratio: self.water_ratio,
            terrain_size: self.terrain_size,
            river_source_catchment_hectares: 0.1,
            river_source_steep_catchment_multiplier: 8.,
            river_source_elevation_boost: 20.,
            river_source_width_metres: 2.,
            river_maximum_width_metres: 5.,
            river_source_depth_metres: 0.35,
            river_maximum_depth_metres: 1.,
            ..IslandOptions::default()
        }
    }
}
#[derive(Serialize)]
struct PrototypeSummary<'a> {
    entry_id: &'a str,
    instances: usize,
    seed: u64,
}
#[derive(Serialize)]
struct Summary<'a> {
    source: &'a str,
    seed: u64,
    section: u32,
    options: IslandOptions,
    strip_options: CoastalStripOptions,
    terrain_vertices: usize,
    river_lines: usize,
    river_points: usize,
    terrain_seconds: f64,
    sampling_seconds: f64,
    town_seconds: f64,
    mouth: Option<island_building::town::TownMouth>,
    centre: Option<[f32; 2]>,
    buildings: usize,
    skipped: u32,
    road_sections: usize,
    bridge_candidates: usize,
    prototypes: Vec<PrototypeSummary<'a>>,
    support_vertices: usize,
    support_triangles: usize,
    clearance_rectangles: usize,
    clearance: &'a [island_building::town::ClearanceRect],
}
fn main() -> Result<()> {
    let args = Arguments::parse()?;
    fs::create_dir_all(&args.output)?;
    if let Some(path) = &args.input {
        return rerun_input(&args, path);
    }
    let strip = CoastalStripOptions {
        section_index: args.section,
        ..CoastalStripOptions::default()
    };
    let options = args.options();
    println!(
        "Real coastal probe: seed={} section={} water_ratio={} terrain_size={} catchment=0.1ha steep=8 widths=2..5m",
        args.seed, args.section, args.water_ratio, args.terrain_size
    );
    let started = Instant::now();
    let island = if let Some(path) = &args.load {
        Island::load(path)?
    } else {
        Island::generate_coastal_strip(args.seed, options, strip)?
    };
    let terrain_seconds = started.elapsed().as_secs_f64();
    println!(
        "Terrain ready in {terrain_seconds:.3}s: {} vertices, {} river lines, {} river points",
        island.terrain().vertex_count(),
        island.rivers().len(),
        island.rivers().iter().map(|r| r.nodes.len()).sum::<usize>()
    );
    if args.save_snapshot {
        island.save(args.output.join("coastal.snapshot"))?;
    }
    let started = Instant::now();
    let input = town_input(
        &island,
        ((args.seed & u64::from(u32::MAX)) << 32) | u64::from(args.section),
    );
    let sampling_seconds = started.elapsed().as_secs_f64();
    write_json(&args.output.join("town-input.json"), &input)?;
    let recipe = args.town_recipe()?;
    let started = Instant::now();
    let output = match generate_town(&recipe, &input) {
        Ok(output) => output,
        Err(error) => {
            fs::write(args.output.join("generation-error.txt"), error.to_string())?;
            return Err(error.into());
        }
    };
    let town_seconds = started.elapsed().as_secs_f64();
    let summary = summary(
        &args,
        &island,
        &input,
        &output,
        strip,
        [terrain_seconds, sampling_seconds, town_seconds],
    );
    write_json(&args.output.join("summary.json"), &summary)?;
    write_json(&args.output.join("town.json"), &output)?;
    write_svg(&args.output.join("plan.svg"), &input, &output)?;
    println!(
        "Town ready in {town_seconds:.3}s (grid {sampling_seconds:.3}s): {} buildings, {} skipped, {} prototypes; mouth={:?}",
        output.instances.len(),
        output.skipped_count,
        output.prototypes.len(),
        output.mouth
    );
    println!(
        "Support mesh: {} vertices / {} triangles; {} clearances; outputs {}",
        output.supports.positions.len(),
        output.supports.triangle_count(),
        output.clearance.len(),
        args.output.display()
    );
    Ok(())
}
fn rerun_input(args: &Arguments, path: &Path) -> Result<()> {
    let input: TownInput = serde_json::from_reader(File::open(path)?)?;
    let recipe = args.town_recipe()?;
    let started = Instant::now();
    let output = generate_town(&recipe, &input)?;
    let elapsed = started.elapsed().as_secs_f64();
    let counts: Vec<_> = output
        .prototypes
        .iter()
        .enumerate()
        .map(|(index, p)| PrototypeSummary {
            entry_id: &p.entry_id,
            seed: p.building.recipe.seed,
            instances: output
                .instances
                .iter()
                .filter(|i| i.prototype_index == index)
                .count(),
        })
        .collect();
    let summary = serde_json::json!({
        "source_input": path,
        "tile_seed": input.tile_seed,
        "town_seconds": elapsed,
        "river_lines": input.rivers.len(),
        "river_points": input.rivers.iter().map(|r| r.points.len()).sum::<usize>(),
        "mouth": output.mouth,
        "centre": output.centre,
        "buildings": output.instances.len(),
        "skipped": output.skipped_count,
        "road_sections": road_sections(&output),
        "bridge_candidates": output.bridge_candidates.len(),
        "prototypes": counts,
        "support_vertices": output.supports.positions.len(),
        "support_triangles": output.supports.triangle_count(),
        "clearance_rectangles": output.clearance.len(),
    });
    write_json(&args.output.join("summary.json"), &summary)?;
    write_json(&args.output.join("town.json"), &output)?;
    write_svg(&args.output.join("plan.svg"), &input, &output)?;
    println!("{}", serde_json::to_string(&summary)?);
    Ok(())
}

fn town_input(island: &Island, tile_seed: u64) -> TownInput {
    const DIM: usize = 1001;
    const SPACING: f32 = 2.;
    let mut heights = vec![0.; DIM * DIM];
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let rows_per_chunk = DIM.div_ceil(threads);
    std::thread::scope(|scope| {
        for (chunk, rows) in heights.chunks_mut(rows_per_chunk * DIM).enumerate() {
            scope.spawn(move || {
                for (row, values) in rows.chunks_mut(DIM).enumerate() {
                    let y = chunk * rows_per_chunk + row;
                    for (x, value) in values.iter_mut().enumerate() {
                        let point = Vec2::new(x as f32, y as f32) * SPACING / ISLAND_WORLD_METRES;
                        *value = island.terrain().sample_accurate_surface(point).map_or_else(
                            || island.terrain().sample(point.x, point.y),
                            |hit| hit.position.z,
                        ) * ISLAND_WORLD_METRES;
                    }
                }
            });
        }
    });
    let options = island.options();
    let rivers = island
        .rivers()
        .iter()
        .map(|river| RiverPolyline {
            join: river.join,
            points: river
                .nodes
                .iter()
                .map(|node| (node.position.truncate() * ISLAND_WORLD_METRES).to_array())
                .collect(),
            widths: river
                .target_half_widths(
                    options.river_source_width_metres,
                    options.river_maximum_width_metres,
                )
                .into_iter()
                .map(|width| width * 2.)
                .collect(),
        })
        .collect();
    TownInput {
        tile_seed,
        sea_level: 0.,
        protected_terrain: island.town_protected_terrain(),
        terrain: HeightGrid {
            origin: [0.; 2],
            spacing: SPACING,
            width: DIM,
            height: DIM,
            heights,
        },
        rivers,
    }
}
fn summary<'a>(
    args: &'a Arguments,
    island: &Island,
    input: &TownInput,
    town: &'a TownOutput,
    strip_options: CoastalStripOptions,
    times: [f64; 3],
) -> Summary<'a> {
    Summary {
        source: if args.load.is_some() {
            "loaded snapshot (options shown are actual snapshot options)"
        } else {
            "generated CoastlinePlayground terrain; default vegetation"
        },
        seed: island.seed(),
        section: args.section,
        options: island.options(),
        strip_options,
        terrain_vertices: island.terrain().vertex_count(),
        river_lines: input.rivers.len(),
        river_points: input.rivers.iter().map(|r| r.points.len()).sum(),
        terrain_seconds: times[0],
        sampling_seconds: times[1],
        town_seconds: times[2],
        mouth: town.mouth,
        centre: town.centre,
        buildings: town.instances.len(),
        skipped: town.skipped_count,
        road_sections: road_sections(town),
        bridge_candidates: town.bridge_candidates.len(),
        prototypes: town
            .prototypes
            .iter()
            .enumerate()
            .map(|(index, p)| PrototypeSummary {
                entry_id: &p.entry_id,
                instances: town
                    .instances
                    .iter()
                    .filter(|i| i.prototype_index == index)
                    .count(),
                seed: p.building.recipe.seed,
            })
            .collect(),
        support_vertices: town.supports.positions.len(),
        support_triangles: town.supports.triangle_count(),
        clearance_rectangles: town.clearance.len(),
        clearance: &town.clearance,
    }
}
fn road_sections(town: &TownOutput) -> usize {
    town.roads.as_ref().map_or(0, |roads| roads.strips.len())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    serde_json::to_writer(&mut writer, value)?;
    writer.flush()?;
    Ok(())
}
fn write_svg(path: &Path, input: &TownInput, town: &TownOutput) -> Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(
        writer,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" viewBox="0 0 2000 2000"><rect width="2000" height="2000" fill="#adc9d0"/><g transform="translate(0 2000) scale(1 -1)">"##
    )?;
    for y in (0..1000).step_by(10) {
        for x in (0..1000).step_by(10) {
            let h = input.terrain.heights[(y + 5) * 1001 + x + 5];
            if h > 0. {
                let light = (68. - h * 0.09).clamp(28., 68.);
                writeln!(
                    writer,
                    r#"<rect x="{}" y="{}" width="20" height="20" fill="hsl(72 18% {light}%)"/>"#,
                    x * 2,
                    y * 2
                )?;
            }
        }
    }
    for river in &input.rivers {
        write!(
            writer,
            r##"<polyline fill="none" stroke="#306aab" stroke-width="3" points=""##
        )?;
        for p in &river.points {
            write!(writer, "{},{} ", p[0], p[1])?;
        }
        writeln!(writer, r#""/>"#)?;
    }
    for rect in &town.clearance {
        writeln!(
            writer,
            r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#e9c391" fill-opacity="0.18" stroke="#493723" stroke-width="0.5" transform="translate({} {}) rotate({})"/>"##,
            -rect.half_extents[0],
            -rect.half_extents[1],
            rect.half_extents[0] * 2.,
            rect.half_extents[1] * 2.,
            rect.centre[0],
            rect.centre[1],
            rect.yaw.to_degrees()
        )?;
    }
    if let Some(roads) = &town.roads {
        for strip in &roads.strips {
            write!(
                writer,
                r##"<polyline fill="none" stroke="#68513e" stroke-width="{}" stroke-linecap="round" points=""##,
                strip.half_width * 2.
            )?;
            for station in &strip.stations {
                write!(writer, "{},{} ", station.point.x, station.point.y)?;
            }
            writeln!(writer, r#""/>"#)?;
        }
    }
    for bridge in &town.bridge_candidates {
        writeln!(
            writer,
            r##"<path d="M {} {} L {} {}" stroke="#8d3dad" stroke-width="2.5" stroke-dasharray="4 3"/>"##,
            bridge.from[0], bridge.from[1], bridge.to[0], bridge.to[1]
        )?;
    }
    if let Some(mouth) = town.mouth {
        writeln!(
            writer,
            r##"<circle cx="{}" cy="{}" r="8" fill="#c12c34"/>"##,
            mouth.position[0], mouth.position[1]
        )?;
    }
    writeln!(writer, "</g></svg>")?;
    Ok(())
}
