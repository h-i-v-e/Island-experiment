//! LODs derive from the same resolved dimensions and openings, without interior surfaces.
use crate::{
    BuildingLod, BuildingRecipe, Facade, MaterialSlot, RoofKind,
    geometry::{self, Builder},
};

pub(crate) fn middle(recipe: &BuildingRecipe) -> BuildingLod {
    let mut builder = Builder::default();
    for facade in Facade::ALL {
        geometry::wall(&mut builder, recipe, facade, false, &mut Vec::new());
    }
    for o in &recipe.openings {
        geometry::frame(&mut builder, recipe, o, false);
        let facade = o.facade;
        let a = o.offset - o.width * 0.5;
        let slot = if o.kind == crate::OpeningKind::Window {
            MaterialSlot::WindowProxy
        } else {
            MaterialSlot::Door
        };
        builder.face(
            slot,
            [
                facade.point(recipe, a, o.sill, 0.015),
                facade.point(recipe, a + o.width, o.sill, 0.015),
                facade.point(recipe, a + o.width, o.sill + o.height, 0.015),
                facade.point(recipe, a, o.sill + o.height, 0.015),
            ],
        );
    }
    geometry::roof(&mut builder, recipe, false);
    geometry::timber_frame::generate(&mut builder, recipe, false);
    geometry::chimney::middle(&mut builder, recipe);
    builder.finish()
}
/// Optional chimneys add two atlas rows; legacy recipes retain the original layout.
#[must_use]
pub(crate) fn atlas_rows(recipe: &BuildingRecipe) -> u32 {
    if recipe.chimney.is_some() { 4 } else { 2 }
}

