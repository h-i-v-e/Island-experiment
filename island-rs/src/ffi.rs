#![allow(
    non_snake_case,
    clippy::cast_possible_wrap,
    clippy::cast_ptr_alignment,
    clippy::cast_sign_loss,
    clippy::missing_safety_doc
)]

use std::{
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
    ffi::{CStr, c_char, c_void},
    mem::{align_of, size_of},
    path::PathBuf,
    ptr,
};

use crate::forest::ForestMeshKind;
use crate::{
    BoundingBox, ForestOptions, Island, IslandOptions, Mesh, SeaMask, SurfaceMaps, Vec2, Vec3,
    generate_tree,
};

const _: () = {
    assert!(size_of::<Vec2>() == size_of::<[f32; 2]>());
    assert!(align_of::<Vec2>() == align_of::<f32>());
    assert!(size_of::<Vec3>() == size_of::<[f32; 3]>());
    assert!(align_of::<Vec3>() == align_of::<f32>());
};

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MotuOptions {
    pub maxZ: f32,
    pub waterRatio: f32,
    pub slopeMultiplier: f32,
    pub coastalSlopeMultiplier: f32,
    pub removedCoastalErosionStrength: f32,
    pub removedBeachFormationStrength: f32,
    pub hydraulicErosionStrength: f32,
    pub hydraulicDepositionStrength: f32,
    pub hydraulicDepositionSlopeDegrees: f32,
    pub riverSourceCatchmentHectares: f32,
    pub riverSourceSteepMultiplier: f32,
    pub riverSourceElevationBoost: f32,
    pub riverSourceWidthMetres: f32,
    pub riverMaximumWidthMetres: f32,
    pub riverSourceDepthMetres: f32,
    pub riverMaximumDepthMetres: f32,
}

const _: () = assert!(size_of::<MotuOptions>() == size_of::<[f32; 16]>());

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MotuForestOptions {
    pub patchSizeMetres: f32,
    pub noiseThreshold: f32,
    pub noiseOctaves: u8,
    pub snowlineMetres: f32,
    pub prototypeCount: u8,
    pub minimumScale: f32,
    pub maximumScale: f32,
}

const _: () = {
    assert!(size_of::<MotuForestOptions>() == size_of::<[u8; 28]>());
    assert!(align_of::<MotuForestOptions>() == align_of::<f32>());
};

impl From<MotuOptions> for IslandOptions {
    fn from(value: MotuOptions) -> Self {
        Self {
            max_height: value.maxZ,
            water_ratio: value.waterRatio,
            slope_multiplier: value.slopeMultiplier,
            coastal_slope_multiplier: value.coastalSlopeMultiplier,
            hydraulic_erosion_strength: value.hydraulicErosionStrength,
            hydraulic_deposition_strength: value.hydraulicDepositionStrength,
            hydraulic_deposition_slope_degrees: value.hydraulicDepositionSlopeDegrees,
            river_source_catchment_hectares: value.riverSourceCatchmentHectares,
            river_source_steep_multiplier: value.riverSourceSteepMultiplier,
            river_source_elevation_boost: value.riverSourceElevationBoost,
            river_source_width_metres: value.riverSourceWidthMetres,
            river_maximum_width_metres: value.riverMaximumWidthMetres,
            river_source_depth_metres: value.riverSourceDepthMetres,
            river_maximum_depth_metres: value.riverMaximumDepthMetres,
            ..Self::default()
        }
    }
}

