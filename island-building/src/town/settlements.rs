use super::{
    TownCatalogueEntry, TownError, TownInput, TownOutput, TownRecipe, TownSettings, TownSettlement,
    TownSettlementKind, input,
    placement::{self, Random, Site, entry_seed},
    prototypes::PrototypePool,
    rivers, roads, wheat,
};
use crate::BuildingRecipe;
use motu::roads::{MAX_ROAD_EDGES, RoadNetwork};
use std::{borrow::Cow, path::Path};

struct Catalogue<'a> {
    entries: &'a [TownCatalogueEntry],
    settings: TownSettings,
    prototype_indices: Vec<usize>,
}

impl<'a> Catalogue<'a> {
    fn prepare(
        recipe: &'a TownRecipe,
        buildings: Vec<Cow<'_, BuildingRecipe>>,
        legacy_seed: Option<u64>,
        shoulder: Option<f32>,
        pool: &mut PrototypePool,
    ) -> Self {
        let prototype_indices = recipe
            .catalogue
            .iter()
            .zip(buildings)
            .map(|(entry, building)| {
                let mut building = building.into_owned();
                // Preserve the single-town variant sequence. Shared prototypes
                // use the authored appearance seed, independent of catalogue IDs.
                if let Some(seed) = legacy_seed {
                    building.seed = Random(entry_seed(seed, &entry.id) ^ building.seed).next();
                }
                pool.prepare(building, legacy_seed.is_none())
            })
            .collect();
        let mut settings = recipe.settings.clone();
        if let (Some(roads), Some(shoulder)) = (&mut settings.roads, shoulder) {
            roads.shoulder = shoulder;
        }
        Self {
            entries: &recipe.catalogue,
            settings,
            prototype_indices,
        }
    }
}

