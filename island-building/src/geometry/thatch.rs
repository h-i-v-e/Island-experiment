//! Closed thatch roof runs with horizontal eave cuts and a separate solid ridge.
#![allow(clippy::cast_precision_loss)]
use super::{Builder, RoofSection, chimney};
use crate::{BuildingRecipe, MaterialSlot, RoofKind, collision::BoxCollider};

mod details;
mod prisms;
mod shells;
mod smooth;

/// Projection beyond the main roof at each gable end, in metres.
pub(crate) const RIDGE_PROJECTION: f32 = 0.12;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Detail {
    Near,
    Middle,
    Distant,
}

struct Coat<'a> {
    recipe: &'a BuildingRecipe,
    half: [f32; 2],
    hole: Option<BoxCollider>,
    detail: Detail,
    detail_inset: f32,
    surfaces: Vec<smooth::Surface>,
}
impl Coat<'_> {
    fn base(&self, x: f32) -> f32 {
        chimney::roof_height(self.recipe, x) - self.recipe.roof.effective_thickness()
    }
    fn slope(&self) -> f32 {
        self.recipe.roof.rise
            / if self.recipe.roof.kind == RoofKind::Gable {
                self.recipe.width * 0.5
            } else {
                self.recipe.width
            }
    }
    fn high_x(&self) -> f32 {
        if self.recipe.roof.kind == RoofKind::Gable {
            0.
        } else {
            self.half[0]
        }
    }
    fn run_length(&self) -> f32 {
        self.half[0]
            * if self.recipe.roof.kind == RoofKind::Gable {
                1.
            } else {
                2.
            }
    }
    fn eave_level(&self) -> f32 {
        let high = self.base(self.high_x());
        let low = self.base(-self.half[0]);
        (low + self.recipe.roof.effective_thickness())
            .min(high - (0.03_f32).min((high - low) * 0.5))
    }
    fn run_slope(&self) -> f32 {
        (self.base(self.high_x()) + self.recipe.roof.effective_thickness() - self.eave_level())
            / self.run_length()
    }
    fn body_height(&self, x: f32) -> f32 {
        let distance = if self.recipe.roof.kind == RoofKind::Gable {
            x.abs()
        } else {
            self.high_x() - x
        };
        self.base(self.high_x()) + self.recipe.roof.effective_thickness()
            - distance * self.run_slope()
    }
    fn ridge_width(&self) -> f32 {
        (1.1 / (1. + self.slope().powi(2)).sqrt()).min(self.half[0] * 0.6)
    }
    fn ridge_distance(&self, x: f32) -> f32 {
        if self.recipe.roof.kind == RoofKind::Gable {
            x.abs()
        } else {
            self.high_x() - x
        }
    }
    fn is_ridge(&self, x: f32) -> bool {
        self.ridge_distance(x) <= self.ridge_width()
    }
    fn ridge_floor(&self) -> f32 {
        self.body_height(self.high_x() - self.ridge_width()) - 0.015
    }
    fn ridge_height(&self, x: f32) -> f32 {
        let crest = self.body_height(self.high_x()) + 0.16;
        let shoulder = self.ridge_floor() + 0.135;
        crest + (shoulder - crest) * self.ridge_distance(x) / self.ridge_width()
    }
    fn height(&self, x: f32, y: f32) -> f32 {
        if let Some(sample) = self.sample(x, y) {
            return sample.point[2];
        }
        if self.is_ridge(x) {
            self.ridge_height(x)
        } else {
            self.body_height(x)
        }
    }
    fn normal(&self, x: f32, y: f32) -> [f32; 3] {
        if let Some(sample) = self.sample(x, y) {
            return sample.normal;
        }
        let slope = if self.is_ridge(x) {
            (self.ridge_height(self.high_x())
                - self.ridge_height(self.high_x() - self.ridge_width()))
                / self.ridge_width()
        } else {
            self.run_slope()
        };
        let derivative = if self.recipe.roof.kind == RoofKind::Gable {
            if x.abs() < 0.00001 {
                0.
            } else {
                -x.signum() * slope
            }
        } else {
            slope
        };
        unit([-derivative, 0., 1.])
    }
    fn sample(&self, x: f32, y: f32) -> Option<smooth::Sample> {
        self.surfaces
            .iter()
            .filter_map(|surface| surface.sample(x, y))
            .max_by(|a, b| a.point[2].total_cmp(&b.point[2]))
    }
    fn section(&self, x: f32) -> RoofSection {
        let span = if self.recipe.roof.kind == RoofKind::Gable {
            if x > 0. {
                [0., self.half[0]]
            } else {
                [-self.half[0], 0.]
            }
        } else {
            [-self.half[0], self.half[0]]
        };
        RoofSection {
            x: span,
            height: span.map(|x| self.base(x)),
            depth: self.half[1] + RIDGE_PROJECTION,
            thickness: self.recipe.roof.effective_thickness(),
            flip_u: self.recipe.roof.kind == RoofKind::Gable && x > 0.,
        }
    }
    fn vertex(&self, section: RoofSection, point: [f32; 3], normal: [f32; 3]) -> Vertex {
        let uv = if self.detail == Detail::Distant {
            crate::lod::atlas_uv(
                self.recipe,
                if section.flip_u { 5 } else { 4 },
                (point[0] - section.x[0]) / (section.x[1] - section.x[0]),
                (point[1] + section.depth) / (section.depth * 2.),
            )
        } else {
            section.uv(point[0], point[1])
        };
        Vertex {
            point,
            normal,
            strand: normal,
            uv,
        }
    }
    fn end_vertex(&self, point: [f32; 3], normal: [f32; 3], ridge: bool) -> Vertex {
        let floor = if ridge {
            self.ridge_floor()
        } else {
            self.base(point[0]).min(self.ridge_floor())
        };
        let depth = self.half[1] + RIDGE_PROJECTION;
        let physical = if normal[1].abs() > 0.5 {
            [point[0] + self.half[0], point[2] - floor]
        } else if normal[2] < -0.1 {
            [point[1] + depth, point[0] + self.half[0]]
        } else {
            [point[1] + depth, point[2] - floor]
        };
        let uv = if self.detail == Detail::Distant {
            let ([width, height], inset) = crate::lod::thatch_end_chart(self.recipe);
            crate::lod::atlas_uv(
                self.recipe,
                7,
                physical[0] / width,
                (physical[1] + inset) / height,
            )
        } else {
            physical.map(|v| v * 0.5)
        };
        Vertex {
            point,
            normal,
            strand: normal,
            uv,
        }
    }
    fn emit(&self, builder: &mut Builder, vertices: [Vertex; 4], interior: bool) {
        let slot = if self.detail == Detail::Distant {
            MaterialSlot::Atlas
        } else if interior {
            MaterialSlot::Interior
        } else {
            MaterialSlot::Roof
        };
        self.emit_slot(builder, vertices, slot);
    }
    fn emit_end<const N: usize>(&self, builder: &mut Builder, vertices: [Vertex; N], fur: bool) {
        self.emit_end_vertices(builder, &vertices, fur);
    }
    fn emit_end_vertices(&self, builder: &mut Builder, vertices: &[Vertex], fur: bool) {
        self.emit_vertices(
            builder,
            vertices,
            if self.detail == Detail::Distant {
                MaterialSlot::Atlas
            } else {
                MaterialSlot::ThatchEnd
            },
        );
        if fur && self.detail == Detail::Near {
            shells::emit(builder, self, vertices);
        }
    }
    fn emit_slot<const N: usize>(
        &self,
        builder: &mut Builder,
        vertices: [Vertex; N],
        slot: MaterialSlot,
    ) {
        self.emit_vertices(builder, &vertices, slot);
    }
    fn emit_vertices(&self, builder: &mut Builder, vertices: &[Vertex], slot: MaterialSlot) {
        let Some(hole) = self.hole.filter(|hole| intersects(vertices, *hole)) else {
            polygon(builder, slot, vertices);
            return;
        };
        for (axis, edge, high) in [(0, hole.min[0], false), (0, hole.max[0], true)] {
            polygon(builder, slot, &clip(vertices, axis, edge, high));
        }
        let middle = clip(&clip(vertices, 0, hole.min[0], true), 0, hole.max[0], false);
        for (edge, high) in [(hole.min[1], false), (hole.max[1], true)] {
            polygon(builder, slot, &clip(&middle, 1, edge, high));
        }
    }
}

