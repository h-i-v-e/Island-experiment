//! Resolve shared dimensions and deterministic facade layouts before geometry is built.
use std::{borrow::Cow, collections::BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    BuildingError, BuildingRecipe, Diagnostic, DoorStyle, Facade, HingeSide, OpeningKind,
    OpeningRecipe, PartitionAxis,
};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningDimensions {
    pub width: f32,
    pub height: f32,
    pub sill: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpeningDefaults {
    pub window: OpeningDimensions,
    pub window_panes: crate::WindowPanes,
    pub door: OpeningDimensions,
}

impl Default for OpeningDefaults {
    fn default() -> Self {
        Self {
            window_panes: crate::WindowPanes::default(),
            window: OpeningDimensions {
                width: 1.4,
                height: 1.1,
                sill: 0.95,
            },
            door: OpeningDimensions {
                width: 1.15,
                height: 2.2,
                sill: 0.,
            },
        }
    }
}

impl OpeningDefaults {
    #[must_use]
    pub const fn for_kind(self, kind: OpeningKind) -> OpeningDimensions {
        match kind {
            OpeningKind::Door => self.door,
            OpeningKind::Window => self.window,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpeningGeneration {
    /// One entrance per selected facade, centred where clear. At least one is required.
    pub door_facades: Vec<Facade>,
    pub window_facades: Vec<Facade>,
    /// Target centre spacing; short clear spans may contain a single window.
    pub window_spacing: f32,
    /// Minimum solid wall between an opening edge and the outside corner.
    pub corner_clearance: f32,
    pub overrides: Vec<OpeningOverride>,
}

impl Default for OpeningGeneration {
    fn default() -> Self {
        Self {
            door_facades: vec![Facade::South],
            window_facades: Facade::ALL.to_vec(),
            window_spacing: 3.,
            corner_clearance: 0.45,
            overrides: Vec::new(),
        }
    }
}

/// Unspecified fields continue to follow generation and building-wide dimensions.
/// Overrides for temporarily absent window IDs are retained for future resizes.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpeningOverride {
    pub id: String,
    pub disabled: bool,
    pub facade: Option<Facade>,
    pub offset: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub sill: Option<f32>,
    pub hinge: Option<HingeSide>,
    pub swing_degrees: Option<f32>,
    pub door_style: Option<DoorStyle>,
    pub window_panes: Option<crate::WindowPanes>,
}

impl OpeningOverride {
    fn apply(&self, opening: &mut OpeningRecipe) {
        if let Some(value) = self.facade {
            opening.facade = value;
        }
        if let Some(value) = self.offset {
            opening.offset = value;
        }
        if let Some(value) = self.width {
            opening.width = value;
        }
        if let Some(value) = self.height {
            opening.height = value;
        }
        if let Some(value) = self.sill {
            opening.sill = value;
        }
        if let Some(value) = self.hinge {
            opening.hinge = value;
        }
        if let Some(value) = self.swing_degrees {
            opening.swing_degrees = value;
        }
        if let Some(value) = self.door_style {
            opening.door_style = value;
        }
        opening.window_panes = self.window_panes;
        opening.use_default_dimensions =
            self.width.is_none() && self.height.is_none() && self.sill.is_none();
    }
}

/// Borrow unchanged manual recipes; otherwise own a small authoring snapshot.
/// The prototype retains this resolved list for collision, LODs and export.
pub fn resolve(recipe: &BuildingRecipe) -> Result<Cow<'_, BuildingRecipe>, BuildingError> {
    validate_settings(recipe)?;
    if recipe.opening_generation.is_none()
        && !recipe.openings.iter().any(|o| o.use_default_dimensions)
        && !recipe
            .partitions
            .iter()
            .any(|p| p.use_default_door_dimensions)
    {
        return Ok(Cow::Borrowed(recipe));
    }
    let mut resolved = recipe.clone();
    if let Some(settings) = &recipe.opening_generation {
        resolved.openings = generated(recipe, settings);
    } else {
        for opening in &mut resolved.openings {
            if opening.use_default_dimensions {
                let size = recipe.opening_defaults.for_kind(opening.kind);
                opening.width = size.width;
                opening.height = size.height;
                opening.sill = size.sill;
            }
        }
    }
    for partition in &mut resolved.partitions {
        if partition.use_default_door_dimensions {
            partition.door_width = recipe.opening_defaults.door.width;
            partition.door_height = recipe.opening_defaults.door.height;
        }
    }
    Ok(Cow::Owned(resolved))
}

/// Includes suppressed slots so the editor can restore them by stable ID.
pub fn generated_slots(recipe: &BuildingRecipe) -> Result<Vec<OpeningRecipe>, BuildingError> {
    validate_settings(recipe)?;
    Ok(recipe
        .opening_generation
        .as_ref()
        .map_or_else(Vec::new, |s| slots(recipe, s)))
}

fn generated(recipe: &BuildingRecipe, settings: &OpeningGeneration) -> Vec<OpeningRecipe> {
    slots(recipe, settings)
        .into_iter()
        .filter(|opening| {
            !settings
                .overrides
                .iter()
                .any(|o| o.id == opening.id && o.disabled)
        })
        .collect()
}

fn slots(recipe: &BuildingRecipe, settings: &OpeningGeneration) -> Vec<OpeningRecipe> {
    (0..recipe.storeys)
        .flat_map(|storey| {
            let local = crate::storeys::recipe_for_storey(recipe, storey);
            floor_slots(&local, settings, storey)
        })
        .collect()
}

fn floor_slots(
    recipe: &BuildingRecipe,
    settings: &OpeningGeneration,
    storey: u8,
) -> Vec<OpeningRecipe> {
    let mut result = Vec::new();
    for facade in Facade::ALL {
        if storey == 0 && settings.door_facades.contains(&facade) {
            let mut opening =
                make_opening(recipe, settings, facade, OpeningKind::Door, 0, 0., storey);
            if !settings
                .overrides
                .iter()
                .any(|o| o.id == opening.id && o.offset.is_some())
            {
                opening.offset = entrance_offset(recipe, settings, &opening, &result);
            }
            result.push(opening);
        }
    }
    // Resolve entrances first so moved/resized doors reserve their actual wall span.
    for facade in Facade::ALL {
        if settings.window_facades.contains(&facade) {
            add_windows(recipe, settings, facade, storey, &mut result);
        }
    }
    result
}

fn make_opening(
    recipe: &BuildingRecipe,
    settings: &OpeningGeneration,
    facade: Facade,
    kind: OpeningKind,
    index: usize,
    offset: f32,
    storey: u8,
) -> OpeningRecipe {
    let wall = match facade {
        Facade::South => "south",
        Facade::East => "east",
        Facade::North => "north",
        Facade::West => "west",
    };
    let mut id = match kind {
        OpeningKind::Door => format!("auto-door-{wall}"),
        OpeningKind::Window => format!("auto-window-{wall}-{index}"),
    };
    if storey > 0 {
        use std::fmt::Write as _;
        write!(id, "-storey-{storey}").expect("string write");
    }
    let size = recipe.opening_defaults.for_kind(kind);
    let mut opening = OpeningRecipe {
        storey,
        id,
        facade,
        kind,
        offset,
        width: size.width,
        height: size.height,
        sill: size.sill,
        use_default_dimensions: true,
        window_panes: None,
        hinge: HingeSide::Left,
        swing_degrees: 105.,
        door_style: DoorStyle::Plank,
    };
    if let Some(overrides) = settings.overrides.iter().find(|o| o.id == opening.id) {
        overrides.apply(&mut opening);
    }
    opening
}

fn add_windows(
    recipe: &BuildingRecipe,
    settings: &OpeningGeneration,
    facade: Facade,
    storey: u8,
    openings: &mut Vec<OpeningRecipe>,
) {
    let bounds = facade_bounds(recipe, settings, facade);
    let blocked = obstructions(
        recipe,
        settings,
        facade,
        OpeningKind::Window,
        openings,
        None,
    );
    let mut index = 0;
    for span in clear_spans(bounds, blocked) {
        place_windows(recipe, settings, facade, storey, span, &mut index, openings);
    }
}

fn facade_bounds(
    recipe: &BuildingRecipe,
    settings: &OpeningGeneration,
    facade: Facade,
) -> [f32; 2] {
    let half = facade.length(recipe) * 0.5;
    let margin = settings.corner_clearance.max(recipe.wall_thickness + 0.12) + 0.001;
    [-half + margin, half - margin]
}

fn clear_spans(bounds: [f32; 2], mut blocked: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    blocked.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut result = Vec::new();
    let mut cursor = bounds[0];
    for block in blocked {
        let end = block[0].min(bounds[1]);
        if end > cursor {
            result.push([cursor, end]);
        }
        cursor = cursor.max(block[1]);
    }
    if bounds[1] > cursor {
        result.push([cursor, bounds[1]]);
    }
    result
}

fn entrance_offset(
    recipe: &BuildingRecipe,
    settings: &OpeningGeneration,
    opening: &OpeningRecipe,
    others: &[OpeningRecipe],
) -> f32 {
    let bounds = facade_bounds(recipe, settings, opening.facade);
    let blocked = obstructions(
        recipe,
        settings,
        opening.facade,
        OpeningKind::Door,
        others,
        None,
    );
    let spans = clear_spans(bounds, blocked);
    let half = opening.width * 0.5;
    if spans.iter().any(|span| span[0] <= -half && span[1] >= half) {
        return 0.;
    }
    spans
        .into_iter()
        .filter(|span| span[1] - span[0] >= opening.width)
        .max_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0])))
        .map_or(0., |span| (span[0] + span[1]) * 0.5)
}

