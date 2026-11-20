//! Additive owned JSON boundary for engine-independent town terrain and clearance.
use super::{Island, c_char, c_void};
use crate::{
    DecorationClearance, ISLAND_WORLD_METRES, TownProtectedTerrain, TownStairFoot, Vec2,
    roads::RoadNetwork,
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

pub(super) fn error(message: &str) {
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

/// Return whether this terrain has graded road material weights.
///
/// # Safety
/// A non-null handle must reference a live, shared island.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn HasTerrainRoadCoverage(handle: *const c_void) -> i32 {
    i32::from(
        unsafe { super::island_ref(handle) }
            .is_some_and(|island| island.terrain().has_road_coverage()),
    )
}

/// Mark exported terrain vertices with the road material profile at their XY.
/// The profile ranges from 0 (natural terrain) through 0.5 (gravel edge)
/// to 1 (paved road). Footpaths use the same gravel band.
///
/// # Safety
/// A non-null handle must reference a live island. The vertex and weight
/// buffers must be valid and disjoint for `vertices.length` elements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CopyTerrainRoadCoverage(
    handle: *const c_void,
    vertices: super::Vector3ExportArray,
    weights: *mut f32,
) -> i32 {
    let Some(island) = (unsafe { super::island_ref(handle) }) else {
        return 0;
    };
    if vertices.length <= 0 || vertices.data.is_null() || weights.is_null() {
        return 0;
    }
    let length = usize::try_from(vertices.length).expect("positive vertex count");
    let vertices = unsafe { std::slice::from_raw_parts(vertices.data, length) };
    let weights = unsafe { std::slice::from_raw_parts_mut(weights, length) };
    let terrain = island.terrain();
    for (weight, vertex) in weights.iter_mut().zip(vertices) {
        *weight = terrain.town_surface_coverage_at(vertex.truncate());
    }
    1
}

#[derive(serde::Deserialize)]
struct TownFootpathRequest {
    feet: Vec<TownStairFoot>,
    foundations: Vec<DecorationClearance>,
}

/// Apply stair footpath paint to existing graded terrain. Returns the number
/// of marked vertices, or -1 for invalid input.
/// # Safety
/// The caller must own exclusive access to a live island and provide a valid,
/// NUL-terminated UTF-8 buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PaintCoastalTownFootpaths(
    handle: *mut c_void,
    json: *const c_char,
) -> i32 {
    let Some(island) = (unsafe { handle.cast::<Island>().as_mut() }) else {
        error("town footpaths require a live exclusively owned island");
        return -1;
    };
    if json.is_null() {
        error("town footpath JSON is null");
        return -1;
    }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    let request = (bytes.len() <= 4 * 1024 * 1024)
        .then(|| serde_json::from_slice::<TownFootpathRequest>(bytes).ok())
        .flatten();
    let Some(request) = request else {
        error("invalid town footpath JSON request");
        return -1;
    };
    match island.paint_town_footpaths(&request.feet, &request.foundations) {
        Ok(count) => {
            error("");
            i32::try_from(count).unwrap_or(i32::MAX)
        }
        Err(reason) => {
            error(&reason);
            -1
        }
    }
}

#[derive(serde::Deserialize)]
struct CartTrackRequest {
    exits: Vec<crate::terrain::cart_tracks::CartTrackExit>,
    blockers: Vec<crate::terrain::cart_tracks::CartTrackBlocker>,
    coast_seams: bool,
    #[serde(default)]
    settlement_count: usize,
    settlements: Option<Vec<CartTrackSettlement>>,
}

#[derive(serde::Deserialize)]
struct CartTrackSettlement {
    kind: String,
    #[serde(default)]
    centre: Option<Vec<f32>>,
}

impl CartTrackSettlement {
    fn centre(&self) -> Option<Vec2> {
        let [x, y] = self.centre.as_deref()? else {
            return None;
        };
        let world = 0. ..=ISLAND_WORLD_METRES;
        (world.contains(x) && world.contains(y)).then_some(Vec2::new(*x, *y))
    }
}

