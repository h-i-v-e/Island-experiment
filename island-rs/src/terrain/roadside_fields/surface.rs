use super::geometry::{
    bounds, intersects_circle, intersects_marked_triangle, overlaps, segment_near,
};
use crate::terrain::{Island, TriangleIndex, spatial_bins::SpatialBins};
use crate::{DecorationClearance, ISLAND_WORLD_METRES, Mesh, Vec2, Vec3};

const TILE_MARGIN: f32 = 5.;
const LOWEST_GROUND_METRES: f32 = 1.75;
const WET_BANK_MARGIN: f32 = 3.0;
const MIN_UP_FIELD: f32 = 0.939_692_6; // cos(20°)
const MIN_UP_WHEAT: f32 = 0.970_295_7; // cos(14°)

pub(super) struct Surface<'a> {
    island: &'a Island,
    water_index: Option<TriangleIndex>,
    obstacles: SpatialBins<Obstacle>,
}

impl<'a> Surface<'a> {
    pub(super) fn new(island: &'a Island, blockers: &[DecorationClearance]) -> Self {
        let mut obstacles = SpatialBins::new();
        for blocker in blockers.iter().filter(|area| area.validate().is_ok()) {
            let centre = Vec2::from_array(blocker.centre);
            let axis = Vec2::from_angle(blocker.yaw);
            let [x, y] = blocker.half_extents.map(|half| half + 1.0);
            let (x, y) = (axis * x, axis.perp() * y);
            let rectangle = [
                centre - x - y,
                centre + x - y,
                centre + x + y,
                centre - x + y,
            ];
            let [min, max] = bounds(&rectangle);
            obstacles.insert(min, max, Obstacle::Polygon(rectangle));
        }
        for area in island.town_protected_terrain().into_iter().skip(4) {
            let min = Vec2::from_array(area.min);
            let max = Vec2::from_array(area.max);
            let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
            obstacles.insert(min, max, Obstacle::Polygon(corners));
        }
        for tree in island.forest.placements() {
            let radius = crate::forest::TREE_CLEARANCE_PER_SCALE_METRES * tree.scale + 0.75;
            let centre = world_xy(tree.anchor);
            let extent = Vec2::splat(radius);
            obstacles.insert(
                centre - extent,
                centre + extent,
                Obstacle::Tree(centre, radius),
            );
        }
        for log in &island.forest.logs {
            let start = world_xy(log.bottom);
            let end = world_xy(log.top);
            let radius = log.radius * ISLAND_WORLD_METRES + 0.75;
            obstacles.insert(
                start.min(end) - Vec2::splat(radius),
                start.max(end) + Vec2::splat(radius),
                Obstacle::Segment(start, end, radius),
            );
        }
        if let Some(decorations) = island.decorations.get() {
            for rock in &decorations.boulders {
                insert_circle(
                    &mut obstacles,
                    world_xy(rock.truncate()),
                    rock.w * ISLAND_WORLD_METRES + 0.75,
                );
            }
        }
        for reed in island.reeds.instances() {
            insert_circle(
                &mut obstacles,
                world_xy(reed.root),
                reed.width_metres + 0.75,
            );
        }
        let water = island.river_mesh();
        let water_index = (!water.triangles.is_empty()).then(|| TriangleIndex::new(water));
        Self {
            island,
            water_index,
            obstacles,
        }
    }

    /// Accept only dry, open, walkable ground. The boolean records whether the
    /// whole strip also meets the gentler cultivation slope.
    pub(super) fn suitable(&self, quad: &[Vec2; 4]) -> Option<bool> {
        let [min, max] = bounds(quad);
        let terrain = self.island.terrain();
        if min.min_element() < TILE_MARGIN
            || max.max_element() > ISLAND_WORLD_METRES - TILE_MARGIN
            || terrain.road_corridor_overlaps(min, max)
            || self
                .obstacles
                .nearby(min, max)
                .any(|obstacle| obstacle.intersects(quad))
            || self.near_water(quad, min, max)
        {
            return None;
        }
        self.position(quad.iter().sum::<Vec2>() * 0.25)?;
        let mut gentle = true;
        for face in terrain
            .triangle_index
            .faces_in_bounds(min / ISLAND_WORLD_METRES, max / ISLAND_WORLD_METRES)
        {
            gentle &= self.suitable_face(quad, face)?;
        }
        Some(gentle)
    }

    fn near_water(&self, quad: &[Vec2; 4], min: Vec2, max: Vec2) -> bool {
        let margin = Vec2::splat(WET_BANK_MARGIN);
        self.water_index.as_ref().is_some_and(|index| {
            let water = self.island.river_mesh();
            index
                .faces_in_bounds(
                    (min - margin) / ISLAND_WORLD_METRES,
                    (max + margin) / ISLAND_WORLD_METRES,
                )
                .any(|face| {
                    let [a, b, c] =
                        face_indices(water, face).map(|index| world_xy(water.vertices[index]));
                    overlaps(quad, &[a, b, c])
                        || [(a, b), (b, c), (c, a)]
                            .into_iter()
                            .any(|(start, end)| segment_near(quad, start, end, WET_BANK_MARGIN))
                })
        })
    }

