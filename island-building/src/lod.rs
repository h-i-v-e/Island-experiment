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
    builder.finish()
}
/// Eight padded atlas charts, four columns by two rows. V=0 denotes image top.
// Chart indices are fixed to the eight atlas cells.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn atlas_uv(chart: usize, u: f32, v: f32) -> [f32; 2] {
    [
        (chart % 4) as f32 * 0.25 + (0.05 + u * 0.9) * 0.25,
        (chart / 4) as f32 * 0.5 + (0.95 - v * 0.9) * 0.5,
    ]
}
pub(crate) fn distant(recipe: &BuildingRecipe) -> BuildingLod {
    let mut builder = Builder::default();
    distant_walls(&mut builder, recipe);
    distant_roof(&mut builder, recipe);
    distant_base(&mut builder, recipe);
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
            atlas_uv(facade.index(), 0., 0.),
            atlas_uv(facade.index(), 1., 0.),
            atlas_uv(
                facade.index(),
                1.,
                (p[2][2] + recipe.floor_thickness) / span,
            ),
            atlas_uv(
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
                        facade.index(),
                        0.,
                        (recipe.wall_height + recipe.floor_thickness) / span,
                    ),
                    atlas_uv(
                        facade.index(),
                        1.,
                        (recipe.wall_height + recipe.floor_thickness) / span,
                    ),
                    atlas_uv(facade.index(), 0.5, 1.),
                ],
            );
        }
    }
}
fn distant_roof(builder: &mut Builder, recipe: &BuildingRecipe) {
    let w = recipe.width * 0.5 + recipe.roof.overhang;
    let d = recipe.depth * 0.5 + recipe.roof.overhang;
    let h = recipe.wall_height + recipe.roof.thickness;
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
        builder.quad(
            MaterialSlot::Atlas,
            [[x0, -d, z0], [x1, -d, z1], [x1, d, z1], [x0, d, z0]],
            [
                atlas_uv(chart, 0., 0.),
                atlas_uv(chart, 1., 0.),
                atlas_uv(chart, 1., 1.),
                atlas_uv(chart, 0., 1.),
            ],
        );
        let low0 = z0 - recipe.roof.thickness;
        let low1 = z1 - recipe.roof.thickness;
        let uv = [
            atlas_uv(chart, 0., 0.),
            atlas_uv(chart, 1., 0.),
            atlas_uv(chart, 1., 1.),
            atlas_uv(chart, 0., 1.),
        ];
        // The underside meets wall tops; the narrow fascia closes roof thickness.
        builder.quad(
            MaterialSlot::Atlas,
            [[x0, d, low0], [x1, d, low1], [x1, -d, low1], [x0, -d, low0]],
            uv,
        );
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
        builder.quad(
            MaterialSlot::Atlas,
            [[x0, d, low0], [x0, -d, low0], [x0, -d, z0], [x0, d, z0]],
            uv,
        );
        builder.quad(
            MaterialSlot::Atlas,
            [[x1, -d, low1], [x1, d, low1], [x1, d, z1], [x1, -d, z1]],
            uv,
        );
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
            atlas_uv(6, 0., 0.),
            atlas_uv(6, 1., 0.),
            atlas_uv(6, 1., 1.),
            atlas_uv(6, 0., 1.),
        ],
    );
}