/// Prefer the authored entrance position, then the nearest clear span alongside the stairs.
pub(crate) fn entrance_candidates(
    recipe: &BuildingRecipe,
    opening: &OpeningRecipe,
    shapes: &[crate::collision::BoxCollider],
) -> Vec<f32> {
    let manual = OpeningGeneration {
        door_facades: Vec::new(),
        window_facades: Vec::new(),
        overrides: Vec::new(),
        window_spacing: 3.,
        corner_clearance: 0.12,
    };
    let settings = recipe.opening_generation.as_ref().unwrap_or(&manual);
    let bounds = facade_bounds(recipe, settings, opening.facade);
    let half = opening.width * 0.5;
    let mut blocked = obstructions(
        recipe,
        settings,
        opening.facade,
        OpeningKind::Door,
        &recipe.openings,
        Some(&opening.id),
    );
    blocked.extend(
        recipe
            .openings
            .iter()
            .filter(|other| {
                other.storey == 0
                    && other.facade == opening.facade
                    && other.kind == OpeningKind::Window
                    && (recipe.opening_generation.is_none()
                        || settings
                            .overrides
                            .iter()
                            .any(|o| o.id == other.id && o.offset.is_some()))
            })
            .map(|other| {
                [
                    other.offset - other.width * 0.5 - 0.15,
                    other.offset + other.width * 0.5 + 0.15,
                ]
            }),
    );
    let mut candidates = Vec::new();
    // Authoring validation already checks the preferred position against the
    // other openings and chimney. Preserve valid overrides beyond the automatic
    // corner margin, and only move them when the actual stair geometry blocks them.
    if opening.offset.abs() + half
        <= opening.facade.length(recipe) * 0.5 - recipe.wall_thickness - 0.12 + 0.00001
    {
        candidates.push(opening.offset);
    }
    blocked.extend(
        shapes
            .iter()
            .filter_map(|shape| entrance_obstruction(recipe, opening, *shape)),
    );
    candidates.extend(clear_spans(bounds, blocked).into_iter().flat_map(|span| {
        let min = span[0] + half;
        let max = span[1] - half;
        (min <= max)
            .then(|| [opening.offset.clamp(min, max), min, max])
            .into_iter()
            .flatten()
    }));
    candidates.sort_by(|a, b| {
        (a - opening.offset)
            .abs()
            .total_cmp(&(b - opening.offset).abs())
            .then_with(|| a.total_cmp(b))
    });
    candidates.dedup_by(|a, b| (*a - *b).abs() < 0.00001);
    candidates
}

