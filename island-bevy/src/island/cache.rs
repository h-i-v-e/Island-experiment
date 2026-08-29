//! A disk cache for the geometry the renderer reads off a generated island.
//!
//! `Island::generate` is deterministic and takes seconds, so a repeat launch
//! with the same inputs reads the finished data back instead of running the
//! generator again. Entries live under the crate's own `target` directory,
//! which means `cargo clean` clears them and version control never sees them.
//!
//! This is a development cache, not a distribution format. Every entry carries
//! the seed and options it was generated from, and an entry that does not
//! parse, that was written under a different format version, or whose inputs do
//! not match the ones asked for is simply a miss: the generator runs and the
//! entry is rewritten. Nothing here can fail the app.

use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process,
};

use motu::{GenerationMethod, IslandOptions, Mesh, Vec2, Vec3, Vec4};

use crate::{
    chunk::{self, ChunkTier, TIERS, TerrainChunk},
    hash::mix,
    island_gen::{IslandData, RiverDrop},
    options,
};

/// Mixed into every key and written into every entry. Bump it when the
/// serialized layout changes or when loading old geometry would violate a new
/// generation contract. Generation methods use separate directories, while
/// appearance-only iteration can still be handled by clearing this development
/// cache locally.
///
/// 2 added the walk-mode height grid, 3 the per-vertex river wetness, 4 the
/// river drops — which also widen the wetness around a plunge pool, so entries
/// written under 3 no longer describe the same ground — 5 replaced the one
/// island-wide terrain mesh with the chunk grid at three levels of detail, and
/// 6 took the skirt off the outside of that grid, where there is no neighbour
/// to close a seam with, and 7 records the pre-skirt surface elevation span
/// used to place every level of one chunk at the same representative height,
/// and 8 retires GPU-river entries now that GPU generation uses the established
/// CPU river and waterfall builder. 9 carries the fourth material channel and
/// the independent forest-floor/stone pair.
const CACHE_FORMAT_VERSION: u32 = 9;

const MAGIC: &[u8; 8] = b"MOTUBVY\0";
/// Distinguishes a cache key from the crate's other hashed values.
const KEY_SALT: u64 = 0x4d6f_7475_4361_6368;

/// Larger entries are treated as corrupt rather than read, so a stray file
/// under the cache directory cannot make the app allocate its size.
const MAX_ENTRY_BYTES: u64 = 2 << 30;

/// Every count and scalar in the format is one of these.
const WORD: usize = size_of::<u32>();

/// The number of `IslandOptions` fields [`option_words`] yields: every scalar
/// parameter in the table, then `terrain_size`.
const OPTION_WORDS: usize = options::PARAMETERS.len() + 1;

/// Every `IslandOptions` field, in table order, as the bit patterns the key
/// hashes and an entry stores. Driving it off the parameter table is what keeps
/// a field added to `IslandOptions` from silently sharing an entry with the
/// island it differs from.
fn option_words(options: &IslandOptions) -> [u32; OPTION_WORDS] {
    let mut copy = *options;
    let mut words = [0; OPTION_WORDS];
    for (word, parameter) in words.iter_mut().zip(&options::PARAMETERS) {
        *word = (parameter.field)(&mut copy).to_bits();
    }
    words[OPTION_WORDS - 1] = options.terrain_size;
    words
}

/// Hashes the exact inputs one island is generated from. Bit patterns rather
/// than the floats themselves, so the key is total: no value compares unequal
/// to itself.
///
/// The chunk grid is one of those inputs. It is not a generator parameter and
/// not part of this file's layout, so neither the options nor the format
/// version would retire an entry when it moves — and an entry written under
/// another grid holds ground cut into other squares, with other skirts on it.
#[must_use]
pub fn key(seed: u64, options: &IslandOptions) -> u64 {
    let mut state = mix(u64::from(CACHE_FORMAT_VERSION), KEY_SALT);
    state = mix(seed, state);
    for word in option_words(options) {
        state = mix(u64::from(word), state);
    }
    state = mix(u64::from(chunk::DIVISIONS), state);
    mix(u64::from(chunk::SKIRT_METRES.to_bits()), state)
}

/// Where the entry for one method and key lives. CPU and GPU generation may
/// produce different geometry from identical inputs, so each method owns its
/// own cache namespace.
#[must_use]
pub fn path(method: GenerationMethod, key: u64) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("island-cache")
        .join(method.as_str())
        .join(format!("{key:016x}.bin"))
}

