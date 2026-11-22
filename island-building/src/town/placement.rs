use super::prototypes::{PrototypePool, PrototypeSlot};
use super::rivers::distance;
use super::roads::{self, Access, Lot, Streets};
use super::{
    ClearanceRect, TownCatalogueEntry, TownError, TownInput, TownInstance, TownMouth, TownOutput,
    TownPlacement, TownSettings,
};
use crate::{
    BuildingMesh,
    collision::BoxCollider,
    terrain_support::{self, PlacementPose},
};
use motu::{Vec2, piers::PierLayout, roads::RoadNetwork};
use std::path::Path;

pub(super) struct Random(pub u64);
impl Random {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
    // Exactly the high 24 random bits; every possible value fits the f32 mantissa.
    #[allow(clippy::cast_precision_loss)]
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16_777_216.
    }
}
pub(super) fn entry_seed(seed: u64, id: &str) -> u64 {
    id.bytes().fold(seed ^ 0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}

struct Search<'a> {
    settings: &'a TownSettings,
    input: &'a TownInput,
    mouth: TownMouth,
    cancelled: &'a dyn Fn() -> bool,
    reservations: &'a [ClearanceRect],
    occupied: Vec<ClearanceRect>,
    access: Vec<ClearanceRect>,
    rows_by_ring: Vec<Vec<roads::Row>>,
    curb_exits: roads::CurbExits,
    accepted_lots: Vec<(usize, Lot)>,
    total_attempts: u32,
    remaining_samples: usize,
    layout: LayoutTrial<'a>,
    reached_rings: usize,
}

#[derive(Clone, Copy, Default)]
struct LayoutTrial<'a> {
    rings: usize,
    anchor: Option<(usize, PlacementPose)>,
    previous_lots: &'a [(usize, Lot)],
    piers: &'a [PierLayout],
}

struct LayoutResult {
    output: TownOutput,
    reached: usize,
    lots: Vec<(usize, Lot)>,
}

struct Request<'a> {
    entry: &'a TownCatalogueEntry,
    index: usize,
    remaining: u32,
    random: Random,
}

impl<'a> Request<'a> {
    fn new(entry: &'a TownCatalogueEntry, index: usize, tile_seed: u64) -> Self {
        let seed = entry_seed(tile_seed, &entry.id);
        let mut random = Random(seed);
        let offset =
            u32::try_from(random.next() % u64::from(entry.max_count - entry.min_count + 1))
                .expect("validated maximum count fits u32");
        Self {
            entry,
            index,
            remaining: entry.min_count + offset,
            random,
        }
    }
}

#[derive(Clone, Copy)]
enum PlacementStage<'a> {
    Site,
    Previous,
    Ring(&'a [roads::Row]),
}

struct EntryGeneration<'a> {
    base: &'a Path,
    id: &'a str,
    prototype: &'a mut PrototypeSlot,
}
impl EntryGeneration<'_> {
    fn resolved_recipe(&mut self) -> Result<&crate::BuildingRecipe, TownError> {
        self.prototype.resolved_recipe(self.base, self.id)
    }
}

struct Placed {
    pose: PlacementPose,
    supports: terrain_support::FittedSupports,
    clearance: ClearanceRect,
    occupied: Vec<ClearanceRect>,
    access: Option<Access>,
    streets: Option<Streets>,
    lot: Option<Lot>,
}

pub(super) struct Site<'a> {
    pub catalogue: &'a [TownCatalogueEntry],
    pub settings: &'a TownSettings,
    pub input: &'a TownInput,
    pub mouth: TownMouth,
    pub seed: u64,
    pub reservations: &'a [ClearanceRect],
    pub base: &'a Path,
    pub cancelled: &'a dyn Fn() -> bool,
    pub prototype_indices: &'a [usize],
    pub piers: &'a [PierLayout],
}

pub(super) fn generate(site: &Site<'_>, pool: &mut PrototypePool) -> Result<TownOutput, TownError> {
    let mut trial = LayoutTrial {
        rings: 1,
        piers: site.piers,
        ..LayoutTrial::default()
    };
    let mut best = generate_layout(site, trial, pool)?;
    if let Some(instance) = best.output.instances.first() {
        trial.anchor = Some((
            instance.prototype_index,
            PlacementPose {
                origin: instance.position,
                yaw: instance.yaw,
            },
        ));
    }
    let mut reached = best.reached;
    // Grading may move junctions or invalidate a frontage. Fit previous lots
    // against the expanded roads before looking for lots for the remaining copies.
    while best.output.skipped_count > 0 && reached == trial.rings {
        trial.rings += 1;
        let expanded = generate_layout(
            site,
            LayoutTrial {
                previous_lots: &best.lots,
                ..trial
            },
            pool,
        )?;
        if expanded.reached < trial.rings {
            break;
        }
        reached = expanded.reached;
        if (
            expanded.output.pier_layout.is_some(),
            expanded.output.instances.len(),
        ) > (
            best.output.pier_layout.is_some(),
            best.output.instances.len(),
        ) {
            best = expanded;
        }
    }
    let mut output = best.output;
    output.routing_blockers.extend_from_slice(&output.clearance);
    if let Some(roads) = &output.roads {
        output.clearance.extend(roads::clearance(roads, 0.25));
    }
    Ok(output)
}

