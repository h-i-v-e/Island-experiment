use super::{
    atlas,
    plan::{Plan, Rect, Wall},
    shell, windows,
};
use crate::{MaterialSlot, collision::BoxCollider, geometry::Builder};

pub(super) fn generate(b: &mut Builder, p: &Plan<'_>, level: u8, colliders: &mut Vec<BoxCollider>) {
    for tower in p.towers() {
        tower_mesh(b, p, tower, level, colliders);
    }
    for layout in p.buttresses() {
        buttress(b, &layout, level, colliders);
    }
    if level == 0 {
        ribs(b, p, colliders);
    }
}
fn tower_mesh(
    b: &mut Builder,
    p: &Plan<'_>,
    rect: Rect,
    level: u8,
    colliders: &mut Vec<BoxCollider>,
) {
    let [a, c] = rect.min;
    let [d, e] = rect.max;
    let top = p.c.tower_height;
    let upper_plan = p.at_height(top - p.r.wall_height);
    let mut upper = Builder::default();
    let mut obstacles = Vec::new();
    for wall in [
        Wall {
            a: [a, c],
            b: [d, c],
        },
        Wall {
            a: [d, c],
            b: [d, e],
        },
        Wall {
            a: [d, e],
            b: [a, e],
        },
        Wall {
            a: [a, e],
            b: [a, c],
        },
    ] {
        shell::bays(&mut upper, &upper_plan, wall, level, &mut obstacles);
    }
    b.append(upper.finish(), p.r.wall_height);
    if level == 0 {
        colliders.extend(obstacles.into_iter().map(|mut c| {
            c.min[2] += p.r.wall_height;
            c.max[2] += p.r.wall_height;
            c
        }));
    }
    for z in [p.r.wall_height - 0.1, top - 0.35] {
        band(b, rect, z, level);
    }
    cube(
        b,
        level,
        1,
        MaterialSlot::Roof,
        [a, c, top - 0.12],
        [d, e, top],
    );
    let h = 0.85;
    let t = 0.25;
    for (min, max) in [
        ([a, c, top], [d, c + t, top + h]),
        ([a, e - t, top], [d, e, top + h]),
        ([a, c + t, top], [a + t, e - t, top + h]),
        ([d - t, c + t, top], [d, e - t, top + h]),
    ] {
        if level == 0 {
            // A low continuous parapet with separated merlons above it.
            let mut base = max;
            base[2] = top + 0.35;
            cube(b, level, 0, MaterialSlot::Wall, min, base);
            let along = usize::from(max[1] - min[1] > max[0] - min[0]);
            let len = max[along] - min[along];
            let count = (len / 0.8).floor().max(1.) as u16;
            for i in 0..count {
                let mut lo = min;
                let mut hi = max;
                lo[2] = top + 0.35;
                lo[along] += len * f32::from(i) / f32::from(count);
                hi[along] = lo[along] + len / f32::from(count) * 0.55;
                cube(b, level, 0, MaterialSlot::Wall, lo, hi);
            }
        } else {
            cube(b, level, 0, MaterialSlot::Wall, min, max);
        }
    }
    for x in [a + 0.2, d - 0.2] {
        for y in [c + 0.2, e - 0.2] {
            pinnacle(b, [x, y, top + 0.7], 2.3, top - 0.02, level);
        }
    }
}
fn band(b: &mut Builder, r: Rect, z: f32, level: u8) {
    let t = 0.14;
    for (min, max) in [
        (
            [r.min[0] - t, r.min[1] - t, z],
            [r.max[0] + t, r.min[1] + t, z + 0.22],
        ),
        (
            [r.min[0] - t, r.max[1] - t, z],
            [r.max[0] + t, r.max[1] + t, z + 0.22],
        ),
        (
            [r.min[0] - t, r.min[1] + t, z],
            [r.min[0] + t, r.max[1] - t, z + 0.22],
        ),
        (
            [r.max[0] - t, r.min[1] + t, z],
            [r.max[0] + t, r.max[1] - t, z + 0.22],
        ),
    ] {
        cube(b, level, 4, MaterialSlot::Trim, min, max);
    }
}
fn buttress(
    b: &mut Builder,
    layout: &super::buttress::Layout,
    level: u8,
    colliders: &mut Vec<BoxCollider>,
) {
    let [x, y] = layout.wall.b;
    let pier = layout.pier;
    let pier_height = pier.max[2];
    cube(b, level, 0, MaterialSlot::Wall, pier.min, pier.max);
    cube(
        b,
        level,
        4,
        MaterialSlot::Trim,
        [x - 0.52, y - 0.55, pier_height - 0.1],
        [x + 0.52, y + 0.55, pier_height + 0.15],
    );
    pinnacle(
        b,
        [x, y, pier_height + 0.15],
        2.0,
        pier_height + 0.15,
        level,
    );
    layout.arch.generate(b, layout.wall, level, colliders);
    if level == 0 {
        colliders.push(pier);
    }
}
fn pinnacle(b: &mut Builder, origin: [f32; 3], height: f32, support_base: f32, level: u8) {
    let [x, y, z] = origin;
    let base_half = 0.24;
    let radius = base_half - 0.025;
    let shoulder = z + height * 0.35;
    cube(
        b,
        level,
        4,
        MaterialSlot::Trim,
        [x - base_half, y - base_half, support_base],
        // Bury the cone foot slightly into the pedestal to close the joint.
        [x + base_half, y + base_half, shoulder + 0.02],
    );
    let count = if level == 0 { 8_u16 } else { 4 };
    for i in 0..count {
        let angles =
            [f32::from(i), f32::from(i + 1)].map(|i| i / f32::from(count) * std::f32::consts::TAU);
        let q = angles.map(|a| [x + radius * a.cos(), y + radius * a.sin(), shoulder]);
        let tri = [q[0], q[1], [x, y, z + height]];
        if level == 2 {
            atlas::triangle(b, 4, tri);
        } else {
            b.triangle(
                MaterialSlot::Trim,
                tri,
                [[0., 0.], [0.3, 0.], [0.15, height * 0.5]],
            );
        }
    }
}
fn ribs(b: &mut Builder, p: &Plan<'_>, colliders: &mut Vec<BoxCollider>) {
    let half = p.n - p.r.wall_thickness - 0.12;
    let spring = rib_spring(p, half);
    for y in p.buttress_rows() {
        let wall = Wall {
            a: [-half, y],
            b: [half, y],
        };
        let profile: [_; 33] = std::array::from_fn(|i| {
            let x = -half + 2. * half * i as f32 / 32.;
            [
                x + half,
                windows::arch(x, half, spring, 3_f32.sqrt() * half),
                shell::nave_ceiling_height(p, x) + 0.01,
            ]
        });
        windows::prism_strip(b, wall, &profile, [-0.13, 0.13], MaterialSlot::Trim);
        for (i, span) in profile.windows(2).enumerate() {
            let xs = [i, i + 1].map(|i| -half + 2. * half * i as f32 / 32.);
            colliders.push(BoxCollider {
                min: [xs[0], y - 0.13, span[0][1].min(span[1][1])],
                max: [
                    xs[1],
                    y + 0.13,
                    shell::nave_ceiling_height(p, xs[0]).max(shell::nave_ceiling_height(p, xs[1]))
                        + 0.01,
                ],
            });
        }
        for x in [-half, half] {
            colliders.push(BoxCollider {
                min: [x - 0.13, y - 0.16, 0.02],
                max: [x + 0.13, y + 0.16, spring + 0.04],
            });
            b.cube(
                MaterialSlot::Trim,
                [x - 0.13, y - 0.16, 0.02],
                [x + 0.13, y + 0.16, spring + 0.04],
            );
        }
    }
}

