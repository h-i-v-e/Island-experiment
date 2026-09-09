use super::{
    ExportMesh, GenerationMethod, ISLAND_WORLD_METRES, Island, MotuFernOptions, MotuForestOptions,
    MotuOptions, MotuReedOptions, Vec2, Vec3, c_void, export_mesh, island_ref, ptr,
};
use crate::caves::{CAVE_REVISION, CaveOptions, CaveStats};

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct CaveInfo {
    pub id: u64,
    pub entrance: Vec3,
    pub inward: Vec2,
    pub chamber: Vec3,
    pub minimum: Vec2,
    pub maximum: Vec2,
    pub chunk_count: u32,
}

#[unsafe(no_mangle)]
pub extern "C" fn CaveAlgorithmRevision() -> u32 {
    CAVE_REVISION
}

/// Additive generation ABI: existing callers continue to generate without caves.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateMotuWithCaves(
    seed: i32,
    options: *const MotuOptions,
    forest: *const MotuForestOptions,
    reeds: *const MotuReedOptions,
    ferns: *const MotuFernOptions,
    caves: *const CaveOptions,
) -> *mut c_void {
    // SAFETY: optional blocks must be readable for their documented C sizes.
    let (options, forest, reeds, ferns, caves) = unsafe {
        (
            options
                .as_ref()
                .copied()
                .map(Into::into)
                .unwrap_or_default(),
            forest.as_ref().copied().map(Into::into).unwrap_or_default(),
            reeds.as_ref().copied().map(Into::into).unwrap_or_default(),
            ferns.as_ref().copied().map(Into::into).unwrap_or_default(),
            caves.as_ref().copied().unwrap_or_default(),
        )
    };
    Island::generate_with_caves(
        u64::from(seed.cast_unsigned()),
        options,
        forest,
        reeds,
        ferns,
        caves,
        GenerationMethod::Cpu,
    )
    .map_or_else(
        |error| {
            eprintln!("CreateMotuWithCaves: {error}");
            ptr::null_mut()
        },
        |island| Box::into_raw(Box::new(island)).cast(),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GetCaveCount(handle: *const c_void) -> u32 {
    // SAFETY: caller supplies a live, exclusively accessed island handle.
    unsafe { island_ref(handle) }.map_or(0, |i| u32::try_from(i.caves().caves.len()).unwrap_or(0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GetCaveStats(handle: *const c_void, output: *mut CaveStats) -> u8 {
    // SAFETY: output points to a writable CaveStats, handle is a live island.
    let (Some(island), Some(output)) = (unsafe { island_ref(handle) }, unsafe { output.as_mut() })
    else {
        return 0;
    };
    *output = island.caves().stats;
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GetCaveInfo(
    handle: *const c_void,
    index: u32,
    output: *mut CaveInfo,
) -> u8 {
    // SAFETY: output points to writable CaveInfo, handle is a live island.
    let (Some(island), Some(output)) = (unsafe { island_ref(handle) }, unsafe { output.as_mut() })
    else {
        return 0;
    };
    *output = CaveInfo::default();
    let Some(cave) = island.caves().caves.get(index as usize) else {
        return 0;
    };
    *output = CaveInfo {
        id: cave.id,
        entrance: cave.entrance,
        inward: cave.inward,
        chamber: cave.nodes.last().map_or(cave.entrance, |n| n.floor),
        minimum: cave.minimum,
        maximum: cave.maximum,
        chunk_count: u32::try_from(cave.chunks.len() + 1).unwrap_or(0),
    };
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateCaveMesh(
    handle: *const c_void,
    index: u32,
    chunk: u32,
    output: *mut ExportMesh,
) -> u8 {
    // SAFETY: output points to an unused ExportMesh. Returned ownership must be
    // passed once to ReleaseMesh, like every other generated mesh export.
    let Some(output) = (unsafe { output.as_mut() }) else {
        return 0;
    };
    *output = ExportMesh::default();
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return 0;
    };
    let Some(cave) = island.caves().caves.get(index as usize) else {
        return 0;
    };
    let Some(mesh) = cave
        .chunks
        .get(chunk as usize)
        .or_else(|| (chunk as usize == cave.chunks.len()).then_some(&cave.entrance_collider))
    else {
        return 0;
    };
    let cave_attributes = mesh
        .vertices
        .iter()
        .map(|p| {
            if chunk as usize == cave.chunks.len() {
                Vec2::new(1.0, 2.0)
            } else {
                cave.surface_attributes(&island.caves().options, *p * ISLAND_WORLD_METRES)
            }
        })
        .collect();
    // The export owns this copy independently of the resident island snapshot.
    let material = island.material_values_for(mesh);
    let environment = island.environment_values_for(mesh);
    let mut owned = mesh.clone();
    // Cave exports use UV0 for ambient/exterior metadata. The Unity transfer
    // restores terrain UVs from positions and stores this pair in UV2.
    owned.uv = cave_attributes;
    *output = export_mesh(owned, material, environment);
    1
}
