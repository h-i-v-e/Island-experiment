use super::CartTrackRequest;
use crate::settlement_names::SettlementNames;
use serde::{Deserialize, Serialize};

const DEFAULT_CORPUS: &str = include_str!("../../settlement_names/default_corpus.txt");
const SETTLEMENT_DOMAIN: u64 = 0x746f_776e_6e61_6d65;
const COAST_ROAD_DOMAIN: u64 = 0x636f_6173_7472_6f61;
const ID_HASH: u64 = 0x9e37_79b9_7f4a_7c15;

#[derive(Deserialize)]
pub(super) struct NameOptions {
    name_seed: Option<String>,
    name_corpus: Option<String>,
    #[serde(default)]
    section_count: u32,
}

#[derive(Serialize)]
struct NamedReport<T> {
    #[serde(flatten)]
    report: T,
    settlement_names: Vec<String>,
    portal_names: [String; 2],
}

pub(super) fn serialize_report(
    report: impl Serialize,
    request: &CartTrackRequest,
    island_seed: u64,
) -> Result<String, String> {
    let model = SettlementNames::from_corpus(
        request
            .sign_names
            .name_corpus
            .as_deref()
            .filter(|corpus| !corpus.trim().is_empty())
            .unwrap_or(DEFAULT_CORPUS),
    );
    let tile_seed = request
        .sign_names
        .name_seed
        .as_deref()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(island_seed);
    let settlement_names = model.names((0..request.settlement_count).map(|index| {
        let river = request
            .settlements
            .as_ref()
            .and_then(|sites| sites.get(index))
            .and_then(|site| site.river_index);
        let id = river.map_or(index as u64 | (1 << 32), u64::from);
        tile_seed.rotate_left(33) ^ SETTLEMENT_DOMAIN ^ id.wrapping_mul(ID_HASH)
    }));
    let portal_names = if request.coast_seams {
        coast_road_seeds(tile_seed, request.sign_names.section_count).map(|seed| {
            let name = model
                .names([seed])
                .pop()
                .expect("one seed produces one name");
            format!("{name} Road")
        })
    } else {
        [String::new(), String::new()]
    };
    serde_json::to_string(&NamedReport {
        report,
        settlement_names,
        portal_names,
    })
    .map_err(|err| err.to_string())
}

fn coast_road_seeds(tile_seed: u64, section_count: u32) -> [u64; 2] {
    let seed = tile_seed >> 32;
    let section = tile_seed & u64::from(u32::MAX);
    // Each seam belongs to the section on its east side, including the wrap.
    let next = (section + 1)
        .checked_rem(u64::from(section_count))
        .unwrap_or(section + 1);
    [section, next]
        .map(|boundary| seed.rotate_left(31) ^ COAST_ROAD_DOMAIN ^ boundary.wrapping_mul(ID_HASH))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn actual_town_names_follow_the_river_ids_and_seams_share_a_road_name() {
        let seed = 42_u64 << 32;
        let prepare = |section: u64, rivers: [u32; 2]| {
            let args = json!({
                "exits": [], "blockers": [], "coast_seams": true,
                "settlement_count": 2, "section_count": 4,
                "name_seed": (seed | section).to_string(),
                "settlements": rivers.map(|river| json!({"kind": "village", "river_index": river})),
            });
            let request: CartTrackRequest = serde_json::from_value(args).unwrap();
            let output = serialize_report(json!({"run_count": 0}), &request, 0).unwrap();
            serde_json::from_str::<Value>(&output).unwrap()
        };
        let first = prepare(0, [23, 81]);
        let reordered = prepare(0, [81, 23]);
        assert_eq!(
            first["settlement_names"][0],
            reordered["settlement_names"][1]
        );
        assert_eq!(
            first["settlement_names"][1],
            reordered["settlement_names"][0]
        );
        assert_ne!(first["settlement_names"][0], first["settlement_names"][1]);
        assert_eq!(
            first["portal_names"][1],
            prepare(1, [32, 16])["portal_names"][0]
        );
        assert_eq!(
            first["portal_names"][0],
            prepare(3, [16, 32])["portal_names"][1]
        );
        assert_eq!(first["run_count"], 0);
    }
}