/// Reads the entry at `path` if it holds an island generated from exactly these
/// inputs. Every other outcome — missing, unreadable, oversized, truncated,
/// written under another format version, or generated from other inputs — is a
/// miss, so the caller regenerates.
#[must_use]
pub fn read(path: &Path, seed: u64, options: &IslandOptions) -> Option<IslandData> {
    if fs::metadata(path).ok()?.len() > MAX_ENTRY_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let mut reader = Reader { bytes: &bytes };
    if reader.take(MAGIC.len())? != MAGIC || reader.u32()? != CACHE_FORMAT_VERSION {
        return None;
    }
    // The inputs are compared word for word rather than through the key, so a
    // hash collision cannot hand back another island's geometry.
    if reader.u64()? != seed {
        return None;
    }
    for word in option_words(options) {
        if reader.u32()? != word {
            return None;
        }
    }
    let rivers = reader.u32()?;
    let terrain_chunks = reader.chunks()?;
    let river_mesh = reader.mesh()?;
    let river_rock_mesh = reader.mesh()?;
    let river_drops = reader.drops()?;
    let trees = reader.points()?;
    let bushes = reader.points()?;
    let heights = reader.scalars()?;
    // Anything left over means the entry was written to a layout this file no
    // longer reads, whatever version it claims.
    if !reader.bytes.is_empty() {
        return None;
    }
    Some(IslandData {
        options: *options,
        terrain_chunks,
        river_mesh,
        river_rock_mesh,
        river_drops,
        trees,
        bushes,
        heights,
        rivers,
    })
}

/// Writes one entry, creating the cache directory if it is not there yet.
pub fn write(path: &Path, seed: u64, data: &IslandData) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    // Written beside the entry and renamed over it, so a run interrupted
    // mid-write leaves no half file where the next run looks. The process id
    // keeps two concurrent runs off each other's temporary.
    let temporary = path.with_extension(format!("{}.tmp", process::id()));
    let mut writer = BufWriter::new(File::create(&temporary)?);
    writer.write_all(MAGIC)?;
    writer.write_all(&CACHE_FORMAT_VERSION.to_le_bytes())?;
    writer.write_all(&seed.to_le_bytes())?;
    for word in option_words(&data.options) {
        writer.write_all(&word.to_le_bytes())?;
    }
    writer.write_all(&data.rivers.to_le_bytes())?;
    write_chunks(&mut writer, &data.terrain_chunks)?;
    write_mesh(&mut writer, &data.river_mesh)?;
    write_mesh(&mut writer, &data.river_rock_mesh)?;
    write_drops(&mut writer, &data.river_drops)?;
    write_points(&mut writer, &data.trees)?;
    write_points(&mut writer, &data.bushes)?;
    write_count(&mut writer, data.heights.len())?;
    write_scalars(&mut writer, &data.heights)?;
    writer
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;
    fs::rename(&temporary, path)
}

/// The grid position, then every level of detail in order, in the order
/// [`Reader::chunks`] reads them back.
fn write_chunks(writer: &mut impl Write, chunks: &[TerrainChunk]) -> io::Result<()> {
    write_count(writer, chunks.len())?;
    for chunk in chunks {
        writer.write_all(&chunk.column.to_le_bytes())?;
        writer.write_all(&chunk.row.to_le_bytes())?;
        write_scalars(writer, &[chunk.surface_low, chunk.surface_high])?;
        for tier in &chunk.tiers {
            write_mesh(writer, &tier.mesh)?;
            write_vec4s(writer, &tier.materials)?;
            write_vec2s(writer, &tier.environment)?;
            write_count(writer, tier.river_wetness.len())?;
            write_scalars(writer, &tier.river_wetness)?;
        }
    }
    Ok(())
}

fn write_mesh(writer: &mut impl Write, mesh: &Mesh) -> io::Result<()> {
    write_points(writer, &mesh.vertices)?;
    write_points(writer, &mesh.normals)?;
    write_count(writer, mesh.triangles.len())?;
    for index in &mesh.triangles {
        writer.write_all(&index.to_le_bytes())?;
    }
    write_count(writer, mesh.uv.len())?;
    for uv in &mesh.uv {
        write_scalars(writer, &[uv.x, uv.y])?;
    }
    Ok(())
}

