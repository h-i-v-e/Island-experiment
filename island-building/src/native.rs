//! Owned C boundary. Generation remains available through the pure Rust API.
mod export;
use crate::town::{TownInput, TownRecipe};
use serde::Deserialize;
use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    ptr,
    sync::OnceLock,
};

thread_local! { static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default()); }
struct TownHandle {
    json: CString,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    recipe: serde_json::Value,
    village: Option<serde_json::Value>,
    input: TownInput,
}

fn error(message: &str) {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = CString::new(message.replace('\0', " ")).expect("NUL removed");
    });
}

fn generate(request: &str, base: &Path) -> Result<CString, String> {
    let request: Request = serde_json::from_str(request).map_err(|e| e.to_string())?;
    let capital = parse_recipe(request.recipe)?;
    let output = if let Some(village) = request.village {
        let village = parse_recipe(village).map_err(|error| format!("village recipe: {error}"))?;
        crate::town::generate_settlements_with_base(
            &capital,
            &village,
            &request.input,
            base,
            &|| false,
        )
    } else {
        crate::town::generate_town_with_base(&capital, &request.input, base, &|| false)
    }
    .map_err(|e| e.to_string())?;
    CString::new(export::encode(&output)?).map_err(|e| e.to_string())
}

fn parse_recipe(mut recipe: serde_json::Value) -> Result<TownRecipe, String> {
    if let Some(entries) = recipe
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
    serde_json::from_value(recipe).map_err(|e| e.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn GetMotuTownApiVersion() -> u32 {
    2
}
#[unsafe(no_mangle)]
pub extern "C" fn GetLastMotuTownError() -> *const c_char {
    LAST_ERROR.with(|cell| cell.borrow().as_ptr())
}

/// The default church's exterior stone maps and weathering, shared by bridges.
/// The returned UTF-8 string is borrowed for the lifetime of the library.
#[unsafe(no_mangle)]
pub extern "C" fn ExportMotuChurchStoneJson() -> *const c_char {
    static STONE: OnceLock<Result<CString, String>> = OnceLock::new();
    match STONE
        .get_or_init(|| CString::new(export::church_stone()?).map_err(|reason| reason.to_string()))
    {
        Ok(json) => json.as_ptr(),
        Err(reason) => {
            error(reason);
            ptr::null()
        }
    }
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
        assert_eq!(GetMotuTownApiVersion(), 2);
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
        assert_eq!(exported["schema_version"], 2);
        assert_eq!(exported["roads_json"], "");
        assert_eq!(exported["road_count"], 0);
        assert_eq!(exported["road_exits"], serde_json::json!([]));
        assert_eq!(exported["wheat_fields"], serde_json::json!([]));
        assert_eq!(exported["instances"].as_array().unwrap().len(), 1);
        assert_eq!(exported["instances"][0]["entry_id"], "hut");
        assert_eq!(exported["instances"][0]["settlement_index"], 0);
        assert_eq!(exported["settlements"][0]["kind"], "capital");
        assert_eq!(exported["settlements"][0]["mouth"], exported["mouth"]);
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
        let blockers: Vec<crate::town::ClearanceRect> =
            serde::Deserialize::deserialize(&exported["routing_blockers"]).unwrap();
        assert_eq!(blockers, areas);
        unsafe {
            DestroyMotuTown(town);
        }
    }

    #[test]
    fn native_wheat_plots_are_oriented_xy_rectangles() {
        let field = crate::town::ClearanceRect {
            centre: [15., -9.],
            half_extents: [6.5, 3.],
            yaw: 0.75,
        };
        let output = crate::town::TownOutput {
            wheat_fields: vec![field],
            ..crate::town::TownOutput::default()
        };
        let exported: serde_json::Value =
            serde_json::from_str(&export::encode(&output).unwrap()).unwrap();
        assert_eq!(
            exported["wheat_fields"],
            serde_json::json!([{
                "centre": [15., -9.], "half_extents": [6.5, 3.], "yaw": 0.75
            }])
        );
        let mut old_output = serde_json::to_value(output).unwrap();
        old_output.as_object_mut().unwrap().remove("wheat_fields");
        let old_output: crate::town::TownOutput = serde_json::from_value(old_output).unwrap();
        assert!(old_output.wheat_fields.is_empty());
    }

    #[test]
    fn native_village_request_returns_all_sites_and_emits_empty_arrays() {
        let mut building = crate::BuildingRecipe::hut();
        building.atlas_size = 128;
        let mut request = serde_json::json!({
            "recipe": {"schema_version":1, "catalogue":[]},
            "village": {"schema_version":1, "catalogue":[{
                "id":"cottage", "building_json":serde_json::to_string(&building).unwrap(),
                "min_count":1, "max_count":1, "placement":"centre"
            }], "settings":{"min_dry_height": 5}},
            "input":{"tile_seed":33,"sea_level":0,"terrain":{
                "origin":[0,0],"spacing":10,"width":5,"height":3,
                "heights":[-2,-2,-2,-2,-2,4,4,4,4,4,4,4,4,4,4]
            }, "rivers":[{"points":[[30,20],[30,0]]},{"points":[[10,20],[10,0]]}]}
        });
        let package = |request: &serde_json::Value| -> serde_json::Value {
            let json = generate(&request.to_string(), Path::new(".")).unwrap();
            serde_json::from_slice(json.as_bytes()).unwrap()
        };
        let output = package(&request);
        let sites = output["settlements"].as_array().unwrap();
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0]["kind"], "capital");
        assert_eq!(
            sites[0]["river_index"], 1,
            "a midpoint tie uses the lower X outlet"
        );
        assert_eq!(sites[0]["mouth"], output["mouth"]);
        assert_eq!(sites[0]["centre"], serde_json::json!([]));
        assert_eq!(sites[1]["kind"], "village");
        assert_eq!(sites[1]["building_count"], 0);
        assert_eq!(sites[1]["skipped_count"], 1);
        assert_eq!(sites[1]["centre"], serde_json::json!([]));
        request["input"]["rivers"] = serde_json::json!([]);
        let dry = package(&request);
        assert_eq!(dry["settlements"], serde_json::json!([]));
        assert_eq!(dry["mouth"], serde_json::json!([]));
    }
}
