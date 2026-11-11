//! Georgian six-panel joinery: shared proportions for the leaf, proxy and atlas.
use super::Builder;
use crate::{BuildingMesh, BuildingRecipe, MaterialSlot, OpeningRecipe};

const PROFILE: [[f32; 2]; 5] = [
    [0., 0.],
    [0.004, 0.],
    [0.010, 0.006],
    [0.016, 0.013],
    [0.042, 0.002],
];
const COARSE: [[f32; 2]; 3] = [[0., 0.], [0.018, 0.012], [0.042, 0.002]];

#[derive(Clone, Copy)]
struct Rect {
    min: [f32; 2],
    max: [f32; 2],
}

impl Rect {
    fn corners(self, inset: f32) -> [[f32; 2]; 4] {
        let [left, bottom] = self.min.map(|v| v + inset);
        let [right, top] = self.max.map(|v| v - inset);
        [[left, bottom], [right, bottom], [right, top], [left, top]]
    }

    fn contains(self, point: [f32; 2]) -> bool {
        (0..2).all(|i| point[i] >= self.min[i] && point[i] <= self.max[i])
    }

    fn edge_distance(self, point: [f32; 2]) -> f32 {
        (0..2)
            .map(|i| (point[i] - self.min[i]).min(self.max[i] - point[i]))
            .fold(f32::INFINITY, f32::min)
    }
}

struct Layout {
    frames: [(Rect, bool); 9],
    panels: [Rect; 6],
}

impl Layout {
    fn new(width: f32, height: f32) -> Self {
        let stile = (width * 0.115).clamp(0.085, 0.14);
        let centre = width * 0.5;
        let muntin = (width * 0.085).clamp(0.065, 0.105) * 0.5;
        let rails = [
            [0., height * 0.095],
            [height * 0.3875, height * 0.4725],
            [height * 0.7875, height * 0.8325],
            [height * 0.94, height],
        ];
        let rows = [
            [rails[0][1], rails[1][0]],
            [rails[1][1], rails[2][0]],
            [rails[2][1], rails[3][0]],
        ];
        let rect = |left, right, bottom, top| Rect {
            min: [left, bottom],
            max: [right, top],
        };
        let frames = [
            (rect(0., stile, 0., height), false),
            (rect(width - stile, width, 0., height), false),
            (rect(stile, width - stile, rails[0][0], rails[0][1]), true),
            (rect(stile, width - stile, rails[1][0], rails[1][1]), true),
            (rect(stile, width - stile, rails[2][0], rails[2][1]), true),
            (rect(stile, width - stile, rails[3][0], rails[3][1]), true),
            (
                rect(centre - muntin, centre + muntin, rows[0][0], rows[0][1]),
                false,
            ),
            (
                rect(centre - muntin, centre + muntin, rows[1][0], rows[1][1]),
                false,
            ),
            (
                rect(centre - muntin, centre + muntin, rows[2][0], rows[2][1]),
                false,
            ),
        ];
        let panels = std::array::from_fn(|i| {
            let [left, right] = if i % 2 == 0 {
                [stile, centre - muntin]
            } else {
                [centre + muntin, width - stile]
            };
            rect(left, right, rows[i / 2][0], rows[i / 2][1])
        });
        Self { frames, panels }
    }
}

pub(crate) struct Surface {
    pub recess: f32,
    pub uv: [f32; 2],
    pub horizontal: bool,
}

pub(crate) fn sample(width: f32, height: f32, point: [f32; 2]) -> Surface {
    let layout = Layout::new(width, height);
    let panel = layout.panels.iter().find(|p| p.contains(point));
    let recess = panel.map_or(0., |p| {
        let distance = p.edge_distance(point);
        PROFILE
            .windows(2)
            .find(|pair| distance < pair[1][0])
            .map_or(0.002, |pair| {
                let t = ((distance - pair[0][0]) / (pair[1][0] - pair[0][0])).clamp(0., 1.);
                pair[0][1] + (pair[1][1] - pair[0][1]) * t
            })
    });
    let horizontal = panel.is_none()
        && layout
            .frames
            .iter()
            .any(|(r, horizontal)| *horizontal && r.contains(point));
    Surface {
        recess,
        uv: if horizontal {
            [point[1], point[0]]
        } else {
            point
        },
        horizontal,
    }
}

fn surface_quad(
    builder: &mut Builder,
    mut points: [[f32; 3]; 4],
    sign: f32,
    horizontal: bool,
    map: &impl Fn([f32; 3]) -> [f32; 3],
) {
    if sign > 0. {
        points.reverse();
    }
    let uv = points.map(|[x, _, z]| {
        if horizontal {
            [z * 0.5, x * 0.5]
        } else {
            [x * 0.5, z * 0.5]
        }
    });
    builder.quad(MaterialSlot::Door, points.map(map), uv);
}

