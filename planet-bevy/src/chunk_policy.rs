//! Screen-space detail selection. Callers retain coarse meshes and schedule I/O;
//! this module borrows error arrays and owns no residency or render state.

/// Select the coarsest level whose cumulative error stays below the pixel budget.
/// `projection_scale` is `viewport_height / (2 * tan(vertical_fov / 2))`. Distance
/// is the nearest distance to the chunk bounds, rather than its centre.
/// A 20% deadband keeps a stationary boundary from switching every frame.
pub fn select_lod(
    errors: &[f64],
    distance: f64,
    projection_scale: f64,
    target_pixels: f64,
    current: usize,
) -> usize {
    if errors.is_empty() {
        return 0;
    }
    let current = current.min(errors.len() - 1);
    if !distance.is_finite()
        || distance <= 0.0
        || !projection_scale.is_finite()
        || projection_scale <= 0.0
        || !target_pixels.is_finite()
        || target_pixels <= 0.0
    {
        return 0;
    }
    let pixels = |error: f64| {
        if error.is_finite() && error >= 0.0 {
            error * projection_scale / distance
        } else {
            f64::INFINITY
        }
    };
    let mut desired = current;
    while desired > 0 && pixels(errors[desired]) > target_pixels * 1.2 {
        desired -= 1;
    }
    while desired + 1 < errors.len() && pixels(errors[desired + 1]) <= target_pixels * 0.8 {
        desired += 1;
    }
    desired
}

/// Preload one finer level near the camera before it becomes necessary.
#[must_use]
pub const fn prefetch_lod(desired: usize, near: bool) -> usize {
    if near {
        desired.saturating_sub(1)
    } else {
        desired
    }
}

/// Coarse fallback geometry stays resident. Fine geometry can be released after
/// its grace period; the caller applies its memory budget to eligible candidates.
#[must_use]
pub fn should_evict(
    last_needed_seconds: f64,
    now_seconds: f64,
    grace_seconds: f64,
    is_coarsest: bool,
) -> bool {
    !is_coarsest
        && last_needed_seconds.is_finite()
        && now_seconds.is_finite()
        && grace_seconds.is_finite()
        && grace_seconds >= 0.0
        && now_seconds - last_needed_seconds >= grace_seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approach_refines_and_retreat_coarsens_with_a_stable_deadband() {
        let errors = [0.0, 0.5, 2.0, 8.0];
        assert_eq!(select_lod(&errors, 1_000.0, 1_000.0, 2.0, 3), 2);
        for distance in [900.0, 1_000.0, 1_100.0] {
            assert_eq!(select_lod(&errors, distance, 1_000.0, 2.0, 2), 2);
        }
        assert_eq!(select_lod(&errors, 700.0, 1_000.0, 2.0, 2), 1);
        assert_eq!(select_lod(&errors, 1_300.0, 1_000.0, 2.0, 1), 2);
        assert_eq!(select_lod(&errors, 5_100.0, 1_000.0, 2.0, 2), 3);
        assert_eq!(select_lod(&errors, 50.0, 1_000.0, 2.0, 3), 0);
    }

    #[test]
    fn invalid_camera_or_error_values_preserve_detail() {
        let errors = [0.0, f64::NAN, f64::INFINITY, 8.0];
        assert_eq!(select_lod(&errors, 100.0, 1_000.0, 2.0, 2), 0);
        assert_eq!(select_lod(&errors, 0.0, 1_000.0, 2.0, 3), 0);
        assert_eq!(select_lod(&errors, f64::NAN, 1_000.0, 2.0, 3), 0);
        assert_eq!(select_lod(&[], 1.0, 1.0, 1.0, 0), 0);
    }

    #[test]
    fn prefetch_and_eviction_keep_the_coarse_safety_net() {
        assert_eq!(prefetch_lod(2, true), 1);
        assert_eq!(prefetch_lod(0, true), 0);
        assert_eq!(prefetch_lod(2, false), 2);
        assert!(!should_evict(10.0, 20.0, 3.0, true));
        assert!(!should_evict(10.0, 12.0, 3.0, false));
        assert!(should_evict(10.0, 13.0, 3.0, false));
        assert!(!should_evict(10.0, 9.0, 3.0, false));
    }
}
