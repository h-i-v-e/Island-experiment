//! Constrained mesh LODs with identical borders at every resolution.
//!
//! Collapses retain an existing endpoint, so borders and protected hydraulic
//! features keep their original positions, normals, and colours. The link
//! condition preserves manifold connectivity, while local normal checks reject
//! folded or degenerate faces. Shape and valence limits prevent thin triangle fans.
//! Work is confined to changed vertex neighbourhoods.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

use crate::chunks::ChunkMesh;

/// One renderable resolution, ordered finest first by [`build_lods`].
#[derive(Clone, Debug, PartialEq)]
pub struct ChunkLod {
    pub mesh: ChunkMesh,
    /// Conservative surface deviation in the same units as vertex positions.
    pub error: f64,
}

/// Builds four levels, targeting successively half as many triangles.
///
/// A constrained mesh may stop reducing before reaching a target. Its remaining
/// levels still preserve all borders and features. `error` bounds the distance
/// between the source and simplified piecewise-linear surfaces: each collapse is
/// a simplicial map, and the displacement bound of its vertex correspondence
/// also bounds every barycentric point on every source triangle.
#[must_use]
pub fn build_lods(source: &ChunkMesh) -> Vec<ChunkLod> {
    build(source, true)
}

/// Builds render LODs while fixing only open chunk boundaries. Interior river,
/// bank and coast flags may collapse; the original LOD0 stays unchanged.
#[must_use]
pub fn build_unpinned_lods(source: &ChunkMesh) -> Vec<ChunkLod> {
    build(source, false)
}

fn build(source: &ChunkMesh, preserve_features: bool) -> Vec<ChunkLod> {
    let mut levels = Vec::with_capacity(4);
    // The finest level owns its independent renderable mesh; the source remains
    // borrowed and is never edited by simplification.
    levels.push(ChunkLod {
        mesh: source.clone(),
        error: 0.0,
    });
    let mut simplifier = Simplifier::new(source, preserve_features);
    for level in 1..4 {
        let target = source.triangles.len() >> level;
        simplifier.reduce_to(target);
        levels.push(ChunkLod {
            mesh: simplifier.snapshot(),
            error: simplifier.error,
        });
    }
    levels
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    from: usize,
    to: usize,
    versions: [u64; 2],
    cost: f64,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Candidate {}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max heap; invert cost and ids for deterministic minima.
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.from.cmp(&self.from))
            .then_with(|| other.to.cmp(&self.to))
            .then_with(|| self.versions.cmp(&other.versions))
    }
}

struct Simplifier<'a> {
    source: &'a ChunkMesh,
    faces: Vec<Option<[u32; 3]>>,
    incident: Vec<BTreeSet<usize>>,
    fixed: Vec<bool>,
    alive: Vec<bool>,
    versions: Vec<u64>,
    radii: Vec<f64>,
    candidates: BinaryHeap<Candidate>,
    face_count: usize,
    error: f64,
}

impl<'a> Simplifier<'a> {
    fn new(source: &'a ChunkMesh, preserve_features: bool) -> Self {
        let count = source.vertices.len();
        let mut edges = BTreeMap::<[usize; 2], usize>::new();
        let mut incident = vec![BTreeSet::new(); count];
        for (index, face) in source.triangles.iter().enumerate() {
            for &vertex in face {
                incident[vertex as usize].insert(index);
            }
            for edge in face_edges(*face) {
                *edges.entry(edge).or_default() += 1;
            }
        }
        let mut fixed: Vec<_> = source
            .vertices
            .iter()
            .map(|vertex| preserve_features && vertex.protected)
            .collect();
        for (&[a, b], &uses) in &edges {
            if uses != 2 {
                fixed[a] = true;
                fixed[b] = true;
            }
        }
        let mut result = Self {
            source,
            faces: source.triangles.iter().copied().map(Some).collect(),
            incident,
            fixed,
            alive: vec![true; count],
            versions: vec![0; count],
            radii: vec![0.0; count],
            candidates: BinaryHeap::new(),
            face_count: source.triangles.len(),
            error: 0.0,
        };
        for [a, b] in edges.into_keys() {
            result.queue_edge(a, b);
        }
        result
    }