/// Plan tracks after town grading and before terrain and decoration exports.
/// Release the returned report with `FreeCoastalTownJson`.
///
/// # Safety
/// The caller must own a live, exclusively borrowed island and provide a
/// readable NUL-terminated request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PrepareCoastalCartTracks(
    handle: *mut c_void,
    json: *const c_char,
) -> *mut c_char {
    let (Some(island), false) = (unsafe { handle.cast::<Island>().as_mut() }, json.is_null())
    else {
        error("cart-track preparation requires a live island and a request");
        return ptr::null_mut();
    };
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    let request = (bytes.len() <= 4 * 1024 * 1024)
        .then(|| serde_json::from_slice::<CartTrackRequest>(bytes).ok())
        .flatten();
    let Some(request) = request else {
        error("invalid cart-track JSON request");
        return ptr::null_mut();
    };
    if request.settlement_count > 512
        || request
            .settlements
            .as_ref()
            .is_some_and(|sites| sites.len() != request.settlement_count)
        // A single graded strip can now contribute departures at both ends.
        || request.exits.len() > crate::roads::MAX_ROAD_EDGES * 2
        || request.blockers.len() > 4096
        || request.exits.iter().any(|exit| {
            exit.settlement_index > 512
                || !exit
                    .position
                    .iter()
                    .chain(&exit.direction)
                    .all(|value| value.is_finite())
                || exit.position[..2]
                    .iter()
                    .any(|v| !(0.0..=ISLAND_WORLD_METRES).contains(v))
                || !(0.1..=50.).contains(&exit.half_width)
                || (Vec2::from_array(exit.direction).length_squared() - 1.).abs() > 0.01
        })
        || request
            .blockers
            .iter()
            .any(|blocker| blocker.validate().is_err())
    {
        error("cart-track exits or blockers exceed routing limits");
        return ptr::null_mut();
    }
    let capital = match &request.settlements {
        Some(sites) => sites.iter().position(|site| site.kind == "capital"),
        None if request.settlement_count > 0 => Some(0),
        None => request.exits.iter().map(|exit| exit.settlement_index).min(),
    };
    let centres: Vec<_> = request
        .settlements
        .iter()
        .flatten()
        .map(CartTrackSettlement::centre)
        .collect();
    let report = island
        .prepare_cart_tracks(
            &request.exits,
            &request.blockers,
            request.coast_seams,
            request.settlement_count,
            capital,
            &centres,
        )
        .and_then(|report| serde_json::to_string(&report).map_err(|err| err.to_string()))
        .and_then(|json| CString::new(json).map_err(|err| err.to_string()));
    match report {
        Ok(json) => {
            error("");
            json.into_raw()
        }
        Err(reason) => {
            error(&reason);
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn HasTerrainCartTracks(handle: *const c_void) -> i32 {
    i32::from(
        unsafe { super::island_ref(handle) }
            .is_some_and(|island| island.terrain().has_cart_tracks()),
    )
}

/// Sample the two wheel ruts for exported terrain vertices.
///
/// # Safety
/// The live island and vertex array are shared. The caller gives writable
/// output storage for every vertex, disjoint from the vertex array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CopyTerrainCartTrackCoverage(
    handle: *const c_void,
    vertices: super::Vector3ExportArray,
    weights: *mut f32,
) -> i32 {
    let Some(island) = (unsafe { super::island_ref(handle) }) else {
        return 0;
    };
    if vertices.length <= 0 || vertices.data.is_null() || weights.is_null() {
        return 0;
    }
    let length = usize::try_from(vertices.length).expect("positive vertex count");
    let positions = unsafe { std::slice::from_raw_parts(vertices.data, length) };
    let weights = unsafe { std::slice::from_raw_parts_mut(weights, length) };
    let terrain = island.terrain();
    for (weight, vertex) in weights.iter_mut().zip(positions) {
        *weight = terrain.cart_track_coverage(vertex.truncate());
    }
    1
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