// The smallest gap between a circular arch and the straight roof occurs where
// their slopes match. Lower the complete arch only when a steep roof needs it.
fn rib_spring(p: &Plan<'_>, half: f32) -> f32 {
    let slope = p.r.roof.rise / p.n;
    let x = (2. * half * slope / slope.hypot(1.) - half).clamp(0., half);
    let rise = 3_f32.sqrt() * half;
    let desired = p.r.wall_height + p.r.roof.rise - 1.2 - rise;
    desired.min(shell::nave_ceiling_height(p, x) - 0.13 - windows::arch(x, half, 0., rise))
}

fn cube(
    b: &mut Builder,
    level: u8,
    chart: usize,
    slot: MaterialSlot,
    min: [f32; 3],
    max: [f32; 3],
) {
    if level == 2 {
        atlas::cube(b, chart, min, max);
    } else {
        b.cube(slot, min, max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingMesh, BuildingRecipe};

    fn hits_stone(mesh: &BuildingMesh, x: f32, z: f32) -> bool {
        mesh.indices.chunks_exact(3).any(|ids| {
            let [a, b, c] = [ids[0], ids[1], ids[2]].map(|i| mesh.positions[i as usize]);
            let determinant = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
            if determinant.abs() < 0.000_001 {
                return false;
            }
            let u = ((x - a[0]) * (c[2] - a[2]) - (z - a[2]) * (c[0] - a[0])) / determinant;
            let v = ((b[0] - a[0]) * (z - a[2]) - (b[2] - a[2]) * (x - a[0])) / determinant;
            u >= -0.00001 && v >= -0.00001 && u + v <= 1.00001
        })
    }

    #[test]
    fn ceiling_arches_fill_to_the_roof_without_blocking_the_passage() {
        for (nave, rise) in [(9., 5.), (6., 4.), (5., 6.)] {
            let mut r = BuildingRecipe::gothic_church();
            r.church.as_mut().unwrap().nave_width = nave;
            r.roof.rise = rise;
            let plan = Plan::new(&r);
            let mut b = Builder::default();
            let mut colliders = Vec::new();
            ribs(&mut b, &plan, &mut colliders);
            let mesh = b.finish().parts.remove(0).mesh;
            let half = nave * 0.5 - r.wall_thickness - 0.12;
            let ceiling = |x: f32| r.wall_height + rise * (1. - x.abs() / (nave * 0.5)) - 0.15;
            let y = plan.buttress_rows().next().unwrap();
            for x in [-half * 0.75, 0., half * 0.75] {
                assert!(
                    hits_stone(&mesh, x, ceiling(x) - 0.04),
                    "gap below roof: {nave}/{rise}, {x}"
                );
                assert!(!hits_stone(&mesh, x, ceiling(x) + 0.04));
                assert!(!hits_stone(&mesh, x, 1.8));
                let point = [x, y, ceiling(x) - 0.04];
                assert!(colliders.iter().any(|c| {
                    (0..3).all(|axis| point[axis] >= c.min[axis] && point[axis] <= c.max[axis])
                }));
            }
            assert!(mesh.positions.iter().all(|p| p[2] <= ceiling(p[0]) + 0.011));
        }
    }
}
