//! Additive Unity exports for native LOD1/2 surface atlases.
use super::{ExportArea, ExportMeshGrid, ExportSurfaceMaps, c_void};

/// Copies the cached unwrap failure into caller-owned UTF-8 storage. Returns
/// the full byte length, excluding the trailing nul; no native allocation crosses
/// the boundary. A successful atlas has an empty error message.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn GetTerrainSurfaceAtlasError(
    handle: *const c_void,
    lod: i32,
    output: *mut u8,
    capacity: i32,
) -> i32 {
    #[cfg(feature = "surface-atlas")]
    let message = match unsafe { super::island_ref(handle) } {
        Some(island) => usize::try_from(lod).map_or("invalid terrain LOD", |lod| {
            island.cached_surface_atlas(lod).err().unwrap_or("")
        }),
        None => "invalid island handle",
    };
    #[cfg(not(feature = "surface-atlas"))]
    let message = {
        let _ = (handle, lod);
        "native surface-atlas feature is disabled"
    };
    if let Ok(capacity) = usize::try_from(capacity)
        && capacity > 0
        && !output.is_null()
    {
        let count = message.len().min(capacity - 1);
        // SAFETY: caller supplies capacity writable bytes; source is an
        // independent borrowed error string retained by the island cache.
        unsafe {
            std::ptr::copy_nonoverlapping(message.as_ptr(), output, count);
            *output.add(count) = 0;
        }
    }
    super::length_i32(message.len())
}

/// Exports cached LOD1=2048px or LOD2=1024px maps. The owner retains the atlas
/// independently of the island; release it only with `ReleaseTerrainSurfaceAtlas`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateTerrainSurfaceAtlas(
    handle: *const c_void,
    lod: i32,
    dimension: i32,
    output: *mut ExportSurfaceMaps,
) -> u8 {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return 0;
    };
    *output = ExportSurfaceMaps::default();
    #[cfg(feature = "surface-atlas")]
    {
        let Some(island) = (unsafe { super::island_ref(handle) }) else {
            return 0;
        };
        let (Ok(lod), Ok(dimension)) = (usize::try_from(lod), u32::try_from(dimension)) else {
            return 0;
        };
        if crate::Island::surface_atlas_dimension(lod) != Some(dimension) {
            return 0;
        }
        let Ok(atlas) = island.cached_surface_atlas(lod) else {
            return 0;
        };
        let owner = Box::new(std::sync::Arc::clone(atlas));
        let maps = &owner.maps;
        *output = ExportSurfaceMaps {
            handle: std::ptr::null_mut(),
            width: super::length_i32(maps.width() as usize),
            height: super::length_i32(maps.height() as usize),
            normalRgb: maps.normal_rgb().as_ptr(),
            occlusion: maps.occlusion().as_ptr(),
        };
        output.handle = Box::into_raw(owner).cast();
        1
    }
    #[cfg(not(feature = "surface-atlas"))]
    {
        let _ = (handle, lod, dimension);
        0
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseTerrainSurfaceAtlas(output: *mut ExportSurfaceMaps) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    #[cfg(feature = "surface-atlas")]
    if !output.handle.is_null() {
        // SAFETY: this owner was allocated by CreateTerrainSurfaceAtlas and
        // remains live until this paired release, even after island disposal.
        drop(unsafe { Box::from_raw(output.handle.cast::<std::sync::Arc<crate::SurfaceAtlas>>()) });
    }
    *output = ExportSurfaceMaps::default();
}

