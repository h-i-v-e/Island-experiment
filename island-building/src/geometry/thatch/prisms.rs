//! Small convex cross-sections extruded along the ridge, with no blend rings.
use super::{Builder, Coat, Detail, RoofSection, unit};
use crate::RoofKind;

#[derive(Clone, Copy, PartialEq)]
enum Skin {
    Run,
    Ridge,
    Closure,
}

pub(super) fn run(builder: &mut Builder, coat: &Coat<'_>, section: RoofSection) {
    let direction = if section.flip_u { 1. } else { -1. };
    let high = coat.high_x();
    let cut = coat.eave_level();
    let inner = (coat.base(high) - cut) / coat.slope();
    let profile = run_profile(coat, direction);
    prism(builder, coat, profile, coat.half[1], Skin::Run);
    let wall = direction * coat.recipe.width * 0.5;
    if cut > coat.base(wall) + 0.0001 {
        let mut closure = [
            [high + direction * inner, cut],
            [wall, coat.base(wall)],
            [wall, cut],
        ];
        if direction < 0. {
            closure.reverse();
        }
        prism(
            builder,
            coat,
            closure,
            coat.recipe.depth * 0.5,
            Skin::Closure,
        );
    }
}

fn run_profile(coat: &Coat<'_>, direction: f32) -> [[f32; 2]; 4] {
    let high = coat.high_x();
    let cut = coat.eave_level();
    let inner = (coat.base(high) - cut) / coat.slope();
    let mut profile = [
        [high, coat.base(high)],
        [high + direction * inner, cut],
        [high + direction * coat.run_length(), cut],
        [high, coat.body_height(high)],
    ];
    if direction < 0. {
        profile.reverse();
    }
    profile
}

pub(super) fn smoothed(coat: &Coat<'_>) -> Vec<super::smooth::Surface> {
    let mut surfaces = Vec::with_capacity(3);
    for direction in [-1., 1.] {
        if direction > 0. && coat.recipe.roof.kind == RoofKind::Shed {
            break;
        }
        surfaces.push(super::smooth::prism(
            &run_profile(coat, direction),
            coat.half[1],
        ));
    }
    let high = coat.high_x();
    let width = coat.ridge_width();
    let floor = coat.ridge_floor();
    let shoulder = floor + 0.135;
    let crest = coat.body_height(high) + 0.16;
    let depth = coat.half[1] + super::RIDGE_PROJECTION;
    surfaces.push(if coat.recipe.roof.kind == RoofKind::Gable {
        super::smooth::prism(
            &[
                [-width, floor],
                [width, floor],
                [width, shoulder],
                [0., crest],
                [-width, shoulder],
            ],
            depth,
        )
    } else {
        super::smooth::prism(
            &[
                [high - width, floor],
                [high + 0.003, floor],
                [high + 0.003, crest],
                [high - width, shoulder],
            ],
            depth,
        )
    });
    surfaces
}

pub(super) fn ridge(builder: &mut Builder, coat: &Coat<'_>) {
    let high = coat.high_x();
    let width = coat.ridge_width();
    let floor = coat.ridge_floor();
    let shoulder = floor + 0.135;
    let crest = coat.body_height(high) + 0.16;
    let depth = coat.half[1] + super::RIDGE_PROJECTION;
    if coat.recipe.roof.kind == RoofKind::Gable {
        prism(
            builder,
            coat,
            [
                [-width, floor],
                [width, floor],
                [width, shoulder],
                [0., crest],
                [-width, shoulder],
            ],
            depth,
            Skin::Ridge,
        );
    } else {
        prism(
            builder,
            coat,
            [
                [high - width, floor],
                [high + 0.003, floor],
                [high + 0.003, crest],
                [high - width, shoulder],
            ],
            depth,
            Skin::Ridge,
        );
    }
}

fn prism<const N: usize>(
    builder: &mut Builder,
    coat: &Coat<'_>,
    profile: [[f32; 2]; N],
    depth: f32,
    skin: Skin,
) {
    if skin != Skin::Closure && coat.detail == Detail::Near {
        let surface = coat
            .surfaces
            .iter()
            .find(|surface| surface.matches(&profile, depth))
            .expect("prepared near-LOD control cage");
        emit_smooth(builder, coat, surface, skin);
        return;
    }
    for i in 0..N {
        let a = profile[i];
        let b = profile[(i + 1) % N];
        let normal = unit([b[1] - a[1], 0., a[0] - b[0]]);
        let section = coat.section((a[0] + b[0]) * 0.5);
        if normal[2] < -0.1 && skin != Skin::Ridge {
            underside(
                builder,
                coat,
                a,
                b,
                depth,
                skin == Skin::Run && (a[1] - b[1]).abs() < 0.0001,
            );
            continue;
        }
        let points = [
            [a[0], -depth, a[1]],
            [a[0], depth, a[1]],
            [b[0], depth, b[1]],
            [b[0], -depth, b[1]],
        ];
        if normal[2] > 0.1 {
            coat.emit(
                builder,
                points.map(|point| coat.vertex(section, point, normal)),
                false,
            );
        } else {
            coat.emit_end(
                builder,
                points.map(|point| coat.end_vertex(point, normal, skin == Skin::Ridge)),
                skin != Skin::Closure,
            );
        }
    }
    for side in [-1., 1.] {
        let normal = [0., side, 0.];
        let mut vertices = profile
            .map(|[x, z]| coat.end_vertex([x, side * depth, z], normal, skin == Skin::Ridge));
        if side > 0. {
            vertices.reverse();
        }
        coat.emit_end(builder, vertices, skin != Skin::Closure);
    }
}

