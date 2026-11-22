//! Reserve rectangular plots on the finished street layout, after all buildings are placed.

use super::{ClearanceRect, TownError, TownInput, TownOutput, TownSettings, placement, roads};
use crate::terrain_support::PlacementPose;
use motu::Vec2;

const CELL: f32 = 2.;
const MIN_ALONG: u16 = 6;
const MIN_ACROSS: u16 = 4;
const MAX_ALONG: u16 = 30;
const END_HEADLAND: f32 = 1.5;
const SIDE_HEADLAND: f32 = 1.;
const MAX_FIELDS: usize = 4096;
const MAX_SLOPE_DEGREES: f32 = 14.;

pub(super) struct Site<'a> {
    pub settings: &'a TownSettings,
    pub rows: Vec<roads::Row>,
}

pub(super) fn generate(
    output: &TownOutput,
    sites: &[Site<'_>],
    input: &TownInput,
    cancelled: &impl Fn() -> bool,
) -> Result<Vec<ClearanceRect>, TownError> {
    if sites.iter().all(|site| site.rows.is_empty()) {
        return Ok(Vec::new());
    }
    let approaches = approaches(output, sites);
    let mut frontages = Vec::new();
    for site in sites {
        let Some(roads) = site.settings.roads else {
            continue;
        };
        let depth = (roads.ring_spacing * 0.75).min(20.);
        for row in &site.rows {
            for grid in grids(*row, roads.shoulder_width(), depth) {
                if cancelled() {
                    return Err(TownError::Cancelled);
                }
                let bounds = grid.reserve(grid.bounds());
                let blockers = nearby(&bounds, output.clearance.iter().chain(&approaches));
                let rivers = rivers(&bounds, input, site.settings.river_margin);
                let mut frontage = Frontage::new(grid);
                frontage.exclude(|cell| {
                    blocked(&cell, &blockers)
                        || placement::protected(&cell, input)
                        || rivers.iter().any(|river| river.intersects(&cell))
                        || !suitable(&cell, input, site.settings)
                });
                if let Some(cells) = frontage.largest() {
                    frontages.push((cells.area(), frontage));
                }
            }
        }
    }

    frontages.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut reservations = Vec::new();
    let mut fields = Vec::new();
    for (_, mut frontage) in frontages {
        if cancelled() {
            return Err(TownError::Cancelled);
        }
        let grid = frontage.grid;
        let blockers = nearby(&grid.reserve(grid.bounds()), reservations.iter());
        frontage.exclude(|cell| blocked(&cell, &blockers));
        while let Some(cells) = frontage.largest() {
            frontage.take(cells);
            let reserve = grid.reserve(cells);
            fields.push(ClearanceRect {
                half_extents: [
                    reserve.half_extents[0] - END_HEADLAND,
                    reserve.half_extents[1] - SIDE_HEADLAND,
                ],
                ..reserve
            });
            reservations.push(reserve);
            if fields.len() == MAX_FIELDS {
                return Ok(fields);
            }
        }
    }
    Ok(fields)
}

fn approaches(output: &TownOutput, sites: &[Site<'_>]) -> Vec<ClearanceRect> {
    let entrances = output.instances.iter().flat_map(|instance| {
        let building = &output.prototypes[instance.prototype_index].building.recipe;
        let pose = PlacementPose {
            origin: instance.position,
            yaw: instance.yaw,
        };
        let length = sites[instance.settlement_index].settings.approach_length;
        placement::door_clearances(building, pose, length)
    });
    let departures = output.road_exits.iter().map(|exit| {
        let start = Vec2::new(exit.position[0], exit.position[1]);
        let end = start + Vec2::from_array(exit.direction) * 4.;
        roads::corridor(start, end, exit.half_width.max(0.9), 0.)
    });
    entrances.chain(departures).collect()
}

fn nearby<'a>(
    bounds: &ClearanceRect,
    candidates: impl Iterator<Item = &'a ClearanceRect>,
) -> Vec<&'a ClearanceRect> {
    candidates
        .filter(|candidate| placement::overlaps(bounds, candidate, 0.))
        .collect()
}

fn blocked(cell: &ClearanceRect, blockers: &[&ClearanceRect]) -> bool {
    blockers
        .iter()
        .any(|other| placement::overlaps(cell, other, 0.))
}

struct RiverSegment {
    start: [f32; 2],
    end: [f32; 2],
    radius: f32,
}

