//! Fits a capital pier and navigable berth to the resident coastal terrain.

mod mesh;
#[cfg(test)]
mod planning_tests;

pub use mesh::{PierMeshes, PierPropPoint, build_pier_meshes};

use crate::{Vec2, Vec3};

const SEARCH_RADIUS: f32 = 220.;
const SHORE_STEP: f32 = 2.;
const DEPTH_STEP: f32 = 1.;
const DECK_WIDTH: f32 = 4.5;
const DECK_FREEBOARD: f32 = 1.2;
const BERTH_LENGTH: f32 = 42.;
const BERTH_WIDTH: f32 = 16.;
const MIN_DEPTH: f32 = 5.;
const FENDER_GAP: f32 = 1.;
const TURN_RADIUS: f32 = 22.;
const MAX_PIER_LENGTH: f32 = 220.;
const MAX_LANDING_GRADE: f32 = 0.18;

/// A surveyed berth in island-local metres. Positions use mean sea level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PierBerth {
    /// Centre of the ship footprint alongside the fixed deck.
    pub position: Vec3,
    /// Centre of the surveyed turning area offshore of the berth.
    pub approach: Vec3,
    /// The bow faces inland along the straight arrival course.
    pub heading: Vec2,
    pub min_depth: f32,
    pub length: f32,
    pub width: f32,
}

/// A fixed pier and collinear land ramp in XY metres with Z-up elevations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PierLayout {
    pub shore: Vec2,
    pub start: Vec3,
    pub end: Vec2,
    pub axis: Vec2,
    pub deck_height: f32,
    pub width: f32,
    /// The berth lies on this sign of the deck axis's left-hand normal.
    pub berth_side: f32,
    pub berth: PierBerth,
}

/// Searches within 220 metres of the capital, where open ocean lies toward negative Y.
/// Missing terrain soundings reject the affected site or approach.
#[must_use]
pub fn plan_pier(
    capital: Vec2,
    sea_level: f32,
    bounds: [Vec2; 2],
    sample: &impl Fn(Vec2) -> Option<f32>,
    shoreline_allowed: &impl Fn(Vec2) -> bool,
) -> Option<PierLayout> {
    let [min, max] = bounds;
    if !sea_level.is_finite()
        || !min.is_finite()
        || !max.is_finite()
        || !min.cmplt(max).all()
        || !inside(capital, bounds)
    {
        return None;
    }
    let survey = Survey {
        sea_level,
        bounds,
        sample,
    };
    let search_min = (capital - SEARCH_RADIUS).max(min);
    let search_max = (capital + SEARCH_RADIUS).min(max);

    // The small collection is sorted so a nearer usable landing always wins
    // over a shorter structure built farther down the coast.
    let mut shores: Vec<_> = std::iter::once(capital.x)
        .chain(stations(search_min.x, search_max.x, SHORE_STEP))
        .filter_map(|x| survey.shore(x, search_min.y, search_max.y))
        .filter(|shore| capital.distance_squared(*shore) <= SEARCH_RADIUS * SEARCH_RADIUS)
        .filter(|shore| shoreline_allowed(*shore))
        .collect();
    shores.sort_unstable_by(|a, b| {
        capital
            .distance_squared(*a)
            .total_cmp(&capital.distance_squared(*b))
            .then_with(|| (a.x - capital.x).abs().total_cmp(&(b.x - capital.x).abs()))
            .then_with(|| a.x.total_cmp(&b.x))
    });
    shores.dedup();
    shores
        .into_iter()
        .find_map(|shore| survey.fit_site(shore, shoreline_allowed))
}

struct Survey<'a, F> {
    sea_level: f32,
    bounds: [Vec2; 2],
    sample: &'a F,
}