/// Nine scalars each, in the order [`Reader::drops`] reads them back.
fn write_drops(writer: &mut impl Write, drops: &[RiverDrop]) -> io::Result<()> {
    write_count(writer, drops.len())?;
    for drop in drops {
        write_scalars(
            writer,
            &[
                drop.lip.x,
                drop.lip.y,
                drop.lip.z,
                drop.foot.x,
                drop.foot.y,
                drop.foot.z,
                drop.direction.x,
                drop.direction.y,
                drop.half_width,
            ],
        )?;
    }
    Ok(())
}

fn write_points(writer: &mut impl Write, points: &[Vec3]) -> io::Result<()> {
    write_count(writer, points.len())?;
    for point in points {
        write_scalars(writer, &[point.x, point.y, point.z])?;
    }
    Ok(())
}

fn write_vec2s(writer: &mut impl Write, values: &[Vec2]) -> io::Result<()> {
    write_count(writer, values.len())?;
    for value in values {
        write_scalars(writer, &[value.x, value.y])?;
    }
    Ok(())
}

fn write_vec4s(writer: &mut impl Write, values: &[Vec4]) -> io::Result<()> {
    write_count(writer, values.len())?;
    for value in values {
        write_scalars(writer, &[value.x, value.y, value.z, value.w])?;
    }
    Ok(())
}

/// Floats cross as bit patterns, which is what the read side reverses.
fn write_scalars(writer: &mut impl Write, values: &[f32]) -> io::Result<()> {
    for value in values {
        writer.write_all(&value.to_bits().to_le_bytes())?;
    }
    Ok(())
}

fn write_count(writer: &mut impl Write, count: usize) -> io::Result<()> {
    let count = u32::try_from(count)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "array is too long to cache"))?;
    writer.write_all(&count.to_le_bytes())
}

