use super::{ChurchLayout, ChurchRecipe};
use crate::{BuildingRecipe, collision::BoxCollider};

#[derive(Clone, Copy)]
pub(super) struct Rect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}
impl Rect {
    pub fn bounds(self, bottom: f32, top: f32) -> BoxCollider {
        BoxCollider {
            min: [self.min[0], self.min[1], bottom],
            max: [self.max[0], self.max[1], top],
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Wall {
    pub a: [f32; 2],
    pub b: [f32; 2],
}
impl Wall {
    pub fn length(self) -> f32 {
        (self.b[0] - self.a[0]).hypot(self.b[1] - self.a[1])
    }
    pub fn point(self, u: f32, z: f32, inward: f32) -> [f32; 3] {
        let len = self.length();
        let t = [(self.b[0] - self.a[0]) / len, (self.b[1] - self.a[1]) / len];
        [
            self.a[0] + t[0] * u - t[1] * inward,
            self.a[1] + t[1] * u + t[0] * inward,
            z,
        ]
    }
    pub fn slice(self, a: f32, b: f32) -> Self {
        let a = self.point(a, 0., 0.);
        let b = self.point(b, 0., 0.);
        Self {
            a: [a[0], a[1]],
            b: [b[0], b[1]],
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Plan<'a> {
    pub r: &'a BuildingRecipe,
    pub c: &'a ChurchRecipe,
    pub n: f32,
    pub t: f32,
    pub y: f32,
    pub d: f32,
    pub w: f32,
    pub height: f32,
    pub window: crate::OpeningDimensions,
    pub window_chart: usize,
    tower_shell: bool,
}
impl<'a> Plan<'a> {
    pub fn new(r: &'a BuildingRecipe) -> Self {
        let c = r.church.as_ref().expect("church formula");
        Self {
            r,
            c,
            n: c.nave_width * 0.5,
            t: c.transept_depth * 0.5,
            y: c.crossing_offset,
            d: r.depth * 0.5,
            w: r.width * 0.5,
            height: r.wall_height,
            window: r.opening_defaults.window,
            window_chart: 2,
            tower_shell: false,
        }
    }
    /// Resolve actual tower-wall dimensions before generating circular tracery.
    pub fn at_height(self, height: f32) -> Self {
        let sill = self.window.sill.min(height * 0.18);
        let window_height = self.window.height.min(height - sill - 0.4);
        let width = self.window.width.min((window_height - 0.8) / 0.87);
        Self {
            height,
            window: crate::OpeningDimensions {
                width,
                height: window_height,
                sill,
            },
            window_chart: 7,
            tower_shell: true,
            ..self
        }
    }
    /// Interior skins meet on the offset outline, rather than continuing behind
    /// the neighbouring wall's exterior skin. Bay subdivisions retain the same
    /// corner ownership as their original perimeter edge.
    pub fn masonry_point(&self, wall: Wall, u: f32, z: f32, inward: f32) -> [f32; 3] {
        wall.point(u + self.corner_offset(wall, u) * inward, z, inward)
    }

    pub fn masonry_depth(&self, wall: Wall, u: f32) -> f32 {
        let mut depth = self.r.wall_thickness;
        if self.corner_offset(wall, 0.) > 0. {
            depth = depth.min(u);
        }
        if self.corner_offset(wall, wall.length()) < 0. {
            depth = depth.min(wall.length() - u);
        }
        depth
    }

    pub fn concave_corners(&self) -> impl Iterator<Item = BoxCollider> {
        let outline = self.outline();
        let thickness = self.r.wall_thickness;
        (0..outline.len()).filter_map(move |i| {
            let a = outline[(i + outline.len() - 1) % outline.len()];
            let b = outline[i];
            let c = outline[(i + 1) % outline.len()];
            let incoming = [b[0] - a[0], b[1] - a[1]];
            let outgoing = [c[0] - b[0], c[1] - b[1]];
            if incoming[0] * outgoing[1] - incoming[1] * outgoing[0] >= 0. {
                return None;
            }
            let wall = Wall { a: b, b: c };
            let corner = self.masonry_point(wall, 0., 0., thickness);
            Some(BoxCollider {
                min: [b[0].min(corner[0]), b[1].min(corner[1]), 0.],
                max: [b[0].max(corner[0]), b[1].max(corner[1]), self.height],
            })
        })
    }

    fn corner_offset(&self, wall: Wall, u: f32) -> f32 {
        if self.tower_shell {
            self.towers()
                .into_iter()
                .find_map(|rect| {
                    let [a, b] = rect.min;
                    let [c, d] = rect.max;
                    corner_offset(&[[a, b], [c, b], [c, d], [a, d]], wall, u)
                })
                .unwrap_or(0.)
        } else {
            corner_offset(&self.outline(), wall, u).unwrap_or(0.)
        }
    }
    pub fn outline(&self) -> [[f32; 2]; 16] {
        let Self {
            n, t, y, d, w, c, ..
        } = *self;
        let q = c.tower_width;
        [
            [-n - q, -d],
            [n + q, -d],
            [n + q, -d + q],
            [n, -d + q],
            [n, y - t],
            [w, y - t],
            [w, y + t],
            [n, y + t],
            [n, d],
            [-n, d],
            [-n, y + t],
            [-w, y + t],
            [-w, y - t],
            [-n, y - t],
            [-n, -d + q],
            [-n - q, -d + q],
        ]
    }
    pub fn walls(&self) -> impl Iterator<Item = Wall> {
        let outline = self.outline();
        (0..outline.len()).map(move |i| Wall {
            a: outline[i],
            b: outline[(i + 1) % outline.len()],
        })
    }
    pub fn floors(&self) -> [Rect; 5] {
        let Self {
            n, t, y, d, w, c, ..
        } = *self;
        let q = c.tower_width;
        [
            Rect {
                min: [-n, -d],
                max: [n, d],
            },
            Rect {
                min: [-w, y - t],
                max: [-n, y + t],
            },
            Rect {
                min: [n, y - t],
                max: [w, y + t],
            },
            Rect {
                min: [-n - q, -d],
                max: [-n, -d + q],
            },
            Rect {
                min: [n, -d],
                max: [n + q, -d + q],
            },
        ]
    }
    pub fn towers(&self) -> [Rect; 2] {
        [self.floors()[3], self.floors()[4]]
    }
    pub fn layout(&self) -> ChurchLayout {
        ChurchLayout {
            outline: self.outline().to_vec(),
            nave: self.floors()[0].bounds(0., self.r.wall_height),
            transept: Rect {
                min: [-self.w, self.y - self.t],
                max: [self.w, self.y + self.t],
            }
            .bounds(0., self.r.wall_height),
            towers: self.towers().map(|r| r.bounds(0., self.c.tower_height)),
        }
    }
    pub fn buttress_rows(&self) -> impl Iterator<Item = f32> + '_ {
        [
            (-self.d + self.c.tower_width, self.y - self.t),
            (self.y + self.t, self.d),
        ]
        .into_iter()
        .flat_map(|(a, b)| {
            let count = ((b - a) / self.c.bay_spacing).floor().max(1.) as u16;
            (0..=count).map(move |i| {
                (a + (b - a) * f32::from(i) / f32::from(count)).clamp(a + 0.35, b - 0.35)
            })
        })
    }

    pub fn buttresses(&self) -> impl Iterator<Item = super::buttress::Layout> + '_ {
        self.buttress_rows()
            .flat_map(|y| [-1., 1.].map(|side| super::buttress::Layout::new(self, side, y)))
    }
}

fn corner_offset(outline: &[[f32; 2]], wall: Wall, u: f32) -> Option<f32> {
    let point = wall.point(u, 0., 0.);
    let direction = [wall.b[0] - wall.a[0], wall.b[1] - wall.a[1]];
    let same_point =
        |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 0.0001 && (a[1] - b[1]).abs() < 0.0001;
    let delta = |a: [f32; 2], b: [f32; 2]| [b[0] - a[0], b[1] - a[1]];
    let cross = |a: [f32; 2], b: [f32; 2]| a[0] * b[1] - a[1] * b[0];
    for i in 0..outline.len() {
        let a = outline[i];
        let b = outline[(i + 1) % outline.len()];
        let edge = delta(a, b);
        if cross(edge, direction).abs() > 0.0001
            || edge[0] * direction[0] + edge[1] * direction[1] <= 0.
        {
            continue;
        }
        if same_point([point[0], point[1]], a) {
            let previous = outline[(i + outline.len() - 1) % outline.len()];
            return Some(cross(delta(previous, a), edge).signum());
        }
        if same_point([point[0], point[1]], b) {
            let next = outline[(i + 2) % outline.len()];
            return Some(-cross(edge, delta(b, next)).signum());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_shared_interior_corners(plan: &Plan<'_>, outline: &[[f32; 2]]) {
        for i in 0..outline.len() {
            let previous = Wall {
                a: outline[(i + outline.len() - 1) % outline.len()],
                b: outline[i],
            };
            let next = Wall {
                a: outline[i],
                b: outline[(i + 1) % outline.len()],
            };
            let a = plan.masonry_point(previous, previous.length(), 1., plan.r.wall_thickness);
            let b = plan.masonry_point(next, 0., 1., plan.r.wall_thickness);
            assert!(a.into_iter().zip(b).all(|(a, b)| (a - b).abs() < 0.0001));
            // Splitting an edge into bays must not create a new corner or lose
            // the original endpoint's ownership.
            let bay = next.slice(0., next.length() * 0.5);
            assert!(
                plan.masonry_point(bay, 0., 1., plan.r.wall_thickness)
                    .into_iter()
                    .zip(b)
                    .all(|(a, b)| (a - b).abs() < 0.0001)
            );
            assert!(plan.corner_offset(bay, bay.length()).abs() < 0.0001);
        }
    }

    #[test]
    fn convex_and_concave_wall_interiors_meet_at_shared_miters() {
        for thickness in [0.08, 0.55, 0.6] {
            let mut recipe = BuildingRecipe::gothic_church();
            recipe.wall_thickness = thickness;
            let plan = Plan::new(&recipe);
            assert_shared_interior_corners(&plan, &plan.outline());
            let tower_plan = plan.at_height(7.);
            for rect in plan.towers() {
                let [a, b] = rect.min;
                let [c, d] = rect.max;
                assert_shared_interior_corners(&tower_plan, &[[a, b], [c, b], [c, d], [a, d]]);
            }
        }
    }

    #[test]
    fn concave_corner_colliders_fill_only_the_interior_miter_squares() {
        let recipe = BuildingRecipe::gothic_church();
        let plan = Plan::new(&recipe);
        let corners: Vec<_> = plan.concave_corners().collect();
        assert_eq!(corners.len(), 6);
        for corner in corners {
            for axis in [0, 1] {
                assert!(
                    (corner.max[axis] - corner.min[axis] - recipe.wall_thickness).abs() < 0.0001
                );
            }
            let centre = [
                (corner.min[0] + corner.max[0]) * 0.5,
                (corner.min[1] + corner.max[1]) * 0.5,
            ];
            assert!(plan.floors().iter().any(|floor| {
                (0..2).all(|axis| centre[axis] > floor.min[axis] && centre[axis] < floor.max[axis])
            }));
            assert!(corner.min[2].abs() < 0.0001);
            assert!((corner.max[2] - plan.height).abs() < 0.0001);
        }
    }
}
