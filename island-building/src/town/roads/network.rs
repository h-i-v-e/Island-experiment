use super::{corridor, strip_ends};
use crate::{
    collision::BoxCollider,
    terrain_support::PlacementPose,
    town::{BridgeCandidate, TownError, TownInput, TownRoadSettings, TownSettings, placement},
};
use motu::{
    Vec2,
    roads::{MAX_ROAD_EDGES, RoadEdge, RoadGradeOptions, RoadNetwork, RoadStrip, grade_roads},
};
use std::{collections::HashMap, ops::Range};

const DIRECTIONS: [Vec2; 4] = [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y];
const EDGE_LENGTH: f32 = 8.;

pub(in crate::town) struct StreetLayout {
    pub roads: RoadNetwork,
    pub plaza: super::ClearanceRect,
    plan: Plan,
    components: Vec<Option<usize>>,
    edge_to_section: Vec<usize>,
    river_sections: Vec<bool>,
}

pub(in crate::town) struct Streets {
    pub network: RoadNetwork,
    pub bridges: Vec<BridgeCandidate>,
    pub ring_length: f32,
    pub ring_count: usize,
    pub plaza: super::ClearanceRect,
}

impl StreetLayout {
    pub fn inner_strips(&self) -> impl Iterator<Item = &RoadStrip> {
        self.roads.strips.iter().filter(|strip| {
            self.edge_to_section[strip.source] < self.plan.inner_section_count
                && self.components[strip.start].is_some()
        })
    }

    pub fn finish(mut self, root: usize, input: &TownInput, settings: &TownSettings) -> Streets {
        let selected = self.components[root];
        let on_street = |node| self.components[node] == selected;
        let bridges = bridge_candidates(
            &self.plan,
            &self.roads,
            &self.edge_to_section,
            &self.river_sections,
            &on_street,
            input,
            settings,
        );
        self.roads.retain_strips(|strip| on_street(strip.start));
        let ring_strips = self.roads.strips.iter().filter_map(|strip| {
            self.plan.sections[self.edge_to_section[strip.source]]
                .ring
                .map(|ring| (strip, ring))
        });
        let (ring_count, ring_length) =
            ring_strips.fold((0, 0.), |(count, length), (strip, ring)| {
                (
                    count.max(ring + 1),
                    length + self.plan.nodes[strip.start].distance(self.plan.nodes[strip.end]),
                )
            });
        Streets {
            network: self.roads,
            bridges,
            ring_length,
            ring_count,
            plaza: self.plaza,
        }
    }
}

#[derive(Clone, Copy)]
struct Section {
    edge: RoadEdge,
    ring: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RouteKind {
    Ring(usize),
    Cross,
    Exit,
}

struct Route {
    sections: Range<usize>,
    kind: RouteKind,
}

#[derive(Default)]
struct Plan {
    nodes: Vec<Vec2>,
    lookup: HashMap<[u32; 2], usize>,
    sections: Vec<Section>,
    routes: Vec<Route>,
    min_span: f32,
    inner_section_count: usize,
}

impl Plan {
    fn node(&mut self, point: Vec2) -> usize {
        let key = point
            .to_array()
            .map(|v| if v == 0. { 0 } else { v.to_bits() });
        *self.lookup.entry(key).or_insert_with(|| {
            self.nodes.push(point);
            self.nodes.len() - 1
        })
    }

    fn route<const N: usize>(&mut self, points: [Vec2; N], half_width: f32, kind: RouteKind) {
        let start = self.sections.len();
        for pair in points.windows(2) {
            let ring = match kind {
                RouteKind::Ring(index) => Some(index),
                _ => None,
            };
            self.sections(pair[0], pair[1], half_width, ring);
        }
        self.routes.push(Route {
            sections: start..self.sections.len(),
            kind,
        });
    }

    // Each ring is added in order and the plan is limited by the shared road edge budget.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn sections(&mut self, from: Vec2, to: Vec2, half_width: f32, ring: Option<usize>) {
        let length = (to - from).length();
        // A short strip between two junctions must reserve both seam offsets.
        let count = (length / EDGE_LENGTH)
            .ceil()
            .min((length / self.min_span).floor().max(1.)) as u16;
        let mut start = self.node(from);
        for step in 1..=count {
            let point = if step == count {
                to
            } else {
                from.lerp(to, f32::from(step) / f32::from(count))
            };
            let end = self.node(point);
            self.sections.push(Section {
                edge: RoadEdge {
                    start,
                    end,
                    half_width,
                },
                ring,
            });
            start = end;
        }
    }
}

