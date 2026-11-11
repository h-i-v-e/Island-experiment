//! Wall-attached masonry fireplace, open soot-lined hood/flue and roof penetration.
use super::Builder;
use crate::{
    BuildingRecipe, ChimneyRecipe, Facade, MaterialSlot, RoofKind, collision::BoxCollider,
};

const LINTEL_DEPTH: f32 = 0.12;
const LINTEL_HEIGHT: f32 = 0.18;
const CAP_HEIGHT: f32 = 0.10;
const CAP_OVERHANG: f32 = 0.06;

struct Frame<'a> {
    recipe: &'a BuildingRecipe,
    chimney: &'a ChimneyRecipe,
}
impl Frame<'_> {
    fn point(&self, u: f32, v: f32, z: f32) -> [f32; 3] {
        self.chimney
            .facade
            .point(self.recipe, self.chimney.offset + u, z, v)
    }
    fn back(&self) -> f32 {
        self.recipe.wall_thickness
    }
    fn front(&self) -> f32 {
        self.back() + self.chimney.depth
    }
    fn opening_top(&self) -> f32 {
        self.chimney.hearth_height + self.chimney.firebox_height
    }
    fn hood_start(&self) -> f32 {
        self.opening_top() + LINTEL_HEIGHT
    }
    fn hood_end(&self) -> f32 {
        self.hood_start() + 0.60
    }
    fn centre(&self) -> f32 {
        self.back() + self.chimney.depth * 0.5
    }
    fn bounds(&self, min: [f32; 3], max: [f32; 3]) -> BoxCollider {
        let a = self.point(min[0], min[1], min[2]);
        let b = self.point(max[0], max[1], max[2]);
        BoxCollider {
            min: std::array::from_fn(|i| a[i].min(b[i])),
            max: std::array::from_fn(|i| a[i].max(b[i])),
        }
    }
    fn cube(&self, b: &mut Builder, slot: MaterialSlot, min: [f32; 3], max: [f32; 3]) {
        let bounds = self.bounds(min, max);
        masonry_box(b, slot, bounds.min, bounds.max);
    }
    fn solid(
        &self,
        b: &mut Builder,
        colliders: &mut Vec<BoxCollider>,
        min: [f32; 3],
        max: [f32; 3],
    ) {
        self.cube(b, MaterialSlot::Chimney, min, max);
        colliders.push(self.bounds(min, max));
    }
    fn face(&self, b: &mut Builder, slot: MaterialSlot, points: [[f32; 3]; 4]) {
        let points = points.map(|p| self.point(p[0], p[1], p[2]));
        let start = b.mesh(slot).positions.len();
        b.face(slot, points);
        project_uvs(b, slot, start);
    }
    fn stack_local(&self) -> ([f32; 2], [f32; 2]) {
        (
            [
                -self.chimney.stack_width * 0.5,
                self.centre() - self.chimney.stack_depth * 0.5,
            ],
            [
                self.chimney.stack_width * 0.5,
                self.centre() + self.chimney.stack_depth * 0.5,
            ],
        )
    }
}

