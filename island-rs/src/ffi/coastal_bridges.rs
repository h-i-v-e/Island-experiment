//! Owned JSON export for bridge geometry fitted to resident terrain.
use super::{Island, c_char, c_void, coastal_town::error};
use crate::{
    Mesh, Vec3,
    bridges::{BridgeMeshes, BridgeRequest, BridgeSpanProfile, BridgeSurface, build_bridge_meshes},
    roads::RoadNetwork,
};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeStruct};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    ffi::{CStr, CString},
    ptr,
};

const MAXIMUM_REQUEST_BYTES: usize = 24 * 1024 * 1024;
const MAXIMUM_INSTALLED_BRIDGES: usize = 128;
const MAXIMUM_EXPORTED_VERTICES: usize = 450_000;
const SAME_LANDING_METRES: f32 = 0.35;

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    profile: BridgeSpanProfile,
    #[serde(default)]
    roads_json: Option<String>,
    #[serde(default)]
    existing_bridge_count: usize,
    #[serde(default)]
    existing_vertex_count: usize,
    crossings: Vec<BridgeRequest>,
}

#[derive(Serialize)]
struct Report {
    bridges: Vec<PreparedBridge>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize)]
struct Diagnostic {
    crossing: usize,
    reason: String,
}

struct PreparedBridge {
    id: String,
    start: [f32; 3],
    end: [f32; 3],
    arch_count: usize,
    width: f32,
    surface: BridgeSurface,
    meshes: BridgeMeshes,
}

impl Serialize for PreparedBridge {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut output = serializer.serialize_struct("PreparedBridge", 12)?;
        output.serialize_field("id", &self.id)?;
        output.serialize_field("start", &self.start)?;
        output.serialize_field("end", &self.end)?;
        output.serialize_field("arch_count", &self.arch_count)?;
        output.serialize_field("width", &self.width)?;
        output.serialize_field("surface", &self.surface)?;
        output.serialize_field("masonry", &MeshPayload::from(&self.meshes.masonry))?;
        output.serialize_field("dressing", &MeshPayload::from(&self.meshes.dressing))?;
        output.serialize_field("deck", &MeshPayload::from(&self.meshes.deck))?;
        output.serialize_field(
            "coarse_masonry",
            &MeshPayload::from(&self.meshes.coarse_masonry),
        )?;
        output.serialize_field("coarse_deck", &MeshPayload::from(&self.meshes.coarse_deck))?;
        output.serialize_field("collision", &MeshPayload::from(&self.meshes.collision))?;
        output.end()
    }
}

struct Flat<'a, T, const N: usize>(&'a [T]);

impl<T: AsRef<[f32; N]>, const N: usize> Serialize for Flat<'_, T, N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().flat_map(AsRef::as_ref))
    }
}

#[derive(Serialize)]
pub(super) struct MeshPayload<'a> {
    positions: Flat<'a, crate::Vec3, 3>,
    normals: Flat<'a, crate::Vec3, 3>,
    uvs: Flat<'a, crate::Vec2, 2>,
    indices: &'a [u32],
}

impl<'a> From<&'a Mesh> for MeshPayload<'a> {
    fn from(mesh: &'a Mesh) -> Self {
        Self {
            positions: Flat(&mesh.vertices),
            normals: Flat(&mesh.normals),
            uvs: Flat(&mesh.uv),
            indices: &mesh.triangles,
        }
    }
}

/// Fits marked bridge crossings on a resident island. The returned JSON is
/// owned by the caller and freed with `FreeCoastalTownJson`.
///
/// # Safety
/// The handle must reference a live island accessible for shared reads. The
/// request must be a valid NUL-terminated UTF-8 buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PrepareCoastalTownBridges(
    handle: *const c_void,
    json: *const c_char,
) -> *mut c_char {
    let Some(island) = (unsafe { super::island_ref(handle) }) else {
        error("bridge preparation requires a live island");
        return ptr::null_mut();
    };
    if json.is_null() {
        error("bridge request is null");
        return ptr::null_mut();
    }
    let bytes = unsafe { CStr::from_ptr(json) }.to_bytes();
    if bytes.len() > MAXIMUM_REQUEST_BYTES {
        error("bridge request exceeds 24 MiB");
        return ptr::null_mut();
    }
    let request = match serde_json::from_slice::<Request>(bytes) {
        Ok(request) => request,
        Err(reason) => {
            error(&reason.to_string());
            return ptr::null_mut();
        }
    };
    let roads = match impending_roads(request.roads_json.as_deref()) {
        Ok(roads) => roads,
        Err(reason) => {
            error(&reason);
            return ptr::null_mut();
        }
    };
    let result = build_report(island, request, roads.as_ref());
    match serde_json::to_string(&result)
        .and_then(|value| CString::new(value).map_err(serde::ser::Error::custom))
    {
        Ok(report) => {
            error("");
            report.into_raw()
        }
        Err(reason) => {
            error(&reason.to_string());
            ptr::null_mut()
        }
    }
}

fn impending_roads(json: Option<&str>) -> Result<Option<RoadNetwork>, String> {
    let Some(text) = json.filter(|text| !text.is_empty()) else {
        return Ok(None);
    };
    let roads: RoadNetwork = serde_json::from_str(text).map_err(|reason| reason.to_string())?;
    roads.validate()?;
    Ok(Some(roads))
}

