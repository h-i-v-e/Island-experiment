//! Typed, versioned building authoring recipes. Dimensions are metres, Z is up.
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildingRecipe {
    pub schema_version: u32,
    pub name: String,
    pub seed: u64,
    pub width: f32,
    pub depth: f32,
    pub wall_height: f32,
    pub wall_thickness: f32,
    pub floor_thickness: f32,
    pub roof: RoofRecipe,
    pub openings: Vec<OpeningRecipe>,
    pub partitions: Vec<PartitionRecipe>,
    /// One wall-attached, unlit fireplace and its roof-penetrating chimney.
    #[serde(default)]
    pub chimney: Option<ChimneyRecipe>,
    pub materials: MaterialPalette,
    pub atlas_size: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Facade {
    South,
    East,
    North,
    West,
}
impl Facade {
    pub const ALL: [Self; 4] = [Self::South, Self::East, Self::North, Self::West];
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::South => 0,
            Self::East => 1,
            Self::North => 2,
            Self::West => 3,
        }
    }
    #[must_use]
    pub fn length(self, recipe: &BuildingRecipe) -> f32 {
        match self {
            Self::South | Self::North => recipe.width,
            Self::East | Self::West => recipe.depth,
        }
    }
    /// Outward surface normal.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        match self {
            Self::South => [0., -1., 0.],
            Self::East => [1., 0., 0.],
            Self::North => [0., 1., 0.],
            Self::West => [-1., 0., 0.],
        }
    }
    /// Local horizontal axis: looking at the wall from outside, this points right.
    #[must_use]
    pub const fn tangent(self) -> [f32; 3] {
        match self {
            Self::South => [1., 0., 0.],
            Self::East => [0., 1., 0.],
            Self::North => [-1., 0., 0.],
            Self::West => [0., -1., 0.],
        }
    }
    #[must_use]
    pub fn point(self, recipe: &BuildingRecipe, along: f32, height: f32, inward: f32) -> [f32; 3] {
        let n = self.normal();
        let t = self.tangent();
        let half = match self {
            Self::South | Self::North => recipe.depth * 0.5,
            Self::East | Self::West => recipe.width * 0.5,
        };
        [
            n[0] * (half - inward) + t[0] * along,
            n[1] * (half - inward) + t[1] * along,
            height,
        ]
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    Door,
    Window,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HingeSide {
    Left,
    Right,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningRecipe {
    pub id: String,
    pub facade: Facade,
    pub kind: OpeningKind,
    /// Centre offset along the facade's tangent, measured from its midpoint.
    pub offset: f32,
    pub width: f32,
    pub height: f32,
    pub sill: f32,
    pub hinge: HingeSide,
    pub swing_degrees: f32,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofKind {
    Gable,
    Shed,
}
/// Surface covering, independently of the structural roof shape.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofCovering {
    #[default]
    StandingSeam,
    Slate,
    Thatch,
}
impl RoofCovering {
    /// Maximum relief above the main roof shell, in metres.
    #[must_use]
    pub const fn detail_height(self) -> f32 {
        match self {
            Self::StandingSeam => 0.018,
            Self::Slate => 0.025,
            Self::Thatch => 0.22,
        }
    }
    #[must_use]
    pub const fn default_colour(self) -> [f32; 3] {
        match self {
            Self::StandingSeam => [0.23, 0.28, 0.29],
            Self::Slate => [0.11, 0.13, 0.16],
            Self::Thatch => [0.27, 0.205, 0.12],
        }
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoofRecipe {
    pub kind: RoofKind,
    #[serde(default)]
    pub covering: RoofCovering,
    pub rise: f32,
    pub overhang: f32,
    pub thickness: f32,
}
impl RoofRecipe {
    /// Covering depth grows upwards, preserving the structural underside.
    #[must_use]
    pub fn effective_thickness(&self) -> f32 {
        match self.covering {
            RoofCovering::Thatch => self.thickness.max(0.45),
            RoofCovering::StandingSeam | RoofCovering::Slate => self.thickness,
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionAxis {
    X,
    Y,
}
/// A complete internal cross-wall. The doorway is always open in the initial release.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionRecipe {
    pub id: String,
    pub axis: PartitionAxis,
    pub offset: f32,
    pub door_offset: f32,
    pub door_width: f32,
    pub door_height: f32,
}
/// Dimensions of one wall-attached fireplace and chimney, all in metres.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChimneyRecipe {
    pub facade: Facade,
    pub offset: f32,
    pub width: f32,
    pub depth: f32,
    pub firebox_width: f32,
    pub firebox_height: f32,
    pub firebox_depth: f32,
    /// Hearth projection beyond the front of the fireplace breast.
    pub hearth_depth: f32,
    pub hearth_height: f32,
    pub stack_width: f32,
    pub stack_depth: f32,
    /// Clearance above the highest roof point under the complete stack footprint.
    pub height_above_roof: f32,
}
impl Default for ChimneyRecipe {
    fn default() -> Self {
        Self {
            facade: Facade::North,
            offset: 0.,
            width: 1.6,
            depth: 0.6,
            firebox_width: 1.0,
            firebox_height: 0.85,
            firebox_depth: 0.42,
            hearth_depth: 0.3,
            hearth_height: 0.12,
            stack_width: 0.65,
            stack_depth: 0.5,
            height_above_roof: 0.8,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialPalette {
    pub wall: [f32; 3],
    pub roof: [f32; 3],
    pub floor: [f32; 3],
    pub interior: [f32; 3],
    pub trim: [f32; 3],
    pub door: [f32; 3],
    pub glass: [f32; 4],
    #[serde(default = "default_chimney_colour")]
    pub chimney: [f32; 3],
    /// Optional material recipe paths, resolved relative to the building source file.
    #[serde(default)]
    pub wall_recipe: Option<String>,
    #[serde(default)]
    pub roof_recipe: Option<String>,
    #[serde(default)]
    pub door_recipe: Option<String>,
    #[serde(default)]
    pub chimney_recipe: Option<String>,
}
impl Default for MaterialPalette {
    fn default() -> Self {
        Self {
            wall: [0.62, 0.38, 0.22],
            roof: [0.23, 0.28, 0.29],
            floor: [0.38, 0.27, 0.17],
            interior: [0.76, 0.72, 0.61],
            trim: [0.82, 0.76, 0.61],
            door: [0.30, 0.20, 0.12],
            glass: [0.65, 0.82, 0.88, 0.16],
            chimney: default_chimney_colour(),
            wall_recipe: None,
            roof_recipe: None,
            door_recipe: None,
            chimney_recipe: None,
        }
    }
}
const fn default_chimney_colour() -> [f32; 3] {
    [0.255, 0.25, 0.23]
}

impl Default for BuildingRecipe {
    fn default() -> Self {
        Self::hut()
    }
}
impl BuildingRecipe {
    #[must_use]
    pub fn hut() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: "Coastal hut".into(),
            seed: 42,
            width: 6.,
            depth: 5.,
            wall_height: 2.8,
            wall_thickness: 0.2,
            floor_thickness: 0.16,
            roof: RoofRecipe {
                covering: RoofCovering::StandingSeam,
                kind: RoofKind::Gable,
                rise: 1.4,
                overhang: 0.35,
                thickness: 0.12,
            },
            openings: vec![
                OpeningRecipe {
                    id: "entrance".into(),
                    facade: Facade::South,
                    kind: OpeningKind::Door,
                    offset: 0.,
                    width: 1.15,
                    height: 2.2,
                    sill: 0.,
                    hinge: HingeSide::Left,
                    swing_degrees: 105.,
                },
                OpeningRecipe {
                    id: "window-east".into(),
                    facade: Facade::East,
                    kind: OpeningKind::Window,
                    offset: 0.,
                    width: 1.4,
                    height: 1.1,
                    sill: 0.95,
                    hinge: HingeSide::Left,
                    swing_degrees: 90.,
                },
                OpeningRecipe {
                    id: "window-west".into(),
                    facade: Facade::West,
                    kind: OpeningKind::Window,
                    offset: 0.,
                    width: 1.4,
                    height: 1.1,
                    sill: 0.95,
                    hinge: HingeSide::Left,
                    swing_degrees: 90.,
                },
            ],
            partitions: Vec::new(),
            chimney: Some(ChimneyRecipe::default()),
            materials: MaterialPalette::default(),
            atlas_size: 512,
        }
    }
    #[must_use]
    pub fn cottage() -> Self {
        let mut r = Self::hut();
        r.name = "Island cottage".into();
        r.width = 9.;
        r.depth = 7.;
        r.roof.rise = 2.;
        r.chimney.as_mut().expect("hut chimney").offset = -2.5;
        r.openings[1].offset = -1.5;
        r.openings[2].offset = 1.5;
        r.partitions.push(PartitionRecipe {
            id: "cross-wall".into(),
            axis: PartitionAxis::X,
            offset: 0.7,
            door_offset: 0.,
            door_width: 1.1,
            door_height: 2.2,
        });
        r.openings.extend([
            OpeningRecipe {
                id: "front-window".into(),
                facade: Facade::South,
                kind: OpeningKind::Window,
                offset: 2.7,
                width: 1.3,
                height: 1.1,
                sill: 0.95,
                hinge: HingeSide::Left,
                swing_degrees: 90.,
            },
            OpeningRecipe {
                id: "rear-window".into(),
                facade: Facade::North,
                kind: OpeningKind::Window,
                offset: 0.,
                width: 1.4,
                height: 1.1,
                sill: 0.95,
                hinge: HingeSide::Left,
                swing_degrees: 90.,
            },
        ]);
        r
    }
    #[must_use]
    pub fn workshop() -> Self {
        let mut r = Self::hut();
        r.name = "Harbour workshop".into();
        r.width = 10.;
        r.depth = 7.;
        r.wall_height = 3.4;
        r.roof.kind = RoofKind::Shed;
        r.roof.rise = 1.2;
        r.materials.wall = [0.49, 0.54, 0.49];
        r.openings[0].width = 1.5;
        r.openings[0].height = 2.6;
        r
    }
}
