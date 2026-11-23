//! Plan enclosed fields against the final terrain and cart-track centrelines.
mod geometry;
mod route;
mod surface;

use crate::{DecorationClearance, ISLAND_WORLD_METRES, Vec2, Vec3, forest::TreePlacement};
use serde::Serialize;
use std::collections::HashSet;

use super::{Island, cart_tracks::CartTrackRun, spatial_bins::SpatialBins};
use geometry::{bounds, contains_convex, convex, overlaps, point_segment_distance_squared};
use route::{Route, TrackNetwork};
use surface::Surface;

const FRONT_SETBACK: f32 = 5.25;
const END_SETBACK: f32 = 16.;
const STATION_SPACING: f32 = 6.;
const SEARCH_STEP: f32 = 6.;
const MIN_FRONTAGE: f32 = 25.;
const MAX_FRONTAGE: f32 = 60.;
const MIN_DEPTH: f32 = 20.;
const MAX_FIELDS: usize = 512;
const TREE_WALL_SETBACK: f32 = 1.5;
const TREE_WALL_CLEARANCE: f32 = 1.35;
const TREE_SPACING: f32 = 3.;
const TRUNK_WHEAT_MARGIN: f32 = 0.38;
const MIN_TRUNK_WHEAT_RADIUS: f32 = 0.72;
const BUSH_RADIUS_PER_SCALE: f32 = 0.78;
const BUSH_WHEAT_MARGIN: f32 = 0.27;

#[derive(Debug, Default, PartialEq, Serialize)]
pub(crate) struct RoadsideFieldPlan {
    fields: Vec<Field>,
    walls: Vec<Wall>,
    gates: Vec<Gate>,
    windbreaks: Vec<Windbreak>,
    bushes: Vec<WindbreakBush>,
}

#[derive(Debug, PartialEq, Serialize)]
struct Field {
    id: u32,
    outline: Vec<f32>,
    wheat: bool,
}

#[derive(Debug, PartialEq, Serialize)]
struct Wall {
    points: Vec<f32>,
}

#[derive(Debug, PartialEq, Serialize)]
struct Gate {
    field_id: u32,
    left: [f32; 3],
    right: [f32; 3],
}

#[derive(Debug, PartialEq, Serialize)]
struct Windbreak {
    position: [f32; 3],
    yaw: f32,
    scale: f32,
    clearance_radius: f32,
    #[serde(skip)]
    bushes: [Option<WindbreakBush>; 2],
}

#[derive(Debug, PartialEq, Serialize)]
struct WindbreakBush {
    position: [f32; 3],
    yaw: f32,
    scale: f32,
    clearance_radius: f32,
}

impl Island {
    pub(crate) fn prepare_roadside_fields(
        &mut self,
        blockers: &[DecorationClearance],
        wind_direction: [f32; 2],
        seed: u64,
    ) -> Result<RoadsideFieldPlan, String> {
        let mut plan = self.plan_roadside_fields(
            self.terrain.cart_track_runs(),
            blockers,
            wind_direction,
            seed,
        );
        self.plant_roadside_windbreaks(&mut plan, seed)?;
        Ok(plan)
    }