/// Outer vertical shaft footprint and complete top elevation, in native metres.
#[must_use]
pub fn stack_bounds(recipe: &BuildingRecipe) -> Option<BoxCollider> {
    let chimney = recipe.chimney.as_ref()?;
    let frame = Frame { recipe, chimney };
    let (min, max) = frame.stack_local();
    let mut bounds = frame.bounds([min[0], min[1], frame.hood_end()], [max[0], max[1], 0.]);
    bounds.max[2] = highest_roof(recipe, bounds) + chimney.height_above_roof;
    bounds.min[2] = frame.hood_end();
    Some(bounds)
}
/// Static, deliberately non-walkable hearth footprint and height.
#[must_use]
pub fn hearth_bounds(recipe: &BuildingRecipe) -> Option<BoxCollider> {
    let chimney = recipe.chimney.as_ref()?;
    let frame = Frame { recipe, chimney };
    Some(frame.bounds(
        [-chimney.width * 0.5 - 0.1, frame.back(), 0.],
        [
            chimney.width * 0.5 + 0.1,
            frame.front() + chimney.hearth_depth,
            chimney.hearth_height,
        ],
    ))
}
#[must_use]
pub fn fireplace_centre(recipe: &BuildingRecipe) -> Option<[f32; 3]> {
    let chimney = recipe.chimney.as_ref()?;
    let frame = Frame { recipe, chimney };
    Some(frame.point(
        0.,
        frame.front() - chimney.firebox_depth * 0.5,
        chimney.hearth_height + chimney.firebox_height * 0.5,
    ))
}
/// Top of the structural roof, including its vertical slab thickness.
#[must_use]
pub fn roof_height(recipe: &BuildingRecipe, x: f32) -> f32 {
    recipe.wall_height
        + recipe.roof.effective_thickness()
        + match recipe.roof.kind {
            RoofKind::Gable => recipe.roof.rise * (1. - x.abs() / (recipe.width * 0.5)),
            RoofKind::Shed => recipe.roof.rise * (x / recipe.width + 0.5),
        }
}
#[must_use]
pub fn highest_roof(recipe: &BuildingRecipe, bounds: BoxCollider) -> f32 {
    let mut result = roof_height(recipe, bounds.min[0]).max(roof_height(recipe, bounds.max[0]));
    if bounds.min[0] <= 0. && bounds.max[0] >= 0. {
        result = result.max(roof_height(recipe, 0.));
    }
    result
        + match recipe.roof.covering {
            crate::RoofCovering::StandingSeam => 0.,
            covering => covering.detail_height(),
        }
}
#[must_use]
pub fn lowest_roof(recipe: &BuildingRecipe, bounds: BoxCollider) -> f32 {
    let height = roof_height(recipe, bounds.min[0]).min(roof_height(recipe, bounds.max[0]));
    if recipe.roof.covering == crate::RoofCovering::Thatch {
        // Continue simplified masonry below the lowest rounded return and
        // small coat undulations, so it cannot float above the roof opening.
        height - recipe.roof.effective_thickness() * 0.5 - 0.02
    } else {
        height
    }
}