fn entrance_obstruction(
    recipe: &BuildingRecipe,
    opening: &OpeningRecipe,
    shape: crate::collision::BoxCollider,
) -> Option<[f32; 2]> {
    if shape.max[2] <= 0.025 || shape.min[2] >= opening.height.max(1.825) {
        return None;
    }
    let normal = opening.facade.normal();
    let tangent = opening.facade.tangent();
    let axis = usize::from(normal[1].abs() > 0.5);
    let along = 1 - axis;
    let surface = [recipe.width, recipe.depth][axis] * 0.5;
    let depths = [
        surface - normal[axis] * shape.min[axis],
        surface - normal[axis] * shape.max[axis],
    ];
    if depths[0].min(depths[1]) > recipe.wall_thickness + opening.width.max(0.95) + 0.1
        || depths[0].max(depths[1]) < recipe.wall_thickness - 0.1
    {
        return None;
    }
    let edges = [
        tangent[along] * shape.min[along],
        tangent[along] * shape.max[along],
    ];
    let over_swing = opening.width * (-opening.swing_degrees.to_radians().cos()).max(0.);
    Some([
        edges[0].min(edges[1])
            - 0.08
            - if opening.hinge == HingeSide::Right {
                over_swing
            } else {
                0.
            },
        edges[0].max(edges[1])
            + 0.08
            + if opening.hinge == HingeSide::Left {
                over_swing
            } else {
                0.
            },
    ])
}

