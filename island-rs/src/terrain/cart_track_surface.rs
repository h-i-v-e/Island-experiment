//! Terrain surface coverage and shallow cuts along dry cart-track alignments.
use std::collections::BTreeMap;

use super::{
    Terrain,
    cart_tracks::{CartTrackBridge, CartTrackRun},
};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2, mesh::EdgeSplitStencil};

const BIN_COUNT: usize = 128;
const WHEEL_OFFSET_METRES: f32 = 0.60;
const RUT_CORE_RADIUS_METRES: f32 = 0.05;
const RUT_RADIUS_METRES: f32 = 0.27;
const TRACK_HALF_WIDTH_METRES: f32 = WHEEL_OFFSET_METRES + RUT_RADIUS_METRES;
const END_FADE_METRES: f32 = 1.5;
const MAX_RUT_CUT: f32 = 0.30 / ISLAND_WORLD_METRES;
const MAXIMUM_CROSS_EDGE_METRES: f32 = 0.19;
const MAXIMUM_ALONG_EDGE_METRES: f32 = 2.0;
const MAXIMUM_ADDED_VERTICES: usize = 320_000;
const SEAM_ENDPOINT_TOLERANCE_METRES: f32 = 0.001;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    run: usize,
    start: Vec2,
    min: Vec2,
    max: Vec2,
    axis: Vec2,
    length: f32,
    distance_before: f32,
    distance_after: f32,
    paint_through_ends: [bool; 2],
}

impl Segment {
    fn new(run: usize, start: Vec2, end: Vec2) -> Option<Self> {
        let in_tile = |point: Vec2| {
            point.is_finite()
                && point.min_element() >= 0.
                && point.max_element() <= ISLAND_WORLD_METRES
        };
        let delta = end - start;
        let length = delta.length();
        let radius = Vec2::splat(TRACK_HALF_WIDTH_METRES);
        (in_tile(start) && in_tile(end) && length > f32::EPSILON).then(|| Self {
            run,
            start,
            min: start.min(end) - radius,
            max: start.max(end) + radius,
            axis: delta / length,
            length,
            distance_before: 0.,
            distance_after: 0.,
            paint_through_ends: [false; 2],
        })
    }

    fn overlaps(self, min: Vec2, max: Vec2) -> bool {
        let normal = self.axis.perp();
        let half = (max - min) * 0.5;
        let center = (min + max) * 0.5 - self.start;
        let along = center.dot(self.axis);
        let along_half = half.dot(self.axis.abs());
        along + along_half >= -TRACK_HALF_WIDTH_METRES
            && along - along_half <= self.length + TRACK_HALF_WIDTH_METRES
            && center.dot(normal).abs() <= TRACK_HALF_WIDTH_METRES + half.dot(normal.abs())
    }

    fn sample(self, point: Vec2) -> RutSample {
        let offset = point - self.start;
        let along = offset.dot(self.axis).clamp(0., self.length);
        let across = offset - self.axis * along;
        let distances = [
            self.distance_before + along,
            self.distance_after + (self.length - along),
        ];
        let paint_distance = distances
            .into_iter()
            .zip(self.paint_through_ends)
            .filter_map(|(distance, through)| (!through).then_some(distance))
            .fold(f32::INFINITY, f32::min);
        RutSample {
            centre: self.start + self.axis * along,
            across: self.axis.perp_dot(across),
            normal: self.axis.perp(),
            distance_squared: across.length_squared(),
            end_distance: distances[0].min(distances[1]),
            paint_distance,
        }
    }

    fn refinement_edge(self, points: [Vec2; 3], min: Vec2, max: Vec2) -> Option<RefinementEdge> {
        if min.cmpgt(self.max).any() || max.cmplt(self.min).any() {
            return None;
        }
        let offsets = points.map(|point| point - self.start);
        let along = offsets.map(|offset| offset.dot(self.axis));
        if along.iter().all(|&value| value < -TRACK_HALF_WIDTH_METRES)
            || along
                .iter()
                .all(|&value| value > self.length + TRACK_HALF_WIDTH_METRES)
        {
            return None;
        }
        let across = offsets.map(|offset| offset.dot(self.axis.perp()));
        if ![-WHEEL_OFFSET_METRES, WHEEL_OFFSET_METRES]
            .into_iter()
            .any(|wheel| {
                across
                    .iter()
                    .any(|&value| value >= wheel - RUT_RADIUS_METRES)
                    && across
                        .iter()
                        .any(|&value| value <= wheel + RUT_RADIUS_METRES)
            })
        {
            return None;
        }

        [[0, 1], [1, 2], [2, 0]]
            .into_iter()
            .filter(|&[a, b]| !on_tile_edge(points[a], points[b]))
            .map(|vertices @ [a, b]| RefinementEdge {
                vertices,
                score: ((across[b] - across[a]).abs() / MAXIMUM_CROSS_EDGE_METRES)
                    .max((along[b] - along[a]).abs() / MAXIMUM_ALONG_EDGE_METRES),
            })
            .filter(|edge| edge.score > 1.)
            .max_by(|a, b| a.score.total_cmp(&b.score))
    }
}