impl RiverSegment {
    fn intersects(&self, rect: &ClearanceRect) -> bool {
        placement::river_intersects(rect, self.start, self.end, self.radius)
    }
}

fn rivers(bounds: &ClearanceRect, input: &TownInput, margin: f32) -> Vec<RiverSegment> {
    input
        .rivers
        .iter()
        .flat_map(|river| {
            river
                .points
                .windows(2)
                .enumerate()
                .map(move |(i, pair)| RiverSegment {
                    start: pair[0],
                    end: pair[1],
                    radius: river.width(i).max(river.width(i + 1)) * 0.5 + margin,
                })
        })
        .filter(|river| river.intersects(bounds))
        .collect()
}

fn suitable(cell: &ClearanceRect, input: &TownInput, settings: &TownSettings) -> bool {
    let sample = |x, y| input.terrain.sample(x, y);
    let minimum = input.sea_level + settings.min_dry_height;
    let max_grade = settings
        .max_slope_degrees
        .min(MAX_SLOPE_DEGREES)
        .to_radians()
        .tan();
    let step = (input.terrain.spacing * 0.5).clamp(0.25, 1.);
    [-1., 0., 1.].into_iter().all(|y| {
        [-1., 0., 1.].into_iter().all(|x| {
            let [x, y] = cell.world([x * CELL * 0.5, y * CELL * 0.5]);
            let [Some(height), Some(left), Some(right), Some(down), Some(up)] = [
                sample(x, y),
                sample(x - step, y),
                sample(x + step, y),
                sample(x, y - step),
                sample(x, y + step),
            ] else {
                return false;
            };
            height > minimum && (right - left).hypot(up - down) <= max_grade * 2. * step
        })
    })
}

#[derive(Clone, Copy)]
struct Cells {
    left: u16,
    right: u16,
    top: u16,
    bottom: u16,
}

impl Cells {
    fn area(self) -> f32 {
        // Rank by the planted area after deducting the headlands.
        let length = f32::from(self.right - self.left) * CELL - 2. * END_HEADLAND;
        let width = f32::from(self.bottom - self.top) * CELL - 2. * SIDE_HEADLAND;
        length * width
    }
}

#[derive(Clone, Copy)]
struct Grid {
    origin: Vec2,
    direction: Vec2,
    side: f32,
    width: u16,
    depth: u16,
}

impl Grid {
    fn bounds(self) -> Cells {
        Cells {
            left: 0,
            right: self.width,
            top: 0,
            bottom: self.depth,
        }
    }

    fn reserve(self, cells: Cells) -> ClearanceRect {
        let along = f32::from(cells.left + cells.right) * CELL * 0.5;
        let across = f32::from(cells.top + cells.bottom) * CELL * 0.5;
        ClearanceRect {
            centre: (self.origin
                + self.direction * along
                + self.direction.perp() * across * self.side)
                .to_array(),
            half_extents: [
                f32::from(cells.right - cells.left) * CELL * 0.5,
                f32::from(cells.bottom - cells.top) * CELL * 0.5,
            ],
            yaw: self.direction.to_angle(),
        }
    }
}

// The accepted street and ring geometry bound both dimensions to positive u16 values.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn grids(row: roads::Row, shoulder: f32, depth: f32) -> impl Iterator<Item = Grid> {
    let span = row.end - row.start;
    let length = (span.length() / CELL).floor() as u16;
    let depth = (depth / CELL).floor() as u16;
    let blocks = length.div_ceil(MAX_ALONG);
    let direction = span.normalize_or_zero();
    // Allow for the extra quarter metre used by street decoration clearance.
    let setback = row.half_width + shoulder + 0.35;
    (0..blocks).flat_map(move |block| {
        let width = length / blocks;
        let spare = length % blocks;
        let start = block * width + block.min(spare);
        let width = width + u16::from(block < spare);
        [-1., 1.]
            .into_iter()
            .filter(move |_| width >= MIN_ALONG && depth >= MIN_ACROSS)
            .map(move |side| Grid {
                origin: row.start
                    + direction * (f32::from(start) * CELL)
                    + direction.perp() * setback * side,
                direction,
                side,
                width,
                depth,
            })
    })
}

struct Frontage {
    grid: Grid,
    // A bit per two-metre slot, with up to MAX_ALONG slots in each row.
    vacant: Vec<u32>,
}