pub(crate) fn apply_entrance_offsets(recipe: &mut Cow<'_, BuildingRecipe>, offsets: &[f32]) {
    if recipe
        .openings
        .iter()
        .filter(|o| o.kind == OpeningKind::Door)
        .zip(offsets)
        .all(|(o, offset)| (o.offset - offset).abs() < 0.00001)
    {
        return;
    }
    let recipe = recipe.to_mut();
    for (opening, &offset) in recipe
        .openings
        .iter_mut()
        .filter(|o| o.kind == OpeningKind::Door)
        .zip(offsets)
    {
        opening.offset = offset;
    }
    if recipe.opening_generation.is_some() {
        regenerate_windows(recipe);
    }
}

fn regenerate_windows(recipe: &mut BuildingRecipe) {
    let mut result = Vec::with_capacity(recipe.openings.len());
    let mut doors = std::mem::take(&mut recipe.openings);
    doors.retain(|o| o.kind == OpeningKind::Door);
    let settings = recipe
        .opening_generation
        .as_ref()
        .expect("generated openings");
    for storey in 0..recipe.storeys {
        let local = crate::storeys::recipe_for_storey(recipe, storey);
        let mut floor = if storey == 0 {
            std::mem::take(&mut doors)
        } else {
            Vec::new()
        };
        for facade in Facade::ALL {
            if settings.window_facades.contains(&facade) {
                add_windows(&local, settings, facade, storey, &mut floor);
            }
        }
        result.extend(floor.into_iter().filter(|o| {
            !settings
                .overrides
                .iter()
                .any(|v| v.id == o.id && v.disabled)
        }));
    }
    recipe.openings = result;
}