#[derive(Clone, Copy)]
struct RefinementEdge {
    vertices: [usize; 2],
    score: f32,
}

#[allow(clippy::float_cmp)] // Domain boundaries have exact, shared positions.
fn on_tile_edge(start: Vec2, end: Vec2) -> bool {
    let same = start.cmpeq(end);
    let border = start.cmpeq(Vec2::ZERO) | start.cmpeq(Vec2::splat(ISLAND_WORLD_METRES));
    (same & border).any()
}

#[derive(Clone, Copy)]
struct RutSample {
    centre: Vec2,
    across: f32,
    normal: Vec2,
    distance_squared: f32,
    end_distance: f32,
    paint_distance: f32,
}

impl RutSample {
    fn coverage(self) -> f32 {
        wheel_profile(self.distance_squared.sqrt())
            * smooth_step(self.paint_distance / END_FADE_METRES)
    }

    fn cut(self, terrain: &Terrain) -> f32 {
        let weight =
            wheel_profile(self.across.abs()) * smooth_step(self.end_distance / END_FADE_METRES);
        if weight <= 0. {
            return 0.;
        }
        let offset = self.normal * (WHEEL_OFFSET_METRES * self.across.signum());
        let [here, opposite] = [self.centre + offset, self.centre - offset].map(|point| {
            let point = point / ISLAND_WORLD_METRES;
            terrain.sample_height_map(point.x, point.y)
        });
        (here - opposite).clamp(0., MAX_RUT_CUT) * weight
    }
}

fn wheel_profile(distance: f32) -> f32 {
    let offset = (distance - WHEEL_OFFSET_METRES).abs();
    1. - smooth_step(
        (offset - RUT_CORE_RADIUS_METRES) / (RUT_RADIUS_METRES - RUT_CORE_RADIUS_METRES),
    )
}

fn smooth_step(value: f32) -> f32 {
    let t = value.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// Spatial index of island-local, metre-scale alignments. Queries use normalized
/// terrain XY and measure wheel offsets from the nearest point on each run.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct CartTrackSurface {
    segments: Vec<Segment>,
    bins: BTreeMap<usize, Vec<usize>>,
    seam_crossings: [Vec<f32>; 2],
}

impl CartTrackSurface {
    #[allow(clippy::float_cmp)] // Bridge endpoints are copied from the associated final dry run.
    pub(super) fn new(runs: &[CartTrackRun], bridges: &[CartTrackBridge]) -> Self {
        let mut track = Self::default();
        for (run, alignment) in runs.iter().enumerate() {
            let points = &alignment.points;
            let [Some(first), Some(last)] = [points.first(), points.last()] else {
                continue;
            };
            let start_index = track.segments.len();
            let start_crossing = seam_endpoint(*first);
            let end_crossing = seam_endpoint(*last);
            // Wheel ruts meet the deck at the exact endpoints of a fitted
            // bridge on this route. Other dry terminals keep their usual fade.
            let link_bridges = || {
                bridges
                    .iter()
                    .filter(|b| b.link_index == alignment.link_index)
            };
            let paint_through_ends = [
                alignment.joins_track
                    || start_crossing.is_some()
                    || link_bridges().any(|b| b.end == *first),
                end_crossing.is_some() || link_bridges().any(|b| b.start == *last),
            ];
            let mut length = 0.;
            for pair in points.windows(2) {
                let [start, end] = [pair[0], pair[1]].map(|p| Vec2::new(p[0], p[1]));
                if let Some(mut segment) = Segment::new(run, start, end) {
                    segment.distance_before = length;
                    segment.paint_through_ends = paint_through_ends;
                    length += segment.length;
                    track.segments.push(segment);
                }
            }
            if track.segments.len() == start_index {
                continue;
            }
            for (side, y) in [start_crossing, end_crossing].into_iter().flatten() {
                track.seam_crossings[side].push(y);
            }
            let mut distance_after = 0.;
            for segment in track.segments[start_index..].iter_mut().rev() {
                segment.distance_after = distance_after;
                distance_after += segment.length;
            }
        }
        for (index, segment) in track.segments.iter().enumerate() {
            for bin in box_bins(segment.min, segment.max) {
                let (bin_min, bin_max) = bin_bounds(bin);
                if segment.overlaps(bin_min, bin_max) {
                    track.bins.entry(bin).or_default().push(index);
                }
            }
        }
        track
    }