fn underside(
    builder: &mut Builder,
    coat: &Coat<'_>,
    a: [f32; 2],
    b: [f32; 2],
    depth: f32,
    cut: bool,
) {
    let min = a[0].min(b[0]);
    let max = a[0].max(b[0]);
    if max - min < 0.0001 {
        return;
    }
    let slope = (b[1] - a[1]) / (b[0] - a[0]);
    let height = |x| a[1] + (x - a[0]) * slope;
    let normal = unit([slope, 0., -1.]);
    let wall = [coat.recipe.width * 0.5, coat.recipe.depth * 0.5];
    let mut xs = [
        min,
        (-wall[0]).clamp(min, max),
        wall[0].clamp(min, max),
        max,
    ];
    let mut ys = [
        -depth,
        (-wall[1]).clamp(-depth, depth),
        wall[1].clamp(-depth, depth),
        depth,
    ];
    xs.sort_by(f32::total_cmp);
    ys.sort_by(f32::total_cmp);
    for x in xs.windows(2).filter(|p| p[1] - p[0] > 0.0001) {
        for y in ys.windows(2).filter(|p| p[1] - p[0] > 0.0001) {
            let outside =
                ((x[0] + x[1]) * 0.5).abs() > wall[0] || ((y[0] + y[1]) * 0.5).abs() > wall[1];
            let points = [
                [x[0], y[1], height(x[0])],
                [x[1], y[1], height(x[1])],
                [x[1], y[0], height(x[1])],
                [x[0], y[0], height(x[0])],
            ];
            if outside || cut {
                coat.emit_end(
                    builder,
                    points.map(|point| coat.end_vertex(point, normal, false)),
                    cut && outside,
                );
            } else if coat.detail == Detail::Near {
                let section = coat.section((x[0] + x[1]) * 0.5);
                coat.emit(
                    builder,
                    points.map(|point| coat.vertex(section, point, normal)),
                    true,
                );
            }
        }
    }
}

fn emit_smooth(
    builder: &mut Builder,
    coat: &Coat<'_>,
    surface: &super::smooth::Surface,
    skin: Skin,
) {
    for patch in &surface.patches {
        let facing = patch.normal();
        let normal = if facing[1].abs() > 0.25 {
            [0., facing[1].signum(), 0.]
        } else {
            patch.source_normal
        };
        let section = coat.section(patch.vertices.iter().map(|v| v.point[0]).sum::<f32>() * 0.25);
        let vertices = patch.vertices.map(|v| {
            let mut vertex = if normal[2] > 0.1 {
                coat.vertex(section, v.point, normal)
            } else {
                coat.end_vertex(v.point, normal, skin == Skin::Ridge)
            };
            vertex.normal = v.normal;
            vertex
        });
        if normal[2] > 0.1 {
            coat.emit(builder, vertices, false);
        } else if normal[2] < -0.1 && skin == Skin::Run {
            smooth_underside(builder, coat, &vertices, normal[0].abs() < 0.0001);
        } else {
            coat.emit_end(builder, vertices, true);
        }
    }
}

fn smooth_underside(builder: &mut Builder, coat: &Coat<'_>, vertices: &[super::Vertex], cut: bool) {
    let wall = [coat.recipe.width * 0.5, coat.recipe.depth * 0.5];
    for (edge, high) in [(-wall[0], false), (wall[0], true)] {
        coat.emit_end_vertices(builder, &super::clip(vertices, 0, edge, high), cut);
    }
    let middle = super::clip(&super::clip(vertices, 0, -wall[0], true), 0, wall[0], false);
    for (edge, high) in [(-wall[1], false), (wall[1], true)] {
        coat.emit_end_vertices(builder, &super::clip(&middle, 1, edge, high), cut);
    }
    let inside = super::clip(&super::clip(&middle, 1, -wall[1], true), 1, wall[1], false);
    if cut {
        coat.emit_end_vertices(builder, &inside, false);
    } else {
        let section = coat.section(vertices.iter().map(|v| v.point[0]).sum::<f32>() / 4.);
        let inside: Vec<_> = inside
            .into_iter()
            .map(|v| coat.vertex(section, v.point, v.normal))
            .collect();
        coat.emit_vertices(builder, &inside, crate::MaterialSlot::Interior);
    }
}
