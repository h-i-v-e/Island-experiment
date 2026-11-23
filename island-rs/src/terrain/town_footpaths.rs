//! Worn stair and pier approaches follow edges of the already graded terrain mesh.
use super::{
    Island, Terrain, TriangleIndex, road_grade::segment_distance_squared, triangle_bin_bounds,
};
use crate::{DecorationClearance, ISLAND_WORLD_METRES, Mesh, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

const SEARCH_RADIUS_METRES: f32 = 24.;
const STAIR_SNAP_METRES: f32 = 6.;
const PIER_SEARCH_RADIUS_METRES: f32 = 160.;
const PIER_SNAP_METRES: f32 = 8.;
const WALKABLE_NORMAL_Z: f32 = std::f32::consts::FRAC_1_SQRT_2;
const PAVEMENT_EDGE: f32 = 0.5;
const ROAD_EDGE_MINIMUM: f32 = 0.35;
const ROAD_EDGE_MAXIMUM: f32 = 0.7;
const FOOTPATH_HALF_WIDTH_METRES: f32 = 0.6;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TownStairFoot {
    /// Stair toe and building origin in island-local XY metres.
    pub position: [f32; 2],
    pub house: [f32; 2],
}

impl TownStairFoot {
    fn is_valid(self) -> bool {
        self.position
            .into_iter()
            .chain(self.house)
            .all(|value| (0. ..=ISLAND_WORLD_METRES).contains(&value))
    }
}

pub(crate) struct PierAccess(Vec<usize>);

impl Island {
    /// Paint existing vertices between the fitted stair toes and graded roads.
    ///
    /// # Errors
    /// Rejects invalid coordinates or oversized requests before updating paint.
    pub fn paint_town_footpaths(
        &mut self,
        feet: &[TownStairFoot],
        foundations: &[DecorationClearance],
    ) -> Result<usize, String> {
        if feet.len() > 4096 || foundations.len() > 8192 || feet.iter().any(|foot| !foot.is_valid())
        {
            return Err("invalid town footpath stair feet or footprint count".into());
        }
        foundations
            .iter()
            .try_for_each(DecorationClearance::validate)?;
        let terrain = &mut self.terrain;
        let mut marked = BTreeSet::new();
        let mut wear = Vec::new();
        if terrain.has_road_coverage() {
            for route in feet
                .iter()
                .filter_map(|foot| trace(terrain, *foot, foundations))
            {
                wear.extend(footpath_wear(terrain.vertices(), &route));
                marked.extend(route);
            }
        }
        let count = marked.len();
        terrain.footpaths = FootpathPaint::new(terrain.mesh(), &terrain.triangle_index, marked);
        self.clear_shrubs(&wear);
        self.wear_settlement_ground(&wear);
        Ok(count)
    }

    pub(crate) fn find_pier_access(
        &self,
        landing: Vec2,
        blockers: &[DecorationClearance],
    ) -> Option<PierAccess> {
        let terrain = &self.terrain;
        if !landing.is_finite()
            || !terrain.road_corridor_overlaps(
                landing - PIER_SEARCH_RADIUS_METRES,
                landing + PIER_SEARCH_RADIUS_METRES,
            )
        {
            return None;
        }
        let origin = terrain.sample_accurate_surface(landing / ISLAND_WORLD_METRES)?;
        if origin.position.z <= 0. || origin.normal.z < WALKABLE_NORMAL_Z {
            return None;
        }
        let blockers = blockers
            .iter()
            .filter(|area| area.intersects_circle(landing, PIER_SEARCH_RADIUS_METRES))
            .collect::<Vec<_>>();
        let channels = pier_channels(self, landing);
        let water_crossing = |a, b| channels.iter().any(|channel| channel.crosses(a, b));
        let nodes = local_graph(
            terrain,
            landing,
            &blockers,
            PIER_SEARCH_RADIUS_METRES,
            true,
            water_crossing,
        );
        let starts = nodes.keys().filter_map(|&id| {
            let vertex = terrain.vertices()[id];
            let point = vertex.truncate() * ISLAND_WORLD_METRES;
            let distance = landing.distance(point);
            if distance > PIER_SNAP_METRES
                || water_crossing(landing, point)
                || blockers
                    .iter()
                    .any(|area| crosses_foundation(area, landing, point))
                || !walkable_landing(terrain, landing, point, origin.position.z, vertex.z)
            {
                return None;
            }
            Some(Visit {
                id,
                cost: origin.position.distance(vertex) * ISLAND_WORLD_METRES,
            })
        });
        shortest_path(
            &nodes,
            terrain.mesh(),
            starts,
            PIER_SEARCH_RADIUS_METRES,
            |id| is_road_edge(&nodes[&id]),
        )
        .map(PierAccess)
    }

    pub(crate) fn install_pier_access(&mut self, access: PierAccess) {
        let terrain = &mut self.terrain;
        let wear: Vec<_> = footpath_wear(terrain.vertices(), &access.0).collect();
        let mut marked = terrain
            .footpaths
            .take()
            .map_or_else(BTreeSet::new, |paint| paint.vertices);
        marked.extend(access.0);
        terrain.footpaths = FootpathPaint::new(terrain.mesh(), &terrain.triangle_index, marked);
        self.clear_shrubs(&wear);
        self.wear_settlement_ground(&wear);
    }
}

fn footpath_wear<'a>(
    vertices: &'a [crate::Vec3],
    route: &'a [usize],
) -> impl Iterator<Item = DecorationClearance> + 'a {
    route.windows(2).map(|pair| {
        let start = vertices[pair[0]].truncate() * ISLAND_WORLD_METRES;
        let end = vertices[pair[1]].truncate() * ISLAND_WORLD_METRES;
        let delta = end - start;
        DecorationClearance {
            centre: start.midpoint(end).to_array(),
            half_extents: [
                delta.length() * 0.5 + FOOTPATH_HALF_WIDTH_METRES,
                FOOTPATH_HALF_WIDTH_METRES,
            ],
            yaw: delta.to_angle(),
        }
    })
}

