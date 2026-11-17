//! Conforming local refinement of the resident terrain before any town exports.
use super::{Island, Terrain, TriangleIndex};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, mesh::EdgeSplitStencil, roads::RoadNetwork};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ops::{Add, Mul},
};

const COASTAL_COLLAR_METRES: f32 = 30.;
const SEAWARD_MARGIN_METRES: f32 = 2.;
const PAVEMENT_EDGE_MAX_METRES: f32 = 0.4;
const PAVEMENT_EDGE_MIN_METRES: f32 = 0.125;
const SHOULDER_EDGE_METRES: f32 = 0.8;
const MAXIMUM_NATIVE_CUT_METRES: f32 = 4.;
const MAXIMUM_SUBGRID_RAISE_METRES: f32 = 0.25;
const MAXIMUM_ADDED_VERTICES: usize = 300_000;
const CORRIDOR_INDEX_SIZE: usize = 64;

/// An island-local XY metre rectangle reserved from town earthworks.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TownProtectedTerrain {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl TownProtectedTerrain {
    #[must_use]
    pub fn is_valid(self) -> bool {
        let (min, max) = self.bounds();
        min.is_finite() && max.is_finite() && min.cmplt(max).all()
    }

    fn bounds(self) -> (Vec2, Vec2) {
        (Vec2::from_array(self.min), Vec2::from_array(self.max))
    }

    fn new(min: Vec2, max: Vec2) -> Self {
        Self {
            min: min.to_array(),
            max: max.to_array(),
        }
    }
}

impl Island {
    /// The fixed tile collar and cave approach/roof support cannot be regraded.
    #[must_use]
    pub fn town_protected_terrain(&self) -> Vec<TownProtectedTerrain> {
        let size = ISLAND_WORLD_METRES;
        let mut areas = vec![
            TownProtectedTerrain::new(Vec2::ZERO, Vec2::new(COASTAL_COLLAR_METRES, size)),
            TownProtectedTerrain::new(
                Vec2::new(size - COASTAL_COLLAR_METRES, 0.),
                Vec2::splat(size),
            ),
            TownProtectedTerrain::new(
                Vec2::new(0., size - COASTAL_COLLAR_METRES),
                Vec2::splat(size),
            ),
            TownProtectedTerrain::new(Vec2::ZERO, Vec2::new(size, SEAWARD_MARGIN_METRES)),
        ];
        let options = self.caves.options;
        for cave in &self.caves.caves {
            let entrance = cave.entrance.truncate();
            let approach = entrance - cave.inward * options.approach_length;
            let mouth_width = options.entrance_width * 0.5 + options.side_margin;
            let min = cave
                .minimum
                .min(entrance.min(approach) - Vec2::splat(mouth_width));
            let max = cave
                .maximum
                .max(entrance.max(approach) + Vec2::splat(mouth_width));
            areas.push(TownProtectedTerrain::new(min, max));
        }
        areas
    }

    /// Grades one road network onto an exclusively owned, pristine island.
    /// All terrain meshes and interpolated fields are staged before replacement.
    ///
    /// # Errors
    /// Rejects invalid or already-applied grades, unsafe cuts, protected areas,
    /// or refinement exceeding the per-mesh vertex budget.
    pub fn apply_town_roads(&mut self, roads: &RoadNetwork) -> Result<(), String> {
        roads.validate()?;
        if roads.is_empty() {
            return Ok(());
        }
        let Some((min, max)) = roads.bounds() else {
            return Ok(());
        };
        if self.terrain.road_sampling_bounds.is_some() {
            return Err("road terrain has already been prepared on this island".into());
        }
        if min.min_element() <= 0. || max.max_element() >= ISLAND_WORLD_METRES {
            return Err("road grading must stay inside this island tile".into());
        }
        let corridor = CorridorIndex::new(roads, min, max);
        self.validate_road_corridor(&corridor)?;

        let GradedMesh { mesh, additions } =
            grade_mesh(self.terrain.mesh(), roads, &corridor, true)?;
        let [first, second] = &self.coarser_lods;
        let lod1 = grade_mesh(first, roads, &corridor, false)?.mesh;
        let lod2 = grade_mesh(second, roads, &corridor, false)?.mesh;
        let index = TriangleIndex::new(&mesh);
        let bounds = [min, max].map(|point| point / ISLAND_WORLD_METRES);
        let mut terrain = Terrain::with_index(mesh, index);
        terrain.road_sampling_bounds = Some(bounds);

        extend_field(&mut self.material.values, &additions);
        extend_field(&mut self.environment.values, &additions);
        extend_field(&mut self.distance_to_land, &additions);
        extend_field(&mut self.submerged_river_carve, &additions);
        mask_paving(&mut self.environment.values, terrain.vertices(), &corridor);
        self.terrain = terrain;
        self.coarser_lods = [lod1, lod2];
        #[cfg(feature = "surface-atlas")]
        {
            self.surface_atlases = super::surface_atlas::SurfaceAtlasCache::default();
        }
        Ok(())
    }