    pub(super) fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub(super) fn corridor_overlaps(&self, min: Vec2, max: Vec2) -> bool {
        self.candidates(min, max)
            .any(|segment| segment.overlaps(min, max))
    }

    pub(super) fn coverage_at(&self, point: Vec2) -> f32 {
        let point = point * ISLAND_WORLD_METRES;
        if let Some(side) = seam_side(point.x) {
            return self.seam_crossings[side]
                .iter()
                .map(|y| wheel_profile((point.y - y).abs()))
                .fold(0., f32::max);
        }
        self.nearest_runs(point)
            .map(RutSample::coverage)
            .fold(0., f32::max)
    }

    fn nearest_runs(&self, point: Vec2) -> impl Iterator<Item = RutSample> + '_ {
        self.bins
            .get(&point_bin(point))
            .into_iter()
            .flat_map(move |indices| {
                indices
                    .chunk_by(|&a, &b| self.segments[a].run == self.segments[b].run)
                    .filter_map(move |run| {
                        run.iter()
                            .map(|&index| self.segments[index].sample(point))
                            .min_by(|a, b| a.distance_squared.total_cmp(&b.distance_squared))
                    })
            })
    }

    pub(super) fn rut_cut(&self, terrain: &Terrain, xy: Vec2, height: f32) -> f32 {
        let point = xy * ISLAND_WORLD_METRES;
        let seam_distance = point
            .min(Vec2::splat(ISLAND_WORLD_METRES) - point)
            .min_element();
        if height <= 0. || seam_distance <= 0. {
            return 0.;
        }
        let depth = self
            .nearest_runs(point)
            .map(|sample| sample.cut(terrain))
            .fold(0., f32::max);
        if depth <= 0. {
            return 0.;
        }
        let pavement = terrain.road_coverage_at(xy);
        let depth = depth * (1. - pavement) * smooth_step(seam_distance / END_FADE_METRES);
        depth.min(height)
    }

    pub(super) fn rutted_mesh(&self, terrain: &Terrain) -> Result<Option<Mesh>, &'static str> {
        let source = terrain.mesh();
        let mut refined = self.refined_mesh(source)?.map(|(mesh, _)| mesh);
        let mesh = refined.as_ref().unwrap_or(source);
        let cuts: Vec<_> = mesh
            .vertices
            .iter()
            .map(|point| self.rut_cut(terrain, point.truncate(), point.z))
            .collect();
        if cuts.iter().any(|&cut| cut > 0.) {
            let mesh = refined.get_or_insert_with(|| source.clone());
            lower_rut_vertices(mesh, &cuts);
        }
        Ok(refined)
    }

    /// Splits only render faces around the wheel ruts and retains the original
    /// terrain surface. The edge stencils can interpolate per-vertex attributes
    /// held separately from the mesh.
    pub(super) fn refined_mesh(
        &self,
        source: &Mesh,
    ) -> Result<Option<(Mesh, Vec<EdgeSplitStencil>)>, &'static str> {
        if self.is_empty() {
            return Ok(None);
        }
        let mut edges = self.refinement_edges(source);
        if edges.is_empty() {
            return Ok(None);
        }
        let mut mesh = source.clone();
        let mut normals = std::mem::take(&mut mesh.normals);
        let mut stencils = Vec::new();
        while !edges.is_empty() {
            if stencils.len() + edges.len() > MAXIMUM_ADDED_VERTICES {
                return Err("cart track exceeds render mesh refinement budget");
            }
            let splits = mesh.split_edges(&edges);
            if !normals.is_empty() {
                normals.reserve(splits.len());
                for split in &splits {
                    let [a, b] = split.edge.map(|index| index as usize);
                    normals.push(normals[a].lerp(normals[b], split.interpolation));
                }
            }
            stencils.extend(splits);
            edges = self.refinement_edges(&mesh);
        }
        mesh.normals = normals;
        Ok(Some((mesh, stencils)))
    }

    fn refinement_edges(&self, mesh: &Mesh) -> BTreeMap<(u32, u32), f32> {
        let mut edges = BTreeMap::new();
        for face in mesh.triangles.chunks_exact(3) {
            let vertices = [face[0], face[1], face[2]];
            let positions = vertices.map(|index| mesh.vertices[index as usize]);
            if positions.iter().all(|position| position.z <= 0.) {
                continue;
            }
            let points = positions.map(|position| position.truncate() * ISLAND_WORLD_METRES);
            let [a, b, c] = points;
            let min = a.min(b).min(c);
            let max = a.max(b).max(c);
            if min.distance_squared(max) <= MAXIMUM_CROSS_EDGE_METRES.powi(2) {
                continue;
            }
            // Choose each shared midpoint solely from the edge and crossing
            // latitude so adjoining islands add identical seam samples.
            for [a, b] in [[0, 1], [1, 2], [2, 0]] {
                if self.split_seam_edge(points[a], points[b]) {
                    let [a, b] = [vertices[a], vertices[b]];
                    edges.insert((a.min(b), a.max(b)), 0.5);
                }
            }
            let edge = self
                .candidates(min, max)
                .filter_map(|segment| segment.refinement_edge(points, min, max))
                .max_by(|a, b| a.score.total_cmp(&b.score));
            if let Some(edge) = edge {
                let [a, b] = edge.vertices.map(|vertex| vertices[vertex]);
                edges.insert((a.min(b), a.max(b)), 0.5);
            }
        }
        edges
    }

    fn split_seam_edge(&self, a: Vec2, b: Vec2) -> bool {
        if a.x.to_bits() != b.x.to_bits() || (a.y - b.y).abs() <= MAXIMUM_CROSS_EDGE_METRES {
            return false;
        }
        seam_side(a.x).is_some_and(|side| {
            self.seam_crossings[side].iter().any(|&y| {
                a.y.min(b.y) <= y + TRACK_HALF_WIDTH_METRES
                    && a.y.max(b.y) >= y - TRACK_HALF_WIDTH_METRES
            })
        })
    }

    fn candidates(&self, min: Vec2, max: Vec2) -> impl Iterator<Item = Segment> + '_ {
        box_bins(min, max)
            .filter_map(|bin| self.bins.get(&bin))
            .flat_map(|indices| indices.iter().map(|&index| self.segments[index]))
    }
}

