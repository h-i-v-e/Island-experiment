use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

use bincode::Options;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    Decorations, GenerationMethod, Island, IslandOptions, Mesh, Terrain, TerrainEnvironmentField,
    TerrainMaterialField,
};
use crate::{
    ferns::FernMeshes,
    forest::{ForestGenerationStats, ForestMeshes, ForestOptions},
    reeds::ReedMeshes,
    rivers::{River, WaterfallFoot},
};

const SNAPSHOT_MAGIC: [u8; 8] = *b"MOTUSNP\0";
const SNAPSHOT_VERSION: u16 = 20;
const COMPRESSION_ZSTD: u8 = 1;
const HEADER_LENGTH: u64 = 8 + 2 + 1 + 1 + 8 + 32;
const MAXIMUM_COMPRESSED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAXIMUM_DECOMPRESSED_BYTES: u64 = 32 * 1024 * 1024 * 1024;
const ZSTD_COMPRESSION_LEVEL: i32 = 3;

#[derive(Serialize)]
struct IslandSnapshotRef<'a> {
    caves: &'a crate::caves::CaveSet,
    seed: u64,
    options: IslandOptions,
    generation_method: GenerationMethod,
    terrain: &'a Terrain,
    material: &'a TerrainMaterialField,
    environment: &'a TerrainEnvironmentField,
    coarser_lods: &'a [Mesh; 2],
    rivers: &'a [River],
    distance_to_land: &'a [f32],
    submerged_river_carve: &'a [f32],
    river_mesh: &'a Mesh,
    river_rock_mesh: &'a Mesh,
    waterfall_feet: &'a [WaterfallFoot],
    reeds: &'a ReedMeshes,
    ferns: &'a FernMeshes,
    forest: &'a ForestMeshes,
    forest_stats: &'a ForestGenerationStats,
    forest_options: ForestOptions,
    decorations: &'a Decorations,
}

#[derive(Deserialize)]
struct IslandSnapshotOwned {
    caves: crate::caves::CaveSet,
    seed: u64,
    options: IslandOptions,
    generation_method: GenerationMethod,
    terrain: Terrain,
    material: TerrainMaterialField,
    environment: TerrainEnvironmentField,
    coarser_lods: [Mesh; 2],
    rivers: Vec<River>,
    distance_to_land: Vec<f32>,
    submerged_river_carve: Vec<f32>,
    river_mesh: Mesh,
    river_rock_mesh: Mesh,
    waterfall_feet: Vec<WaterfallFoot>,
    reeds: ReedMeshes,
    ferns: FernMeshes,
    forest: ForestMeshes,
    forest_stats: ForestGenerationStats,
    forest_options: ForestOptions,
    decorations: Decorations,
}

pub(super) fn is_snapshot(path: &Path) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut magic = [0_u8; SNAPSHOT_MAGIC.len()];
    file.read_exact(&mut magic)?;
    Ok(magic == SNAPSHOT_MAGIC)
}