fn generate_layout(
    site: &Site<'_>,
    layout: LayoutTrial<'_>,
    pool: &mut PrototypePool,
) -> Result<LayoutResult, TownError> {
    let mut output = TownOutput {
        tile_seed: site.input.tile_seed,
        mouth: Some(site.mouth),
        ..TownOutput::default()
    };
    let mut search = Search {
        settings: site.settings,
        input: site.input,
        mouth: site.mouth,
        cancelled: site.cancelled,
        reservations: site.reservations,
        occupied: Vec::new(),
        access: Vec::new(),
        rows_by_ring: Vec::new(),
        curb_exits: roads::CurbExits::default(),
        accepted_lots: Vec::new(),
        total_attempts: 0,
        remaining_samples: 8_000_000,
        layout,
        reached_rings: 0,
    };
    let mut requests: Vec<_> = site
        .catalogue
        .iter()
        .enumerate()
        .map(|(index, entry)| Request::new(entry, index, site.seed))
        .collect();
    let roads_requested = site.settings.roads.is_some();
    requests.sort_by_cached_key(|request| {
        let rank = if roads_requested {
            let bounds = shell_footprint(&request.entry.building);
            let area = (bounds.max[0] - bounds.min[0]) * (bounds.max[1] - bounds.min[1]);
            u32::MAX - area.to_bits()
        } else {
            u32::from(request.entry.placement == TownPlacement::Other)
        };
        (
            layout
                .anchor
                .is_none_or(|(anchor, _)| request.index != anchor),
            rank,
        )
    });
    for request in &mut requests {
        let prototype = pool.get_mut(site.prototype_indices[request.index]);
        generate_entry(
            request,
            prototype,
            &mut search,
            &mut output,
            site.base,
            PlacementStage::Site,
        )?;
        if output.roads.is_some() || layout.anchor.is_some() {
            break;
        }
    }
    if search.reached_rings >= layout.rings {
        for request in &mut requests {
            let prototype = pool.get_mut(site.prototype_indices[request.index]);
            generate_entry(
                request,
                prototype,
                &mut search,
                &mut output,
                site.base,
                PlacementStage::Previous,
            )?;
        }
        for rows in std::mem::take(&mut search.rows_by_ring) {
            for request in &mut requests {
                let prototype = pool.get_mut(site.prototype_indices[request.index]);
                generate_entry(
                    request,
                    prototype,
                    &mut search,
                    &mut output,
                    site.base,
                    PlacementStage::Ring(&rows),
                )?;
            }
        }
    }
    if let (Some(roads), Some(centre)) = (&output.roads, output.centre) {
        output.road_exits.extend(roads::clear_curb_exits(
            search.curb_exits,
            centre,
            roads,
            &output.clearance,
            site.reservations,
            site.input,
            site.settings,
        ));
    }
    output.skipped_count = requests.iter().map(|request| request.remaining).sum();
    Ok(LayoutResult {
        output,
        reached: search.reached_rings,
        lots: search.accepted_lots,
    })
}

fn generate_entry(
    request: &mut Request<'_>,
    prototype: &mut PrototypeSlot,
    search: &mut Search<'_>,
    output: &mut TownOutput,
    base: &Path,
    stage: PlacementStage<'_>,
) -> Result<(), TownError> {
    if (search.cancelled)() {
        return Err(TownError::Cancelled);
    }
    if request.remaining == 0
        || matches!(stage, PlacementStage::Previous) && search.layout.previous_lots.is_empty()
    {
        return Ok(());
    }
    let entry = request.entry;
    let roads_requested = search.settings.roads.is_some();
    let footprint = if roads_requested {
        shell_footprint(&prototype.recipe)
    } else {
        footprint(&prototype.recipe, search.settings)
    };
    let mut generation = EntryGeneration {
        base,
        id: &entry.id,
        prototype,
    };
    match stage {
        PlacementStage::Site => {
            let (copies, priority) = if roads_requested {
                (1, TownPlacement::Centre)
            } else {
                (request.remaining, entry.placement)
            };
            for _ in 0..copies {
                if let Some(placed) = find_placement(
                    search,
                    output,
                    &mut generation,
                    footprint,
                    priority,
                    &mut request.random,
                )? {
                    accept_placement(search, output, placed, request.index)?;
                    request.remaining -= 1;
                }
            }
        }
        PlacementStage::Previous => {
            for &(_, lot) in search
                .layout
                .previous_lots
                .iter()
                .filter(|(index, _)| *index == request.index)
            {
                if request.remaining == 0
                    || search.remaining_samples == 0
                    || search.total_attempts >= search.settings.max_total_attempts
                {
                    break;
                }
                if (search.cancelled)() {
                    return Err(TownError::Cancelled);
                }
                let network = output
                    .roads
                    .as_ref()
                    .expect("previous lots had connected roads");
                search.total_attempts += 1;
                if let Some(placed) =
                    fit_frontage(search, &mut generation, footprint, network, lot)?
                {
                    accept_placement(search, output, placed, request.index)?;
                    request.remaining -= 1;
                }
            }
        }
        PlacementStage::Ring(rows) => {
            let building = generation.resolved_recipe()?;
            let mut lots = roads::entrance(building).map_or_else(roads::Lots::default, |door| {
                roads::lots(rows, building, door, footprint, search.settings)
            });
            for _ in 0..request.remaining {
                let network = output
                    .roads
                    .as_ref()
                    .expect("street ring has connected roads");
                lots.start_at(request.random.next());
                if let Some(placed) =
                    find_frontage(search, &mut generation, footprint, network, &mut lots)?
                {
                    accept_placement(search, output, placed, request.index)?;
                    request.remaining -= 1;
                }
            }
        }
    }
    Ok(())
}

