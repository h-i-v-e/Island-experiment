//! Renderer-independent spherical Delaunay construction.
//!
//! A translating plane orders insertion into a global 3D convex hull. With
//! well-separated sites on the unit sphere and the origin inside the final
//! hull, its boundary is the spherical Delaunay triangulation.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub use glam::DVec3;
use robust::{Coord3D, orient3d};

pub mod chunk_lod;
pub mod chunks;
mod noise;
pub mod ocean_surface;
pub mod surface_atlas;
pub mod terrain;

pub use terrain::{
    OceanPass, OceanStage, PlanetTerrain, RiverSegment, TerrainOptions, TerrainStats, Waterfall,
    generate_terrain,
};

/// The initial implementation scans all faces; keep its quadratic work bounded.
pub const MAX_VERTICES: usize = 8_192;
/// Keep f64 projection roundoff much smaller than the gap between sites.
const MIN_SITE_DISTANCE_SQUARED: f64 = 1.0e-14;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetOptions {
    pub seed: u64,
    pub vertex_count: usize,
    pub radius: f64,
    pub sweep_direction: DVec3,
}

impl Default for PlanetOptions {
    fn default() -> Self {
        Self {
            seed: 42,
            vertex_count: 1_024,
            radius: 1000.0,
            sweep_direction: DVec3::Y,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct PlanetMesh {
    /// Unit directions in stable input order, independent of insertion order.
    pub directions: Vec<DVec3>,
    /// Counterclockwise when viewed from outside the planet.
    pub triangles: Vec<[u32; 3]>,
    pub radius: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshStats {
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanetError {
    InvalidOptions(&'static str),
    InvalidSite { index: usize, reason: &'static str },
    DegenerateInput,
    HemisphereCoverage,
    InvalidTopology(&'static str),
}

impl fmt::Display for PlanetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions(reason) => write!(f, "invalid planet options: {reason}"),
            Self::InvalidSite { index, reason } => write!(f, "invalid site {index}: {reason}"),
            Self::DegenerateInput => f.write_str("sites do not define a nondegenerate 3D hull"),
            Self::HemisphereCoverage => f.write_str(
                "sites do not surround the origin; choose another seed or more vertices",
            ),
            Self::InvalidTopology(reason) => write!(f, "invalid planet mesh: {reason}"),
        }
    }
}

impl Error for PlanetError {}

/// Generates, triangulates, and validates a planet before returning it.
///
/// # Errors
/// Rejects invalid options, insufficient spherical coverage, and mesh failures.
pub fn generate(options: PlanetOptions) -> Result<PlanetMesh, PlanetError> {
    check_options(
        options.vertex_count,
        options.radius,
        options.sweep_direction,
    )?;
    let mut state = options.seed;
    let mut directions: Vec<DVec3> = Vec::with_capacity(options.vertex_count);
    // SplitMix64 is the inherited island RNG's stream, using 53-bit f64 samples.
    for _ in 0..options.vertex_count {
        let mut accepted = None;
        for _ in 0..128 {
            let y = 1.0 - 2.0 * unit_random(&mut state);
            let phi = std::f64::consts::TAU * unit_random(&mut state);
            let ring_radius = (1.0 - y * y).max(0.0).sqrt();
            let (sin, cos) = phi.sin_cos();
            let point = DVec3::new(ring_radius * cos, y, ring_radius * sin).normalize();
            if directions
                .iter()
                .all(|other| other.distance_squared(point) >= MIN_SITE_DISTANCE_SQUARED)
            {
                accepted = Some(point);
                break;
            }
        }
        directions.push(accepted.ok_or(PlanetError::InvalidOptions(
            "could not sample separated sites",
        ))?);
    }
    triangulate(directions, options.radius, options.sweep_direction)
}

/// Consumes unit-sphere sites and builds their spherical triangulation.
///
/// The returned mesh owns the sites; no renderer or GPU is involved. Exact
/// orientation signs are computed adaptively for the supplied f64 coordinates.
///
/// # Errors
/// Rejects non-unit, duplicate/near-duplicate, degenerate, or hemisphere-only
/// sites, invalid options, and a hull that fails full validation.
pub fn triangulate(
    directions: Vec<DVec3>,
    radius: f64,
    sweep_direction: DVec3,
) -> Result<PlanetMesh, PlanetError> {
    check_options(directions.len(), radius, sweep_direction)?;
    for (index, &site) in directions.iter().enumerate() {
        if !site.is_finite() || (site.length_squared() - 1.0).abs() > 1.0e-12 {
            return Err(PlanetError::InvalidSite {
                index,
                reason: "expected a finite unit direction",
            });
        }
        if directions[..index]
            .iter()
            .any(|other| other.distance_squared(site) < MIN_SITE_DISTANCE_SQUARED)
        {
            return Err(PlanetError::InvalidSite {
                index,
                reason: "duplicate or insufficiently separated direction",
            });
        }
    }
    let axis = sweep_direction.normalize();
    let mut order: Vec<usize> = (0..directions.len()).collect();
    order.sort_unstable_by(|&a, &b| {
        directions[a]
            .dot(axis)
            .total_cmp(&directions[b].dot(axis))
            .then(a.cmp(&b))
    });
    let a = order[0];
    let b = order[1];
    let c = order
        .iter()
        .copied()
        .skip(2)
        .find(|&c| {
            (directions[b] - directions[a])
                .cross(directions[c] - directions[a])
                .length_squared()
                > 0.0
        })
        .ok_or(PlanetError::DegenerateInput)?;
    let d = order
        .iter()
        .copied()
        .find(|&d| orientation(directions[a], directions[b], directions[c], directions[d]) != 0.0)
        .ok_or(PlanetError::DegenerateInput)?;
    let seed = [a, b, c, d];
    // This interior point stays inside every subsequent hull, even before the
    // sweep reaches the equator and the origin enters the partial hull.
    let interior = seed.iter().map(|&i| directions[i]).sum::<DVec3>() / 4.0;
    let mut faces = Vec::with_capacity(2 * directions.len());
    for face in [[a, b, c], [a, d, b], [a, c, d], [b, d, c]] {
        faces.push(outward(face, &directions, interior));
    }
    let mut horizon = BTreeSet::new();
    for index in order.into_iter().filter(|i| !seed.contains(i)) {
        horizon.clear();
        // Opposite edges shared by two removed faces cancel, leaving the
        // oriented boundary between removed and retained faces.
        faces.retain(|&face| {
            if side(face, &directions, directions[index]) >= 0.0 {
                return true;
            }
            for (from, to) in edges(face) {
                if !horizon.remove(&(to, from)) {
                    horizon.insert((from, to));
                }
            }
            false
        });
        if horizon.is_empty() {
            return Err(PlanetError::InvalidTopology("insertion lost a sphere site"));
        }
        for &(from, to) in &horizon {
            faces.push(outward([from, to, index], &directions, interior));
        }
    }
    // Every index comes from the input enumeration, bounded by MAX_VERTICES.
    #[allow(clippy::cast_possible_truncation)]
    let triangles = faces
        .into_iter()
        .map(|face| face.map(|index| index as u32))
        .collect();
    let mesh = PlanetMesh {
        directions,
        triangles,
        radius,
    };
    mesh.validate()?;
    Ok(mesh)
}

impl PlanetMesh {
    /// Checks the complete geometric and combinatorial sphere contract.
    ///
    /// Supporting-plane tests against every site establish convexity and the
    /// spherical empty-cap property, also ruling out crossed surface faces.
    /// This exhaustive reference validation is O(V * F).
    ///
    /// # Errors
    /// Returns the first violated geometry or topology condition.
    pub fn validate(&self) -> Result<MeshStats, PlanetError> {
        let count = self.directions.len();
        check_options(count, self.radius, DVec3::Y)?;
        if self
            .directions
            .iter()
            .any(|p| !p.is_finite() || (p.length_squared() - 1.0).abs() > 1.0e-12)
        {
            return Err(PlanetError::InvalidTopology("non-unit or non-finite site"));
        }
        if self.triangles.len() != 2 * count - 4 {
            return Err(PlanetError::InvalidTopology("face count must be 2V - 4"));
        }
        let mut edge_uses: BTreeMap<(usize, usize), (usize, isize)> = BTreeMap::new();
        let mut links = vec![Vec::new(); count];
        let mut unique_faces = BTreeSet::new();
        for triangle in &self.triangles {
            let face = triangle.map(|i| i as usize);
            if face.iter().any(|&i| i >= count)
                || face[0] == face[1]
                || face[1] == face[2]
                || face[2] == face[0]
            {
                return Err(PlanetError::InvalidTopology("invalid triangle indices"));
            }
            let mut canonical = face;
            canonical.sort_unstable();
            if !unique_faces.insert(canonical) {
                return Err(PlanetError::InvalidTopology("duplicate face"));
            }
            if side(face, &self.directions, DVec3::ZERO) <= 0.0 {
                return Err(PlanetError::HemisphereCoverage);
            }
            if self
                .directions
                .iter()
                .any(|&point| side(face, &self.directions, point) < 0.0)
            {
                return Err(PlanetError::InvalidTopology(
                    "supporting plane is not empty",
                ));
            }
            for (from, to) in edges(face) {
                let entry = edge_uses.entry((from.min(to), from.max(to))).or_default();
                entry.0 += 1;
                entry.1 += if from < to { 1 } else { -1 };
            }
            links[face[0]].push((face[1], face[2]));
            links[face[1]].push((face[2], face[0]));
            links[face[2]].push((face[0], face[1]));
        }
        if edge_uses.len() != 3 * count - 6
            || edge_uses
                .values()
                .any(|&(uses, winding)| uses != 2 || winding != 0)
        {
            return Err(PlanetError::InvalidTopology(
                "edges must have two oppositely wound incident faces",
            ));
        }
        validate_links(&links)?;
        Ok(MeshStats {
            vertices: count,
            edges: edge_uses.len(),
            faces: self.triangles.len(),
        })
    }
}

/// Check each vertex neighbourhood and global connectivity independently of
/// the geometric supporting-plane tests.
fn validate_links(links: &[Vec<(usize, usize)>]) -> Result<(), PlanetError> {
    // The link around every vertex must form exactly one directed cycle.
    // Together with the edge tests, this excludes pinched/nonmanifold sites.
    for link in links {
        let Some(&(start, _)) = link.first() else {
            return Err(PlanetError::InvalidTopology("unused vertex"));
        };
        let next: BTreeMap<_, _> = link.iter().copied().collect();
        if next.len() != link.len() {
            return Err(PlanetError::InvalidTopology("branched vertex link"));
        }
        let mut current = start;
        for step in 0..link.len() {
            current = *next
                .get(&current)
                .ok_or(PlanetError::InvalidTopology("open vertex link"))?;
            if current == start && step + 1 != link.len() {
                return Err(PlanetError::InvalidTopology("disconnected vertex link"));
            }
        }
        if current != start {
            return Err(PlanetError::InvalidTopology("vertex link is not a cycle"));
        }
    }
    let mut visited = vec![false; links.len()];
    let mut stack = vec![0];
    while let Some(vertex) = stack.pop() {
        if std::mem::replace(&mut visited[vertex], true) {
            continue;
        }
        stack.extend(
            links[vertex]
                .iter()
                .map(|&(neighbour, _)| neighbour)
                .filter(|&i| !visited[i]),
        );
    }
    if visited.iter().any(|seen| !seen) {
        return Err(PlanetError::InvalidTopology("disconnected surface"));
    }
    Ok(())
}

fn check_options(count: usize, radius: f64, direction: DVec3) -> Result<(), PlanetError> {
    if !(4..=MAX_VERTICES).contains(&count) {
        return Err(PlanetError::InvalidOptions(
            "vertex count must be between 4 and 8192",
        ));
    }
    if !radius.is_finite() || radius <= 0.0 {
        return Err(PlanetError::InvalidOptions(
            "radius must be finite and positive",
        ));
    }
    if !direction.is_finite()
        || !direction.length_squared().is_finite()
        || direction.length_squared() <= f64::MIN_POSITIVE
    {
        return Err(PlanetError::InvalidOptions(
            "sweep direction must be finite and nonzero",
        ));
    }
    Ok(())
}

fn coord(point: DVec3) -> Coord3D<f64> {
    Coord3D {
        x: point.x,
        y: point.y,
        z: point.z,
    }
}

fn orientation(a: DVec3, b: DVec3, c: DVec3, d: DVec3) -> f64 {
    orient3d(coord(a), coord(b), coord(c), coord(d))
}

fn side(face: [usize; 3], points: &[DVec3], point: DVec3) -> f64 {
    orientation(points[face[0]], points[face[1]], points[face[2]], point)
}

fn outward(mut face: [usize; 3], points: &[DVec3], interior: DVec3) -> [usize; 3] {
    if side(face, points, interior) < 0.0 {
        face.swap(1, 2);
    }
    face
}

fn edges([a, b, c]: [usize; 3]) -> [(usize, usize); 3] {
    [(a, b), (b, c), (c, a)]
}

#[allow(clippy::cast_precision_loss)]
fn unit_random(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value >> 11) as f64 / 9_007_199_254_740_992.0
}