    fn plant_roadside_windbreaks(
        &mut self,
        plan: &mut RoadsideFieldPlan,
        seed: u64,
    ) -> Result<(), String> {
        plan.bushes.clear();
        if self.forest.hybrid.is_none() {
            plan.windbreaks.clear();
        }
        if plan.windbreaks.is_empty() {
            return Ok(());
        }
        let mut tree_vertices: HashSet<_> = self
            .forest
            .placements()
            .iter()
            .map(|tree| tree.terrain_vertex)
            .collect();
        let ordinal = self
            .forest
            .placements()
            .iter()
            .map(|tree| tree.seed_ordinal)
            .max()
            .unwrap_or_default()
            + 1;
        let mut plantings = Vec::with_capacity(plan.windbreaks.len());
        let mesh = self.terrain.mesh();
        plan.windbreaks.retain(|spot| {
            let anchor = Vec3::from_array(spot.position) / ISLAND_WORLD_METRES;
            let index = self
                .terrain
                .triangle_index
                .nearest_vertex(mesh, anchor.truncate());
            let Ok(terrain_vertex) = u32::try_from(index) else {
                return false;
            };
            if !tree_vertices.insert(terrain_vertex) {
                return false;
            }
            let seed_ordinal =
                ordinal + u32::try_from(plantings.len()).expect("bounded roadside fields");
            plantings.push(TreePlacement {
                seed_ordinal,
                terrain_vertex,
                anchor,
                yaw_radians: spot.yaw,
                scale: spot.scale,
                prototype: (mix(seed ^ u64::from(terrain_vertex)) & 7) as u8,
            });
            true
        });
        let before = self.forest.placements().len();
        crate::forest::hybrid::extend_plantings(
            seed,
            &mut self.forest,
            &self.terrain,
            self.forest_options,
            plantings,
        )?;
        for (tree, trunk) in plan
            .windbreaks
            .iter_mut()
            .zip(self.forest.trunk_colliders().skip(before))
        {
            if let Ok(trunk) = trunk {
                let offset = horizontal_position(tree.position)
                    .distance(trunk.bottom.truncate() * ISLAND_WORLD_METRES);
                let footprint_radius = trunk.radius * ISLAND_WORLD_METRES + offset;
                tree.clearance_radius =
                    (footprint_radius + TRUNK_WHEAT_MARGIN).max(MIN_TRUNK_WHEAT_RADIUS);
            }
        }
        for tree in &mut plan.windbreaks {
            plan.bushes
                .extend(std::mem::take(&mut tree.bushes).into_iter().flatten());
        }
        self.forest_stats.accepted_trees = self.forest.placements().len();
        if let Some(decorations) = self.decorations.get_mut() {
            decorations.set_tree_anchors(self.forest.placements().iter().map(|tree| tree.anchor));
        }
        Ok(())
    }

    pub(crate) fn plan_roadside_fields(
        &self,
        runs: &[CartTrackRun],
        blockers: &[DecorationClearance],
        wind_direction: [f32; 2],
        seed: u64,
    ) -> RoadsideFieldPlan {
        let network = TrackNetwork::new(runs);
        if network.routes.is_empty() {
            return RoadsideFieldPlan::default();
        }
        let mut planner = Planner {
            network: &network,
            surface: Surface::new(self, blockers),
            wind: Vec2::from_array(wind_direction).normalize_or_zero(),
            occupied: SpatialBins::new(),
            planted: SpatialBins::new(),
            plan: RoadsideFieldPlan::default(),
        };
        for (route_index, route) in network.routes.iter().enumerate() {
            for side in [1., -1.] {
                planner.fit_side(route, route_index, side, seed);
            }
        }
        planner.plan
    }
}

struct Planner<'a> {
    network: &'a TrackNetwork,
    surface: Surface<'a>,
    wind: Vec2,
    occupied: SpatialBins<[Vec2; 4]>,
    planted: SpatialBins<Vec2>,
    plan: RoadsideFieldPlan,
}