pub(super) fn save(island: &Island, path: &Path) -> io::Result<()> {
    validate_island(island)?;
    let temporary_path = temporary_path(path);
    let result = save_to_temporary_file(island, &temporary_path)
        .and_then(|()| replace_file(&temporary_path, path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn save_to_temporary_file(island: &Island, path: &Path) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&[0_u8; HEADER_LENGTH as usize])?;

    let snapshot = IslandSnapshotRef {
        caves: &island.caves,
        seed: island.seed,
        options: island.options,
        generation_method: island.generation_method,
        terrain: &island.terrain,
        material: &island.material,
        environment: &island.environment,
        coarser_lods: &island.coarser_lods,
        rivers: &island.rivers,
        distance_to_land: &island.distance_to_land,
        submerged_river_carve: &island.submerged_river_carve,
        river_mesh: &island.river_mesh,
        river_rock_mesh: &island.river_rock_mesh,
        waterfall_feet: &island.waterfall_feet,
        reeds: &island.reeds,
        ferns: &island.ferns,
        forest: &island.forest,
        forest_stats: &island.forest_stats,
        forest_options: island.forest_options,
        decorations: island.decorations(),
    };
    {
        let mut encoder = zstd::stream::write::Encoder::new(file, ZSTD_COMPRESSION_LEVEL)?;
        bincode_options()
            .serialize_into(&mut encoder, &snapshot)
            .map_err(invalid_data)?;
        file = encoder.finish()?;
    }
    file.flush()?;

    let file_length = file.metadata()?.len();
    let payload_length = file_length
        .checked_sub(HEADER_LENGTH)
        .ok_or_else(|| invalid_data("snapshot payload length underflow"))?;
    if payload_length > MAXIMUM_COMPRESSED_BYTES {
        return Err(invalid_data(
            "compressed snapshot exceeds the supported size",
        ));
    }
    let checksum = checksum_payload(&mut file, payload_length)?;
    file.seek(SeekFrom::Start(0))?;
    write_header(&mut file, payload_length, checksum)?;
    file.flush()?;
    file.sync_all()
}

pub(super) fn load(path: &Path) -> io::Result<Island> {
    let mut file = File::open(path)?;
    let payload_length = read_header(&mut file)?;
    let actual_length = file.metadata()?.len();
    if actual_length != HEADER_LENGTH + payload_length {
        return Err(invalid_data("snapshot length does not match its header"));
    }

    let expected_checksum = read_checksum(&mut file)?;
    let actual_checksum = checksum_payload(&mut file, payload_length)?;
    if expected_checksum != actual_checksum {
        return Err(invalid_data("snapshot checksum mismatch"));
    }

    file.seek(SeekFrom::Start(HEADER_LENGTH))?;
    let compressed = file.take(payload_length);
    let decoder = zstd::stream::read::Decoder::new(compressed)?;
    let snapshot: IslandSnapshotOwned = bincode_options()
        .with_limit(MAXIMUM_DECOMPRESSED_BYTES)
        .deserialize_from(decoder)
        .map_err(invalid_data)?;
    snapshot.into_island()
}

impl IslandSnapshotOwned {
    fn into_island(self) -> io::Result<Island> {
        validate_mesh(&self.terrain.mesh, "terrain")?;
        for (index, mesh) in self.coarser_lods.iter().enumerate() {
            validate_mesh(mesh, &format!("terrain LOD {}", index + 1))?;
        }
        validate_mesh(&self.river_mesh, "river")?;
        validate_mesh(&self.river_rock_mesh, "river rocks")?;
        let vertex_count = self.terrain.mesh.vertices.len();
        if self.material.values.len() != vertex_count
            || self.environment.values.len() != vertex_count
            || self.distance_to_land.len() != vertex_count
            || self.submerged_river_carve.len() != vertex_count
        {
            return Err(invalid_data(
                "snapshot terrain sidecar lengths do not match its vertex count",
            ));
        }
        if self
            .submerged_river_carve
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(invalid_data(
                "snapshot submerged-river-carve coverage contains invalid data",
            ));
        }
        self.options.validate().map_err(invalid_data)?;
        self.forest_options.validate().map_err(invalid_data)?;

        self.caves.validate().map_err(invalid_data)?;
        Ok(Island {
            caves: self.caves,
            seed: self.seed,
            options: self.options,
            generation_method: self.generation_method,
            terrain: self.terrain,
            material: self.material,
            environment: self.environment,
            coarser_lods: self.coarser_lods,
            rivers: self.rivers,
            distance_to_land: self.distance_to_land,
            submerged_river_carve: self.submerged_river_carve,
            river_mesh: self.river_mesh,
            river_rock_mesh: self.river_rock_mesh,
            waterfall_feet: self.waterfall_feet,
            reeds: self.reeds,
            ferns: self.ferns,
            forest: self.forest,
            forest_stats: self.forest_stats,
            forest_options: self.forest_options,
            decorations: OnceLock::from(self.decorations),
            #[cfg(feature = "surface-atlas")]
            surface_atlases: super::surface_atlas::SurfaceAtlasCache::default(),
        })
    }
}