fn coat(recipe: &BuildingRecipe, detail: Detail) -> Coat<'_> {
    let mut coat = Coat {
        recipe,
        half: [
            recipe.width * 0.5 + recipe.roof.overhang,
            recipe.depth * 0.5 + recipe.roof.overhang,
        ],
        hole: chimney::stack_bounds(recipe),
        detail,
        detail_inset: 0.05,
        surfaces: Vec::new(),
    };
    if detail == Detail::Near {
        coat.surfaces = prisms::smoothed(&coat);
    }
    coat
}
/// Build the surface sampler once for near or middle chimney flashing.
pub(super) fn surface_height(recipe: &BuildingRecipe, near: bool) -> impl Fn(f32, f32) -> f32 + '_ {
    let coat = coat(recipe, if near { Detail::Near } else { Detail::Middle });
    move |x, y| coat.height(x, y)
}
pub(crate) fn generate(builder: &mut Builder, recipe: &BuildingRecipe, detail: Detail) {
    let coat = coat(recipe, detail);
    let stations = if recipe.roof.kind == RoofKind::Gable {
        [-coat.half[0] * 0.5, coat.half[0] * 0.5]
    } else {
        [0., 0.]
    };
    for (index, x) in stations.into_iter().enumerate() {
        if index == 1 && recipe.roof.kind == RoofKind::Shed {
            break;
        }
        let section = coat.section(x);
        prisms::run(builder, &coat, section);
        if detail == Detail::Near {
            fibres(builder, &coat, section);
            details::surface_reeds(builder, &coat, section);
            details::ridge_bindings(builder, &coat, section);
        }
    }
    prisms::ridge(builder, &coat);
    if detail == Detail::Near {
        wall_joint(builder, &coat);
    }
}

