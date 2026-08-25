//! The terrain grid: one stable square of the island per chunk, each carrying
//! the same ground at three levels of detail.
//!
//! An island-wide terrain mesh is one entity, and one entity is either drawn
//! whole or not at all: nothing behind the camera can be left out, and the
//! close views pay for two million vertices to see a few hundred thousand of
//! them. Cutting the same surface into a fixed grid gives the frustum something
//! to cull and gives each square its own choice of detail, and neither costs a
//! frame anything: the chunks are sliced once, on the generation task, and go
//! into the cache with everything else.
//!
//! The grid is the generator's own [`Mesh::sliced_grid`], which clips one
//! source-triangle pass into every tile at once. Two tiles of one grid share
//! their boundary points exactly — the same clip plane produces the same
//! intersections — so nothing has to be done about a seam between two chunks
//! drawn at the same level.
//!
//! A seam between two levels is a different matter, and the skirt below is what
//! answers it. See [`skirt`].

use std::collections::{HashMap, HashSet};

use motu::{BoundingBox, ISLAND_WORLD_METRES, Mesh, Vec3};

/// Chunks along each edge of the island square.
///
/// Eight is 64 chunks of 250 m, which at terrain size 1024 is 26 k vertices in
/// an average one and 86 k in the largest. Finer would cull more precisely and
/// duplicate more boundary vertices — the grid already costs 5.9 per cent at
/// LOD 0 and 91 per cent at LOD 2, where a chunk is a few hundred vertices to
/// begin with — and coarser would leave the close views drawing ground behind
/// the camera again.
pub const DIVISIONS: u32 = 8;

/// Levels of detail every chunk carries, which is every level the generator
/// publishes.
pub const TIERS: usize = 3;

/// Metres a chunk's skirt hangs below its own edge.
///
/// It has to clear the tallest step any two levels can leave at a shared
/// boundary. On the reference island the worst LOD 0 boundary vertex stands
/// 4.8 m off the LOD 1 surface and the worst LOD 1 vertex 19.0 m off LOD 2, so
/// this is the larger of the two with a margin. Nothing is paid for the margin:
/// a skirt is only ever seen through the gap it is filling.
pub const SKIRT_METRES: f32 = 24.0;

/// Which edges of its own square a chunk vertex stands on. A corner stands on
/// two.
const LEFT: u8 = 1;
const RIGHT: u8 = 2;
const BOTTOM: u8 = 4;
const TOP: u8 = 8;

/// The sides of one chunk that face another chunk.
///
/// Only those need an apron. The four sides on the outside of the grid are the
/// terrain square's own edge, where there is no neighbour to disagree with and
/// no gap to fill — and where an apron would be a lip standing proud of the
/// edge instead, which is exactly the boundary the island is meant not to draw
/// attention to.
#[must_use]
pub fn interior_sides(column: u32, row: u32) -> u8 {
    let mut sides = 0;
    if column > 0 {
        sides |= LEFT;
    }
    if column + 1 < DIVISIONS {
        sides |= RIGHT;
    }
    if row > 0 {
        sides |= BOTTOM;
    }
    if row + 1 < DIVISIONS {
        sides |= TOP;
    }
    sides
}

/// One chunk at one level of detail: the sliced surface and the two per-vertex
/// fields the terrain shader reads off it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkTier {
    pub mesh: Mesh,
    /// The generator's material triple per vertex: bedrock hardness, loose
    /// cover, sea proximity.
    pub materials: Vec<Vec3>,
    /// The renderer's own proximity to running water, per vertex.
    pub river_wetness: Vec<f32>,
}

/// One square of the island at every level of detail.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainChunk {
    pub column: u32,
    pub row: u32,
    /// Finest first, so the index is the LOD level.
    pub tiers: [ChunkTier; TIERS],
}

impl TerrainChunk {
    /// Vertices across every level this chunk holds, which is what it costs to
    /// keep resident whichever one is drawn.
    #[must_use]
    pub fn vertices(&self) -> usize {
        self.tiers.iter().map(|tier| tier.mesh.vertices.len()).sum()
    }
}