fn accept_placement(
    search: &mut Search<'_>,
    output: &mut TownOutput,
    mut placed: Placed,
    index: usize,
) -> Result<(), TownError> {
    output
        .centre
        .get_or_insert([placed.pose.origin[0], placed.pose.origin[1]]);
    search.occupied.append(&mut placed.occupied);
    if let Some(access) = &placed.access {
        search.access.push(access.clearance);
    }
    if let Some(lot) = placed.lot {
        search.accepted_lots.push((index, lot));
    }
    if let Some(streets) = placed.streets.take() {
        if let Some(pier) = streets.pier {
            let land = pier.land_clearance();
            let footprint = ClearanceRect {
                centre: land.centre,
                half_extents: land.half_extents,
                yaw: land.yaw,
            };
            search.occupied.push(footprint);
            output.clearance.push(footprint);
            output.pier_layout = Some(pier);
        }
        for bridge in &streets.bridges {
            let footprint = roads::bridge_clearance(bridge, streets.network.shoulder);
            search.occupied.push(footprint);
            output.clearance.push(footprint);
        }
        search.rows_by_ring = streets.rows;
        search.curb_exits = streets.curb_exits;
        search.reached_rings = streets.ring_count;
        output.roads = Some(streets.network);
        output.road_exits = streets.exits;
        output.bridge_candidates = streets.bridges;
    }
    publish_placement(output, placed, index)
}

fn find_placement(
    search: &mut Search<'_>,
    output: &TownOutput,
    generation: &mut EntryGeneration<'_>,
    footprint: BoxCollider,
    placement: TownPlacement,
    random: &mut Random,
) -> Result<Option<Placed>, TownError> {
    let settings = search.settings;
    let mut site_attempt = 0;
    let mut alternate_sites = None;
    let mut fallback = None::<Placed>;
    let anchor = search.layout.anchor.map(|(_, pose)| pose);
    let attempts = if anchor.is_some() {
        1
    } else {
        settings.attempts_per_copy
    };
    for _ in 0..attempts {
        if (search.cancelled)() {
            return Err(TownError::Cancelled);
        }
        if search.total_attempts >= settings.max_total_attempts || search.remaining_samples == 0 {
            break;
        }
        search.total_attempts += 1;
        let alternate = alternate_sites.as_mut().and_then(Iterator::next);
        let (position, yaw) = if let Some(pose) = anchor {
            ([pose.origin[0], pose.origin[1]], pose.yaw)
        } else {
            alternate.unwrap_or_else(|| {
                let site = candidate(
                    search.mouth,
                    output.centre,
                    placement,
                    site_attempt,
                    &footprint,
                    settings,
                    random,
                );
                site_attempt += 1;
                site
            })
        };
        let Some(mut placed) = fit_placement(search, generation, footprint, position, yaw, None)?
        else {
            continue;
        };
        if settings.roads.is_some() {
            if output.roads.is_none() && alternate.is_none() {
                alternate_sites = Some(centre_orientations(position, yaw));
            }
            if !connect_street(
                search,
                generation.resolved_recipe()?,
                &mut placed,
                None,
                None,
            )? {
                continue;
            }
        }
        if let Some(placed) = prefer_ring_frontage(placed, &mut fallback) {
            return Ok(Some(placed));
        }
    }
    Ok(fallback)
}

fn find_frontage(
    search: &mut Search<'_>,
    generation: &mut EntryGeneration<'_>,
    footprint: BoxCollider,
    network: &RoadNetwork,
    lots: &mut roads::Lots,
) -> Result<Option<Placed>, TownError> {
    let settings = search.settings;
    let mut attempts = 0;
    for lot in lots.by_ref() {
        if (search.cancelled)() {
            return Err(TownError::Cancelled);
        }
        if search.total_attempts >= settings.max_total_attempts || search.remaining_samples == 0 {
            break;
        }
        let rect = posed_rect(footprint, lot.position, lot.yaw, 0.);
        if blocked(&rect, search) || roads::obstructs(network, &rect, false) {
            continue;
        }
        attempts += 1;
        search.total_attempts += 1;
        if let Some(placed) = fit_frontage(search, generation, footprint, network, lot)? {
            return Ok(Some(placed));
        }
        if attempts >= settings.attempts_per_copy {
            break;
        }
    }
    Ok(None)
}

fn fit_frontage(
    search: &mut Search<'_>,
    generation: &mut EntryGeneration<'_>,
    footprint: BoxCollider,
    network: &RoadNetwork,
    lot: Lot,
) -> Result<Option<Placed>, TownError> {
    let Some(mut placed) = fit_placement(
        search,
        generation,
        footprint,
        lot.position,
        lot.yaw,
        Some((network, lot)),
    )?
    else {
        return Ok(None);
    };
    let building = generation.resolved_recipe()?;
    if connect_street(search, building, &mut placed, Some(lot), Some(network))? {
        return Ok(Some(placed));
    }
    Ok(None)
}

fn prefer_ring_frontage(placed: Placed, fallback: &mut Option<Placed>) -> Option<Placed> {
    let Some(streets) = &placed.streets else {
        return Some(placed);
    };
    if streets.ring_length >= 88. {
        return Some(placed);
    }
    // Give a useful arc of the square priority, while keeping the best short
    // plan in reserve for a bank with a broken circuit.
    if fallback
        .as_ref()
        .and_then(|prior| prior.streets.as_ref())
        .is_none_or(|prior| {
            streets
                .ring_length
                .total_cmp(&prior.ring_length)
                .then_with(|| {
                    streets
                        .network
                        .strips
                        .len()
                        .cmp(&prior.network.strips.len())
                })
                .is_gt()
        })
    {
        *fallback = Some(placed);
    }
    None
}

