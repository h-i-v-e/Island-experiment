//! Compact borrowed-placement / owned-prototype exports for the shared forest renderer.
use super::{ExportMesh, ISLAND_WORLD_METRES, Vec3, Vec4, c_void, export_mesh, island_ref};

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HybridTreeInstance {
    pub anchor: Vec3,
    pub yaw_radians: f32,
    pub scale: f32,
    pub prototype: u32,
}
const _: () = assert!(size_of::<HybridTreeInstance>() == 24);

/// Bake directly into caller-owned R8 pixels; returns bytes written or zero.
/// Capacity must cover resolution squared bytes. No native image allocation or
/// release is needed. Invalid inputs leave caller storage unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CopyHybridForestShadowMask(
    handle: *const c_void,
    resolution: u32,
    output: *mut u8,
    capacity: u32,
) -> u32 {
    let Some(length) = crate::forest::shadows::pixel_count(resolution) else {
        return 0;
    };
    if output.is_null() || (capacity as usize) < length {
        return 0;
    }
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return 0;
    };
    // SAFETY: the caller supplies writable, non-aliasing storage of at least
    // capacity bytes; length is bounded by the validated 2048-square layout.
    let pixels = unsafe { std::slice::from_raw_parts_mut(output, length) };
    if island.write_forest_shadow_mask(resolution, pixels) {
        u32::try_from(length).unwrap_or(0)
    } else {
        0
    }
}

/// Exports fallen logs only, as a fixed 8x8 owner grid for a shared-catalog forest.
/// Release every successful export with `ReleaseMeshGrid`, including empty grids.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateForestLogMeshGrid(
    handle: *const c_void,
    output: *mut super::ExportMeshGrid,
) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    *output = super::ExportMeshGrid::default();
    if (unsafe { island_ref(handle) })
        .and_then(crate::Island::hybrid_forest)
        .is_none()
    {
        return;
    }
    let area = super::ExportArea {
        min: Vec3::ZERO,
        max: Vec3::ONE,
    };
    // SAFETY: the island is live, area is readable, and output is writable.
    unsafe { super::CreateForestWoodMeshGrid(handle, &raw const area, 0, 8, output) };
}

/// Versioned fixed Planet catalog and shell-data contract; restart hosts after upgrading.
#[unsafe(no_mangle)]
pub extern "C" fn GetHybridForestCatalogVersion() -> u32 {
    crate::forest::hybrid::CATALOG_VERSION
}

/// Bake the shared broadleaf (0), pine (1), or palm (2) bark recipe. The returned treeBark
/// maps use DirectX normals and the existing `ReleaseMaterialTextureSet` owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn BakeHybridForestBark(
    species: u32,
    resolution: u32,
    output: *mut super::ExportMaterialTextureSet,
) -> u8 {
    if output.is_null() {
        return 0;
    }
    // SAFETY: output is a writable export struct for this call.
    unsafe { output.write(super::ExportMaterialTextureSet::default()) };
    let Ok(species) = crate::procedural_textures::tree_bark::TreeBarkSpecies::try_from(species)
    else {
        return 0;
    };
    if !(32..=2048).contains(&resolution) {
        return 0;
    }
    let Ok(maps) = crate::procedural_textures::tree_bark::bake_species_bark(
        species,
        resolution,
        None,
        crate::procedural_textures::NormalConvention::DirectX,
    ) else {
        return 0;
    };
    let Ok(texture) = super::ffi_material_texture(&maps) else {
        return 0;
    };
    let owned = Box::new(super::FfiMaterialTextureSet {
        tree_bark: Some(texture),
        ..super::FfiMaterialTextureSet::default()
    });
    let tree_bark = super::export_material_texture(owned.tree_bark.as_ref());
    let handle = Box::into_raw(owned).cast();
    // SAFETY: output is non-null and writable, and owns the exported buffers.
    unsafe {
        output.write(super::ExportMaterialTextureSet {
            handle,
            treeBark: tree_bark,
            ..super::ExportMaterialTextureSet::default()
        });
    }
    1
}

