//! Additive borrowed final-mesh surface API; no loading or export allocation.
use super::{Vec3, c_void};

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct NativeTerrainSurface {
    pub position: Vec3,
    pub normal: Vec3,
}
const _: () = assert!(size_of::<NativeTerrainSurface>() == 24);

#[unsafe(no_mangle)]
pub extern "C" fn GetAccurateTerrainSurfaceApiVersion() -> u32 {
    1
}

/// Samples a world-down line converted into normalized island coordinates.
/// The caller retains the island and supplies independent writable output.
/// Misses, null handles and nonfinite inputs return zero with cleared output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn TryGetAccurateTerrainSurface(
    handle: *const c_void,
    origin: Vec3,
    down: Vec3,
    output: *mut NativeTerrainSurface,
) -> u8 {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return 0;
    };
    *output = NativeTerrainSurface::default();
    let Some(island) = (unsafe { super::island_ref(handle) }) else {
        return 0;
    };
    let Some(hit) = island.accurate_terrain_surface(origin, down) else {
        return 0;
    };
    *output = NativeTerrainSurface {
        position: hit.position,
        normal: hit.normal,
    };
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resident_handle_returns_fixed_surface_and_clears_a_later_miss() {
        let island = crate::Island::shared_forest_fixture(crate::ForestOptions::default());
        let mut output = NativeTerrainSurface::default();
        // SAFETY: the borrowed island and independent output outlive both reads.
        unsafe {
            let handle = (&raw const island).cast();
            assert_eq!(
                TryGetAccurateTerrainSurface(
                    handle,
                    Vec3::new(0.32, 0.32, 100.),
                    -Vec3::Z,
                    &raw mut output
                ),
                1
            );
            assert!((output.position.z - 0.02).abs() < 1e-6);
            assert_eq!(output.normal, Vec3::Z);
            assert_eq!(
                TryGetAccurateTerrainSurface(
                    handle,
                    Vec3::new(0.9, 0.9, 100.),
                    -Vec3::Z,
                    &raw mut output
                ),
                0
            );
        }
        assert_eq!(output.position, Vec3::ZERO);
        assert_eq!(output.normal, Vec3::ZERO);
    }

    #[test]
    fn stable_version_and_null_queries_clear_output() {
        assert_eq!(GetAccurateTerrainSurfaceApiVersion(), 1);
        let mut output = NativeTerrainSurface {
            position: Vec3::ONE,
            normal: Vec3::ONE,
        };
        // SAFETY: null handles deliberately miss, output is writable and independent.
        assert_eq!(
            unsafe {
                TryGetAccurateTerrainSurface(
                    std::ptr::null(),
                    Vec3::ZERO,
                    -Vec3::Z,
                    &raw mut output,
                )
            },
            0
        );
        assert_eq!(output.position, Vec3::ZERO);
        assert_eq!(output.normal, Vec3::ZERO);
        // SAFETY: both null pointers are explicitly accepted by the API.
        assert_eq!(
            unsafe {
                TryGetAccurateTerrainSurface(
                    std::ptr::null(),
                    Vec3::ZERO,
                    -Vec3::Z,
                    std::ptr::null_mut(),
                )
            },
            0
        );
    }
}