/// Exports owned terrain tiles using the exact cached atlas layout. Invalid
/// LODs, bounds, clamp flags or division counts return an empty output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateTerrainAtlasMeshGrid(
    handle: *const c_void,
    area: *const ExportArea,
    lod: i32,
    divisions: i32,
    clamp_sides: u8,
    output: *mut ExportMeshGrid,
) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    *output = ExportMeshGrid::default();
    #[cfg(feature = "surface-atlas")]
    {
        let Some(island) = (unsafe { super::island_ref(handle) }) else {
            return;
        };
        let (Ok(lod), Ok(divisions)) = (usize::try_from(lod), usize::try_from(divisions)) else {
            return;
        };
        if !(1..=64).contains(&divisions) || clamp_sides & !15 != 0 {
            return;
        }
        let bounds = if area.is_null() {
            crate::BoundingBox::default()
        } else {
            // SAFETY: a non-null area points to one readable ExportArea.
            unsafe { (*area).into() }
        };
        let extent = bounds.max.truncate() - bounds.min.truncate();
        if !bounds.min.is_finite()
            || !bounds.max.is_finite()
            || !extent.is_finite()
            || extent.min_element() <= 0.0
        {
            return;
        }
        let Ok(tiles) = island.terrain_atlas_mesh_grid(lod, bounds, divisions, clamp_sides) else {
            return;
        };
        let exports: Vec<_> = tiles
            .into_iter()
            .map(|mut tile| {
                // Native material/environment queries historically interpret .uv
                // as islandXY. Temporarily remove atlas coordinates so the same
                // authoritative samplers fall back to each vertex's islandXY.
                let atlas_uv = std::mem::take(&mut tile.uv);
                let material = island.material_values_for(&tile);
                let environment = island.environment_values_for(&tile);
                tile.uv = atlas_uv;
                super::export_mesh(tile, material, environment)
            })
            .collect();
        let owner = Box::new(exports);
        *output = ExportMeshGrid {
            handle: std::ptr::null_mut(),
            data: owner.as_ptr(),
            length: super::length_i32(owner.len()),
        };
        output.handle = Box::into_raw(owner).cast();
    }
    #[cfg(not(feature = "surface-atlas"))]
    {
        let _ = (handle, area, lod, divisions, clamp_sides);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    #[test]
    fn terrain_atlas_invalid_inputs_zero_outputs() {
        unsafe {
            let length = GetTerrainSurfaceAtlasError(ptr::null(), 1, ptr::null_mut(), 0);
            assert!(length > 0);
            let mut message = [255; 8];
            assert_eq!(
                GetTerrainSurfaceAtlasError(ptr::null(), 1, message.as_mut_ptr(), 8),
                length
            );
            assert_eq!(message[7], 0);
            GetTerrainSurfaceAtlasError(ptr::null(), 1, message.as_mut_ptr(), -1);
            let mut maps = ExportSurfaceMaps {
                handle: ptr::dangling_mut(),
                width: 1,
                height: 1,
                normalRgb: ptr::dangling(),
                occlusion: ptr::dangling(),
            };
            assert_eq!(
                CreateTerrainSurfaceAtlas(ptr::null(), 2, 1024, &raw mut maps),
                0
            );
            assert!(maps.handle.is_null() && maps.normalRgb.is_null() && maps.occlusion.is_null());
            assert_eq!(maps.width, 0);
            assert_eq!(
                CreateTerrainSurfaceAtlas(ptr::null(), 2, 1024, ptr::null_mut()),
                0
            );
            ReleaseTerrainSurfaceAtlas(&raw mut maps);
            ReleaseTerrainSurfaceAtlas(&raw mut maps);
            ReleaseTerrainSurfaceAtlas(ptr::null_mut());
            let mut grid = ExportMeshGrid {
                handle: ptr::dangling_mut(),
                data: ptr::dangling(),
                length: 1,
            };
            CreateTerrainAtlasMeshGrid(ptr::null(), ptr::null(), 2, 2, 0, &raw mut grid);
            assert!(grid.handle.is_null() && grid.data.is_null() && grid.length == 0);
            CreateTerrainAtlasMeshGrid(ptr::null(), ptr::null(), 2, 2, 0, ptr::null_mut());
        }
        assert_eq!(
            super::super::abi::GetMotuTerrainSurfaceAtlasVersion(),
            u32::from(cfg!(feature = "surface-atlas"))
        );
    }

    #[test]
    #[cfg(feature = "surface-atlas")]
    fn terrain_atlas_all_subfloor_island_exports_neutral_maps_and_empty_grids() {
        let island = crate::Island::generate_with_forest(
            73,
            crate::IslandOptions {
                terrain_size: 16,
                water_ratio: 0.95,
                land_mass_offset: -2.0,
                ..crate::IslandOptions::default()
            },
            crate::ForestOptions {
                noise_threshold: 1.0,
                prototype_count: 1,
                ..crate::ForestOptions::default()
            },
        )
        .unwrap();
        let floor = -crate::sea_mask::SEA_MASK_DEPTH_METRES / crate::ISLAND_WORLD_METRES;
        assert!(
            island
                .terrain()
                .vertices()
                .iter()
                .all(|point| point.z < floor)
        );
        let handle = Box::into_raw(Box::new(island)).cast::<c_void>();
        unsafe {
            for (lod, dimension) in [(1, 2048), (2, 1024)] {
                let mut maps = ExportSurfaceMaps::default();
                assert_eq!(
                    CreateTerrainSurfaceAtlas(handle, lod, dimension, &raw mut maps),
                    1
                );
                assert_eq!(maps.width, dimension);
                assert_eq!(maps.height, 1);
                let normals = std::slice::from_raw_parts(maps.normalRgb, dimension as usize * 3);
                assert!(
                    normals
                        .chunks_exact(3)
                        .all(|pixel| pixel == [127, 127, 255])
                );
                assert!(
                    std::slice::from_raw_parts(maps.occlusion, dimension as usize)
                        .iter()
                        .all(|&ao| ao == 255)
                );
                for clamp_sides in [0, 15] {
                    let mut grid = ExportMeshGrid::default();
                    CreateTerrainAtlasMeshGrid(
                        handle,
                        ptr::null(),
                        lod,
                        2,
                        clamp_sides,
                        &raw mut grid,
                    );
                    assert_eq!(grid.length, 4);
                    assert!(!grid.handle.is_null());
                    let tiles = std::slice::from_raw_parts(grid.data, 4);
                    assert!(tiles.iter().all(|tile| tile.vertices.length == 0
                        && tile.normals.length == 0
                        && tile.triangles.length == 0
                        && tile.uv.length == 0
                        && tile.material.length == 0
                        && tile.environment.length == 0));
                    super::super::ReleaseMeshGrid(&raw mut grid);
                }
                ReleaseTerrainSurfaceAtlas(&raw mut maps);
                assert!(maps.handle.is_null());
            }
            super::super::ReleaseMotu(handle);
        }
    }

    #[test]
    #[cfg(feature = "surface-atlas")]
    #[allow(clippy::too_many_lines)] // One snapshot fixture checks paired ownership and export contracts.
    fn terrain_atlas_snapshot_loaded_handle_uses_shared_maps_and_owned_grids() {
        use crate::{ForestOptions, Island, IslandOptions};
        use std::ffi::CString;
        let source = Island::generate_with_forest(
            73,
            IslandOptions {
                terrain_size: 16,
                ..IslandOptions::default()
            },
            ForestOptions {
                noise_threshold: 1.0,
                prototype_count: 1,
                ..ForestOptions::default()
            },
        )
        .unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "motu-terrain-atlas-{}-{unique}.motu",
            std::process::id()
        ));
        source.save(&path).unwrap();
        let original_snapshot = std::fs::read(&path).unwrap();
        let c_path = CString::new(path.to_str().unwrap()).unwrap();
        unsafe {
            let mut status = -1;
            let handle = super::super::LoadMotuSnapshot(c_path.as_ptr(), &raw mut status);
            assert_eq!(status, 0);
            assert!(!handle.is_null());
            let mut first = ExportSurfaceMaps::default();
            let mut second = ExportSurfaceMaps::default();
            assert_eq!(
                CreateTerrainSurfaceAtlas(handle, 0, 1024, &raw mut first),
                0
            );
            assert_eq!(
                CreateTerrainSurfaceAtlas(handle, 1, 1024, &raw mut first),
                0
            );
            assert_eq!(CreateTerrainSurfaceAtlas(handle, 2, -1, &raw mut first), 0);
            assert_eq!(
                CreateTerrainSurfaceAtlas(handle, 2, 1024, &raw mut first),
                1
            );
            assert_eq!(
                CreateTerrainSurfaceAtlas(handle, 2, 1024, &raw mut second),
                1
            );
            assert_eq!(first.width, 1024);
            assert!((1..=1024).contains(&first.height));
            assert_eq!(first.normalRgb, second.normalRgb);
            assert_eq!(first.occlusion, second.occlusion);
            assert_ne!(first.handle, second.handle);
            let island = super::super::island_ref(handle).unwrap();
            assert_eq!(island, &source); // Derived caches do not affect island equality.
            island.save(&path).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), original_snapshot);
            let mut grid = ExportMeshGrid::default();
            for (lod, divisions, flags) in [
                (0, 2, 0),
                (2, 0, 0),
                (2, 65, 0),
                (2, i32::MAX, 0),
                (2, 2, 128),
            ] {
                CreateTerrainAtlasMeshGrid(
                    handle,
                    ptr::null(),
                    lod,
                    divisions,
                    flags,
                    &raw mut grid,
                );
                assert!(grid.handle.is_null() && grid.data.is_null() && grid.length == 0);
            }
            let area = ExportArea {
                min: crate::Vec3::splat(f32::NAN),
                max: crate::Vec3::ONE,
            };
            CreateTerrainAtlasMeshGrid(handle, &raw const area, 2, 2, 0, &raw mut grid);
            assert!(grid.handle.is_null());
            CreateTerrainAtlasMeshGrid(handle, ptr::null(), 2, 2, 0, &raw mut grid);
            assert_eq!(grid.length, 4);
            let tiles = std::slice::from_raw_parts(grid.data, 4);
            assert!(tiles.iter().any(|tile| tile.triangles.length > 0));
            for tile in tiles {
                let owner = &*tile.handle.cast::<super::super::ExportedMesh>();
                let mut physical_mesh = owner.mesh.clone();
                physical_mesh.uv.clear();
                assert_eq!(owner.material, island.material_values_for(&physical_mesh));
                assert_eq!(
                    owner.environment,
                    island.environment_values_for(&physical_mesh)
                );
                assert_eq!(owner.mesh.uv.len(), owner.mesh.vertices.len());
                assert!(owner.mesh.uv.iter().all(|uv| uv.is_finite()
                    && uv.min_element() >= 0.0
                    && uv.max_element() <= 1.0));
                assert!(owner.mesh.vertices.iter().all(|point| point.z
                    >= -crate::sea_mask::SEA_MASK_DEPTH_METRES / crate::ISLAND_WORLD_METRES));
            }
            let normal_before_release = *first.normalRgb;
            super::super::ReleaseMotu(handle);
            assert_eq!(*first.normalRgb, normal_before_release);
            ReleaseTerrainSurfaceAtlas(&raw mut first);
            assert_eq!(*second.normalRgb, normal_before_release);
            assert!(tiles.iter().any(|tile| tile.triangles.length > 0
                && *tile.triangles.data >= 0
                && *tile.triangles.data < tile.vertices.length));
            ReleaseTerrainSurfaceAtlas(&raw mut second);
            super::super::ReleaseMeshGrid(&raw mut grid);
            assert!(first.handle.is_null() && second.handle.is_null() && grid.handle.is_null());
        }
        std::fs::remove_file(path).unwrap();
    }
}