pub(crate) fn thatch_end_chart(recipe: &BuildingRecipe) -> ([f32; 2], f32) {
    let span = recipe.width.max(recipe.depth)
        + recipe.roof.overhang * 2.
        + crate::geometry::thatch::RIDGE_PROJECTION * 2.
        + 0.05;
    (
        [span, span.max(recipe.roof.effective_thickness() + 1.5)],
        0.,
    )
}
/// Four atlas columns with padded charts. V=0 denotes image top.
// Chart indices are fixed to the eight atlas cells.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn atlas_uv(recipe: &BuildingRecipe, chart: usize, u: f32, v: f32) -> [f32; 2] {
    [
        (chart % 4) as f32 * 0.25 + (0.05 + u * 0.9) * 0.25,
        ((chart / 4) as f32 + 0.95 - v * 0.9) / atlas_rows(recipe) as f32,
    ]
}
pub(crate) fn distant(recipe: &BuildingRecipe) -> BuildingLod {
    let mut builder = Builder::default();
    distant_walls(&mut builder, recipe);
    distant_roof(&mut builder, recipe);
    distant_base(&mut builder, recipe);
    geometry::chimney::distant(&mut builder, recipe);
    builder.finish()
}
fn distant_walls(builder: &mut Builder, recipe: &BuildingRecipe) {
    let peak = recipe.wall_height + recipe.roof.rise;
    let span = peak + recipe.floor_thickness;
    for facade in Facade::ALL {
        let half = facade.length(recipe) * 0.5;
        let mut p = [
            facade.point(recipe, -half, -recipe.floor_thickness, 0.),
            facade.point(recipe, half, -recipe.floor_thickness, 0.),
            facade.point(recipe, half, recipe.wall_height, 0.),
            facade.point(recipe, -half, recipe.wall_height, 0.),
        ];
        if recipe.roof.kind == RoofKind::Shed {
            for point in &mut p[2..] {
                point[2] += ((point[0] / (recipe.width * 0.5) + 1.) * 0.5) * recipe.roof.rise;
            }
        }
        let uv = [
            atlas_uv(recipe, facade.index(), 0., 0.),
            atlas_uv(recipe, facade.index(), 1., 0.),
            atlas_uv(
                recipe,
                facade.index(),
                1.,
                (p[2][2] + recipe.floor_thickness) / span,
            ),
            atlas_uv(
                recipe,
                facade.index(),
                0.,
                (p[3][2] + recipe.floor_thickness) / span,
            ),
        ];
        builder.quad(MaterialSlot::Atlas, p, uv);
        if recipe.roof.kind == RoofKind::Gable && matches!(facade, Facade::South | Facade::North) {
            builder.triangle(
                MaterialSlot::Atlas,
                [p[3], p[2], facade.point(recipe, 0., peak, 0.)],
                [
                    atlas_uv(
                        recipe,
                        facade.index(),
                        0.,
                        (recipe.wall_height + recipe.floor_thickness) / span,
                    ),
                    atlas_uv(
                        recipe,
                        facade.index(),
                        1.,
                        (recipe.wall_height + recipe.floor_thickness) / span,
                    ),
                    atlas_uv(recipe, facade.index(), 0.5, 1.),
                ],
            );
        }
    }
}
#[allow(clippy::many_single_char_names)] // Roof section corners follow the mesh coordinate notation.
fn distant_roof(builder: &mut Builder, recipe: &BuildingRecipe) {
    if recipe.roof.covering == crate::RoofCovering::Thatch {
        geometry::thatch::generate(builder, recipe, geometry::thatch::Detail::Distant);
        return;
    }
    let w = recipe.width * 0.5 + recipe.roof.overhang;
    let d = recipe.depth * 0.5 + recipe.roof.overhang;
    let h = recipe.wall_height + recipe.roof.effective_thickness();
    let rise = recipe.roof.rise;
    let spans = match recipe.roof.kind {
        RoofKind::Gable => vec![
            (
                -w,
                0.,
                h - rise * recipe.roof.overhang / (recipe.width * 0.5),
                h + rise,
            ),
            (
                0.,
                w,
                h + rise,
                h - rise * recipe.roof.overhang / (recipe.width * 0.5),
            ),
        ],
        RoofKind::Shed => vec![(
            -w,
            w,
            h - rise * recipe.roof.overhang / recipe.width,
            h + rise + rise * recipe.roof.overhang / recipe.width,
        )],
    };
    for (i, (x0, x1, z0, z1)) in spans.into_iter().enumerate() {
        let chart = 4 + i;
        let low0 = z0 - recipe.roof.effective_thickness();
        let low1 = z1 - recipe.roof.effective_thickness();
        let uv = [
            atlas_uv(recipe, chart, 0., 0.),
            atlas_uv(recipe, chart, 1., 0.),
            atlas_uv(recipe, chart, 1., 1.),
            atlas_uv(recipe, chart, 0., 1.),
        ];
        let z = |x: f32| z0 + (z1 - z0) * (x - x0) / (x1 - x0);
        let tex =
            |x: f32, y: f32| atlas_uv(recipe, chart, (x - x0) / (x1 - x0), (y + d) / (2. * d));
        for (min, max) in geometry::chimney::roof_cells(recipe, [x0, -d], [x1, d]) {
            let [a, b] = min;
            let [c, e] = max;
            builder.quad(
                MaterialSlot::Atlas,
                [[a, b, z(a)], [c, b, z(c)], [c, e, z(c)], [a, e, z(a)]],
                [tex(a, b), tex(c, b), tex(c, e), tex(a, e)],
            );
            builder.quad(
                MaterialSlot::Atlas,
                [
                    [a, e, z(a) - recipe.roof.effective_thickness()],
                    [c, e, z(c) - recipe.roof.effective_thickness()],
                    [c, b, z(c) - recipe.roof.effective_thickness()],
                    [a, b, z(a) - recipe.roof.effective_thickness()],
                ],
                [tex(a, e), tex(c, e), tex(c, b), tex(a, b)],
            );
        }
        builder.quad(
            MaterialSlot::Atlas,
            [[x0, -d, low0], [x1, -d, low1], [x1, -d, z1], [x0, -d, z0]],
            uv,
        );
        builder.quad(
            MaterialSlot::Atlas,
            [[x1, d, low1], [x0, d, low0], [x0, d, z0], [x1, d, z1]],
            uv,
        );
        for [a, c] in geometry::chimney::strip_spans(recipe, x0, d) {
            if c > a {
                builder.quad(
                    MaterialSlot::Atlas,
                    [[x0, c, low0], [x0, a, low0], [x0, a, z0], [x0, c, z0]],
                    uv,
                );
            }
        }
        for [a, c] in geometry::chimney::strip_spans(recipe, x1, d) {
            if c > a {
                builder.quad(
                    MaterialSlot::Atlas,
                    [[x1, a, low1], [x1, c, low1], [x1, c, z1], [x1, a, z1]],
                    uv,
                );
            }
        }
    }
}
fn distant_base(builder: &mut Builder, recipe: &BuildingRecipe) {
    builder.quad(
        MaterialSlot::Atlas,
        [
            [
                -recipe.width * 0.5,
                recipe.depth * 0.5,
                -recipe.floor_thickness,
            ],
            [
                recipe.width * 0.5,
                recipe.depth * 0.5,
                -recipe.floor_thickness,
            ],
            [
                recipe.width * 0.5,
                -recipe.depth * 0.5,
                -recipe.floor_thickness,
            ],
            [
                -recipe.width * 0.5,
                -recipe.depth * 0.5,
                -recipe.floor_thickness,
            ],
        ],
        [
            atlas_uv(recipe, 6, 0., 0.),
            atlas_uv(recipe, 6, 1., 0.),
            atlas_uv(recipe, 6, 1., 1.),
            atlas_uv(recipe, 6, 0., 1.),
        ],
    );
}