fn validate_island(island: &Island) -> io::Result<()> {
    island.caves.validate().map_err(invalid_data)?;
    validate_mesh(&island.terrain.mesh, "terrain")?;
    for (index, mesh) in island.coarser_lods.iter().enumerate() {
        validate_mesh(mesh, &format!("terrain LOD {}", index + 1))?;
    }
    validate_mesh(&island.river_mesh, "river")?;
    validate_mesh(&island.river_rock_mesh, "river rocks")?;
    Ok(())
}

fn validate_mesh(mesh: &Mesh, label: &str) -> io::Result<()> {
    let vertex_count = mesh.vertices.len();
    if mesh.normals.len() != vertex_count || (!mesh.uv.is_empty() && mesh.uv.len() != vertex_count)
    {
        return Err(invalid_data(format!(
            "{label} mesh attribute lengths do not match its vertex count"
        )));
    }
    if !mesh.triangles.len().is_multiple_of(3)
        || mesh
            .triangles
            .iter()
            .any(|&vertex| vertex as usize >= vertex_count)
    {
        return Err(invalid_data(format!("{label} mesh indices are invalid")));
    }
    if mesh.vertices.iter().any(|value| !value.is_finite())
        || mesh.normals.iter().any(|value| !value.is_finite())
        || mesh.uv.iter().any(|value| !value.is_finite())
    {
        return Err(invalid_data(format!(
            "{label} mesh contains non-finite data"
        )));
    }
    Ok(())
}

fn read_header(file: &mut File) -> io::Result<u64> {
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic)?;
    if magic != SNAPSHOT_MAGIC {
        return Err(invalid_data("not a Motu generated-island snapshot"));
    }
    let version = read_u16(file)?;
    if version != SNAPSHOT_VERSION {
        return Err(invalid_data(format!(
            "unsupported generated-island snapshot version {version}"
        )));
    }
    let mut format = [0_u8; 2];
    file.read_exact(&mut format)?;
    if format != [COMPRESSION_ZSTD, 0] {
        return Err(invalid_data("unsupported snapshot compression format"));
    }
    let payload_length = read_u64(file)?;
    if payload_length > MAXIMUM_COMPRESSED_BYTES {
        return Err(invalid_data(
            "compressed snapshot exceeds the supported size",
        ));
    }
    Ok(payload_length)
}

fn read_checksum(file: &mut File) -> io::Result<[u8; 32]> {
    file.seek(SeekFrom::Start(8 + 2 + 1 + 1 + 8))?;
    let mut checksum = [0_u8; 32];
    file.read_exact(&mut checksum)?;
    Ok(checksum)
}

fn write_header(file: &mut File, payload_length: u64, checksum: [u8; 32]) -> io::Result<()> {
    file.write_all(&SNAPSHOT_MAGIC)?;
    file.write_all(&SNAPSHOT_VERSION.to_le_bytes())?;
    file.write_all(&[COMPRESSION_ZSTD, 0])?;
    file.write_all(&payload_length.to_le_bytes())?;
    file.write_all(&checksum)
}

fn checksum_payload(file: &mut File, payload_length: u64) -> io::Result<[u8; 32]> {
    file.seek(SeekFrom::Start(HEADER_LENGTH))?;
    let mut source = file.take(payload_length);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().into())
}

fn bincode_options() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
}

fn read_u16(source: &mut impl Read) -> io::Result<u16> {
    let mut bytes = [0_u8; 2];
    source.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u64(source: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0_u8; 8];
    source.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn replace_file(temporary_path: &Path, destination: &Path) -> io::Result<()> {
    match fs::rename(temporary_path, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(destination)?;
            fs::rename(temporary_path, destination)
        }
        Err(error) => Err(error),
    }
}

