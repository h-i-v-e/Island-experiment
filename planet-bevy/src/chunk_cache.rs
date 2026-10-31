//! Versioned, checksummed surface storage. Writes borrow geometry and optional
//! atlas buffers; reads own validated payloads for worker tasks. Native struct
//! layouts are never stored.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use planet_rs::{
    DVec3,
    chunks::{ChunkBounds, ChunkMesh, ChunkVertex},
};

const MAGIC: &[u8; 8] = b"PLNCHUNK";
const VERSION: u32 = 3;
const HEADER_PREFIX_BYTES: usize = 44;
const HEADER_BYTES: u64 = 52;
const VERTEX_BYTES: u64 = 65;
const TRIANGLE_BYTES: u64 = 12;
// Cache corruption/allocation guard for one file, not a planet vertex budget.
// Payload counts are checked against their encoded size and actual file length.
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ATLAS_DIMENSION: u32 = 8192;
const MAX_ATLAS_BYTES: u64 = MAX_FILE_BYTES;
const UV_BYTES: u64 = 8;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

const INDEX_MAGIC: &[u8; 8] = b"PLNINDEX";
const INDEX_PREFIX_BYTES: usize = 32;
const INDEX_HEADER_BYTES: usize = 40;
const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CHUNKS: usize = 100_000;
const MAX_LODS: usize = 16;

#[derive(Debug)]
pub struct CachedChunk {
    pub id: u32,
    pub layer: u8,
    pub bounds: ChunkBounds,
    pub errors: Vec<f64>,
    pub triangles: Vec<usize>,
    pub vertex_counts: Vec<usize>,
    pub atlas_sizes: Vec<[u32; 2]>,
}

#[derive(Debug)]
pub struct CachedWorld {
    pub occluder_radius: f64,
    pub chunks: Vec<CachedChunk>,
}

/// Linear RGBA texture: signed normal correction RGB and ambient occlusion alpha.
#[derive(Debug, PartialEq)]
pub struct SurfaceAtlasData {
    pub uv: Vec<[f32; 2]>,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub rivers: Vec<u8>,
}

#[derive(Debug)]
pub struct CachedSurface {
    pub mesh: ChunkMesh,
    pub atlas: Option<SurfaceAtlasData>,
}

#[derive(Debug, Clone)]
pub struct ChunkCache {
    directory: PathBuf,
    key: u64,
}

impl ChunkCache {
    #[must_use]
    pub const fn new(directory: PathBuf, key: u64) -> Self {
        Self { directory, key }
    }

    #[must_use]
    pub fn path(&self, chunk: u32, lod: usize) -> PathBuf {
        self.directory
            .join(format!("v{VERSION}-{:016x}-{chunk}-{lod}.mesh", self.key))
    }

    pub fn write(&self, chunk: u32, lod: usize, mesh: &ChunkMesh) -> io::Result<()> {
        self.write_surface(chunk, lod, mesh, None)
    }

    pub fn write_surface(
        &self,
        chunk: u32,
        lod: usize,
        mesh: &ChunkMesh,
        atlas: Option<&SurfaceAtlasData>,
    ) -> io::Result<()> {
        validate_mesh(mesh)?;
        let lod = u32::try_from(lod).map_err(|_| invalid("LOD is out of range"))?;
        let vertices =
            u32::try_from(mesh.vertices.len()).map_err(|_| invalid("too many vertices"))?;
        let triangles =
            u32::try_from(mesh.triangles.len()).map_err(|_| invalid("too many triangles"))?;
        let (width, height) = atlas.map_or((0, 0), |atlas| (atlas.width, atlas.height));
        validate_counts(vertices, triangles, width, height)?;
        if let Some(atlas) = atlas {
            validate_atlas(atlas, mesh.vertices.len())?;
        }
        fs::create_dir_all(&self.directory)?;
        let destination = self.path(chunk, lod as usize);
        let temporary = destination.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let result = self
            .write_temporary(&temporary, chunk, lod, mesh, atlas)
            .and_then(|()| fs::rename(&temporary, &destination));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn write_temporary(
        &self,
        path: &Path,
        chunk: u32,
        lod: u32,
        mesh: &ChunkMesh,
        atlas: Option<&SurfaceAtlasData>,
    ) -> io::Result<()> {
        let mut writer =
            BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);
        let prefix = self.header_prefix(chunk, lod, mesh, atlas);
        writer.write_all(&prefix)?;
        writer.write_all(&0_u64.to_le_bytes())?;
        let mut checksum = hash(FNV_OFFSET, &prefix);
        for vertex in &mesh.vertices {
            for value in vertex
                .position
                .to_array()
                .into_iter()
                .chain(vertex.normal.to_array())
            {
                write_hashed(&mut writer, &mut checksum, &value.to_le_bytes())?;
            }
            for value in vertex.colour {
                write_hashed(&mut writer, &mut checksum, &value.to_le_bytes())?;
            }
            write_hashed(&mut writer, &mut checksum, &[u8::from(vertex.protected)])?;
        }
        for triangle in &mesh.triangles {
            for index in triangle {
                write_hashed(&mut writer, &mut checksum, &index.to_le_bytes())?;
            }
        }
        if let Some(atlas) = atlas {
            for uv in &atlas.uv {
                for value in uv {
                    write_hashed(&mut writer, &mut checksum, &value.to_le_bytes())?;
                }
            }
            write_hashed(&mut writer, &mut checksum, &atlas.pixels)?;
            write_hashed(&mut writer, &mut checksum, &atlas.rivers)?;
        }
        writer.seek(SeekFrom::Start(HEADER_PREFIX_BYTES as u64))?;
        writer.write_all(&checksum.to_le_bytes())?;
        writer.flush()?;
        writer.get_ref().sync_all()
    }