fn obstructions(
    recipe: &BuildingRecipe,
    settings: &OpeningGeneration,
    facade: Facade,
    kind: OpeningKind,
    openings: &[OpeningRecipe],
    exclude: Option<&str>,
) -> Vec<[f32; 2]> {
    let mut blocked: Vec<_> = openings
        .iter()
        .filter(|o| {
            o.facade == facade
                && o.kind == OpeningKind::Door
                && exclude != Some(o.id.as_str())
                && !settings
                    .overrides
                    .iter()
                    .any(|v| v.id == o.id && v.disabled)
        })
        .map(|o| {
            [
                o.offset - o.width * 0.5 - 0.3,
                o.offset + o.width * 0.5 + 0.3,
            ]
        })
        .collect();
    if let Some(chimney) = &recipe.chimney
        && chimney.facade == facade
    {
        let clearance = if kind == OpeningKind::Door {
            0.45
        } else {
            0.25
        };
        let half = chimney.width * 0.5 + clearance;
        blocked.push([chimney.offset - half, chimney.offset + half]);
    }
    for partition in &recipe.partitions {
        let offset = match (partition.axis, facade) {
            (PartitionAxis::X, Facade::East) | (PartitionAxis::Y, Facade::South) => {
                Some(partition.offset)
            }
            (PartitionAxis::X, Facade::West) | (PartitionAxis::Y, Facade::North) => {
                Some(-partition.offset)
            }
            _ => None,
        };
        if let Some(offset) = offset {
            let half = recipe.wall_thickness * 0.5 + 0.1;
            blocked.push([offset - half, offset + half]);
        }
    }
    blocked
}

fn place_windows(
    recipe: &BuildingRecipe,
    settings: &OpeningGeneration,
    facade: Facade,
    storey: u8,
    span: [f32; 2],
    index: &mut usize,
    openings: &mut Vec<OpeningRecipe>,
) {
    let width = recipe.opening_defaults.window.width;
    let available = span[1] - span[0];
    if available < width {
        return;
    }
    let pitch = settings.window_spacing.max(width + 0.3);
    // At most 15 windows per facade plus four entrances stays within the 64 limit.
    let count = (1_u16..=15)
        .take_while(|&count| width + f32::from(count - 1) * pitch <= available)
        .last()
        .unwrap_or(1);
    let step = available / f32::from(count);
    for cell in 0..count {
        if *index >= 15 {
            return;
        }
        let offset = span[0] + (f32::from(cell) + 0.5) * step;
        openings.push(make_opening(
            recipe,
            settings,
            facade,
            OpeningKind::Window,
            *index,
            offset,
            storey,
        ));
        *index += 1;
    }
}

fn validate_settings(recipe: &BuildingRecipe) -> Result<(), BuildingError> {
    let mut diagnostics = Vec::new();
    if !recipe.opening_defaults.window_panes.is_valid() {
        diagnostics.push(Diagnostic {
            element: "opening_defaults.window_panes".into(),
            message: "pane rows and columns must each be between 1 and 32".into(),
        });
    }
    if !(1..=6).contains(&recipe.storeys) {
        diagnostics.push(Diagnostic {
            element: "storeys".into(),
            message: "choose one to six storeys, including the ground floor".into(),
        });
    }
    for (name, size, min_width, min_height) in [
        ("window", recipe.opening_defaults.window, 0.35, 0.35),
        ("door", recipe.opening_defaults.door, 0.95, 2.1),
    ] {
        for (field, value, minimum, maximum) in [
            ("width", size.width, min_width, 20.),
            ("height", size.height, min_height, 8.),
            ("sill", size.sill, 0., 8.),
        ] {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                diagnostics.push(Diagnostic {
                    element: format!("opening_defaults.{name}.{field}"),
                    message: format!("must be finite and between {minimum} and {maximum} metres"),
                });
            }
        }
    }
    if recipe.opening_defaults.door.sill.abs() > 0.001 {
        diagnostics.push(Diagnostic {
            element: "opening_defaults.door.sill".into(),
            message: "exterior doors must be flush with the floor".into(),
        });
    }
    if let Some(settings) = &recipe.opening_generation {
        validate_generation(recipe, settings, &mut diagnostics);
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(BuildingError { diagnostics })
    }
}

