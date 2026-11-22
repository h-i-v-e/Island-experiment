//! Unity-friendly JSON encoding without copying source meshes or raw image data.
use crate::{
    BakedAtlas, BuildingMesh, BuildingPrototype, MaterialSlot, MaterialTexture,
    collision::BoxCollider,
    decoration_anchors::DecorationAnchorPoses,
    town::{
        BridgeCandidate, ClearanceRect, TownFrontage, TownOutput, TownRoadExit, TownSettlementKind,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Serialize, Serializer};
use std::collections::BTreeSet;

struct Flat<'a, const N: usize>(&'a [[f32; N]]);
impl<const N: usize> Serialize for Flat<'_, N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().flatten())
    }
}
#[derive(Serialize)]
struct Mesh<'a> {
    positions: Flat<'a, 3>,
    normals: Flat<'a, 3>,
    uvs: Flat<'a, 2>,
    indices: &'a [u32],
    grub_positions: Flat<'a, 3>,
}
fn mesh<'a>(source: &'a BuildingMesh, grub_positions: &'a [[f32; 3]]) -> Mesh<'a> {
    Mesh {
        positions: Flat(&source.positions),
        normals: Flat(&source.normals),
        uvs: Flat(&source.uvs),
        indices: &source.indices,
        grub_positions: Flat(grub_positions),
    }
}
#[derive(Serialize)]
struct Part<'a> {
    material: MaterialSlot,
    mesh: Mesh<'a>,
}
#[derive(Serialize)]
struct Lod<'a> {
    parts: Vec<Part<'a>>,
}
#[derive(Serialize)]
struct Material {
    slot: MaterialSlot,
    size: u32,
    physical_size: [f32; 2],
    colour: [f32; 4],
    albedo_png: String,
    normal_png: String,
    masks_png: String,
    grub_uniform: Vec<f32>,
}
#[derive(Serialize)]
struct Door<'a> {
    id: &'a str,
    hinge: [f32; 3],
    closed_angle: f32,
    swing_angle: f32,
    width: f32,
    height: f32,
    thickness: f32,
    depth_offset: f32,
    exterior: bool,
    collision_sections: &'a [BoxCollider],
    mesh: Mesh<'a>,
    hardware_mesh: Mesh<'a>,
}
#[derive(Serialize)]
struct Prototype<'a> {
    entry_id: &'a str,
    hash: &'a str,
    is_church: bool,
    lods: [Lod<'a>; 3],
    doors: Vec<Door<'a>>,
    decoration_anchors: DecorationAnchorPoses<'a>,
    colliders: &'a [BoxCollider],
    support_colliders: &'a [BoxCollider],
    materials: Vec<Material>,
    atlas: Material,
}
#[derive(Serialize)]
struct Instance<'a> {
    prototype_index: usize,
    entry_id: &'a str,
    settlement_index: usize,
    position: [f32; 3],
    yaw_radians: f32,
    frontage: Option<&'a TownFrontage>,
}
#[derive(Serialize)]
struct Settlement<'a> {
    kind: TownSettlementKind,
    river_index: usize,
    mouth: [f32; 2],
    centre: &'a [f32],
    building_count: usize,
    skipped_count: u32,
}
#[derive(Serialize)]
struct Package<'a> {
    schema_version: u32,
    seed: String,
    mouth: Vec<f32>,
    settlements: Vec<Settlement<'a>>,
    diagnostics: Vec<String>,
    prototypes: Vec<Prototype<'a>>,
    instances: Vec<Instance<'a>>,
    supports: Mesh<'a>,
    support_material: Option<Material>,
    support_colliders: &'a [BoxCollider],
    stair_colliders: &'a [BoxCollider],
    stair_feet: &'a [motu::TownStairFoot],
    footpath_blockers: &'a [ClearanceRect],
    clearance_json: String,
    wheat_fields: &'a [ClearanceRect],
    routing_blockers: &'a [ClearanceRect],
    roads_json: String,
    road_exits: &'a [TownRoadExit],
    road_mesh: Mesh<'a>,
    road_count: usize,
    bridge_candidates: &'a [BridgeCandidate],
    pier_layout_json: String,
}