/// Returns the number of shared prototypes (zero for a legacy island).
/// `broadleaf_count` is the variants per species: broadleaf, then pine, then palm.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn GetHybridForestInfo(
    handle: *const c_void,
    tree_count: *mut u32,
    broadleaf_count: *mut u32,
) -> u32 {
    if tree_count.is_null() || broadleaf_count.is_null() {
        return 0;
    }
    // SAFETY: outputs are writable, and handle is a live exclusively accessed island.
    unsafe {
        tree_count.write(0);
        broadleaf_count.write(0);
    }
    let Some((hybrid, placements)) =
        (unsafe { island_ref(handle) }).and_then(|i| i.hybrid_forest())
    else {
        return 0;
    };
    let Ok(count) = u32::try_from(placements.len()) else {
        return 0;
    };
    unsafe {
        tree_count.write(count);
        broadleaf_count.write(u32::try_from(hybrid.broadleaf_count).unwrap_or(0));
    }
    u32::try_from(hybrid.prototypes.len()).unwrap_or(0)
}

/// Copies all placements into caller-owned storage; never returns native borrowed pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CopyHybridForestInstances(
    handle: *const c_void,
    output: *mut HybridTreeInstance,
    capacity: u32,
) -> u32 {
    let Some((_, placements)) = (unsafe { island_ref(handle) }).and_then(|i| i.hybrid_forest())
    else {
        return 0;
    };
    if output.is_null() || (capacity as usize) < placements.len() {
        return 0;
    }
    for (index, tree) in placements.iter().enumerate() {
        // SAFETY: capacity covers the complete array, each output slot is distinct.
        unsafe {
            output.add(index).write(HybridTreeInstance {
                anchor: tree.anchor,
                yaw_radians: tree.yaw_radians,
                scale: tree.scale,
                prototype: u32::from(tree.prototype),
            });
        }
    }
    u32::try_from(placements.len()).unwrap_or(0)
}