    pub fn read(&self, chunk: u32, lod: usize) -> io::Result<ChunkMesh> {
        self.read_surface(chunk, lod).map(|surface| surface.mesh)
    }

    pub fn read_surface(&self, chunk: u32, lod: usize) -> io::Result<CachedSurface> {
        let file = File::open(self.path(chunk, lod))?;
        let file_bytes = file.metadata()?.len();
        if !(HEADER_BYTES..=MAX_FILE_BYTES).contains(&file_bytes) {
            return Err(invalid("invalid chunk file length"));
        }
        let mut reader = BufReader::new(file);
        let mut prefix = [0_u8; HEADER_PREFIX_BYTES];
        reader.read_exact(&mut prefix)?;
        if &prefix[..8] != MAGIC || u32_at(&prefix, 8) != VERSION {
            return Err(invalid("unknown chunk cache format"));
        }
        let key = u64::from_le_bytes(prefix[12..20].try_into().expect("fixed header field"));
        if key != self.key
            || u32_at(&prefix, 20) != chunk
            || usize::try_from(u32_at(&prefix, 24)) != Ok(lod)
        {
            return Err(invalid("chunk cache identity mismatch"));
        }
        let vertex_count = u32_at(&prefix, 28);
        let triangle_count = u32_at(&prefix, 32);
        let width = u32_at(&prefix, 36);
        let height = u32_at(&prefix, 40);
        let expected_bytes = validate_counts(vertex_count, triangle_count, width, height)?;
        if file_bytes != expected_bytes {
            return Err(invalid("truncated or oversized chunk payload"));
        }
        let mut checksum_bytes = [0_u8; 8];
        reader.read_exact(&mut checksum_bytes)?;
        let expected_checksum = u64::from_le_bytes(checksum_bytes);
        let mut checksum = hash(FNV_OFFSET, &prefix);
        let vertices = (0..vertex_count)
            .map(|_| read_vertex(&mut reader, &mut checksum))
            .collect::<io::Result<Vec<_>>>()?;
        let triangles = (0..triangle_count)
            .map(|_| {
                Ok([
                    u32::from_le_bytes(read_hashed(&mut reader, &mut checksum)?),
                    u32::from_le_bytes(read_hashed(&mut reader, &mut checksum)?),
                    u32::from_le_bytes(read_hashed(&mut reader, &mut checksum)?),
                ])
            })
            .collect::<io::Result<Vec<_>>>()?;
        let atlas = read_atlas(&mut reader, &mut checksum, vertex_count, width, height)?;
        if checksum != expected_checksum {
            return Err(invalid("chunk checksum mismatch"));
        }
        let mesh = ChunkMesh {
            vertices,
            triangles,
        };
        validate_mesh(&mesh)?;
        Ok(CachedSurface { mesh, atlas })
    }

    fn index_path(&self) -> PathBuf {
        self.directory
            .join(format!("v{VERSION}-{:016x}.index", self.key))
    }