    fn validate_road_corridor(&self, corridor: &CorridorIndex) -> Result<(), String> {
        if self.town_protected_terrain().iter().any(|area| {
            let (min, max) = area.bounds();
            corridor.overlaps(min, max, false)
        }) {
            return Err("road grading crosses a tile seam or protected cave terrain".into());
        }
        for river in &self.rivers {
            let widths = river.target_half_widths(
                self.options.river_source_width_metres,
                self.options.river_maximum_width_metres,
            );
            for (i, pair) in river.nodes.windows(2).enumerate() {
                let [a, b] = [pair[0].position, pair[1].position]
                    .map(|point| point.truncate() * ISLAND_WORLD_METRES);
                let radius = widths[i].max(widths[i + 1]);
                let margin = Vec2::splat(radius);
                let min = a.min(b) - margin;
                let max = a.max(b) + margin;
                if corridor.candidates(min, max).any(|road| {
                    road.overlaps(min, max, false)
                        && segment_distance_squared(road.start, road.end, a, b)
                            <= (road.radius + radius).powi(2)
                }) {
                    return Err("road grading crosses an existing river channel".into());
                }
            }
        }
        Ok(())
    }
}

struct GradedMesh {
    mesh: Mesh,
    additions: Vec<EdgeSplitStencil>,
}

fn grade_mesh(
    source: &Mesh,
    roads: &RoadNetwork,
    corridor: &CorridorIndex,
    validate_grade: bool,
) -> Result<GradedMesh, String> {
    // A working copy keeps the base topology available for rollback, preserves
    // the original vertex indices, and retains the source seam normals.
    let mut mesh = source.clone();
    let mut additions = Vec::new();
    loop {
        let edges = refinement_edges(&mesh, corridor);
        if edges.is_empty() {
            break;
        }
        if mesh.vertices.len() - source.vertices.len() + edges.len() > MAXIMUM_ADDED_VERTICES {
            return Err("road grading exceeded the localized refinement budget".into());
        }
        let splits = mesh.split_edges(&edges);
        if validate_grade {
            additions.extend(splits);
        }
    }
    for point in &mut mesh.vertices {
        let location = point.truncate() * ISLAND_WORLD_METRES;
        if !corridor.overlaps(location, location, false) {
            continue;
        }
        let natural = point.z * ISLAND_WORLD_METRES;
        // A coarser source can miss an entire low bank beneath a dry native
        // road. Render LODs inherit the grade already validated against LOD0.
        let maximum_raise = if validate_grade {
            MAXIMUM_SUBGRID_RAISE_METRES
        } else {
            f32::INFINITY
        };
        let graded = roads.terrain_support_height(location, natural, maximum_raise);
        if validate_grade {
            if roads.surface_height(location).is_some() && (natural <= 0. || graded <= 0.) {
                return Err("road pavement reaches below sea level on the native terrain".into());
            }
            if natural - graded > MAXIMUM_NATIVE_CUT_METRES {
                return Err(format!(
                    "road grading exceeds the native cut limit at {:.2}, {:.2} metres",
                    location.x, location.y
                ));
            }
        }
        if (natural > 0. || !validate_grade)
            && graded > 0.
            && (graded - natural).abs() > f32::EPSILON
        {
            point.z = graded / ISLAND_WORLD_METRES;
        }
    }
    mesh.calculate_normals();
    for (normal, (&position, &original)) in mesh
        .normals
        .iter_mut()
        .zip(source.vertices.iter().zip(&source.normals))
    {
        let point = position.truncate() * ISLAND_WORLD_METRES;
        let padding = Vec2::splat(SHOULDER_EDGE_METRES);
        if !corridor.overlaps(point - padding, point + padding, false) {
            *normal = original;
        }
    }
    Ok(GradedMesh { mesh, additions })
}