pub(crate) fn near(b: &mut Builder, recipe: &BuildingRecipe, colliders: &mut Vec<BoxCollider>) {
    let Some(chimney) = recipe.chimney.as_ref() else {
        return;
    };
    let frame = Frame { recipe, chimney };
    fireplace_body(b, &frame, colliders);
    breast(b, &frame);
    surround(b, &frame, colliders);
    let (min, max) = frame.stack_local();
    let top = stack_bounds(recipe).expect("enabled chimney").max[2];
    let wall = flue_wall(chimney);
    ring(
        b,
        &frame,
        min,
        max,
        frame.hood_end(),
        top - CAP_HEIGHT,
        wall,
        true,
    );
    cap(b, &frame, min, max, top, wall, true);
    // An inaccessible hood envelope is conservative above the low firebox opening.
    colliders.push(frame.bounds(
        [-chimney.width * 0.5, frame.back(), frame.hood_start()],
        [chimney.width * 0.5, frame.front(), frame.hood_end()],
    ));
    colliders.extend(shell_solids(
        &frame,
        (min, max),
        (
            [min[0] + wall, min[1] + wall],
            [max[0] - wall, max[1] - wall],
        ),
        [frame.hood_end(), top],
    ));
    colliders.extend(shell_solids(
        &frame,
        (
            [-chimney.width * 0.5, frame.back()],
            [chimney.width * 0.5, frame.front()],
        ),
        (min, max),
        [frame.hood_end(), super::ceiling_top(recipe)],
    ));
    flashing(b, recipe, MaterialSlot::Chimney, true);
}
fn fireplace_body(b: &mut Builder, f: &Frame<'_>, colliders: &mut Vec<BoxCollider>) {
    let c = f.chimney;
    let half = c.width * 0.5;
    let opening = c.firebox_width * 0.5;
    let back = f.front() - c.firebox_depth;
    let top = f.hood_start();
    f.solid(
        b,
        colliders,
        [-half - 0.1, f.back(), 0.],
        [half + 0.1, f.front() + c.hearth_depth, c.hearth_height],
    );
    f.solid(
        b,
        colliders,
        [-half, f.back(), c.hearth_height],
        [-opening, f.front(), top],
    );
    f.solid(
        b,
        colliders,
        [opening, f.back(), c.hearth_height],
        [half, f.front(), top],
    );
    f.solid(
        b,
        colliders,
        [-opening, f.back(), c.hearth_height],
        [opening, back, top],
    );
    f.solid(
        b,
        colliders,
        [-opening, f.front() - LINTEL_DEPTH, f.opening_top()],
        [opening, f.front(), top],
    );
    let z0 = c.hearth_height + 0.001;
    let z1 = top;
    let left = -opening + 0.001;
    let right = opening - 0.001;
    let rear = back + 0.001;
    f.face(
        b,
        MaterialSlot::Firebox,
        [
            [right, rear, z0],
            [left, rear, z0],
            [left, rear, z1],
            [right, rear, z1],
        ],
    );
    f.face(
        b,
        MaterialSlot::Firebox,
        [
            [left, rear, z0],
            [left, f.front(), z0],
            [left, f.front(), z1],
            [left, rear, z1],
        ],
    );
    f.face(
        b,
        MaterialSlot::Firebox,
        [
            [right, f.front(), z0],
            [right, rear, z0],
            [right, rear, z1],
            [right, f.front(), z1],
        ],
    );
    f.face(
        b,
        MaterialSlot::Firebox,
        [
            [left, rear, z0],
            [right, rear, z0],
            [right, f.front(), z0],
            [left, f.front(), z0],
        ],
    );
    f.face(
        b,
        MaterialSlot::Firebox,
        [
            [left, f.front() - LINTEL_DEPTH - 0.001, f.opening_top()],
            [right, f.front() - LINTEL_DEPTH - 0.001, f.opening_top()],
            [right, f.front() - LINTEL_DEPTH - 0.001, z1],
            [left, f.front() - LINTEL_DEPTH - 0.001, z1],
        ],
    );
    f.face(
        b,
        MaterialSlot::Firebox,
        [
            [left, f.front(), f.opening_top() - 0.001],
            [right, f.front(), f.opening_top() - 0.001],
            [right, f.front() - LINTEL_DEPTH, f.opening_top() - 0.001],
            [left, f.front() - LINTEL_DEPTH, f.opening_top() - 0.001],
        ],
    );
}
fn breast(b: &mut Builder, f: &Frame<'_>) {
    let c = f.chimney;
    let (min, max) = f.stack_local();
    let wall = flue_wall(c);
    let outer0 = rectangle(
        [-c.width * 0.5, f.back()],
        [c.width * 0.5, f.front()],
        f.hood_start(),
    );
    // Keep the visible stone courses vertical; only the hidden throat narrows.
    let outer1 = rectangle(
        [-c.width * 0.5, f.back()],
        [c.width * 0.5, f.front()],
        super::ceiling_top(f.recipe),
    );
    let inner0 = rectangle(
        [-c.firebox_width * 0.5, f.front() - c.firebox_depth],
        [c.firebox_width * 0.5, f.front() - LINTEL_DEPTH],
        f.hood_start(),
    );
    let inner1 = rectangle(
        [min[0] + wall, min[1] + wall],
        [max[0] - wall, max[1] - wall],
        f.hood_end(),
    );
    for edge in 0..4 {
        let next = (edge + 1) % 4;
        f.face(
            b,
            MaterialSlot::Chimney,
            [outer0[edge], outer0[next], outer1[next], outer1[edge]],
        );
        f.face(
            b,
            MaterialSlot::Firebox,
            [inner0[next], inner0[edge], inner1[edge], inner1[next]],
        );
        f.face(
            b,
            MaterialSlot::Firebox,
            [outer0[next], outer0[edge], inner0[edge], inner0[next]],
        );
    }
}
/// Raised jambs, header and projecting shelf share the adjacent room-wall material.
fn surround(b: &mut Builder, f: &Frame<'_>, colliders: &mut Vec<BoxCollider>) {
    let c = f.chimney;
    let opening = c.firebox_width * 0.5;
    let border = ((c.width - c.firebox_width) * 0.5).min(0.18);
    let back = f.front() - 0.005;
    let front = f.front() + 0.045;
    let header = f.opening_top() + 0.14;
    for (min, max) in [
        (
            [-opening - border, back, c.hearth_height],
            [-opening, front, f.opening_top()],
        ),
        (
            [opening, back, c.hearth_height],
            [opening + border, front, f.opening_top()],
        ),
        (
            [-opening - border, back, f.opening_top()],
            [opening + border, front, header],
        ),
        (
            [-c.width * 0.5 - 0.1, back, header],
            [
                c.width * 0.5 + 0.1,
                f.front() + c.hearth_depth.min(0.12),
                header + 0.1,
            ],
        ),
    ] {
        f.cube(b, MaterialSlot::Interior, min, max);
        colliders.push(f.bounds(min, max));
    }
}