fn plan(bounds: BoxCollider, roads: TownRoadSettings, ring_count: usize) -> Option<(Plan, f32)> {
    let width = roads.half_width.max(roads.access_half_width);
    let extent = [bounds.max[0], bounds.max[1], -bounds.min[0], -bounds.min[1]]
        .into_iter()
        .fold(0., f32::max);
    let inner = extent + roads.shoulder + roads.half_width.hypot(roads.access_half_width) + 2.;
    let mut plan = Plan {
        min_span: width * 2. + 0.5,
        ..Plan::default()
    };
    let mut radius = inner;
    let mut previous = None;
    for index in 0..ring_count {
        for direction in DIRECTIONS {
            let tangent = direction.perp();
            plan.route(
                [
                    (direction - tangent) * radius,
                    direction * radius,
                    (direction + tangent) * radius,
                ],
                roads.half_width,
                RouteKind::Ring(index),
            );
        }
        if let Some(prior) = previous {
            for direction in DIRECTIONS {
                plan.route(
                    [direction * prior, direction * radius],
                    roads.access_half_width,
                    RouteKind::Cross,
                );
            }
        } else {
            plan.inner_section_count = plan.sections.len();
        }
        if plan.sections.len() > MAX_ROAD_EDGES {
            return None;
        }
        previous = Some(radius);
        radius += roads.ring_spacing;
    }
    let last = previous?;
    let exit = last + roads.ring_spacing * 0.60;
    for direction in DIRECTIONS {
        plan.route(
            [direction * last, direction * exit],
            roads.access_half_width,
            RouteKind::Exit,
        );
    }
    (plan.sections.len() <= MAX_ROAD_EDGES).then_some((plan, inner))
}

