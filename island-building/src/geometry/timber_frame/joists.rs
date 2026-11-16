//! Exposed LOD0 joists span the window walls at the wall-plate soffit.
use super::{Builder, BuildingRecipe, MaterialSlot, WallStyle, roof_overlap};
use crate::collision::BoxCollider;

// Stand proud of adjacent wall planes instead of sharing their draw surface.
pub(crate) const RIM_INSET: f32 = 0.015;

pub(crate) struct JoistGrid {
    pub origin: f32,
    pub spacing: f32,
    bays: u16,
}

// Validated room depths and bay widths require at most 54 equal bays.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn grid(recipe: &BuildingRecipe) -> Option<JoistGrid> {
    if recipe.walls.style != WallStyle::WattleAndDaub {
        return None;
    }
    let inner = recipe.depth * 0.5 - recipe.wall_thickness - recipe.walls.frame_projection;
    // Include a virtual end beam at each wall, giving equal clear gaps at both ends too.
    let span = inner * 2. + recipe.walls.beam_width;
    let bays = (span / recipe.walls.bay_width).ceil().max(2.) as u16;
    Some(JoistGrid {
        origin: -span * 0.5,
        spacing: span / f32::from(bays),
        bays,
    })
}

fn stations(recipe: &BuildingRecipe) -> impl Iterator<Item = f32> {
    grid(recipe)
        .into_iter()
        .flat_map(|grid| (1..grid.bays).map(move |i| grid.origin + f32::from(i) * grid.spacing))
}

fn chimney_clearance(recipe: &BuildingRecipe) -> Option<BoxCollider> {
    let chimney = recipe.chimney.as_ref()?;
    let a = chimney.facade.point(
        recipe,
        chimney.offset - chimney.width * 0.5,
        0.,
        recipe.wall_thickness,
    );
    let b = chimney.facade.point(
        recipe,
        chimney.offset + chimney.width * 0.5,
        recipe.wall_height,
        recipe.wall_thickness + chimney.depth,
    );
    Some(BoxCollider {
        min: std::array::from_fn(|i| a[i].min(b[i])),
        max: std::array::from_fn(|i| a[i].max(b[i])),
    })
}

fn split(beam: BoxCollider, clearance: Option<BoxCollider>) -> [Option<BoxCollider>; 2] {
    let Some(cut) =
        clearance.filter(|c| (0..2).all(|i| beam.min[i] < c.max[i] && beam.max[i] > c.min[i]))
    else {
        return [Some(beam), None];
    };
    let mut left = beam;
    left.max[0] = cut.min[0].max(beam.min[0]);
    let mut right = beam;
    right.min[0] = cut.max[0].min(beam.max[0]);
    [left, right].map(|b| (b.max[0] - b.min[0] > 0.001).then_some(b))
}

fn boxes(recipe: &BuildingRecipe) -> impl Iterator<Item = BoxCollider> {
    let width = recipe.walls.beam_width;
    let half = recipe.width * 0.5 - recipe.wall_thickness - recipe.walls.frame_projection;
    let top = recipe.wall_height + roof_overlap(recipe);
    let clearance = chimney_clearance(recipe);
    stations(recipe).flat_map(move |y| {
        split(
            BoxCollider {
                min: [-half, y - width * 0.5, top - width],
                max: [half, y + width * 0.5, top],
            },
            clearance,
        )
        .into_iter()
        .flatten()
    })
}

fn timber_box(builder: &mut Builder, beam: BoxCollider, along: usize) {
    let across = 1 - along;
    let start = builder.mesh(MaterialSlot::Timber).positions.len();
    builder.cube(MaterialSlot::Timber, beam.min, beam.max);
    let mesh = builder.mesh(MaterialSlot::Timber);
    for ((point, normal), uv) in mesh.positions[start..]
        .iter()
        .zip(&mesh.normals[start..])
        .zip(&mut mesh.uvs[start..])
    {
        // Grain follows each joist or trimmer; end caps use the transverse face.
        *uv = if normal[along].abs() > 0.5 {
            [point[across] * 0.5, point[2] * 0.5]
        } else {
            [
                if normal[across].abs() > 0.5 {
                    point[2]
                } else {
                    point[across]
                } * 0.5,
                point[along] * 0.5,
            ]
        };
    }
}

pub(crate) fn generate(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    colliders: &mut Vec<BoxCollider>,
) {
    generate_with_cuts(builder, recipe, &[], colliders);
}

