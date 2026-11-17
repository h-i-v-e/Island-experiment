//! Owned C boundary. Generation remains available through the pure Rust API.
mod export;
use crate::town::{TownInput, TownRecipe};
use serde::Deserialize;
use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    ptr,
};

thread_local! { static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default()); }
struct TownHandle {
    json: CString,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    recipe: serde_json::Value,
    input: TownInput,
}

fn error(message: &str) {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = CString::new(message.replace('\0', " ")).expect("NUL removed");
    });
}

fn generate(request: &str, base: &Path) -> Result<CString, String> {
    let mut request: Request = serde_json::from_str(request).map_err(|e| e.to_string())?;
    if let Some(entries) = request
        .recipe
        .get_mut("catalogue")
        .and_then(serde_json::Value::as_array_mut)
    {
        for entry in entries {
            if let Some(text) = entry
                .get("building_json")
                .and_then(serde_json::Value::as_str)
            {
                let building: serde_json::Value =
                    serde_json::from_str(text).map_err(|e| format!("building_json: {e}"))?;
                entry
                    .as_object_mut()
                    .expect("entry with field is an object")
                    .remove("building_json");
                entry["building"] = building;
            }
        }
    }
    let recipe: TownRecipe = serde_json::from_value(request.recipe).map_err(|e| e.to_string())?;
    let output = crate::town::generate_town_with_base(&recipe, &request.input, base, &|| false)
        .map_err(|e| e.to_string())?;
    CString::new(export::encode(&output)?).map_err(|e| e.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn GetMotuTownApiVersion() -> u32 {
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn GetLastMotuTownError() -> *const c_char {
    LAST_ERROR.with(|cell| cell.borrow().as_ptr())
}

/// Borrows UTF-8 strings for this call; owns all output until `DestroyMotuTown`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateMotuTown(
    request: *const c_char,
    base: *const c_char,
) -> *mut c_void {
    if request.is_null() {
        error("town request is null");
        return ptr::null_mut();
    }
    let result = std::panic::catch_unwind(|| {
        let request = unsafe { CStr::from_ptr(request) }
            .to_str()
            .map_err(|e| e.to_string())?;
        if request.len() > 256 * 1024 * 1024 {
            return Err("town request exceeds 256MiB".into());
        }
        let base = if base.is_null() {
            "."
        } else {
            unsafe { CStr::from_ptr(base) }
                .to_str()
                .map_err(|e| e.to_string())?
        };
        generate(request, Path::new(if base.is_empty() { "." } else { base }))
    });
    match result {
        Ok(Ok(json)) => {
            error("");
            Box::into_raw(Box::new(TownHandle { json })).cast()
        }
        Ok(Err(reason)) => {
            error(&reason);
            ptr::null_mut()
        }
        Err(_) => {
            error("town generation panicked");
            ptr::null_mut()
        }
    }
}

/// Borrowed pointer remains valid until this town handle is destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ExportMotuTownJson(handle: *const c_void) -> *const c_char {
    unsafe { handle.cast::<TownHandle>().as_ref() }.map_or(ptr::null(), |town| town.json.as_ptr())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn DestroyMotuTown(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<TownHandle>()) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn null_and_invalid_native_requests_report_errors() {
        assert_eq!(GetMotuTownApiVersion(), 1);
        assert!(unsafe { CreateMotuTown(ptr::null(), ptr::null()) }.is_null());
        let request = CString::new("{}").unwrap();
        assert!(unsafe { CreateMotuTown(request.as_ptr(), ptr::null()) }.is_null());
        assert!(
            !unsafe { CStr::from_ptr(GetLastMotuTownError()) }
                .to_bytes()
                .is_empty()
        );
        assert!(unsafe { ExportMotuTownJson(ptr::null()) }.is_null());
        unsafe {
            DestroyMotuTown(ptr::null_mut());
        }
    }

    #[test]
    fn native_town_exports_flat_meshes_shared_stone_and_grub_coordinates() {
        let mut building = crate::BuildingRecipe::hut();
        building.atlas_size = 128;
        let heights: Vec<f32> = (0_u16..101)
            .flat_map(|y| std::iter::repeat_n((f32::from(y) * 0.4).min(4.), 101))
            .collect();
        let request = serde_json::json!({
            "recipe": {"schema_version":1,"catalogue":[{"id":"hut","building_json":serde_json::to_string(&building).unwrap(),
                "min_count":1,"max_count":1,"placement":"centre"}]},
            "input":{"tile_seed":543,"sea_level":0,"terrain":{"origin":[0,0],"spacing":2,"width":101,"height":101,"heights":heights},
                "rivers":[{"points":[[100,150],[100,0]],"widths":[4,6]}]}
        });
        let request = CString::new(request.to_string()).unwrap();
        let town = unsafe { CreateMotuTown(request.as_ptr(), ptr::null()) };
        assert!(
            !town.is_null(),
            "{}",
            unsafe { CStr::from_ptr(GetLastMotuTownError()) }.to_string_lossy()
        );
        let text = unsafe { CStr::from_ptr(ExportMotuTownJson(town)) }
            .to_str()
            .unwrap();
        let exported: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(exported["instances"].as_array().unwrap().len(), 1);
        let supports = &exported["supports"];
        assert_eq!(
            supports["positions"].as_array().unwrap().len(),
            supports["grub_positions"].as_array().unwrap().len()
        );
        assert!(supports["positions"][0].is_number());
        assert_eq!(
            exported["support_material"]["grub_uniform"]
                .as_array()
                .unwrap()
                .len(),
            20
        );
        assert!(
            exported["support_material"]["albedo_png"]
                .as_str()
                .unwrap()
                .starts_with("iVBOR")
        );
        assert!(
            exported["prototypes"][0]["lods"][0]["parts"]
                .as_array()
                .unwrap()
                .iter()
                .all(|part| !["foundation", "stairs", "stair_treads"]
                    .contains(&part["material"].as_str().unwrap()))
        );
        let areas: Vec<crate::town::ClearanceRect> =
            serde_json::from_str(exported["clearance_json"].as_str().unwrap()).unwrap();
        assert!(!areas.is_empty());
        unsafe {
            DestroyMotuTown(town);
        }
    }
}
