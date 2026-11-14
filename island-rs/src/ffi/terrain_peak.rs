use super::{Vec3, c_void};

#[unsafe(no_mangle)]
pub extern "C" fn GetTerrainPeakApiVersion() -> u32 {
    1
}

/// Borrow the resident island; the independent output is cleared on every miss.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn TryGetTerrainPeak(
    handle: *const c_void,
    up: Vec3,
    output: *mut Vec3,
) -> u8 {
    let Some(output) = (unsafe { output.as_mut() }) else {
        return 0;
    };
    *output = Vec3::ZERO;
    let Some(island) = (unsafe { super::island_ref(handle) }) else {
        return 0;
    };
    let Some(peak) = island.terrain_peak(up) else {
        return 0;
    };
    *output = peak;
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_queries_clear_output_and_resident_queries_match_the_mesh() {
        let mut peak = Vec3::ONE;
        // SAFETY: null island handles deliberately miss and output is writable.
        assert_eq!(
            unsafe { TryGetTerrainPeak(std::ptr::null(), Vec3::Z, &raw mut peak) },
            0
        );
        assert_eq!(peak, Vec3::ZERO);
        // SAFETY: null output is explicitly accepted.
        assert_eq!(
            unsafe { TryGetTerrainPeak(std::ptr::null(), Vec3::Z, std::ptr::null_mut()) },
            0
        );
        let island = crate::Island::shared_forest_fixture(crate::ForestOptions::default());
        // SAFETY: the borrowed island and independent output outlive this call.
        assert_eq!(
            unsafe { TryGetTerrainPeak((&raw const island).cast(), Vec3::Z, &raw mut peak) },
            1
        );
        assert_eq!(Some(peak), island.terrain_peak(Vec3::Z));
        assert_eq!(GetTerrainPeakApiVersion(), 1);
    }
}
