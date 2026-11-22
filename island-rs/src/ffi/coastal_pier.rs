//! Native JSON export for one capital pier on a resident coastal section.

use super::{Island, c_char, c_void, coastal_bridges::MeshPayload, coastal_town::error};
use crate::{
    DecorationClearance, ISLAND_WORLD_METRES, Vec2, Vec3,
    piers::{PierBerth, PierLayout, PierPropPoint},
    terrain::town_pier::CapitalPier,
};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString},
    ptr,
};

#[derive(Deserialize)]
struct Request {
    centre: Vec2,
    mouth: Option<Vec2>,
    #[serde(default)]
    blockers: Vec<DecorationClearance>,
    #[serde(default)]
    seed: u64,
}

#[derive(Serialize)]
struct Report<'a> {
    pier: Option<PreparedPier<'a>>,
    reason: &'static str,
}

#[derive(Serialize)]
struct PreparedPier<'a> {
    id: String,
    start: Vec3,
    end: Vec3,
    deck_height: f32,
    width: f32,
    berth_side: f32,
    berth: Berth<'a>,
    clearance: DecorationClearance,
    access_connected: bool,
    timber: MeshPayload<'a>,
    stone: MeshPayload<'a>,
    metal: MeshPayload<'a>,
    coarse_timber: MeshPayload<'a>,
    coarse_stone: MeshPayload<'a>,
    collision: MeshPayload<'a>,
    navigation: MeshPayload<'a>,
    prop_points: &'a [PierPropPoint],
}

// Flatten the explicitly shared layout without depending on its binary layout.
#[derive(Serialize)]
struct Berth<'a> {
    position: &'a Vec3,
    approach: &'a Vec3,
    heading: &'a Vec2,
    min_depth: f32,
    length: f32,
    width: f32,
}

impl<'a> From<&'a PierBerth> for Berth<'a> {
    fn from(berth: &'a PierBerth) -> Self {
        Self {
            position: &berth.position,
            approach: &berth.approach,
            heading: &berth.heading,
            min_depth: berth.min_depth,
            length: berth.length,
            width: berth.width,
        }
    }
}

impl<'a> PreparedPier<'a> {
    fn new(pier: &'a CapitalPier, seed: u64) -> Self {
        let layout = &pier.layout;
        let meshes = &pier.meshes;
        let key = u64::from(layout.shore.x.to_bits()) << 32 | u64::from(layout.shore.y.to_bits());
        Self {
            id: format!("capital-pier-{:016x}", key ^ seed),
            start: layout.start,
            end: layout.end.extend(layout.deck_height),
            deck_height: layout.deck_height,
            width: layout.width,
            berth_side: layout.berth_side,
            berth: Berth::from(&layout.berth),
            clearance: pier.clearance,
            access_connected: pier.access_connected,
            timber: MeshPayload::from(&meshes.timber),
            stone: MeshPayload::from(&meshes.stone),
            metal: MeshPayload::from(&meshes.metal),
            coarse_timber: MeshPayload::from(&meshes.coarse_timber),
            coarse_stone: MeshPayload::from(&meshes.coarse_stone),
            collision: MeshPayload::from(&meshes.collision),
            navigation: MeshPayload::from(&meshes.navigation),
            prop_points: &meshes.prop_points,
        }
    }
}

/// Prepare the capital pier. The caller owns the returned JSON and frees it
/// with `FreeCoastalTownJson`.
///
/// # Safety
/// The handle must reference a live island exclusively borrowed for the call.
/// The request must be a readable NUL-terminated UTF-8 buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PrepareCoastalCapitalPier(
    handle: *mut c_void,
    json: *const c_char,
) -> *mut c_char {
    let (Some(island), false) = (unsafe { handle.cast::<Island>().as_mut() }, json.is_null())
    else {
        error("capital pier preparation needs a live island and a request");
        return ptr::null_mut();
    };
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    let result = prepare_report(island, bytes);
    match result {
        Ok(report) => {
            error("");
            report.into_raw()
        }
        Err(reason) => {
            error(&reason);
            ptr::null_mut()
        }
    }
}

fn prepare_report(island: &mut Island, bytes: &[u8]) -> Result<CString, String> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("capital pier request exceeds 4 MiB".into());
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|reason| reason.to_string())?;
    let inside = |point: Vec2| {
        point.cmpge(Vec2::ZERO).all() && point.cmple(Vec2::splat(ISLAND_WORLD_METRES)).all()
    };
    if !inside(request.centre)
        || request.mouth.is_some_and(|point| !inside(point))
        || request.blockers.len() > 8192
    {
        return Err("invalid capital pier location or blocker count".into());
    }
    request
        .blockers
        .iter()
        .try_for_each(DecorationClearance::validate)?;

    let pier = island.prepare_capital_pier(request.centre, request.mouth, &request.blockers)?;
    let report = Report {
        reason: match &pier {
            None => "no_usable_explorer_berth",
            Some(pier) if !pier.access_connected => "no_walkable_road_approach",
            Some(_) => "",
        },
        pier: pier
            .as_ref()
            .map(|pier| PreparedPier::new(pier, request.seed)),
    };
    let json = serde_json::to_string(&report).map_err(|reason| reason.to_string())?;
    CString::new(json).map_err(|reason| reason.to_string())
}

/// Survey exact terrain before grading. Release the returned JSON with `FreeCoastalTownJson`.
///
/// # Safety
/// The handle must reference a live island for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SurveyCoastalCapitalPiers(
    handle: *const c_void,
    mouth_x: f32,
    mouth_y: f32,
) -> *mut c_char {
    let Some(island) = (unsafe { super::island_ref(handle) }) else {
        error("capital pier survey requires a live island");
        return ptr::null_mut();
    };
    let mouth = Vec2::new(mouth_x, mouth_y);
    let layouts = island.survey_capital_piers(mouth, Some(mouth), &[], 16);
    export_json(&layouts)
}

/// Build the selected surveyed pier against the finished paved approach.
///
/// # Safety
/// The handle must be live and the layout JSON must be readable and NUL-terminated.
/// Release the returned JSON with `FreeCoastalTownJson`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ExportPavedCapitalPier(
    handle: *const c_void,
    json: *const c_char,
    seed: u64,
) -> *mut c_char {
    let (Some(island), false) = (unsafe { super::island_ref(handle) }, json.is_null()) else {
        error("paved pier export requires a live island and surveyed layout");
        return ptr::null_mut();
    };
    let layout: PierLayout =
        match serde_json::from_slice(unsafe { CStr::from_ptr(json) }.to_bytes()) {
            Ok(layout) => layout,
            Err(reason) => {
                error(&reason.to_string());
                return ptr::null_mut();
            }
        };
    let pier = island.paved_capital_pier(layout);
    export_json(&Report {
        pier: pier.as_ref().map(|pier| PreparedPier::new(pier, seed)),
        reason: if pier.is_some() {
            ""
        } else {
            "no_paved_road_approach"
        },
    })
}

fn export_json(value: &impl Serialize) -> *mut c_char {
    match serde_json::to_string(value)
        .and_then(|json| CString::new(json).map_err(serde::ser::Error::custom))
    {
        Ok(json) => {
            error("");
            json.into_raw()
        }
        Err(reason) => {
            error(&reason.to_string());
            ptr::null_mut()
        }
    }
}