pub(super) fn encode(output: &TownOutput) -> Result<String, String> {
    let prototypes = output
        .prototypes
        .iter()
        .map(|p| prototype(&p.entry_id, &p.building))
        .collect::<Result<_, _>>()?;
    let support_material = output
        .support_material
        .as_ref()
        .map(|source| {
            texture(
                source,
                [1.; 4],
                crate::foundation_stone::grub_filter()
                    .uniform_vectors(crate::foundation_stone::SEED)
                    .into_iter()
                    .flatten()
                    .collect(),
            )
        })
        .transpose()?;
    let road_mesh = output
        .roads
        .as_ref()
        .map(motu::roads::RoadNetwork::mesh)
        .unwrap_or_default();
    let road_count = output.roads.as_ref().map_or(0, |roads| roads.strips.len());
    let roads_json = output
        .roads
        .as_ref()
        .filter(|_| road_count > 0)
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let package = Package {
        schema_version: 2,
        seed: output.tile_seed.to_string(),
        mouth: output
            .mouth
            .map_or_else(Vec::new, |mouth| mouth.position.to_vec()),
        settlements: output
            .settlements
            .iter()
            .map(|site| Settlement {
                kind: site.kind,
                river_index: site.river_index,
                mouth: site.mouth,
                centre: site.centre.as_ref().map_or(&[], |centre| centre.as_slice()),
                building_count: site.building_count,
                skipped_count: site.skipped_count,
            })
            .collect(),
        diagnostics: vec![format!(
            "Placed {} buildings and {} road sections. Skipped {} requested copies.",
            output.instances.len(),
            road_count,
            output.skipped_count
        )],
        prototypes,
        instances: output
            .instances
            .iter()
            .map(|p| Instance {
                prototype_index: p.prototype_index,
                entry_id: &p.entry_id,
                settlement_index: p.settlement_index,
                position: p.position,
                yaw_radians: p.yaw,
                frontage: p.frontage.as_ref(),
            })
            .collect(),
        supports: mesh(&output.supports, &output.supports_grub_positions),
        support_material,
        support_colliders: &output.support_colliders,
        stair_colliders: &output.stair_colliders,
        stair_feet: &output.stair_feet,
        footpath_blockers: &output.footpath_blockers,
        clearance_json: serde_json::to_string(&output.clearance).map_err(|e| e.to_string())?,
        wheat_fields: &output.wheat_fields,
        routing_blockers: &output.routing_blockers,
        roads_json,
        road_exits: &output.road_exits,
        road_mesh: Mesh {
            positions: Flat(&road_mesh.positions),
            normals: Flat(&road_mesh.normals),
            uvs: Flat(&road_mesh.uvs),
            indices: &road_mesh.indices,
            grub_positions: Flat(&[]),
        },
        road_count,
        bridge_candidates: &output.bridge_candidates,
        pier_layout_json: output
            .pier_layout
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default(),
    };
    serde_json::to_string(&package).map_err(|e| e.to_string())
}

