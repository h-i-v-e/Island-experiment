//! Additive owned JSON boundary for engine-independent town terrain and clearance.
use super::{Island, c_char, c_void};
use crate::{
    DecorationClearance, ISLAND_WORLD_METRES, TownProtectedTerrain, Vec2, roads::RoadNetwork,
};
use rayon::prelude::*;
use serde::Serialize;
use std::{
    cell::RefCell,
    ffi::{CStr, CString},
    ptr,
};

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn error(message: &str) {
    let text = message.replace('\0', " ");
    LAST_ERROR.with(|cell| *cell.borrow_mut() = CString::new(text).expect("NUL removed"));
}

#[unsafe(no_mangle)]
pub extern "C" fn GetCoastalTownApiVersion() -> u32 {
    2
}

#[unsafe(no_mangle)]
pub extern "C" fn GetLastCoastalTownError() -> *const c_char {
    LAST_ERROR.with(|cell| cell.borrow().as_ptr())
}

#[derive(Serialize)]
struct HeightGrid {
    origin: [f32; 2],
    spacing: f32,
    width: usize,
    height: usize,
    heights: Vec<f32>,
}
#[derive(Serialize)]
struct RiverPolyline {
    points: Vec<[f32; 2]>,
    widths: Vec<f32>,
    join: Option<usize>,
}
#[derive(Serialize)]
struct TownInput {
    tile_seed: u64,
    sea_level: f32,
    terrain: HeightGrid,
    rivers: Vec<RiverPolyline>,
    protected_terrain: Vec<TownProtectedTerrain>,
}

#[allow(clippy::cast_precision_loss)] // Grid indices are at most 1000, exactly representable in f32.
fn terrain_input(island: &Island, tile_seed: u64) -> TownInput {
    const DIMENSION: usize = 1001;
    const SPACING: f32 = 2.;
    let mut heights = vec![0.; DIMENSION * DIMENSION];
    heights
        .par_chunks_mut(DIMENSION)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, value) in row.iter_mut().enumerate() {
                let point = Vec2::new(x as f32 * SPACING, y as f32 * SPACING) / ISLAND_WORLD_METRES;
                *value = island.terrain().sample_accurate_surface(point).map_or_else(
                    || island.terrain().sample(point.x, point.y),
                    |hit| hit.position.z,
                ) * ISLAND_WORLD_METRES;
            }
        });
    let mut remap = vec![None; island.rivers().len()];
    let mut next = 0;
    for (index, river) in island.rivers().iter().enumerate() {
        if river.nodes.len() >= 2 {
            remap[index] = Some(next);
            next += 1;
        }
    }
    let rivers = island
        .rivers()
        .iter()
        .filter(|r| r.nodes.len() >= 2)
        .map(|river| RiverPolyline {
            join: river
                .join
                .and_then(|index| remap.get(index).copied().flatten()),
            points: river
                .nodes
                .iter()
                .map(|node| (node.position.truncate() * ISLAND_WORLD_METRES).to_array())
                .collect(),
            widths: river
                .target_half_widths(
                    island.options().river_source_width_metres,
                    island.options().river_maximum_width_metres,
                )
                .into_iter()
                .map(|width| width * 2.)
                .collect(),
        })
        .collect();
    TownInput {
        tile_seed,
        sea_level: 0.,
        terrain: HeightGrid {
            origin: [0.; 2],
            spacing: SPACING,
            width: DIMENSION,
            height: DIMENSION,
            heights,
        },
        rivers,
        protected_terrain: island.town_protected_terrain(),
    }
}

