//! Exposed LOD0 joists span the window walls at the wall-plate soffit.
use super::{Builder, BuildingRecipe, MaterialSlot, WallStyle, roof_overlap};
use crate::collision::BoxCollider;

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

fn timber_box(builder: &mut Builder, beam: BoxCollider) {
    let start = builder.mesh(MaterialSlot::Timber).positions.len();
    builder.cube(MaterialSlot::Timber, beam.min, beam.max);
    let mesh = builder.mesh(MaterialSlot::Timber);
    for ((point, normal), uv) in mesh.positions[start..]
        .iter()
        .zip(&mesh.normals[start..])
        .zip(&mut mesh.uvs[start..])
    {
        // Wood grain follows the joist along X; end caps use their local Y/Z face.
        *uv = if normal[0].abs() > 0.5 {
            [point[1] * 0.5, point[2] * 0.5]
        } else {
            [
                if normal[1].abs() > 0.5 {
                    point[2]
                } else {
                    point[1]
                } * 0.5,
                point[0] * 0.5,
            ]
        };
    }
}

pub(crate) fn generate(
    builder: &mut Builder,
    recipe: &BuildingRecipe,
    colliders: &mut Vec<BoxCollider>,
) {
    for beam in boxes(recipe) {
        timber_box(builder, beam);
        colliders.push(beam);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Facade;

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