fn rectangle(min: [f32; 2], max: [f32; 2], z: f32) -> [[f32; 3]; 4] {
    [
        [min[0], min[1], z],
        [max[0], min[1], z],
        [max[0], max[1], z],
        [min[0], max[1], z],
    ]
}
fn flue_wall(c: &ChimneyRecipe) -> f32 {
    0.10_f32.min(c.stack_width.min(c.stack_depth) * 0.2)
}
#[allow(clippy::too_many_arguments)] // Two rectangle corners and vertical extents define a hollow shaft.
fn ring(
    b: &mut Builder,
    f: &Frame<'_>,
    min: [f32; 2],
    max: [f32; 2],
    z0: f32,
    z1: f32,
    wall: f32,
    soot: bool,
) {
    let outer0 = rectangle(min, max, z0);
    let outer1 = rectangle(min, max, z1);
    let inner0 = rectangle(
        [min[0] + wall, min[1] + wall],
        [max[0] - wall, max[1] - wall],
        z0,
    );
    let inner1 = rectangle(
        [min[0] + wall, min[1] + wall],
        [max[0] - wall, max[1] - wall],
        z1,
    );
    for edge in 0..4 {
        let next = (edge + 1) % 4;
        f.face(
            b,
            MaterialSlot::Chimney,
            [outer0[edge], outer0[next], outer1[next], outer1[edge]],
        );
        f.face(
            b,
            if soot {
                MaterialSlot::Firebox
            } else {
                MaterialSlot::Chimney
            },
            [inner0[next], inner0[edge], inner1[edge], inner1[next]],
        );
    }
}
fn cap(
    b: &mut Builder,
    f: &Frame<'_>,
    min: [f32; 2],
    max: [f32; 2],
    top: f32,
    wall: f32,
    soot: bool,
) {
    let cmin = [min[0] - CAP_OVERHANG, min[1] - CAP_OVERHANG];
    let cmax = [max[0] + CAP_OVERHANG, max[1] + CAP_OVERHANG];
    let inner_min = [min[0] + wall, min[1] + wall];
    let inner_max = [max[0] - wall, max[1] - wall];
    let outer0 = rectangle(cmin, cmax, top - CAP_HEIGHT);
    let outer1 = rectangle(cmin, cmax, top);
    let inner0 = rectangle(inner_min, inner_max, top - CAP_HEIGHT);
    let inner1 = rectangle(inner_min, inner_max, top);
    for edge in 0..4 {
        let next = (edge + 1) % 4;
        f.face(
            b,
            MaterialSlot::Chimney,
            [outer0[edge], outer0[next], outer1[next], outer1[edge]],
        );
        f.face(
            b,
            MaterialSlot::Chimney,
            [outer1[edge], outer1[next], inner1[next], inner1[edge]],
        );
        f.face(
            b,
            if soot {
                MaterialSlot::Firebox
            } else {
                MaterialSlot::Chimney
            },
            [inner0[next], inner0[edge], inner1[edge], inner1[next]],
        );
        f.face(
            b,
            MaterialSlot::Chimney,
            [outer0[next], outer0[edge], inner0[edge], inner0[next]],
        );
    }
}
fn shell_solids(
    f: &Frame<'_>,
    outer: ([f32; 2], [f32; 2]),
    inner: ([f32; 2], [f32; 2]),
    height: [f32; 2],
) -> [BoxCollider; 4] {
    let (min, max) = outer;
    let (hole_min, hole_max) = inner;
    let [z0, z1] = height;
    [
        f.bounds([min[0], min[1], z0], [hole_min[0], max[1], z1]),
        f.bounds([hole_max[0], min[1], z0], [max[0], max[1], z1]),
        f.bounds([hole_min[0], min[1], z0], [hole_max[0], hole_min[1], z1]),
        f.bounds([hole_min[0], hole_max[1], z0], [hole_max[0], max[1], z1]),
    ]
}