fn panel_face(
    builder: &mut Builder,
    rect: Rect,
    half: f32,
    sign: f32,
    profile: &[[f32; 2]],
    map: &impl Fn([f32; 3]) -> [f32; 3],
) {
    for pair in profile.windows(2) {
        let outer = rect
            .corners(pair[0][0])
            .map(|[x, z]| [x, sign * (half - pair[0][1]), z]);
        let inner = rect
            .corners(pair[1][0])
            .map(|[x, z]| [x, sign * (half - pair[1][1]), z]);
        for i in 0..4 {
            let next = (i + 1) % 4;
            surface_quad(
                builder,
                [outer[i], outer[next], inner[next], inner[i]],
                sign,
                false,
                map,
            );
        }
    }
    let [inset, recess] = profile[profile.len() - 1];
    surface_quad(
        builder,
        rect.corners(inset)
            .map(|[x, z]| [x, sign * (half - recess), z]),
        sign,
        false,
        map,
    );
}

fn frame_box(builder: &mut Builder, rect: Rect, half: f32, horizontal: bool) {
    let start = builder.mesh(MaterialSlot::Door).positions.len();
    builder.cube(
        MaterialSlot::Door,
        [rect.min[0], -half, rect.min[1]],
        [rect.max[0], half, rect.max[1]],
    );
    let mesh = builder.mesh(MaterialSlot::Door);
    let grain = if horizontal { 0 } else { 2 };
    let across = 2 - grain;
    for ((p, n), uv) in mesh.positions[start..]
        .iter()
        .zip(&mesh.normals[start..])
        .zip(&mut mesh.uvs[start..])
    {
        *uv = if n[grain].abs() > 0.5 {
            [p[across] * 0.5, p[1] * 0.5]
        } else {
            [
                if n[1].abs() > 0.5 { p[across] } else { p[1] } * 0.5,
                p[grain] * 0.5,
            ]
        };
    }
}

pub(super) fn near(width: f32, height: f32, thickness: f32) -> BuildingMesh {
    let mut builder = Builder::default();
    let layout = Layout::new(width, height);
    let half = thickness * 0.5;
    for (rect, horizontal) in layout.frames {
        frame_box(&mut builder, rect, half, horizontal);
    }
    for rect in layout.panels {
        for sign in [-1., 1.] {
            panel_face(&mut builder, rect, half, sign, &PROFILE, &|p| p);
        }
        let front = rect.corners(0.).map(|[x, z]| [x, -half, z]);
        let back = rect.corners(0.).map(|[x, z]| [x, half, z]);
        for i in 0..4 {
            let next = (i + 1) % 4;
            builder.face(
                MaterialSlot::Door,
                [front[next], front[i], back[i], back[next]],
            );
        }
    }
    builder.finish().parts.remove(0).mesh
}

pub(crate) fn proxy(builder: &mut Builder, recipe: &BuildingRecipe, opening: &OpeningRecipe) {
    let layout = Layout::new(opening.width, opening.height);
    let map = |[x, y, z]: [f32; 3]| {
        opening.facade.point(
            recipe,
            opening.offset - opening.width * 0.5 + x,
            opening.sill + z,
            0.015 + y,
        )
    };
    for (rect, horizontal) in layout.frames {
        surface_quad(
            builder,
            rect.corners(0.).map(|[x, z]| [x, 0., z]),
            -1.,
            horizontal,
            &map,
        );
    }
    for rect in layout.panels {
        panel_face(builder, rect, 0., -1., &COARSE, &map);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joinery_has_six_fielded_panels_and_non_degenerate_material_coordinates() {
        for (width, height) in [(0.944, 2.091), (1.144, 2.191), (2.5, 5.)] {
            let layout = Layout::new(width, height);
            for panel in layout.panels {
                assert!(panel.max[0] - panel.min[0] > 0.084);
                assert!(panel.max[1] - panel.min[1] > 0.084);
                let centre = std::array::from_fn(|i| (panel.min[i] + panel.max[i]) * 0.5);
                assert!((sample(width, height, centre).recess - 0.002).abs() < 0.00001);
                let groove = [panel.min[0] + 0.016, centre[1]];
                assert!(sample(width, height, groove).recess > 0.012);
            }
            let mesh = near(width, height, 0.055);
            assert!(mesh.positions.iter().all(|p| p[0] >= 0.
                && p[0] <= width
                && p[1].abs() <= 0.0275
                && p[2] >= 0.
                && p[2] <= height));
            for tri in mesh.indices.chunks_exact(3) {
                let uv = std::array::from_fn::<_, 3, _>(|i| mesh.uvs[tri[i] as usize]);
                let area = (uv[1][0] - uv[0][0]) * (uv[2][1] - uv[0][1])
                    - (uv[1][1] - uv[0][1]) * (uv[2][0] - uv[0][0]);
                assert!(
                    area.abs() > 0.000_000_01,
                    "material tangents have usable UVs on every face"
                );
            }
        }
    }
}