/// Grades the resident terrain before terrain, collider and decoration exports.
///
/// Returns 1 on success, 0 for malformed JSON or null arguments, and -1 if the
/// road network fails geometry checks. A failure leaves the island unchanged.
///
/// # Safety
/// The caller must own exclusive access to a live island and supply a valid
/// NUL-terminated UTF-8 buffer. The native island must be a fresh generated or
/// restored base snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ApplyCoastalTownRoadsJson(
    handle: *mut c_void,
    json: *const c_char,
) -> i32 {
    const MAXIMUM_ROAD_JSON_BYTES: usize = 16 * 1024 * 1024;
    let Some(island) = (unsafe { handle.cast::<Island>().as_mut() }) else {
        error("road grading requires a live exclusively owned island");
        return 0;
    };
    if json.is_null() {
        error("road grading JSON is null");
        return 0;
    }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    if bytes.len() > MAXIMUM_ROAD_JSON_BYTES {
        error("road grading JSON exceeds 16 MiB");
        return 0;
    }
    let roads = match serde_json::from_slice::<RoadNetwork>(bytes) {
        Ok(roads) => roads,
        Err(reason) => {
            error(&reason.to_string());
            return 0;
        }
    };
    match island.apply_town_roads(&roads) {
        Ok(()) => {
            error("");
            1
        }
        Err(reason) => {
            error(&reason);
            -1
        }
    }
}

/// Caller owns the returned UTF-8 allocation and releases it with `FreeCoastalTownJson`.
/// Resident island access must be exclusive for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ExportCoastalTownInputJson(
    handle: *const c_void,
    tile_seed: u64,
) -> *mut c_char {
    let Some(island) = (unsafe { super::island_ref(handle) }) else {
        error("town terrain export requires a live island handle");
        return ptr::null_mut();
    };
    match serde_json::to_string(&terrain_input(island, tile_seed))
        .and_then(|text| CString::new(text).map_err(serde::ser::Error::custom))
    {
        Ok(text) => {
            error("");
            text.into_raw()
        }
        Err(reason) => {
            error(&reason.to_string());
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FreeCoastalTownJson(json: *mut c_char) {
    if !json.is_null() {
        drop(unsafe { CString::from_raw(json) });
    }
}

/// Applies clearance once to a fresh or cached pre-clearance island. Returns
/// one on success; invalid JSON never partially modifies the island.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SetCoastalTownClearanceJson(
    handle: *mut c_void,
    json: *const c_char,
) -> i32 {
    let Some(island) = (unsafe { handle.cast::<Island>().as_mut() }) else {
        error("town clearance requires a live exclusively owned island");
        return 0;
    };
    if json.is_null() {
        error("town clearance JSON is null");
        return 0;
    }
    let result = (unsafe { CStr::from_ptr(json) })
        .to_str()
        .map_err(|reason| reason.to_string())
        .and_then(|text| {
            serde_json::from_str::<Vec<DecorationClearance>>(text)
                .map_err(|reason| reason.to_string())
        })
        .and_then(|areas| island.clear_decorations(&areas));
    match result {
        Ok(_) => {
            error("");
            1
        }
        Err(reason) => {
            error(&reason);
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_clearance_does_not_modify_an_island() {
        let mut island = Island::shared_forest_fixture(crate::ForestOptions::default());
        let before = island.clone();
        let json = CString::new("[{\"centre\":[0,0],\"half_extents\":[-1,2],\"yaw\":0}]").unwrap();
        assert_eq!(
            unsafe { SetCoastalTownClearanceJson((&raw mut island).cast(), json.as_ptr()) },
            0
        );
        assert!(
            island == before,
            "invalid clearance must not mutate the island"
        );
        assert_eq!(GetCoastalTownApiVersion(), 2);
    }

    #[test]
    fn road_boundary_distinguishes_protocol_rejection_and_no_op() {
        let mut island = Island::shared_forest_fixture(crate::ForestOptions::default());
        let before = island.clone();
        let handle = (&raw mut island).cast();
        let empty = c"{\"strips\":[],\"junctions\":[],\"shoulder\":0}";
        let invalid = c"{\"strips\":[],\"junctions\":[],\"shoulder\":-2}";
        assert_eq!(
            unsafe { ApplyCoastalTownRoadsJson(handle, c"[]".as_ptr()) },
            0
        );
        assert_eq!(
            unsafe { ApplyCoastalTownRoadsJson(handle, invalid.as_ptr()) },
            -1
        );
        assert_eq!(
            unsafe { ApplyCoastalTownRoadsJson(handle, empty.as_ptr()) },
            1
        );
        assert!(island == before);
    }
}
