//! Place scattered understory shrubs in the clearings beside established trees.
use std::f32::consts::TAU;

use super::{
    ISLAND_WORLD_METRES, Mesh, Rng, Terrain, TriangleIndex, Vec2, Vec3, Vec4,
    road_grade::segment_distance_squared, spatial_bins::SpatialBins,
};
use crate::{
    caves::CaveSet,
    forest::{ForestMeshes, TREE_CLEARANCE_PER_SCALE_METRES},
};

pub(super) const SHRUB_RADIUS_METRES: f32 = 1.4;
const MINIMUM_SPACING_METRES: f32 = 4.5;
const NEIGHBOUR_REACH_METRES: f32 = 22.;
const OPEN_REACH_METRES: f32 = 16.;
const OPEN_ANGLE_COSINE: f32 = 0.55;
const MINIMUM_HEIGHT_METRES: f32 = 2.;
const MINIMUM_UP_NORMAL: f32 = 0.819_152; // cos(35°)
const MAX_SHRUBS: usize = 1024;
const SHRUB_DOMAIN: u64 = 0x7368_7275_622d_6564;

pub(super) struct ShrubSurface<'a> {
    pub(super) terrain: &'a Terrain,
    pub(super) water: &'a Mesh,
    pub(super) forest: &'a ForestMeshes,
    pub(super) caves: &'a CaveSet,
    pub(super) boulders: &'a [Vec4],
}

pub(super) fn scatter(seed: u64, surface: ShrubSurface<'_>) -> Vec<Vec3> {
    let placements = surface.forest.placements();
    if placements.len() < 2 {
        return Vec::new();
    }
    let mut trees = SpatialBins::new();
    let mut largest_tree_radius: f32 = 0.;
    for tree in placements {
        let centre = metres_xy(tree.anchor);
        let radius = TREE_CLEARANCE_PER_SCALE_METRES * tree.scale;
        largest_tree_radius = largest_tree_radius.max(radius);
        trees.insert(centre, centre, (centre, radius));
    }
    let obstacles = surface.obstacles();
    let water = (!surface.water.triangles.is_empty()).then(|| TriangleIndex::new(surface.water));
    let mut spacing = SpatialBins::new();
    let mut shrubs = Vec::new();
    for tree in placements {
        let centre = metres_xy(tree.anchor);
        let mut rng = Rng::new(seed ^ SHRUB_DOMAIN ^ u64::from(tree.terrain_vertex));
        if rng.unit() > 0.55 || !has_neighbour(centre, &trees) {
            continue;
        }
        let minimum =
            5.5_f32.max(TREE_CLEARANCE_PER_SCALE_METRES * tree.scale + SHRUB_RADIUS_METRES);
        let start = rng.range(0., TAU);
        let root = (0_u8..4).find_map(|attempt| {
            let direction = Vec2::from_angle((TAU / 4.).mul_add(f32::from(attempt), start));
            let point = centre + direction * rng.range(minimum, minimum + 4.5);
            if !has_room(point, direction, &trees, largest_tree_radius)
                || spacing
                    .nearby(
                        point - MINIMUM_SPACING_METRES,
                        point + MINIMUM_SPACING_METRES,
                    )
                    .any(|other: &Vec2| {
                        other.distance_squared(point) < MINIMUM_SPACING_METRES.powi(2)
                    })
                || obstacles
                    .nearby(point, point)
                    .any(|obstacle| obstacle.overlaps(point))
            {
                return None;
            }
            surface.fit(point, water.as_ref())
        });
        if let Some(root) = root {
            let point = metres_xy(root);
            spacing.insert(point, point, point);
            shrubs.push(root);
            if shrubs.len() == MAX_SHRUBS {
                break;
            }
        }
    }
    shrubs
}

fn metres_xy(point: Vec3) -> Vec2 {
    point.truncate() * ISLAND_WORLD_METRES
}

