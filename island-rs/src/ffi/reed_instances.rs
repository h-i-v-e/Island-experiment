//! Owned compact reed placements. Legacy card meshes are expanded only on demand.
use super::{c_void, island_ref};
use crate::reeds::REED_TILE_RESOLUTION;

pub use crate::reeds::ReedInstanceData;

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ExportReedInstances {
    pub handle: *mut c_void,
    pub data: *const ReedInstanceData,
    pub length: u32,
    pub tile_offsets: *const u32,
    pub tile_offsets_length: u32,
    pub tile_resolution: u32,
}

struct OwnedReedInstances {
    instances: Vec<ReedInstanceData>,
    tile_offsets: Vec<u32>,
}

fn export_instances(
    instances: &[ReedInstanceData],
    tile_offsets: &[u32],
) -> Option<ExportReedInstances> {
    let length = u32::try_from(instances.len()).ok()?;
    let tile_offsets_length = u32::try_from(tile_offsets.len()).ok()?;
    let tile_resolution = u32::try_from(REED_TILE_RESOLUTION).ok()?;
    let owned = Box::new(OwnedReedInstances {
        instances: instances.to_vec(),
        tile_offsets: tile_offsets.to_vec(),
    });
    let data = owned.instances.as_ptr();
    let tile_offsets = owned.tile_offsets.as_ptr();
    Some(ExportReedInstances {
        handle: Box::into_raw(owned).cast(),
        data,
        length,
        tile_offsets,
        tile_offsets_length,
        tile_resolution,
    })
}

/// Copies compact placements independently of the island lifetime, without expanding meshes.
/// Tile `i` occupies `[tile_offsets[i], tile_offsets[i + 1])` in `data`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateReedInstances(
    handle: *const c_void,
    output: *mut ExportReedInstances,
) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    *output = ExportReedInstances::default();
    let Some(island) = (unsafe { island_ref(handle) }) else {
        return;
    };
    let reeds = island.reed_instances();
    if let Some(export) = export_instances(reeds.instances(), reeds.tile_offsets()) {
        *output = export;
    }
}

/// Releases the owned placement arrays and clears every exported field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReleaseReedInstances(output: *mut ExportReedInstances) {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return;
    };
    if !output.handle.is_null() {
        // SAFETY: this handle was returned by CreateReedInstances and is released once.
        drop(unsafe { Box::from_raw(output.handle.cast::<OwnedReedInstances>()) });
    }
    *output = ExportReedInstances::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    #[test]
    fn null_island_clears_output_and_release_accepts_empty_outputs() {
        let mut output = ExportReedInstances {
            length: 123,
            tile_resolution: 64,
            ..ExportReedInstances::default()
        };
        // SAFETY: output points to a live writable export; null handles are supported.
        unsafe {
            CreateReedInstances(ptr::null(), &raw mut output);
            assert!(output.handle.is_null());
            assert_eq!(output.length, 0);
            assert_eq!(output.tile_resolution, 0);
            ReleaseReedInstances(&raw mut output);
            ReleaseReedInstances(ptr::null_mut());
            CreateReedInstances(ptr::null(), ptr::null_mut());
        }
    }

    #[test]
    fn empty_grid_export_owns_offsets_and_release_clears_all_fields() {
        let reeds = crate::reeds::ReedMeshes::default();
        let mut output = export_instances(reeds.instances(), reeds.tile_offsets()).unwrap();
        assert!(!output.handle.is_null());
        assert_eq!(output.tile_resolution, 64);
        assert_eq!(output.tile_offsets_length, 4097);
        assert_eq!(output.length, 0);
        // SAFETY: the export owns 4097 live offset values until it is released.
        unsafe {
            assert!(
                std::slice::from_raw_parts(output.tile_offsets, 4097)
                    .iter()
                    .all(|&n| n == 0)
            );
            ReleaseReedInstances(&raw mut output);
            assert!(output.handle.is_null());
            assert!(output.data.is_null());
            assert!(output.tile_offsets.is_null());
            assert_eq!(output.tile_offsets_length, 0);
            assert_eq!(output.tile_resolution, 0);
            ReleaseReedInstances(&raw mut output);
        }
    }

    #[test]
    fn placement_export_copies_every_attribute_and_outlives_source_buffers() {
        let expected = ReedInstanceData {
            root: crate::Vec3::new(0.1, 0.2, 0.03),
            ground_normal: crate::Vec3::new(-0.4, 0.2, 1.0).normalize(),
            height_metres: 1.8,
            width_metres: 0.7,
            variant: 1.0,
            tint: 0.3,
            stiffness: 0.8,
            phase: 0.4,
        };
        let mut output = {
            let instances = vec![expected];
            let mut offsets = vec![0; 4097];
            offsets[4..].fill(1);
            export_instances(&instances, &offsets).unwrap()
        };
        assert_eq!(output.length, 1);
        // SAFETY: both arrays remain owned by output after the source vectors were dropped.
        unsafe {
            assert_eq!(*output.data, expected);
            let offsets = std::slice::from_raw_parts(output.tile_offsets, 4097);
            assert_eq!(&offsets[..5], &[0, 0, 0, 0, 1]);
            assert_eq!(offsets[4096], 1);
            ReleaseReedInstances(&raw mut output);
        }
    }
}