pub(crate) fn generate_with_cuts(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    cuts: &[BoxCollider],
    colliders: &mut Vec<BoxCollider>,
) {
    let width = recipe.walls.beam_width;
    let framed_cuts: Vec<_> = cuts.iter().map(|cut| expanded(*cut, width)).collect();
    for beam in boxes(recipe) {
        for piece in crate::storeys::cells(beam, &framed_cuts) {
            timber_box(builder, piece, 0);
            colliders.push(piece);
        }
    }
    for &cut in cuts {
        for (index, beam) in opening_frame(recipe, cut).into_iter().enumerate() {
            timber_box(builder, beam, usize::from(index < 2));
            colliders.push(beam);
        }
    }
}

fn expanded(mut cut: BoxCollider, width: f32) -> BoxCollider {
    for axis in 0..2 {
        cut.min[axis] -= width;
        cut.max[axis] += width;
    }
    cut
}

/// Four butt-jointed timbers sit beneath the floorboards, just proud of the wall faces.
fn opening_frame(recipe: &BuildingRecipe, cut: BoxCollider) -> [BoxCollider; 4] {
    let width = recipe.walls.beam_width;
    let top = recipe.wall_height;
    let [left, near, _] = cut.min;
    let [right, far, _] = cut.max;
    let half = [recipe.width, recipe.depth].map(|size| size * 0.5 - recipe.wall_thickness * 0.5);
    [
        (
            [left - width, near - width],
            [left + RIM_INSET, far + width],
        ),
        (
            [right - RIM_INSET, near - width],
            [right + width, far + width],
        ),
        (
            [left + RIM_INSET, near - width],
            [right - RIM_INSET, near + RIM_INSET],
        ),
        (
            [left + RIM_INSET, far - RIM_INSET],
            [right - RIM_INSET, far + width],
        ),
    ]
    .map(|(min, max)| BoxCollider {
        min: [min[0].max(-half[0]), min[1].max(-half[1]), top - width],
        max: [max[0].min(half[0]), max[1].min(half[1]), top],
    })
}

