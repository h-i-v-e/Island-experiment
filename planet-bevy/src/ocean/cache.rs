//! Separate coastal cache: adding ocean maps does not invalidate terrain LODs.
use crate::chunk_cache::ChunkCache;
use planet_rs::{chunks::ChunkMesh, ocean_surface::CoastMap, surface_atlas::SurfaceSampler};
use std::{
    fs,
    io::{Read, Write},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const SIZE: u32 = 1024;
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}
pub(crate) fn prepare(
    cache: &ChunkCache,
    source: &ChunkMesh,
    radius: f64,
) -> Result<CoastMap, String> {
    prepare_size(cache, source, radius, SIZE)
}
fn prepare_size(
    cache: &ChunkCache,
    source: &ChunkMesh,
    radius: f64,
    size: u32,
) -> Result<CoastMap, String> {
    let path = cache.path(u32::MAX, 0).with_extension("ocean-v1");
    let length = size as usize * size as usize * 24;
    let read = || -> Option<CoastMap> {
        let mut file = fs::File::open(&path).ok()?;
        if file.metadata().ok()?.len() != u64::try_from(length + 24).ok()? {
            return None;
        }
        let mut header = [0; 24];
        file.read_exact(&mut header).ok()?;
        if &header[..8] != b"PLNOCE01" || u32::from_le_bytes(header[8..12].try_into().ok()?) != size
        {
            return None;
        }
        let mut pixels = vec![0; length];
        file.read_exact(&mut pixels).ok()?;
        if hash(&pixels) != u64::from_le_bytes(header[16..24].try_into().ok()?) {
            return None;
        }
        Some(CoastMap { size, pixels })
    };
    if let Some(map) = read() {
        return Ok(map);
    }
    let sampler = SurfaceSampler::new(source)?;
    let map = CoastMap::bake(&sampler, radius, size)?;
    let write = || -> std::io::Result<()> {
        fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension(format!(
            "ocean-tmp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = fs::File::create(&temporary)?;
        file.write_all(b"PLNOCE01")?;
        file.write_all(&size.to_le_bytes())?;
        file.write_all(&[0; 4])?;
        file.write_all(&hash(&map.pixels).to_le_bytes())?;
        file.write_all(&map.pixels)?;
        file.sync_all()?;
        fs::rename(temporary, &path)
    };
    if let Err(error) = write() {
        bevy::log::warn!("Ocean map cache: {error}");
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use planet_rs::{DVec3, chunks::ChunkVertex};
    #[test]
    fn cache_roundtrip_corruption_repair_and_size_upgrade() {
        let directory = std::env::temp_dir().join(format!(
            "planet-ocean-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let cache = ChunkCache::new(directory.clone(), 123);
        let source = ChunkMesh {
            vertices: [
                DVec3::X,
                -DVec3::X,
                DVec3::Y,
                -DVec3::Y,
                DVec3::Z,
                -DVec3::Z,
            ]
            .map(|p| ChunkVertex {
                position: p * 0.9,
                normal: p,
                colour: [0.0; 4],
                protected: false,
            })
            .into(),
            triangles: vec![
                [0, 2, 4],
                [4, 2, 1],
                [1, 2, 5],
                [5, 2, 0],
                [4, 3, 0],
                [1, 3, 4],
                [5, 3, 1],
                [0, 3, 5],
            ],
        };
        let original = prepare_size(&cache, &source, 1000.0, 32).unwrap();
        let empty = ChunkMesh {
            vertices: Vec::new(),
            triangles: Vec::new(),
        };
        assert_eq!(
            prepare_size(&cache, &empty, 1000.0, 32).unwrap().pixels,
            original.pixels,
            "valid cache does not resample terrain"
        );
        let path = cache.path(u32::MAX, 0).with_extension("ocean-v1");
        let mut corrupt = fs::read(&path).unwrap();
        corrupt[32] ^= 1;
        fs::write(&path, corrupt).unwrap();
        assert!(prepare_size(&cache, &empty, 1000.0, 32).is_err());
        assert_eq!(
            prepare_size(&cache, &source, 1000.0, 32).unwrap().pixels,
            original.pixels
        );
        assert!(prepare_size(&cache, &empty, 1000.0, 64).is_err());
        assert_eq!(prepare_size(&cache, &source, 1000.0, 64).unwrap().size, 64);
        fs::write(&path, b"truncated").unwrap();
        assert!(prepare_size(&cache, &empty, 1000.0, 64).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
