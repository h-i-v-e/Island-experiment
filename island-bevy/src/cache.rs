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

use motu::{IslandOptions, Mesh, Vec2, Vec3};

use crate::{hash::mix, island_gen::IslandData};

/// Mixed into every key and written into every entry. Bump it when the
/// generator's output changes, when the layout of `IslandOptions` changes, or
/// when this file's format changes: entries written before the bump then hash
/// to a different key, so none of them is ever read again.
const CACHE_FORMAT_VERSION: u32 = 1;

const MAGIC: &[u8; 8] = b"MOTUBVY\0";
/// Distinguishes a cache key from the crate's other hashed values.
const KEY_SALT: u64 = 0x4d6f_7475_4361_6368;

/// Larger entries are treated as corrupt rather than read, so a stray file
/// under the cache directory cannot make the app allocate its size.
const MAX_ENTRY_BYTES: u64 = 2 << 30;

/// Every count and scalar in the format is one of these.
const WORD: usize = size_of::<u32>();

/// The number of `IslandOptions` fields [`option_words`] lists.
const OPTION_WORDS: usize = 15;

/// Every `IslandOptions` field, in declaration order, as the bit patterns the
/// key hashes and an entry stores.
///
/// The list has to match `IslandOptions` by hand: a field added there and
/// forgotten here is not a compile error, and two islands differing only in
/// that field would then share one entry.
fn option_words(options: &IslandOptions) -> [u32; OPTION_WORDS] {
    [
        options.max_height.to_bits(),
        options.water_ratio.to_bits(),
        options.slope_multiplier.to_bits(),
        options.coastal_slope_multiplier.to_bits(),
        options.hydraulic_erosion_strength.to_bits(),
        options.hydraulic_deposition_strength.to_bits(),
        options.hydraulic_deposition_slope_degrees.to_bits(),
        options.river_source_catchment_hectares.to_bits(),
        options.river_source_steep_multiplier.to_bits(),
        options.river_source_elevation_boost.to_bits(),
        options.river_source_width_metres.to_bits(),
        options.river_maximum_width_metres.to_bits(),
        options.river_source_depth_metres.to_bits(),
        options.river_maximum_depth_metres.to_bits(),
        options.terrain_size,
    ]
}

/// Hashes the exact inputs one island is generated from. Bit patterns rather
/// than the floats themselves, so the key is total: no value compares unequal
/// to itself.
#[must_use]
pub fn key(seed: u64, options: &IslandOptions) -> u64 {
    let mut state = mix(u64::from(CACHE_FORMAT_VERSION), KEY_SALT);
    state = mix(seed, state);
    for word in option_words(options) {
        state = mix(u64::from(word), state);
    }
    state
}

/// Where the entry for one key lives.
#[must_use]
pub fn path(key: u64) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("island-cache")
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
    let terrain = reader.mesh()?;
    let materials = reader.points()?;
    let river_mesh = reader.mesh()?;
    let river_rock_mesh = reader.mesh()?;
    let trees = reader.points()?;
    let bushes = reader.points()?;
    // Anything left over means the entry was written to a layout this file no
    // longer reads, whatever version it claims.
    if !reader.bytes.is_empty() {
        return None;
    }
    Some(IslandData {
        options: *options,
        terrain,
        materials,
        river_mesh,
        river_rock_mesh,
        trees,
        bushes,
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
    write_mesh(&mut writer, &data.terrain)?;
    write_points(&mut writer, &data.materials)?;
    write_mesh(&mut writer, &data.river_mesh)?;
    write_mesh(&mut writer, &data.river_rock_mesh)?;
    write_points(&mut writer, &data.trees)?;
    write_points(&mut writer, &data.bushes)?;
    writer
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;
    fs::rename(&temporary, path)
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

fn write_points(writer: &mut impl Write, points: &[Vec3]) -> io::Result<()> {
    write_count(writer, points.len())?;
    for point in points {
        write_scalars(writer, &[point.x, point.y, point.z])?;
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

    /// Field order matches [`write_mesh`], and struct expressions evaluate in
    /// the order written.
    fn mesh(&mut self) -> Option<Mesh> {
        Some(Mesh {
            vertices: self.points()?,
            normals: self.points()?,
            triangles: self.array(WORD, Self::u32)?,
            uv: self.array(2 * WORD, |reader| {
                Some(Vec2::new(reader.f32()?, reader.f32()?))
            })?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use motu::{IslandOptions, Mesh, Vec2, Vec3};

    use super::{IslandData, key, read, write};

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
            terrain: Mesh {
                vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z],
                normals: vec![Vec3::Z, Vec3::Z, Vec3::Z, Vec3::Z],
                triangles: vec![0, 1, 2, 0, 2, 3],
                uv: vec![Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::new(0.25, 0.75)],
            },
            materials: vec![
                Vec3::new(0.5, 1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.25),
                Vec3::new(0.0, 0.5, 1.0),
                Vec3::new(0.125, 0.25, 0.5),
            ],
            river_mesh: Mesh {
                vertices: vec![Vec3::X, Vec3::Y],
                normals: vec![Vec3::Z],
                triangles: vec![0, 1, 0],
                uv: vec![Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0)],
            },
            river_rock_mesh: Mesh {
                vertices: vec![Vec3::splat(0.5)],
                normals: Vec::new(),
                triangles: Vec::new(),
                uv: Vec::new(),
            },
            trees: vec![Vec3::new(0.1, 0.2, 0.3), Vec3::new(0.4, 0.5, 0.6)],
            bushes: vec![Vec3::new(0.7, 0.8, 0.9)],
            rivers: 7,
        }
    }

    #[test]
    fn round_trips_an_island() {
        let path = scratch("round-trip");
        let data = island();
        write(&path, 42, &data).unwrap();
        assert_eq!(read(&path, 42, &data.options), Some(data.clone()));
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
                // The count of the terrain mesh's vertices, which is the first
                // array past the header.
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
}
