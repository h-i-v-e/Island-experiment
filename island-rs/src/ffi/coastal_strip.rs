//! Additive coastal-section API. Existing terrain option layouts stay unchanged.
use super::{MotuFernOptions, MotuForestOptions, MotuOptions, MotuReedOptions, c_void};
use crate::caves::{CaveNetworkOptions, CaveOptions, CaveWalkOptions};
use crate::{CoastalStripOptions, HybridForestOptions};

/// Version of the optional flat, periodically repeating coastal terrain generator.
#[unsafe(no_mangle)]
pub extern "C" fn GetCoastalStripApiVersion() -> u32 {
    1
}

/// Baked coastal geometry revision, independent of the stable option layout.
#[unsafe(no_mangle)]
pub extern "C" fn GetCoastalStripAlgorithmRevision() -> u32 {
    4
}

/// Scalar layout guard, callable before the host passes the new option block.
#[unsafe(no_mangle)]
pub extern "C" fn GetCoastalStripOptionsSize() -> u32 {
    u32::try_from(size_of::<CoastalStripOptions>()).expect("coastal option ABI fits in u32")
}

/// Generates one independently processed square section of a periodic coast.
/// The seed and all settings except `section_index` must match neighboring sections.
/// Null option blocks use defaults. Errors use `GetLastCaveGenerationError` on this
/// thread; a successful handle is owned and must be released with `ReleaseMotu`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateMotuWithCoastalStrip(
    seed: i32,
    options: *const MotuOptions,
    forest: *const MotuForestOptions,
    reeds: *const MotuReedOptions,
    ferns: *const MotuFernOptions,
    caves: *const CaveOptions,
    network: *const CaveNetworkOptions,
    walk: *const CaveWalkOptions,
    hybrid: *const HybridForestOptions,
    strip: *const CoastalStripOptions,
) -> *mut c_void {
    // SAFETY: every non-null block is readable for its documented C layout.
    let strip = unsafe { strip.as_ref().copied() }.unwrap_or_default();
    unsafe {
        super::caves::create_terrain(
            seed,
            options,
            forest,
            reeds,
            ferns,
            caves,
            network,
            walk,
            hybrid,
            Some(strip),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::CStr, ptr};

    #[test]
    fn additive_coastal_api_has_a_stable_layout_and_reports_invalid_sections() {
        assert_eq!(GetCoastalStripApiVersion(), 1);
        assert_eq!(GetCoastalStripAlgorithmRevision(), 4);
        assert_eq!(GetCoastalStripOptionsSize(), 20);
        let invalid = CoastalStripOptions {
            section_count: 0,
            ..CoastalStripOptions::default()
        };
        // SAFETY: the strip block is live and all other blocks deliberately use defaults.
        let handle = unsafe {
            CreateMotuWithCoastalStrip(
                -42,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                &raw const invalid,
            )
        };
        assert!(handle.is_null());
        let error = super::super::caves::GetLastCaveGenerationError();
        assert!(!error.is_null());
        // SAFETY: the error remains valid until another generation call on this thread.
        let message = unsafe { CStr::from_ptr(error) }.to_str().unwrap();
        assert!(message.contains("-42"));
        assert!(message.to_ascii_lowercase().contains("section"));
    }
}
