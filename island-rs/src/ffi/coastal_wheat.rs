//! Owned JSON export for fitted field rectangles in native XY metres.
use super::{Island, c_char, c_void, coastal_town::error};
use crate::DecorationClearance;
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString},
    ptr,
};

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
struct Request {
    fields: Vec<DecorationClearance>,
    #[serde(default)]
    bridge_clearance: Vec<DecorationClearance>,
}

#[derive(Serialize)]
struct Report {
    fields: Vec<DecorationClearance>,
}

/// Fits fields after roads, footpaths, cart tracks and bridges have been
/// prepared. The report is owned by the caller and freed with `FreeCoastalTownJson`.
///
/// # Safety
/// The handle must reference a live island accessible exclusively. The request
/// must be a readable NUL-terminated UTF-8 buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PrepareCoastalWheatFields(
    handle: *mut c_void,
    json: *const c_char,
) -> *mut c_char {
    let (Some(island), false) = (unsafe { handle.cast::<Island>().as_mut() }, json.is_null())
    else {
        error("wheat preparation requires a live island and a request");
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
        return Err("wheat request exceeds 2 MiB".into());
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|reason| reason.to_string())?;
    let fields = island.prepare_coastal_wheat_fields(&request.fields, &request.bridge_clearance)?;
    let json = serde_json::to_vec(&Report { fields }).map_err(|reason| reason.to_string())?;
    CString::new(json).map_err(|reason| reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::coastal_town::FreeCoastalTownJson;

    #[test]
    fn empty_and_valid_requests_return_owned_field_reports() {
        let mut island = Island::shared_forest_fixture(crate::ForestOptions::default());
        let handle = (&raw mut island).cast();
        let empty = unsafe { PrepareCoastalWheatFields(handle, c"{\"fields\":[]}".as_ptr()) };
        assert!(!empty.is_null());
        assert_eq!(
            unsafe { CStr::from_ptr(empty) }.to_bytes(),
            br#"{"fields":[]}"#
        );
        unsafe { FreeCoastalTownJson(empty) };

        let request = c"{\"fields\":[{\"centre\":[650,650],\"half_extents\":[6,4],\"yaw\":0.4}]}";
        let report = unsafe { PrepareCoastalWheatFields(handle, request.as_ptr()) };
        assert!(!report.is_null());
        let report_json: serde_json::Value =
            serde_json::from_slice(unsafe { CStr::from_ptr(report) }.to_bytes()).unwrap();
        assert_eq!(report_json["fields"].as_array().unwrap().len(), 1);
        unsafe { FreeCoastalTownJson(report) };
    }

    #[test]
    fn an_invalid_request_leaves_the_island_unchanged() {
        let mut island = Island::shared_forest_fixture(crate::ForestOptions::default());
        let original = island.clone();
        let invalid = c"{\"fields\":[{\"centre\":[650,650],\"half_extents\":[6,4],\"yaw\":0},{\"centre\":[650,650],\"half_extents\":[999,4],\"yaw\":0}]}";
        let report =
            unsafe { PrepareCoastalWheatFields((&raw mut island).cast(), invalid.as_ptr()) };
        assert!(report.is_null());
        assert!(island == original);
    }
}