fn has_neighbour(centre: Vec2, trees: &SpatialBins<(Vec2, f32)>) -> bool {
    trees
        .nearby(
            centre - NEIGHBOUR_REACH_METRES,
            centre + NEIGHBOUR_REACH_METRES,
        )
        .any(|&(other, _)| {
            let distance = other.distance_squared(centre);
            distance > 0. && distance <= NEIGHBOUR_REACH_METRES.powi(2)
        })
}

fn has_room(
    point: Vec2,
    outward: Vec2,
    trees: &SpatialBins<(Vec2, f32)>,
    largest_tree_radius: f32,
) -> bool {
    let reach = OPEN_REACH_METRES.max(largest_tree_radius + SHRUB_RADIUS_METRES);
    !trees
        .nearby(point - reach, point + reach)
        .any(|&(tree, radius)| {
            let delta = tree - point;
            let distance = delta.length_squared();
            distance < (radius + SHRUB_RADIUS_METRES).powi(2)
                || (distance <= OPEN_REACH_METRES.powi(2)
                    && delta.dot(outward) > distance.sqrt() * OPEN_ANGLE_COSINE)
        })
}

impl ShrubSurface<'_> {
    fn fit(&self, point: Vec2, water: Option<&TriangleIndex>) -> Option<Vec3> {
        let radius = SHRUB_RADIUS_METRES;
        let margin = radius + 1.;
        if point.min_element() < margin || point.max_element() > ISLAND_WORLD_METRES - margin {
            return None;
        }
        let root = self.ground(point)?;
        if self.caves.excludes(root * ISLAND_WORLD_METRES, radius)
            || water.is_some_and(|index| self.intersects_water(index, point))
        {
            return None;
        }
        [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y]
            .into_iter()
            .all(|direction| self.ground(point + direction * radius).is_some())
            .then_some(root)
    }

    fn ground(&self, point: Vec2) -> Option<Vec3> {
        let hit = self
            .terrain
            .sample_accurate_surface(point / ISLAND_WORLD_METRES)?;
        (hit.position.z * ISLAND_WORLD_METRES > MINIMUM_HEIGHT_METRES
            && hit.normal.z >= MINIMUM_UP_NORMAL)
            .then_some(hit.position)
    }

    fn intersects_water(&self, index: &TriangleIndex, point: Vec2) -> bool {
        let radius = SHRUB_RADIUS_METRES;
        let min = (point - radius) / ISLAND_WORLD_METRES;
        let max = (point + radius) / ISLAND_WORLD_METRES;
        index.faces_in_bounds(min, max).any(|face| {
            let [a, b, c] = [0, 1, 2].map(|i| {
                let vertex = self.water.triangles[face as usize * 3 + i] as usize;
                metres_xy(self.water.vertices[vertex])
            });
            let area = (b - a).perp_dot(c - a);
            let edges = [(a, b), (b, c), (c, a)];
            (area != 0.
                && edges
                    .iter()
                    .all(|&(a, b)| (b - a).perp_dot(point - a) * area >= 0.))
                || edges
                    .into_iter()
                    .any(|(a, b)| segment_distance_squared(point, point, a, b) <= radius.powi(2))
        })
    }

    fn obstacles(&self) -> SpatialBins<Obstacle> {
        let mut bins = SpatialBins::new();
        let mut add = |start: Vec2, end: Vec2, radius: f32| {
            let reach = radius + SHRUB_RADIUS_METRES;
            bins.insert(
                start.min(end) - reach,
                start.max(end) + reach,
                Obstacle { start, end, reach },
            );
        };
        for log in &self.forest.logs {
            add(
                metres_xy(log.bottom),
                metres_xy(log.top),
                log.radius * ISLAND_WORLD_METRES,
            );
        }
        for rock in self.boulders {
            let centre = metres_xy(rock.truncate());
            add(centre, centre, rock.w * ISLAND_WORLD_METRES);
        }
        bins
    }
}

struct Obstacle {
    start: Vec2,
    end: Vec2,
    reach: f32,
}