/// The point all of one chunk's entities stand at, in island space: the centre
/// of its square, at the middle of the elevations it covers.
///
/// Bevy takes the level-of-detail crossfade distance from the entity's own
/// translation and not from the vertex being drawn, so every level of one chunk
/// has to stand at exactly the same point and carry its vertices relative to
/// it. Leaving the whole terrain at the origin instead — which world-space
/// vertices invite — makes every chunk answer the distance test as if it were
/// the island's centre, and the shader then discards a chunk the culling stage
/// kept.
///
/// The square's own centre is used rather than the mesh's, so the levels agree
/// whatever each of them happens to cover and the bands fall evenly across the
/// grid; only the elevation is read off the geometry.
#[must_use]
pub fn origin(chunk: &TerrainChunk) -> Vec3 {
    let square = bounds(chunk.column, chunk.row);
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    for vertex in chunk.tiers.iter().flat_map(|tier| &tier.mesh.vertices) {
        low = low.min(vertex.z);
        high = high.max(vertex.z);
    }
    Vec3::new(
        f32::midpoint(square.min.x, square.max.x),
        f32::midpoint(square.min.y, square.max.y),
        if low <= high {
            f32::midpoint(low, high)
        } else {
            0.0
        },
    )
}

/// The island-space bounds of one chunk. Z is left wide open: the grid divides
/// the square, not the elevation.
#[must_use]
pub fn bounds(column: u32, row: u32) -> BoundingBox {
    let step = 1.0 / f64::from(DIVISIONS);
    #[allow(clippy::cast_possible_truncation)]
    let edge = |index: u32| (f64::from(index) * step) as f32;
    BoundingBox::new(
        Vec3::new(edge(column), edge(row), f32::MIN),
        Vec3::new(edge(column + 1), edge(row + 1), f32::MAX),
    )
}

/// One level of detail cut into the grid, row by row from `row == 0`, which is
/// the order [`bounds`] indexes.
#[must_use]
pub fn sliced(source: &Mesh) -> Vec<Mesh> {
    source.sliced_grid(BoundingBox::default(), DIVISIONS as usize)
}

/// Hangs a vertical apron below every edge of one chunk, and reports the vertex
/// each apron vertex was copied from so the caller can copy its material and
/// wetness with it.
///
/// This is what keeps a level transition from cracking. Chunks are sliced
/// without any boundary clamping, so two chunks drawn at the same level meet
/// exactly and the near view — where every chunk is at LOD 0 — has no seam
/// treatment at all and no ground pulled onto a coarser profile. Two chunks
/// drawn at different levels do not meet: whichever surface stands higher
/// leaves a gap under it, and the apron hanging from that higher edge is what
/// fills the gap, shaded by the same normal and the same material weights as
/// the ground it hangs from.
///
/// Where the two surfaces do meet, the apron costs nothing to look at. The
/// terrain is a height field, so a ray from any camera above it that reaches a
/// point below the shared edge has already crossed the surface on the near
/// side: the apron is behind the ground it hangs from, from every direction the
/// ground can be seen from.
///
/// Only the sides in `sides` are given an apron; see [`interior_sides`].
///
/// Both windings are emitted for each quad, because which side of a gap the
/// camera stands on is not known when the mesh is built.
pub fn skirt(mesh: &mut Mesh, bounds: BoundingBox, depth: f32, sides: u8) -> Vec<u32> {
    let mut sources = Vec::new();
    if mesh.triangles.is_empty() || sides == 0 {
        return sources;
    }
    let epsilon =
        ((bounds.max.x - bounds.min.x).abs() + (bounds.max.y - bounds.min.y).abs()) * 1.0e-6;
    let edges: Vec<u8> = mesh
        .vertices
        .iter()
        .map(|vertex| {
            let mut sides = 0;
            if (vertex.x - bounds.min.x).abs() <= epsilon {
                sides |= LEFT;
            }
            if (vertex.x - bounds.max.x).abs() <= epsilon {
                sides |= RIGHT;
            }
            if (vertex.y - bounds.min.y).abs() <= epsilon {
                sides |= BOTTOM;
            }
            if (vertex.y - bounds.max.y).abs() <= epsilon {
                sides |= TOP;
            }
            sides
        })
        .collect();

    let has_normals = mesh.normals.len() == mesh.vertices.len();
    let has_uv = mesh.uv.len() == mesh.vertices.len();
    let mut hanging: HashMap<u32, u32> = HashMap::new();
    let mut walked: HashSet<(u32, u32)> = HashSet::new();
    let mut apron: Vec<u32> = Vec::new();
    // Taken out of the mesh so the surface can grow while its own triangles are
    // being walked, and put back whole below.
    let triangles = std::mem::take(&mut mesh.triangles);
    for triangle in triangles.as_chunks::<3>().0 {
        let corners = [triangle[0], triangle[1], triangle[2]];
        for pair in 0..3 {
            let (from, to) = (corners[pair], corners[(pair + 1) % 3]);
            // Both ends on one side of the square is what makes an edge part of
            // the chunk's own border: the surface stops there, so no second
            // triangle carries the same edge.
            if edges[from as usize] & edges[to as usize] & sides == 0 {
                continue;
            }
            if !walked.insert((from.min(to), from.max(to))) {
                continue;
            }
            let mut below = |index: u32| {
                *hanging.entry(index).or_insert_with(|| {
                    #[allow(clippy::cast_possible_truncation)]
                    let hung = mesh.vertices.len() as u32;
                    let mut vertex = mesh.vertices[index as usize];
                    vertex.z -= depth;
                    mesh.vertices.push(vertex);
                    if has_normals {
                        let normal = mesh.normals[index as usize];
                        mesh.normals.push(normal);
                    }
                    if has_uv {
                        let uv = mesh.uv[index as usize];
                        mesh.uv.push(uv);
                    }
                    sources.push(index);
                    hung
                })
            };
            let (under_from, under_to) = (below(from), below(to));
            apron.extend([
                from,
                to,
                under_to,
                from,
                under_to,
                under_from,
                from,
                under_to,
                to,
                from,
                under_from,
                under_to,
            ]);
        }
    }
    mesh.triangles = triangles;
    mesh.triangles.extend(apron);
    sources
}