fn lower_rut_vertices(mesh: &mut Mesh, cuts: &[f32]) {
    for (point, depth) in mesh.vertices.iter_mut().zip(cuts) {
        point.z -= depth;
    }
    if mesh.normals.is_empty() {
        return;
    }

    let mut affected = vec![false; mesh.vertices.len()];
    for face in mesh.triangles.chunks_exact(3) {
        if face.iter().any(|&index| cuts[index as usize] > 0.) {
            for &index in face {
                affected[index as usize] = true;
            }
        }
    }
    let previous_normals = std::mem::take(&mut mesh.normals);
    mesh.calculate_normals();
    for (index, (new, old)) in mesh.normals.iter_mut().zip(previous_normals).enumerate() {
        let xy = mesh.vertices[index].truncate();
        // The shared border keeps its native height and normal on both islands.
        if !affected[index] || xy.min_element() <= 0. || xy.max_element() >= 1. {
            *new = old;
        }
    }
}

fn seam_side(x: f32) -> Option<usize> {
    [0.0_f32, ISLAND_WORLD_METRES]
        .into_iter()
        .position(|edge| edge.to_bits() == x.to_bits())
}

fn seam_endpoint([x, y, _]: [f32; 3]) -> Option<(usize, f32)> {
    if !(0.0..=ISLAND_WORLD_METRES).contains(&y) {
        return None;
    }
    [0.0_f32, ISLAND_WORLD_METRES]
        .into_iter()
        .position(|edge| (edge - x).abs() <= SEAM_ENDPOINT_TOLERANCE_METRES)
        .map(|side| (side, y))
}

fn point_bin(point: Vec2) -> usize {
    bin_coordinate(point.y) * BIN_COUNT + bin_coordinate(point.x)
}

