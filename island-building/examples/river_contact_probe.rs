//! Offline geometric diagnostics for two close, parallel river reaches.
use motu::{ISLAND_WORLD_METRES, Island, Vec2};
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy)]
struct Reach {
    river: usize,
    a: Vec2,
    b: Vec2,
    surface: [f32; 2],
    width: [f32; 2],
    remaining: f32,
}

#[derive(Serialize)]
struct Contact {
    rivers: [usize; 2],
    samples: usize,
    minimum_separation_metres: f32,
    maximum_level_difference_metres: f32,
    example_position_metres: [f32; 2],
}

fn reaches(island: &Island) -> Vec<Reach> {
    let options = island.options();
    island
        .rivers()
        .iter()
        .enumerate()
        .flat_map(|(river, line)| {
            let widths = line.target_half_widths(
                options.river_source_width_metres,
                options.river_maximum_width_metres,
            );
            let mut remaining = 0.;
            let mut output = Vec::with_capacity(line.nodes.len().saturating_sub(1));
            for (index, pair) in line.nodes.windows(2).enumerate().rev() {
                let [a, b] =
                    [pair[0], pair[1]].map(|node| node.position.truncate() * ISLAND_WORLD_METRES);
                remaining += a.distance(b);
                output.push(Reach {
                    river,
                    a,
                    b,
                    surface: [pair[0], pair[1]].map(|node| node.surface * ISLAND_WORLD_METRES),
                    width: [widths[index], widths[index + 1]],
                    remaining,
                });
            }
            output
        })
        .collect()
}

fn projected(point: Vec2, reach: Reach) -> (f32, Vec2) {
    let delta = reach.b - reach.a;
    let along = ((point - reach.a).dot(delta) / delta.length_squared().max(1e-8)).clamp(0., 1.);
    (along, reach.a + delta * along)
}

fn closest(left: Reach, right: Reach) -> (f32, f32, f32, Vec2) {
    let candidates = [
        {
            let (t, p) = projected(left.a, right);
            (left.a.distance(p), 0., t, (left.a + p) * 0.5)
        },
        {
            let (t, p) = projected(left.b, right);
            (left.b.distance(p), 1., t, (left.b + p) * 0.5)
        },
        {
            let (t, p) = projected(right.a, left);
            (right.a.distance(p), t, 0., (right.a + p) * 0.5)
        },
        {
            let (t, p) = projected(right.b, left);
            (right.b.distance(p), t, 1., (right.b + p) * 0.5)
        },
    ];
    candidates
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("four endpoints")
}

fn analyse(island: &Island) -> Vec<Contact> {
    let segments = reaches(island);
    let mut contacts = BTreeMap::<(usize, usize), Contact>::new();
    for (index, &left) in segments.iter().enumerate() {
        for &right in &segments[index + 1..] {
            if left.river == right.river || left.remaining < 12. || right.remaining < 12. {
                continue;
            }
            let alignment = (left.b - left.a)
                .normalize_or_zero()
                .dot((right.b - right.a).normalize_or_zero())
                .abs();
            if alignment < 0.85 {
                continue;
            }
            let (separation, left_t, right_t, position) = closest(left, right);
            let interpolate = |values: [f32; 2], t: f32| values[0] + (values[1] - values[0]) * t;
            let width = interpolate(left.width, left_t) + interpolate(right.width, right_t);
            let level_a = interpolate(left.surface, left_t);
            let level_b = interpolate(right.surface, right_t);
            let difference = (level_a - level_b).abs();
            if separation > width + 1. || difference < 0.25 || level_a < 0. || level_b < 0. {
                continue;
            }
            let key = (left.river.min(right.river), left.river.max(right.river));
            let contact = contacts.entry(key).or_insert(Contact {
                rivers: [key.0, key.1],
                samples: 0,
                minimum_separation_metres: separation,
                maximum_level_difference_metres: difference,
                example_position_metres: position.to_array(),
            });
            contact.samples += 1;
            if difference > contact.maximum_level_difference_metres {
                contact.maximum_level_difference_metres = difference;
                contact.example_position_metres = position.to_array();
            }
            contact.minimum_separation_metres = contact.minimum_separation_metres.min(separation);
        }
    }
    contacts.into_values().collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(
        args.next()
            .ok_or("usage: river_contact_probe SNAPSHOT OUTPUT.json")?,
    );
    let output = PathBuf::from(args.next().ok_or("output JSON is required")?);
    let island = Island::load(&input)?;
    let contacts = analyse(&island);
    let result = serde_json::json!({"snapshot":input,"river_count":island.rivers().len(),"near_parallel_pairs":contacts,
        "note":"Diagnostic candidates only: parallel reaches within combined half widths plus 1m, water levels >=0.25m apart, excluding last12m of either branch. Natural waterfall transitions require manual review."});
    std::fs::write(output, serde_json::to_vec_pretty(&result)?)?;
    println!(
        "{} rivers; {} near-parallel candidate pairs",
        island.rivers().len(),
        result["near_parallel_pairs"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}