    fn queue_edge(&mut self, a: usize, b: usize) {
        if !self.alive[a] || !self.alive[b] || (self.fixed[a] && self.fixed[b]) {
            return;
        }
        let length = self.source.vertices[a]
            .position
            .distance(self.source.vertices[b].position);
        // Consider both endpoints: one direction may preserve good triangles
        // while the reverse direction stretches the opposite side of the edge.
        for (from, to) in [(a, b), (b, a)] {
            if self.fixed[from] {
                continue;
            }
            self.candidates.push(Candidate {
                from,
                to,
                versions: [self.versions[from], self.versions[to]],
                // Include the survivor's accumulated displacement. Ignoring it
                // rewards repeatedly growing the same hub into a huge fan.
                cost: self.radii[to].max(self.radii[from] + length),
            });
        }
    }

    fn neighbours(&self, vertex: usize) -> BTreeSet<usize> {
        self.incident[vertex]
            .iter()
            .filter_map(|&face| self.faces[face])
            .flatten()
            .map(|index| index as usize)
            .filter(|&index| index != vertex)
            .collect()
    }

    fn reduce_to(&mut self, target: usize) {
        while self.face_count > target {
            let Some(candidate) = self.candidates.pop() else {
                break;
            };
            let Candidate {
                from, to, versions, ..
            } = candidate;
            if !self.alive[from]
                || !self.alive[to]
                || versions != [self.versions[from], self.versions[to]]
                || !self.can_collapse(from, to)
            {
                continue;
            }
            self.collapse(from, to);
        }
    }

    fn can_collapse(&self, from: usize, to: usize) -> bool {
        let shared: Vec<_> = self.incident[from]
            .intersection(&self.incident[to])
            .copied()
            .collect();
        if shared.len() != 2 {
            return false;
        }
        // Interior edge collapse removes two shared faces from each endpoint.
        // Existing high-valence source vertices can survive, but cannot grow.
        let degree = self.incident[from].len() + self.incident[to].len() - 4;
        if degree > 16 && degree > self.incident[from].len().max(self.incident[to].len()) {
            return false;
        }
        let a = self.neighbours(from);
        let b = self.neighbours(to);
        let opposite: BTreeSet<_> = shared
            .iter()
            .filter_map(|&face| self.faces[face])
            .flatten()
            .map(|index| index as usize)
            .filter(|&vertex| vertex != from && vertex != to)
            .collect();
        if a.intersection(&b).copied().collect::<BTreeSet<_>>() != opposite {
            return false;
        }

        let mut keys = BTreeSet::new();
        for &face_index in self.incident[from].union(&self.incident[to]) {
            let face = self.faces[face_index].expect("incident face is live");
            if face.contains(&index_u32(from)) && face.contains(&index_u32(to)) {
                continue;
            }
            let updated = face.map(|index| {
                if index as usize == from {
                    index_u32(to)
                } else {
                    index
                }
            });
            let mut key = updated;
            key.sort_unstable();
            if !keys.insert(key) {
                return false;
            }
            let normal = self.normal(updated);
            let old_normal = self.normal(face);
            let reference_face = self.source.triangles[face_index];
            let reference = self.normal(reference_face);
            let area = normal.length_squared();
            // Relative checks work for both kilometre-scale terrain and tiny
            // tessellated banks, without an arbitrary world-space area cutoff.
            // Minimum altitude / longest edge: limit new triangles to 10:1.
            // Clipping may supply narrower boundary slivers; retain those
            // without allowing simplification to make their shape worse.
            let minimum_quality = self.quality_squared(face).min(0.01);
            if self.quality_squared(updated) + 1.0e-15 < minimum_quality
                || !area.is_finite()
                || area <= old_normal.length_squared() * 1.0e-12
                || normal.dot(old_normal) <= (area * old_normal.length_squared()).sqrt() * 0.2
                || normal.dot(reference) <= (area * reference.length_squared()).sqrt() * 0.2
                // Reference-normal agreement alone can accept a triangle
                // folding across a spherical direction: preserve radial graphs.
                || (self.radial_orientation(reference_face, reference) > 0.0
                    && self.radial_orientation(updated, normal) <= 0.0)
            {
                return false;
            }
        }
        true
    }