fn refinement_edges(mesh: &Mesh, corridor: &CorridorIndex) -> BTreeMap<(u32, u32), f32> {
    let mut edges = BTreeMap::new();
    for face in mesh.triangles.chunks_exact(3) {
        let [a, b, c] = [face[0], face[1], face[2]];
        let [p, q, r] =
            [a, b, c].map(|id| mesh.vertices[id as usize].truncate() * ISLAND_WORLD_METRES);
        let min = p.min(q).min(r);
        let max = p.max(q).max(r);
        if !corridor.overlaps(min, max, false) {
            continue;
        }
        let limit = corridor.refinement_limit(min, max);
        for (first, second, start, end) in [(a, b, p, q), (b, c, q, r), (c, a, r, p)] {
            if start.distance_squared(end) <= limit * limit || on_tile_edge(start, end) {
                continue;
            }
            edges.insert((first.min(second), first.max(second)), 0.5);
        }
    }
    edges
}

fn on_tile_edge(start: Vec2, end: Vec2) -> bool {
    (start.x == 0. && end.x == 0.)
        || (start.y == 0. && end.y == 0.)
        || (start.x >= ISLAND_WORLD_METRES && end.x >= ISLAND_WORLD_METRES)
        || (start.y >= ISLAND_WORLD_METRES && end.y >= ISLAND_WORLD_METRES)
}

fn extend_field<T>(values: &mut Vec<T>, splits: &[EdgeSplitStencil])
where
    T: Copy + Add<Output = T> + Mul<f32, Output = T>,
{
    values.reserve(splits.len());
    for split in splits {
        let [a, b] = split.edge.map(|index| index as usize);
        debug_assert_eq!(values.len(), split.vertex as usize);
        values.push(values[a] * 0.5 + values[b] * 0.5);
    }
}

fn mask_paving(environment: &mut [Vec2], vertices: &[crate::Vec3], corridor: &CorridorIndex) {
    for (value, vertex) in environment.iter_mut().zip(vertices) {
        let point = vertex.truncate() * ISLAND_WORLD_METRES;
        let coverage = corridor.paving_coverage(point);
        // Stone coverage displaces grass-eligible dirt in the ground shaders.
        // Leave the terrain's material, particularly river-bed weight, alone.
        if coverage > 0. {
            *value = value.lerp(Vec2::Y, coverage);
        }
    }
}

#[derive(Clone, Copy)]
struct Corridor {
    start: Vec2,
    end: Vec2,
    axis: Vec2,
    length: f32,
    radius: f32,
    core_radius: f32,
    shoulder: f32,
}

impl Corridor {
    fn new(start: Vec2, end: Vec2, core_radius: f32, shoulder: f32) -> Self {
        let direction = end - start;
        Self {
            start,
            end,
            axis: direction.try_normalize().unwrap_or(Vec2::X),
            length: direction.length(),
            radius: core_radius + shoulder,
            core_radius,
            shoulder,
        }
    }

    fn bounds(self, radius: f32) -> (Vec2, Vec2) {
        (
            self.start.min(self.end) - Vec2::splat(radius),
            self.start.max(self.end) + Vec2::splat(radius),
        )
    }

    /// Conservative rectangle overlap against a segment capsule. The oriented
    /// bounds avoid refining an entire diagonal strip's axis-aligned box.
    fn overlaps(self, min: Vec2, max: Vec2, core: bool) -> bool {
        let radius = if core { self.core_radius } else { self.radius };
        let (road_min, road_max) = self.bounds(radius);
        if min.cmpgt(road_max).any() || max.cmplt(road_min).any() {
            return false;
        }
        let half = (max - min) * 0.5;
        let centre = (min + max) * 0.5 - self.start;
        let normal = self.axis.perp();
        let along = centre.dot(self.axis);
        let along_half = half.dot(self.axis.abs());
        let extension = if self.length > 0. {
            if core { 0. } else { self.shoulder }
        } else {
            radius
        };
        along + along_half >= -extension
            && along - along_half <= self.length + extension
            && centre.dot(normal).abs() <= radius + half.dot(normal.abs())
    }