impl From<MotuForestOptions> for ForestOptions {
    fn from(value: MotuForestOptions) -> Self {
        Self {
            patch_size_metres: value.patchSizeMetres,
            noise_threshold: value.noiseThreshold,
            noise_octaves: value.noiseOctaves,
            snowline_metres: value.snowlineMetres,
            prototype_count: value.prototypeCount,
            minimum_scale: value.minimumScale,
            maximum_scale: value.maximumScale,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Vector3ExportArray {
    pub data: *const Vec3,
    pub length: i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Vector2ExportArray {
    pub data: *const Vec2,
    pub length: i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct TriangleExportArray {
    pub data: *const i32,
    pub length: i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportArea {
    pub min: Vec3,
    pub max: Vec3,
}

impl From<ExportArea> for BoundingBox {
    fn from(value: ExportArea) -> Self {
        Self::new(value.min, value.max)
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportMesh {
    pub handle: *mut c_void,
    pub vertices: Vector3ExportArray,
    pub normals: Vector3ExportArray,
    pub triangles: TriangleExportArray,
    pub uv: Vector2ExportArray,
    pub material: Vector3ExportArray,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportMeshWithUv {
    pub handle: *mut c_void,
    pub vertices: Vector3ExportArray,
    pub normals: Vector3ExportArray,
    pub triangles: TriangleExportArray,
    pub uv: Vector2ExportArray,
    pub material: Vector3ExportArray,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportSurfaceMaps {
    pub handle: *mut c_void,
    pub width: i32,
    pub height: i32,
    pub normalRgb: *const u8,
    pub occlusion: *const u8,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportSeaMask {
    pub handle: *mut c_void,
    pub width: i32,
    pub height: i32,
    pub rg: *const u8,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportMeshArray {
    pub data: *mut ExportMesh,
    pub length: i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportMeshGrid {
    pub handle: *mut c_void,
    pub data: *const ExportMesh,
    pub length: i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct RiverEmitterExport {
    pub position: Vec3,
    pub direction: Vec3,
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportRiverEmitters {
    pub handle: *mut c_void,
    pub data: *const RiverEmitterExport,
    pub length: i32,
}

const _: () = assert!(size_of::<RiverEmitterExport>() == size_of::<[f32; 7]>());

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportDecoration {
    pub trees: Vector3ExportArray,
    pub bushes: Vector3ExportArray,
}

const _: () = assert!(size_of::<ExportDecoration>() == size_of::<Vector3ExportArray>() * 2);

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TreeMeshPrototype {
    pub offset: i32,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TreeMeshPrototypes {
    pub prototypes: *const TreeMeshPrototype,
    pub length: i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportTreeBillboards {
    pub mesh: ExportMesh,
    pub offsets: *mut i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportTreeBillboardsArray {
    pub octants: [ExportTreeBillboards; 8],
    pub offsetsHandle: *mut c_void,
}

struct BillboardOffsets([Vec<i32>; 8]);

#[derive(Debug)]
#[repr(C)]
pub struct ExportHeightMapWithSeaLevel {
    pub width: i32,
    pub height: i32,
    pub data: *mut f32,
    pub seaLevel: f32,
}

const TERRAIN_COLLIDER_TILE_COUNT: i32 = 64;
const MIN_TERRAIN_COLLIDER_SAMPLES_PER_TILE: i32 = 33;
const MAX_TERRAIN_COLLIDER_SAMPLES_PER_TILE: i32 = 129;
// Forest grids own one `ExportedMesh` allocation per tile. Keep malformed or
// hostile ABI requests bounded before the fixed-grid owner is allocated.
const MAX_FOREST_GRID_DIVISIONS: usize = 256;

fn terrain_collider_heightmap_dimension(samples_per_tile: i32) -> Option<i32> {
    let intervals_per_tile = samples_per_tile.checked_sub(1)?;
    let intervals_per_tile = u32::try_from(intervals_per_tile).ok()?;
    if !(MIN_TERRAIN_COLLIDER_SAMPLES_PER_TILE..=MAX_TERRAIN_COLLIDER_SAMPLES_PER_TILE)
        .contains(&samples_per_tile)
        || !intervals_per_tile.is_power_of_two()
    {
        return None;
    }

    TERRAIN_COLLIDER_TILE_COUNT
        .checked_mul(i32::try_from(intervals_per_tile).ok()?)?
        .checked_add(1)
}

fn forest_grid_arguments(
    area: *const ExportArea,
    visual_lod: i32,
    divisions: i32,
) -> Option<(BoundingBox, usize, usize, usize)> {
    let visual_lod = usize::try_from(visual_lod).ok()?;
    if visual_lod > 2 {
        return None;
    }
    let divisions = usize::try_from(divisions).ok()?;
    if divisions == 0 || divisions > MAX_FOREST_GRID_DIVISIONS {
        return None;
    }
    let tile_count = divisions.checked_mul(divisions)?;
    i32::try_from(tile_count).ok()?;

    let bounds = if area.is_null() {
        BoundingBox::default()
    } else {
        // SAFETY: the caller promises a readable ExportArea when non-null.
        unsafe { (*area).into() }
    };
    let finite = [
        bounds.min.x,
        bounds.min.y,
        bounds.min.z,
        bounds.max.x,
        bounds.max.y,
        bounds.max.z,
    ]
    .into_iter()
    .all(f32::is_finite);
    if !finite
        || bounds.max.x <= bounds.min.x
        || bounds.max.y <= bounds.min.y
        || bounds.max.z < bounds.min.z
    {
        return None;
    }
    Some((bounds, visual_lod, divisions, tile_count))
}

#[repr(C)]
struct BufferHeader {
    length: usize,
}

fn buffer_layout<T>(length: usize) -> Layout {
    let alignment = align_of::<BufferHeader>().max(align_of::<T>());
    let size = size_of::<BufferHeader>()
        .checked_add(
            size_of::<T>()
                .checked_mul(length)
                .expect("buffer size overflow"),
        )
        .expect("buffer size overflow");
    Layout::from_size_align(size.max(1), alignment).expect("valid buffer layout")
}

fn leak_buffer<T: Copy>(data: &[T]) -> *mut T {
    let layout = buffer_layout::<T>(data.len());
    // SAFETY: layout is non-zero and valid. Allocation failure is delegated to
    // the standard allocation error handler.
    let allocation = unsafe { alloc(layout) };
    if allocation.is_null() {
        handle_alloc_error(layout);
    }
    // SAFETY: allocation has room for the header followed by every T element.
    unsafe {
        allocation
            .cast::<BufferHeader>()
            .write(BufferHeader { length: data.len() });
        let output = allocation.add(size_of::<BufferHeader>()).cast::<T>();
        ptr::copy_nonoverlapping(data.as_ptr(), output, data.len());
        output
    }
}

unsafe fn release_buffer<T>(data: *mut T) {
    if data.is_null() {
        return;
    }
    // SAFETY: pointers accepted here must have been returned by leak_buffer.
    let allocation = unsafe { data.cast::<u8>().sub(size_of::<BufferHeader>()) };
    // SAFETY: allocation starts with a BufferHeader initialized by leak_buffer.
    let length = unsafe { allocation.cast::<BufferHeader>().read().length };
    // SAFETY: layout matches the one used by leak_buffer for this T and length.
    unsafe { dealloc(allocation, buffer_layout::<T>(length)) };
}

fn length_i32(length: usize) -> i32 {
    i32::try_from(length).unwrap_or(i32::MAX)
}

struct ExportedMesh {
    mesh: Mesh,
    material: Vec<Vec3>,
}

fn export_mesh(mesh: Mesh, material: Vec<Vec3>) -> ExportMesh {
    debug_assert!(material.is_empty() || material.len() == mesh.vertices.len());
    let owner = Box::new(ExportedMesh { mesh, material });
    let handle = Box::into_raw(owner);
    // SAFETY: handle remains owned by the caller until ReleaseMesh.
    let owner = unsafe { &*handle };
    let mesh = &owner.mesh;
    ExportMesh {
        handle: handle.cast(),
        vertices: Vector3ExportArray {
            data: mesh.vertices.as_ptr(),
            length: length_i32(mesh.vertices.len()),
        },
        normals: Vector3ExportArray {
            data: mesh.normals.as_ptr(),
            length: length_i32(mesh.normals.len()),
        },
        triangles: TriangleExportArray {
            data: mesh.triangles.as_ptr().cast(),
            length: length_i32(mesh.triangles.len()),
        },
        uv: Vector2ExportArray {
            data: mesh.uv.as_ptr(),
            length: length_i32(mesh.uv.len()),
        },
        material: Vector3ExportArray {
            data: owner.material.as_ptr(),
            length: length_i32(owner.material.len()),
        },
    }
}

fn export_mesh_grid(
    tiles: Vec<Mesh>,
    material_values: impl Fn(&Mesh) -> Vec<Vec3>,
) -> ExportMeshGrid {
    let exports: Vec<ExportMesh> = tiles
        .into_iter()
        .map(|tile| {
            let material = material_values(&tile);
            export_mesh(tile, material)
        })
        .collect();
    let owner = Box::new(exports);
    let output = ExportMeshGrid {
        handle: ptr::null_mut(),
        data: owner.as_ptr(),
        length: length_i32(owner.len()),
    };
    ExportMeshGrid {
        handle: Box::into_raw(owner).cast(),
        ..output
    }
}

fn export_forest_mesh_grid(tiles: Vec<Mesh>, expected_length: usize) -> Option<ExportMeshGrid> {
    // A forest accessor may represent the all-empty LOD2 wood stream with an
    // empty source vector. Expand only that representation; every other
    // length mismatch is rejected so the ABI always publishes a fixed grid.
    let tiles = match tiles.len() {
        length if length == expected_length => tiles,
        0 => vec![Mesh::default(); expected_length],
        _ => return None,
    };
    Some(export_mesh_grid(tiles, |_| Vec::new()))
}

fn export_surface_maps(maps: Box<SurfaceMaps>) -> ExportSurfaceMaps {
    let handle = Box::into_raw(maps);
    // SAFETY: handle remains owned by the caller until ReleaseSurfaceMaps.
    let maps = unsafe { &*handle };
    ExportSurfaceMaps {
        handle: handle.cast(),
        width: length_i32(maps.width() as usize),
        height: length_i32(maps.height() as usize),
        normalRgb: maps.normal_rgb().as_ptr(),
        occlusion: maps.occlusion().as_ptr(),
    }
}

fn export_sea_mask(mask: Box<SeaMask>) -> ExportSeaMask {
    let handle = Box::into_raw(mask);
    // SAFETY: handle remains owned by the caller until ReleaseSeaMask.
    let mask = unsafe { &*handle };
    ExportSeaMask {
        handle: handle.cast(),
        width: length_i32(mask.width() as usize),
        height: length_i32(mask.height() as usize),
        rg: mask.rg().as_ptr(),
    }
}

unsafe fn island_ref<'a>(handle: *const c_void) -> Option<&'a Island> {
    // SAFETY: caller promises handle is null or from CreateMotu/LoadMotu.
    unsafe { handle.cast::<Island>().as_ref() }
}

unsafe fn path_from_c(path: *const c_char) -> Option<PathBuf> {
    if path.is_null() {
        return None;
    }
    // SAFETY: caller promises a valid NUL-terminated path string.
    let path = unsafe { CStr::from_ptr(path) };
    Some(PathBuf::from(path.to_string_lossy().into_owned()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateMotu(seed: i32, options: *const MotuOptions) -> *mut c_void {
    let options = if options.is_null() {
        IslandOptions::default()
    } else {
        // SAFETY: non-null options must point to a readable MotuOptions.
        unsafe { (*options).into() }
    };
    Island::generate(u64::from(seed.cast_unsigned()), options).map_or(ptr::null_mut(), |island| {
        Box::into_raw(Box::new(island)).cast()
    })
}

/// Creates an island using the historical terrain options plus explicit
/// forest controls. `CreateMotu` remains the compatibility entry point and
/// uses validated Rust forest defaults.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateMotuWithForest(
    seed: i32,
    options: *const MotuOptions,
    forest_options: *const MotuForestOptions,
) -> *mut c_void {
    let options = if options.is_null() {
        IslandOptions::default()
    } else {
        // SAFETY: non-null options must point to a readable MotuOptions.
        unsafe { (*options).into() }
    };
    let forest_options = if forest_options.is_null() {
        ForestOptions::default()
    } else {
        // SAFETY: non-null forest_options must point to a readable
        // MotuForestOptions.
        unsafe { (*forest_options).into() }
    };
    Island::generate_with_forest(u64::from(seed.cast_unsigned()), options, forest_options)
        .map_or(ptr::null_mut(), |island| {
            Box::into_raw(Box::new(island)).cast()
        })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseMotu(handle: *mut c_void) {
    if !handle.is_null() {
        // SAFETY: handle came from CreateMotu or LoadMotu and is released once.
        drop(unsafe { Box::from_raw(handle.cast::<Island>()) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateProceduralTree(
    seed: i32,
    lod0_wood_output: *mut ExportMesh,
    lod0_foliage_output: *mut ExportMesh,
    lod1_wood_output: *mut ExportMesh,
    lod1_foliage_output: *mut ExportMesh,
) {
    let outputs = [
        lod0_wood_output,
        lod0_foliage_output,
        lod1_wood_output,
        lod1_foliage_output,
    ];
    if outputs.iter().any(|output| output.is_null())
        || outputs
            .iter()
            .enumerate()
            .any(|(index, output)| outputs[index + 1..].contains(output))
    {
        return;
    }
    // SAFETY: all pointers are non-null, distinct, and promised writable by
    // the caller. Defaults make every early observation safely releasable.
    for output in outputs {
        unsafe { output.write(ExportMesh::default()) };
    }
    let tree = generate_tree(u64::from(seed.cast_unsigned()));
    let lod0_wood = export_mesh(tree.lod0_wood, Vec::new());
    let lod0_foliage = export_mesh(tree.lod0_foliage, Vec::new());
    let lod1_wood = export_mesh(tree.lod1_wood, Vec::new());
    let lod1_foliage = export_mesh(tree.lod1_foliage, Vec::new());
    // SAFETY: output ownership transfers to the caller and each independent
    // handle must subsequently be passed to ReleaseMesh exactly once.
    unsafe {
        lod0_wood_output.write(lod0_wood);
        lod0_foliage_output.write(lod0_foliage);
        lod1_wood_output.write(lod1_wood);
        lod1_foliage_output.write(lod1_foliage);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateMesh(
    handle: *const c_void,
    area: *const ExportArea,
    lod: i32,
    clamp_sides: u8,
    output: *mut ExportMesh,
) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    let bounds = if area.is_null() {
        BoundingBox::default()
    } else {
        // SAFETY: non-null area must point to a readable ExportArea.
        unsafe { (*area).into() }
    };
    let lod = usize::try_from(lod.max(0)).unwrap_or(0);
    let Some(sliced) = island.render_mesh_in(lod, bounds, clamp_sides) else {
        return;
    };
    let material = island.material_values_for(&sliced);
    *output = export_mesh(sliced, material);
}

/// Exports the authoritative XY-safe surface for collision and downward
/// queries. This never returns render-only folds or overhangs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateSupportMesh(
    handle: *const c_void,
    area: *const ExportArea,
    lod: i32,
    output: *mut ExportMesh,
) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    let bounds = if area.is_null() {
        BoundingBox::default()
    } else {
        // SAFETY: non-null area must point to a readable ExportArea.
        unsafe { (*area).into() }
    };
    let lod = usize::try_from(lod.max(0)).unwrap_or(0);
    let Some(mesh) = island.mesh_in(lod, bounds) else {
        return;
    };
    let material = island.material_values_for(&mesh);
    *output = export_mesh(mesh, material);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateMeshGrid(
    handle: *const c_void,
    area: *const ExportArea,
    lod: i32,
    divisions: i32,
    clamp_sides: u8,
    output: *mut ExportMeshGrid,
) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    let lod = usize::try_from(lod.max(0)).unwrap_or(0);
    let bounds = if area.is_null() {
        BoundingBox::default()
    } else {
        // SAFETY: non-null area must point to a readable ExportArea.
        unsafe { (*area).into() }
    };
    let divisions = usize::try_from(divisions.max(0)).unwrap_or(0);
    let Some(tiles) = island.render_mesh_grid(lod, bounds, divisions, clamp_sides) else {
        return;
    };
    *output = export_mesh_grid(tiles, |tile| island.material_values_for(tile));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseMeshGrid(output: *mut ExportMeshGrid) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    if !output.handle.is_null() {
        // SAFETY: handle came from CreateMeshGrid and is released once.
        let mut meshes = unsafe { Box::from_raw(output.handle.cast::<Vec<ExportMesh>>()) };
        for mesh in meshes.iter_mut() {
            // SAFETY: each mesh follows ExportMesh ownership rules.
            unsafe { ReleaseMesh(mesh) };
        }
    }
    *output = ExportMeshGrid::default();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseMesh(output: *mut ExportMesh) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    if !output.handle.is_null() {
        // SAFETY: handle came from export_mesh and is released once.
        drop(unsafe { Box::from_raw(output.handle.cast::<ExportedMesh>()) });
        *output = ExportMesh::default();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateRiverMesh(
    handle: *const c_void,
    area: *const ExportArea,
    output: *mut ExportMeshWithUv,
) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    let bounds = if area.is_null() {
        BoundingBox::default()
    } else {
        // SAFETY: non-null area must point to a readable ExportArea.
        unsafe { (*area).into() }
    };
    let mesh = island.river_mesh().sliced(bounds);
    let uv_data = mesh.uv.as_ptr();
    let uv_length = length_i32(mesh.uv.len());
    let base = export_mesh(mesh, Vec::new());
    *output = ExportMeshWithUv {
        handle: base.handle,
        vertices: base.vertices,
        normals: base.normals,
        triangles: base.triangles,
        uv: Vector2ExportArray {
            data: uv_data,
            length: uv_length,
        },
        material: base.material,
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateRiverMeshGrid(
    handle: *const c_void,
    area: *const ExportArea,
    divisions: i32,
    output: *mut ExportMeshGrid,
) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    let bounds = if area.is_null() {
        BoundingBox::default()
    } else {
        // SAFETY: non-null area must point to a readable ExportArea.
        unsafe { (*area).into() }
    };
    let divisions = usize::try_from(divisions.max(0)).unwrap_or(0);
    *output = export_mesh_grid(island.river_mesh().sliced_grid(bounds, divisions), |_| {
        Vec::new()
    });
}

/// Exports whole-tree owner tiles for the requested visual wood LOD.
///
/// Visual LOD mapping is owned by `Island`: LOD0 and LOD1 select the matching
/// combined streams while LOD2 is a fixed, empty grid. Every successful call
/// publishes exactly `divisions * divisions` releasable mesh entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateForestWoodMeshGrid(
    handle: *const c_void,
    area: *const ExportArea,
    visual_lod: i32,
    divisions: i32,
    output: *mut ExportMeshGrid,
) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    *output = ExportMeshGrid::default();
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some((bounds, visual_lod, divisions, tile_count)) =
        forest_grid_arguments(area, visual_lod, divisions)
    else {
        return;
    };
    let Some(tiles) = island.forest_mesh_grid(ForestMeshKind::Wood, visual_lod, bounds, divisions)
    else {
        return;
    };
    let Some(export) = export_forest_mesh_grid(tiles, tile_count) else {
        return;
    };
    *output = export;
}

/// Exports whole-cluster owner tiles for the requested visual foliage LOD.
///
/// Visual LOD2 and LOD1 intentionally share the low-poly foliage stream. The
/// island accessor performs that mapping and returns a fixed owner grid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateForestFoliageMeshGrid(
    handle: *const c_void,
    area: *const ExportArea,
    visual_lod: i32,
    divisions: i32,
    output: *mut ExportMeshGrid,
) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    *output = ExportMeshGrid::default();
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some((bounds, visual_lod, divisions, tile_count)) =
        forest_grid_arguments(area, visual_lod, divisions)
    else {
        return;
    };
    let Some(tiles) =
        island.forest_mesh_grid(ForestMeshKind::Foliage, visual_lod, bounds, divisions)
    else {
        return;
    };
    let Some(export) = export_forest_mesh_grid(tiles, tile_count) else {
        return;
    };
    *output = export;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateRiverRockMeshGrid(
    handle: *const c_void,
    area: *const ExportArea,
    divisions: i32,
    output: *mut ExportMeshGrid,
) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    let bounds = if area.is_null() {
        BoundingBox::default()
    } else {
        // SAFETY: non-null area must point to a readable ExportArea.
        unsafe { (*area).into() }
    };
    let divisions = usize::try_from(divisions.max(0)).unwrap_or(0);
    *output = export_mesh_grid(
        island.river_rock_mesh().sliced_grid(bounds, divisions),
        |_| Vec::new(),
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateRiverEmitters(
    handle: *const c_void,
    sharpness_degrees: f32,
    spacing_metres: f32,
    output: *mut ExportRiverEmitters,
) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    let emitters: Vec<RiverEmitterExport> = island
        .river_emitters(sharpness_degrees, spacing_metres)
        .into_iter()
        .map(|emitter| RiverEmitterExport {
            position: emitter.position,
            direction: emitter.direction,
            strength: emitter.strength,
        })
        .collect();
    let owner = Box::new(emitters);
    let export = ExportRiverEmitters {
        handle: ptr::null_mut(),
        data: owner.as_ptr(),
        length: length_i32(owner.len()),
    };
    *output = ExportRiverEmitters {
        handle: Box::into_raw(owner).cast(),
        ..export
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseRiverEmitters(output: *mut ExportRiverEmitters) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    if !output.handle.is_null() {
        // SAFETY: handle came from CreateRiverEmitters and is released once.
        drop(unsafe { Box::from_raw(output.handle.cast::<Vec<RiverEmitterExport>>()) });
    }
    *output = ExportRiverEmitters::default();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseMeshWithUV(output: *mut ExportMeshWithUv) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    let mut base = ExportMesh {
        handle: output.handle,
        vertices: output.vertices,
        normals: output.normals,
        triangles: output.triangles,
        uv: output.uv,
        material: output.material,
    };
    // SAFETY: base owns the same mesh handle.
    unsafe { ReleaseMesh(&raw mut base) };
    *output = ExportMeshWithUv::default();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GetDecoration(handle: *const c_void, output: *mut ExportDecoration) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    let decorations = island.decorations();
    *output = ExportDecoration {
        trees: Vector3ExportArray {
            data: decorations.trees().as_ptr(),
            length: length_i32(decorations.trees().len()),
        },
        bushes: Vector3ExportArray {
            data: decorations.bushes().as_ptr(),
            length: length_i32(decorations.bushes().len()),
        },
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateTreeBillboards(
    handle: *const c_void,
    input: *const TreeMeshPrototypes,
    output: *mut ExportTreeBillboardsArray,
) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(input) = (unsafe { input.as_ref() }) else {
        return;
    };
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    if input.prototypes.is_null() || input.length <= 0 {
        return;
    }
    let length = usize::try_from(input.length).unwrap_or(0);
    // SAFETY: input promises a readable prototype array of the given length.
    let prototypes = unsafe { std::slice::from_raw_parts(input.prototypes, length) };
    let trees = island.decorations().trees();
    let mut offsets: [Vec<i32>; 8] = std::array::from_fn(|_| Vec::with_capacity(length));

    for (octant, offset_list) in offsets.iter_mut().enumerate() {
        let angle = f32::from(u8::try_from(octant).unwrap_or(0)) * std::f32::consts::TAU / 8.0;
        let side = Vec2::new(angle.cos(), angle.sin());
        let facing = Vec3::new(-side.y, side.x, 0.0);
        let mut mesh = Mesh::default();
        for (prototype_index, prototype) in prototypes.iter().enumerate() {
            let Ok(tree_index) = usize::try_from(prototype.offset) else {
                continue;
            };
            let Some(&tree) = trees.get(tree_index) else {
                continue;
            };
            let scale = prototype.scale.max(0.000_001);
            let half_width = scale * 0.5;
            let height = scale * 2.0;
            let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
            mesh.vertices.extend([
                Vec3::new(
                    tree.x - side.x * half_width,
                    tree.y - side.y * half_width,
                    tree.z,
                ),
                Vec3::new(
                    tree.x + side.x * half_width,
                    tree.y + side.y * half_width,
                    tree.z,
                ),
                Vec3::new(
                    tree.x - side.x * half_width,
                    tree.y - side.y * half_width,
                    tree.z + height,
                ),
                Vec3::new(
                    tree.x + side.x * half_width,
                    tree.y + side.y * half_width,
                    tree.z + height,
                ),
            ]);
            mesh.normals.extend([facing; 4]);
            mesh.uv.extend([
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
            ]);
            mesh.triangles
                .extend([base, base + 2, base + 1, base + 1, base + 2, base + 3]);
            offset_list.push(i32::try_from(prototype_index).unwrap_or(i32::MAX));
        }
        output.octants[octant].mesh = export_mesh(mesh, Vec::new());
        output.octants[octant].offsets = offset_list.as_mut_ptr();
    }
    output.offsetsHandle = Box::into_raw(Box::new(BillboardOffsets(offsets))).cast();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseTreeBillboards(output: *mut ExportTreeBillboardsArray) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    for octant in &mut output.octants {
        // SAFETY: each mesh was allocated by CreateTreeBillboards.
        unsafe { ReleaseMesh(&raw mut octant.mesh) };
        octant.offsets = ptr::null_mut();
    }
    if !output.offsetsHandle.is_null() {
        // SAFETY: handle came from CreateTreeBillboards and is released once.
        let offsets = unsafe { Box::from_raw(output.offsetsHandle.cast::<BillboardOffsets>()) };
        let _ = offsets.0.iter().map(Vec::len).sum::<usize>();
        drop(offsets);
    }
    *output = ExportTreeBillboardsArray::default();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateHeightMap(
    handle: *const c_void,
    resolution: i32,
) -> *mut ExportHeightMapWithSeaLevel {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return ptr::null_mut();
    };
    let resolution = resolution.max(1);
    let dimension = u32::try_from(resolution).unwrap_or(1);
    let data = island.height_map(dimension, dimension);
    Box::into_raw(Box::new(ExportHeightMapWithSeaLevel {
        width: resolution,
        height: resolution,
        data: leak_buffer(&data),
        seaLevel: 0.0,
    }))
}

/// Samples the final LOD 0 terrain on one global lattice whose overlapping
/// rows and columns are shared by all 64x64 Unity terrain-collider tiles.
///
/// `samples_per_tile` must be 33, 65, or 129. The returned map owns its data
/// and must be released exactly once with `ReleaseTerrainColliderHeightMap`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateTerrainColliderHeightMap(
    handle: *const c_void,
    samples_per_tile: i32,
) -> *mut ExportHeightMapWithSeaLevel {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return ptr::null_mut();
    };
    let Some(dimension) = terrain_collider_heightmap_dimension(samples_per_tile) else {
        return ptr::null_mut();
    };
    let Ok(dimension_u32) = u32::try_from(dimension) else {
        return ptr::null_mut();
    };
    let data = island.height_map(dimension_u32, dimension_u32);
    Box::into_raw(Box::new(ExportHeightMapWithSeaLevel {
        width: dimension,
        height: dimension,
        data: leak_buffer(&data),
        seaLevel: 0.0,
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseHeightMap(map: *mut ExportHeightMapWithSeaLevel) {
    if map.is_null() {
        return;
    }
    // SAFETY: map came from CreateHeightMap and is released once.
    let map = unsafe { Box::from_raw(map) };
    // SAFETY: data came from leak_buffer and is released once.
    unsafe { release_buffer(map.data) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseTerrainColliderHeightMap(map: *mut ExportHeightMapWithSeaLevel) {
    // SAFETY: both height-map constructors use the same owned representation.
    unsafe { ReleaseHeightMap(map) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateNormalMap(
    handle: *const c_void,
    lod: i32,
    dimension: i32,
) -> *mut u8 {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return ptr::null_mut();
    };
    let dimension = u32::try_from(dimension.max(1)).unwrap_or(1);
    let Ok(lod) = usize::try_from(lod) else {
        return ptr::null_mut();
    };
    island
        .surface_maps(lod, dimension, dimension)
        .map_or(ptr::null_mut(), |maps| leak_buffer(maps.normal_rgb()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseNormalMap(data: *mut u8) {
    // SAFETY: data came from CreateNormalMap and is released once.
    unsafe { release_buffer(data) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateSurfaceMaps(
    handle: *const c_void,
    lod: i32,
    dimension: i32,
    output: *mut ExportSurfaceMaps,
) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    *output = ExportSurfaceMaps::default();
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Ok(lod) = usize::try_from(lod) else {
        return;
    };
    let dimension = u32::try_from(dimension.max(1)).unwrap_or(1);
    let Some(maps) = island.surface_maps(lod, dimension, dimension) else {
        return;
    };
    *output = export_surface_maps(Box::new(maps));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseSurfaceMaps(output: *mut ExportSurfaceMaps) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    if !output.handle.is_null() {
        // SAFETY: handle came from CreateSurfaceMaps and is released once.
        drop(unsafe { Box::from_raw(output.handle.cast::<SurfaceMaps>()) });
    }
    *output = ExportSurfaceMaps::default();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateSeaMask(
    handle: *const c_void,
    dimension: i32,
    output: *mut ExportSeaMask,
) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    *output = ExportSeaMask::default();
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let dimension = u32::try_from(dimension.max(1)).unwrap_or(1);
    let Some(mask) = island.sea_mask(dimension, dimension) else {
        return;
    };
    *output = export_sea_mask(Box::new(mask));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseSeaMask(output: *mut ExportSeaMask) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    if !output.handle.is_null() {
        // SAFETY: handle came from CreateSeaMask and is released once.
        drop(unsafe { Box::from_raw(output.handle.cast::<SeaMask>()) });
    }
    *output = ExportSeaMask::default();
}

#[unsafe(no_mangle)]
pub extern "C" fn CreateNormalMap3DC(
    _handle: *const c_void,
    _lod: i32,
    _dimension: i32,
) -> *mut u8 {
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn ReleaseNormalMap3DC(_data: *mut u8) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ExportFoliageData(handle: *const c_void, dimension: i32) -> *mut u32 {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return ptr::null_mut();
    };
    leak_buffer(&island.foliage_map(u32::try_from(dimension.max(1)).unwrap_or(1)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseFoliageData(data: *mut u32) {
    // SAFETY: data came from ExportFoliageData and is released once.
    unsafe { release_buffer(data) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateSeaDepthMap(handle: *const c_void, dimension: i32) -> *mut f32 {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return ptr::null_mut();
    };
    let dimension = u32::try_from(dimension.max(1)).unwrap_or(1);
    leak_buffer(&island.sea_depth_map(dimension, dimension))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseSeaDepthMap(data: *mut f32) {
    // SAFETY: data came from CreateSeaDepthMap and is released once.
    unsafe { release_buffer(data) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn SaveMotu(handle: *const c_void, path: *const c_char) {
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let Some(path) = (unsafe { path_from_c(path) }) else {
        return;
    };
    let _ = island.save(path);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn LoadMotu(path: *const c_char) -> *mut c_void {
    let Some(path) = (unsafe { path_from_c(path) }) else {
        return ptr::null_mut();
    };
    Island::load(path).map_or(ptr::null_mut(), |island| {
        Box::into_raw(Box::new(island)).cast()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseMeshes(output: *mut ExportMeshArray) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    if output.data.is_null() || output.length <= 0 {
        return;
    }
    // SAFETY: data is an array of length elements supplied by the ABI caller.
    let length = usize::try_from(output.length).unwrap_or(0);
    let meshes = unsafe { std::slice::from_raw_parts_mut(output.data, length) };
    for mesh in meshes {
        // SAFETY: each element follows ExportMesh ownership rules.
        unsafe { ReleaseMesh(mesh) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn SetLogFile(_path: *const c_char) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forest_grid_invalid_inputs_leave_a_default_export() {
        let invalid_areas = [
            ExportArea {
                min: Vec3::new(f32::NAN, 0.0, f32::MIN),
                max: Vec3::new(1.0, 1.0, f32::MAX),
            },
            ExportArea {
                min: Vec3::new(0.0, 0.0, f32::MIN),
                max: Vec3::new(0.0, 1.0, f32::MAX),
            },
            ExportArea {
                min: Vec3::new(0.0, 0.0, 1.0),
                max: Vec3::new(1.0, 1.0, 0.0),
            },
        ];
        for area in invalid_areas {
            assert!(forest_grid_arguments(&raw const area, 0, 8).is_none());
        }

        for (visual_lod, divisions) in [
            (3, 8),
            (-1, 8),
            (0, 0),
            (0, -2),
            (0, i32::MAX),
            (
                0,
                i32::try_from(MAX_FOREST_GRID_DIVISIONS + 1).unwrap_or(i32::MAX),
            ),
        ] {
            assert!(forest_grid_arguments(ptr::null(), visual_lod, divisions).is_none());
        }

        assert!(forest_grid_arguments(ptr::null(), 0, 8).is_some());

        let mut output = ExportMeshGrid {
            handle: ptr::dangling_mut(),
            data: ptr::dangling(),
            length: 13,
        };
        unsafe {
            CreateForestFoliageMeshGrid(ptr::null(), ptr::null(), 0, 8, &raw mut output);
        }
        assert!(output.handle.is_null());
        assert!(output.data.is_null());
        assert_eq!(output.length, 0);
    }

    #[test]
    fn forest_grid_rejects_non_finite_bounds_directly() {
        let area = ExportArea {
            min: Vec3::new(0.0, 0.0, f32::INFINITY),
            max: Vec3::new(1.0, 1.0, f32::MAX),
        };
        assert!(forest_grid_arguments(&raw const area, 0, 8).is_none());
    }

    #[test]
    fn forest_grid_null_output_is_safe() {
        unsafe {
            CreateForestWoodMeshGrid(ptr::null(), ptr::null(), 0, 8, ptr::null_mut());
        }
    }

    #[test]
    fn forest_grid_default_bounds_are_valid() {
        let (bounds, visual_lod, divisions, tile_count) =
            forest_grid_arguments(ptr::null(), 2, 8).expect("default bounds are valid");
        assert_eq!(visual_lod, 2);
        assert_eq!(divisions, 8);
        assert_eq!(tile_count, 64);
        assert_eq!(bounds, BoundingBox::default());
    }

    /*
     * Keep the valid-argument FFI call separate from the direct validation
     * tests above: a null island should still reset the output without
     * allocating a grid.
     */
    #[test]
    fn forest_grid_null_island_leaves_a_default_export() {
        let mut output = ExportMeshGrid {
            handle: ptr::dangling_mut(),
            data: ptr::dangling(),
            length: 13,
        };
        unsafe {
            CreateForestWoodMeshGrid(ptr::null(), ptr::null(), 0, 8, &raw mut output);
        }
        assert!(output.handle.is_null());
        assert!(output.data.is_null());
        assert_eq!(output.length, 0);
    }

    #[test]
    fn forest_grid_valid_lifecycle_on_a_small_island() {
        let terrain_options = IslandOptions {
            terrain_size: 16,
            ..IslandOptions::default()
        };
        let forest_options = ForestOptions {
            noise_threshold: 1.0,
            prototype_count: 1,
            ..ForestOptions::default()
        };
        let island = Island::generate_with_forest(2018, terrain_options, forest_options)
            .expect("small island generation should succeed");
        let handle = Box::into_raw(Box::new(island)).cast::<c_void>();

        // SAFETY: handle is a freshly allocated Island and each output is a
        // distinct writable value released through its paired grid function.
        unsafe {
            let mut wood_lod2 = ExportMeshGrid::default();
            CreateForestWoodMeshGrid(handle, ptr::null(), 2, 2, &raw mut wood_lod2);
            assert!(!wood_lod2.handle.is_null());
            assert_eq!(wood_lod2.length, 4);
            let wood_tiles = std::slice::from_raw_parts(wood_lod2.data, 4);
            assert!(wood_tiles.iter().all(|tile| {
                !tile.handle.is_null()
                    && tile.vertices.length == 0
                    && tile.normals.length == 0
                    && tile.triangles.length == 0
            }));

            let mut foliage_lod1 = ExportMeshGrid::default();
            let mut foliage_lod2 = ExportMeshGrid::default();
            CreateForestFoliageMeshGrid(handle, ptr::null(), 1, 2, &raw mut foliage_lod1);
            CreateForestFoliageMeshGrid(handle, ptr::null(), 2, 2, &raw mut foliage_lod2);
            assert!(!foliage_lod1.handle.is_null());
            assert!(!foliage_lod2.handle.is_null());
            assert_eq!(foliage_lod1.length, 4);
            assert_eq!(foliage_lod2.length, 4);
            ReleaseMeshGrid(&raw mut wood_lod2);
            assert!(wood_lod2.handle.is_null());
            assert!(!foliage_lod1.handle.is_null());
            ReleaseMeshGrid(&raw mut foliage_lod1);
            ReleaseMeshGrid(&raw mut foliage_lod2);
            assert!(foliage_lod1.handle.is_null());
            assert!(foliage_lod2.handle.is_null());
            ReleaseMotu(handle);
        }
    }

    #[test]
    fn null_island_leaves_a_default_sea_mask_export() {
        let mut output = ExportSeaMask {
            handle: ptr::dangling_mut::<u8>().cast(),
            width: 7,
            height: 9,
            rg: ptr::dangling(),
        };
        unsafe { CreateSeaMask(ptr::null(), 16, &raw mut output) };
        assert!(output.handle.is_null());
        assert!(output.rg.is_null());
        assert_eq!(output.width, 0);
        assert_eq!(output.height, 0);
    }

    fn terrain_attributes_match(mesh: &ExportMesh) -> bool {
        mesh.uv.length == mesh.vertices.length && mesh.material.length == mesh.vertices.length
    }

    unsafe fn assert_material_channels(mesh: &ExportMesh) {
        let values = unsafe {
            std::slice::from_raw_parts(mesh.material.data, mesh.material.length as usize)
        };
        if let Some((index, value)) = values.iter().enumerate().find(|(_, value)| {
            !value.is_finite()
                || !value.cmpge(Vec3::ZERO).all()
                || value.x > 1.0
                || value.y > 1.0
                || value.z > 1.0
        }) {
            panic!("invalid material value at {index}: {value:?}");
        }
        assert!(values.iter().any(|value| value.x > 0.1));
        assert!(values.iter().any(|value| value.y > 0.1));
        assert!(values.iter().any(|value| value.z > 0.9));
        assert!(values.iter().any(|value| value.z == 0.0));
    }

    unsafe fn assert_river_emitters(handle: *const c_void) {
        let mut output = ExportRiverEmitters::default();
        unsafe { CreateRiverEmitters(handle, 35.0, 2.0, &raw mut output) };
        assert!(!output.handle.is_null());
        assert!(output.length > 0);
        let values = unsafe { std::slice::from_raw_parts(output.data, output.length as usize) };
        assert!(values.iter().all(|emitter| {
            emitter.position.is_finite()
                && emitter.direction.is_finite()
                && (emitter.direction.length() - 1.0).abs() < 1.0e-4
                && (0.0..=1.0).contains(&emitter.strength)
        }));
        unsafe { ReleaseRiverEmitters(&raw mut output) };
        assert!(output.handle.is_null());
    }

    unsafe fn assert_sea_mask(handle: *const c_void) {
        let mut sea_mask = ExportSeaMask::default();
        unsafe { CreateSeaMask(handle, 16, &raw mut sea_mask) };
        assert!(!sea_mask.handle.is_null());
        assert_eq!(sea_mask.width, 16);
        assert_eq!(sea_mask.height, 16);
        assert!(!sea_mask.rg.is_null());
        let pixels = unsafe { std::slice::from_raw_parts(sea_mask.rg, 16 * 16 * 2) };
        assert_eq!(pixels.len(), 16 * 16 * 2);
        unsafe { ReleaseSeaMask(&raw mut sea_mask) };
        assert!(sea_mask.handle.is_null());
        assert!(sea_mask.rg.is_null());
        assert_eq!(sea_mask.width, 0);
        assert_eq!(sea_mask.height, 0);
    }

    fn test_options() -> MotuOptions {
        MotuOptions {
            maxZ: 0.2,
            waterRatio: 0.6,
            slopeMultiplier: 1.3,
            coastalSlopeMultiplier: 1.0,
            removedCoastalErosionStrength: 0.0,
            removedBeachFormationStrength: 0.0,
            hydraulicErosionStrength: 0.25,
            hydraulicDepositionStrength: 1.5,
            hydraulicDepositionSlopeDegrees: 12.0,
            riverSourceCatchmentHectares: 0.05,
            riverSourceSteepMultiplier: 4.0,
            riverSourceElevationBoost: 9.0,
            riverSourceWidthMetres: 2.0,
            riverMaximumWidthMetres: 14.0,
            riverSourceDepthMetres: 0.35,
            riverMaximumDepthMetres: 2.0,
        }
    }

    fn test_forest_options() -> MotuForestOptions {
        MotuForestOptions {
            patchSizeMetres: 200.0,
            noiseThreshold: 0.62,
            noiseOctaves: 4,
            snowlineMetres: 100.0,
            prototypeCount: 8,
            minimumScale: 0.85,
            maximumScale: 1.15,
        }
    }

    #[test]
    fn motu_forest_options_forward_settings() {
        let forest = ForestOptions::from(test_forest_options());
        assert!((forest.patch_size_metres - 200.0).abs() < f32::EPSILON);
        assert!((forest.noise_threshold - 0.62).abs() < f32::EPSILON);
        assert_eq!(forest.noise_octaves, 4);
        assert!((forest.snowline_metres - 100.0).abs() < f32::EPSILON);
        assert_eq!(forest.prototype_count, 8);
        assert!((forest.minimum_scale - 0.85).abs() < f32::EPSILON);
        assert!((forest.maximum_scale - 1.15).abs() < f32::EPSILON);
    }

    #[test]
    fn invalid_forest_options_reject_island_creation() {
        let options = test_options();
        let mut forest_options = test_forest_options();
        forest_options.noiseThreshold = f32::NAN;
        // SAFETY: the options pointer is valid for this call; validation must
        // reject it before terrain generation allocates an island.
        let handle =
            unsafe { CreateMotuWithForest(2018, &raw const options, &raw const forest_options) };
        assert!(handle.is_null());
    }

    unsafe fn assert_river_exports(handle: *const c_void) {
        let mut river_grid = ExportMeshGrid::default();
        unsafe { CreateRiverMeshGrid(handle, ptr::null(), 8, &raw mut river_grid) };
        assert!(!river_grid.handle.is_null());
        assert_eq!(river_grid.length, 64);
        let river_tiles =
            unsafe { std::slice::from_raw_parts(river_grid.data, river_grid.length as usize) };
        assert!(river_tiles.iter().any(|tile| tile.triangles.length > 0));
        assert!(
            river_tiles
                .iter()
                .all(|tile| tile.uv.length == tile.vertices.length && tile.material.length == 0)
        );
        unsafe { ReleaseMeshGrid(&raw mut river_grid) };
        assert!(river_grid.handle.is_null());

        let mut river_rock_grid = ExportMeshGrid::default();
        unsafe { CreateRiverRockMeshGrid(handle, ptr::null(), 8, &raw mut river_rock_grid) };
        assert!(!river_rock_grid.handle.is_null());
        assert_eq!(river_rock_grid.length, 64);
        let river_rock_tiles = unsafe {
            std::slice::from_raw_parts(river_rock_grid.data, river_rock_grid.length as usize)
        };
        assert!(river_rock_tiles.iter().all(|tile| {
            tile.normals.length == tile.vertices.length
                && tile.uv.length == 0
                && tile.material.length == 0
        }));
        unsafe { ReleaseMeshGrid(&raw mut river_rock_grid) };
        assert!(river_rock_grid.handle.is_null());

        unsafe { assert_river_emitters(handle) };
    }

    #[test]
    #[ignore = "slow export API lifecycle test; run explicitly when the export API changes"]
    #[allow(clippy::too_many_lines)]
    fn ffi_allocations_have_matching_release_functions() {
        let options = test_options();
        let mut forest_options = test_forest_options();
        // Keep this broad allocation smoke test focused on ABI ownership. The
        // small-island test above exercises the same forest grids without
        // asking the historical terrain fixture to assemble a large forest.
        forest_options.noiseThreshold = 1.0;
        forest_options.prototypeCount = 1;
        // SAFETY: this test passes valid pointers and releases every returned
        // allocation exactly once through its paired ABI function.
        unsafe {
            let mut tree_lod0_wood = ExportMesh::default();
            let mut tree_lod0_foliage = ExportMesh::default();
            let mut tree_lod1_wood = ExportMesh::default();
            let mut tree_lod1_foliage = ExportMesh::default();
            CreateProceduralTree(
                2018,
                &raw mut tree_lod0_wood,
                &raw mut tree_lod0_foliage,
                &raw mut tree_lod1_wood,
                &raw mut tree_lod1_foliage,
            );
            for tree_mesh in [
                &raw mut tree_lod0_wood,
                &raw mut tree_lod0_foliage,
                &raw mut tree_lod1_wood,
                &raw mut tree_lod1_foliage,
            ] {
                assert!(!(*tree_mesh).handle.is_null());
                assert!((*tree_mesh).triangles.length > 0);
                assert_eq!((*tree_mesh).vertices.length, (*tree_mesh).normals.length);
                ReleaseMesh(tree_mesh);
            }

            let handle = CreateMotuWithForest(2018, &raw const options, &raw const forest_options);
            assert!(!handle.is_null());

            let mut mesh = ExportMesh::default();
            CreateMesh(handle, ptr::null(), 2, 0, &raw mut mesh);
            assert!(!mesh.handle.is_null());
            assert!(mesh.triangles.length > 0);
            assert!(terrain_attributes_match(&mesh));
            ReleaseMesh(&raw mut mesh);

            let mut support = ExportMesh::default();
            CreateSupportMesh(handle, ptr::null(), 0, &raw mut support);
            assert!(!support.handle.is_null());
            assert!(support.triangles.length > 0);
            assert!(terrain_attributes_match(&support));
            assert_material_channels(&support);
            ReleaseMesh(&raw mut support);

            let mut grid = ExportMeshGrid::default();
            CreateMeshGrid(handle, ptr::null(), 2, 8, 0, &raw mut grid);
            assert!(!grid.handle.is_null());
            assert_eq!(grid.length, 64);
            let tiles = std::slice::from_raw_parts(grid.data, grid.length as usize);
            assert!(tiles.iter().any(|tile| tile.triangles.length > 0));
            assert!(tiles.iter().all(terrain_attributes_match));
            ReleaseMeshGrid(&raw mut grid);
            assert!(grid.handle.is_null());

            let mut forest_wood_lod2 = ExportMeshGrid::default();
            CreateForestWoodMeshGrid(handle, ptr::null(), 2, 8, &raw mut forest_wood_lod2);
            assert!(!forest_wood_lod2.handle.is_null());
            assert_eq!(forest_wood_lod2.length, 64);
            let forest_wood_lod2_tiles =
                std::slice::from_raw_parts(forest_wood_lod2.data, forest_wood_lod2.length as usize);
            assert!(forest_wood_lod2_tiles.iter().all(|tile| {
                tile.vertices.length == 0
                    && tile.normals.length == 0
                    && tile.triangles.length == 0
                    && tile.uv.length == 0
            }));

            let mut forest_foliage_lod1 = ExportMeshGrid::default();
            let mut forest_foliage_lod2 = ExportMeshGrid::default();
            CreateForestFoliageMeshGrid(handle, ptr::null(), 1, 8, &raw mut forest_foliage_lod1);
            CreateForestFoliageMeshGrid(handle, ptr::null(), 2, 8, &raw mut forest_foliage_lod2);
            assert!(!forest_foliage_lod1.handle.is_null());
            assert!(!forest_foliage_lod2.handle.is_null());
            assert_eq!(forest_foliage_lod1.length, 64);
            assert_eq!(forest_foliage_lod2.length, 64);
            let foliage_lod1_tiles = std::slice::from_raw_parts(forest_foliage_lod1.data, 64);
            let foliage_lod2_tiles = std::slice::from_raw_parts(forest_foliage_lod2.data, 64);
            assert!(
                foliage_lod1_tiles
                    .iter()
                    .zip(foliage_lod2_tiles)
                    .all(|(lod1, lod2)| {
                        lod1.vertices.length == lod2.vertices.length
                            && lod1.normals.length == lod2.normals.length
                            && lod1.triangles.length == lod2.triangles.length
                            && lod1.uv.length == lod2.uv.length
                    })
            );
            ReleaseMeshGrid(&raw mut forest_wood_lod2);
            assert!(forest_wood_lod2.handle.is_null());
            assert!(!forest_foliage_lod1.handle.is_null());
            ReleaseMeshGrid(&raw mut forest_foliage_lod1);
            ReleaseMeshGrid(&raw mut forest_foliage_lod2);
            assert!(forest_foliage_lod1.handle.is_null());
            assert!(forest_foliage_lod2.handle.is_null());

            assert_river_exports(handle);

            let height_map = CreateHeightMap(handle, 16);
            assert!(!height_map.is_null());
            assert_eq!((*height_map).width, 16);
            ReleaseHeightMap(height_map);
            assert!(CreateTerrainColliderHeightMap(handle, 32).is_null());
            assert!(CreateTerrainColliderHeightMap(handle, 66).is_null());
            assert!(CreateTerrainColliderHeightMap(ptr::null(), 65).is_null());
            ReleaseTerrainColliderHeightMap(ptr::null_mut());

            let normal_map = CreateNormalMap(handle, 0, 16);
            assert!(!normal_map.is_null());
            ReleaseNormalMap(normal_map);

            let mut surface_maps = ExportSurfaceMaps::default();
            CreateSurfaceMaps(handle, 1, 16, &raw mut surface_maps);
            assert!(!surface_maps.handle.is_null());
            assert_eq!(surface_maps.width, 16);
            assert_eq!(surface_maps.height, 16);
            assert!(!surface_maps.normalRgb.is_null());
            assert!(!surface_maps.occlusion.is_null());
            ReleaseSurfaceMaps(&raw mut surface_maps);
            assert!(surface_maps.handle.is_null());

            assert_sea_mask(handle);

            let foliage = ExportFoliageData(handle, 16);
            assert!(!foliage.is_null());
            ReleaseFoliageData(foliage);

            let mut decoration = ExportDecoration::default();
            GetDecoration(handle, &raw mut decoration);
            if decoration.trees.length > 0 {
                let prototype = TreeMeshPrototype {
                    offset: 0,
                    scale: 1.0 / 8192.0,
                };
                let prototypes = TreeMeshPrototypes {
                    prototypes: &raw const prototype,
                    length: 1,
                };
                let mut billboards = ExportTreeBillboardsArray::default();
                CreateTreeBillboards(handle, &raw const prototypes, &raw mut billboards);
                assert!(billboards.octants[0].mesh.vertices.length > 0);
                ReleaseTreeBillboards(&raw mut billboards);
            }

            ReleaseMotu(handle);
        }
    }

    #[test]
    fn procedural_tree_exports_independently_releasable_meshes() {
        // SAFETY: the test supplies distinct writable outputs and releases
        // both handles exactly once.
        unsafe {
            let mut untouched = ExportMesh::default();
            CreateProceduralTree(
                7,
                &raw mut untouched,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            assert!(untouched.handle.is_null());

            let mut lod0_wood = ExportMesh::default();
            let mut lod0_foliage = ExportMesh::default();
            let mut lod1_wood = ExportMesh::default();
            let mut lod1_foliage = ExportMesh::default();
            CreateProceduralTree(
                7,
                &raw mut lod0_wood,
                &raw mut lod0_foliage,
                &raw mut lod1_wood,
                &raw mut lod1_foliage,
            );
            assert!(lod0_wood.vertices.length > lod1_wood.vertices.length);
            assert!(lod0_foliage.vertices.length > lod1_foliage.vertices.length);
            for tree_mesh in [
                &raw mut lod0_wood,
                &raw mut lod0_foliage,
                &raw mut lod1_wood,
                &raw mut lod1_foliage,
            ] {
                assert!(!(*tree_mesh).handle.is_null());
                assert!((*tree_mesh).vertices.length > 0);
                assert_eq!((*tree_mesh).vertices.length, (*tree_mesh).normals.length);
                assert!((*tree_mesh).triangles.length > 0);
                ReleaseMesh(tree_mesh);
                assert!((*tree_mesh).handle.is_null());
            }
        }
    }

    #[test]
    fn terrain_collider_heightmap_dimensions_share_tile_edges() {
        assert_eq!(terrain_collider_heightmap_dimension(33), Some(2049));
        assert_eq!(terrain_collider_heightmap_dimension(65), Some(4097));
        assert_eq!(terrain_collider_heightmap_dimension(129), Some(8193));
        assert_eq!(terrain_collider_heightmap_dimension(32), None);
        assert_eq!(terrain_collider_heightmap_dimension(66), None);
        assert_eq!(terrain_collider_heightmap_dimension(0), None);

        for samples_per_tile in [33, 65, 129] {
            let dimension = terrain_collider_heightmap_dimension(samples_per_tile).unwrap();
            let intervals_per_tile = samples_per_tile - 1;
            for tile in 0..TERRAIN_COLLIDER_TILE_COUNT - 1 {
                let right_edge = tile * intervals_per_tile + intervals_per_tile;
                let next_left_edge = (tile + 1) * intervals_per_tile;
                assert_eq!(right_edge, next_left_edge);
                assert!(right_edge < dimension);
            }
        }
    }
}