fn fit_placement(
    search: &mut Search<'_>,
    generation: &mut EntryGeneration<'_>,
    footprint: BoxCollider,
    position: [f32; 2],
    yaw: f32,
    street: Option<(&RoadNetwork, Lot)>,
) -> Result<Option<Placed>, TownError> {
    let settings = search.settings;
    let rect = posed_rect(footprint, position, yaw, 0.);
    if blocked(&rect, search)
        || street.is_some_and(|(network, _)| roads::obstructs(network, &rect, false))
    {
        return Ok(None);
    }
    let Some(floor) = sample_regions(
        generation.prototype.foundations.iter().copied(),
        position,
        yaw,
        search,
    ) else {
        return Ok(None);
    };
    let building = generation.resolved_recipe()?;
    let pose = PlacementPose {
        origin: [
            position[0],
            position[1],
            floor + building.floor_thickness + 0.02,
        ],
        yaw,
    };
    let Some(supports) = fit_supports(building, pose, search, street) else {
        return Ok(None);
    };
    if supports
        .foundation_colliders
        .iter()
        .any(|bounds| bounds.max[2] - bounds.min[2] > settings.max_support_height)
    {
        return Ok(None);
    }
    let approaches = door_approaches(building, settings.approach_length);
    if sample_regions(
        approaches.chain(supports.stairs.iter().map(|stairs| stairs.bounds)),
        position,
        yaw,
        search,
    )
    .is_none()
    {
        return Ok(None);
    }
    let reserved = enclose(supports.clearance_bounds, footprint);
    let clearance = posed_rect(reserved, position, yaw, settings.clearance_margin);
    let occupied = if settings.roads.is_some() {
        let shell_margin = building.roof.overhang + 0.15;
        let mut occupied: Vec<_> = generation
            .prototype
            .foundations
            .iter()
            .map(|&bounds| posed_rect(bounds, position, yaw, shell_margin))
            .collect();
        occupied.extend(
            supports
                .stairs
                .iter()
                .map(|stair| posed_rect(stair.bounds, position, yaw, 0.)),
        );
        if occupied.iter().any(|region| blocked(region, search))
            || overlaps_any(&clearance, search.reservations)
        {
            return Ok(None);
        }
        occupied
    } else if blocked(&clearance, search) {
        return Ok(None);
    } else {
        vec![clearance]
    };
    Ok(Some(Placed {
        pose,
        supports,
        clearance,
        occupied,
        access: None,
        streets: None,
        lot: street.map(|(_, lot)| lot),
    }))
}

fn fit_supports(
    building: &crate::BuildingRecipe,
    pose: PlacementPose,
    search: &Search<'_>,
    street: Option<(&RoadNetwork, Lot)>,
) -> Option<terrain_support::FittedSupports> {
    let terrain = &search.input.terrain;
    let options = terrain_support::SupportOptions {
        sample_spacing: search.settings.sample_spacing.min(terrain.spacing * 0.5),
        ..terrain_support::SupportOptions::default()
    };
    if let Some((network, lot)) = street.filter(|(_, lot)| lot.road_steps.is_some()) {
        let door = roads::entrance(building)?;
        let target = roads::stair_target(network, door, lot)?;
        let finished = |x, y| {
            terrain
                .sample(x, y)
                .map(|natural| network.graded_height(motu::Vec2::new(x, y), natural))
        };
        return terrain_support::fit_with_stair_target(
            building,
            pose,
            &finished,
            options,
            Some(target),
        )
        .ok();
    }
    terrain_support::fit_with_options(building, pose, &|x, y| terrain.sample(x, y), options).ok()
}

fn blocked(rect: &ClearanceRect, search: &Search<'_>) -> bool {
    let settings = search.settings;
    search
        .occupied
        .iter()
        .any(|other| overlaps(rect, other, settings.building_gap))
        || overlaps_any(rect, &search.access)
        || overlaps_any(rect, search.reservations)
        || (settings.roads.is_some() && protected(rect, search.input))
        || touches_river(rect, search.input, settings.river_margin)
}

fn connect_street(
    search: &Search<'_>,
    building: &crate::BuildingRecipe,
    placed: &mut Placed,
    lot: Option<Lot>,
    roads: Option<&RoadNetwork>,
) -> Result<bool, TownError> {
    let settings = search.settings;
    let input = search.input;
    let pose = placed.pose;
    let supports = &placed.supports;
    let access_blocked = |rect: &ClearanceRect| {
        overlaps_any(rect, &search.occupied) || overlaps_any(rect, search.reservations)
    };
    let support_rect = posed_rect(
        supports.clearance_bounds,
        [pose.origin[0], pose.origin[1]],
        pose.yaw,
        0.05,
    );
    if let (Some(lot), Some(network)) = (lot, roads) {
        placed.access = roads::entrance(building).and_then(|door| {
            if lot.road_steps.is_some() {
                if !stairs_clear_pavement(supports, door, network, lot) {
                    return None;
                }
                return roads::stair_access(network, door, lot, input, settings, &access_blocked);
            }
            if roads::obstructs(network, &support_rect, true) {
                return None;
            }
            roads::approach(
                network,
                building,
                door,
                supports,
                pose,
                lot.street_edge,
                input,
                settings,
                &access_blocked,
                settings.roads.expect("road lot").max_frontage_distance,
            )
        });
    } else {
        let core = enclose(shell_footprint(building), supports.clearance_bounds);
        let Some(streets) = roads::layout(
            core,
            pose,
            settings,
            input,
            search.reservations,
            search.layout.rings,
            search.cancelled,
        )?
        else {
            return Ok(false);
        };
        let approach = |streets: &roads::StreetLayout| {
            (!roads::obstructs(&streets.roads, &support_rect, true))
                .then(|| {
                    roads::central_approach(
                        streets,
                        building,
                        supports,
                        pose,
                        input,
                        settings,
                        &access_blocked,
                    )
                })
                .flatten()
        };
        if let Some((root, access)) = approach(&streets) {
            let centre = Vec2::new(pose.origin[0], pose.origin[1]);
            if let Some(with_pier) = streets.with_pier(
                root,
                centre,
                search.layout.piers,
                settings,
                input,
                search.cancelled,
            )? && let Some((connected, access)) = approach(&with_pier)
                && with_pier.same_component(root, connected)
            {
                placed.access = Some(access);
                placed.streets = Some(with_pier.finish(connected));
            } else {
                placed.access = Some(access);
                placed.streets = Some(streets.finish(root));
            }
        }
    }
    Ok(placed.access.is_some())
}