/// A concealed wall overlap closes the joint beneath the rounded underside.
fn wall_joint(builder: &mut Builder, coat: &Coat<'_>) {
    use crate::Facade;
    if coat.recipe.walls.style == crate::WallStyle::WattleAndDaub {
        // The timber rake/eaves band supplies this overlap with the correct finish.
        return;
    }
    for facade in Facade::ALL {
        let half = facade.length(coat.recipe) * 0.5;
        for span in [[-half, 0.], [0., half]] {
            let points = [span[0], span[1], span[1], span[0]].map(|along| {
                let mut point = facade.point(coat.recipe, along, 0., 0.);
                point[2] = coat.base(point[0]);
                point
            });
            let mut points = points;
            points[2][2] += 0.10;
            points[3][2] += 0.10;
            let tangent = facade.tangent();
            let normal = [tangent[1], -tangent[0], 0.];
            let vertices = std::array::from_fn::<_, 4, _>(|i| Vertex {
                point: points[i],
                normal,
                strand: normal,
                uv: [
                    span[usize::from(matches!(i, 1 | 2))] * 0.5,
                    points[i][2] * 0.5,
                ],
            });
            coat.emit_slot(builder, vertices, MaterialSlot::Wall);
        }
    }
}

/// Sparse fine, irregular reed ends break the dressed edge silhouette without
/// turning the whole roof into separate ropes or horizontal courses.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn fibres(builder: &mut Builder, coat: &Coat<'_>, section: RoofSection) {
    let edge = if section.flip_u {
        section.x[1]
    } else {
        section.x[0]
    };
    let count = (coat.half[1] * 2. / 0.018).ceil().clamp(1., 1200.) as u16;
    for index in 0..count {
        let random = |salt: u64| {
            let mut value = coat.recipe.seed ^ u64::from(index).wrapping_mul(0x9e37_79b9) ^ salt;
            value ^= value >> 30;
            value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
            value ^= value >> 27;
            (value & 0xffff) as f32 / 65_535.
        };
        let width = 0.002 + random(31) * 0.003;
        let y = (-coat.half[1]
            + (f32::from(index) + random(17) * 0.7) / f32::from(count) * coat.half[1] * 2.)
            .clamp(-coat.half[1] + width, coat.half[1] - width);
        let inward = 0.04 + random(53) * 0.13;
        let direction = -edge.signum();
        let tip = edge + direction * (0.004 + random(71) * 0.025);
        let root = edge + direction * inward;
        let points = [
            [tip, y - width],
            [root, y - width * 0.45],
            [root, y + width * 0.45],
            [tip, y + width],
        ];
        let vertices = points.map(|[x, y]| {
            coat.vertex(
                section,
                [x, y, coat.height(x, y) + 0.003 + random(89) * 0.004],
                coat.normal(x, y),
            )
        });
        // Eave-to-ridge order changes on the right gable half.
        let mut vertices = vertices;
        if direction < 0. {
            vertices.reverse();
        }
        coat.emit(builder, vertices, false);
    }
}