fn build_report(island: &Island, request: Request, roads: Option<&RoadNetwork>) -> Report {
    let mut crossings: Vec<_> = request.crossings.into_iter().enumerate().collect();
    // Where both road passes nominate the same landings, preserve the street surface.
    crossings.sort_by_key(|(_, request)| request.surface != BridgeSurface::Paved);
    let mut report = Report {
        bridges: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut vertices = request.existing_vertex_count.min(MAXIMUM_EXPORTED_VERTICES);
    for (crossing, candidate) in crossings {
        if report
            .bridges
            .iter()
            .any(|bridge| same_crossing(bridge, &candidate))
        {
            continue;
        }
        if report.bridges.len()
            >= MAXIMUM_INSTALLED_BRIDGES.saturating_sub(request.existing_bridge_count)
        {
            report.diagnostics.push(bridge_budget(crossing));
            continue;
        }
        let layout = match island.fit_town_bridge(candidate, request.profile, roads) {
            Ok(layout) => layout,
            Err(reason) => {
                report.diagnostics.push(Diagnostic { crossing, reason });
                continue;
            }
        };
        let meshes = build_bridge_meshes(&layout);
        let count = [
            &meshes.masonry,
            &meshes.dressing,
            &meshes.deck,
            &meshes.coarse_masonry,
            &meshes.coarse_deck,
            &meshes.collision,
        ]
        .into_iter()
        .map(|mesh| mesh.vertices.len())
        .sum::<usize>();
        if vertices + count > MAXIMUM_EXPORTED_VERTICES {
            report.diagnostics.push(bridge_budget(crossing));
            continue;
        }
        vertices += count;
        let start = layout.start.to_array();
        let end = layout.end.to_array();
        report.bridges.push(PreparedBridge {
            id: bridge_id(start, end),
            start,
            end,
            arch_count: layout.bays.len(),
            width: layout.width,
            surface: layout.surface,
            meshes,
        });
    }
    report
        .diagnostics
        .sort_by_key(|diagnostic| diagnostic.crossing);
    report
}

fn bridge_budget(crossing: usize) -> Diagnostic {
    Diagnostic {
        crossing,
        reason: "the section has reached its bridge mesh budget".into(),
    }
}

fn same_crossing(bridge: &PreparedBridge, request: &BridgeRequest) -> bool {
    let close = |a, b| {
        Vec3::from_array(a).distance_squared(Vec3::from_array(b)) <= SAME_LANDING_METRES.powi(2)
    };
    (close(bridge.start, request.start) && close(bridge.end, request.end))
        || (close(bridge.start, request.end) && close(bridge.end, request.start))
}

fn bridge_id(start: [f32; 3], end: [f32; 3]) -> String {
    let order = start.iter().zip(end).find_map(|(a, b)| {
        let order = a.total_cmp(&b);
        (order != Ordering::Equal).then_some(order)
    });
    let ends = if order == Some(Ordering::Greater) {
        [end, start]
    } else {
        [start, end]
    };
    let mut hash = Sha256::new();
    for coordinate in ends.into_iter().flatten() {
        hash.update(coordinate.to_le_bytes());
    }
    let digest = hash.finalize();
    let short = u64::from_le_bytes(digest[..8].try_into().expect("eight digest bytes"));
    format!("bridge-{short:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bridge_export_prioritizes_paving_and_reports_invalid_crossings() {
        let island = Island::bridge_fixture();
        let from = [612., 650., 10.];
        let to = [638., 650., 10.5];
        let cart = json!({"start": to, "end": from, "kind": "river", "width": 2.4, "surface": "cart_track"});
        // Many road routes can nominate the same physical crossing.
        let mut crossings = vec![cart; 140];
        crossings.push(
            json!({"start": from, "end": to, "start_heading": [1., 0.], "end_heading": [],
            "kind": "river", "width": 4., "surface": "paved"}),
        );
        crossings.push(json!({"start": from, "end": from,
            "kind": "ravine", "width": 4., "surface": "paved"}));
        let request = json!({
            "crossings": crossings,
            "coast_seams": true
        });
        let text = CString::new(request.to_string()).unwrap();
        let handle = (&raw const island).cast();
        // SAFETY: both the resident island and the input remain live for the call.
        let report = unsafe { PrepareCoastalTownBridges(handle, text.as_ptr()) };
        assert!(!report.is_null());
        // SAFETY: the ABI returns one owned NUL-terminated string.
        let result: serde_json::Value =
            unsafe { serde_json::from_slice(CStr::from_ptr(report).to_bytes()).unwrap() };
        unsafe { super::super::coastal_town::FreeCoastalTownJson(report) };
        let bridges = result["bridges"].as_array().unwrap();
        assert_eq!(bridges.len(), 1, "{:?}", result["diagnostics"]);
        let bridge = &bridges[0];
        assert_eq!(bridge["id"], bridge_id(to, from));
        assert_eq!(bridge["surface"], "paved");
        assert_eq!(bridge["start"], json!(from));
        assert_eq!(bridge["end"], json!(to));
        assert!(bridge["arch_count"].as_u64().unwrap() > 1);
        for key in [
            "masonry",
            "dressing",
            "deck",
            "coarse_masonry",
            "coarse_deck",
            "collision",
        ] {
            let mesh = &bridge[key];
            let count = mesh["positions"].as_array().unwrap().len() / 3;
            assert!(count > 0, "{key}");
            assert_eq!(
                mesh["normals"].as_array().unwrap().len(),
                count * 3,
                "{key}"
            );
            assert_eq!(mesh["uvs"].as_array().unwrap().len(), count * 2, "{key}");
            assert!(
                mesh["indices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|index| index.as_u64().unwrap() < count as u64),
                "{key}"
            );
        }
        let diagnostics = result["diagnostics"].as_array().unwrap();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0]["crossing"], 141);
    }
}