fn temporary_path(destination: &Path) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let mut name = destination.as_os_str().to_owned();
    name.push(format!(".tmp-{}-{unique}", std::process::id()));
    PathBuf::from(name)
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_forest_snapshot_roundtrip_retains_catalog_placements_and_collision_proxies() {
        for latitude_degrees in [0.0, 70.0] {
            let forest_options = ForestOptions {
                noise_threshold: 0.0,
                prototype_count: 2,
                hybrid: Some(crate::forest::HybridForestOptions {
                    latitude_degrees,
                    altitude_cooling: 0.0,
                    variants_per_species: 2,
                }),
                ..ForestOptions::default()
            };
            let island = Island::shared_forest_fixture(forest_options);
            assert!(
                !island.forest.placements.is_empty(),
                "fixture must contain live trees"
            );
            let path = temporary_path(&std::env::temp_dir().join("motu-shared-forest-snapshot"));
            save(&island, &path).unwrap();
            let restored = load(&path).unwrap();
            if latitude_degrees == 0.0 {
                assert_unsupported_catalog_is_rejected(&path, &island.forest);
            }
            fs::remove_file(&path).unwrap();
            assert_eq!(restored.forest_options, forest_options);
            assert_eq!(restored.forest, island.forest);
            assert_eq!(restored.forest_stats, island.forest_stats);
            assert_eq!(restored.reeds, island.reeds);
            assert_eq!(restored.ferns, island.ferns);
            let catalog = island.forest.hybrid.as_ref().unwrap();
            let restored_catalog = restored.forest.hybrid.as_ref().unwrap();
            assert!(std::sync::Arc::ptr_eq(
                &catalog.prototypes,
                &restored_catalog.prototypes
            ));
            assert_eq!(restored_catalog.prototypes.len(), 6);
            assert_eq!(restored.forest.placements, island.forest.placements);
            assert_eq!(
                restored.forest_shadow_mask(256),
                island.forest_shadow_mask(256),
                "the shared bake must derive identically from an existing snapshot"
            );
            if latitude_degrees > 60.0 {
                assert!(
                    restored
                        .forest
                        .placements
                        .iter()
                        .all(|tree| (2..4).contains(&tree.prototype))
                );
            } else {
                assert!(
                    restored
                        .forest
                        .placements
                        .iter()
                        .any(|tree| tree.prototype < 2)
                );
                assert!(
                    restored
                        .forest
                        .placements
                        .iter()
                        .any(|tree| tree.prototype >= 4)
                );
            }
            let colliders = island
                .forest
                .trunk_colliders()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let restored_colliders = restored
                .forest
                .trunk_colliders()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!(!colliders.is_empty());
            assert_eq!(restored_colliders, colliders);
            assert!(restored.forest.lod0_foliage.vertices.is_empty());
            assert_eq!(
                restored.forest.lod1_foliage.vertices.len(),
                colliders.len() * 5
            );
            println!(
                "shared forest snapshot: latitude {latitude_degrees}, {} placements, {} colliders, six shared prototypes",
                restored.forest.placements.len(),
                colliders.len()
            );
        }
    }

    impl Island {
        pub(crate) fn shared_forest_fixture(forest_options: ForestOptions) -> Self {
            let vertices = (0_u16..6)
                .flat_map(|y| {
                    (0_u16..6).map(move |x| {
                        crate::Vec3::new(0.3 + f32::from(x) * 0.02, 0.3 + f32::from(y) * 0.02, 0.02)
                    })
                })
                .collect::<Vec<_>>();
            let triangles = (0_u32..5)
                .flat_map(|y| {
                    (0_u32..5).flat_map(move |x| {
                        let row = y * 6 + x;
                        [row, row + 1, row + 6, row + 1, row + 7, row + 6]
                    })
                })
                .collect();
            let vertex_count = vertices.len();
            let terrain = Terrain::new(Mesh {
                normals: vec![crate::Vec3::Z; vertex_count],
                uv: vertices.iter().map(|vertex| vertex.truncate()).collect(),
                vertices,
                triangles,
            });
            let (forest, forest_stats) = crate::forest::generate_forest(
                2018,
                &terrain,
                crate::forest::ForestSurface {
                    river_bed: &vec![false; vertex_count],
                    stones: &[],
                    reeds: &[],
                    deposited_depths: &vec![0.001; vertex_count],
                    sea_proximity: &vec![0.0; vertex_count],
                },
                forest_options,
            )
            .unwrap();
            Self {
                caves: crate::caves::CaveSet::default(),
                seed: 2018,
                options: IslandOptions {
                    terrain_size: 16,
                    ..IslandOptions::default()
                },
                generation_method: GenerationMethod::Cpu,
                material: TerrainMaterialField {
                    values: vec![crate::Vec4::ZERO; vertex_count],
                },
                environment: TerrainEnvironmentField {
                    values: vec![crate::Vec2::ZERO; vertex_count],
                },
                coarser_lods: [terrain.mesh().clone(), terrain.mesh().clone()],
                terrain,
                rivers: Vec::new(),
                distance_to_land: vec![0.0; vertex_count],
                submerged_river_carve: vec![0.0; vertex_count],
                river_mesh: Mesh::default(),
                river_rock_mesh: Mesh::default(),
                waterfall_feet: Vec::new(),
                reeds: ReedMeshes::default(),
                ferns: FernMeshes::default(),
                forest,
                forest_stats,
                forest_options,
                decorations: OnceLock::from(Decorations::default()),
                #[cfg(feature = "surface-atlas")]
                surface_atlases: super::super::surface_atlas::SurfaceAtlasCache::default(),
            }
        }
    }

    fn assert_unsupported_catalog_is_rejected(path: &Path, forest: &ForestMeshes) {
        let snapshot = fs::read(path).unwrap();
        let mut payload = zstd::stream::decode_all(&snapshot[HEADER_LENGTH as usize..]).unwrap();
        let forest_bytes = bincode_options().serialize(forest).unwrap();
        let forest_start = payload
            .windows(forest_bytes.len())
            .position(|bytes| bytes == forest_bytes)
            .unwrap();
        // Hybrid metadata is the final forest field: version u32, variant count u8.
        let catalog_start = forest_start + forest_bytes.len() - 5;
        let catalog_version = crate::forest::hybrid::CATALOG_VERSION;
        assert_eq!(
            &payload[catalog_start..catalog_start + 4],
            catalog_version.to_le_bytes()
        );
        payload[catalog_start..catalog_start + 4]
            .copy_from_slice(&(catalog_version + 1).to_le_bytes());
        let compressed =
            zstd::stream::encode_all(payload.as_slice(), ZSTD_COMPRESSION_LEVEL).unwrap();
        let checksum: [u8; 32] = Sha256::digest(&compressed).into();
        let mut file = File::create(path).unwrap();
        write_header(
            &mut file,
            u64::try_from(compressed.len()).unwrap(),
            checksum,
        )
        .unwrap();
        file.write_all(&compressed).unwrap();
        drop(file);
        let error = load(path).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            error
                .to_string()
                .contains("unsupported forest catalog version")
        );
    }

    #[test]
    fn header_rejects_an_unknown_version() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SNAPSHOT_MAGIC);
        bytes.extend_from_slice(&(SNAPSHOT_VERSION - 1).to_le_bytes());
        bytes.extend_from_slice(&[COMPRESSION_ZSTD, 0]);
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        bytes.extend_from_slice(&[0_u8; 32]);

        let path = temporary_path(Path::new("motu-version-test"));
        fs::write(&path, bytes).expect("write invalid snapshot fixture");
        let error = load(&path).expect_err("unknown version must fail");
        fs::remove_file(path).expect("remove invalid snapshot fixture");
        assert!(error.to_string().contains("unsupported"));
    }
}