pub(crate) fn middle(b: &mut Builder, recipe: &BuildingRecipe) {
    let Some(chimney) = recipe.chimney.as_ref() else {
        return;
    };
    let f = Frame { recipe, chimney };
    let (min, max) = f.stack_local();
    let bounds = stack_bounds(recipe).expect("enabled chimney");
    let bottom = lowest_roof(recipe, bounds) - 0.02;
    let wall = flue_wall(chimney);
    ring(
        b,
        &f,
        min,
        max,
        bottom,
        bounds.max[2] - CAP_HEIGHT,
        wall,
        false,
    );
    cap(b, &f, min, max, bounds.max[2], wall, false);
    f.face(
        b,
        MaterialSlot::Chimney,
        rectangle(
            [min[0] + wall, min[1] + wall],
            [max[0] - wall, max[1] - wall],
            bottom,
        ),
    );
    flashing(b, recipe, MaterialSlot::Chimney, false);
}

fn masonry_box(b: &mut Builder, slot: MaterialSlot, min: [f32; 3], max: [f32; 3]) {
    let start = b.mesh(slot).positions.len();
    b.cube(slot, min, max);
    project_uvs(b, slot, start);
}
fn project_uvs(b: &mut Builder, slot: MaterialSlot, start: usize) {
    let mesh = b.mesh(slot);
    for index in start..mesh.positions.len() {
        let p = mesh.positions[index];
        let n = mesh.normals[index];
        mesh.uvs[index] = if n[2].abs() > 0.8 {
            [p[0] * 0.5, p[1] * 0.5]
        } else {
            [(-p[0] * n[1] + p[1] * n[0]) * 0.5, p[2] * 0.5]
        };
    }
}

fn flashing(b: &mut Builder, recipe: &BuildingRecipe, slot: MaterialSlot, near: bool) {
    let Some(bounds) = stack_bounds(recipe) else {
        return;
    };
    let margin = 0.035;
    let thatch = (recipe.roof.covering == crate::RoofCovering::Thatch)
        .then(|| super::thatch::surface_height(recipe, near));
    let height = |x, y| {
        thatch
            .as_ref()
            .map_or_else(|| roof_height(recipe, x), |height| height(x, y))
            + 0.003
    };
    for (x0, x1, y0, y1) in [
        (
            bounds.min[0] - margin,
            bounds.min[0],
            bounds.min[1] - margin,
            bounds.max[1] + margin,
        ),
        (
            bounds.max[0],
            bounds.max[0] + margin,
            bounds.min[1] - margin,
            bounds.max[1] + margin,
        ),
        (
            bounds.min[0],
            bounds.max[0],
            bounds.min[1] - margin,
            bounds.min[1],
        ),
        (
            bounds.min[0],
            bounds.max[0],
            bounds.max[1],
            bounds.max[1] + margin,
        ),
    ] {
        b.face(
            slot,
            [
                [x0, y0, height(x0, y0)],
                [x1, y0, height(x1, y0)],
                [x1, y1, height(x1, y1)],
                [x0, y1, height(x0, y1)],
            ],
        );
    }
}

/// Tessellate an XY rectangle around the complete outer shaft footprint.
pub(crate) fn roof_cells(
    recipe: &BuildingRecipe,
    min: [f32; 2],
    max: [f32; 2],
) -> Vec<([f32; 2], [f32; 2])> {
    let Some(hole) = stack_bounds(recipe) else {
        return vec![(min, max)];
    };
    let mut xs = vec![min[0], max[0]];
    let mut ys = vec![min[1], max[1]];
    for value in [hole.min[0], hole.max[0]] {
        if value > min[0] && value < max[0] {
            xs.push(value);
        }
    }
    for value in [hole.min[1], hole.max[1]] {
        if value > min[1] && value < max[1] {
            ys.push(value);
        }
    }
    xs.sort_by(f32::total_cmp);
    ys.sort_by(f32::total_cmp);
    let mut result = Vec::new();
    for x in xs.windows(2) {
        for y in ys.windows(2) {
            let cx = (x[0] + x[1]) * 0.5;
            let cy = (y[0] + y[1]) * 0.5;
            if cx > hole.min[0] && cx < hole.max[0] && cy > hole.min[1] && cy < hole.max[1] {
                continue;
            }
            result.push(([x[0], y[0]], [x[1], y[1]]));
        }
    }
    result
}

