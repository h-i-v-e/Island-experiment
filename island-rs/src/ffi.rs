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

use crate::{BoundingBox, Island, IslandOptions, Mesh, SurfaceMaps, Vec2, Vec3};

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
    pub noiseMultiplier: f32,
    pub coastalErosionStrength: f32,
    pub beachFormationStrength: f32,
    pub hydraulicErosionStrength: f32,
    pub hydraulicDepositionStrength: f32,
    pub hydraulicDepositionSlopeDegrees: f32,
    pub riverLod2SourceThreshold: f32,
    pub riverLod1SourceThreshold: f32,
    pub riverBroadSourceThreshold: f32,
    pub riverLandSourceThreshold: f32,
    pub riverFinalSourceThreshold: f32,
}

const _: () = assert!(size_of::<MotuOptions>() == size_of::<[f32; 15]>());

impl From<MotuOptions> for IslandOptions {
    fn from(value: MotuOptions) -> Self {
        Self {
            max_height: value.maxZ,
            water_ratio: value.waterRatio,
            slope_multiplier: value.slopeMultiplier,
            coastal_slope_multiplier: value.coastalSlopeMultiplier,
            noise_multiplier: value.noiseMultiplier,
            coastal_erosion_strength: value.coastalErosionStrength,
            beach_formation_strength: value.beachFormationStrength,
            hydraulic_erosion_strength: value.hydraulicErosionStrength,
            hydraulic_deposition_strength: value.hydraulicDepositionStrength,
            hydraulic_deposition_slope_degrees: value.hydraulicDepositionSlopeDegrees,
            river_lod2_source_threshold: value.riverLod2SourceThreshold,
            river_lod1_source_threshold: value.riverLod1SourceThreshold,
            river_broad_source_threshold: value.riverBroadSourceThreshold,
            river_land_source_threshold: value.riverLandSourceThreshold,
            river_final_source_threshold: value.riverFinalSourceThreshold,
            ..Self::default()
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
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportMeshWithUv {
    pub handle: *mut c_void,
    pub vertices: Vector3ExportArray,
    pub normals: Vector3ExportArray,
    pub triangles: TriangleExportArray,
    pub uv: Vector2ExportArray,
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
pub struct ExportDecoration {
    pub trees: Vector3ExportArray,
    pub bushes: Vector3ExportArray,
    pub rocks: Vector3ExportArray,
}

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

fn export_mesh(mesh: Box<Mesh>) -> ExportMesh {
    let handle = Box::into_raw(mesh);
    // SAFETY: handle remains owned by the caller until ReleaseMesh.
    let mesh = unsafe { &*handle };
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
    }
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseMotu(handle: *mut c_void) {
    if !handle.is_null() {
        // SAFETY: handle came from CreateMotu or LoadMotu and is released once.
        drop(unsafe { Box::from_raw(handle.cast::<Island>()) });
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
    let Some(mesh) = island.lod(lod) else {
        return;
    };
    let sliced = island
        .lod(lod + 1)
        .filter(|_| clamp_sides != 0)
        .map_or_else(
            || mesh.sliced(bounds),
            |coarser| {
                mesh.sliced_grid_clamped(bounds, 1, coarser, clamp_sides)
                    .pop()
                    .unwrap_or_default()
            },
        );
    *output = export_mesh(Box::new(sliced));
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
    let Some(mesh) = island.lod(lod) else {
        return;
    };
    let bounds = if area.is_null() {
        BoundingBox::default()
    } else {
        // SAFETY: non-null area must point to a readable ExportArea.
        unsafe { (*area).into() }
    };
    let divisions = usize::try_from(divisions.max(0)).unwrap_or(0);
    let tiles = island
        .lod(lod + 1)
        .filter(|_| clamp_sides != 0)
        .map_or_else(
            || mesh.sliced_grid(bounds, divisions),
            |coarser| mesh.sliced_grid_clamped(bounds, divisions, coarser, clamp_sides),
        );
    let exports: Vec<ExportMesh> = tiles
        .into_iter()
        .map(|tile| export_mesh(Box::new(tile)))
        .collect();
    let owner = Box::new(exports);
    output.data = owner.as_ptr();
    output.length = length_i32(owner.len());
    output.handle = Box::into_raw(owner).cast();
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
        drop(unsafe { Box::from_raw(output.handle.cast::<Mesh>()) });
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
    let mesh = Box::new(island.river_mesh().sliced(bounds));
    let uv_data = mesh.uv.as_ptr();
    let uv_length = length_i32(mesh.uv.len());
    let base = export_mesh(mesh);
    *output = ExportMeshWithUv {
        handle: base.handle,
        vertices: base.vertices,
        normals: base.normals,
        triangles: base.triangles,
        uv: Vector2ExportArray {
            data: uv_data,
            length: uv_length,
        },
    };
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
        rocks: Vector3ExportArray {
            data: decorations.rocks().as_ptr(),
            length: length_i32(decorations.rocks().len()),
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
        output.octants[octant].mesh = export_mesh(Box::new(mesh));
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
    fn ffi_allocations_have_matching_release_functions() {
        let options = MotuOptions {
            maxZ: 0.2,
            waterRatio: 0.6,
            slopeMultiplier: 1.3,
            coastalSlopeMultiplier: 1.0,
            noiseMultiplier: 0.0005,
            coastalErosionStrength: 1.0,
            beachFormationStrength: 1.0,
            hydraulicErosionStrength: 0.25,
            hydraulicDepositionStrength: 1.5,
            hydraulicDepositionSlopeDegrees: 12.0,
            riverLod2SourceThreshold: 0.35,
            riverLod1SourceThreshold: 0.65,
            riverBroadSourceThreshold: 1.0,
            riverLandSourceThreshold: 1.3,
            riverFinalSourceThreshold: 1.6,
        };
        // SAFETY: this test passes valid pointers and releases every returned
        // allocation exactly once through its paired ABI function.
        unsafe {
            let handle = CreateMotu(2018, &raw const options);
            assert!(!handle.is_null());

            let mut mesh = ExportMesh::default();
            CreateMesh(handle, ptr::null(), 2, 0, &raw mut mesh);
            assert!(!mesh.handle.is_null());
            assert!(mesh.triangles.length > 0);
            ReleaseMesh(&raw mut mesh);

            let mut grid = ExportMeshGrid::default();
            CreateMeshGrid(handle, ptr::null(), 2, 8, 0, &raw mut grid);
            assert!(!grid.handle.is_null());
            assert_eq!(grid.length, 64);
            let tiles = std::slice::from_raw_parts(grid.data, grid.length as usize);
            assert!(tiles.iter().all(|tile| tile.triangles.length > 0));
            ReleaseMeshGrid(&raw mut grid);
            assert!(grid.handle.is_null());

            let height_map = CreateHeightMap(handle, 16);
            assert!(!height_map.is_null());
            assert_eq!((*height_map).width, 16);
            ReleaseHeightMap(height_map);

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
}