pub(in crate::town) fn layout(
    bounds: BoxCollider,
    pose: PlacementPose,
    settings: &TownSettings,
    input: &TownInput,
    ring_count: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<StreetLayout>, TownError> {
    let Some(config) = settings.roads else {
        return Ok(None);
    };
    let Some((mut plan, inner)) = plan(bounds, config, ring_count) else {
        return Ok(None);
    };
    for point in &mut plan.nodes {
        let world = pose.world_point([point.x, point.y, 0.]);
        *point = Vec2::new(world[0], world[1]);
    }
    let mut eligible = vec![false; plan.sections.len()];
    let mut river_sections = vec![false; plan.sections.len()];
    let min_height = input.sea_level + settings.min_dry_height;
    for (index, section) in plan.sections.iter().enumerate() {
        if cancelled() {
            return Err(TownError::Cancelled);
        }
        let edge = section.edge;
        let (from, to) = (plan.nodes[edge.start], plan.nodes[edge.end]);
        let radius = config.half_width.max(config.access_half_width) * std::f32::consts::SQRT_2;
        let corridor = corridor(from, to, radius + config.shoulder, radius + config.shoulder);
        if placement::protected(&corridor, input) {
            continue;
        }
        if placement::touches_river(&corridor, input, settings.river_margin) {
            river_sections[index] = true;
        } else {
            eligible[index] = true;
        }
    }
    let (roads, edge_to_section) = grade_complete_access(
        &plan,
        eligible,
        RoadGradeOptions {
            min_height,
            max_grade: config.max_grade,
            max_cut: config.max_cut,
            shoulder: config.shoulder,
            sample_spacing: settings
                .sample_spacing
                .min(input.terrain.spacing * 0.5)
                .max(0.25),
        },
        &|point: Vec2| {
            (!cancelled())
                .then(|| input.terrain.sample(point.x, point.y))
                .flatten()
        },
        cancelled,
    )?;
    let roots = roads
        .strips
        .iter()
        .filter(|strip| edge_to_section[strip.source] < plan.inner_section_count)
        .map(|strip| strip.start);
    let components = connected_to_centre(&roads, &plan, roots);
    Ok(components
        .iter()
        .any(Option::is_some)
        .then_some(StreetLayout {
            roads,
            plaza: super::ClearanceRect {
                centre: [pose.origin[0], pose.origin[1]],
                half_extents: [inner - config.half_width; 2],
                yaw: pose.yaw,
            },
            plan,
            components,
            edge_to_section,
            river_sections,
        }))
}

fn grade_complete_access(
    plan: &Plan,
    mut eligible: Vec<bool>,
    options: RoadGradeOptions,
    terrain: &impl Fn(Vec2) -> Option<f32>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(RoadNetwork, Vec<usize>), TownError> {
    let mut surviving = eligible.clone();
    disable_incomplete_access(plan, &surviving, &mut eligible);
    loop {
        let edge_to_section: Vec<_> = eligible
            .iter()
            .enumerate()
            .filter_map(|(section, &enabled)| enabled.then_some(section))
            .collect();
        let edges: Vec<_> = edge_to_section
            .iter()
            .map(|&section| plan.sections[section].edge)
            .collect();
        let roads = grade_roads(&plan.nodes, &edges, options, terrain);
        if cancelled() {
            return Err(TownError::Cancelled);
        }
        surviving.fill(false);
        for strip in &roads.strips {
            surviving[edge_to_section[strip.source]] = true;
        }
        if !disable_incomplete_access(plan, &surviving, &mut eligible) {
            return Ok((roads, edge_to_section));
        }
        // A missing link changes junction width and therefore the allowed grade
        // of nearby streets. Retry eligible ring sections against the new junctions.
    }
}

fn disable_incomplete_access(plan: &Plan, surviving: &[bool], eligible: &mut [bool]) -> bool {
    let mut on_ring = vec![false; plan.nodes.len()];
    for (section, &kept) in plan.sections.iter().zip(surviving) {
        if kept && section.ring.is_some() {
            on_ring[section.edge.start] = true;
            on_ring[section.edge.end] = true;
        }
    }
    let mut changed = false;
    for route in plan
        .routes
        .iter()
        .filter(|route| !matches!(route.kind, RouteKind::Ring(_)))
    {
        let sections = route.sections.clone();
        let from = plan.sections[sections.start].edge.start;
        let to = plan.sections[sections.end - 1].edge.end;
        let complete = on_ring[from]
            && (route.kind == RouteKind::Exit || on_ring[to])
            && surviving[sections.clone()].iter().all(|&kept| kept);
        if !complete {
            let requested = &mut eligible[sections];
            changed |= requested.iter().any(|&enabled| enabled);
            requested.fill(false);
        }
    }
    changed
}

fn connected_to_centre(
    roads: &RoadNetwork,
    plan: &Plan,
    roots: impl Iterator<Item = usize>,
) -> Vec<Option<usize>> {
    let mut components = vec![None; plan.nodes.len()];
    let mut adjacent = vec![Vec::new(); plan.nodes.len()];
    for strip in &roads.strips {
        adjacent[strip.start].push(strip.end);
        adjacent[strip.end].push(strip.start);
    }
    let mut pending = Vec::new();
    for root in roots {
        if components[root].is_some() {
            continue;
        }
        components[root] = Some(root);
        pending.push(root);
        while let Some(node) = pending.pop() {
            for &next in &adjacent[node] {
                if components[next].is_none() {
                    components[next] = Some(root);
                    pending.push(next);
                }
            }
        }
    }
    let mut reach = vec![0.; plan.nodes.len()];
    for strip in &roads.strips {
        if let Some(root) = components[strip.start] {
            reach[root] += plan.nodes[strip.start].distance(plan.nodes[strip.end]);
        }
    }
    components
        .into_iter()
        .map(|root| root.filter(|&root| reach[root] >= 8. - 0.001))
        .collect()
}

fn bridge_candidates(
    plan: &Plan,
    roads: &RoadNetwork,
    edge_to_section: &[usize],
    river_sections: &[bool],
    connected: &impl Fn(usize) -> bool,
    input: &TownInput,
    settings: &TownSettings,
) -> Vec<BridgeCandidate> {
    let mut graded = vec![None; plan.sections.len()];
    for strip in &roads.strips {
        graded[edge_to_section[strip.source]] = Some(strip);
    }
    plan.routes
        .iter()
        .flat_map(|route| {
            let states = &river_sections[route.sections.clone()];
            let graded = &graded[route.sections.clone()];
            states
                .chunk_by(|a, b| a == b)
                .scan(0, |first, chunk| {
                    let start = *first;
                    *first += chunk.len();
                    Some((chunk[0], start, *first))
                })
                .filter_map(move |(river, start, after)| {
                    if !river || start == 0 || after == states.len() {
                        return None;
                    }
                    let from = graded[start - 1]?;
                    let to = graded[after]?;
                    if !connected(from.end) && !connected(to.start) {
                        return None;
                    }
                    plausible_bridge(from, to, input, settings)
                })
        })
        .collect()
}

fn plausible_bridge(
    from: &RoadStrip,
    to: &RoadStrip,
    input: &TownInput,
    settings: &TownSettings,
) -> Option<BridgeCandidate> {
    let a = from.stations.last()?;
    let b = to.stations.first()?;
    let length = a.point.distance(b.point);
    let width = from.half_width.min(to.half_width) * 2.;
    if length < width || length > 50. || (b.height - a.height).abs() / length > 0.12 {
        return None;
    }
    if !crosses_channel(a.point, b.point, input) {
        return None;
    }
    // A wet gap may follow the coastline. Every low sample along a plausible
    // crossing must lie on a river corridor rather than in open water.
    let (a0, _) = strip_ends(from);
    let (_, b1) = strip_ends(to);
    if a0.distance(a.point) < 2. || b1.distance(b.point) < 2. {
        return None;
    }
    for step in 0..=32_u16 {
        let point = a.point.lerp(b.point, f32::from(step) / 32.);
        let low =
            input.terrain.sample(point.x, point.y)? <= input.sea_level + settings.min_dry_height;
        if low && !placement::touches_river(&corridor(point, point, 0.05, 0.05), input, 0.) {
            return None;
        }
    }
    Some(BridgeCandidate {
        from: [a.point.x, a.point.y, a.height],
        to: [b.point.x, b.point.y, b.height],
        width,
    })
}

fn crosses_channel(from: Vec2, to: Vec2, input: &TownInput) -> bool {
    let deck = to - from;
    let mut crossing: Option<Vec2> = None;
    for segment in input
        .rivers
        .iter()
        .flat_map(|river| river.points.windows(2))
    {
        let a = Vec2::from_array(segment[0]);
        let offset = a - from;
        let channel = Vec2::from_array(segment[1]) - a;
        let cross = deck.perp_dot(channel);
        if cross.abs() < 0.0001 {
            continue;
        }
        let along_deck = offset.perp_dot(channel) / cross;
        let along_channel = offset.perp_dot(deck) / cross;
        if !(0. ..=1.).contains(&along_deck) || !(0. ..=1.).contains(&along_channel) {
            continue;
        }
        let point = from + deck * along_deck;
        if cross.abs() < 0.5 * deck.length() * channel.length()
            || crossing.is_some_and(|previous| previous.distance(point) > 0.2)
        {
            return false;
        }
        crossing = Some(point);
    }
    crossing.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::town::{HeightGrid, RiverPolyline};
    use motu::TownProtectedTerrain;

    fn dry_land() -> TownInput {
        TownInput {
            tile_seed: 11,
            sea_level: 0.,
            terrain: HeightGrid {
                origin: [-100., -100.],
                spacing: 2.,
                width: 101,
                height: 101,
                heights: vec![10.; 101 * 101],
            },
            protected_terrain: Vec::new(),
            rivers: Vec::new(),
        }
    }

    fn settings() -> TownSettings {
        TownSettings {
            roads: Some(TownRoadSettings::default()),
            ..TownSettings::default()
        }
    }

    fn bounds() -> BoxCollider {
        BoxCollider {
            min: [-5., -5., 0.],
            max: [5., 5., 0.],
        }
    }

    #[test]
    fn squares_share_nodes_with_four_access_roads_even_when_wide() {
        let input = dry_land();
        let settings = TownSettings {
            roads: Some(TownRoadSettings {
                half_width: 4.,
                access_half_width: 6.,
                ..TownRoadSettings::default()
            }),
            ..settings()
        };
        let streets = layout(
            bounds(),
            PlacementPose::default(),
            &settings,
            &input,
            2,
            &|| false,
        )
        .unwrap()
        .unwrap();
        let initial_sections = streets.roads.strips.len();
        let root = streets.inner_strips().next().unwrap().start;
        let inner = streets.plaza.half_extents[0] + 4.;
        let Streets {
            network: roads,
            bridges,
            ..
        } = streets.finish(root, &input, &settings);
        assert_eq!(roads.strips.len(), initial_sections);
        assert!(bridges.is_empty());
        assert!(roads.validate().is_ok());
        let intersections = roads
            .junctions
            .iter()
            .filter(|junction| junction.square_axis.is_some())
            .count();
        assert_eq!(
            intersections, 16,
            "four corners and four junctions per square"
        );
        for axis in DIRECTIONS {
            assert!(
                roads.surface_height(axis * (inner - 4.05)).is_none(),
                "junctions stay outside the cathedral square"
            );
            assert!(
                roads.surface_height(axis * (inner + 36.)).is_some(),
                "street continues beyond the squares"
            );
        }
    }

    #[test]
    fn cross_road_is_omitted_when_a_span_or_destination_fails() {
        let config = TownRoadSettings {
            ring_spacing: 36.,
            ..TownRoadSettings::default()
        };
        let settings = TownSettings {
            roads: Some(config),
            ..settings()
        };
        let (_, inner) = plan(bounds(), config, 2).unwrap();
        let outer = inner + config.ring_spacing;
        let middle = f32::midpoint(inner, outer);
        let mut river = dry_land();
        river.rivers.push(RiverPolyline {
            points: vec![[middle, -4.], [middle, 4.]],
            widths: vec![1., 1.],
            join: None,
        });
        let mut cut = dry_land();
        for offset in [-2., 0., 2.] {
            // HeightGrid starts at -100 m with 2 m cells. A deep dip is visible
            // to grading but has no river or protected area prefilter.
            let x = (middle * 0.5).round() + offset * 0.5;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let cell = (50. + x) as usize;
            cut.terrain.heights[50 * cut.terrain.width + cell] = -1.;
        }
        let mut missing_junction = dry_land();
        for y in [-10., 10.] {
            missing_junction
                .protected_terrain
                .push(TownProtectedTerrain {
                    min: [outer - 0.5, y - 0.5],
                    max: [outer + 0.5, y + 0.5],
                });
        }
        for (label, input) in [
            ("river", river),
            ("grade", cut),
            ("junction", missing_junction),
        ] {
            let streets = layout(
                bounds(),
                PlacementPose::default(),
                &settings,
                &input,
                2,
                &|| false,
            )
            .unwrap()
            .unwrap();
            let root = streets.inner_strips().next().unwrap().start;
            let roads = streets.finish(root, &input, &settings).network;
            roads.validate().unwrap();
            for x in [inner + 7., middle, outer - 7.] {
                assert!(
                    roads.surface_height(Vec2::new(x, 0.)).is_none(),
                    "{label} left a cross road stub at {x}"
                );
            }
            assert!(
                roads.surface_height(Vec2::new(0., middle)).is_some(),
                "{label} removed an unblocked crossing"
            );
            assert!(
                roads
                    .junctions
                    .iter()
                    .all(|junction| junction.point.distance(Vec2::new(inner, 0.)) > 0.001),
                "{label} left junction paving after the crossing was removed"
            );
        }
    }

    #[test]
    fn channel_preserves_gap_and_records_landings_without_orphan_streets() {
        let mut input = dry_land();
        input.rivers.push(RiverPolyline {
            points: vec![[16., 100.], [16., -100.]],
            widths: vec![4., 4.],
            join: None,
        });
        let settings = settings();
        let streets = layout(
            bounds(),
            PlacementPose::default(),
            &settings,
            &input,
            2,
            &|| false,
        )
        .unwrap()
        .unwrap();
        let root = streets
            .inner_strips()
            .find(|strip| strip_ends(strip).0.x < 0.)
            .unwrap()
            .start;
        let Streets {
            network: roads,
            bridges,
            ..
        } = streets.finish(root, &input, &settings);
        assert!(
            roads
                .strips
                .iter()
                .flat_map(|strip| &strip.stations)
                .all(|station| station.point.x < 14.)
        );
        assert!(!bridges.is_empty());
        assert!(
            bridges
                .iter()
                .all(|bridge| (bridge.from[0] - 16.) * (bridge.to[0] - 16.) < 0.)
        );
    }

    #[test]
    fn a_meander_that_returns_to_the_same_bank_is_no_bridge() {
        let mut input = dry_land();
        input.rivers.push(RiverPolyline {
            points: vec![[5., -10.], [5., 10.], [-5., 10.], [-5., -10.]],
            widths: Vec::new(),
            join: None,
        });
        assert!(!crosses_channel(
            Vec2::new(-10., 0.),
            Vec2::new(10., 0.),
            &input
        ));
    }
}
