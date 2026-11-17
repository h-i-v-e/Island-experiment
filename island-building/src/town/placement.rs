use super::rivers::distance;
use super::{
    ClearanceRect, TownError, TownInput, TownInstance, TownMouth, TownOutput, TownPlacement,
    TownPrototype, TownRecipe, TownSettings,
};
use crate::{
    BuildingMesh, BuildingPrototype, MaterialSlot,
    collision::BoxCollider,
    terrain_support::{self, PlacementPose},
};
use std::path::Path;

struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
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
fn entry_seed(seed: u64, id: &str) -> u64 {
    id.bytes().fold(seed ^ 0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}

struct Search<'a> {
    settings: &'a TownSettings,
    input: &'a TownInput,
    mouth: TownMouth,
    cancelled: &'a dyn Fn() -> bool,
    occupied: Vec<ClearanceRect>,
    total_attempts: u32,
    remaining_samples: usize,
}
struct EntryGeneration<'a> {
    building: &'a crate::BuildingRecipe,
    base: &'a Path,
    id: &'a str,
    prototype: Option<BuildingPrototype>,
    foundations: Vec<BoxCollider>,
}
impl EntryGeneration<'_> {
    fn resolved_recipe(&mut self) -> Result<&crate::BuildingRecipe, TownError> {
        if self.prototype.is_none() {
            self.prototype = Some(crate::generate_with_base(self.building, self.base).map_err(
                |error| TownError::Building {
                    entry: self.id.into(),
                    reason: error.to_string(),
                },
            )?);
        }
        Ok(&self
            .prototype
            .as_ref()
            .expect("prototype generated above")
            .recipe)
    }
}

struct Placed {
    pose: PlacementPose,
    supports: terrain_support::FittedSupports,
    clearance: ClearanceRect,
}