/// Export one owned shared mesh. Kind: wood0, foliage0, wood1, foliage1, wood2, foliage2, shells, twig cards.
/// Release every successful export once with `ReleaseMesh`, including empty meshes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateHybridForestPrototypeMesh(
    handle: *const c_void,
    prototype: u32,
    kind: u32,
    output: *mut ExportMesh,
) {
    if output.is_null() {
        return;
    }
    unsafe {
        output.write(ExportMesh::default());
    }
    let Some((hybrid, _)) = (unsafe { island_ref(handle) }).and_then(|i| i.hybrid_forest()) else {
        return;
    };
    let Some(tree) = hybrid.prototypes.get(prototype as usize) else {
        return;
    };
    let species = PrototypeSpecies::from_catalog(prototype as usize, hybrid.broadleaf_count);
    let Some((mut mesh, material)) = prototype_mesh(tree, species, kind) else {
        return;
    };
    // Existing Unity mesh conversion centres normalized XY on 0.5.
    for p in &mut mesh.vertices {
        *p += Vec3::new(0.5, 0.5, 0.0);
    }
    debug_assert!(
        mesh.vertices
            .iter()
            .all(|p| p.z * ISLAND_WORLD_METRES < 1000.0)
    );
    unsafe {
        output.write(export_mesh(mesh, material, Vec::new()));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PrototypeSpecies {
    Broadleaf,
    Pine,
    Palm,
}
impl PrototypeSpecies {
    fn from_catalog(prototype: usize, count: usize) -> Self {
        if prototype >= 2 * count {
            Self::Palm
        } else if prototype >= count {
            Self::Pine
        } else {
            Self::Broadleaf
        }
    }

    fn channel(self) -> f32 {
        match self {
            Self::Broadleaf => 1.0,
            Self::Pine => 0.0,
            Self::Palm => 2.0,
        }
    }
}

// Exports own their mesh buffers because callers release them independently.
fn prototype_mesh(
    tree: &crate::TreeMeshes,
    species: PrototypeSpecies,
    kind: u32,
) -> Option<(crate::Mesh, Vec<Vec4>)> {
    if species == PrototypeSpecies::Palm && matches!(kind, 6 | 7) {
        return Some((crate::Mesh::default(), Vec::new()));
    }
    if kind == 6 {
        let conifer = species == PrototypeSpecies::Pine;
        let shells = crate::tree_shells(
            if conifer {
                &tree.lod0_foliage
            } else {
                &tree.lod1_foliage
            },
            conifer,
        );
        let material = shells
            .layers
            .iter()
            .map(|&layer| Vec4::new(layer, 1.0, species.channel(), 0.0))
            .collect();
        return Some((shells.mesh, material));
    }
    let source = match kind {
        0 => &tree.lod0_wood,
        1 => &tree.lod0_foliage,
        2 => &tree.lod1_wood,
        3 => &tree.lod1_foliage,
        4 => &tree.lod2_wood,
        5 => &tree.lod2_foliage,
        7 => &tree.lod0_twigs,
        _ => return None,
    };
    let stage = if species == PrototypeSpecies::Palm {
        match kind {
            0 | 1 => 0.0,
            2 | 3 => 1.0,
            _ => 2.0,
        }
    } else {
        1.0
    };
    Some((
        source.clone(),
        vec![Vec4::new(0.0, 1.0, species.channel(), stage); source.vertices.len()],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_mask_ffi_matches_the_shared_rust_bake_without_touching_padding() {
        let island = crate::Island::shared_forest_fixture(crate::ForestOptions {
            noise_threshold: 0.0,
            prototype_count: 1,
            hybrid: Some(crate::HybridForestOptions {
                latitude_degrees: 0.0,
                altitude_cooling: 0.0,
                variants_per_species: 1,
            }),
            ..crate::ForestOptions::default()
        });
        let expected = island.forest_shadow_mask(256).unwrap();
        assert_eq!(expected.resolution(), 256);
        assert!(expected.pixels().contains(&255));
        let mut pixels = vec![17; expected.pixels().len() + 32];
        // SAFETY: the island remains live, output covers capacity writable bytes.
        let written = unsafe {
            CopyHybridForestShadowMask(
                std::ptr::from_ref(&island).cast(),
                256,
                pixels.as_mut_ptr(),
                u32::try_from(pixels.len()).unwrap(),
            )
        };
        assert_eq!(written, 256 * 256);
        assert_eq!(&pixels[..expected.pixels().len()], expected.pixels());
        assert!(pixels[expected.pixels().len()..].iter().all(|&p| p == 17));
        let allocation = expected.pixels().as_ptr();
        let transferred = expected.into_pixels();
        assert_eq!(
            transferred.as_ptr(),
            allocation,
            "Rust renderers can take ownership without copying the image"
        );
    }

    #[test]
    fn shadow_mask_rejects_invalid_inputs_without_modifying_caller_storage() {
        let island = crate::Island::shared_forest_fixture(crate::ForestOptions {
            noise_threshold: 1.0,
            prototype_count: 1,
            ..crate::ForestOptions::default()
        });
        let mut pixels = [17; 4];
        // SAFETY: nulls are supported, and the output is valid for its capacity.
        unsafe {
            for resolution in [0, 3, 4096, u32::MAX] {
                assert_eq!(
                    CopyHybridForestShadowMask(
                        std::ptr::from_ref(&island).cast(),
                        resolution,
                        pixels.as_mut_ptr(),
                        4
                    ),
                    0
                );
            }
            assert_eq!(
                CopyHybridForestShadowMask(std::ptr::null(), 1, pixels.as_mut_ptr(), 4),
                0
            );
            assert_eq!(
                CopyHybridForestShadowMask(
                    std::ptr::from_ref(&island).cast(),
                    256,
                    pixels.as_mut_ptr(),
                    4
                ),
                0
            );
            assert_eq!(
                CopyHybridForestShadowMask(
                    std::ptr::from_ref(&island).cast(),
                    1,
                    std::ptr::null_mut(),
                    4
                ),
                0
            );
            assert_eq!(
                CopyHybridForestShadowMask(
                    std::ptr::from_ref(&island).cast(),
                    1,
                    pixels.as_mut_ptr(),
                    4
                ),
                0
            );
        }
        assert_eq!(pixels, [17; 4]);
        assert!(island.forest_shadow_mask(256).is_none());
        assert!(!island.write_forest_shadow_mask(2, &mut pixels));
        assert_eq!(pixels, [17; 4]);
    }

    #[test]
    fn empty_shared_forests_clear_masks_and_reject_wrong_buffer_lengths() {
        let island = crate::Island::shared_forest_fixture(crate::ForestOptions {
            noise_threshold: 1.0,
            prototype_count: 1,
            hybrid: Some(crate::HybridForestOptions {
                latitude_degrees: 0.0,
                altitude_cooling: 0.0,
                variants_per_species: 1,
            }),
            ..crate::ForestOptions::default()
        });
        assert_eq!(island.forest_shadow_mask(1).unwrap().pixels(), &[0]);
        let mut pixels = [17; 4];
        assert!(!island.write_forest_shadow_mask(1, &mut pixels));
        assert!(!island.write_forest_shadow_mask(3, &mut pixels));
        assert_eq!(pixels, [17; 4]);
        assert!(island.write_forest_shadow_mask(2, &mut pixels));
        assert_eq!(pixels, [0; 4]);
    }

    #[test]
    fn fallen_log_grid_null_handles_leave_default_exports() {
        let mut output = super::super::ExportMeshGrid {
            length: 23,
            ..super::super::ExportMeshGrid::default()
        };
        // SAFETY: output is writable; both null arguments are supported.
        unsafe {
            CreateForestLogMeshGrid(std::ptr::null(), &raw mut output);
            assert!(output.handle.is_null());
            assert!(output.data.is_null());
            assert_eq!(output.length, 0);
            CreateForestLogMeshGrid(std::ptr::null(), std::ptr::null_mut());
        }
    }

    #[test]
    fn fallen_log_grid_contains_authored_logs_without_standing_trunks_or_canopies() {
        let island = crate::Island::shared_forest_fixture(crate::ForestOptions {
            noise_threshold: 0.0,
            prototype_count: 1,
            fallen_log_density: 1.0,
            hybrid: Some(crate::forest::HybridForestOptions {
                latitude_degrees: 0.0,
                altitude_cooling: 0.0,
                variants_per_species: 1,
            }),
            ..crate::ForestOptions::default()
        });
        let forest = island.forest_meshes();
        assert!(
            !forest.logs.is_empty(),
            "fixture must retain authored fallen logs"
        );
        assert!(!forest.trees.is_empty());
        assert!(
            !forest.lod2_wood.vertices.is_empty(),
            "fixture contains standing collision proxies"
        );
        assert!(
            !forest.lod1_foliage.vertices.is_empty(),
            "fixture contains canopy occlusion proxies"
        );
        let expected_vertices: u32 = forest
            .logs
            .iter()
            .map(|log| log.ranges[0].vertex_count)
            .sum();
        let expected_indices: u32 = forest
            .logs
            .iter()
            .map(|log| log.ranges[0].triangle_count)
            .sum();
        let mut output = super::super::ExportMeshGrid::default();
        // SAFETY: the borrowed island and writable export remain live through release.
        unsafe {
            CreateForestLogMeshGrid(std::ptr::from_ref(&island).cast(), &raw mut output);
            assert!(!output.handle.is_null());
            assert_eq!(output.length, 64);
            let tiles = std::slice::from_raw_parts(output.data, 64);
            assert_eq!(
                tiles
                    .iter()
                    .map(|tile| u32::try_from(tile.vertices.length).unwrap())
                    .sum::<u32>(),
                expected_vertices
            );
            assert_eq!(
                tiles
                    .iter()
                    .map(|tile| u32::try_from(tile.triangles.length).unwrap())
                    .sum::<u32>(),
                expected_indices
            );
            for tile in tiles {
                let material = std::slice::from_raw_parts(
                    tile.material.data,
                    usize::try_from(tile.material.length).unwrap(),
                );
                assert!(
                    material.iter().all(|value| value.w < 0.3),
                    "only stationary fallen wood is exported"
                );
                assert_eq!(tile.material.length, tile.vertices.length);
                assert_eq!(tile.environment.length, tile.vertices.length);
            }
            super::super::ReleaseMeshGrid(&raw mut output);
            assert!(output.handle.is_null());
            assert!(output.data.is_null());
        }
    }

    #[test]
    fn fallen_log_grid_rejects_legacy_forests() {
        let island = crate::Island::shared_forest_fixture(crate::ForestOptions {
            noise_threshold: 1.0,
            prototype_count: 1,
            fallen_log_density: 0.0,
            ..crate::ForestOptions::default()
        });
        assert!(island.hybrid_forest().is_none());
        let mut output = super::super::ExportMeshGrid::default();
        // SAFETY: this legacy island is live and output is writable.
        unsafe { CreateForestLogMeshGrid(std::ptr::from_ref(&island).cast(), &raw mut output) };
        assert!(output.handle.is_null());
        assert!(output.data.is_null());
        assert_eq!(output.length, 0);
    }

    #[test]
    fn catalog_tags_three_species_and_palm_detail_exports_are_owned_empty_meshes() {
        assert_eq!(GetHybridForestCatalogVersion(), 6);
        assert_eq!(size_of::<HybridTreeInstance>(), 24);
        for (index, tree, expected) in [
            (0, crate::generate_tree(2018), 1.0),
            (4, crate::generate_conifer(0), 0.0),
            (8, crate::generate_palm(0), 2.0),
        ] {
            let species = PrototypeSpecies::from_catalog(index, 4);
            for kind in 0..=7 {
                let (mesh, material) = prototype_mesh(&tree, species, kind).unwrap();
                assert_eq!(mesh.vertices.len(), material.len());
                assert!(material.iter().all(|m| m.z == expected));
                if species == PrototypeSpecies::Palm && kind < 6 {
                    let stage = f32::from(u8::try_from(kind / 2).unwrap());
                    assert!(material.iter().all(|m| m.w == stage));
                }
                if species == PrototypeSpecies::Palm && matches!(kind, 6 | 7) {
                    assert!(mesh.vertices.is_empty());
                    assert!(mesh.triangles.is_empty());
                }
                let mut output = export_mesh(mesh, material, Vec::new());
                assert!(!output.handle.is_null(), "even empty exports need an owner");
                // SAFETY: this output owns the exported allocation and is released once.
                unsafe { super::super::ReleaseMesh(&raw mut output) };
                assert!(output.handle.is_null());
            }
        }
    }

    #[test]
    fn bark_exports_are_owned_and_invalid_requests_leave_empty_outputs() {
        for species in [0, 1, 2] {
            let mut output = super::super::ExportMaterialTextureSet::default();
            // SAFETY: the local output remains writable and is released once.
            unsafe {
                assert_eq!(BakeHybridForestBark(species, 32, &raw mut output), 1);
                assert!(!output.handle.is_null());
                assert_eq!(output.treeBark.width, 32);
                assert_eq!(output.treeBark.height, 32);
                assert!(!output.treeBark.normalRgb.data.is_null());
                super::super::ReleaseMaterialTextureSet(&raw mut output);
                assert!(output.handle.is_null());
            }
        }
        for (species, resolution) in [(3, 32), (0, 0), (1, 4096)] {
            let mut output = super::super::ExportMaterialTextureSet::default();
            // SAFETY: this output is a live writable export struct.
            assert_eq!(
                unsafe { BakeHybridForestBark(species, resolution, &raw mut output) },
                0
            );
            assert!(output.handle.is_null());
            assert!(output.treeBark.albedoRgb.data.is_null());
        }
    }
}