struct PierChannel {
    start: Vec2,
    end: Vec2,
    width: f32,
}

impl PierChannel {
    fn crosses(&self, start: Vec2, end: Vec2) -> bool {
        segment_distance_squared(self.start, self.end, start, end) <= self.width.powi(2)
    }
}

fn pier_channels(island: &Island, landing: Vec2) -> Vec<PierChannel> {
    let mut channels = Vec::new();
    for river in island.rivers() {
        let options = island.options();
        let widths = river.target_half_widths(
            options.river_source_width_metres,
            options.river_maximum_width_metres,
        );
        for (nodes, widths) in river.nodes.windows(2).zip(widths.windows(2)) {
            let start = nodes[0].position.truncate() * ISLAND_WORLD_METRES;
            let end = nodes[1].position.truncate() * ISLAND_WORLD_METRES;
            let width = widths[0].max(widths[1]);
            if segment_distance_squared(start, end, landing, landing)
                <= (PIER_SEARCH_RADIUS_METRES + width).powi(2)
            {
                channels.push(PierChannel { start, end, width });
            }
        }
    }
    channels
}

fn walkable_landing(
    terrain: &Terrain,
    landing: Vec2,
    point: Vec2,
    start_z: f32,
    end_z: f32,
) -> bool {
    let run = landing.distance(point);
    if (start_z - end_z).abs() * ISLAND_WORLD_METRES > run + 0.001 {
        return false;
    }
    let steps = (run * 2.).ceil() as u32;
    (1..steps).all(|step| {
        let location = landing.lerp(point, step as f32 / steps as f32) / ISLAND_WORLD_METRES;
        terrain
            .sample_accurate_surface(location)
            .is_some_and(|hit| {
                hit.position.z > 0.
                    && hit.normal.z >= WALKABLE_NORMAL_Z
                    && terrain.road_coverage_at(location) <= ROAD_EDGE_MAXIMUM
            })
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct FootpathPaint {
    vertices: BTreeSet<usize>,
    bins: Vec<bool>,
}

impl FootpathPaint {
    pub(super) fn contains_vertex(&self, index: usize) -> bool {
        self.vertices.contains(&index)
    }

    fn new(mesh: &Mesh, index: &TriangleIndex, vertices: BTreeSet<usize>) -> Option<Self> {
        if vertices.is_empty() {
            return None;
        }
        let dimension = index.dimension;
        let mut bins = vec![false; dimension * dimension];
        for face in mesh.triangles.chunks_exact(3) {
            if !face.iter().any(|&id| vertices.contains(&(id as usize))) {
                continue;
            }
            let (x0, x1, y0, y1) = triangle_bin_bounds(mesh, face, dimension);
            for y in y0..=y1 {
                bins[y * dimension + x0..=y * dimension + x1].fill(true);
            }
        }
        Some(Self { vertices, bins })
    }

    fn coverage(&self, terrain: &Terrain, point: Vec2) -> f32 {
        let [x, y] = terrain.triangle_index.point_bin(point);
        if !self.bins[y * terrain.triangle_index.dimension + x] {
            return 0.;
        }
        terrain
            .sample_triangle(point)
            .map_or(0., |(triangle, weights)| {
                let marked_weight: f32 = triangle
                    .into_iter()
                    .zip(weights)
                    .filter(|(id, _)| self.vertices.contains(id))
                    .map(|(_, weight)| weight)
                    .sum();
                marked_weight.clamp(0., 1.)
            })
    }
}

impl Terrain {
    /// The normalized gravel weight painted along stair and pier footpaths.
    pub(crate) fn footpath_coverage_at(&self, point: Vec2) -> f32 {
        self.footpaths
            .as_ref()
            .map_or(0., |paint| paint.coverage(self, point))
    }
}

struct Node {
    road: f32,
    neighbours: Vec<usize>,
}

fn trace(
    terrain: &Terrain,
    foot: TownStairFoot,
    foundations: &[DecorationClearance],
) -> Option<Vec<usize>> {
    let toe = Vec2::from_array(foot.position);
    let house = Vec2::from_array(foot.house);
    let blockers = foundations
        .iter()
        .filter(|area| area.intersects_circle(toe, SEARCH_RADIUS_METRES))
        .collect::<Vec<_>>();
    let nodes = local_graph(
        terrain,
        toe,
        &blockers,
        SEARCH_RADIUS_METRES,
        false,
        |_, _| false,
    );
    let positions = terrain.vertices();
    let point = |id: usize| positions[id].truncate() * ISLAND_WORLD_METRES;
    let by_toe_distance = |a: &usize, b: &usize| {
        point(*a)
            .distance_squared(toe)
            .total_cmp(&point(*b).distance_squared(toe))
            .then_with(|| a.cmp(b))
    };
    let edge = nodes
        .iter()
        .filter_map(|(&id, node)| is_road_edge(node).then_some(id))
        .min_by(by_toe_distance)?;
    let curb = point(edge);
    let toward_road = (curb - house).normalize_or_zero();
    let start = nodes
        .iter()
        .filter_map(|(&id, node)| {
            let start = point(id);
            (node.road < PAVEMENT_EDGE
                && (start - curb).dot(toward_road) < 0.
                && start.distance_squared(toe) <= STAIR_SNAP_METRES.powi(2)
                && !blockers
                    .iter()
                    .any(|area| crosses_foundation(area, toe, start)))
            .then_some(id)
        })
        .min_by(by_toe_distance)?;
    let goal = |id: usize| {
        id != start && is_road_edge(&nodes[&id]) && (point(id) - curb).dot(toward_road) <= 1.
    };
    shortest_path(
        &nodes,
        terrain.mesh(),
        std::iter::once(Visit {
            id: start,
            cost: 0.,
        }),
        SEARCH_RADIUS_METRES,
        goal,
    )
}

fn is_road_edge(node: &Node) -> bool {
    (ROAD_EDGE_MINIMUM..=ROAD_EDGE_MAXIMUM).contains(&node.road)
}

fn local_graph(
    terrain: &Terrain,
    origin: Vec2,
    blockers: &[&DecorationClearance],
    radius: f32,
    walkable_faces_only: bool,
    water_crossing: impl Fn(Vec2, Vec2) -> bool,
) -> BTreeMap<usize, Node> {
    let mesh = terrain.mesh();
    let faces = terrain
        .triangle_index
        .faces_in_bounds(
            (origin - radius) / ISLAND_WORLD_METRES,
            (origin + radius) / ISLAND_WORLD_METRES,
        )
        .collect::<BTreeSet<_>>();
    let point = |id: usize| mesh.vertices[id].truncate() * ISLAND_WORLD_METRES;
    let face = |id: u32| {
        let offset = id as usize * 3;
        [
            mesh.triangles[offset],
            mesh.triangles[offset + 1],
            mesh.triangles[offset + 2],
        ]
        .map(|vertex| vertex as usize)
    };
    let mut nodes = BTreeMap::new();
    for id in faces
        .iter()
        .copied()
        .flat_map(face)
        .collect::<BTreeSet<_>>()
    {
        let position = point(id);
        if mesh.vertices[id].z <= 0.
            || position.distance_squared(origin) > radius.powi(2)
            || water_crossing(position, position)
            || blockers
                .iter()
                .any(|area| area.intersects_circle(position, 0.))
        {
            continue;
        }
        let road = terrain.road_coverage_at(mesh.vertices[id].truncate());
        if road <= ROAD_EDGE_MAXIMUM {
            nodes.insert(
                id,
                Node {
                    road,
                    neighbours: Vec::new(),
                },
            );
        }
    }
    for [a, b, c] in faces.into_iter().map(face) {
        if walkable_faces_only {
            let [p, q, r] = [a, b, c].map(|id| mesh.vertices[id]);
            let normal = (q - p).cross(r - p).normalize_or_zero();
            if normal.z.abs() < WALKABLE_NORMAL_Z {
                continue;
            }
        }
        for (from, to) in [(a, b), (b, c), (c, a)] {
            if !nodes.contains_key(&from) || !nodes.contains_key(&to) {
                continue;
            }
            let [p, q] = [from, to].map(point);
            if water_crossing(p, q)
                || blockers.iter().any(|area| crosses_foundation(area, p, q))
                || (mesh.vertices[from].z - mesh.vertices[to].z).abs() * ISLAND_WORLD_METRES
                    > p.distance(q) + 0.001
                || terrain.road_coverage_at(p.midpoint(q) / ISLAND_WORLD_METRES) > ROAD_EDGE_MAXIMUM
            {
                continue;
            }
            nodes.get_mut(&from).expect("present").neighbours.push(to);
            nodes.get_mut(&to).expect("present").neighbours.push(from);
        }
    }
    nodes
}

fn crosses_foundation(area: &DecorationClearance, start: Vec2, end: Vec2) -> bool {
    let axis = Vec2::from_angle(area.yaw);
    let centre = Vec2::from_array(area.centre);
    let half = Vec2::from_array(area.half_extents);
    let [a, b] = [start, end].map(|p| {
        let p = p - centre;
        Vec2::new(axis.dot(p), axis.perp_dot(p))
    });
    let normal = (b - a).perp();
    !a.min(b).cmpgt(half).any()
        && !a.max(b).cmplt(-half).any()
        && normal.dot(a).abs() <= normal.abs().dot(half)
}

#[derive(Clone, Copy)]
struct Visit {
    id: usize,
    cost: f32,
}

impl Ord for Visit {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.id.cmp(&self.id))
    }
}
impl PartialOrd for Visit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Visit {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Visit {}

fn shortest_path(
    nodes: &BTreeMap<usize, Node>,
    mesh: &Mesh,
    starts: impl Iterator<Item = Visit>,
    limit: f32,
    goal: impl Fn(usize) -> bool,
) -> Option<Vec<usize>> {
    let mut frontier = BinaryHeap::new();
    let mut visited = BTreeMap::new();
    for visit in starts {
        frontier.push(visit);
        visited.insert(visit.id, (visit.cost, None));
    }
    while let Some(Visit { id, cost }) = frontier.pop() {
        if cost > visited[&id].0 {
            continue;
        }
        if goal(id) {
            let mut path = vec![id];
            while let Some(parent) = visited[path.last().expect("nonempty path")].1 {
                path.push(parent);
            }
            return Some(path);
        }
        for &next in &nodes[&id].neighbours {
            let length = mesh.vertices[id].distance(mesh.vertices[next]) * ISLAND_WORLD_METRES;
            let candidate = cost + length;
            if candidate <= limit && visited.get(&next).is_none_or(|&(best, _)| candidate < best) {
                visited.insert(next, (candidate, Some(id)));
                frontier.push(Visit {
                    id: next,
                    cost: candidate,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ForestOptions, River, RiverNode,
        roads::{RoadNetwork, RoadStation, RoadStrip},
    };

    #[test]
    fn pier_access_follows_dry_terrain_and_keeps_existing_stair_paint() {
        let mut island = Island::shared_forest_fixture(ForestOptions::default());
        let road = RoadNetwork {
            strips: vec![RoadStrip {
                source: 0,
                start: 0,
                end: 1,
                half_width: 2.,
                stations: vec![
                    RoadStation {
                        point: Vec2::new(645., 650.),
                        height: 39.,
                    },
                    RoadStation {
                        point: Vec2::new(675., 650.),
                        height: 39.,
                    },
                ],
            }],
            junctions: Vec::new(),
            shoulder: 2.,
        };
        let landing = Vec2::new(680., 720.);
        assert!(island.find_pier_access(landing, &[]).is_none());
        island.apply_town_roads(&road).unwrap();
        let foot = TownStairFoot {
            position: [660., 656.],
            house: [660., 660.],
        };
        assert!(island.paint_town_footpaths(&[foot], &[]).unwrap() > 0);
        let wall = DecorationClearance {
            centre: [700., 680.],
            half_extents: [180., 0.25],
            yaw: 0.,
        };
        assert!(island.find_pier_access(landing, &[wall]).is_none());
        let river = River {
            nodes: [600., 800.]
                .into_iter()
                .map(|x| RiverNode {
                    vertex: 0,
                    flow: 1,
                    surface: 42. / ISLAND_WORLD_METRES,
                    position: Vec2::new(x, 680.).extend(40.) / ISLAND_WORLD_METRES,
                })
                .collect(),
            join: None,
        };
        island.rivers.push(river);
        assert!(island.find_pier_access(landing, &[]).is_none());
        island.rivers.clear();

        let terrain = island.terrain();
        let original_paint = terrain.footpaths.as_ref().unwrap().vertices.clone();
        let geometry = (terrain.vertex_count(), terrain.triangle_count());
        let access = island
            .find_pier_access(landing, &[])
            .expect("dry path to road");
        let edge = terrain.vertices()[access.0[0]].truncate();
        assert!((ROAD_EDGE_MINIMUM..=ROAD_EDGE_MAXIMUM).contains(&terrain.road_coverage_at(edge)));
        let start = terrain.vertices()[*access.0.last().unwrap()].truncate();
        assert!(landing.distance(start * ISLAND_WORLD_METRES) <= PIER_SNAP_METRES);
        island.install_pier_access(access);
        let terrain = island.terrain();
        assert_eq!(geometry, (terrain.vertex_count(), terrain.triangle_count()));
        let marked = &terrain.footpaths.as_ref().unwrap().vertices;
        assert!(original_paint.is_subset(marked));
        assert!(marked.len() > original_paint.len());
        assert!(terrain.footpath_coverage_at(start) > 0.98);
    }
}