#[derive(Clone, Copy)]
struct Vertex {
    point: [f32; 3],
    normal: [f32; 3],
    // Cut reeds extend along their own axis, independently of lighting normals.
    strand: [f32; 3],
    uv: [f32; 2],
}
impl Vertex {
    fn blend(self, other: Self, t: f32) -> Self {
        Self {
            point: std::array::from_fn(|i| self.point[i] + (other.point[i] - self.point[i]) * t),
            normal: unit(std::array::from_fn(|i| {
                self.normal[i] + (other.normal[i] - self.normal[i]) * t
            })),
            strand: unit(std::array::from_fn(|i| {
                self.strand[i] + (other.strand[i] - self.strand[i]) * t
            })),
            uv: std::array::from_fn(|i| self.uv[i] + (other.uv[i] - self.uv[i]) * t),
        }
    }
}
fn unit(v: [f32; 3]) -> [f32; 3] {
    let length = v.iter().map(|v| v * v).sum::<f32>().sqrt();
    if length > 0.000_001 {
        v.map(|v| v / length)
    } else {
        [0., 0., 1.]
    }
}
fn intersects(vertices: &[Vertex], hole: BoxCollider) -> bool {
    (0..2).all(|axis| {
        vertices.iter().any(|v| v.point[axis] > hole.min[axis])
            && vertices.iter().any(|v| v.point[axis] < hole.max[axis])
    })
}
fn clip(vertices: &[Vertex], axis: usize, edge: f32, high: bool) -> Vec<Vertex> {
    let mut output = Vec::with_capacity(vertices.len() + 1);
    let Some(&last) = vertices.last() else {
        return output;
    };
    let inside = |v: Vertex| {
        if high {
            v.point[axis] >= edge
        } else {
            v.point[axis] <= edge
        }
    };
    let mut previous = last;
    for &current in vertices {
        if inside(previous) != inside(current) {
            output.push(previous.blend(
                current,
                (edge - previous.point[axis]) / (current.point[axis] - previous.point[axis]),
            ));
        }
        if inside(current) {
            output.push(current);
        }
        previous = current;
    }
    output
}
fn polygon(builder: &mut Builder, slot: MaterialSlot, vertices: &[Vertex]) {
    if vertices.len() < 3 {
        return;
    }
    let mesh = builder.mesh(slot);
    let start = u32::try_from(mesh.positions.len()).expect("bounded thatch mesh fits u32");
    for v in vertices {
        mesh.positions.push(v.point);
        mesh.normals.push(v.normal);
        mesh.uvs.push(v.uv);
    }
    for i in 1..vertices.len() - 1 {
        let a = vertices[0].point;
        let b = vertices[i].point;
        let c = vertices[i + 1].point;
        let ab = std::array::from_fn::<_, 3, _>(|j| b[j] - a[j]);
        let ac = std::array::from_fn::<_, 3, _>(|j| c[j] - a[j]);
        let cross = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        if cross.iter().map(|v| v * v).sum::<f32>() > 1e-12 {
            mesh.indices.extend([
                start,
                start + u32::try_from(i).expect("polygon index"),
                start + u32::try_from(i + 1).expect("polygon index"),
            ]);
        }
    }
}