fn stairs_clear_pavement(
    supports: &terrain_support::FittedSupports,
    door: &crate::OpeningRecipe,
    network: &RoadNetwork,
    lot: Lot,
) -> bool {
    let normal = door.facade.normal();
    supports.stairs.iter().all(|stair| {
        let mut bounds = stair.bounds;
        if stair.door_id == door.id {
            // Exclude only the nose of the selected tread from the overlap check.
            let reach = roads::PAVED_TREAD_OVERLAP + 0.001;
            for (axis, direction) in normal.into_iter().enumerate().take(2) {
                bounds.max[axis] -= direction.max(0.) * reach;
                bounds.min[axis] -= direction.min(0.) * reach;
            }
        }
        !roads::obstructs(
            network,
            &posed_rect(bounds, lot.position, lot.yaw, 0.),
            false,
        )
    })
}

fn centre_orientations(position: [f32; 2], yaw: f32) -> impl Iterator<Item = ([f32; 2], f32)> {
    // The building can fit on a bank that only has a usable road in one
    // direction. Recheck the footprint and door for alternative headings.
    [
        90_f32, -90., 180., 30., -30., 60., -60., 120., -120., 150., -150., 15., -15.,
    ]
    .into_iter()
    .map(move |degrees| (position, yaw + degrees.to_radians()))
}

fn sample_regions(
    mut regions: impl Iterator<Item = BoxCollider>,
    position: [f32; 2],
    yaw: f32,
    search: &mut Search<'_>,
) -> Option<f32> {
    regions.try_fold(f32::NEG_INFINITY, |highest, bounds| {
        let rect = posed_rect(bounds, position, yaw, 0.);
        fitted_floor(
            &rect,
            search.input,
            search.settings,
            &mut search.remaining_samples,
            search.cancelled,
        )
        .map(|height| highest.max(height))
    })
}

fn door_approaches(
    building: &crate::BuildingRecipe,
    length: f32,
) -> impl Iterator<Item = BoxCollider> + '_ {
    building
        .openings
        .iter()
        .filter(|opening| opening.kind == crate::OpeningKind::Door && opening.storey == 0)
        .map(move |opening| {
            let point = opening.facade.point(building, opening.offset, 0., 0.);
            let normal = opening.facade.normal();
            let tangent = opening.facade.tangent();
            let mut bounds = BoxCollider {
                min: [f32::INFINITY; 3],
                max: [f32::NEG_INFINITY; 3],
            };
            for along in [-opening.width * 0.5 - 0.5, opening.width * 0.5 + 0.5] {
                for outward in [0., length.max(0.01)] {
                    for axis in 0..2 {
                        let value = point[axis] + tangent[axis] * along + normal[axis] * outward;
                        bounds.min[axis] = bounds.min[axis].min(value);
                        bounds.max[axis] = bounds.max[axis].max(value);
                    }
                }
            }
            bounds.min[2] = 0.;
            bounds.max[2] = 0.;
            bounds
        })
}

pub(super) fn door_clearances(
    building: &crate::BuildingRecipe,
    pose: PlacementPose,
    length: f32,
) -> impl Iterator<Item = ClearanceRect> + '_ {
    door_approaches(building, length)
        .map(move |bounds| posed_rect(bounds, [pose.origin[0], pose.origin[1]], pose.yaw, 0.))
}

fn publish_placement(
    output: &mut TownOutput,
    placed: Placed,
    index: usize,
) -> Result<(), TownError> {
    output.clearance.push(placed.clearance);
    append_mesh(
        &mut output.supports,
        &mut output.supports_grub_positions,
        &placed.supports.mesh,
        placed.pose,
    )?;
    output.support_colliders.extend(
        placed
            .supports
            .foundation_colliders
            .iter()
            .map(|bounds| world_bounds(*bounds, placed.pose)),
    );
    output.stair_colliders.extend(
        placed
            .supports
            .stair_colliders
            .iter()
            .map(|bounds| world_bounds(*bounds, placed.pose)),
    );
    if placed.access.is_some() {
        let house = [placed.pose.origin[0], placed.pose.origin[1]];
        output
            .stair_feet
            .extend(placed.supports.stairs.iter().map(|stair| {
                let world = placed.pose.world_point([stair.foot[0], stair.foot[1], 0.]);
                motu::TownStairFoot {
                    position: [world[0], world[1]],
                    house,
                }
            }));
        output.footpath_blockers.extend(
            placed
                .supports
                .occupied_footprints
                .iter()
                .map(|&bounds| posed_rect(bounds, house, placed.pose.yaw, 0.)),
        );
    }
    output.instances.push(TownInstance {
        prototype_index: index,
        entry_id: String::new(),
        settlement_index: 0,
        position: placed.pose.origin,
        yaw: placed.pose.yaw,
        frontage: placed.access.map(|access| {
            output.clearance.push(access.clearance);
            access.frontage
        }),
    });
    Ok(())
}

fn enclose(a: BoxCollider, b: BoxCollider) -> BoxCollider {
    BoxCollider {
        min: std::array::from_fn(|i| a.min[i].min(b.min[i])),
        max: std::array::from_fn(|i| a.max[i].max(b.max[i])),
    }
}

fn shell_footprint(building: &crate::BuildingRecipe) -> BoxCollider {
    let mut bounds = BoxCollider {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };
    for footprint in terrain_support::foundation_footprints(building) {
        for axis in 0..2 {
            bounds.min[axis] = bounds.min[axis].min(footprint.min[axis]);
            bounds.max[axis] = bounds.max[axis].max(footprint.max[axis]);
        }
    }
    for axis in 0..2 {
        bounds.min[axis] -= building.roof.overhang + 0.15;
        bounds.max[axis] += building.roof.overhang + 0.15;
    }
    bounds.min[2] = 0.;
    bounds.max[2] = 0.;
    bounds
}