impl Frontage {
    fn new(grid: Grid) -> Self {
        Self {
            grid,
            vacant: vec![(1 << grid.width) - 1; usize::from(grid.depth)],
        }
    }

    fn exclude(&mut self, blocked: impl Fn(ClearanceRect) -> bool) {
        for top in 0..self.grid.depth {
            let row = &mut self.vacant[usize::from(top)];
            for left in 0..self.grid.width {
                let bit = 1 << left;
                let cell = Cells {
                    left,
                    right: left + 1,
                    top,
                    bottom: top + 1,
                };
                if *row & bit != 0 && blocked(self.grid.reserve(cell)) {
                    *row &= !bit;
                }
            }
        }
    }

    fn take(&mut self, cells: Cells) {
        let mask = ((1 << (cells.right - cells.left)) - 1) << cells.left;
        for row in &mut self.vacant[usize::from(cells.top)..usize::from(cells.bottom)] {
            *row &= !mask;
        }
    }

    fn largest(&self) -> Option<Cells> {
        let mut best = None::<Cells>;
        for top in 0..self.grid.depth {
            let mut contiguous = u32::MAX;
            for bottom in top + 1..=self.grid.depth {
                contiguous &= self.vacant[usize::from(bottom - 1)];
                if contiguous == 0 {
                    break;
                }
                if bottom - top < MIN_ACROSS {
                    continue;
                }
                for (left, right) in runs(contiguous) {
                    let cells = Cells {
                        left,
                        right,
                        top,
                        bottom,
                    };
                    if right - left >= MIN_ALONG
                        && best.is_none_or(|best| cells.area() > best.area())
                    {
                        best = Some(cells);
                    }
                }
            }
        }
        best
    }
}

fn runs(mut vacant: u32) -> impl Iterator<Item = (u16, u16)> {
    std::iter::from_fn(move || {
        if vacant == 0 {
            return None;
        }
        let left = vacant.trailing_zeros();
        let width = (vacant >> left).trailing_ones();
        vacant &= !(((1 << width) - 1) << left);
        Some((
            u16::try_from(left).unwrap(),
            u16::try_from(left + width).unwrap(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::town::{HeightGrid, RiverPolyline, TownRoadSettings};

    #[test]
    fn full_plots_keep_the_reserved_headlands_away_from_water_slopes_and_protected_ground() {
        let row = roads::Row {
            start: Vec2::new(-32., 0.),
            end: Vec2::new(32., 0.),
            half_width: 2.,
        };
        let settings = TownSettings {
            roads: Some(TownRoadSettings::default()),
            ..TownSettings::default()
        };
        let sites = [Site {
            settings: &settings,
            rows: vec![row],
        }];
        let heights = (-40_i16..=40)
            .flat_map(|y| {
                (-40_i16..=40).map(move |x| {
                    if y < 0 {
                        -2.
                    } else {
                        4. + f32::from((x - 22).max(0)) * 0.5
                    }
                })
            })
            .collect();
        let input = TownInput {
            tile_seed: 42,
            sea_level: 0.,
            terrain: HeightGrid {
                origin: [-40., -40.],
                spacing: 1.,
                width: 81,
                height: 81,
                heights,
            },
            rivers: vec![RiverPolyline {
                points: vec![[0., 40.], [0., -40.]],
                widths: vec![4., 4.],
                join: None,
            }],
            protected_terrain: vec![motu::TownProtectedTerrain {
                min: [-24., 4.],
                max: [-22., 32.],
            }],
            bridge_exclusions: Vec::new(),
            disable_bridges: false,
        };
        let output = TownOutput::default();
        let fields = generate(&output, &sites, &input, &|| false).unwrap();
        assert_eq!(
            fields,
            generate(&output, &sites, &input, &|| false).unwrap()
        );
        assert!(fields.iter().any(|field| field.centre[0] < 0.));
        assert!(fields.iter().any(|field| field.centre[0] > 0.));
        let pavement_edge = row.half_width + settings.roads.unwrap().shoulder_width();
        for field in &fields {
            assert!(field.yaw.abs() < 0.001);
            assert!(field.centre[1] - field.half_extents[1] > pavement_edge + SIDE_HEADLAND);
            assert!(field.centre[0] + field.half_extents[0] < 22.);
            assert!(!placement::protected(field, &input));
            assert!(!placement::touches_river(
                field,
                &input,
                settings.river_margin
            ));
        }
    }
}
