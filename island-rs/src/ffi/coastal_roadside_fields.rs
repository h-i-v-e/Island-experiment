//! Owned field layout report in native XY metres, with elevation in metres.
use super::{Island, c_char, c_void, coastal_town::error};
use crate::DecorationClearance;
use serde::Deserialize;
use std::{
    ffi::{CStr, CString},
    ptr,
};

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_BLOCKERS: usize = 16_384;

#[derive(Deserialize)]
struct Request {
    seed: String,
    wind_direction: [f32; 2],
    blockers: Vec<DecorationClearance>,
}

/// Prepare fields and plant their windbreak trees after roads, bridges and town wheat are fitted.
/// The report includes the accompanying bushes and horizontal wheat clearance radii in metres.
/// The caller releases a successful report with `FreeCoastalTownJson`.
///
/// # Safety
/// `handle` must reference a live island borrowed exclusively for the duration of the call.
/// `json` must point to a readable NUL-terminated UTF-8 buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PrepareCoastalRoadsideFields(
    handle: *mut c_void,
    json: *const c_char,
) -> *mut c_char {
    let (Some(island), false) = (unsafe { handle.cast::<Island>().as_mut() }, json.is_null())
    else {
        error("roadside field preparation requires a live island and a request");
        return ptr::null_mut();
    };
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    match prepare(island, bytes) {
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

fn prepare(island: &mut Island, bytes: &[u8]) -> Result<CString, String> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err("roadside field request exceeds 2 MiB".into());
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|reason| reason.to_string())?;
    let seed = request
        .seed
        .parse::<u64>()
        .map_err(|reason| reason.to_string())?;
    if request.blockers.len() > MAX_BLOCKERS
        || request
            .wind_direction
            .iter()
            .any(|value| !value.is_finite())
    {
        return Err("invalid roadside field blockers or wind direction".into());
    }
    for blocker in &request.blockers {
        blocker.validate()?;
    }
    let fields = island.prepare_roadside_fields(&request.blockers, request.wind_direction, seed)?;
    let json = serde_json::to_vec(&fields).map_err(|reason| reason.to_string())?;
    CString::new(json).map_err(|reason| reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::coastal_town::FreeCoastalTownJson;

    #[test]
    fn island_without_tracks_returns_empty_owned_report() {
        let mut island = Island::shared_forest_fixture(crate::ForestOptions::default());
        let request =
            c"{\"seed\":\"18446744073709551615\",\"wind_direction\":[1,0],\"blockers\":[]}";
        let report =
            unsafe { PrepareCoastalRoadsideFields((&raw mut island).cast(), request.as_ptr()) };
        assert!(!report.is_null());
        let bytes = unsafe { CStr::from_ptr(report) }.to_bytes();
        assert_eq!(
            bytes,
            br#"{"fields":[],"walls":[],"gates":[],"windbreaks":[],"bushes":[]}"#
        );
        unsafe { FreeCoastalTownJson(report) };
    }
}