fn bin_coordinate(value_metres: f32) -> usize {
    super::bin_coordinate(value_metres / ISLAND_WORLD_METRES, BIN_COUNT)
}

fn bin_bounds(index: usize) -> (Vec2, Vec2) {
    let width = ISLAND_WORLD_METRES / BIN_COUNT as f32;
    let min = Vec2::new((index % BIN_COUNT) as f32, (index / BIN_COUNT) as f32) * width;
    (min, min + Vec2::splat(width))
}

fn box_bins(min: Vec2, max: Vec2) -> impl Iterator<Item = usize> {
    let x = bin_coordinate(min.x)..=bin_coordinate(max.x);
    (bin_coordinate(min.y)..=bin_coordinate(max.y))
        .flat_map(move |y| x.clone().map(move |x| y * BIN_COUNT + x))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Vec3,
        terrain::{Terrain, cart_tracks::Crossing},
    };

    fn normalized(x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y) / ISLAND_WORLD_METRES
    }

    fn dry_tracks<'a>(runs: impl IntoIterator<Item = &'a [[f32; 3]]>) -> CartTrackSurface {
        let runs: Vec<_> = runs
            .into_iter()
            .map(|points| CartTrackRun {
                link_index: 0,
                points: points.to_vec(),
                joins_track: false,
            })
            .collect();
        CartTrackSurface::new(&runs, &[])
    }

    #[test]
    fn wheat_clearance_blocks_the_gap_between_the_wheel_ruts() {
        let points = [[40., 100., 6.], [80., 100., 6.]];
        let track = dry_tracks([points.as_slice()]);
        assert!(track.coverage_at(normalized(60., 100.)) < f32::EPSILON);
        assert!(track.corridor_overlaps(Vec2::new(59.8, 99.9), Vec2::new(60.2, 100.1)));
        assert!(!track.corridor_overlaps(Vec2::new(59.8, 101.2), Vec2::new(60.2, 101.4)));
    }

    #[test]
    fn both_wheel_ruts_reach_the_bridge_banks_while_dry_ends_still_fade() {
        let near = [50., 100., 6.];
        let far = [70., 100., 6.];
        let runs = [
            CartTrackRun {
                link_index: 0,
                points: vec![[40., 100., 6.], near],
                joins_track: false,
            },
            CartTrackRun {
                link_index: 0,
                points: vec![far, [80., 100., 6.]],
                joins_track: false,
            },
        ];
        let mut bridges = [CartTrackBridge {
            link_index: 0,
            start: near,
            end: far,
            width: 2.4,
            kind: Crossing::River,
            start_heading: Some([1., 0.]),
            end_heading: Some([1., 0.]),
        }];
        let tracks = CartTrackSurface::new(&runs, &bridges);
        for y in [99.4, 100.6] {
            for x in [49.9, 50., 70., 70.1] {
                assert!(tracks.coverage_at(normalized(x, y)) > 0.99);
            }
            for x in [40., 60., 80.] {
                assert!(tracks.coverage_at(normalized(x, y)) < f32::EPSILON);
            }
        }
        // Coincidence with a bank on another link does not pin a dry terminal.
        bridges[0].link_index = 1;
        let unrelated = CartTrackSurface::new(&runs, &bridges);
        for x in [50., 70.] {
            assert!(unrelated.coverage_at(normalized(x, 100.6)) < f32::EPSILON);
        }
    }

    #[test]
    fn wheel_ruts_keep_a_natural_centre_and_fade_at_the_dry_end() {
        let run = [[30., 30., 1.], [45., 30., 1.], [60., 38., 1.]];
        let tracks = dry_tracks([run.as_slice()]);
        for y in [29.4, 30.6] {
            assert!(tracks.coverage_at(normalized(40., y)) > 0.99);
            assert!(tracks.coverage_at(normalized(30.75, y)) > 0.49);
            assert!(tracks.coverage_at(normalized(30., y)) < f32::EPSILON);
        }
        for y in [29., 30., 31.] {
            assert!(tracks.coverage_at(normalized(42., y)) < f32::EPSILON);
        }
        let bend = Vec2::new(45., 30.);
        let axis = Vec2::new(15., 8.).normalize();
        let across = axis.perp() * WHEEL_OFFSET_METRES;
        assert!(tracks.coverage_at((bend + axis * 2. + across) / ISLAND_WORLD_METRES) > 0.99);
        assert!(tracks.coverage_at((bend + axis * 0.05 - across) / ISLAND_WORLD_METRES) > 0.99);
        assert!(tracks.coverage_at(bend / ISLAND_WORLD_METRES) < f32::EPSILON);
    }

    #[test]
    fn cross_slope_cuts_only_the_higher_rut_and_caps_the_depth() {
        let run = [[34., 40., 6.], [54., 40., 6.]];
        let height = |y, cross_slope| 6. + (y - 40.) * cross_slope;
        let tracks = dry_tracks([run.as_slice()]);
        for (cross_slope, cut) in [(0.15, 0.18), (0.50, 0.30), (-0.50, 0.30), (0., 0.)] {
            let [a, b, c, d] = [(31., 37.), (57., 37.), (57., 43.), (31., 43.)].map(|(x, y)| {
                normalized(x, y).extend(height(y, cross_slope) / ISLAND_WORLD_METRES)
            });
            let terrain = Terrain::new(Mesh {
                vertices: vec![a, b, c, d],
                normals: vec![Vec3::Z; 4],
                triangles: vec![0, 1, 2, 0, 2, 3],
                uv: Vec::new(),
            });
            let rut_side = if cross_slope < 0. { -1. } else { 1. };
            for (x, offset, expected) in [
                (44., rut_side * 0.60, cut),
                (44., -rut_side * 0.60, 0.),
                (44., 0., 0.),
                (44., 1.0, 0.),
                (34., rut_side * 0.60, 0.),
                (34.75, rut_side * 0.60, cut * 0.5),
                (54., rut_side * 0.60, 0.),
            ] {
                let point = normalized(x, 40. + offset);
                let depth =
                    tracks.rut_cut(&terrain, point, terrain.sample_height_map(point.x, point.y));
                assert!((depth * ISLAND_WORLD_METRES - expected).abs() < 0.0001);
            }
            let mesh = tracks.rutted_mesh(&terrain).unwrap().unwrap();
            for point in mesh.vertices {
                let [x, y, z] = (point * ISLAND_WORLD_METRES).to_array();
                let depth = height(y, cross_slope) - z;
                assert!((-0.0001..=cut + 0.0001).contains(&depth));
                if (y - 40.).abs() <= 0.32 || (y - 40.) * rut_side < 0. {
                    assert!(depth.abs() < 0.0001, "{x}, {y}: {depth}");
                }
            }
        }
    }

    #[test]
    fn render_refinement_preserves_the_original_surface_and_uvs() {
        let coordinates = [
            normalized(30., 30.),
            normalized(62., 30.),
            normalized(62., 62.),
            normalized(30., 62.),
        ];
        let source = Mesh {
            vertices: coordinates
                .iter()
                .enumerate()
                .map(|(index, &point)| point.extend((index + 2) as f32 / 1000.))
                .collect(),
            normals: vec![Vec3::Z, Vec3::new(0.1, 0., 1.), Vec3::Z, Vec3::Z],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: coordinates.to_vec(),
        };
        let run = [[34., 43., 2.], [54., 52., 2.]];
        let tracks = dry_tracks([run.as_slice()]);
        let (mesh, stencils) = tracks.refined_mesh(&source).unwrap().unwrap();
        assert!(!stencils.is_empty());
        assert_eq!(&mesh.vertices[..4], &source.vertices);
        assert_eq!(&mesh.normals[..4], &source.normals);
        assert_eq!(&mesh.uv[..4], &source.uv);
        for split in stencils {
            let [a, b] = split.edge.map(|index| index as usize);
            let i = split.vertex as usize;
            let t = split.interpolation;
            assert_eq!(mesh.vertices[i], mesh.vertices[a].lerp(mesh.vertices[b], t));
            assert_eq!(mesh.normals[i], mesh.normals[a].lerp(mesh.normals[b], t));
            assert_eq!(mesh.uv[i], mesh.uv[a].lerp(mesh.uv[b], t));
        }

        let original = Terrain::new(source);
        let mut refined = Terrain::new(mesh);
        let axis = Vec2::new(20., 9.).normalize();
        let across = axis.perp();
        let location = Vec2::new(34., 43.) + axis * 10.;
        for offset in [0., -WHEEL_OFFSET_METRES, WHEEL_OFFSET_METRES, 1.3] {
            let point = (location + across * offset) / ISLAND_WORLD_METRES;
            let before = original.sample_accurate_surface(point).unwrap().position.z;
            let after = refined.sample_accurate_surface(point).unwrap().position.z;
            assert!((before - after).abs() < 1.0e-7);
        }
        // Treat the height buffer as a scalar vertex field to exercise the exact
        // interpolation used for rendered coverage.
        for vertex in &mut refined.mesh.vertices {
            vertex.z = tracks.coverage_at(vertex.truncate());
        }
        for (offset, minimum, maximum) in [
            (0., 0., 0.05),
            (-0.60, 0.7, 1.0),
            (0.60, 0.7, 1.0),
            (1.15, 0., 0.05),
        ] {
            let point = (location + across * offset) / ISLAND_WORLD_METRES;
            let coverage = refined.sample_accurate_surface(point).unwrap().position.z;
            assert!(
                (minimum..=maximum).contains(&coverage),
                "{offset}: {coverage}"
            );
        }
    }

    #[test]
    fn independent_tiles_add_the_same_vertices_and_rut_weights_on_the_seam() {
        let east = ISLAND_WORLD_METRES;
        let make_mesh = |border: f32, inner: f32, reverse: bool| {
            let [a, b, c] = [
                normalized(border, 590.).extend(0.04),
                normalized(border, 622.).extend(0.046),
                normalized(inner, if reverse { 588. } else { 622. }).extend(0.05),
            ];
            Mesh {
                vertices: if reverse {
                    vec![c, b, a]
                } else {
                    vec![a, b, c]
                },
                triangles: vec![0, 1, 2],
                normals: vec![Vec3::Z; 3],
                uv: Vec::new(),
            }
        };
        let east_run = [[east - 8., 607., 41.], [east, 607., 41.]];
        let west_run = [[0., 607., 41.], [8., 607., 41.]];
        let extract = |run: &[[f32; 3]], mesh: Mesh, edge: f32| {
            let tracks = dry_tracks([run]);
            let (mesh, _) = tracks.refined_mesh(&mesh).unwrap().unwrap();
            assert!(
                tracks.coverage_at(Vec2::new(
                    edge * 0.9999 + (1. - edge) * 0.0001,
                    normalized(0., 607.6).y
                )) > 0.99
            );
            let mut hits = mesh
                .vertices
                .into_iter()
                .filter(|point| point.x.to_bits() == edge.to_bits())
                .map(|point| (point.y, point.z, tracks.coverage_at(point.truncate())))
                .collect::<Vec<_>>();
            hits.sort_by(|a, b| a.0.total_cmp(&b.0));
            hits
        };
        let left = extract(&east_run, make_mesh(east, east - 20., false), 1.);
        let right = extract(&west_run, make_mesh(0., 20., true), 0.);
        assert_eq!(left, right);
        let crossing = normalized(0., 607.).y;
        assert!(
            left.iter()
                .any(|&(y, _, weight)| y < crossing && weight > 0.95)
        );
        assert!(
            left.iter()
                .any(|&(y, _, weight)| y > crossing && weight > 0.95)
        );
    }

    #[test]
    #[ignore = "measures refinement on several kilometres of tracks"]
    fn long_curved_paths_stay_inside_the_refinement_budget() {
        let grid_size = 161;
        let vertices = (0..grid_size)
            .flat_map(|y| {
                (0..grid_size).map(move |x| Vec3::new(x as f32 / 160., y as f32 / 160., 0.04))
            })
            .collect();
        let triangles = (0..grid_size - 1)
            .flat_map(|y| {
                (0..grid_size - 1).flat_map(move |x| {
                    let start = y * grid_size + x;
                    [
                        start,
                        start + 1,
                        start + 1 + grid_size,
                        start,
                        start + 1 + grid_size,
                        start + grid_size,
                    ]
                })
            })
            .collect();
        let mesh = Mesh {
            vertices,
            triangles,
            ..Default::default()
        };
        let runs = [310., 700., 1100., 1500.].map(|base| {
            (50..1951)
                .map(|station| {
                    let x = station as f32;
                    [
                        x,
                        base + 80. * (x / 300.).sin() + 12. * (x / 50.).sin(),
                        80.,
                    ]
                })
                .collect::<Vec<_>>()
        });
        let tracks = dry_tracks(runs.iter().map(Vec::as_slice));
        let now = std::time::Instant::now();
        let (_, splits) = tracks.refined_mesh(&mesh).unwrap().unwrap();
        eprintln!(
            "Added {} splits for 7.6 km of curved track in {:?}",
            splits.len(),
            now.elapsed()
        );
    }
}