    fn normal(&self, face: [u32; 3]) -> glam::DVec3 {
        let [a, b, c] = face.map(|index| self.source.vertices[index as usize].position);
        (b - a).cross(c - a)
    }

    fn quality_squared(&self, face: [u32; 3]) -> f64 {
        let [a, b, c] = face.map(|index| self.source.vertices[index as usize].position);
        let longest_squared = a
            .distance_squared(b)
            .max(b.distance_squared(c))
            .max(c.distance_squared(a));
        (b - a).cross(c - a).length_squared() / longest_squared.powi(2)
    }

    fn radial_orientation(&self, face: [u32; 3], normal: glam::DVec3) -> f64 {
        let [a, b, c] = face.map(|index| self.source.vertices[index as usize].position);
        normal.dot(a + b + c)
    }

    fn collapse(&mut self, from: usize, to: usize) {
        let mut affected = self.neighbours(from);
        affected.extend(self.neighbours(to));
        affected.insert(to);
        let changed: Vec<_> = self.incident[from].iter().copied().collect();
        for face_index in changed {
            let face = self.faces[face_index]
                .take()
                .expect("incident face is live");
            for vertex in face {
                self.incident[vertex as usize].remove(&face_index);
            }
            if face.contains(&index_u32(to)) {
                self.face_count -= 1;
            } else {
                let updated = face.map(|index| {
                    if index as usize == from {
                        index_u32(to)
                    } else {
                        index
                    }
                });
                self.faces[face_index] = Some(updated);
                for vertex in updated {
                    self.incident[vertex as usize].insert(face_index);
                }
            }
        }
        self.alive[from] = false;
        let distance = self.source.vertices[from]
            .position
            .distance(self.source.vertices[to].position);
        self.radii[to] = self.radii[to].max((self.radii[from] + distance).next_up());
        self.error = self.error.max(self.radii[to]);
        // Invalidate candidates whose local topology changed, and reconsider
        // rejected neighbours: a previously blocked edge can now be safe.
        for &vertex in &affected {
            self.versions[vertex] += 1;
        }
        let mut edges = BTreeSet::new();
        for vertex in affected {
            for neighbour in self.neighbours(vertex) {
                edges.insert(sorted_edge(vertex, neighbour));
            }
        }
        for [a, b] in edges {
            self.queue_edge(a, b);
        }
    }

    fn snapshot(&self) -> ChunkMesh {
        let mut remap = vec![0; self.source.vertices.len()];
        let vertices = self
            .source
            .vertices
            .iter()
            .enumerate()
            .filter(|(index, _)| self.alive[*index])
            .enumerate()
            .map(|(new_index, (old_index, vertex))| {
                remap[old_index] = index_u32(new_index);
                *vertex
            })
            .collect();
        let triangles = self
            .faces
            .iter()
            .flatten()
            .map(|face| face.map(|index| remap[index as usize]))
            .collect();
        ChunkMesh {
            vertices,
            triangles,
        }
    }
}

fn index_u32(index: usize) -> u32 {
    u32::try_from(index).expect("chunk vertex index fits u32")
}
fn sorted_edge(a: usize, b: usize) -> [usize; 2] {
    if a < b { [a, b] } else { [b, a] }
}
fn face_edges([a, b, c]: [u32; 3]) -> [[usize; 2]; 3] {
    [
        sorted_edge(a as usize, b as usize),
        sorted_edge(b as usize, c as usize),
        sorted_edge(c as usize, a as usize),
    ]
}
