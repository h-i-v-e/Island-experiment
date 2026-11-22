//! Direction sign departures on the finished cart-track network.
use std::{cmp::Ordering, collections::BinaryHeap};

use serde::Serialize;

use super::{
    Island,
    cart_track_surface::CartTrackSurface,
    cart_tracks::{CartTrackExit, CartTrackPlan, CartTrackPlanner},
    road_grade::segment_distance_squared,
    spatial_bins::SpatialBins,
};
use crate::{DecorationClearance, ISLAND_WORLD_METRES, Vec2, Vec3};

const FOOTPRINT_RADIUS: f32 = 0.85;
const VERGE_OFFSETS: [f32; 3] = [2.05, 2.7, 3.35];
const STATION_DISTANCES: [f32; 8] = [2.75, 4., 5.5, 7., 9., 12., 15., 18.];
const LOWEST_GROUND_METRES: f32 = 0.75;
const MINIMUM_NORMAL_UP: f32 = 0.906_307_8; // cos(25°)

#[derive(Debug, Serialize)]
pub(super) struct CartTrackDeparture {
    settlement_index: usize,
    exit_index: usize,
    destination_index: i32,
    portal_side: i32,
    position: [f32; 3],
    direction: [f32; 2],
}

pub(super) fn cart_track_departures(
    island: &Island,
    planner: &CartTrackPlanner<'_>,
    plan: &CartTrackPlan,
    tracks: &CartTrackSurface,
    exits: &[CartTrackExit],
    portal_sides: &[i32],
) -> Vec<CartTrackDeparture> {
    let local_count = exits.len() - portal_sides.len();
    if local_count == 0 || plan.runs.is_empty() {
        return Vec::new();
    }

    let graph = TrackGraph::new(plan, exits, portal_sides);
    let mut verge = SignVerge::new(island, planner, plan, tracks);
    let mut departures = Vec::new();
    for (exit_index, exit) in exits[..local_count].iter().enumerate() {
        let Some((destination, route)) = graph.destination(exit_index, exits, portal_sides) else {
            continue;
        };
        let Some((position, direction)) = verge.fit(graph.dry_departure(&route), exit) else {
            continue;
        };
        let (destination_index, portal_side) = match destination {
            Destination::Settlement(index) => (index, -1),
            Destination::Portal(side) => (-1, side),
        };
        departures.push(CartTrackDeparture {
            settlement_index: exit.settlement_index,
            exit_index,
            destination_index,
            portal_side,
            position,
            direction,
        });
    }
    departures
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Destination {
    Settlement(i32),
    Portal(i32),
}

impl Destination {
    fn at(node: usize, exits: &[CartTrackExit], portal_sides: &[i32], from: usize) -> Option<Self> {
        let local_count = exits.len() - portal_sides.len();
        if node < local_count {
            let target = exits[node].settlement_index;
            if target == exits[from].settlement_index {
                return None;
            }
            i32::try_from(target).ok().map(Self::Settlement)
        } else {
            portal_sides
                .get(node - local_count)
                .copied()
                .map(Self::Portal)
        }
    }
}

#[derive(Clone, Copy)]
struct TrackEdge {
    to: usize,
    distance: f32,
    bridge: bool,
}

struct TrackNode {
    point: Vec3,
    edges: Vec<TrackEdge>,
}

struct TrackGraph(Vec<TrackNode>);

impl TrackGraph {
    fn new(plan: &CartTrackPlan, exits: &[CartTrackExit], portal_sides: &[i32]) -> Self {
        let point_count = exits.len() + plan.runs.iter().map(|run| run.points.len()).sum::<usize>();
        let mut graph = Self(Vec::with_capacity(point_count));
        let (local, portals) = exits.split_at(exits.len() - portal_sides.len());
        for exit in local {
            graph.add_point(exit.position);
        }
        for (portal, &side) in portals.iter().zip(portal_sides) {
            let [_, y, z] = portal.position;
            let x = if side == 0 { 0. } else { ISLAND_WORLD_METRES };
            graph.add_point([x, y, z]);
        }
        let mut run_starts = Vec::with_capacity(plan.runs.len() + 1);
        for run in &plan.runs {
            let start = graph.0.len();
            run_starts.push(start);
            for (i, &point) in run.points.iter().enumerate() {
                let node = graph.add_point(point);
                if i > 0 {
                    graph.join(node - 1, node, false);
                }
            }
        }
        run_starts.push(graph.0.len());

        let mut first_run = 0;
        for (link_index, link) in plan.links.iter().enumerate() {
            let run_count = plan.runs[first_run..]
                .iter()
                .take_while(|run| run.link_index == link_index)
                .count();
            if run_count == 0 {
                continue;
            }
            let end_run = first_run + run_count;
            let first = run_starts[first_run];
            let last = run_starts[end_run] - 1;
            if let Some(parent) = link.parent_run {
                // The planner registered this exact point on a specific finished
                // run. An incidental crossing of two polylines is not a junction.
                let branch_point = graph.0[first].point.truncate();
                if let Some(anchor) = (run_starts[parent]..run_starts[parent + 1])
                    .find(|&node| graph.0[node].point.truncate() == branch_point)
                {
                    graph.join(anchor, first, false);
                }
            } else {
                graph.join(link.from_exit, first, false);
            }
            for &landing in &run_starts[first_run + 1..end_run] {
                graph.join(landing - 1, landing, true);
            }
            graph.join(last, link.to_exit, false);
            first_run = end_run;
        }
        graph
    }

    fn add_point(&mut self, point: [f32; 3]) -> usize {
        let index = self.0.len();
        self.0.push(TrackNode {
            point: Vec3::from_array(point),
            edges: Vec::new(),
        });
        index
    }

    fn join(&mut self, a: usize, b: usize, bridge: bool) {
        let from = self.0[a].point;
        let to = self.0[b].point;
        // The paved terminal is coincident in XY with its track endpoint but
        // can have an authored deck height. Only emitted track spans have length.
        let distance = if from.truncate() == to.truncate() {
            0.
        } else {
            from.distance(to)
        };
        self.0[a].edges.push(TrackEdge {
            to: b,
            distance,
            bridge,
        });
        self.0[b].edges.push(TrackEdge {
            to: a,
            distance,
            bridge,
        });
    }

    fn destination(
        &self,
        exit_index: usize,
        exits: &[CartTrackExit],
        portal_sides: &[i32],
    ) -> Option<(Destination, Vec<(usize, bool)>)> {
        if self.0[exit_index].edges.is_empty() {
            return None;
        }
        let origin = Visit {
            node: exit_index,
            distance: 0.,
        };
        let mut queue = BinaryHeap::from([origin]);
        let mut arrivals = vec![Arrival::default(); self.0.len()];
        arrivals[exit_index].distance = 0.;
        let mut best_local: Option<(Visit, Destination)> = None;
        let mut best_portal: Option<(Visit, Destination)> = None;
        while let Some(visit) = queue.pop() {
            if best_local.is_some_and(|(best, _)| visit.distance > best.distance) {
                break;
            }
            if visit.distance > arrivals[visit.node].distance {
                continue;
            }
            if let Some(destination) = Destination::at(visit.node, exits, portal_sides, exit_index)
            {
                let best = match destination {
                    Destination::Settlement(_) => &mut best_local,
                    Destination::Portal(_) => &mut best_portal,
                };
                if best.is_none_or(|(previous, target)| {
                    visit
                        .distance
                        .total_cmp(&previous.distance)
                        .then(destination.cmp(&target))
                        .then(visit.node.cmp(&previous.node))
                        .is_lt()
                }) {
                    *best = Some((visit, destination));
                }
            }
            for &edge in &self.0[visit.node].edges {
                let distance = visit.distance + edge.distance;
                if distance < arrivals[edge.to].distance {
                    arrivals[edge.to] = Arrival {
                        distance,
                        parent: Some((visit.node, edge.bridge)),
                    };
                    queue.push(Visit {
                        node: edge.to,
                        distance,
                    });
                }
            }
        }

        let (visit, destination) = best_local.or(best_portal)?;
        let mut route = vec![(visit.node, false)];
        while let Some((parent, bridge)) = arrivals[route.last()?.0].parent {
            route.push((parent, bridge));
        }
        route.reverse();
        Some((destination, route))
    }

    fn dry_departure<'a>(
        &'a self,
        route: &'a [(usize, bool)],
    ) -> impl Iterator<Item = (Vec2, Vec2)> + 'a {
        route
            .windows(2)
            .take_while(|pair| !pair[0].1)
            .filter_map(|pair| {
                let a = self.0[pair[0].0].point.truncate();
                let b = self.0[pair[1].0].point.truncate();
                (a.distance_squared(b) > f32::EPSILON).then_some((a, b))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::cart_tracks::{CartTrackLink, CartTrackRun};

    fn exit(settlement_index: usize, x: f32, y: f32) -> CartTrackExit {
        CartTrackExit {
            settlement_index,
            position: [x, y, 10.],
            direction: [1., 0.],
            half_width: 2.,
            curb: false,
        }
    }

    fn link(from_exit: usize, to_exit: usize, parent_run: Option<usize>) -> CartTrackLink {
        CartTrackLink {
            from_settlement: from_exit,
            to_settlement: to_exit,
            from_exit,
            to_exit,
            from_track: parent_run.is_some(),
            parent_run,
        }
    }

    fn run(link_index: usize, points: &[[f32; 2]]) -> CartTrackRun {
        CartTrackRun {
            link_index,
            points: points.iter().map(|&[x, y]| [x, y, 10.]).collect(),
            joins_track: false,
        }
    }

    #[test]
    fn destinations_use_the_actual_branch_and_road_distance() {
        let exits = [exit(0, 0., 0.), exit(1, 90., 0.), exit(2, 10., 21.)];
        let mut plan = CartTrackPlan::default();
        // The branch's recorded `from_exit` is the trunk target. Its actual
        // junction is 80m away. The curved branch also makes the village at
        // (10, 21) farther from the origin by road despite its closer position.
        plan.links = vec![link(0, 1, None), link(1, 2, Some(0))];
        plan.runs = vec![
            run(0, &[[0., 0.], [10., 0.], [90., 0.]]),
            run(1, &[[10., 0.], [80., 20.], [10., 21.]]),
        ];
        let graph = TrackGraph::new(&plan, &exits, &[]);
        assert!(matches!(
            graph.destination(0, &exits, &[]),
            Some((Destination::Settlement(1), _))
        ));
        assert!(matches!(
            graph.destination(1, &exits, &[]),
            Some((Destination::Settlement(0), _))
        ));
        let (target, path) = graph.destination(2, &exits, &[]).unwrap();
        assert!(matches!(target, Destination::Settlement(0)));
        let dry: Vec<_> = graph.dry_departure(&path).collect();
        assert_eq!(dry[2], (Vec2::new(10., 0.), Vec2::ZERO));
    }

    #[test]
    fn shared_exit_ties_choose_settlement_index_and_seams_keep_their_side() {
        let x = ISLAND_WORLD_METRES;
        let exits = [
            exit(0, 100., 100.),
            exit(9, 100., 130.),
            exit(2, 130., 100.),
            exit(4, 100., 70.),
            exit(5, x - 40., 100.),
            exit(10, x - 8., 100.),
        ];
        let mut plan = CartTrackPlan::default();
        plan.links = vec![
            link(0, 1, None),
            link(0, 2, None),
            link(0, 3, None),
            link(4, 5, None),
        ];
        plan.runs = vec![
            run(0, &[[100., 100.], [100., 130.]]),
            run(1, &[[100., 100.], [130., 100.]]),
            run(2, &[[100., 100.], [100., 70.]]),
            run(3, &[[x - 40., 100.], [x - 8., 100.], [x, 100.]]),
        ];
        let east_only = [1];
        let graph = TrackGraph::new(&plan, &exits, &east_only);
        assert!(matches!(
            graph.destination(0, &exits, &east_only),
            Some((Destination::Settlement(2), _))
        ));
        let (target, path) = graph.destination(4, &exits, &east_only).unwrap();
        assert!(matches!(target, Destination::Portal(1)));
        assert_eq!(
            graph.dry_departure(&path).last().unwrap().1.x.to_bits(),
            x.to_bits()
        );
    }

    #[test]
    fn a_bridge_counts_toward_distance_but_ends_the_sign_search() {
        let exits = [exit(0, 100., 100.), exit(1, 150., 100.)];
        let mut plan = CartTrackPlan::default();
        plan.links.push(link(0, 1, None));
        plan.runs = vec![
            run(0, &[[100., 100.], [104., 100.]]),
            run(0, &[[125., 100.], [150., 100.]]),
        ];
        let graph = TrackGraph::new(&plan, &exits, &[]);
        let (target, path) = graph.destination(0, &exits, &[]).unwrap();
        assert!(matches!(target, Destination::Settlement(1)));
        let dry: Vec<_> = graph.dry_departure(&path).collect();
        assert_eq!(dry, [(Vec2::new(100., 100.), Vec2::new(104., 100.))]);
    }
}

#[derive(Clone, Copy)]
struct Visit {
    node: usize,
    distance: f32,
}

impl Ord for Visit {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then(other.node.cmp(&self.node))
    }
}

impl PartialOrd for Visit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Visit {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Visit {}

#[derive(Clone, Copy)]
struct Arrival {
    distance: f32,
    parent: Option<(usize, bool)>,
}

impl Default for Arrival {
    fn default() -> Self {
        Self {
            distance: f32::INFINITY,
            parent: None,
        }
    }
}

enum SignObstacle {
    Bridge(DecorationClearance),
    Feature { start: Vec2, end: Vec2, radius: f32 },
}

impl SignObstacle {
    fn blocks(&self, point: Vec2) -> bool {
        match *self {
            Self::Bridge(area) => area.intersects_circle(point, FOOTPRINT_RADIUS),
            Self::Feature { start, end, radius } => {
                segment_distance_squared(point, point, start, end)
                    <= (radius + FOOTPRINT_RADIUS).powi(2)
            }
        }
    }
}

struct SignVerge<'a> {
    island: &'a Island,
    planner: &'a CartTrackPlanner<'a>,
    tracks: &'a CartTrackSurface,
    obstacles: SpatialBins<SignObstacle>,
    occupied: Vec<Vec2>,
}

impl<'a> SignVerge<'a> {
    fn new(
        island: &'a Island,
        planner: &'a CartTrackPlanner<'a>,
        plan: &CartTrackPlan,
        tracks: &'a CartTrackSurface,
    ) -> Self {
        let mut obstacles = SpatialBins::new();
        for bridge in plan.bridge_clearance() {
            let centre = Vec2::from_array(bridge.centre);
            let axis = Vec2::from_angle(bridge.yaw);
            let extent =
                axis.abs() * bridge.half_extents[0] + axis.perp().abs() * bridge.half_extents[1];
            obstacles.insert(
                centre - extent,
                centre + extent,
                SignObstacle::Bridge(bridge),
            );
        }
        let mut add = |start: Vec2, end: Vec2, radius: f32| {
            obstacles.insert(
                start.min(end) - radius,
                start.max(end) + radius,
                SignObstacle::Feature { start, end, radius },
            );
        };
        let xy = |point: Vec3| point.truncate() * ISLAND_WORLD_METRES;
        for tree in island.forest.placements() {
            let centre = xy(tree.anchor);
            add(
                centre,
                centre,
                crate::forest::TREE_CLEARANCE_PER_SCALE_METRES * tree.scale,
            );
        }
        for log in &island.forest.logs {
            add(
                xy(log.bottom),
                xy(log.top),
                log.radius * ISLAND_WORLD_METRES,
            );
        }
        if let Some(decorations) = island.decorations.get() {
            for rock in &decorations.boulders {
                let centre = xy(rock.truncate());
                add(centre, centre, rock.w * ISLAND_WORLD_METRES);
            }
            for bush in &decorations.bushes {
                let centre = xy(*bush);
                add(centre, centre, 0.65);
            }
        }
        for reed in island.reeds.instances() {
            let centre = xy(reed.root);
            add(centre, centre, reed.width_metres);
        }
        Self {
            island,
            planner,
            tracks,
            obstacles,
            occupied: Vec::new(),
        }
    }

    fn fit(
        &mut self,
        segments: impl Iterator<Item = (Vec2, Vec2)>,
        exit: &CartTrackExit,
    ) -> Option<([f32; 3], [f32; 2])> {
        let mut travelled = 0.;
        let mut stations = STATION_DISTANCES.into_iter().peekable();
        let mut short_stretch = None;
        for (start, end) in segments {
            let length = start.distance(end);
            let direction = (end - start) / length;
            let next = travelled + length;
            if next < STATION_DISTANCES[0] {
                short_stretch = Some((start.lerp(end, 0.5), direction));
            }
            while let Some(&station) = stations.peek() {
                if station > next {
                    break;
                }
                stations.next();
                let track_point = start + direction * (station - travelled);
                if let Some(sign) = self.station(track_point, direction, exit) {
                    return Some(sign);
                }
            }
            stations.peek()?;
            travelled = next;
        }
        if travelled < STATION_DISTANCES[0] {
            let (point, direction) = short_stretch?;
            return self.station(point, direction, exit);
        }
        None
    }

    fn station(
        &mut self,
        track: Vec2,
        direction: Vec2,
        exit: &CartTrackExit,
    ) -> Option<([f32; 3], [f32; 2])> {
        let outward = Vec2::from_array(exit.direction);
        // Prefer an approach that is clearly beyond the paved terminal, even
        // when the track starts with a sharp corner.
        if (track - Vec3::from_array(exit.position).truncate()).dot(outward) < 0.5 {
            return None;
        }
        for offset in VERGE_OFFSETS {
            let across = direction.perp() * offset;
            for position in [track + across, track - across] {
                if let Some(height) = self.height(position) {
                    self.occupied.push(position);
                    return Some((position.extend(height).to_array(), direction.to_array()));
                }
            }
        }
        None
    }

    fn height(&self, point: Vec2) -> Option<f32> {
        let min = point - FOOTPRINT_RADIUS;
        let max = point + FOOTPRINT_RADIUS;
        let terrain = self.island.terrain();
        if terrain.paved_road_overlaps(min, max)
            || self.tracks.corridor_overlaps(min, max)
            || !self.planner.clear_sign_ground(point, FOOTPRINT_RADIUS)
            || self
                .obstacles
                .nearby(min, max)
                .any(|obstacle| obstacle.blocks(point))
            || self
                .occupied
                .iter()
                .any(|other| point.distance_squared(*other) <= (2. * FOOTPRINT_RADIUS).powi(2))
        {
            return None;
        }
        let centre = self.support_height(point)?;
        for direction in [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y] {
            self.support_height(point + direction * FOOTPRINT_RADIUS)?;
        }
        Some(centre)
    }

    fn support_height(&self, point: Vec2) -> Option<f32> {
        let uv = point / ISLAND_WORLD_METRES;
        let hit = self
            .island
            .accurate_terrain_surface(uv.extend(0.), Vec3::NEG_Z)?;
        let height = hit.position.z * ISLAND_WORLD_METRES;
        (height >= LOWEST_GROUND_METRES
            && hit.normal.z >= MINIMUM_NORMAL_UP
            && self.island.terrain().footpath_coverage_at(uv) < 0.45)
            .then_some(height)
    }
}