/// A bounds-checked reader over a whole entry. Nothing it returns is trusted:
/// every length is checked against the bytes actually left before anything is
/// reserved, so a corrupt count cannot ask for an allocation the file could
/// never have held.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.bytes.split_at_checked(count)?;
        self.bytes = tail;
        Some(head)
    }

    fn u32(&mut self) -> Option<u32> {
        let word: [u8; WORD] = self.take(WORD)?.try_into().ok()?;
        Some(u32::from_le_bytes(word))
    }

    fn u64(&mut self) -> Option<u64> {
        let word: [u8; 2 * WORD] = self.take(2 * WORD)?.try_into().ok()?;
        Some(u64::from_le_bytes(word))
    }

    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }

    /// Reads a length-prefixed array whose elements are `stride` bytes each.
    fn array<T>(
        &mut self,
        stride: usize,
        mut element: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<Vec<T>> {
        let count = usize::try_from(self.u32()?).ok()?;
        if count.checked_mul(stride)? > self.bytes.len() {
            return None;
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(element(self)?);
        }
        Some(values)
    }

    fn points(&mut self) -> Option<Vec<Vec3>> {
        self.array(3 * WORD, |reader| {
            Some(Vec3::new(reader.f32()?, reader.f32()?, reader.f32()?))
        })
    }

    fn vec2s(&mut self) -> Option<Vec<Vec2>> {
        self.array(2 * WORD, |reader| {
            Some(Vec2::new(reader.f32()?, reader.f32()?))
        })
    }

    fn vec4s(&mut self) -> Option<Vec<Vec4>> {
        self.array(4 * WORD, |reader| {
            Some(Vec4::new(
                reader.f32()?,
                reader.f32()?,
                reader.f32()?,
                reader.f32()?,
            ))
        })
    }

    fn scalars(&mut self) -> Option<Vec<f32>> {
        self.array(WORD, Self::f32)
    }

    /// Field order matches [`write_drops`], and struct expressions evaluate in
    /// the order written.
    fn drops(&mut self) -> Option<Vec<RiverDrop>> {
        self.array(9 * WORD, |reader| {
            Some(RiverDrop {
                lip: Vec3::new(reader.f32()?, reader.f32()?, reader.f32()?),
                foot: Vec3::new(reader.f32()?, reader.f32()?, reader.f32()?),
                direction: Vec2::new(reader.f32()?, reader.f32()?),
                half_width: reader.f32()?,
            })
        })
    }

    /// Field order matches [`write_chunks`]. The stride is the shortest a chunk
    /// can be — its two grid coordinates, its two surface bounds, and one
    /// length prefix for each of the seven arrays a level of detail carries — so
    /// a corrupt count is rejected before anything is reserved for it.
    fn chunks(&mut self) -> Option<Vec<TerrainChunk>> {
        self.array((4 + TIERS * 7) * WORD, |reader| {
            let column = reader.u32()?;
            let row = reader.u32()?;
            let surface_low = reader.f32()?;
            let surface_high = reader.f32()?;
            let empty_bounds = surface_low.to_bits() == f32::MAX.to_bits()
                && surface_high.to_bits() == f32::MIN.to_bits();
            if !surface_low.is_finite()
                || !surface_high.is_finite()
                || (surface_low > surface_high && !empty_bounds)
            {
                return None;
            }
            let mut tiers = Vec::with_capacity(TIERS);
            for _ in 0..TIERS {
                let mesh = reader.mesh()?;
                let materials = reader.vec4s()?;
                let environment = reader.vec2s()?;
                let river_wetness = reader.scalars()?;
                if materials.len() != mesh.vertices.len()
                    || environment.len() != mesh.vertices.len()
                    || river_wetness.len() != mesh.vertices.len()
                {
                    return None;
                }
                tiers.push(ChunkTier {
                    mesh,
                    materials,
                    environment,
                    river_wetness,
                });
            }
            Some(TerrainChunk {
                column,
                row,
                surface_low,
                surface_high,
                tiers: tiers.try_into().ok()?,
            })
        })
    }

    /// Field order matches [`write_mesh`], and struct expressions evaluate in
    /// the order written.
    fn mesh(&mut self) -> Option<Mesh> {
        let mesh = Mesh {
            vertices: self.points()?,
            normals: self.points()?,
            triangles: self.array(WORD, Self::u32)?,
            uv: self.array(2 * WORD, |reader| {
                Some(Vec2::new(reader.f32()?, reader.f32()?))
            })?,
        };
        let vertices = mesh.vertices.len();
        let attributes_match = |length| length == 0 || length == vertices;
        if !mesh.triangles.len().is_multiple_of(3)
            || !mesh
                .triangles
                .iter()
                .all(|&index| usize::try_from(index).is_ok_and(|index| index < vertices))
            || !attributes_match(mesh.normals.len())
            || !attributes_match(mesh.uv.len())
        {
            return None;
        }
        Some(mesh)
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use motu::{GenerationMethod, IslandOptions, Mesh, Vec2, Vec3, Vec4};

    use super::{ChunkTier, IslandData, RiverDrop, TIERS, TerrainChunk, key, path, read, write};

    /// One chunk whose three levels are all different lengths, so a level read
    /// out of order cannot still parse.
    // Every count here is a handful, so nothing crossing to f32 loses a digit.
    #[allow(clippy::cast_precision_loss)]
    fn chunk(column: u32, row: u32) -> TerrainChunk {
        let tier = |level: usize| {
            let count = level + 1;
            ChunkTier {
                mesh: Mesh {
                    vertices: (0..count)
                        .map(|index| Vec3::splat(index as f32 + column as f32))
                        .collect(),
                    normals: vec![Vec3::Z; count],
                    triangles: vec![0; count * 3],
                    uv: (0..count)
                        .map(|index| Vec2::new(index as f32, row as f32))
                        .collect(),
                },
                materials: (0..count).map(|index| Vec4::splat(index as f32)).collect(),
                environment: vec![Vec2::new(0.5, 0.75); count],
                river_wetness: vec![0.25; count],
            }
        };
        TerrainChunk {
            column,
            row,
            surface_low: -0.02 - row as f32 * 0.01,
            surface_high: 0.2 + column as f32 * 0.01,
            tiers: std::array::from_fn(tier),
        }
    }

    /// A path of its own per test, so the suite never touches the real cache
    /// and its own entries cannot collide when tests run in parallel.
    fn scratch(name: &str) -> std::path::PathBuf {
        env::temp_dir().join(format!(
            "island-bevy-cache-{}-{name}.bin",
            std::process::id()
        ))
    }

    /// Small, but with every array non-empty and none of them the same length,
    /// so a field read out of order cannot still parse.
    fn island() -> IslandData {
        IslandData {
            options: IslandOptions {
                terrain_size: 64,
                max_height: 0.3,
                ..IslandOptions::default()
            },
            // Three chunks rather than a full grid: the format stores the count
            // it was written with, and the reader is what the round trip has to
            // hold, not the grid size.
            terrain_chunks: vec![chunk(0, 0), chunk(1, 0), chunk(0, 1)],
            river_mesh: Mesh {
                vertices: vec![Vec3::X, Vec3::Y],
                normals: vec![Vec3::Z; 2],
                triangles: vec![0, 1, 0],
                uv: vec![Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0)],
            },
            river_rock_mesh: Mesh {
                vertices: vec![Vec3::splat(0.5)],
                normals: Vec::new(),
                triangles: Vec::new(),
                uv: Vec::new(),
            },
            // Two drops, no two fields alike, so a scalar written or read out
            // of order lands somewhere the assertions can see it.
            river_drops: vec![
                RiverDrop {
                    lip: Vec3::new(0.11, 0.12, 0.013),
                    foot: Vec3::new(0.14, 0.15, 0.006),
                    direction: Vec2::new(0.6, 0.8),
                    half_width: 0.001_25,
                },
                RiverDrop {
                    lip: Vec3::new(0.71, 0.72, 0.043),
                    foot: Vec3::new(0.73, 0.74, 0.021),
                    direction: Vec2::new(-1.0, 0.0),
                    half_width: 0.003_5,
                },
            ],
            trees: vec![Vec3::new(0.1, 0.2, 0.3), Vec3::new(0.4, 0.5, 0.6)],
            bushes: vec![Vec3::new(0.7, 0.8, 0.9)],
            heights: vec![-0.01, 0.0, 0.125, 0.5, -0.25],
            rivers: 7,
        }
    }

    #[test]
    fn round_trips_an_island() {
        let path = scratch("round-trip");
        let data = island();
        write(&path, 42, &data).unwrap();
        let read_back = read(&path, 42, &data.options);
        assert_eq!(read_back, Some(data.clone()));
        // The two per-vertex arrays cross as written. Both are read back by
        // index against another array, so a shift or a truncation in either
        // would only show as ground that is wet or sunken somewhere else.
        let read_back = read_back.unwrap();
        assert_eq!(read_back.heights, data.heights);
        // Every chunk crosses with its grid position and all three of its
        // levels, in order: a level read out of order would put coarse ground
        // where the near view draws.
        assert_eq!(read_back.terrain_chunks.len(), 3);
        for (read_back, original) in read_back.terrain_chunks.iter().zip(&data.terrain_chunks) {
            assert_eq!(
                (read_back.column, read_back.row),
                (original.column, original.row)
            );
            for level in 0..TIERS {
                assert_eq!(
                    read_back.tiers[level], original.tiers[level],
                    "level {level}"
                );
            }
        }
        // Every drop is nine scalars in one flat run, so a field crossing out
        // of order would still parse and would place a fall somewhere else.
        assert_eq!(read_back.river_drops, data.river_drops);
        // The same entry read under other inputs is a miss, not other geometry.
        assert_eq!(read(&path, 43, &data.options), None);
        assert_eq!(read(&path, 42, &IslandOptions::default()), None);
        fs::remove_file(&path).unwrap();
    }

    /// Damage anywhere is a miss rather than a panic or a partial island.
    #[test]
    fn a_corrupt_entry_is_a_miss() {
        let path = scratch("corrupt");
        let data = island();
        write(&path, 42, &data).unwrap();
        let sound = fs::read(&path).unwrap();

        for (name, damaged) in [
            ("empty", Vec::new()),
            ("wrong magic", {
                let mut bytes = sound.clone();
                bytes[0] = b'X';
                bytes
            }),
            ("wrong version", {
                let mut bytes = sound.clone();
                bytes[MAGIC_END] = 0xff;
                bytes
            }),
            ("truncated", sound[..sound.len() / 2].to_vec()),
            ("trailing bytes", {
                let mut bytes = sound.clone();
                bytes.push(0);
                bytes
            }),
            ("oversized array count", {
                // The chunk count, which is the first array past the header.
                let mut bytes = sound.clone();
                bytes[ARRAYS_START..ARRAYS_START + 4].copy_from_slice(&u32::MAX.to_le_bytes());
                bytes
            }),
        ] {
            fs::write(&path, &damaged).unwrap();
            assert_eq!(read(&path, 42, &data.options), None, "{name} was read back");
        }

        fs::remove_file(&path).unwrap();
        assert_eq!(read(&path, 42, &data.options), None, "a missing entry");
    }

    /// Index buffers and optional per-vertex attributes cross the cache as
    /// independent arrays, so damage to one must not be handed to Bevy as a
    /// different mesh shape.
    #[test]
    fn a_structurally_invalid_mesh_is_a_miss() {
        let path = scratch("invalid-mesh");
        let original = island();
        for (name, mesh) in [
            ("out-of-bounds index", {
                let mut mesh = original.river_mesh.clone();
                mesh.triangles[1] = u32::MAX;
                mesh
            }),
            ("incomplete triangle", {
                let mut mesh = original.river_mesh.clone();
                mesh.triangles.pop();
                mesh
            }),
            ("partial normals", {
                let mut mesh = original.river_mesh.clone();
                mesh.normals.pop();
                mesh
            }),
            ("partial UVs", {
                let mut mesh = original.river_mesh.clone();
                mesh.uv.pop();
                mesh
            }),
        ] {
            let mut data = original.clone();
            data.river_mesh = mesh;
            write(&path, 42, &data).unwrap();
            assert_eq!(read(&path, 42, &data.options), None, "accepted {name}");
        }

        fs::remove_file(path).unwrap();
    }

    /// Terrain material and wetness values are indexed by terrain vertex and
    /// have no meaningful fallback in a cached, already-generated chunk.
    #[test]
    fn mismatched_chunk_vertex_fields_are_a_miss() {
        let path = scratch("invalid-chunk-fields");
        let original = island();

        for wetness in [false, true] {
            let mut data = original.clone();
            let tier = &mut data.terrain_chunks[0].tiers[0];
            if wetness {
                tier.river_wetness.pop();
            } else {
                tier.materials.pop();
            }
            write(&path, 42, &data).unwrap();
            assert_eq!(
                read(&path, 42, &data.options),
                None,
                "accepted mismatched {}",
                if wetness { "wetness" } else { "materials" }
            );
        }

        fs::remove_file(path).unwrap();
    }

    /// The chunk origin uses these bounds directly in an entity transform; a
    /// corrupt non-finite word must remain a cache miss. The inverted finite
    /// MAX/MIN pair used by an empty chunk is deliberately still valid.
    #[test]
    fn invalid_chunk_surface_bounds_are_a_miss() {
        let path = scratch("invalid-chunk-bounds");
        let original = island();

        for high in [false, true] {
            let mut data = original.clone();
            if high {
                data.terrain_chunks[0].surface_high = f32::INFINITY;
            } else {
                data.terrain_chunks[0].surface_low = f32::NAN;
            }
            write(&path, 42, &data).unwrap();
            assert_eq!(
                read(&path, 42, &data.options),
                None,
                "accepted a non-finite {} bound",
                if high { "high" } else { "low" }
            );
        }

        let mut inverted = original.clone();
        inverted.terrain_chunks[0].surface_low = 0.5;
        inverted.terrain_chunks[0].surface_high = 0.2;
        write(&path, 42, &inverted).unwrap();
        assert_eq!(
            read(&path, 42, &inverted.options),
            None,
            "accepted inverted non-empty bounds"
        );

        let mut empty = original.clone();
        empty.terrain_chunks[0].surface_low = f32::MAX;
        empty.terrain_chunks[0].surface_high = f32::MIN;
        write(&path, 42, &empty).unwrap();
        assert_eq!(read(&path, 42, &empty.options), Some(empty));

        fs::remove_file(path).unwrap();
    }

    /// Offsets into the header the damage tests reach for: the magic, then the
    /// format version, the seed, the option words and the river count.
    const MAGIC_END: usize = 8;
    const ARRAYS_START: usize = MAGIC_END + 4 + 8 + super::OPTION_WORDS * 4 + 4;

    #[test]
    fn every_input_changes_the_key() {
        let options = IslandOptions::default();
        let base = key(1, &options);
        assert_ne!(base, key(2, &options));
        for changed in [
            IslandOptions {
                max_height: 0.21,
                ..options
            },
            IslandOptions {
                water_ratio: 0.61,
                ..options
            },
            IslandOptions {
                terrain_size: 128,
                ..options
            },
            IslandOptions {
                hydraulic_erosion_strength: 4.0,
                ..options
            },
            IslandOptions {
                river_maximum_depth_metres: 2.5,
                ..options
            },
        ] {
            assert_ne!(base, key(1, &changed), "{changed:?} kept the key");
        }
    }

    #[test]
    fn generation_methods_have_separate_cache_directories() {
        let key = key(1, &IslandOptions::default());
        let cpu = path(GenerationMethod::Cpu, key);
        let gpu = path(GenerationMethod::Gpu, key);
        assert_ne!(cpu, gpu);
        assert_eq!(
            cpu.parent().and_then(std::path::Path::file_name),
            Some(std::ffi::OsStr::new("cpu"))
        );
        assert_eq!(
            gpu.parent().and_then(std::path::Path::file_name),
            Some(std::ffi::OsStr::new("gpu"))
        );
    }
}
