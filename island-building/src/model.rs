//! Owned renderer-independent geometry and texture buffers.
use crate::{BuildingRecipe, collision::BoxCollider, layout::BuildingLayout};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialSlot {
    Wall,
    Roof,
    ThatchEnd,
    ThatchShell1,
    ThatchShell2,
    ThatchShell3,
    ThatchShell4,
    Floor,
    Interior,
    Trim,
    Door,
    Hardware,
    Chimney,
    Firebox,
    Glass,
    WindowProxy,
    Atlas,
}
impl MaterialSlot {
    /// Four masked cut-end layers, ordered from straw roots to tips.
    pub const THATCH_SHELLS: [Self; 4] = [
        Self::ThatchShell1,
        Self::ThatchShell2,
        Self::ThatchShell3,
        Self::ThatchShell4,
    ];

    #[must_use]
    pub const fn thatch_shell_layer(self) -> Option<u8> {
        match self {
            Self::ThatchShell1 => Some(0),
            Self::ThatchShell2 => Some(1),
            Self::ThatchShell3 => Some(2),
            Self::ThatchShell4 => Some(3),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BuildingMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}
impl BuildingMesh {
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MeshPart {
    pub material: MaterialSlot,
    pub mesh: BuildingMesh,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BuildingLod {
    pub parts: Vec<MeshPart>,
}
impl BuildingLod {
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.parts.iter().map(|p| p.mesh.triangle_count()).sum()
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DoorDescriptor {
    pub id: String,
    pub hinge: [f32; 3],
    pub closed_angle: f32,
    pub swing_angle: f32,
    pub width: f32,
    pub height: f32,
    pub thickness: f32,
    pub mesh: BuildingMesh,
    /// Untextured metal knobs and roses in the same hinge-local frame as the leaf.
    #[serde(default)]
    pub hardware_mesh: BuildingMesh,
    pub exterior: bool,
}
/// Images are tightly packed row-major; albedo RGBA8 sRGB, normals RGBA8 linear, masks RGBA8 (AO/roughness/metallic/255).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BakedAtlas {
    pub size: u32,
    pub albedo: Vec<u8>,
    pub normal: Vec<u8>,
    pub masks: Vec<u8>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MaterialTexture {
    pub slot: MaterialSlot,
    pub size: u32,
    pub albedo: Vec<u8>,
    pub normal: Vec<u8>,
    pub masks: Vec<u8>,
    pub physical_size: [f32; 2],
    pub source_hash: String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BuildingPrototype {
    pub recipe: BuildingRecipe,
    pub layout: BuildingLayout,
    pub lods: [BuildingLod; 3],
    pub doors: Vec<DoorDescriptor>,
    pub colliders: Vec<BoxCollider>,
    pub atlas: BakedAtlas,
    pub materials: Vec<MaterialTexture>,
    pub hash: String,
}
