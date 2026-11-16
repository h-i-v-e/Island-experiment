use super::*;
use crate::{BuildingLod, BuildingRecipe, OpeningDimensions};

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Probe across a reveal, well away from triangle edges and neighbouring trim.
fn surfaces_at(lod: &BuildingLod, point: [f32; 3], normal: [f32; 3]) -> Vec<MaterialSlot> {
    let origin = std::array::from_fn(|axis| point[axis] + normal[axis] * 0.02);
    let direction = normal.map(|v| -v);
    lod.parts
        .iter()
        .flat_map(|part| {
            part.mesh.indices.chunks_exact(3).filter_map(|ids| {
                let [a, b, c] = [ids[0], ids[1], ids[2]].map(|i| part.mesh.positions[i as usize]);
                let edge = subtract(b, a);
                let other = subtract(c, a);
                let perpendicular = cross(direction, other);
                let determinant = dot(edge, perpendicular);
                if determinant.abs() < 0.000_001 {
                    return None;
                }
                let offset = subtract(origin, a);
                let first = dot(offset, perpendicular) / determinant;
                let second = dot(direction, cross(offset, edge)) / determinant;
                let distance = dot(other, cross(offset, edge)) / determinant;
                (first > 0.000_001
                    && second > 0.000_001
                    && first + second < 0.999_999
                    && (0. ..0.04).contains(&distance))
                .then_some(part.material)
            })
        })
        .collect()
}

fn aperture(recipe: &BuildingRecipe, wall: Wall, portal: bool) -> (BuildingLod, OpeningDimensions) {
    let plan = Plan::new(recipe);
    let mut builder = Builder::default();
    bay(&mut builder, &plan, wall, 0, portal, &mut Vec::new());
    let size = if portal {
        let door = &recipe.openings[0];
        OpeningDimensions {
            width: door.width,
            height: door.height + door.width * 0.866_025_4,
            sill: 0.,
        }
    } else {
        plan.window
    };
    (builder.finish(), size)
}

fn assert_reveal_ownership(recipe: &BuildingRecipe, wall: Wall, portal: bool) {
    let (lod, size) = aperture(recipe, wall, portal);
    let half = size.width * 0.5;
    let middle = wall.length() * 0.5;
    let spring = size.sill + size.height - 3_f32.sqrt() * half;
    let depth = recipe.wall_thickness * 0.37;
    let along = subtract(wall.point(1., 0., 0.), wall.point(0., 0., 0.));
    for side in [-1., 1.] {
        let point = wall.point(
            middle + side * half,
            size.sill + (spring - size.sill) * 0.43,
            depth,
        );
        let normal = along.map(|value| -side * value);
        assert_eq!(
            surfaces_at(&lod, point, normal),
            [MaterialSlot::Trim],
            "competing jamb faces: portal={portal}, thickness={}",
            recipe.wall_thickness
        );
        if portal {
            assert_eq!(
                surfaces_at(&lod, wall.point(middle + side * half, 0.01, depth), normal),
                [MaterialSlot::Interior],
                "the exposed masonry below the raised door frame must remain"
            );
        }
    }
    // Stay inside the wall thickness but outside the tracery's depth range.
    // Every actual head segment must have one lining surface, not a second
    // masonry cap occupying the same plane.
    for segment in 0..24_u16 {
        let xs = [segment, segment + 1].map(|step| -half + size.width * f32::from(step) / 24.);
        let zs = xs.map(|x| arch(x, half, size.sill, size.height));
        let length = (xs[1] - xs[0]).hypot(zs[1] - zs[0]);
        let normal = std::array::from_fn(|axis| {
            along[axis] * (zs[1] - zs[0]) / length
                - if axis == 2 {
                    (xs[1] - xs[0]) / length
                } else {
                    0.
                }
        });
        let point = wall.point(
            middle + xs[0] + (xs[1] - xs[0]) * 0.37,
            zs[0] + (zs[1] - zs[0]) * 0.37,
            recipe.wall_thickness * 0.15,
        );
        assert_eq!(
            surfaces_at(&lod, point, normal),
            [MaterialSlot::Trim],
            "competing head faces: portal={portal}, segment={segment}, thickness={}",
            recipe.wall_thickness
        );
    }
}

#[test]
fn window_and_door_reveals_have_one_visible_owner_on_every_wall_orientation() {
    let mut recipe = BuildingRecipe::gothic_church();
    for thickness in [0.08, 0.55, 0.6] {
        recipe.wall_thickness = thickness;
        for end in [[6., 0.], [0., 6.], [-6., 0.], [0., -6.]] {
            let wall = Wall {
                a: [0., 0.],
                b: end,
            };
            for portal in [false, true] {
                assert_reveal_ownership(&recipe, wall, portal);
            }
        }
    }
}