fn footprint(building: &crate::BuildingRecipe, settings: &TownSettings) -> BoxCollider {
    let mut bounds = shell_footprint(building);
    for door in building
        .openings
        .iter()
        .filter(|opening| opening.kind == crate::OpeningKind::Door && opening.storey == 0)
    {
        let point = door.facade.point(building, door.offset, 0., 0.);
        for (axis, value) in point.into_iter().enumerate().take(2) {
            bounds.min[axis] = bounds.min[axis].min(value - settings.approach_length);
            bounds.max[axis] = bounds.max[axis].max(value + settings.approach_length);
        }
    }
    bounds.min[2] = 0.;
    bounds.max[2] = 0.;
    bounds
}

fn candidate(
    mouth: TownMouth,
    centre: Option<[f32; 2]>,
    placement: TownPlacement,
    attempt: u32,
    bounds: &BoxCollider,
    settings: &TownSettings,
    random: &mut Random,
) -> ([f32; 2], f32) {
    let d = [
        mouth.upstream[0] - mouth.position[0],
        mouth.upstream[1] - mouth.position[1],
    ];
    let length = d[0].hypot(d[1]).max(0.001);
    let inward = [d[0] / length, d[1] / length];
    let yaw = inward[1].atan2(inward[0]);
    let radius = distance(
        [bounds.min[0], bounds.min[1]],
        [bounds.max[0], bounds.max[1]],
    ) * 0.5;
    if let Some(centre) = centre {
        let inner = if placement == TownPlacement::Centre {
            0.
        } else {
            radius + settings.building_gap + 8.
        };
        let outer = if placement == TownPlacement::Centre {
            settings.town_radius * 0.4
        } else {
            settings.town_radius
        };
        let angle = random.unit() * std::f32::consts::TAU;
        let r = (inner + random.unit().sqrt() * (outer - inner).max(0.)).min(settings.town_radius);
        let position = [centre[0] + angle.cos() * r, centre[1] + angle.sin() * r];
        // The conventional south-facing main entrance faces the settlement centre.
        let facing =
            (centre[1] - position[1]).atan2(centre[0] - position[0]) + std::f32::consts::FRAC_PI_2;
        (position, facing)
    } else {
        // A river can curve along the shore or enter a tidal cut. Its final
        // tangent does not identify the only dry bank: cover the full circle.
        let phase = f32::from(u16::try_from(attempt).expect("validated attempt limit"));
        let angle = yaw + phase * 2.399_963_1 + random.unit() * 0.2;
        let min_radius = radius + settings.river_margin + 4.;
        // A larger search radius gives a street layout more fallback sites.
        // Check the usual river-mouth area before sampling distant banks.
        let outer = if settings.roads.is_some() && attempt < settings.attempts_per_copy / 2 {
            settings
                .site_search_radius
                .min(TownSettings::default().site_search_radius.max(min_radius))
        } else {
            settings.site_search_radius
        };
        let inner = min_radius.min(outer);
        let r = inner + random.unit() * (outer - inner);
        let heading = yaw + (random.unit() - 0.5) * std::f32::consts::PI;
        (
            [
                mouth.position[0] + angle.cos() * r,
                mouth.position[1] + angle.sin() * r,
            ],
            heading,
        )
    }
}

fn posed_rect(bounds: BoxCollider, origin: [f32; 2], yaw: f32, margin: f32) -> ClearanceRect {
    let mut rect = ClearanceRect {
        centre: origin,
        half_extents: [
            (bounds.max[0] - bounds.min[0]) * 0.5 + margin,
            (bounds.max[1] - bounds.min[1]) * 0.5 + margin,
        ],
        yaw,
    };
    rect.centre = rect.world([
        (bounds.min[0] + bounds.max[0]) * 0.5,
        (bounds.min[1] + bounds.max[1]) * 0.5,
    ]);
    rect
}

pub(super) fn overlaps(a: &ClearanceRect, b: &ClearanceRect, gap: f32) -> bool {
    let (sa, ca) = a.yaw.sin_cos();
    let (sb, cb) = b.yaw.sin_cos();
    let axes_a = [[ca, sa], [-sa, ca]];
    let axes_b = [[cb, sb], [-sb, cb]];
    axes_a.into_iter().chain(axes_b).all(|axis| {
        let dot = |u: [f32; 2]| axis[0] * u[0] + axis[1] * u[1];
        let radius = |rect: &ClearanceRect, axes: [[f32; 2]; 2]| {
            (rect.half_extents[0] + gap * 0.5) * dot(axes[0]).abs()
                + (rect.half_extents[1] + gap * 0.5) * dot(axes[1]).abs()
        };
        ((a.centre[0] - b.centre[0]) * axis[0] + (a.centre[1] - b.centre[1]) * axis[1]).abs()
            < radius(a, axes_a) + radius(b, axes_b)
    })
}

pub(super) fn overlaps_any(rect: &ClearanceRect, others: &[ClearanceRect]) -> bool {
    others.iter().any(|other| overlaps(rect, other, 0.))
}

