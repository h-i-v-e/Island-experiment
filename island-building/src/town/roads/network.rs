use super::{corridor, strip_ends};
use crate::{
    collision::BoxCollider,
    terrain_support::PlacementPose,
    town::{BridgeCandidate, TownError, TownInput, TownRoadSettings, TownSettings, placement},
};
use motu::{
    Vec2,
    roads::{RoadEdge, RoadGradeOptions, RoadNetwork, RoadStrip, grade_roads},
};
use std::collections::HashMap;

const DIRECTIONS: [Vec2; 4] = [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y];
const EDGE_LENGTH: f32 = 8.;

pub(in crate::town) struct StreetLayout {
    pub roads: RoadNetwork,
    pub roots: [Option<usize>; 4],
    plan: Plan,
    components: Vec<Option<usize>>,
    edge_to_section: Vec<usize>,
    river_sections: Vec<bool>,
}

pub(in crate::town) struct Streets {
    pub network: RoadNetwork,
    pub bridges: Vec<BridgeCandidate>,
    pub ring_length: f32,
}

impl StreetLayout {
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
        let ring_length = self
            .roads
            .strips
            .iter()
            .filter(|strip| self.plan.sections[self.edge_to_section[strip.source]].ring)
            .map(|strip| self.plan.nodes[strip.start].distance(self.plan.nodes[strip.end]))
            .sum();
        Streets {
            network: self.roads,
            bridges,
            ring_length,
        }
    }
}

#[derive(Clone, Copy)]
struct Section {
    edge: RoadEdge,
    ring: bool,
}

#[derive(Default)]
struct Plan {
    nodes: Vec<Vec2>,
    lookup: HashMap<[u32; 2], usize>,
    sections: Vec<Section>,
    routes: Vec<std::ops::Range<usize>>,
    min_span: f32,
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

    fn route<const N: usize>(&mut self, points: [Vec2; N], half_width: f32, ring: bool) {
        let start = self.sections.len();
        for pair in points.windows(2) {
            self.sections(pair[0], pair[1], half_width, ring);
        }
        self.routes.push(start..self.sections.len());
    }

    // Radii are bounded by TownSettings and this conversion is limited to 128 segments.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn sections(&mut self, from: Vec2, to: Vec2, half_width: f32, ring: bool) {
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

fn plan(
    bounds: BoxCollider,
    settings: &TownSettings,
    roads: TownRoadSettings,
) -> Option<(Plan, [usize; 4])> {
    let width = roads.half_width.max(roads.access_half_width);
    let start = [bounds.max[0], bounds.max[1], -bounds.min[0], -bounds.min[1]]
        .map(|extent| extent + roads.shoulder + roads.access_half_width + 0.35);
    let inner = start.into_iter().fold(0., f32::max) + width * std::f32::consts::SQRT_2 + 4.;
    let outer = settings.town_radius / std::f32::consts::SQRT_2;
    if inner > outer {
        return None;
    }
    let rings = [inner, inner + roads.ring_spacing];
    let ring_count = if roads.ring_count == 2 && rings[1] <= outer {
        2
    } else {
        1
    };
    let mut plan = Plan {
        min_span: width * 2. + 0.5,
        ..Plan::default()
    };
    let roots = std::array::from_fn(|axis| plan.node(DIRECTIONS[axis] * start[axis]));
    for (index, &radius) in rings[..ring_count].iter().enumerate() {
        for (axis, direction) in DIRECTIONS.into_iter().enumerate() {
            let tangent = direction.perp();
            plan.route(
                [
                    (direction - tangent) * radius,
                    direction * radius,
                    (direction + tangent) * radius,
                ],
                roads.half_width,
                true,
            );
            let previous = if index == 0 {
                start[axis]
            } else {
                rings[index - 1]
            };
            plan.route(
                [direction * previous, direction * radius],
                roads.access_half_width,
                false,
            );
        }
    }
    let last = rings[ring_count - 1];
    let exit = (last + roads.ring_spacing * 0.60).min(settings.town_radius);
    for direction in DIRECTIONS {
        plan.route(
            [direction * last, direction * exit],
            roads.access_half_width,
            false,
        );
    }
    Some((plan, roots))
}

pub(in crate::town) fn layout(
    bounds: BoxCollider,
    pose: PlacementPose,
    settings: &TownSettings,
    input: &TownInput,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<StreetLayout>, TownError> {
    let Some(config) = settings.roads else {
        return Ok(None);
    };
    let Some((mut plan, roots)) = plan(bounds, settings, config) else {
        return Ok(None);
    };
    for point in &mut plan.nodes {
        let world = pose.world_point([point.x, point.y, 0.]);
        *point = Vec2::new(world[0], world[1]);
    }
    let mut edges = Vec::new();
    let mut edge_to_section = Vec::new();
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
            edges.push(edge);
            edge_to_section.push(index);
        }
    }
    let roads = grade_roads(
        &plan.nodes,
        &edges,
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
        |point| {
            (!cancelled())
                .then(|| input.terrain.sample(point.x, point.y))
                .flatten()
        },
    );
    if cancelled() {
        return Err(TownError::Cancelled);
    }
    let components = connected_to_centre(&roads, &plan, roots);
    let roots = roots.map(|root| components[root].map(|_| root));
    Ok(roots.iter().any(Option::is_some).then_some(StreetLayout {
        roads,
        roots,
        plan,
        components,
        edge_to_section,
        river_sections,
    }))
}

fn connected_to_centre(roads: &RoadNetwork, plan: &Plan, roots: [usize; 4]) -> Vec<Option<usize>> {
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
            let states = &river_sections[route.clone()];
            let graded = &graded[route.clone()];
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
                half_width: 6.,
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
            &|| false,
        )
        .unwrap()
        .unwrap();
        assert!(streets.roots.iter().all(Option::is_some));
        let initial_sections = streets.roads.strips.len();
        let root = streets.roots[0].unwrap();
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
            .filter(|junction| junction.radius > 1.4 * 6.)
            .count();
        assert_eq!(
            intersections, 16,
            "four corners and four junctions per square"
        );
        for axis in DIRECTIONS {
            assert!(
                roads.surface_height(axis * 59.).is_some(),
                "street continues beyond the squares"
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
            &|| false,
        )
        .unwrap()
        .unwrap();
        let root = streets.roots[2].unwrap();
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