    /// Forget the publication marker without deleting any reusable mesh data.
    pub fn invalidate_index(&self) -> io::Result<()> {
        match fs::remove_file(self.index_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn validate_references(&self, world: &CachedWorld) -> io::Result<()> {
        for chunk in &world.chunks {
            for lod in 0..chunk.errors.len() {
                if !self.path(chunk.id, lod).is_file() {
                    return Err(invalid("world index references a missing chunk LOD"));
                }
            }
        }
        Ok(())
    }

    /// Publish the index only after every referenced LOD has been written.
    pub fn write_index(&self, world: &CachedWorld) -> io::Result<()> {
        validate_world(world)?;
        self.validate_references(world)?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(INDEX_MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.key.to_le_bytes());
        bytes.extend_from_slice(
            &u32::try_from(world.chunks.len())
                .expect("validated count")
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&world.occluder_radius.to_le_bytes());
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        for chunk in &world.chunks {
            bytes.extend_from_slice(&chunk.id.to_le_bytes());
            bytes.push(chunk.layer);
            for value in chunk
                .bounds
                .centre
                .to_array()
                .into_iter()
                .chain([chunk.bounds.radius])
                .chain(chunk.bounds.direction.to_array())
                .chain([chunk.bounds.half_angle, chunk.bounds.max_radius])
            {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes.extend_from_slice(
                &u32::try_from(chunk.errors.len())
                    .expect("validated LOD count")
                    .to_le_bytes(),
            );
            for (((&error, &triangles), &vertices), &[width, height]) in chunk
                .errors
                .iter()
                .zip(&chunk.triangles)
                .zip(&chunk.vertex_counts)
                .zip(&chunk.atlas_sizes)
            {
                bytes.extend_from_slice(&error.to_le_bytes());
                bytes.extend_from_slice(
                    &u64::try_from(triangles)
                        .expect("validated triangle count")
                        .to_le_bytes(),
                );
                bytes.extend_from_slice(
                    &u32::try_from(vertices)
                        .expect("validated vertex count")
                        .to_le_bytes(),
                );
                bytes.extend_from_slice(&width.to_le_bytes());
                bytes.extend_from_slice(&height.to_le_bytes());
            }
        }
        let checksum = hash(
            hash(FNV_OFFSET, &bytes[..INDEX_PREFIX_BYTES]),
            &bytes[INDEX_HEADER_BYTES..],
        );
        bytes[INDEX_PREFIX_BYTES..INDEX_HEADER_BYTES].copy_from_slice(&checksum.to_le_bytes());
        fs::create_dir_all(&self.directory)?;
        atomic_write_bytes(&self.index_path(), &bytes)
    }

    pub fn read_index(&self) -> io::Result<CachedWorld> {
        let file = File::open(self.index_path())?;
        if !(INDEX_HEADER_BYTES as u64..=MAX_INDEX_BYTES).contains(&file.metadata()?.len()) {
            return Err(invalid("invalid world index size"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_INDEX_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() < INDEX_HEADER_BYTES || bytes.len() as u64 > MAX_INDEX_BYTES {
            return Err(invalid("invalid world index size"));
        }
        if &bytes[..8] != INDEX_MAGIC || u32_at(&bytes, 8) != VERSION {
            return Err(invalid("unknown world index format"));
        }
        if u64::from_le_bytes(bytes[12..20].try_into().expect("fixed field")) != self.key {
            return Err(invalid("world cache identity mismatch"));
        }
        let count = u32_at(&bytes, 20) as usize;
        if count > MAX_CHUNKS {
            return Err(invalid("too many indexed chunks"));
        }
        let expected = u64::from_le_bytes(
            bytes[INDEX_PREFIX_BYTES..INDEX_HEADER_BYTES]
                .try_into()
                .expect("fixed field"),
        );
        let actual = hash(
            hash(FNV_OFFSET, &bytes[..INDEX_PREFIX_BYTES]),
            &bytes[INDEX_HEADER_BYTES..],
        );
        if actual != expected {
            return Err(invalid("world index checksum mismatch"));
        }
        let occluder_radius = f64::from_le_bytes(bytes[24..32].try_into().expect("fixed field"));
        let mut reader = Cursor::new(&bytes[INDEX_HEADER_BYTES..]);
        let chunks = (0..count)
            .map(|_| read_cached_chunk(&mut reader))
            .collect::<io::Result<Vec<_>>>()?;
        if reader.position() != (bytes.len() - INDEX_HEADER_BYTES) as u64 {
            return Err(invalid("unexpected trailing world index bytes"));
        }
        let world = CachedWorld {
            occluder_radius,
            chunks,
        };
        validate_world(&world)?;
        self.validate_references(&world)?;
        Ok(world)
    }

    fn header_prefix(
        &self,
        chunk: u32,
        lod: u32,
        mesh: &ChunkMesh,
        atlas: Option<&SurfaceAtlasData>,
    ) -> [u8; HEADER_PREFIX_BYTES] {
        let mut prefix = [0_u8; HEADER_PREFIX_BYTES];
        prefix[..8].copy_from_slice(MAGIC);
        prefix[8..12].copy_from_slice(&VERSION.to_le_bytes());
        prefix[12..20].copy_from_slice(&self.key.to_le_bytes());
        prefix[20..24].copy_from_slice(&chunk.to_le_bytes());
        prefix[24..28].copy_from_slice(&lod.to_le_bytes());
        let vertices = u32::try_from(mesh.vertices.len()).expect("validated vertex count");
        let triangles = u32::try_from(mesh.triangles.len()).expect("validated triangle count");
        prefix[28..32].copy_from_slice(&vertices.to_le_bytes());
        prefix[32..36].copy_from_slice(&triangles.to_le_bytes());
        if let Some(atlas) = atlas {
            prefix[36..40].copy_from_slice(&atlas.width.to_le_bytes());
            prefix[40..44].copy_from_slice(&atlas.height.to_le_bytes());
        }
        prefix
    }
}

fn atomic_write_bytes(destination: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = destination.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_world(world: &CachedWorld) -> io::Result<()> {
    if !world.occluder_radius.is_finite()
        || world.occluder_radius < 0.0
        || world.chunks.len() > MAX_CHUNKS
    {
        return Err(invalid("invalid world index bounds"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for chunk in &world.chunks {
        let bounds = &chunk.bounds;
        if chunk.layer > 2
            || chunk.id / 100_000 != u32::from(chunk.layer)
            || !ids.insert(chunk.id)
            || !bounds.centre.is_finite()
            || !bounds.direction.is_finite()
            || (bounds.direction.length_squared() - 1.0).abs() > 1.0e-6
            || !bounds.radius.is_finite()
            || bounds.radius < 0.0
            || !bounds.max_radius.is_finite()
            || bounds.max_radius <= 0.0
            || !bounds.half_angle.is_finite()
            || !(0.0..=std::f64::consts::PI).contains(&bounds.half_angle)
        {
            return Err(invalid("invalid indexed chunk bounds or identity"));
        }
        if chunk.errors.is_empty()
            || chunk.errors.len() > MAX_LODS
            || chunk.triangles.len() != chunk.errors.len()
            || chunk.vertex_counts.len() != chunk.errors.len()
            || chunk.atlas_sizes.len() != chunk.errors.len()
            || chunk.errors[0] != 0.0
            || chunk
                .errors
                .iter()
                .any(|error| !error.is_finite() || *error < 0.0)
            || chunk.errors.windows(2).any(|pair| pair[1] < pair[0])
        {
            return Err(invalid("invalid indexed chunk LOD metadata"));
        }
        for ((&vertices, &triangles), &[width, height]) in chunk
            .vertex_counts
            .iter()
            .zip(&chunk.triangles)
            .zip(&chunk.atlas_sizes)
        {
            validate_counts(
                u32::try_from(vertices).map_err(|_| invalid("indexed vertex count overflow"))?,
                u32::try_from(triangles).map_err(|_| invalid("indexed triangle count overflow"))?,
                width,
                height,
            )?;
        }
    }
    Ok(())
}

fn read_value<const N: usize>(reader: &mut impl Read) -> io::Result<[u8; N]> {
    let mut bytes = [0_u8; N];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_cached_chunk(reader: &mut impl Read) -> io::Result<CachedChunk> {
    let id = u32::from_le_bytes(read_value(reader)?);
    let [layer] = read_value(reader)?;
    let centre = DVec3::new(
        f64::from_le_bytes(read_value(reader)?),
        f64::from_le_bytes(read_value(reader)?),
        f64::from_le_bytes(read_value(reader)?),
    );
    let radius = f64::from_le_bytes(read_value(reader)?);
    let direction = DVec3::new(
        f64::from_le_bytes(read_value(reader)?),
        f64::from_le_bytes(read_value(reader)?),
        f64::from_le_bytes(read_value(reader)?),
    );
    let half_angle = f64::from_le_bytes(read_value(reader)?);
    let max_radius = f64::from_le_bytes(read_value(reader)?);
    let levels = u32::from_le_bytes(read_value(reader)?) as usize;
    if levels == 0 || levels > MAX_LODS {
        return Err(invalid("invalid index LOD count"));
    }
    let mut errors = Vec::with_capacity(levels);
    let mut triangles = Vec::with_capacity(levels);
    let mut vertex_counts = Vec::with_capacity(levels);
    let mut atlas_sizes = Vec::with_capacity(levels);
    for _ in 0..levels {
        errors.push(f64::from_le_bytes(read_value(reader)?));
        triangles.push(
            usize::try_from(u64::from_le_bytes(read_value(reader)?))
                .map_err(|_| invalid("triangle count overflow"))?,
        );
        vertex_counts.push(u32::from_le_bytes(read_value(reader)?) as usize);
        atlas_sizes.push([
            u32::from_le_bytes(read_value(reader)?),
            u32::from_le_bytes(read_value(reader)?),
        ]);
    }
    Ok(CachedChunk {
        id,
        layer,
        bounds: ChunkBounds {
            centre,
            radius,
            direction,
            half_angle,
            max_radius,
        },
        errors,
        triangles,
        vertex_counts,
        atlas_sizes,
    })
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed header field"),
    )
}

fn atlas_pixel_bytes(width: u32, height: u32) -> io::Result<u64> {
    if width == 0 || height == 0 || width > MAX_ATLAS_DIMENSION || height > MAX_ATLAS_DIMENSION {
        return Err(invalid("invalid atlas dimensions"));
    }
    let bytes = u64::from(width) * u64::from(height) * 4;
    if bytes > MAX_ATLAS_BYTES {
        return Err(invalid("atlas exceeds allocation limit"));
    }
    Ok(bytes)
}

fn validate_counts(vertices: u32, triangles: u32, width: u32, height: u32) -> io::Result<u64> {
    let atlas_bytes = if width == 0 && height == 0 {
        0
    } else {
        atlas_pixel_bytes(width, height)? * 5 / 4 + u64::from(vertices) * UV_BYTES
    };
    let bytes = HEADER_BYTES
        + u64::from(vertices) * VERTEX_BYTES
        + u64::from(triangles) * TRIANGLE_BYTES
        + atlas_bytes;
    if bytes > MAX_FILE_BYTES {
        return Err(invalid("chunk exceeds allocation limits"));
    }
    Ok(bytes)
}

fn validate_atlas(atlas: &SurfaceAtlasData, vertices: usize) -> io::Result<()> {
    if atlas_pixel_bytes(atlas.width, atlas.height)? != atlas.pixels.len() as u64
        || atlas.rivers.len() as u64 != u64::from(atlas.width) * u64::from(atlas.height)
        || atlas.uv.len() != vertices
        || atlas
            .uv
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(invalid("invalid atlas payload or UV coordinates"));
    }
    Ok(())
}

fn read_atlas(
    reader: &mut impl Read,
    checksum: &mut u64,
    vertices: u32,
    width: u32,
    height: u32,
) -> io::Result<Option<SurfaceAtlasData>> {
    if width == 0 && height == 0 {
        return Ok(None);
    }
    let pixel_bytes = usize::try_from(atlas_pixel_bytes(width, height)?)
        .map_err(|_| invalid("atlas size overflow"))?;
    let uv = (0..vertices)
        .map(|_| {
            Ok([
                f32::from_le_bytes(read_hashed(reader, checksum)?),
                f32::from_le_bytes(read_hashed(reader, checksum)?),
            ])
        })
        .collect::<io::Result<Vec<_>>>()?;
    let mut pixels = vec![0; pixel_bytes];
    reader.read_exact(&mut pixels)?;
    *checksum = hash(*checksum, &pixels);
    let mut rivers = vec![0; pixel_bytes / 4];
    reader.read_exact(&mut rivers)?;
    *checksum = hash(*checksum, &rivers);
    let atlas = SurfaceAtlasData {
        uv,
        width,
        height,
        pixels,
        rivers,
    };
    validate_atlas(&atlas, vertices as usize)?;
    Ok(Some(atlas))
}

fn validate_mesh(mesh: &ChunkMesh) -> io::Result<()> {
    if mesh.vertices.iter().any(|vertex| {
        !vertex.position.is_finite()
            || !vertex.normal.is_finite()
            || vertex.normal.length_squared() <= 0.0
            || !vertex.colour.iter().all(|value| value.is_finite())
    }) {
        return Err(invalid("chunk contains invalid vertex attributes"));
    }
    for &[a, b, c] in &mesh.triangles {
        let (Some(a), Some(b), Some(c)) = (
            mesh.vertices.get(a as usize),
            mesh.vertices.get(b as usize),
            mesh.vertices.get(c as usize),
        ) else {
            return Err(invalid("chunk triangle index is out of bounds"));
        };
        let area = (b.position - a.position)
            .cross(c.position - a.position)
            .length_squared();
        if !area.is_finite() || area <= 0.0 {
            return Err(invalid("chunk contains degenerate triangles"));
        }
    }
    Ok(())
}

fn hash(mut checksum: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        checksum = (checksum ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
    }
    checksum
}

fn write_hashed(writer: &mut impl Write, checksum: &mut u64, bytes: &[u8]) -> io::Result<()> {
    writer.write_all(bytes)?;
    *checksum = hash(*checksum, bytes);
    Ok(())
}

fn read_hashed<const N: usize>(reader: &mut impl Read, checksum: &mut u64) -> io::Result<[u8; N]> {
    let mut bytes = [0_u8; N];
    reader.read_exact(&mut bytes)?;
    *checksum = hash(*checksum, &bytes);
    Ok(bytes)
}

fn read_vertex(reader: &mut impl Read, checksum: &mut u64) -> io::Result<ChunkVertex> {
    let position = DVec3::new(
        f64::from_le_bytes(read_hashed(reader, checksum)?),
        f64::from_le_bytes(read_hashed(reader, checksum)?),
        f64::from_le_bytes(read_hashed(reader, checksum)?),
    );
    let normal = DVec3::new(
        f64::from_le_bytes(read_hashed(reader, checksum)?),
        f64::from_le_bytes(read_hashed(reader, checksum)?),
        f64::from_le_bytes(read_hashed(reader, checksum)?),
    );
    let colour = [
        f32::from_le_bytes(read_hashed(reader, checksum)?),
        f32::from_le_bytes(read_hashed(reader, checksum)?),
        f32::from_le_bytes(read_hashed(reader, checksum)?),
        f32::from_le_bytes(read_hashed(reader, checksum)?),
    ];
    let [protected] = read_hashed(reader, checksum)?;
    if protected > 1 {
        return Err(invalid("invalid protected flag"));
    }
    Ok(ChunkVertex {
        position,
        normal,
        colour,
        protected: protected == 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "planet-cache-test-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("temporary directory");
            Self(path)
        }

        fn cache(&self, key: u64) -> ChunkCache {
            ChunkCache::new(self.0.clone(), key)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn triangle() -> ChunkMesh {
        ChunkMesh {
            vertices: [DVec3::X, DVec3::Y, DVec3::Z]
                .into_iter()
                .enumerate()
                .map(|(i, position)| ChunkVertex {
                    position,
                    normal: position,
                    colour: [0.1, 0.2, 0.3, 1.0],
                    protected: i == 0,
                })
                .collect(),
            triangles: vec![[0, 1, 2]],
        }
    }

    #[test]
    fn cache_round_trips_all_attributes_and_atomically_replaces() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        let mut mesh = triangle();
        cache.write(17, 2, &mesh).expect("write mesh");
        let restored = cache.read(17, 2).expect("read mesh");
        assert_eq!(restored.triangles, mesh.triangles);
        for (a, b) in restored.vertices.iter().zip(&mesh.vertices) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.normal, b.normal);
            assert_eq!(a.colour.map(f32::to_bits), b.colour.map(f32::to_bits));
            assert_eq!(a.protected, b.protected);
        }
        mesh.vertices[0].position *= 2.0;
        cache.write(17, 2, &mesh).expect("replace mesh");
        assert_eq!(
            cache.read(17, 2).expect("read replacement").vertices[0].position,
            mesh.vertices[0].position
        );
        assert_eq!(
            fs::read_dir(&directory.0)
                .expect("directory entries")
                .count(),
            1
        );
    }

    #[test]
    fn cache_rejects_truncation_corruption_wrong_identity_and_trailing_bytes() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        cache.write(17, 2, &triangle()).expect("write mesh");
        let path = cache.path(17, 2);
        let bytes = fs::read(&path).expect("mesh bytes");
        for length in [0, 8, 43, bytes.len() - 1] {
            fs::write(&path, &bytes[..length]).expect("truncate cache");
            assert!(cache.read(17, 2).is_err());
        }
        for offset in [0, 8, 12, 20, 24, 28, 32, 36, 44, bytes.len() - 1] {
            let mut corrupted = bytes.clone();
            corrupted[offset] ^= 1;
            fs::write(&path, &corrupted).expect("corrupt cache");
            assert!(cache.read(17, 2).is_err(), "offset {offset}");
        }
        fs::write(&path, &bytes).expect("restore cache");
        let wrong_key = directory.cache(43);
        fs::copy(&path, wrong_key.path(17, 2)).expect("copy stale cache");
        assert!(wrong_key.read(17, 2).is_err());
        let mut appended = bytes;
        appended.push(0);
        fs::write(&path, appended).expect("append payload");
        assert!(cache.read(17, 2).is_err());
    }

    #[test]
    fn invalid_geometry_does_not_replace_a_valid_cache() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        cache.write(17, 2, &triangle()).expect("write mesh");
        let path = cache.path(17, 2);
        let original = fs::read(&path).expect("valid bytes");
        let mut invalid_mesh = triangle();
        invalid_mesh.vertices[0].position.x = f64::NAN;
        assert!(cache.write(17, 2, &invalid_mesh).is_err());
        invalid_mesh = triangle();
        invalid_mesh.vertices[0].normal = DVec3::ZERO;
        assert!(cache.write(17, 2, &invalid_mesh).is_err());
        invalid_mesh = triangle();
        invalid_mesh.triangles[0][2] = 50;
        assert!(cache.write(17, 2, &invalid_mesh).is_err());
        invalid_mesh.triangles[0] = [0, 0, 1];
        assert!(cache.write(17, 2, &invalid_mesh).is_err());
        assert_eq!(fs::read(path).expect("unchanged bytes"), original);
    }

    fn indexed_world() -> CachedWorld {
        CachedWorld {
            occluder_radius: 900.0,
            chunks: vec![CachedChunk {
                id: 17,
                layer: 0,
                bounds: ChunkBounds {
                    centre: DVec3::X * 1000.0,
                    radius: 50.0,
                    direction: DVec3::X,
                    half_angle: 0.1,
                    max_radius: 1050.0,
                },
                errors: vec![0.0, 0.5],
                triangles: vec![1, 1],
                vertex_counts: vec![3, 3],
                atlas_sizes: vec![[0, 0], [0, 0]],
            }],
        }
    }

    #[test]
    fn world_index_round_trips_only_after_referenced_meshes_exist() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        let world = indexed_world();
        assert!(cache.write_index(&world).is_err());
        assert!(!cache.index_path().exists());
        cache.write(17, 0, &triangle()).expect("first LOD");
        assert!(cache.write_index(&world).is_err());
        cache.write(17, 1, &triangle()).expect("second LOD");
        cache.write_index(&world).expect("publish index");
        let restored = cache.read_index().expect("read index");
        assert_eq!(
            restored.occluder_radius.to_bits(),
            world.occluder_radius.to_bits()
        );
        assert_eq!(restored.chunks.len(), 1);
        let chunk = &restored.chunks[0];
        assert_eq!(chunk.id, 17);
        assert_eq!(chunk.layer, 0);
        assert_eq!(chunk.bounds, world.chunks[0].bounds);
        assert_eq!(chunk.errors, world.chunks[0].errors);
        assert_eq!(chunk.triangles, world.chunks[0].triangles);
        assert_eq!(chunk.vertex_counts, world.chunks[0].vertex_counts);
        assert_eq!(chunk.atlas_sizes, world.chunks[0].atlas_sizes);
    }

    #[test]
    fn missing_fine_mesh_invalidates_publication_and_explicit_invalidation_preserves_meshes() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        cache.invalidate_index().expect("missing index is harmless");
        for lod in 0..2 {
            cache.write(17, lod, &triangle()).expect("LOD");
        }
        cache.write_index(&indexed_world()).expect("publish index");
        fs::remove_file(cache.path(17, 0)).expect("remove fine mesh");
        assert!(cache.read_index().is_err());
        cache.invalidate_index().expect("remove index");
        assert!(!cache.index_path().exists());
        assert!(
            cache.read(17, 1).is_ok(),
            "coarse fallback survives invalidation"
        );
        cache
            .invalidate_index()
            .expect("repeated invalidation is harmless");
    }

    #[test]
    fn world_index_rejects_corrupt_truncated_stale_or_invalid_metadata() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        for lod in 0..2 {
            cache.write(17, lod, &triangle()).expect("LOD");
        }
        let mut world = indexed_world();
        cache.write_index(&world).expect("publish index");
        let path = cache.index_path();
        let bytes = fs::read(&path).expect("index bytes");
        for offset in [0, 8, 12, 20, 24, 32, 40, bytes.len() - 1] {
            let mut corrupted = bytes.clone();
            corrupted[offset] ^= 1;
            fs::write(&path, corrupted).expect("corrupt index");
            assert!(cache.read_index().is_err(), "offset {offset}");
        }
        for length in [0, 39, bytes.len() - 1] {
            fs::write(&path, &bytes[..length]).expect("truncate index");
            assert!(cache.read_index().is_err());
        }
        fs::write(&path, &bytes).expect("restore index");
        let stale = directory.cache(43);
        fs::copy(&path, stale.index_path()).expect("copy stale index");
        assert!(stale.read_index().is_err());
        world.chunks[0].errors[1] = f64::NAN;
        assert!(cache.write_index(&world).is_err());
        world = indexed_world();
        world.chunks[0].bounds.direction = DVec3::ZERO;
        assert!(cache.write_index(&world).is_err());
        assert_eq!(fs::read(path).expect("unchanged index"), bytes);
    }

    fn atlas() -> SurfaceAtlasData {
        SurfaceAtlasData {
            uv: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            width: 2,
            height: 2,
            rivers: vec![0, 85, 170, 255],
            pixels: vec![
                128, 128, 255, 255, 128, 140, 253, 210, 140, 128, 253, 190, 110, 130, 252, 180,
            ],
        }
    }

    fn resign_surface(bytes: &mut [u8]) {
        let header_bytes = usize::try_from(HEADER_BYTES).expect("fixed header fits usize");
        let checksum = hash(
            hash(FNV_OFFSET, &bytes[..HEADER_PREFIX_BYTES]),
            &bytes[header_bytes..],
        );
        bytes[HEADER_PREFIX_BYTES..header_bytes].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn surface_round_trips_atlas_and_seam_uvs_and_mesh_only_remains_compatible() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        let mut mesh = triangle();
        mesh.vertices.push(mesh.vertices[0]);
        mesh.triangles.push([3, 2, 1]);
        let mut atlas = atlas();
        atlas.uv.push([1.0, 1.0]);
        cache
            .write_surface(17, 1, &mesh, Some(&atlas))
            .expect("write surface");
        let restored = cache.read_surface(17, 1).expect("read surface");
        assert_eq!(restored.atlas.as_ref(), Some(&atlas));
        assert_eq!(restored.mesh.triangles, mesh.triangles);
        assert_eq!(
            cache
                .read(17, 1)
                .expect("mesh compatibility")
                .vertices
                .len(),
            4
        );
        cache.write(17, 0, &mesh).expect("write finest mesh");
        assert!(
            cache
                .read_surface(17, 0)
                .expect("read finest mesh")
                .atlas
                .is_none()
        );
        cache.write(17, 1, &mesh).expect("replace mapped mesh");
        assert!(
            cache
                .read_surface(17, 1)
                .expect("read replacement")
                .atlas
                .is_none()
        );
    }

    #[test]
    fn invalid_atlas_does_not_replace_valid_surface() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        let mesh = triangle();
        cache
            .write_surface(17, 1, &mesh, Some(&atlas()))
            .expect("write surface");
        let original = fs::read(cache.path(17, 1)).expect("original surface");
        for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let mut invalid = atlas();
            invalid.uv[0][0] = value;
            assert!(cache.write_surface(17, 1, &mesh, Some(&invalid)).is_err());
        }
        let mut invalid = atlas();
        invalid.uv.pop();
        assert!(cache.write_surface(17, 1, &mesh, Some(&invalid)).is_err());
        invalid = atlas();
        invalid.pixels.pop();
        assert!(cache.write_surface(17, 1, &mesh, Some(&invalid)).is_err());
        invalid = atlas();
        invalid.rivers.pop();
        assert!(cache.write_surface(17, 1, &mesh, Some(&invalid)).is_err());
        invalid = atlas();
        invalid.width = MAX_ATLAS_DIMENSION + 1;
        assert!(cache.write_surface(17, 1, &mesh, Some(&invalid)).is_err());
        invalid.width = 0;
        assert!(cache.write_surface(17, 1, &mesh, Some(&invalid)).is_err());
        assert_eq!(
            fs::read(cache.path(17, 1)).expect("unchanged surface"),
            original
        );
    }

    #[test]
    fn atlas_reader_rejects_corruption_and_invalid_resigned_payloads() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        cache
            .write_surface(17, 1, &triangle(), Some(&atlas()))
            .expect("write surface");
        let path = cache.path(17, 1);
        let original = fs::read(&path).expect("surface bytes");
        let uv_offset = usize::try_from(HEADER_BYTES + 3 * VERTEX_BYTES + TRIANGLE_BYTES)
            .expect("fixture size fits usize");
        for offset in [36, 40, HEADER_PREFIX_BYTES, uv_offset, original.len() - 1] {
            let mut bytes = original.clone();
            bytes[offset] ^= 1;
            fs::write(&path, bytes).expect("corrupt surface");
            assert!(cache.read_surface(17, 1).is_err(), "offset {offset}");
        }
        for invalid_uv in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let mut bytes = original.clone();
            bytes[uv_offset..uv_offset + 4].copy_from_slice(&invalid_uv.to_le_bytes());
            resign_surface(&mut bytes);
            fs::write(&path, bytes).expect("write invalid UV");
            assert!(cache.read_surface(17, 1).is_err());
        }
        for [width, height] in [[MAX_ATLAS_DIMENSION + 1, 2], [0, 2], [8192, 8192]] {
            let mut bytes = original.clone();
            bytes[36..40].copy_from_slice(&width.to_le_bytes());
            bytes[40..44].copy_from_slice(&height.to_le_bytes());
            resign_surface(&mut bytes);
            fs::write(&path, bytes).expect("write invalid dimensions");
            assert!(cache.read_surface(17, 1).is_err());
        }
        fs::write(&path, &original[..original.len() - 1]).expect("truncate pixels");
        assert!(cache.read_surface(17, 1).is_err());
    }

    #[test]
    fn world_index_validates_per_lod_vertex_and_atlas_sizes() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        cache.write(17, 0, &triangle()).expect("finest LOD");
        cache
            .write_surface(17, 1, &triangle(), Some(&atlas()))
            .expect("mapped LOD");
        let mut world = indexed_world();
        world.chunks[0].atlas_sizes[1] = [2, 2];
        cache.write_index(&world).expect("publish mapped index");
        let restored = cache.read_index().expect("read mapped index");
        assert_eq!(restored.chunks[0].atlas_sizes, [[0, 0], [2, 2]]);
        world.chunks[0].vertex_counts.pop();
        assert!(cache.write_index(&world).is_err());
        world = indexed_world();
        world.chunks[0].vertex_counts[1] = u32::MAX as usize;
        assert!(cache.write_index(&world).is_err());
        world = indexed_world();
        world.chunks[0].atlas_sizes[1] = [8192, 8192];
        assert!(cache.write_index(&world).is_err());
        world.chunks[0].atlas_sizes.pop();
        assert!(cache.write_index(&world).is_err());
    }

    #[test]
    fn oversized_counts_are_rejected_before_allocation() {
        let directory = TestDirectory::new();
        let cache = directory.cache(42);
        let mut prefix = cache.header_prefix(17, 2, &triangle(), None);
        prefix[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        fs::write(cache.path(17, 2), bytes).expect("oversized header");
        assert!(cache.read(17, 2).is_err());
    }

    #[test]
    fn legitimate_payload_counts_are_not_limited_by_the_old_planet_vertex_budget() {
        assert!(validate_counts(2_000_001, 4_000_002, 0, 0).is_ok());
        assert!(validate_counts(u32::MAX, u32::MAX, 0, 0).is_err());
    }
}