// Positive finite geometry and the 100000-cell bound make these grid casts safe.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn fitted_floor(
    rect: &ClearanceRect,
    input: &TownInput,
    settings: &TownSettings,
    remaining_samples: &mut usize,
    cancelled: &dyn Fn() -> bool,
) -> Option<f32> {
    let spacing = settings.sample_spacing.min(input.terrain.spacing * 0.5);
    let nx = (rect.half_extents[0] * 2. / spacing).ceil() as usize;
    let ny = (rect.half_extents[1] * 2. / spacing).ceil() as usize;
    if nx == 0 || ny == 0 || nx.checked_mul(ny)? > 100_000 {
        return None;
    }
    let sample_count = (nx + 1).checked_mul(ny + 1)?;
    if *remaining_samples < sample_count {
        *remaining_samples = 0;
        return None;
    }
    *remaining_samples -= sample_count;
    let dx = rect.half_extents[0] * 2. / nx as f32;
    let dy = rect.half_extents[1] * 2. / ny as f32;
    let max_gradient = settings.max_slope_degrees.to_radians().tan();
    let mut max = f32::NEG_INFINITY;
    for y in 0..=ny {
        if cancelled() {
            return None;
        }
        for x in 0..=nx {
            let p = rect.world([
                -rect.half_extents[0] + x as f32 * dx,
                -rect.half_extents[1] + y as f32 * dy,
            ]);
            let h = input.terrain.sample(p[0], p[1])?;
            if h <= input.sea_level + settings.min_dry_height {
                return None;
            }
            // World-axis finite differences catch both cross-slope and internal peaks.
            let gx = (input.terrain.sample(p[0] + spacing * 0.5, p[1])?
                - input.terrain.sample(p[0] - spacing * 0.5, p[1])?)
                / spacing;
            let gy = (input.terrain.sample(p[0], p[1] + spacing * 0.5)?
                - input.terrain.sample(p[0], p[1] - spacing * 0.5)?)
                / spacing;
            if gx.hypot(gy) > max_gradient {
                return None;
            }
            max = max.max(h);
        }
    }
    Some(max)
}

pub(super) fn protected(rect: &ClearanceRect, input: &TownInput) -> bool {
    use motu::Vec2;
    input.protected_terrain.iter().any(|area| {
        let a = Vec2::from_array(area.min);
        let b = Vec2::from_array(area.max);
        overlaps(
            rect,
            &ClearanceRect {
                centre: ((a + b) * 0.5).to_array(),
                half_extents: ((b - a) * 0.5).to_array(),
                yaw: 0.,
            },
            0.,
        )
    })
}

pub(super) fn touches_river(rect: &ClearanceRect, input: &TownInput, margin: f32) -> bool {
    input.rivers.iter().any(|river| {
        river.points.windows(2).enumerate().any(|(i, pair)| {
            let radius = river.width(i).max(river.width(i + 1)) * 0.5 + margin;
            river_intersects(rect, pair[0], pair[1], radius)
        })
    })
}

pub(super) fn river_intersects(
    rect: &ClearanceRect,
    start: [f32; 2],
    end: [f32; 2],
    radius: f32,
) -> bool {
    segment_intersects_box(
        rect.local(start),
        rect.local(end),
        rect.half_extents.map(|half| half + radius),
    )
}

fn segment_intersects_box(a: [f32; 2], b: [f32; 2], half: [f32; 2]) -> bool {
    let mut near = 0_f32;
    let mut far = 1_f32;
    for axis in 0..2 {
        let direction = b[axis] - a[axis];
        if direction.abs() < 1e-7 {
            if a[axis].abs() > half[axis] {
                return false;
            }
        } else {
            let lower = (-half[axis] - a[axis]) / direction;
            let upper = (half[axis] - a[axis]) / direction;
            near = near.max(lower.min(upper));
            far = far.min(lower.max(upper));
            if near > far {
                return false;
            }
        }
    }
    true
}

fn transform(point: [f32; 3], pose: PlacementPose) -> [f32; 3] {
    let (s, c) = pose.yaw.sin_cos();
    [
        pose.origin[0] + c * point[0] - s * point[1],
        pose.origin[1] + s * point[0] + c * point[1],
        pose.origin[2] + point[2],
    ]
}
fn world_bounds(bounds: BoxCollider, pose: PlacementPose) -> BoxCollider {
    let mut result = BoxCollider {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };
    for x in [bounds.min[0], bounds.max[0]] {
        for y in [bounds.min[1], bounds.max[1]] {
            for z in [bounds.min[2], bounds.max[2]] {
                let point = transform([x, y, z], pose);
                for (axis, value) in point.into_iter().enumerate() {
                    result.min[axis] = result.min[axis].min(value);
                    result.max[axis] = result.max[axis].max(value);
                }
            }
        }
    }
    result
}
fn append_mesh(
    target: &mut BuildingMesh,
    grub: &mut Vec<[f32; 3]>,
    source: &BuildingMesh,
    pose: PlacementPose,
) -> Result<(), TownError> {
    let base = support_index_offset(target, source)?;
    target
        .positions
        .extend(source.positions.iter().map(|point| transform(*point, pose)));
    grub.extend_from_slice(&source.positions);
    let rotation = PlacementPose {
        origin: [0.; 3],
        ..pose
    };
    target.normals.extend(
        source
            .normals
            .iter()
            .map(|normal| transform(*normal, rotation)),
    );
    target.uvs.extend_from_slice(&source.uvs);
    target
        .indices
        .extend(source.indices.iter().map(|index| index + base));
    Ok(())
}

