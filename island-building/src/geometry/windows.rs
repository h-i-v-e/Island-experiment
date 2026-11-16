//! Near-LOD window casing, recessed sash and projecting weather sill.
use super::Builder;
use crate::window_panes::PaneGrid;
use crate::{BuildingRecipe, MaterialSlot, OpeningRecipe, collision::BoxCollider};

pub(super) const CASING_WIDTH: f32 = 0.065;
pub(super) const SILL_DROP: f32 = 0.07;
const SILL_HORN: f32 = 0.07;
const SILL_PROJECTION: f32 = 0.12;
pub(super) const GLASS_INSET: f32 = 0.04;

struct Frame<'a> {
    recipe: &'a BuildingRecipe,
    opening: &'a OpeningRecipe,
}

impl Frame<'_> {
    fn point(&self, u: f32, z: f32, inward: f32) -> [f32; 3] {
        self.opening.facade.point(self.recipe, u, z, inward)
    }

    fn timber(&self, builder: &mut Builder, x: [f32; 2], z: [f32; 2], depth: [f32; 2]) {
        let a = self.point(x[0], z[0], depth[0]);
        let b = self.point(x[1], z[1], depth[1]);
        builder.cube(
            MaterialSlot::Trim,
            std::array::from_fn(|i| a[i].min(b[i])),
            std::array::from_fn(|i| a[i].max(b[i])),
        );
    }
}

pub(super) fn casing(builder: &mut Builder, recipe: &BuildingRecipe, opening: &OpeningRecipe) {
    let frame = Frame { recipe, opening };
    let a = opening.offset - opening.width * 0.5;
    let c = opening.offset + opening.width * 0.5;
    let bottom = opening.sill;
    let top = bottom + opening.height;
    // The architrave covers the wall joint; its lining extends through the wall.
    for (x, z) in [
        ([a - CASING_WIDTH, a], [bottom, top + CASING_WIDTH]),
        ([c, c + CASING_WIDTH], [bottom, top + CASING_WIDTH]),
        ([a, c], [top, top + CASING_WIDTH]),
    ] {
        frame.timber(builder, x, z, [-0.03, 0.015]);
    }
    for (x, z) in [
        ([a - 0.012, a + 0.012], [bottom, top]),
        ([c - 0.012, c + 0.012], [bottom, top]),
        ([a + 0.012, c - 0.012], [top - 0.012, top + 0.012]),
    ] {
        frame.timber(builder, x, z, [0., recipe.wall_thickness + 0.012]);
    }
    sash(builder, &frame, [a, c], [bottom, top]);
    sill(builder, &frame, [a - SILL_HORN, c + SILL_HORN], bottom);
}

fn sash(builder: &mut Builder, frame: &Frame<'_>, x: [f32; 2], z: [f32; 2]) {
    let [a, c] = x;
    let [bottom, top] = z;
    // Match the existing glass/collision plane; the bars now sit around the glass.
    let glass = frame.recipe.wall_thickness * 0.48;
    let half_depth = (frame.recipe.wall_thickness * 0.175).min(0.024);
    let depth = [glass - half_depth, glass + half_depth];
    let grid = PaneGrid::new(frame.recipe, frame.opening);
    for (x, z) in sash_bars(&grid, [a, c], [bottom, top]) {
        frame.timber(builder, x, z, depth);
    }
}

fn sash_bars(
    grid: &PaneGrid,
    [a, c]: [f32; 2],
    [bottom, top]: [f32; 2],
) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
    [
        ([a + 0.012, grid.left], [bottom + 0.012, top - 0.012]),
        ([grid.right, c - 0.012], [bottom + 0.012, top - 0.012]),
        ([grid.left, grid.right], [grid.top, top - 0.012]),
        ([grid.left, grid.right], [bottom + 0.012, grid.bottom]),
    ]
    .into_iter()
    .chain(grid.bars())
}

/// The opaque middle LOD retains the pane pattern using front-facing ribbons.
pub(crate) fn proxy(builder: &mut Builder, recipe: &BuildingRecipe, opening: &OpeningRecipe) {
    let frame = Frame { recipe, opening };
    let grid = PaneGrid::new(recipe, opening);
    let x = [
        opening.offset - opening.width * 0.5,
        opening.offset + opening.width * 0.5,
    ];
    let z = [opening.sill, opening.sill + opening.height];
    for ([left, right], [bottom, top]) in sash_bars(&grid, x, z) {
        builder.face(
            MaterialSlot::Trim,
            [
                frame.point(left, bottom, -0.045),
                frame.point(right, bottom, -0.045),
                frame.point(right, top, -0.045),
                frame.point(left, top, -0.045),
            ],
        );
    }
}

fn sill(builder: &mut Builder, frame: &Frame<'_>, x: [f32; 2], height: f32) {
    let [a, c] = x;
    let front = -SILL_PROJECTION;
    let back = frame.recipe.wall_thickness + 0.025;
    let bottom = height - SILL_DROP;
    // One raised surface closes the wall joint and replaces the bottom lining.
    // Keep the shallow fall, with the low edge above the opening's lower cut.
    let low = height + 0.01;
    let high = height + 0.037;
    for face in [
        [
            [a, front, bottom],
            [c, front, bottom],
            [c, front, low],
            [a, front, low],
        ],
        [
            [c, back, bottom],
            [a, back, bottom],
            [a, back, high],
            [c, back, high],
        ],
        [
            [a, back, bottom],
            [a, front, bottom],
            [a, front, low],
            [a, back, high],
        ],
        [
            [c, front, bottom],
            [c, back, bottom],
            [c, back, high],
            [c, front, low],
        ],
        [
            [a, front, low],
            [c, front, low],
            [c, back, high],
            [a, back, high],
        ],
        [
            [a, back, bottom],
            [c, back, bottom],
            [c, front, bottom],
            [a, front, bottom],
        ],
    ] {
        builder.face(
            MaterialSlot::Trim,
            face.map(|[u, v, z]| frame.point(u, z, v)),
        );
    }
}

pub(crate) fn bounds(recipe: &BuildingRecipe, opening: &OpeningRecipe) -> BoxCollider {
    let frame = Frame { recipe, opening };
    let a = frame.point(
        opening.offset - opening.width * 0.5 - SILL_HORN,
        opening.sill - SILL_DROP,
        -SILL_PROJECTION,
    );
    let b = frame.point(
        opening.offset + opening.width * 0.5 + SILL_HORN,
        opening.sill + opening.height + CASING_WIDTH,
        recipe.wall_thickness + 0.025,
    );
    BoxCollider {
        min: std::array::from_fn(|i| a[i].min(b[i])),
        max: std::array::from_fn(|i| a[i].max(b[i])),
    }
}
