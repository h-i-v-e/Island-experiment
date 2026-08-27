//! Renderer-neutral botanical recipes, organ graphs, meshes and textures.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use motu::{Mesh, Vec3};

pub const LEAF_ARCHETYPE_COUNT: usize = 8;
pub const FOLIAGE_PAD_ARCHETYPE_COUNT: usize = 2;
pub const SHOOT_TIP_ARCHETYPE_COUNT: usize = 2;
pub const REPRODUCTIVE_ARCHETYPE_COUNT: usize = 2;
pub(super) const AXIS_POINTS: usize = 5;
const RECIPE_VERSION: u16 = 1;

/// Species architectures available through the shared botanical prototype.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BotanicalSpecies {
    #[default]
    Pohutukawa,
    Nikau,
    Harakeke,
}

impl BotanicalSpecies {
    pub const ALL: [Self; 3] = [Self::Pohutukawa, Self::Nikau, Self::Harakeke];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pohutukawa => "Pōhutukawa · mature tree",
            Self::Nikau => "Nīkau · mature palm",
            Self::Harakeke => "Harakeke · mature flax clump",
        }
    }

    #[must_use]
    pub const fn scientific_name(self) -> &'static str {
        match self {
            Self::Pohutukawa => "Metrosideros excelsa",
            Self::Nikau => "Rhopalostylis sapida",
            Self::Harakeke => "Phormium tenax",
        }
    }
}

/// Bounded physical controls for one mature procedural plant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotanicalRecipe {
    pub version: u16,
    pub species: BotanicalSpecies,
    pub trunk_height_metres: f32,
    pub trunk_radius_metres: f32,
    pub primary_count: u8,
    pub secondaries_per_primary: u8,
    pub terminals_per_secondary: u8,
    pub leaves_per_terminal: u8,
}

impl Default for BotanicalRecipe {
    fn default() -> Self {
        Self::for_species(BotanicalSpecies::Pohutukawa)
    }
}

impl BotanicalRecipe {
    #[must_use]
    pub const fn for_species(species: BotanicalSpecies) -> Self {
        match species {
            BotanicalSpecies::Pohutukawa => Self {
                version: RECIPE_VERSION,
                species,
                trunk_height_metres: 7.2,
                trunk_radius_metres: 0.58,
                primary_count: 9,
                secondaries_per_primary: 6,
                terminals_per_secondary: 8,
                leaves_per_terminal: 64,
            },
            BotanicalSpecies::Nikau => Self {
                version: RECIPE_VERSION,
                species,
                trunk_height_metres: 5.4,
                trunk_radius_metres: 0.24,
                primary_count: 17,
                secondaries_per_primary: 3,
                terminals_per_secondary: 3,
                leaves_per_terminal: 48,
            },
            BotanicalSpecies::Harakeke => Self {
                version: RECIPE_VERSION,
                species,
                trunk_height_metres: 2.35,
                trunk_radius_metres: 0.34,
                primary_count: 8,
                secondaries_per_primary: 3,
                terminals_per_secondary: 3,
                leaves_per_terminal: 9,
            },
        }
    }

    pub(super) fn validate(self) -> Result<Self, String> {
        if self.version != RECIPE_VERSION {
            return Err(format!(
                "unsupported botanical recipe version {}; expected {RECIPE_VERSION}",
                self.version
            ));
        }
        if !self.trunk_height_metres.is_finite()
            || !self.trunk_radius_metres.is_finite()
            || self.trunk_height_metres <= 0.0
            || self.trunk_radius_metres <= 0.0
            || self.trunk_radius_metres >= self.trunk_height_metres * 0.2
        {
            return Err("botanical trunk dimensions are outside their bounds".into());
        }
        let primary_is_bounded = match self.species {
            BotanicalSpecies::Pohutukawa => (5..=10).contains(&self.primary_count),
            BotanicalSpecies::Nikau => (12..=24).contains(&self.primary_count),
            BotanicalSpecies::Harakeke => (4..=16).contains(&self.primary_count),
        };
        let leaves_are_bounded = match self.species {
            BotanicalSpecies::Harakeke => (9..=18).contains(&self.leaves_per_terminal),
            BotanicalSpecies::Pohutukawa | BotanicalSpecies::Nikau => {
                (8..=64).contains(&self.leaves_per_terminal)
            }
        };
        if !primary_is_bounded
            || !(3..=8).contains(&self.secondaries_per_primary)
            || !(3..=8).contains(&self.terminals_per_secondary)
            || !leaves_are_bounded
        {
            return Err("botanical axis or leaf counts are outside their bounds".into());
        }
        Ok(self)
    }
}