impl<F: Fn(Vec2) -> Option<f32>> Survey<'_, F> {
    fn height(&self, point: Vec2) -> Option<f32> {
        if !inside(point, self.bounds) {
            return None;
        }
        (self.sample)(point).filter(|height| height.is_finite())
    }

    fn shore(&self, x: f32, lower_y: f32, upper_y: f32) -> Option<Vec2> {
        let mut wet = lower_y;
        if self.height(Vec2::new(x, wet))? >= self.sea_level {
            return None;
        }
        for y in stations(lower_y, upper_y, SHORE_STEP).skip(1) {
            if self.height(Vec2::new(x, y))? >= self.sea_level {
                let mut dry = y;
                for _ in 0..7 {
                    let middle = wet.midpoint(dry);
                    if self.height(Vec2::new(x, middle))? < self.sea_level {
                        wet = middle;
                    } else {
                        dry = middle;
                    }
                }
                return Some(Vec2::new(x, wet));
            }
            wet = y;
        }
        None
    }

    fn fit_site(
        &self,
        shore: Vec2,
        shoreline_allowed: &impl Fn(Vec2) -> bool,
    ) -> Option<PierLayout> {
        let axes = [0_f32, 12., -12., 24., -24., 36., -36.].map(|angle| {
            let (x, y) = angle.to_radians().sin_cos();
            Vec2::new(x, -y)
        });
        let mut best: Option<(f32, PierLayout)> = None;
        for axis in axes {
            let limit = best.map_or(MAX_PIER_LENGTH, |(length, _)| length - DEPTH_STEP);
            if limit < BERTH_LENGTH {
                break;
            }
            let Some(start) = self.landing(shore, axis, shoreline_allowed) else {
                continue;
            };
            let Some(water_length) = self.unobstructed_deck(shore, axis, limit) else {
                continue;
            };
            for side in [1., -1.] {
                let limit = best.map_or(water_length, |(length, _)| {
                    water_length.min(length - DEPTH_STEP)
                });
                let lateral =
                    axis.perp() * (side * (DECK_WIDTH * 0.5 + FENDER_GAP + BERTH_WIDTH * 0.5));
                if let Some((length, berth)) = self.fit_berth(shore + lateral, axis, limit) {
                    best = Some((
                        length,
                        PierLayout {
                            shore,
                            start,
                            end: shore + axis * length,
                            axis,
                            deck_height: self.sea_level + DECK_FREEBOARD,
                            width: DECK_WIDTH,
                            berth_side: side,
                            berth,
                        },
                    ));
                }
            }
        }
        best.map(|(_, layout)| layout)
    }

    fn landing(
        &self,
        shore: Vec2,
        axis: Vec2,
        shoreline_allowed: &impl Fn(Vec2) -> bool,
    ) -> Option<Vec3> {
        let deck = self.sea_level + DECK_FREEBOARD;
        let across = axis.perp() * (DECK_WIDTH * 0.5);
        (6_u16..=18).map(f32::from).find_map(|length| {
            let start = shore - axis * length;
            let height = self.height(start)?;
            if height < self.sea_level + 0.25
                || (height - deck).abs() > length * MAX_LANDING_GRADE
                || ![-across, across].into_iter().all(|offset| {
                    self.height(start + offset)
                        .is_some_and(|edge| (edge - height).abs() <= 0.25)
                })
            {
                return None;
            }
            let ramp_clear = stations(0., length, 0.75).all(|distance| {
                let point = shore - axis * distance;
                let ramp = deck + (height - deck) * distance / length;
                shoreline_allowed(point)
                    && [-across, Vec2::ZERO, across].into_iter().all(|offset| {
                        self.height(point + offset)
                            .is_some_and(|ground| ground <= ramp + 0.08)
                    })
            });
            ramp_clear.then_some(start.extend(height))
        })
    }

    fn unobstructed_deck(&self, shore: Vec2, axis: Vec2, limit: f32) -> Option<f32> {
        let across = axis.perp() * (DECK_WIDTH * 0.5);
        let mut clear = 0.;
        for distance in stations(0., limit, DEPTH_STEP) {
            let point = shore + axis * distance;
            // A diagonal deck can still cross the upper beach beside its quay.
            let ceiling = self.sea_level
                + if distance < 5. {
                    DECK_FREEBOARD - 0.2
                } else {
                    0.
                };
            if ![-across, Vec2::ZERO, across].into_iter().all(|offset| {
                self.height(point + offset)
                    .is_some_and(|ground| ground <= ceiling)
            }) {
                break;
            }
            clear = distance;
        }
        (clear >= BERTH_LENGTH).then_some(clear)
    }

    fn fit_berth(&self, origin: Vec2, axis: Vec2, limit: f32) -> Option<(f32, PierBerth)> {
        if limit < BERTH_LENGTH {
            return None;
        }
        let across = axis.perp();
        let mut deep_since = None;
        for distance in stations(0., limit, DEPTH_STEP) {
            let row = origin + axis * distance;
            let Some(_) = self.deep_row(row, across, BERTH_WIDTH * 0.5) else {
                deep_since = None;
                continue;
            };
            let first_deep = *deep_since.get_or_insert(distance);
            if distance - first_deep < BERTH_LENGTH {
                continue;
            }
            let position = origin + axis * (distance - BERTH_LENGTH * 0.5);
            let approach = row + axis * (TURN_RADIUS + 3.);
            if !self.clear_approach(row, approach, axis) {
                continue;
            }
            let min_depth = stations(-BERTH_LENGTH * 0.5, BERTH_LENGTH * 0.5, DEPTH_STEP)
                .try_fold(f32::INFINITY, |shallowest, station| {
                    let depth =
                        self.deep_row(position + axis * station, across, BERTH_WIDTH * 0.5)?;
                    Some(shallowest.min(depth))
                })?;
            return Some((
                distance,
                PierBerth {
                    position: position.extend(self.sea_level),
                    approach: approach.extend(self.sea_level),
                    heading: -axis,
                    min_depth,
                    length: BERTH_LENGTH,
                    width: BERTH_WIDTH,
                },
            ));
        }
        None
    }

    fn deep_row(&self, centre: Vec2, across: Vec2, half_width: f32) -> Option<f32> {
        stations(-half_width, half_width, DEPTH_STEP).try_fold(f32::INFINITY, |lowest, lateral| {
            let depth = self.sea_level - self.height(centre + across * lateral)?;
            (depth >= MIN_DEPTH).then_some(lowest.min(depth))
        })
    }

    fn clear_approach(&self, berth_end: Vec2, turn: Vec2, axis: Vec2) -> bool {
        let across = axis.perp();
        let channel = stations(0., TURN_RADIUS + 3., DEPTH_STEP).all(|distance| {
            self.deep_row(berth_end + axis * distance, across, BERTH_WIDTH * 0.5)
                .is_some()
        });
        let turn_clear = || {
            stations(-TURN_RADIUS, TURN_RADIUS, DEPTH_STEP).all(|along| {
                let half_width = (TURN_RADIUS * TURN_RADIUS - along * along).sqrt();
                self.deep_row(turn + axis * along, across, half_width)
                    .is_some()
            })
        };
        // Continue to the open edge of this coastal section. This rejects a
        // deep patch of water sealed behind a reef or headland.
        let open_ocean = || {
            stations(self.bounds[0].y, turn.y, DEPTH_STEP).all(|y| {
                self.deep_row(Vec2::new(turn.x, y), Vec2::X, BERTH_WIDTH * 0.5)
                    .is_some()
            })
        };
        channel && turn_clear() && open_ocean()
    }
}

fn inside(point: Vec2, [min, max]: [Vec2; 2]) -> bool {
    point.cmpge(min).all() && point.cmple(max).all()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn stations(start: f32, end: f32, max_step: f32) -> impl Iterator<Item = f32> {
    let steps = ((end - start) / max_step).ceil() as u16;
    // Keep the interior soundings fixed when the surveyed reach is shortened.
    (0..=steps).map(move |step| f32::from(step).mul_add(max_step, start).min(end))
}