pub(crate) fn exterior_bounds(recipe: &BuildingRecipe) -> Option<BoxCollider> {
    let mut bounds = stack_bounds(recipe)?;
    bounds.min[2] = lowest_roof(recipe, bounds) - 0.02;
    Some(bounds)
}
pub(crate) fn distant(b: &mut Builder, recipe: &BuildingRecipe) {
    let Some(mut bounds) = exterior_bounds(recipe) else {
        return;
    };
    let top = bounds.max[2];
    let base = bounds.min[2];
    bounds.max[2] = top - CAP_HEIGHT;
    atlas_box_sides(b, recipe, bounds, base, top);
    let cap = BoxCollider {
        min: [
            bounds.min[0] - CAP_OVERHANG,
            bounds.min[1] - CAP_OVERHANG,
            top - CAP_HEIGHT,
        ],
        max: [
            bounds.max[0] + CAP_OVERHANG,
            bounds.max[1] + CAP_OVERHANG,
            top,
        ],
    };
    atlas_box_sides(b, recipe, cap, base, top);
    b.quad(
        MaterialSlot::Atlas,
        [
            [cap.min[0], cap.min[1], top],
            [cap.max[0], cap.min[1], top],
            [cap.max[0], cap.max[1], top],
            [cap.min[0], cap.max[1], top],
        ],
        [
            crate::lod::atlas_uv(recipe, 12, 0., 0.),
            crate::lod::atlas_uv(recipe, 12, 1., 0.),
            crate::lod::atlas_uv(recipe, 12, 1., 1.),
            crate::lod::atlas_uv(recipe, 12, 0., 1.),
        ],
    );
    // Close the cap underside while retaining its projecting silhouette.
    b.quad(
        MaterialSlot::Atlas,
        [
            [cap.min[0], cap.max[1], cap.min[2]],
            [cap.max[0], cap.max[1], cap.min[2]],
            [cap.max[0], cap.min[1], cap.min[2]],
            [cap.min[0], cap.min[1], cap.min[2]],
        ],
        [
            crate::lod::atlas_uv(recipe, 12, 0., 1.),
            crate::lod::atlas_uv(recipe, 12, 1., 1.),
            crate::lod::atlas_uv(recipe, 12, 1., 0.),
            crate::lod::atlas_uv(recipe, 12, 0., 0.),
        ],
    );
}
fn atlas_box_sides(
    b: &mut Builder,
    recipe: &BuildingRecipe,
    bounds: BoxCollider,
    base: f32,
    top: f32,
) {
    for facade in Facade::ALL {
        let p = box_face(bounds, facade);
        let low = (bounds.min[2] - base) / (top - base);
        let high = (bounds.max[2] - base) / (top - base);
        let chart = 8 + facade.index();
        b.quad(
            MaterialSlot::Atlas,
            p,
            [
                crate::lod::atlas_uv(recipe, chart, 0., low),
                crate::lod::atlas_uv(recipe, chart, 1., low),
                crate::lod::atlas_uv(recipe, chart, 1., high),
                crate::lod::atlas_uv(recipe, chart, 0., high),
            ],
        );
    }
}
fn box_face(bounds: BoxCollider, facade: Facade) -> [[f32; 3]; 4] {
    let [a, b, c] = bounds.min;
    let [d, e, f] = bounds.max;
    match facade {
        Facade::South => [[a, b, c], [d, b, c], [d, b, f], [a, b, f]],
        Facade::East => [[d, b, c], [d, e, c], [d, e, f], [d, b, f]],
        Facade::North => [[d, e, c], [a, e, c], [a, e, f], [d, e, f]],
        Facade::West => [[a, e, c], [a, b, c], [a, b, f], [a, e, f]],
    }
}

pub(crate) fn strip_spans(recipe: &BuildingRecipe, x: f32, depth: f32) -> [[f32; 2]; 2] {
    if let Some(bounds) =
        stack_bounds(recipe).filter(|bounds| x >= bounds.min[0] && x <= bounds.max[0])
    {
        [[-depth, bounds.min[1]], [bounds.max[1], depth]]
    } else {
        [[-depth, depth], [0., 0.]]
    }
}