pub(super) fn support_index_offset(
    target: &BuildingMesh,
    source: &BuildingMesh,
) -> Result<u32, TownError> {
    let overflow =
        || TownError::Support("combined support vertex count exceeds u32 indices".into());
    let base = u32::try_from(target.positions.len()).map_err(|_| overflow())?;
    let count = u32::try_from(source.positions.len()).map_err(|_| overflow())?;
    base.checked_add(count).ok_or_else(overflow)?;
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::town::HeightGrid;

    #[test]
    fn footprint_samples_reject_interior_peak_between_flat_corners() {
        let mut heights = vec![4.; 21 * 21];
        heights[10 * 21 + 10] = 8.;
        let input = TownInput {
            tile_seed: 1,
            sea_level: 0.,
            bridge_exclusions: Vec::new(),
            disable_bridges: false,
            protected_terrain: vec![],
            rivers: vec![],
            terrain: HeightGrid {
                origin: [-10., -10.],
                spacing: 1.,
                width: 21,
                height: 21,
                heights,
            },
        };
        let rect = ClearanceRect {
            centre: [0.; 2],
            half_extents: [5.; 2],
            yaw: 0.,
        };
        let mut budget = 10_000;
        assert!(
            fitted_floor(
                &rect,
                &input,
                &TownSettings::default(),
                &mut budget,
                &|| false
            )
            .is_none()
        );
    }

    #[test]
    fn channel_crossing_is_rejected_when_both_endpoints_are_outside() {
        assert!(segment_intersects_box([-10., -10.], [10., 10.], [2.; 2]));
        assert!(!segment_intersects_box([-10., 3.], [10., 3.], [2.; 2]));
    }
}

#[cfg(test)]
mod occupied_footprint_tests {
    use super::*;
    use crate::town::HeightGrid;

    #[test]
    fn isolated_buttress_footing_participates_in_town_dry_and_slope_validation() {
        let building = crate::BuildingRecipe::gothic_church();
        let regions = terrain_support::foundation_footprints(&building);
        let pier = regions
            .iter()
            .find(|region| {
                (region.max[2] + 0.2).abs() < 0.00001
                    && (-13. ..-5.).contains(&((region.min[1] + region.max[1]) * 0.5))
            })
            .unwrap();
        let point = [
            (pier.min[0] + pier.max[0]) * 0.5,
            (pier.min[1] + pier.max[1]) * 0.5,
        ];
        for height in [0.2, 14.] {
            let mut heights = vec![10.; 121 * 121];
            for y in 0_u16..121 {
                for x in 0_u16..121 {
                    let world = [-30. + f32::from(x) * 0.5, -30. + f32::from(y) * 0.5];
                    if (world[0] - point[0]).abs() < 0.75 && (world[1] - point[1]).abs() < 0.75 {
                        heights[usize::from(y) * 121 + usize::from(x)] = height;
                    }
                }
            }
            let input = TownInput {
                tile_seed: 1,
                sea_level: 0.,
                bridge_exclusions: Vec::new(),
                disable_bridges: false,
                protected_terrain: vec![],
                rivers: vec![],
                terrain: HeightGrid {
                    origin: [-30.; 2],
                    spacing: 0.5,
                    width: 121,
                    height: 121,
                    heights,
                },
            };
            let settings = TownSettings::default();
            let mut search = Search {
                settings: &settings,
                input: &input,
                mouth: TownMouth {
                    river_index: 0,
                    position: [0.; 2],
                    upstream: [0., 1.],
                    river_length: 1.,
                },
                cancelled: &|| false,
                reservations: &[],
                occupied: vec![],
                access: Vec::new(),
                rows_by_ring: Vec::new(),
                curb_exits: roads::CurbExits::default(),
                accepted_lots: Vec::new(),
                total_attempts: 0,
                remaining_samples: 100_000,
                layout: LayoutTrial::default(),
                reached_rings: 0,
            };
            assert!(
                sample_regions(regions[..5].iter().copied(), [0.; 2], 0., &mut search).is_some(),
                "a changed isolated pier lies outside the five floor regions"
            );
            assert!(
                sample_regions(regions.iter().copied(), [0.; 2], 0., &mut search).is_none(),
                "wet or steep terrain under an actual pier footing must reject the site"
            );
        }
    }

    #[test]
    fn unused_cruciform_recess_dip_does_not_reject_or_raise_foundations() {
        let building = crate::BuildingRecipe::gothic_church();
        let mut heights = vec![10.; 61 * 61];
        // A wet, steep hollow inside the overall church bounds, but outside
        // its nave, transepts, towers and south-facing entrance approach.
        for y in 42..=47 {
            for x in 18..=19 {
                heights[y * 61 + x] = 0.2;
            }
        }
        let input = TownInput {
            tile_seed: 1,
            sea_level: 0.,
            bridge_exclusions: Vec::new(),
            disable_bridges: false,
            protected_terrain: vec![],
            rivers: vec![],
            terrain: HeightGrid {
                origin: [-30.; 2],
                spacing: 1.,
                width: 61,
                height: 61,
                heights,
            },
        };
        let settings = TownSettings::default();
        let mouth = TownMouth {
            river_index: 0,
            position: [0.; 2],
            upstream: [0., 1.],
            river_length: 1.,
        };
        let mut search = Search {
            settings: &settings,
            input: &input,
            mouth,
            cancelled: &|| false,
            reservations: &[],
            occupied: vec![],
            access: Vec::new(),
            rows_by_ring: Vec::new(),
            curb_exits: roads::CurbExits::default(),
            accepted_lots: Vec::new(),
            total_attempts: 0,
            remaining_samples: 100_000,
            layout: LayoutTrial::default(),
            reached_rings: 0,
        };
        let floor = sample_regions(
            terrain_support::foundation_footprints(&building).into_iter(),
            [0.; 2],
            0.,
            &mut search,
        )
        .unwrap();
        assert!((floor - 10.).abs() < 0.001);
        let pose = PlacementPose {
            origin: [0., 0., floor + building.floor_thickness + 0.02],
            yaw: 0.,
        };
        let supports =
            terrain_support::fit(&building, pose, &|x, y| input.terrain.sample(x, y)).unwrap();
        assert!(
            supports
                .foundation_colliders
                .iter()
                .all(|b| b.max[2] - b.min[2] < 0.5)
        );
        assert!(
            sample_regions(
                door_approaches(&building, settings.approach_length),
                [0.; 2],
                0.,
                &mut search
            )
            .is_some()
        );
    }
}