impl Planner<'_> {
    fn fit_side(&mut self, route: &Route, route_index: usize, side: f32, seed: u64) {
        let end_limit = route.length() - END_SETBACK;
        let mut start = END_SETBACK;
        let mut previous_depth = 0.;
        let seed = mix(seed ^ route.key ^ u64::from(side.to_bits()));
        while end_limit - start >= MIN_FRONTAGE && self.plan.fields.len() < MAX_FIELDS {
            let key = mix(seed ^ u64::from(start.to_bits()));
            let remaining = end_limit - start;
            if let Some(parcel) = self.fit_parcel(
                route,
                route_index,
                side,
                start,
                remaining,
                key,
                previous_depth,
            ) {
                start = parcel.end;
                previous_depth = parcel.depth;
                self.add_parcel(parcel);
            } else {
                start += SEARCH_STEP;
                previous_depth = 0.;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fit_parcel(
        &self,
        route: &Route,
        route_index: usize,
        side: f32,
        start: f32,
        remaining: f32,
        key: u64,
        previous_depth: f32,
    ) -> Option<Parcel> {
        let frontage = (33. + 23. * unit(key)).min(remaining);
        let depth = 27. + 23. * unit(key ^ 1);
        let sizes = [
            (frontage, depth),
            (frontage, (depth - 8.).max(MIN_DEPTH)),
            (frontage, MIN_DEPTH),
            ((frontage - 9.).max(MIN_FRONTAGE).min(remaining), depth),
            ((frontage - 9.).max(MIN_FRONTAGE).min(remaining), MIN_DEPTH),
            (27.0_f32.min(remaining), MIN_DEPTH),
        ];
        sizes
            .into_iter()
            .enumerate()
            .find_map(|(i, (width, depth))| {
                if sizes[..i].contains(&(width, depth)) {
                    return None;
                }
                let end = start + width;
                if self.network.near_junction(route_index, start, end) {
                    return None;
                }
                let layout = Layout::new(route, side, start, end, depth)?;
                let mut gentle = true;
                for (i, quad) in layout.quads.iter().enumerate() {
                    let [min, max] = bounds(quad);
                    if self.network.track_overlaps(quad, min, max)
                        || self
                            .occupied
                            .nearby(min, max)
                            .any(|existing| overlaps(quad, existing))
                        || layout.quads[..i.saturating_sub(1)]
                            .iter()
                            .any(|previous| overlaps(quad, previous))
                    {
                        return None;
                    }
                    gentle &= self.surface.suitable(quad)?;
                }
                let wheat = gentle && unit(key ^ 2) < 0.68;
                let output = self.make_enclosure(&layout, previous_depth, key, wheat)?;
                Some(Parcel {
                    end,
                    depth,
                    layout,
                    output,
                })
            })
    }

    fn make_enclosure(
        &self,
        layout: &Layout,
        previous_depth: f32,
        key: u64,
        wheat: bool,
    ) -> Option<Enclosure> {
        let id = self.plan.fields.len() as u32 + 1;
        let opening = layout.gate(&self.surface, key)?;
        let left = self.surface.position(opening.left)?;
        let right = self.surface.position(opening.right)?;
        let mut walls = Vec::with_capacity(5);
        walls.push(
            self.wall(
                layout.front[..=opening.left_segment]
                    .iter()
                    .copied()
                    .chain([opening.left]),
            )?,
        );
        walls.push(
            self.wall(
                [opening.right]
                    .into_iter()
                    .chain(layout.front[opening.right_segment + 1..].iter().copied()),
            )?,
        );
        walls.push(self.wall(layout.rear.iter().copied())?);
        let last = layout.front.len() - 1;
        walls.push(self.wall([layout.front[last], layout.rear[0]])?);
        // The previous parcel already installed the coincident part of the divider.
        let depth = layout.front[0].distance(layout.rear[last]);
        if depth > previous_depth + 0.01 {
            let first = layout.front[0].lerp(layout.rear[last], previous_depth / depth);
            walls.push(self.wall([first, layout.rear[last]])?);
        }
        let outline = layout
            .front
            .iter()
            .chain(&layout.rear)
            .map(|&point| self.surface.position(point))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        let windbreaks = layout.windbreak(self.wind, key, &self.surface, &self.planted);
        Some(Enclosure {
            field: Field { id, outline, wheat },
            walls,
            gate: Gate {
                field_id: id,
                left,
                right,
            },
            windbreaks,
        })
    }

    fn wall(&self, points: impl IntoIterator<Item = Vec2>) -> Option<Wall> {
        let mut points = points.into_iter();
        let mut first = points.next()?;
        let mut positions = self.surface.position(first)?.to_vec();
        for last in points {
            let distance = first.distance(last);
            if distance < 0.005 {
                continue;
            }
            let steps = (distance / 1.75).ceil() as u32;
            for step in 1..steps {
                let point = first.lerp(last, step as f32 / steps as f32);
                positions.extend(self.surface.position(point)?);
            }
            positions.extend(self.surface.position(last)?);
            first = last;
        }
        Some(Wall { points: positions })
    }

    fn add_parcel(&mut self, parcel: Parcel) {
        for quad in parcel.layout.quads {
            let [min, max] = bounds(&quad);
            self.occupied.insert(min, max, quad);
        }
        for plant in &parcel.output.windbreaks {
            let point = horizontal_position(plant.position);
            self.planted.insert(point, point, point);
        }
        self.plan.fields.push(parcel.output.field);
        self.plan.walls.extend(parcel.output.walls);
        self.plan.gates.push(parcel.output.gate);
        self.plan.windbreaks.extend(parcel.output.windbreaks);
    }
}

struct Parcel {
    end: f32,
    depth: f32,
    layout: Layout,
    output: Enclosure,
}

struct Enclosure {
    field: Field,
    walls: Vec<Wall>,
    gate: Gate,
    windbreaks: Vec<Windbreak>,
}

struct Layout {
    front: Vec<Vec2>,
    rear: Vec<Vec2>,
    road: Vec<Vec2>,
    frontage: f32,
    quads: Vec<[Vec2; 4]>,
    interior_side: f32,
}

struct Opening {
    left: Vec2,
    right: Vec2,
    left_segment: usize,
    right_segment: usize,
}

impl Layout {
    fn new(route: &Route, side: f32, start: f32, end: f32, depth: f32) -> Option<Self> {
        let count = ((end - start) / STATION_SPACING).ceil() as u32;
        let mut front = Vec::with_capacity(count as usize + 1);
        let mut rear = Vec::with_capacity(front.capacity());
        let mut road = Vec::with_capacity(front.capacity());
        for station in 0..=count {
            let distance = if station == count {
                end
            } else {
                start + (end - start) * station as f32 / count as f32
            };
            let point = route.point(distance);
            let left = (route.point(distance + 3.) - route.point(distance - 3.))
                .try_normalize()?
                .perp();
            front.push(point + left * side * FRONT_SETBACK);
            rear.push(point + left * side * (FRONT_SETBACK + depth));
            road.push(point);
        }
        let frontage = polyline_length(&front);
        if !(MIN_FRONTAGE..=MAX_FRONTAGE).contains(&frontage) {
            return None;
        }
        let quads: Vec<_> = front
            .windows(2)
            .zip(rear.windows(2))
            .map(|(a, b)| [a[0], a[1], b[1], b[0]])
            .collect();
        rear.reverse();
        quads.iter().all(|quad| convex(quad, side)).then_some(Self {
            front,
            rear,
            road,
            frontage,
            quads,
            interior_side: side,
        })
    }

    fn gate(&self, surface: &Surface<'_>, key: u64) -> Option<Opening> {
        const HALF_GAP: f32 = 0.92;
        let middle = 0.42 + 0.16 * unit(key ^ 3);
        [middle, 0.35, 0.65, 0.24, 0.76]
            .into_iter()
            .find_map(|fraction| {
                let centre = self.frontage * fraction;
                let (midpoint, segment) = self.frontage_point(centre);
                let fraction = midpoint.distance(self.front[segment])
                    / self.front[segment].distance(self.front[segment + 1]);
                let road = self.road[segment].lerp(self.road[segment + 1], fraction);
                let (left, left_segment) = self.frontage_point(centre - HALF_GAP);
                let (right, right_segment) = self.frontage_point(centre + HALF_GAP);
                if left.distance(right) < 1.74 || !surface.usable_gate_approach(road, midpoint) {
                    return None;
                }
                Some(Opening {
                    left,
                    right,
                    left_segment,
                    right_segment,
                })
            })
    }

    fn frontage_point(&self, distance: f32) -> (Vec2, usize) {
        point_along(&self.front, distance)
    }

    fn windbreak(
        &self,
        wind: Vec2,
        key: u64,
        surface: &Surface<'_>,
        planted: &SpatialBins<Vec2>,
    ) -> Vec<Windbreak> {
        if wind == Vec2::ZERO {
            return Vec::new();
        }
        let last = self.front.len() - 1;
        let right = [self.front[last], self.rear[0]];
        let left = [self.rear[last], self.front[0]];
        // Frontage remains open around the working gate. Score each other
        // edge using its windward exposure and useful planting length.
        let edges = [self.rear.as_slice(), &right, &left];
        let score = |edge: &[Vec2]| {
            let chord = edge[edge.len() - 1] - edge[0];
            (chord.perp() * self.interior_side).dot(wind).max(0.)
        };
        let Some(edge) = edges
            .into_iter()
            .max_by(|a, b| score(a).total_cmp(&score(b)))
        else {
            return Vec::new();
        };
        let length = polyline_length(edge);
        if score(edge) < length * 0.28 {
            return Vec::new();
        }
        let step = 3.75 + 1.25 * unit(key ^ 4);
        let mut windbreaks = Vec::new();
        let mut previous_station = None;
        let mut along = 3.5 + 1.5 * unit(key ^ 5);
        while along < length - 3.5 {
            let point = self.windbreak_point(edge, along, TREE_WALL_SETBACK);
            let margin = Vec2::splat(TREE_SPACING);
            if self.contains_plant(point, TREE_WALL_CLEARANCE)
                && !surface.sheltered(point, wind)
                && !planted
                    .nearby(point - margin, point + margin)
                    .any(|other| other.distance_squared(point) < TREE_SPACING.powi(2))
                && !windbreaks.iter().any(|tree: &Windbreak| {
                    horizontal_position(tree.position).distance_squared(point)
                        < TREE_SPACING.powi(2)
                })
                && let Some(position) = surface.position(point)
            {
                let random = mix(key ^ u64::from(along.to_bits()));
                let mut bushes = [None, None];
                if let Some(previous) =
                    previous_station.filter(|previous| along - previous < step + 0.05)
                {
                    bushes = std::array::from_fn(|index| {
                        let key = mix(random ^ (index as u64 + 6));
                        let fraction = if index == 0 { 0.32 } else { 0.68 };
                        let station =
                            previous + (along - previous) * (fraction + 0.06 * (unit(key) - 0.5));
                        self.windbreak_bush(edge, station, key, surface)
                    });
                }
                windbreaks.push(Windbreak {
                    position,
                    yaw: unit(random) * std::f32::consts::TAU,
                    scale: 0.72 + 0.22 * unit(random ^ 1),
                    clearance_radius: MIN_TRUNK_WHEAT_RADIUS,
                    bushes,
                });
                previous_station = Some(along);
            }
            along += step;
        }
        windbreaks
    }

    fn windbreak_bush(
        &self,
        edge: &[Vec2],
        station: f32,
        key: u64,
        surface: &Surface<'_>,
    ) -> Option<WindbreakBush> {
        let scale = 1.45 + 0.30 * unit(key ^ 1);
        let radius = BUSH_RADIUS_PER_SCALE * scale;
        let setback = TREE_WALL_SETBACK + 0.45 + 0.25 * unit(key ^ 2);
        let point = self.windbreak_point(edge, station, setback);
        // Allow for the stonework along the wall centreline and at its corners.
        if !self.contains_plant(point, radius + 0.5) {
            return None;
        }
        Some(WindbreakBush {
            position: surface.position(point)?,
            yaw: unit(key ^ 3) * std::f32::consts::TAU,
            scale,
            clearance_radius: radius + BUSH_WHEAT_MARGIN,
        })
    }

    fn windbreak_point(&self, edge: &[Vec2], along: f32, setback: f32) -> Vec2 {
        let (point, segment) = point_along(edge, along);
        let tangent = (edge[segment + 1] - edge[segment]).normalize_or_zero();
        point + tangent.perp() * self.interior_side * setback
    }

    fn contains_plant(&self, point: Vec2, wall_clearance: f32) -> bool {
        let last = self.front.len() - 1;
        let sides = [
            [self.front[last], self.rear[0]],
            [self.rear[last], self.front[0]],
        ];
        self.quads.iter().any(|quad| contains_convex(quad, point))
            && self
                .front
                .windows(2)
                .chain(self.rear.windows(2))
                .chain(sides.iter().map(<[Vec2; 2]>::as_slice))
                .all(|edge| {
                    point_segment_distance_squared(point, edge[0], edge[1])
                        >= wall_clearance.powi(2)
                })
    }
}

fn horizontal_position(position: [f32; 3]) -> Vec2 {
    Vec3::from_array(position).truncate()
}

fn polyline_length(points: &[Vec2]) -> f32 {
    points
        .windows(2)
        .map(|segment| segment[0].distance(segment[1]))
        .sum()
}

fn point_along(points: &[Vec2], mut distance: f32) -> (Vec2, usize) {
    for (i, segment) in points.windows(2).enumerate() {
        let length = segment[0].distance(segment[1]);
        if distance <= length {
            return (segment[0].lerp(segment[1], distance / length), i);
        }
        distance -= length;
    }
    (points[points.len() - 1], points.len() - 2)
}

fn mix(mut key: u64) -> u64 {
    key = (key ^ (key >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    key = (key ^ (key >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    key ^ (key >> 31)
}

fn unit(key: u64) -> f32 {
    ((mix(key) >> 40) as u32) as f32 / 16_777_216.
}

#[cfg(test)]
mod tests;