    fn contains_core(self, min: Vec2, max: Vec2) -> bool {
        let corners = [min, Vec2::new(min.x, max.y), Vec2::new(max.x, min.y), max];
        if self.length == 0. {
            return corners
                .into_iter()
                .all(|p| p.distance_squared(self.start) <= self.core_radius.powi(2));
        }
        corners.into_iter().all(|p| {
            let offset = p - self.start;
            (0. ..=self.length).contains(&offset.dot(self.axis))
                && offset.dot(self.axis.perp()).abs() <= self.core_radius
        })
    }

    fn paving_coverage(self, point: Vec2) -> f32 {
        let offset = point - self.start;
        let distance = if self.length > 0. {
            let along = offset.dot(self.axis);
            Vec2::new(
                (offset.dot(self.axis.perp()).abs() - self.core_radius).max(0.),
                (along - along.clamp(0., self.length)).abs(),
            )
            .length()
        } else {
            (offset.length() - self.core_radius).max(0.)
        };
        let transition = self
            .shoulder
            .clamp(PAVEMENT_EDGE_MIN_METRES, SHOULDER_EDGE_METRES);
        let apron = transition * 0.5;
        1. - ((distance - apron) / apron).clamp(0., 1.)
    }
}

struct CorridorIndex {
    roads: Vec<Corridor>,
    bins: Vec<Vec<usize>>,
    min: Vec2,
    max: Vec2,
}

impl CorridorIndex {
    fn new(network: &RoadNetwork, min: Vec2, max: Vec2) -> Self {
        let roads = network
            .strips
            .iter()
            .map(|strip| {
                let start = strip.stations[0].point;
                let end = strip.stations[strip.stations.len() - 1].point;
                Corridor::new(start, end, strip.half_width, network.shoulder)
            })
            .chain(
                network
                    .junctions
                    .iter()
                    .filter(|junction| junction.radius > 0.)
                    .map(|junction| {
                        Corridor::new(
                            junction.point,
                            junction.point,
                            junction.radius,
                            network.shoulder,
                        )
                    }),
            )
            .collect::<Vec<_>>();
        let mut bins = vec![Vec::new(); CORRIDOR_INDEX_SIZE * CORRIDOR_INDEX_SIZE];
        for (index, road) in roads.iter().enumerate() {
            let (a, b) = road.bounds(road.radius);
            for bin in box_bins(a, b) {
                bins[bin].push(index);
            }
        }
        Self {
            roads,
            bins,
            min,
            max,
        }
    }

    fn candidates(&self, min: Vec2, max: Vec2) -> impl Iterator<Item = Corridor> + '_ {
        let overlaps = !min.cmpgt(self.max).any() && !max.cmplt(self.min).any();
        overlaps
            .then(|| box_bins(min, max))
            .into_iter()
            .flatten()
            .flat_map(|bin| self.bins[bin].iter().map(|&index| self.roads[index]))
    }

    fn overlaps(&self, min: Vec2, max: Vec2, core: bool) -> bool {
        self.candidates(min, max)
            .any(|road| road.overlaps(min, max, core))
    }

    fn refinement_limit(&self, min: Vec2, max: Vec2) -> f32 {
        self.candidates(min, max)
            .filter(|road| road.overlaps(min, max, true) && !road.contains_core(min, max))
            .map(|road| {
                (road.shoulder / 4.).clamp(PAVEMENT_EDGE_MIN_METRES, PAVEMENT_EDGE_MAX_METRES)
            })
            .fold(SHOULDER_EDGE_METRES, f32::min)
    }

    fn paving_coverage(&self, point: Vec2) -> f32 {
        self.candidates(point, point)
            .map(|road| road.paving_coverage(point))
            .fold(0., f32::max)
    }
}

fn box_bins(min: Vec2, max: Vec2) -> impl Iterator<Item = usize> {
    let bin = |value| super::bin_coordinate(value / ISLAND_WORLD_METRES, CORRIDOR_INDEX_SIZE);
    let x = bin(min.x)..=bin(max.x);
    (bin(min.y)..=bin(max.y)).flat_map(move |y| x.clone().map(move |x| y * CORRIDOR_INDEX_SIZE + x))
}