    fn suitable_face(&self, quad: &[Vec2; 4], face: u32) -> Option<bool> {
        let terrain = self.island.terrain();
        let mesh = terrain.mesh();
        let indices = face_indices(mesh, face);
        let vertices = indices.map(|index| mesh.vertices[index]);
        let [a, b, c] = vertices;
        let cross = (b - a).cross(c - a);
        if cross.z == 0. {
            return Some(true);
        }
        let normal = cross.z.abs() / cross.length();
        let points = vertices.map(world_xy);
        if self.face_has_unusable_ground(quad, indices, vertices, points) {
            return None;
        }
        if normal >= MIN_UP_WHEAT || !overlaps(quad, &points) {
            return Some(true);
        }
        (normal >= MIN_UP_FIELD).then_some(false)
    }

    fn face_has_unusable_ground(
        &self,
        quad: &[Vec2; 4],
        indices: [usize; 3],
        vertices: [Vec3; 3],
        points: [Vec2; 3],
    ) -> bool {
        let material = &self.island.material.values;
        let environment = &self.island.environment.values;
        let terrain = self.island.terrain();
        let wet = vertices.map(|vertex| LOWEST_GROUND_METRES - vertex.z * ISLAND_WORLD_METRES);
        let river_bed = indices.map(|i| material[i].z - 0.35);
        let forest =
            indices.map(|i| environment[i].x * (1. - terrain.ground_disturbance(i)) - 0.50);
        let stone = indices.map(|i| environment[i].y - 0.55);
        let rock = indices.map(|i| (material[i].x - 0.93).min(0.20 - material[i].y));
        let paint = terrain.footpaths.as_ref();
        let footpath =
            indices.map(|i| f32::from(paint.is_some_and(|paint| paint.contains_vertex(i))) - 0.35);
        [wet, river_bed, forest, stone, rock, footpath]
            .into_iter()
            .any(|margins| intersects_marked_triangle(quad, points, margins))
    }

    pub(super) fn position(&self, point: Vec2) -> Option<[f32; 3]> {
        let hit = self
            .island
            .terrain()
            .sample_accurate_surface(point / ISLAND_WORLD_METRES)?;
        Some([point.x, point.y, hit.position.z * ISLAND_WORLD_METRES])
    }

    pub(super) fn sheltered(&self, point: Vec2, wind: Vec2) -> bool {
        let upstream = point - wind * 22.;
        let margin = Vec2::splat(6.);
        self.obstacles
            .nearby(point.min(upstream) - margin, point.max(upstream) + margin)
            .any(|obstacle| {
                let Obstacle::Tree(centre, radius) = *obstacle else {
                    return false;
                };
                let offset = point - centre;
                (0. ..=radius * 3.5).contains(&offset.dot(wind))
                    && offset.perp_dot(wind).abs() <= radius * 1.15
            })
    }

    pub(super) fn usable_gate_approach(&self, route: Vec2, gate: Vec2) -> bool {
        let from = route.lerp(gate, 0.3);
        let right = (gate - from).normalize_or_zero().perp() * 0.85;
        let approach = [from - right, from + right, gate + right, gate - right];
        let [min, max] = bounds(&approach);
        if self
            .obstacles
            .nearby(min, max)
            .any(|obstacle| obstacle.intersects(&approach))
            || self.near_water(&approach, min, max)
        {
            return false;
        }
        // The verge can slope more than the field. Keep the gate approach continuous.
        [0., 0.33, 0.66, 1.].into_iter().all(|t| {
            let centre = from.lerp(gate, t);
            [-1., 0., 1.].into_iter().all(|side| {
                self.island
                    .terrain()
                    .sample_accurate_surface((centre + right * side) / ISLAND_WORLD_METRES)
                    .is_some_and(|hit| {
                        hit.position.z * ISLAND_WORLD_METRES > LOWEST_GROUND_METRES
                            && hit.normal.z > 0.891_006_5
                    })
            })
        })
    }
}

fn insert_circle(obstacles: &mut SpatialBins<Obstacle>, centre: Vec2, radius: f32) {
    let extent = Vec2::splat(radius);
    obstacles.insert(
        centre - extent,
        centre + extent,
        Obstacle::Circle(centre, radius),
    );
}

fn face_indices(mesh: &Mesh, face: u32) -> [usize; 3] {
    let offset = face as usize * 3;
    [0, 1, 2].map(|index| mesh.triangles[offset + index] as usize)
}

fn world_xy(vertex: Vec3) -> Vec2 {
    vertex.truncate() * ISLAND_WORLD_METRES
}

enum Obstacle {
    Polygon([Vec2; 4]),
    Circle(Vec2, f32),
    Tree(Vec2, f32),
    Segment(Vec2, Vec2, f32),
}

impl Obstacle {
    fn intersects(&self, quad: &[Vec2; 4]) -> bool {
        match self {
            Self::Polygon(polygon) => overlaps(quad, polygon),
            Self::Circle(centre, radius) | Self::Tree(centre, radius) => {
                intersects_circle(quad, *centre, *radius)
            }
            Self::Segment(start, end, radius) => segment_near(quad, *start, *end, *radius),
        }
    }
}