pub(super) fn generate(
    capital: &TownRecipe,
    village: Option<&TownRecipe>,
    input: &TownInput,
    base: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<TownOutput, TownError> {
    input::validate_terrain(input)?;
    let capital_buildings = input::validate_recipe(capital)?;
    let village_buildings = village.map(input::validate_recipe).transpose()?;
    if cancelled() {
        return Err(TownError::Cancelled);
    }
    let mut output = TownOutput {
        tile_seed: input.tile_seed,
        ..TownOutput::default()
    };
    let mouths = if village.is_some() {
        rivers::settlement_mouths(input)
    } else {
        rivers::longest_mouth(input).into_iter().collect()
    };
    let Some(capital_mouth) = mouths.first() else {
        return Ok(output);
    };
    output.mouth = Some(*capital_mouth);

    // The aggregate native road format carries a single shoulder width.
    let shoulder = capital
        .settings
        .roads
        .or_else(|| village.and_then(|recipe| recipe.settings.roads))
        .map(|roads| roads.shoulder);
    let mut pool = PrototypePool::default();
    let capital = Catalogue::prepare(
        capital,
        capital_buildings,
        village.is_none().then_some(input.tile_seed),
        shoulder,
        &mut pool,
    );
    let village = village.zip(village_buildings).map(|(recipe, buildings)| {
        Catalogue::prepare(recipe, buildings, None, shoulder, &mut pool)
    });
    let mut field_sites = Vec::with_capacity(mouths.len());
    for (index, mouth) in mouths.into_iter().enumerate() {
        if cancelled() {
            return Err(TownError::Cancelled);
        }
        let (kind, catalogue, seed) = if index == 0 {
            (TownSettlementKind::Capital, &capital, input.tile_seed)
        } else {
            let [x, y] = mouth.position.map(|point| u64::from(point.to_bits()));
            let seed = Random(input.tile_seed ^ (x << 32 | y)).next();
            (
                TownSettlementKind::Village,
                village.as_ref().expect("village catalogue"),
                seed,
            )
        };
        let site = Site {
            catalogue: catalogue.entries,
            settings: &catalogue.settings,
            input,
            mouth,
            seed,
            reservations: &output.clearance,
            base,
            cancelled,
            prototype_indices: &catalogue.prototype_indices,
        };
        let mut planned = placement::generate(&site, &mut pool)?;
        if cancelled() {
            return Err(TownError::Cancelled);
        }
        // Keep every outlet in the result even if the whole site won't fit in
        // the aggregate terrain edit. Keep connected sites together.
        if village.is_some() && !fits_terrain_limits(&output, &planned) {
            planned = TownOutput {
                skipped_count: planned.skipped_count
                    + u32::try_from(planned.instances.len()).expect("up to 256 copies per site"),
                ..TownOutput::default()
            };
        }
        output.settlements.push(TownSettlement {
            kind,
            river_index: mouth.river_index,
            mouth: mouth.position,
            centre: planned.centre,
            building_count: planned.instances.len(),
            skipped_count: planned.skipped_count,
        });
        if index == 0 {
            output.centre = planned.centre;
        }
        field_sites.push(wheat::Site {
            settings: &catalogue.settings,
            rows: planned
                .roads
                .as_ref()
                .map_or_else(Vec::new, roads::field_rows),
        });
        append_site(&mut output, planned, catalogue, index)?;
    }
    pool.finish(&mut output);
    output.wheat_fields = wheat::generate(&output, &field_sites, input, cancelled)?;
    Ok(output)
}

fn fits_terrain_limits(output: &TownOutput, site: &TownOutput) -> bool {
    let roads = || output.roads.iter().chain(&site.roads);
    output.clearance.len() + site.clearance.len() <= 4096
        && roads().map(|network| network.strips.len()).sum::<usize>() <= MAX_ROAD_EDGES
        && roads()
            .map(|network| network.junctions.len())
            .sum::<usize>()
            <= 8192
        && roads()
            .flat_map(|network| &network.strips)
            .map(|strip| strip.stations.len())
            .sum::<usize>()
            <= 65_536
}

fn append_site(
    output: &mut TownOutput,
    mut site: TownOutput,
    catalogue: &Catalogue<'_>,
    settlement_index: usize,
) -> Result<(), TownError> {
    for instance in &mut site.instances {
        let entry = instance.prototype_index;
        instance.prototype_index = catalogue.prototype_indices[entry];
        instance.settlement_index = settlement_index;
        instance.entry_id.clone_from(&catalogue.entries[entry].id);
    }
    for exit in &mut site.road_exits {
        exit.settlement_index = settlement_index;
    }
    let offset = placement::support_index_offset(&output.supports, &site.supports)?;
    output
        .supports
        .positions
        .append(&mut site.supports.positions);
    output.supports.normals.append(&mut site.supports.normals);
    output.supports.uvs.append(&mut site.supports.uvs);
    output.supports.indices.extend(
        site.supports
            .indices
            .into_iter()
            .map(|index| index + offset),
    );
    output
        .supports_grub_positions
        .append(&mut site.supports_grub_positions);
    output.support_colliders.append(&mut site.support_colliders);
    output.stair_colliders.append(&mut site.stair_colliders);
    output.stair_feet.append(&mut site.stair_feet);
    output.footpath_blockers.append(&mut site.footpath_blockers);
    output.clearance.append(&mut site.clearance);
    output.routing_blockers.append(&mut site.routing_blockers);
    output.instances.append(&mut site.instances);
    output.road_exits.append(&mut site.road_exits);
    output.bridge_candidates.append(&mut site.bridge_candidates);
    output.skipped_count += site.skipped_count;
    if let Some(roads) = site.roads {
        if let Some(existing) = &mut output.roads {
            append_roads(existing, roads);
        } else {
            output.roads = Some(roads);
        }
    }
    Ok(())
}

fn append_roads(target: &mut RoadNetwork, mut source: RoadNetwork) {
    let node_offset = target
        .strips
        .iter()
        .map(|strip| strip.start.max(strip.end))
        .max()
        .map_or(0, |node| node + 1);
    let source_offset = target
        .strips
        .iter()
        .map(|strip| strip.source)
        .max()
        .map_or(0, |source| source + 1);
    for strip in &mut source.strips {
        strip.start += node_offset;
        strip.end += node_offset;
        strip.source += source_offset;
    }
    for junction in &mut source.junctions {
        junction.node += node_offset;
    }
    target.strips.append(&mut source.strips);
    target.junctions.append(&mut source.junctions);
}
