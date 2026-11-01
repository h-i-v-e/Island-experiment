//! Separate coastal cache: adding ocean maps does not invalidate terrain LODs.
use crate::chunk_cache::ChunkCache;
use planet_rs::{chunks::ChunkMesh, ocean_surface::CoastMap, surface_atlas::SurfaceSampler};
use std::{
    collections::BTreeMap,
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
    river: Option<&ChunkMesh>,
    radius: f64,
) -> Result<CoastMap, String> {
    prepare_size(cache, source, river, radius, SIZE)
}
fn prepare_size(
    cache: &ChunkCache,
    source: &ChunkMesh,
    river: Option<&ChunkMesh>,
    radius: f64,
    size: u32,
) -> Result<CoastMap, String> {
    let path = cache.path(u32::MAX, 0).with_extension("ocean-v2");
    let length = size as usize * size as usize * 24;
    let read = || -> Option<CoastMap> {
        let mut file = fs::File::open(&path).ok()?;
        if file.metadata().ok()?.len() != u64::try_from(length + 24).ok()? {
            return None;
        }
        let mut header = [0; 24];
        file.read_exact(&mut header).ok()?;
        if &header[..8] != b"PLNOCE02" || u32::from_le_bytes(header[8..12].try_into().ok()?) != size
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
    let mut map = CoastMap::bake(&sampler, radius, size)?;
    if let Some(river) = river {
        map.flatten_sea_contacts(&sea_contact_edges(river));
    }
    let write = || -> std::io::Result<()> {
        fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension(format!(
            "ocean-tmp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = fs::File::create(&temporary)?;
        file.write_all(b"PLNOCE02")?;
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

/// Only used triangle edges count: clipping leaves unused submerged vertices.
fn sea_contact_edges(river: &ChunkMesh) -> Vec<[planet_rs::DVec3; 2]> {
    let mut edges = BTreeMap::<_, usize>::new();
    for &[a, b, c] in &river.triangles {
        for (a, b) in [(a, b), (b, c), (c, a)] {
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    edges
        .into_iter()
        .filter_map(|((a, b), count)| {
            let edge = [
                river.vertices[a as usize].position,
                river.vertices[b as usize].position,
            ];
            (count == 1
                && edge.iter().all(|p| p.length() <= 1.0 + 1.0e-9)
                && edge[0].distance_squared(edge[1]) > 1.0e-18)
                .then_some(edge)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use planet_rs::{DVec3, chunks::ChunkVertex};
    #[test]
    fn contacts_use_only_open_sea_edges_not_internal_edges_or_orphan_vertices() {
        let p = |x: f64, height: f64| DVec3::new(x, 1.0, 0.0).normalize() * (1.0 + height);
        let river = ChunkMesh {
            vertices: [
                p(-0.02, 0.0),
                p(0.0, 0.0),
                p(0.02, 0.0),
                p(0.0, 0.01),
                -DVec3::Y,
            ]
            .map(|position| ChunkVertex {
                position,
                normal: position.normalize(),
                colour: [0.0; 4],
                protected: true,
            })
            .into(),
            triangles: vec![[0, 1, 3], [1, 2, 3]],
        };
        let contacts = sea_contact_edges(&river);
        assert_eq!(
            contacts,
            vec![
                [river.vertices[0].position, river.vertices[1].position],
                [river.vertices[1].position, river.vertices[2].position]
            ]
        );
    }
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
        let river = ChunkMesh {
            vertices: [
                DVec3::new(-0.1, 1.0, 0.0).normalize(),
                DVec3::new(0.1, 1.0, 0.0).normalize(),
                DVec3::new(0.0, 1.01, 0.01),
            ]
            .map(|position| ChunkVertex {
                position,
                normal: position.normalize(),
                colour: [0.0; 4],
                protected: true,
            })
            .into(),
            triangles: vec![[0, 1, 2]],
        };
        let original = prepare_size(&cache, &source, Some(&river), 1000.0, 32).unwrap();
        let (face, x, y) = planet_rs::ocean_surface::cube_pixel(DVec3::Y, 32);
        assert_eq!(
            original.pixels[((face * 32 + y as usize) * 32 + x as usize) * 4 + 2],
            255,
            "unexcavated seabed at an actual river outlet must also suppress waves"
        );
        let empty = ChunkMesh {
            vertices: Vec::new(),
            triangles: Vec::new(),
        };
        assert_eq!(
            prepare_size(&cache, &empty, None, 1000.0, 32)
                .unwrap()
                .pixels,
            original.pixels,
            "valid cache does not resample terrain"
        );
        let path = cache.path(u32::MAX, 0).with_extension("ocean-v2");
        let mut corrupt = fs::read(&path).unwrap();
        corrupt[32] ^= 1;
        fs::write(&path, corrupt).unwrap();
        assert!(prepare_size(&cache, &empty, None, 1000.0, 32).is_err());
        assert_eq!(
            prepare_size(&cache, &source, Some(&river), 1000.0, 32)
                .unwrap()
                .pixels,
            original.pixels
        );
        assert!(prepare_size(&cache, &empty, None, 1000.0, 64).is_err());
        assert_eq!(
            prepare_size(&cache, &source, None, 1000.0, 64)
                .unwrap()
                .size,
            64
        );
        fs::write(&path, b"truncated").unwrap();
        assert!(prepare_size(&cache, &empty, None, 1000.0, 64).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