pub(super) fn generate(
    recipe: &TownRecipe,
    input: &TownInput,
    mouth: TownMouth,
    base: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<TownOutput, TownError> {
    let mut output = TownOutput {
        tile_seed: input.tile_seed,
        mouth: Some(mouth),
        ..TownOutput::default()
    };
    let mut search = Search {
        settings: &recipe.settings,
        input,
        mouth,
        cancelled,
        occupied: Vec::new(),
        total_attempts: 0,
        remaining_samples: 8_000_000,
    };
    for placement in [TownPlacement::Centre, TownPlacement::Other] {
        for entry in recipe
            .catalogue
            .iter()
            .filter(|entry| entry.placement == placement)
        {
            generate_entry(entry, &mut search, &mut output, base)?;
        }
    }
    Ok(output)
}

fn generate_entry(
    entry: &super::TownCatalogueEntry,
    search: &mut Search<'_>,
    output: &mut TownOutput,
    base: &Path,
) -> Result<(), TownError> {
    if (search.cancelled)() {
        return Err(TownError::Cancelled);
    }
    let seed = entry_seed(search.input.tile_seed, &entry.id);
    let mut random = Random(seed);
    let offset = u32::try_from(random.next() % u64::from(entry.max_count - entry.min_count + 1))
        .expect("validated maximum count fits u32");
    let copies = entry.min_count + offset;
    if copies == 0 {
        return Ok(());
    }
    let mut building = entry.building.clone(); // Entry seed override needs independent owned recipe.
    building.seed = Random(seed ^ building.seed).next();
    building.materials.foundation_recipe = None;
    let resolved = crate::openings::resolve(&building).map_err(|error| TownError::Building {
        entry: entry.id.clone(),
        reason: error.to_string(),
    })?;
    let footprint = footprint(resolved.as_ref(), search.settings);
    let first_instance = output.instances.len();
    let mut generation = EntryGeneration {
        building: &building,
        base,
        id: &entry.id,
        prototype: None,
        foundations: terrain_support::foundation_footprints(&building),
    };
    for _ in 0..copies {
        let Some(placed) = find_placement(
            search,
            output.centre,
            &mut generation,
            footprint,
            entry.placement,
            &mut random,
        )?
        else {
            output.skipped_count += 1;
            continue;
        };
        output
            .centre
            .get_or_insert([placed.pose.origin[0], placed.pose.origin[1]]);
        search.occupied.push(placed.clearance);
        publish_placement(output, &placed)?;
    }
    if output.instances.len() > first_instance
        && let Some(mut prototype) = generation.prototype
    {
        if output.support_material.is_none()
            && let Some(index) = prototype
                .materials
                .iter()
                .position(|material| material.slot == MaterialSlot::Foundation)
        {
            output.support_material = Some(prototype.materials.remove(index));
        }
        terrain_support::remove_supports(&mut prototype);
        prototype
            .materials
            .retain(|material| material.slot != MaterialSlot::Foundation);
        output.prototypes.push(TownPrototype {
            entry_id: entry.id.clone(),
            building: prototype,
        });
    }
    Ok(())
}

fn find_placement(
    search: &mut Search<'_>,
    centre: Option<[f32; 2]>,
    generation: &mut EntryGeneration<'_>,
    footprint: BoxCollider,
    placement: TownPlacement,
    random: &mut Random,
) -> Result<Option<Placed>, TownError> {
    let settings = search.settings;
    let input = search.input;
    for attempt in 0..settings.attempts_per_copy {
        if (search.cancelled)() {
            return Err(TownError::Cancelled);
        }
        if search.total_attempts >= settings.max_total_attempts || search.remaining_samples == 0 {
            break;
        }
        search.total_attempts += 1;
        let (position, yaw) = candidate(
            search.mouth,
            centre,
            placement,
            attempt,
            &footprint,
            settings,
            random,
        );
        let rect = posed_rect(footprint, position, yaw, 0.);
        if search
            .occupied
            .iter()
            .any(|other| overlaps(&rect, other, settings.building_gap))
            || touches_river(&rect, input, settings.river_margin)
        {
            continue;
        }
        let Some(floor) = sample_regions(
            generation.foundations.iter().copied(),
            position,
            yaw,
            search,
        ) else {
            continue;
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
        let sampler = |x, y| input.terrain.sample(x, y);
        let options = terrain_support::SupportOptions {
            sample_spacing: settings.sample_spacing.min(input.terrain.spacing * 0.5),
            ..terrain_support::SupportOptions::default()
        };
        let Ok(supports) = terrain_support::fit_with_options(building, pose, &sampler, options)
        else {
            continue;
        };
        if supports
            .foundation_colliders
            .iter()
            .any(|bounds| bounds.max[2] - bounds.min[2] > settings.max_support_height)
        {
            continue;
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
            continue;
        }
        let mut reserved = supports.clearance_bounds;
        for axis in 0..2 {
            reserved.min[axis] = reserved.min[axis].min(footprint.min[axis]);
            reserved.max[axis] = reserved.max[axis].max(footprint.max[axis]);
        }
        let clearance = posed_rect(reserved, position, yaw, settings.clearance_margin);
        if touches_river(&clearance, input, settings.river_margin)
            || search
                .occupied
                .iter()
                .any(|other| overlaps(&clearance, other, settings.building_gap))
        {
            continue;
        }
        return Ok(Some(Placed {
            pose,
            supports,
            clearance,
        }));
    }
    Ok(None)
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

fn publish_placement(output: &mut TownOutput, placed: &Placed) -> Result<(), TownError> {
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
    output.instances.push(TownInstance {
        prototype_index: output.prototypes.len(),
        position: placed.pose.origin,
        yaw: placed.pose.yaw,
    });
    Ok(())
}

fn footprint(building: &crate::BuildingRecipe, settings: &TownSettings) -> BoxCollider {
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
        let inner = (radius + settings.river_margin + 4.).min(settings.site_search_radius);
        let r = inner + random.unit() * (settings.site_search_radius - inner);
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

fn overlaps(a: &ClearanceRect, b: &ClearanceRect, gap: f32) -> bool {
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

fn touches_river(rect: &ClearanceRect, input: &TownInput, margin: f32) -> bool {
    input.rivers.iter().any(|river| {
        river.points.windows(2).enumerate().any(|(i, pair)| {
            let a = rect.local(pair[0]);
            let b = rect.local(pair[1]);
            let radius = river.width(i).max(river.width(i + 1)) * 0.5 + margin;
            segment_intersects_box(
                a,
                b,
                [rect.half_extents[0] + radius, rect.half_extents[1] + radius],
            )
        })
    })
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
    let base = u32::try_from(target.positions.len()).map_err(|_| {
        TownError::Support("combined support vertex count exceeds u32 indices".into())
    })?;
    if target
        .positions
        .len()
        .checked_add(source.positions.len())
        .is_none_or(|count| count > u32::MAX as usize)
    {
        return Err(TownError::Support(
            "combined support vertex count exceeds u32 indices".into(),
        ));
    }
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
                occupied: vec![],
                total_attempts: 0,
                remaining_samples: 100_000,
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
            occupied: vec![],
            total_attempts: 0,
            remaining_samples: 100_000,
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