/// [`SKIRT_METRES`] in the normalized units the generator's meshes are in.
#[must_use]
pub fn skirt_depth() -> f32 {
    SKIRT_METRES / ISLAND_WORLD_METRES
}

#[cfg(test)]
mod tests {
    use motu::{BoundingBox, Mesh, Vec2, Vec3};

    use super::{
        BOTTOM, DIVISIONS, LEFT, RIGHT, TIERS, TOP, bounds, interior_sides, skirt, skirt_depth,
    };

    /// Every side at once, which is what a chunk with a neighbour on each of
    /// them asks for.
    const ALL_SIDES: u8 = LEFT | RIGHT | BOTTOM | TOP;

    /// Two triangles over the unit square, so every edge of it is a border
    /// edge and a slice of it has a border on every side.
    fn quad() -> Mesh {
        Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.10),
                Vec3::new(1.0, 0.0, 0.11),
                Vec3::new(1.0, 1.0, 0.12),
                Vec3::new(0.0, 1.0, 0.13),
            ],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0),
            ],
        }
    }

    /// Two chunks have to share their clip plane bit for bit, so the equality
    /// here is exact on purpose: anything looser would pass on a grid whose
    /// tiles do not quite meet.
    #[allow(clippy::float_cmp)]
    #[test]
    fn the_grid_tiles_the_square_without_gaps_or_overlaps() {
        let step = 1.0 / f64::from(DIVISIONS);
        for row in 0..DIVISIONS {
            for column in 0..DIVISIONS {
                let square = bounds(column, row);
                #[allow(clippy::cast_possible_truncation)]
                let expected = (f64::from(column) * step) as f32;
                assert!((square.min.x - expected).abs() < f32::EPSILON);
                // The right edge of one chunk is the left edge of the next,
                // bit for bit, or the two would not share their clip plane.
                if column + 1 < DIVISIONS {
                    assert_eq!(square.max.x, bounds(column + 1, row).min.x);
                }
                if row + 1 < DIVISIONS {
                    assert_eq!(square.max.y, bounds(column, row + 1).min.y);
                }
            }
        }
        // The grid reaches both edges of the square exactly.
        assert!(bounds(0, 0).min.x.abs() < f32::EPSILON);
        assert!((bounds(DIVISIONS - 1, DIVISIONS - 1).max.x - 1.0).abs() < f32::EPSILON);
    }

    /// Every border edge gets an apron, hung from the vertices it already had
    /// and carrying their normals and UVs, so the caller only has to copy the
    /// two per-vertex fields the generator does not put on a mesh.
    #[test]
    fn the_skirt_hangs_from_every_border_edge() {
        let mut mesh = quad();
        let triangles = mesh.triangles.len();
        let depth = skirt_depth();
        let sources = skirt(&mut mesh, BoundingBox::new(Vec3::ZERO, Vec3::ONE), depth, ALL_SIDES);

        // Four corners, each hung once however many edges meet there.
        assert_eq!(sources, vec![0, 1, 2, 3]);
        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.normals.len(), 8);
        assert_eq!(mesh.uv.len(), 8);
        for (hung, &source) in sources.iter().enumerate() {
            let above = mesh.vertices[source as usize];
            let below = mesh.vertices[4 + hung];
            assert!((below.x - above.x).abs() < f32::EPSILON);
            assert!((below.y - above.y).abs() < f32::EPSILON);
            assert!((below.z - (above.z - depth)).abs() < 1.0e-9);
            assert_eq!(mesh.uv[4 + hung], mesh.uv[source as usize]);
        }
        // Four sides, four triangles each: two for the quad and two more for
        // its back face, because which side of a gap is seen is not known here.
        assert_eq!(mesh.triangles.len(), triangles + 4 * 4 * 3);
    }

    /// The diagonal of the quad joins two vertices that are both on the border
    /// but not on the same side of it, and it is not a border edge. An apron
    /// there would be a wall through the middle of the chunk.
    #[test]
    fn an_edge_across_the_square_is_not_a_border() {
        let mut mesh = quad();
        // The diagonal runs from (0,0) to (1,1): corner to opposite corner.
        skirt(&mut mesh, BoundingBox::new(Vec3::ZERO, Vec3::ONE), 0.01, ALL_SIDES);
        let hung = mesh.vertices.len() - 4;
        for triangle in mesh.triangles[6..].as_chunks::<3>().0 {
            let low = triangle.iter().filter(|&&index| index as usize >= hung).count();
            assert!(low == 1 || low == 2, "{triangle:?} is not a skirt quad");
        }
        // Eight quads would mean the diagonal grew one; four is one per side.
        assert_eq!(mesh.triangles.len(), 6 + 4 * 4 * 3);
    }

    /// An empty tile is left alone rather than indexed into.
    #[test]
    fn an_empty_chunk_grows_nothing() {
        let mut mesh = Mesh::default();
        assert!(skirt(&mut mesh, BoundingBox::new(Vec3::ZERO, Vec3::ONE), 0.01, ALL_SIDES).is_empty());
        assert!(mesh.vertices.is_empty());
    }

    /// The renderer indexes tiers by LOD level, so the count has to match what
    /// the generator publishes.
    #[test]
    fn the_tier_count_is_the_generator_s() {
        assert_eq!(TIERS, 3);
    }

    /// A chunk in the middle of the grid has a neighbour on every side and a
    /// chunk on its edge does not. Only the sides that face another chunk get
    /// an apron: the outside of the grid is the terrain square's own edge, and
    /// an apron there would stand proud of it rather than fill anything.
    #[test]
    fn only_the_sides_facing_another_chunk_are_skirted() {
        let last = DIVISIONS - 1;
        assert_eq!(interior_sides(1, 1), ALL_SIDES);
        assert_eq!(interior_sides(0, 0), ALL_SIDES & !(LEFT | BOTTOM));
        assert_eq!(interior_sides(last, last), ALL_SIDES & !(RIGHT | TOP));
        // Every interior seam is claimed from both sides, so neither can be the
        // only one to fill a gap.
        for row in 0..DIVISIONS {
            for column in 0..last {
                let right = interior_sides(column, row) & RIGHT != 0;
                let left = interior_sides(column + 1, row) & LEFT != 0;
                assert!(right && left, "the seam at {column},{row} is one-sided");
            }
        }
    }

    /// A chunk on the outside of the grid grows no apron on that side at all.
    #[test]
    fn an_outer_edge_grows_no_apron() {
        let mut mesh = quad();
        let triangles = mesh.triangles.len();
        // Everything but the left side, which is where the unit square's own
        // border stands in for the outside of the grid.
        let sources = skirt(
            &mut mesh,
            BoundingBox::new(Vec3::ZERO, Vec3::ONE),
            0.01,
            ALL_SIDES & !LEFT,
        );
        assert_eq!(mesh.triangles.len(), triangles + 3 * 4 * 3, "{sources:?}");
        // And nothing at all when no side faces a neighbour.
        let mut alone = quad();
        assert!(skirt(&mut alone, BoundingBox::new(Vec3::ZERO, Vec3::ONE), 0.01, 0).is_empty());
        assert_eq!(alone.triangles.len(), triangles);
    }
}