pub(crate) fn landing_header(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    stairs: &crate::StaircaseLayout,
    colliders: &mut Vec<BoxCollider>,
) {
    // The landing projects into the slab cutout. A shallower header beneath its
    // open edge closes the rim without reducing headroom over the first flight.
    let top = -crate::stairs::TREAD_THICKNESS;
    let beam = stairs.rectangle(
        [
            0.,
            stairs.landing_depth - recipe.walls.beam_width,
            (-recipe.floor_thickness).min(top - 0.02),
        ],
        [stairs.span(), stairs.landing_depth + RIM_INSET, top],
    );
    timber_box(
        builder,
        beam,
        usize::from(stairs.axis == crate::StairAxis::X),
    );
    colliders.push(beam);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Facade;

    #[test]
    fn opening_has_a_continuous_timber_rim_and_joists_abut_its_outer_faces() {
        let mut recipe = BuildingRecipe::hut();
        recipe.chimney = None;
        let cut = BoxCollider {
            min: [-0.5, -1., 0.],
            max: [0.5, 1., 0.],
        };
        let width = recipe.walls.beam_width;
        for style in [WallStyle::Plain, WallStyle::WattleAndDaub] {
            recipe.walls.style = style;
            let mut builder = Builder::default();
            let mut colliders = Vec::new();
            generate_with_cuts(&mut builder, &recipe, &[cut], &mut colliders);
            let frame = opening_frame(&recipe, cut);
            assert!(frame.iter().all(|b| colliders.contains(b)));
            for beam in frame {
                assert!((beam.max[2] - recipe.wall_height).abs() < 0.00001);
                assert!((beam.max[2] - beam.min[2] - width).abs() < 0.00001);
                assert!(
                    !(0..2).all(|i| beam.min[i] < cut.max[i] - RIM_INSET - 0.00001
                        && beam.max[i] > cut.min[i] + RIM_INSET + 0.00001)
                );
            }
            assert!((frame[0].max[0] - cut.min[0] - RIM_INSET).abs() < 0.00001);
            assert!((frame[1].min[0] - cut.max[0] + RIM_INSET).abs() < 0.00001);
            assert!((frame[2].max[1] - cut.min[1] - RIM_INSET).abs() < 0.00001);
            assert!((frame[3].min[1] - cut.max[1] + RIM_INSET).abs() < 0.00001);
            // Headers meet both side trimmers without gaps or overlapping corners.
            for header in &frame[2..] {
                assert!((header.min[0] - frame[0].max[0]).abs() < 0.00001);
                assert!((header.max[0] - frame[1].min[0]).abs() < 0.00001);
            }
            if style == WallStyle::WattleAndDaub {
                let joists = &colliders[..colliders.len() - 4];
                assert!(
                    joists
                        .iter()
                        .any(|b| (b.max[0] - frame[0].min[0]).abs() < 0.00001)
                );
                assert!(
                    joists
                        .iter()
                        .any(|b| (b.min[0] - frame[1].max[0]).abs() < 0.00001)
                );
                for beam in joists {
                    let clearance = expanded(cut, width);
                    assert!(!(0..2).all(|i| beam.min[i] < clearance.max[i] - 0.00001
                        && beam.max[i] > clearance.min[i] + 0.00001));
                }
            }
            let mesh = &builder.finish().parts.remove(0).mesh;
            assert_eq!(mesh.positions.len(), colliders.len() * 24);
        }
    }

    #[test]
    fn joists_are_evenly_spaced_independent_of_openings_and_keep_longitudinal_grain() {
        let mut recipe = BuildingRecipe::hut();
        recipe.walls.style = WallStyle::WattleAndDaub;
        recipe.chimney = None;
        for depth in [3., 5., 40.] {
            for bay in [0.75, 1.5, 3.] {
                recipe.depth = depth;
                recipe.walls.bay_width = bay;
                let rows: Vec<_> = stations(&recipe).collect();
                assert!(!rows.is_empty());
                let inner = depth * 0.5 - recipe.wall_thickness - recipe.walls.frame_projection;
                let width = recipe.walls.beam_width;
                let end_gap = rows[0] - width * 0.5 + inner;
                assert!(end_gap > 0.);
                assert!((inner - rows[rows.len() - 1] - width * 0.5 - end_gap).abs() < 0.00001);
                for pair in rows.windows(2) {
                    assert!(
                        (pair[1] - pair[0] - width - end_gap).abs() < 0.00001,
                        "all interior and end gaps are equal"
                    );
                    assert!(pair[1] - pair[0] <= bay + 0.00001);
                }
                recipe.openings[1].offset += 0.2;
                recipe.openings[2].width += 0.1;
                assert!(
                    rows.iter()
                        .zip(stations(&recipe))
                        .all(|(a, b)| (a - b).abs() < 0.00001),
                    "moving or resizing windows does not move joists"
                );
            }
        }
        recipe.depth = 5.;
        recipe.walls.bay_width = 1.5;
        let beams: Vec<_> = boxes(&recipe).collect();
        assert!(!beams.is_empty());
        for beam in &beams {
            assert!((beam.min[2] - (recipe.wall_height - recipe.walls.beam_width)).abs() < 0.00001);
        }
        let mut builder = Builder::default();
        let mut colliders = Vec::new();
        generate(&mut builder, &recipe, &mut colliders);
        assert_eq!(colliders, beams);
        let mesh = &builder.finish().parts.remove(0).mesh;
        for ((points, normals), uvs) in mesh
            .positions
            .chunks_exact(4)
            .zip(mesh.normals.chunks_exact(4))
            .zip(mesh.uvs.chunks_exact(4))
        {
            if normals[0][0].abs() > 0.5 {
                continue;
            }
            for i in 1..4 {
                assert!(
                    ((uvs[i][1] - uvs[0][1]) - (points[i][0] - points[0][0]) * 0.5).abs() < 0.00001,
                    "longitudinal wood grain on the sides and underside"
                );
            }
        }
    }

    #[test]
    fn joists_and_collision_leave_the_full_chimney_breast_and_flue_clear_on_all_walls() {
        let mut recipe = BuildingRecipe::hut();
        assert_eq!(
            boxes(&recipe).count(),
            0,
            "plain walls retain their ceiling"
        );
        recipe.walls.style = WallStyle::WattleAndDaub;
        for facade in Facade::ALL {
            recipe.chimney.as_mut().unwrap().facade = facade;
            for offset in [-0.45, 0., 0.45] {
                recipe.chimney.as_mut().unwrap().offset = offset;
                let cut = chimney_clearance(&recipe).unwrap();
                let beams: Vec<_> = boxes(&recipe).collect();
                assert!(!beams.is_empty());
                for beam in &beams {
                    assert!(beam.max[0] > beam.min[0]);
                    assert!(!(0..2).all(|i| beam.min[i] < cut.max[i] - 0.00001
                        && beam.max[i] > cut.min[i] + 0.00001));
                }
                let mut builder = Builder::default();
                let mut colliders = Vec::new();
                generate(&mut builder, &recipe, &mut colliders);
                assert_eq!(colliders, beams, "collision uses exactly the cut geometry");
            }
        }
    }
}