/// One curved growth axis. Points and radii are fixed-size to keep the organ
/// graph compact and deterministic across renderers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub parent: Option<u32>,
    pub order: u8,
    pub points_metres: [Vec3; AXIS_POINTS],
    pub radii_metres: [f32; AXIS_POINTS],
    pub exposure: f32,
    pub alive: bool,
}

impl Axis {
    #[must_use]
    pub fn sample(self, fraction: f32) -> (Vec3, Vec3, f32) {
        let scaled = fraction.clamp(0.0, 1.0) * (AXIS_POINTS - 1) as f32;
        let segment = (scaled.floor() as usize).min(AXIS_POINTS - 2);
        let local = scaled - segment as f32;
        let a = self.points_metres[segment];
        let b = self.points_metres[segment + 1];
        let position = a.lerp(b, smoothstep(local));
        let tangent = (b - a).normalize_or(Vec3::Z);
        let start_radius = self.radii_metres[segment];
        let radius = (self.radii_metres[segment + 1] - start_radius).mul_add(local, start_radius);
        (position, tangent, radius)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AxisGraph {
    pub axes: Vec<Axis>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeafOrgan {
    pub axis: u32,
    pub blade_base_metres: Vec3,
    pub direction: Vec3,
    pub normal: Vec3,
    pub length_metres: f32,
    pub width_metres: f32,
    pub archetype: u8,
    pub age: f32,
    pub light_exposure: f32,
    pub variation: f32,
}

/// Retained state at the end of one generated fine shoot. Renderers instance a
/// bounded shared archetype; the organ carries only physical placement and
/// deterministic growth state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShootTipOrgan {
    pub axis: u32,
    pub base_metres: Vec3,
    pub direction: Vec3,
    pub length_metres: f32,
    pub radius_metres: f32,
    pub state: ShootTipState,
    pub variation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShootTipState {
    ActiveBud,
    DormantBud,
    Broken,
}

/// One bounded flowering or fruiting cluster attached below a palm crown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReproductiveOrgan {
    pub axis: u32,
    pub base_metres: Vec3,
    pub direction: Vec3,
    pub length_metres: f32,
    pub radius_metres: f32,
    pub state: ReproductiveState,
    pub variation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReproductiveState {
    Flower,
    Fruit,
}

/// One twig-local middle-distance instance derived from the terminal's real
/// leaf envelope. The extents use the pad's direction, normal, and transverse
/// frame rather than world axes. Renderers share exactly two porous meshes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoliagePad {
    pub axis: u32,
    pub centre_metres: Vec3,
    pub direction: Vec3,
    pub normal: Vec3,
    pub half_extents_metres: Vec3,
    pub archetype: u8,
    pub mean_age: f32,
    pub light_exposure: f32,
    pub density: f32,
    pub variation: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BotanicalTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Renderer-neutral bark state aligned one-to-one with a wood mesh's vertices.
/// Radius remains in physical metres; maturity is a bounded morphology proxy
/// derived from supporting radius and branch order rather than a material ID.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarkVertex {
    pub radius_metres: f32,
    pub maturity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BotanicalPrototype {
    pub species: BotanicalSpecies,
    pub graph: AxisGraph,
    pub wood: Mesh,
    pub wood_bark: Vec<BarkVertex>,
    /// One merged set of exposed break faces on bounded persistent dead stubs.
    /// The separate mesh allows a shared weathered-wood material without
    /// consuming the bark maturity channel or allocating one entity per scar.
    pub wood_scars: Mesh,
    /// Deterministic end-grain colour shared by every exposed break face.
    pub wood_scar_albedo: BotanicalTexture,
    /// One merged fine-shoot mesh derived from the terminal axes. Keeping this
    /// outside [`AxisGraph`] avoids turning visual microstructure into public
    /// growth topology or renderer entities.
    pub microtwigs: Mesh,
    pub microtwig_bark: Vec<BarkVertex>,
    pub leaf_archetypes: [Mesh; LEAF_ARCHETYPE_COUNT],
    pub shoot_tip_archetypes: [Mesh; SHOOT_TIP_ARCHETYPE_COUNT],
    pub reproductive_archetypes: [Mesh; REPRODUCTIVE_ARCHETYPE_COUNT],
    pub foliage_pad_archetypes: [Mesh; FOLIAGE_PAD_ARCHETYPE_COUNT],
    pub leaves: Vec<LeafOrgan>,
    pub shoot_tips: Vec<ShootTipOrgan>,
    pub reproductive_organs: Vec<ReproductiveOrgan>,
    pub foliage_pads: Vec<FoliagePad>,
    pub bark_albedo: BotanicalTexture,
    pub bark_normal: BotanicalTexture,
    pub bark_depth: BotanicalTexture,
    pub bark_metallic_roughness: BotanicalTexture,
    pub leaf_albedo: BotanicalTexture,
    pub leaf_metallic_roughness: BotanicalTexture,
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}
