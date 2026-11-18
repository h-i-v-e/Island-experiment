//! Engaged timber posts cover the ends of two frames at a wall junction.
use crate::{BuildingRecipe, Facade, MaterialSlot, geometry::Builder};

pub(crate) const CASING_MARGIN: f32 = 0.075;

pub(crate) fn opening_clearance(
    recipe: &BuildingRecipe,
    facade: Facade,
    point: [f32; 3],
    direction: [f32; 3],
    height: f32,
) -> f32 {
    let tangent = facade.tangent();
    let along = xy_dot(point, tangent);
    let sign = xy_dot(direction, tangent);
    recipe
        .openings
        .iter()
        .filter(|opening| {
            opening.facade == facade
                && recipe.floor_height(opening.storey) + opening.sill - CASING_MARGIN < height
        })
        .filter_map(|opening| {
            let distance = (opening.offset - along) * sign;
            let half = opening.width * 0.5 + CASING_MARGIN;
            (distance + half > 0.).then_some(distance - half)
        })
        .fold(recipe.walls.beam_width, f32::min)
}

pub(crate) fn xy_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

#[derive(Clone, Copy)]
pub(crate) struct Post {
    base: [[f32; 3]; 4],
    head: [[f32; 3]; 4],
}

impl Post {
    pub(crate) fn at(
        recipe: &BuildingRecipe,
        corner: [f32; 3],
        reach: [f32; 2],
        top: impl Fn([f32; 3]) -> f32,
    ) -> Option<Self> {
        // The two room-facing faces sit proud of the existing framing.
        if reach
            .iter()
            .any(|r| r.abs() < recipe.walls.frame_projection + 0.005)
        {
            return None;
        }
        let [a, b] = [0, 1].map(|i| corner[i] - reach[i].signum() * 0.008);
        let [c, d] = [0, 1].map(|i| corner[i] + reach[i]);
        let [x0, x1] = [a.min(c), a.max(c)];
        let [y0, y1] = [b.min(d), b.max(d)];
        let foot = corner[2] - 0.008;
        let base = [
            [x0, y0, foot],
            [x1, y0, foot],
            [x1, y1, foot],
            [x0, y1, foot],
        ];
        let head = base.map(|mut point| {
            point[2] = top(point);
            point
        });
        head.iter()
            .all(|p| p[2] > corner[2])
            .then_some(Self { base, head })
    }

    pub(crate) fn append(&self, builder: &mut Builder) {
        for edge in 0..4 {
            let next = (edge + 1) % 4;
            let face = [
                self.base[edge],
                self.base[next],
                self.head[next],
                self.head[edge],
            ];
            builder.quad(
                MaterialSlot::Timber,
                face,
                face.map(|[x, y, z]| [(x + y) * 0.5, z * 0.5]),
            );
        }
        builder.face(MaterialSlot::Timber, self.head);
        let mut bottom = self.base;
        bottom.reverse();
        builder.face(MaterialSlot::Timber, bottom);
    }

    pub(crate) fn points(&self) -> impl Iterator<Item = &[f32; 3]> {
        self.base.iter().chain(&self.head)
    }
}
