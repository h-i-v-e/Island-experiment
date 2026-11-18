//! Deterministic, floor-aligned internal U-stairs and their placement metadata.
use serde::{Deserialize, Serialize};
mod enclosure;
mod entrances;
mod placement;
mod windows;
pub(crate) use windows::fit as fit_windows;

use crate::{BuildingError, BuildingRecipe, Facade, collision::BoxCollider};

pub const TREAD_THICKNESS: f32 = 0.07;
pub const RAIL_HEIGHT: f32 = 0.95;
pub const FLIGHT_GAP: f32 = 0.12;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StairAxis {
    #[default]
    Auto,
    X,
    Y,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StaircaseRecipe {
    /// Enclose the flights in an interior stairwell with one door per floor.
    pub enclosed: bool,
    pub door_style: crate::DoorStyle,
    /// A selected wall sets the flight axis and leaves the position along it automatic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall: Option<Facade>,
    pub axis: StairAxis,
    /// None selects a clear position automatically. Metres from the building centre.
    pub position: Option<[f32; 2]>,
    pub width: f32,
    pub going: f32,
    pub target_riser: f32,
    pub landing_depth: f32,
}

impl Default for StaircaseRecipe {
    fn default() -> Self {
        Self {
            enclosed: true,
            door_style: crate::DoorStyle::FrameAndPanel,
            wall: None,
            axis: StairAxis::Auto,
            position: None,
            width: 1.,
            going: 0.24,
            target_riser: 0.19,
            landing_depth: 1.,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StaircaseLayout {
    /// Auto placement may turn the flights 180 degrees along the chosen axis.
    #[serde(default)]
    pub reversed: bool,
    #[serde(default)]
    pub stairwell: Option<StairwellLayout>,
    pub axis: StairAxis,
    pub position: [f32; 2],
    pub width: f32,
    pub going: f32,
    pub riser: f32,
    pub risers_per_flight: u16,
    pub landing_depth: f32,
    pub floor_heights: Vec<f32>,
    pub landings: Vec<BoxCollider>,
    pub turn_landings: Vec<BoxCollider>,
    /// Floors and joists are cut here; the near strip is filled by each landing.
    pub opening: BoxCollider,
    pub bounds: BoxCollider,
}

/// One continuous stair room, with a door opening onto each floor landing.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StairwellLayout {
    pub bounds: BoxCollider,
    #[serde(default)]
    pub enclosure: Option<StairwellEnclosure>,
    pub wall_thickness: f32,
    pub door_width: f32,
    pub door_height: f32,
    pub doorways: Vec<StairwellDoorway>,
}

/// Sides in stair-local coordinates, ordered near, right, far, left.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StairwellSide {
    #[default]
    Near,
    Right,
    Far,
    Left,
}

impl StairwellSide {
    pub const ALL: [Self; 4] = [Self::Near, Self::Right, Self::Far, Self::Left];
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Near => 0,
            Self::Right => 1,
            Self::Far => 2,
            Self::Left => 3,
        }
    }
    #[must_use]
    pub const fn outward(self) -> [f32; 2] {
        match self {
            Self::Near => [0., -1.],
            Self::Right => [1., 0.],
            Self::Far => [0., 1.],
            Self::Left => [-1., 0.],
        }
    }
    #[must_use]
    pub const fn tangent(self) -> [f32; 2] {
        let [x, y] = self.outward();
        [-y, x]
    }
    #[must_use]
    pub const fn across(self) -> bool {
        matches!(self, Self::Near | Self::Far)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct StairwellEnclosure {
    /// Clear room limits in stair-local across/along metres.
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Near, right, far, left. True reuses the corresponding external wall.
    pub shared_sides: [bool; 4],
    pub merge_unit: f32,
    pub door_side: StairwellSide,
    pub door_offset: f32,
}

impl StairwellEnclosure {
    #[must_use]
    pub fn surface(self, side: StairwellSide, offset: f32) -> [f32; 2] {
        match side {
            StairwellSide::Near => [offset, self.min[1]],
            StairwellSide::Right => [self.max[0], offset],
            StairwellSide::Far => [offset, self.max[1]],
            StairwellSide::Left => [self.min[0], offset],
        }
    }
    pub(crate) fn original(stairs: &StaircaseLayout) -> Self {
        Self {
            min: [0.; 2],
            max: [stairs.span(), stairs.length()],
            shared_sides: [false; 4],
            merge_unit: 0.,
            door_side: StairwellSide::Near,
            door_offset: stairs.span() * 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct StairwellDoorway {
    pub storey: u8,
    pub centre: [f32; 3],
    pub room_approach: [f32; 3],
    pub stair_approach: [f32; 3],
}

impl StaircaseLayout {
    #[must_use]
    pub fn run(&self) -> f32 {
        f32::from(self.risers_per_flight) * self.going
    }
    #[must_use]
    pub fn length(&self) -> f32 {
        self.run() + self.landing_depth * 2.
    }
    #[must_use]
    pub fn span(&self) -> f32 {
        self.width * 2. + FLIGHT_GAP
    }
    #[must_use]
    pub fn point(&self, across: f32, along: f32, height: f32) -> [f32; 3] {
        let sign = if self.reversed { -1. } else { 1. };
        let u = (across - self.span() * 0.5) * sign;
        let v = (along - self.length() * 0.5) * sign;
        match self.axis {
            StairAxis::X => [self.position[0] + v, self.position[1] - u, height],
            StairAxis::Auto | StairAxis::Y => [self.position[0] + u, self.position[1] + v, height],
        }
    }
    #[must_use]
    pub fn coordinates(&self, point: [f32; 3]) -> [f32; 3] {
        let sign = if self.reversed { -1. } else { 1. };
        let x = (point[0] - self.position[0]) * sign;
        let y = (point[1] - self.position[1]) * sign;
        match self.axis {
            StairAxis::X => [-y + self.span() * 0.5, x + self.length() * 0.5, point[2]],
            StairAxis::Auto | StairAxis::Y => {
                [x + self.span() * 0.5, y + self.length() * 0.5, point[2]]
            }
        }
    }
    #[must_use]
    pub fn rectangle(&self, min: [f32; 3], max: [f32; 3]) -> BoxCollider {
        let a = self.point(min[0], min[1], min[2]);
        let b = self.point(max[0], max[1], max[2]);
        BoxCollider {
            min: std::array::from_fn(|i| a[i].min(b[i])),
            max: std::array::from_fn(|i| a[i].max(b[i])),
        }
    }
}

pub fn resolve(recipe: &BuildingRecipe) -> Result<Option<StaircaseLayout>, BuildingError> {
    resolve_with_base(recipe, std::path::Path::new("."))
}

pub fn resolve_with_base(
    recipe: &BuildingRecipe,
    base: &std::path::Path,
) -> Result<Option<StaircaseLayout>, BuildingError> {
    let resolved = crate::openings::resolve(recipe)?;
    Ok(resolve_placement(resolved.as_ref(), base)?.map(|p| p.layout))
}

pub(crate) struct Placement {
    pub layout: StaircaseLayout,
    pub entrance_offsets: Vec<f32>,
}

pub(crate) fn resolve_placement(
    recipe: &BuildingRecipe,
    base: &std::path::Path,
) -> Result<Option<Placement>, BuildingError> {
    if !(1..=6).contains(&recipe.storeys)
        || !recipe.wall_height.is_finite()
        || !recipe.floor_thickness.is_finite()
        || !(2.4..=8.).contains(&recipe.wall_height)
        || !(0.05..=0.5).contains(&recipe.floor_thickness)
    {
        return Err(BuildingError::new(
            "storeys",
            "stair generation requires one to six storeys and valid wall/floor heights",
        ));
    }
    check_settings(&recipe.staircase)?;
    check_enclosure(recipe)?;
    if recipe.storeys == 1 {
        return Ok(None);
    }
    if let Some(wall) = recipe.staircase.wall
        && recipe
            .chimney
            .as_ref()
            .is_some_and(|chimney| chimney.facade == wall)
    {
        return Err(BuildingError::new(
            "staircase.wall",
            format!(
                "The {wall:?} wall is reserved for the fireplace. Choose another staircase wall or move the fireplace."
            ),
        ));
    }
    let merge_unit = if recipe.staircase.enclosed {
        crate::units::MaterialGrid::load(recipe, base, crate::MaterialSlot::Wall)?.metres[0]
    } else {
        0.
    };
    let mut entrances = entrances::Entrances::new(recipe);
    for candidate in placement::candidates(recipe) {
        for reversed in [false, true] {
            let candidate = layout(
                recipe,
                candidate.axis,
                candidate.position,
                merge_unit,
                reversed,
            );
            let Some(shapes) = clear(recipe, &candidate) else {
                continue;
            };
            if entrances.fit(recipe, &shapes) {
                return Ok(Some(Placement {
                    layout: candidate,
                    entrance_offsets: entrances.offsets(),
                }));
            }
        }
    }
    if let Some(wall) = recipe.staircase.wall {
        return Err(BuildingError::new(
            "staircase.wall",
            format!(
                "No staircase and door approach fit along the {wall:?} wall. Choose another wall or adjust the footprint, stairs or nearby openings."
            ),
        ));
    }
    Err(BuildingError::new(
        "staircase",
        "no clear staircase and door approach fit: enlarge the footprint, move partitions/fireplace or adjust the stair position/orientation",
    ))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Validated height/riser bounds give at most 43 risers per flight.
fn layout(
    recipe: &BuildingRecipe,
    axis: StairAxis,
    position: [f32; 2],
    merge_unit: f32,
    reversed: bool,
) -> StaircaseLayout {
    let settings = recipe.staircase;
    let pitch = recipe.floor_height(1);
    let risers = (pitch * 0.5 / settings.target_riser).ceil().max(1.) as u16;
    let empty = BoxCollider {
        min: [0.; 3],
        max: [0.; 3],
    };
    let mut result = StaircaseLayout {
        reversed,
        stairwell: None,
        axis,
        position,
        width: settings.width,
        going: settings.going,
        riser: pitch * 0.5 / f32::from(risers),
        risers_per_flight: risers,
        landing_depth: settings.landing_depth,
        floor_heights: (0..recipe.storeys)
            .map(|i| recipe.floor_height(i))
            .collect(),
        landings: Vec::new(),
        turn_landings: Vec::new(),
        opening: empty,
        bounds: empty,
    };
    result.opening = result.rectangle(
        [0., 0., 0.],
        [result.span(), result.length(), recipe.eaves_height()],
    );
    result.bounds = result.opening;
    result.bounds.min[2] = -TREAD_THICKNESS;
    result.bounds.max[2] = recipe.floor_height(recipe.storeys - 1) + RAIL_HEIGHT;
    for &height in &result.floor_heights {
        result.landings.push(result.rectangle(
            [0., 0., height - TREAD_THICKNESS],
            [result.span(), result.landing_depth, height],
        ));
    }
    for &height in result
        .floor_heights
        .iter()
        .take(usize::from(recipe.storeys - 1))
    {
        let z = height + pitch * 0.5;
        result.turn_landings.push(result.rectangle(
            [0., result.landing_depth + result.run(), z - TREAD_THICKNESS],
            [result.span(), result.length(), z],
        ));
    }
    if recipe.staircase.enclosed {
        result.stairwell = enclosure::resolve(recipe, &result, merge_unit);
    }
    result
}

fn clear(recipe: &BuildingRecipe, layout: &StaircaseLayout) -> Option<Vec<BoxCollider>> {
    if recipe.staircase.enclosed && layout.stairwell.is_none() {
        return None;
    }
    let reserved = layout
        .stairwell
        .as_ref()
        .map_or(layout.opening, |s| s.bounds);
    if !keeps_chimney_wall_clear(recipe, reserved) {
        return None;
    }
    if recipe.partitions.iter().any(|p| {
        let axis = usize::from(p.axis == crate::PartitionAxis::X);
        p.offset + recipe.wall_thickness * 0.5 > reserved.min[axis]
            && p.offset - recipe.wall_thickness * 0.5 < reserved.max[axis]
    }) {
        return None;
    }
    let mut shapes = crate::geometry::internal_stairs::collision(recipe, layout);
    shapes.extend(crate::geometry::stairwell::walls(recipe, layout));
    let internal_doors = crate::geometry::stairwell::doors(recipe, layout);
    if crate::validation::validate_door_swings(&internal_doors, &shapes).is_err() {
        return None;
    }
    if layout.stairwell.as_ref().is_some_and(|room| {
        room.doorways.iter().any(|d| {
            let p = d.room_approach;
            p[0].abs() > recipe.width * 0.5 - recipe.wall_thickness - 0.3
                || p[1].abs() > recipe.depth * 0.5 - recipe.wall_thickness - 0.3
        })
    }) {
        return None;
    }
    if let Some(chimney) = &recipe.chimney {
        let a = chimney.facade.point(
            recipe,
            chimney.offset - chimney.width * 0.5 - 0.1,
            0.,
            recipe.wall_thickness,
        );
        let b = chimney.facade.point(
            recipe,
            chimney.offset + chimney.width * 0.5 + 0.1,
            recipe.wall_height,
            recipe.wall_thickness + chimney.depth + 0.05,
        );
        let breast = BoxCollider {
            min: std::array::from_fn(|i| a[i].min(b[i])),
            max: std::array::from_fn(|i| a[i].max(b[i])),
        };
        let shaft = crate::geometry::chimney::stack_bounds(recipe).expect("enabled chimney");
        if shapes.iter().any(|s| overlaps(*s, shaft))
            || (0..recipe.storeys).any(|storey| {
                let floor = recipe.floor_height(storey);
                let mut breast = breast;
                breast.min[2] += floor;
                breast.max[2] += floor;
                let hearth = crate::geometry::chimney::hearth_bounds_on_floor(recipe, storey)
                    .expect("enabled chimney");
                shapes
                    .iter()
                    .any(|s| overlaps(*s, breast) || overlaps(*s, hearth))
            })
        {
            return None;
        }
    }
    Some(shapes)
}

fn keeps_chimney_wall_clear(recipe: &BuildingRecipe, reserved: BoxCollider) -> bool {
    let Some(chimney) = &recipe.chimney else {
        return true;
    };
    let normal = chimney.facade.normal();
    let axis = usize::from(normal[1].abs() > 0.5);
    let edge = if normal[axis] > 0. {
        reserved.max[axis]
    } else {
        -reserved.min[axis]
    };
    let wall = [recipe.width, recipe.depth][axis] * 0.5 - recipe.wall_thickness;
    wall - edge >= chimney.depth + chimney.hearth_depth + 0.66 - 0.00001
}

pub(crate) fn overlaps(a: BoxCollider, b: BoxCollider) -> bool {
    (0..3).all(|i| a.min[i] < b.max[i] - 0.001 && a.max[i] > b.min[i] + 0.001)
}

fn check_settings(s: &StaircaseRecipe) -> Result<(), BuildingError> {
    for (name, value, min, max) in [
        ("width", s.width, 0.9, 2.),
        ("going", s.going, 0.24, 0.4),
        ("target_riser", s.target_riser, 0.1, 0.2),
        ("landing_depth", s.landing_depth, 0.9, 2.),
    ] {
        if !value.is_finite() || !(min..=max).contains(&value) {
            return Err(BuildingError::new(
                format!("staircase.{name}"),
                format!("must be finite and between {min} and {max} metres"),
            ));
        }
    }
    if s.landing_depth < s.width || s.position.is_some_and(|p| p.iter().any(|v| !v.is_finite())) {
        return Err(BuildingError::new(
            "staircase",
            "landings must be at least as deep as the flight width and positions must be finite",
        ));
    }
    Ok(())
}

pub(crate) fn enclosure_thickness(recipe: &BuildingRecipe) -> f32 {
    recipe.wall_thickness.min(0.12)
}

fn check_enclosure(recipe: &BuildingRecipe) -> Result<(), BuildingError> {
    if recipe.storeys == 1 || !recipe.staircase.enclosed {
        return Ok(());
    }
    let door = recipe.opening_defaults.door;
    if !door.width.is_finite()
        || !door.height.is_finite()
        || door.width < 0.7
        || door.width > recipe.staircase.width * 2. + FLIGHT_GAP - 0.16
        || door.height < 1.9
        || door.height > recipe.wall_height - 0.12
    {
        return Err(BuildingError::new(
            "staircase.door",
            "shared door dimensions must fit the stairwell front wall, with height 1.9 m to the room ceiling; widen the stairs or adjust the shared door dimensions",
        ));
    }
    if recipe
        .openings
        .iter()
        .any(|o| o.id.starts_with("stairwell-door-"))
    {
        return Err(BuildingError::new(
            "staircase.door",
            "stairwell-door- IDs are reserved for generated internal doors",
        ));
    }
    Ok(())
}

fn placement_offsets(min: f32, max: f32) -> [f32; 17] {
    [
        0., 1., 0.5, 0.25, 0.75, 0.125, 0.375, 0.625, 0.875, 0.0625, 0.1875, 0.3125, 0.4375,
        0.5625, 0.6875, 0.8125, 0.9375,
    ]
    .map(|t| min + (max - min) * t)
}