fn prototype<'a>(
    entry_id: &'a str,
    source: &'a BuildingPrototype,
) -> Result<Prototype<'a>, String> {
    let slots: BTreeSet<_> = source
        .lods
        .iter()
        .flat_map(|lod| lod.parts.iter().map(|part| part.material.texture_slot()))
        .chain([MaterialSlot::Door, MaterialSlot::Hardware])
        .filter(|slot| *slot != MaterialSlot::Atlas)
        .collect();
    let materials = slots
        .into_iter()
        .map(|slot| material(source, slot))
        .collect::<Result<_, _>>()?;
    Ok(Prototype {
        entry_id,
        hash: &source.hash,
        is_church: source.recipe.church.is_some(),
        lods: std::array::from_fn(|i| Lod {
            parts: source.lods[i]
                .parts
                .iter()
                .map(|part| Part {
                    material: part.material.texture_slot(),
                    mesh: mesh(&part.mesh, &[]),
                })
                .collect(),
        }),
        doors: source
            .doors
            .iter()
            .map(|door| Door {
                id: &door.id,
                hinge: door.hinge,
                closed_angle: door.closed_angle,
                swing_angle: door.swing_angle,
                width: door.width,
                height: door.height,
                thickness: door.thickness,
                depth_offset: door.depth_offset,
                exterior: door.exterior,
                collision_sections: &door.collision_sections,
                mesh: mesh(&door.mesh, &[]),
                hardware_mesh: mesh(&door.hardware_mesh, &[]),
            })
            .collect(),
        decoration_anchors: DecorationAnchorPoses(&source.recipe.decoration_anchors),
        colliders: &source.colliders,
        support_colliders: &source.support_colliders,
        materials,
        atlas: atlas(&source.atlas)?,
    })
}

pub(super) fn church_stone() -> Result<String, String> {
    let recipe = crate::BuildingRecipe::parish_church();
    let slot = MaterialSlot::Wall;
    let maps = crate::materials::material(
        &recipe,
        std::path::Path::new("."),
        slot,
        recipe.materials.wall,
        recipe.materials.wall_recipe.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    let material = texture(&maps, [1.; 4], grub_uniform(&recipe, slot))?;
    serde_json::to_string(&material).map_err(|error| error.to_string())
}

fn grub_uniform(recipe: &crate::BuildingRecipe, slot: MaterialSlot) -> Vec<f32> {
    crate::grub::material_filter(recipe, slot).map_or_else(Vec::new, |(filter, seed)| {
        filter.uniform_vectors(seed).into_iter().flatten().collect()
    })
}

fn material(source: &BuildingPrototype, slot: MaterialSlot) -> Result<Material, String> {
    let uniform = grub_uniform(&source.recipe, slot);
    if let Some(image) = source.materials.iter().find(|image| image.slot == slot) {
        texture(image, [1.; 4], uniform)
    } else {
        Ok(Material {
            slot,
            size: 0,
            physical_size: [1.; 2],
            colour: crate::materials::colour(&source.recipe, slot),
            albedo_png: String::new(),
            normal_png: String::new(),
            masks_png: String::new(),
            grub_uniform: uniform,
        })
    }
}
fn texture(
    source: &MaterialTexture,
    colour: [f32; 4],
    grub_uniform: Vec<f32>,
) -> Result<Material, String> {
    Ok(Material {
        slot: source.slot,
        size: source.size,
        physical_size: source.physical_size,
        colour,
        albedo_png: png(source.size, &source.albedo)?,
        normal_png: png(source.size, &source.normal)?,
        masks_png: png(source.size, &source.masks)?,
        grub_uniform,
    })
}
fn atlas(source: &BakedAtlas) -> Result<Material, String> {
    Ok(Material {
        slot: MaterialSlot::Atlas,
        size: source.size,
        physical_size: [2.; 2],
        colour: [1.; 4],
        albedo_png: png(source.size, &source.albedo)?,
        normal_png: png(source.size, &source.normal)?,
        masks_png: png(source.size, &source.masks)?,
        grub_uniform: Vec::new(),
    })
}
fn png(size: u32, pixels: &[u8]) -> Result<String, String> {
    let mut image_bytes = Vec::new();
    {
        let mut writer = png::Encoder::new(&mut image_bytes, size, size);
        writer.set_color(png::ColorType::Rgba);
        writer.set_depth(png::BitDepth::Eight);
        writer
            .write_header()
            .map_err(|e| e.to_string())?
            .write_image_data(pixels)
            .map_err(|e| e.to_string())?;
    }
    Ok(STANDARD.encode(image_bytes))
}