fn segment_distance_squared(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> f32 {
    let u = b - a;
    let v = d - c;
    let cross = u.perp_dot(v);
    if cross.abs() > f32::EPSILON {
        let delta = c - a;
        let range = 0.0..=1.0;
        if range.contains(&(delta.perp_dot(v) / cross))
            && range.contains(&(delta.perp_dot(u) / cross))
        {
            return 0.;
        }
    }
    let distance = |p: Vec2, start: Vec2, direction: Vec2| {
        let t = (p - start).dot(direction) / direction.length_squared().max(f32::EPSILON);
        p.distance_squared(start + direction * t.clamp(0., 1.))
    };
    distance(a, c, v)
        .min(distance(b, c, v))
        .min(distance(c, a, u))
        .min(distance(d, a, u))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ForestOptions,
        roads::{RoadStation, RoadStrip},
    };

    fn road(start: Vec2, end: Vec2, height: f32) -> RoadNetwork {
        RoadNetwork {
            strips: vec![RoadStrip {
                source: 0,
                start: 0,
                end: 1,
                half_width: 2.,
                stations: vec![
                    RoadStation {
                        point: start,
                        height,
                    },
                    RoadStation { point: end, height },
                ],
            }],
            junctions: Vec::new(),
            shoulder: 2.,
        }
    }

    fn fixture() -> Island {
        Island::shared_forest_fixture(ForestOptions::default())
    }

    #[test]
    fn a_road_inside_coarse_faces_refines_collision_and_render_meshes() {
        let mut island = fixture();
        let old_vertex_count = island.terrain().vertex_count();
        let route = road(Vec2::new(645., 646.), Vec2::new(674., 648.), 39.);
        let station = route.strips[0].stations[0]
            .point
            .lerp(route.strips[0].stations[1].point, 0.6);
        let direction = (route.strips[0].stations[1].point - route.strips[0].stations[0].point)
            .normalize()
            .perp();
        let far = Vec2::new(732., 741.) / ISLAND_WORLD_METRES;
        let old_height = island.terrain().sample_height_map(far.x, far.y);

        island.apply_town_roads(&route).unwrap();
        let vertex_count = island.terrain().vertex_count();
        assert!(vertex_count > old_vertex_count);
        assert_eq!(island.material.values.len(), vertex_count);
        assert_eq!(island.environment.values.len(), vertex_count);
        assert_eq!(island.distance_to_land.len(), vertex_count);
        assert_eq!(island.submerged_river_carve.len(), vertex_count);
        assert!((island.terrain().sample_height_map(far.x, far.y) - old_height).abs() < 1.0e-7);
        for offset in [-1.99, -1.5, 0., 1.5, 1.99, 3., 4.5] {
            let location = station + direction * offset;
            let point = location / ISLAND_WORLD_METRES;
            let expected = route.graded_height(location, 40.);
            let fine = island.terrain();
            let hit = fine.sample_accurate_surface(point).unwrap().position.z;
            assert!((hit * ISLAND_WORLD_METRES - expected).abs() < 0.07);
            assert!((fine.sample_height_map(point.x, point.y) - hit).abs() < 1.0e-6);
            assert!((fine.sample(point.x, point.y) - hit).abs() < 1.0e-6);
            let environment = island.environment.sample(fine, point);
            assert!(environment.x.abs() < 1.0e-6);
            if offset.abs() < 2. {
                assert!(
                    environment.y > 0.98,
                    "paving must suppress grass at {offset}"
                );
            }
            if offset > 4. {
                assert!(
                    environment.y < 0.02,
                    "unaltered terrain retains its ground cover"
                );
            }
            for mesh in &island.coarser_lods {
                let value = Terrain::new(mesh.clone())
                    .sample_accurate_surface(point)
                    .unwrap();
                assert!((value.position.z - hit).abs() < 1.0e-7);
            }
        }
        assert!(island.apply_town_roads(&route).is_err());
        assert_eq!(
            island
                .save("graded-island-should-never-be-created.snapshot")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn rejecting_tile_seams_and_excessive_cut_keeps_the_pristine_island() {
        let mut island = fixture();
        let before = island.clone();
        let tile_seam = road(Vec2::new(26., 646.), Vec2::new(26., 660.), 39.);
        let excessive_cut = road(Vec2::new(645., 646.), Vec2::new(674., 648.), 5.);
        assert!(
            island
                .apply_town_roads(&tile_seam)
                .unwrap_err()
                .contains("tile seam")
        );
        assert!(
            island
                .apply_town_roads(&excessive_cut)
                .unwrap_err()
                .contains("cut limit")
        );
        assert!(island == before);
    }
}