impl Obstacle {
    fn overlaps(&self, point: Vec2) -> bool {
        segment_distance_squared(point, point, self.start, self.end) <= self.reach.powi(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forest::TreePlacement;

    fn rectangle(min: Vec2, max: Vec2, heights: [f32; 2]) -> Mesh {
        let vertices = [
            min.extend(heights[0]),
            Vec3::new(max.x, min.y, heights[1]),
            max.extend(heights[1]),
            Vec3::new(min.x, max.y, heights[0]),
        ]
        .map(|point| point / ISLAND_WORLD_METRES);
        Mesh {
            vertices: vertices.to_vec(),
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            ..Mesh::default()
        }
    }

    fn grove() -> ForestMeshes {
        let mut roots = (0_u16..11)
            .flat_map(|y| {
                (0_u16..11).map(move |x| {
                    Vec3::new(500. + 8. * f32::from(x), 500. + 8. * f32::from(y), 30.)
                })
            })
            .enumerate()
            .map(|(index, root)| TreePlacement {
                seed_ordinal: u32::try_from(index).unwrap(),
                terrain_vertex: u32::try_from(index).unwrap(),
                anchor: root / ISLAND_WORLD_METRES,
                yaw_radians: 0.,
                scale: 1.,
                prototype: 0,
            })
            .collect::<Vec<_>>();
        roots.push(TreePlacement {
            seed_ordinal: 999,
            terrain_vertex: 999,
            anchor: Vec3::new(675., 675., 30.) / ISLAND_WORLD_METRES,
            yaw_radians: 0.,
            scale: 1.,
            prototype: 0,
        });
        ForestMeshes {
            placements: roots,
            ..ForestMeshes::default()
        }
    }

    #[test]
    fn forest_edge_shrubs_are_repeatable_and_avoid_the_grove_interior() {
        let terrain = Terrain::new(rectangle(Vec2::splat(420.), Vec2::splat(700.), [30.; 2]));
        let forest = grove();
        let water = Mesh::default();
        let caves = CaveSet::default();
        let roots = |seed| {
            scatter(
                seed,
                ShrubSurface {
                    terrain: &terrain,
                    water: &water,
                    forest: &forest,
                    caves: &caves,
                    boulders: &[],
                },
            )
        };
        let shrubs = roots(42);
        assert!(shrubs.len() >= 10);
        assert_eq!(shrubs, roots(42));
        assert_ne!(shrubs, roots(69));
        for (index, &root) in shrubs.iter().enumerate() {
            let point = metres_xy(root);
            let inside = point.min_element() >= 516. && point.max_element() <= 564.;
            assert!(!inside);
            assert!(point.distance(Vec2::splat(675.)) > NEIGHBOUR_REACH_METRES);
            assert!((root.z * ISLAND_WORLD_METRES - 30.).abs() < 0.001);
            assert!(shrubs[..index].iter().all(|&other| {
                point.distance(metres_xy(other)) + 0.001 >= MINIMUM_SPACING_METRES
            }));
        }
    }

    #[test]
    fn forest_edge_shrubs_keep_their_footprint_off_water_and_steep_ground() {
        let min = Vec2::splat(420.);
        let max = Vec2::splat(700.);
        let flat = Terrain::new(rectangle(min, max, [30.; 2]));
        let steep = Terrain::new(rectangle(min, max, [30., 300.]));
        let wet = Terrain::new(rectangle(min, max, [0.5; 2]));
        let water = rectangle(Vec2::new(470., 470.), Vec2::new(545., 620.), [31.; 2]);
        let forest = grove();
        let caves = CaveSet::default();
        let roots = |terrain| {
            scatter(
                42,
                ShrubSurface {
                    terrain,
                    water: &water,
                    forest: &forest,
                    caves: &caves,
                    boulders: &[],
                },
            )
        };
        let shrubs = roots(&flat);
        assert!(!shrubs.is_empty());
        assert!(
            shrubs
                .iter()
                .all(|&root| metres_xy(root).x > 545. + SHRUB_RADIUS_METRES)
        );
        assert!(roots(&steep).is_empty());
        assert!(roots(&wet).is_empty());
    }
}