fn validate_generation(
    recipe: &BuildingRecipe,
    settings: &OpeningGeneration,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (field, value, minimum, maximum) in [
        ("width", recipe.width, 3., 40.),
        ("depth", recipe.depth, 3., 40.),
        ("wall_thickness", recipe.wall_thickness, 0.08, 0.6),
        (
            "opening_generation.window_spacing",
            settings.window_spacing,
            0.5,
            20.,
        ),
        (
            "opening_generation.corner_clearance",
            settings.corner_clearance,
            0.12,
            3.,
        ),
    ] {
        if !value.is_finite() || !(minimum..=maximum).contains(&value) {
            diagnostics.push(Diagnostic {
                element: field.into(),
                message: format!("must be finite and between {minimum} and {maximum} metres"),
            });
        }
    }
    for (field, facades) in [
        ("door_facades", &settings.door_facades),
        ("window_facades", &settings.window_facades),
    ] {
        if facades.len() > 4
            || facades
                .iter()
                .enumerate()
                .any(|(i, f)| facades[..i].contains(f))
            || (field == "door_facades" && facades.is_empty())
        {
            diagnostics.push(Diagnostic {
                element: format!("opening_generation.{field}"),
                message: "select each facade at most once and retain at least one entrance facade"
                    .into(),
            });
        }
    }
    validate_overrides(&settings.overrides, diagnostics);
}

fn validate_overrides(overrides: &[OpeningOverride], diagnostics: &mut Vec<Diagnostic>) {
    let mut ids = BTreeSet::new();
    for overrides in overrides {
        if overrides
            .window_panes
            .is_some_and(|panes| !panes.is_valid())
        {
            diagnostics.push(Diagnostic {
                element: overrides.id.clone(),
                message: "pane rows and columns must each be between 1 and 32".into(),
            });
        }
        if overrides.id.trim().is_empty() || overrides.id.len() > 128 || !ids.insert(&overrides.id)
        {
            diagnostics.push(Diagnostic {
                element: "opening_generation.overrides".into(),
                message: "override IDs must be unique and contain 1 to 128 characters".into(),
            });
        }
        if !generated_id(&overrides.id) {
            diagnostics.push(Diagnostic {
                element: overrides.id.clone(),
                message:
                    "use auto-door-{wall} or auto-window-{wall}-{0..14} for an opening override"
                        .into(),
            });
        }
        if [
            overrides.offset,
            overrides.width,
            overrides.height,
            overrides.sill,
            overrides.swing_degrees,
        ]
        .into_iter()
        .flatten()
        .any(|v| !v.is_finite())
        {
            diagnostics.push(Diagnostic {
                element: overrides.id.clone(),
                message: "override dimensions must be finite".into(),
            });
        }
    }
}

fn generated_id(id: &str) -> bool {
    let id = if let Some((base, storey)) = id.rsplit_once("-storey-") {
        if !storey.parse::<u8>().is_ok_and(|n| (1..=5).contains(&n)) {
            return false;
        }
        base
    } else {
        id
    };
    let walls = ["south", "east", "north", "west"];
    if let Some(wall) = id.strip_prefix("auto-door-") {
        return walls.contains(&wall);
    }
    let Some((wall, index)) = id
        .strip_prefix("auto-window-")
        .and_then(|s| s.rsplit_once('-'))
    else {
        return false;
    };
    walls.contains(&wall)
        && index.bytes().all(|b| b.is_ascii_digit())
        && (index.len() == 1 || !index.starts_with('0'))
        && index.parse::<u8>().is_ok_and(|v| v < 15)
}
