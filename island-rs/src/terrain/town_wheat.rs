//! Fit planned wheat rectangles to the final resident terrain and route surfaces.
use super::{Island, Terrain, TriangleIndex};
use crate::{DecorationClearance, ISLAND_WORLD_METRES, Mesh, Vec2, Vec3};

const MAX_FIELDS: usize = 4096;
const MAX_WATERFRONT_STRUCTURES: usize = 129; // Up to 128 bridges and one capital pier.
const MAX_CELLS: usize = 4_000_000;
const MAX_FIELD_HALF_EXTENT_METRES: f32 = 64.;
const MIN_FIELD_SIZE_METRES: Vec2 = Vec2::new(8., 5.);
const CELL_METRES: f32 = 1.;
const MIN_HEIGHT: f32 = 0.35 / ISLAND_WORLD_METRES;
const MIN_UP_NORMAL: f32 = 0.970_295_7; // cos(14°)
const FOOTPATH_MIN_MARKED_WEIGHT: f32 = 0.4;

impl Island {
    pub(crate) fn prepare_coastal_wheat_fields(
        &mut self,
        fields: &[DecorationClearance],
        bridge_clearance: &[DecorationClearance],
    ) -> Result<Vec<DecorationClearance>, String> {
        validate(fields, bridge_clearance)?;
        if fields.is_empty() {
            return Ok(Vec::new());
        }
        let surface = FieldSurface::new(self);
        let mut accepted = Vec::new();
        for field in fields {
            let field_frame = FieldFrame::from(*field);
            let [min, max] = field_frame.bounds();
            let obstacles: Vec<_> = bridge_clearance
                .iter()
                .chain(&accepted)
                .copied()
                .map(FieldFrame::from)
                .chain(surface.caves.iter().copied())
                .filter(|rect| rect.overlaps_bounds(min, max))
                .map(FieldFrame::corners)
                .collect();
            if let Some(field) = trim(*field, |cell| {
                !obstacles.iter().any(|rect| cell.intersects_polygon(rect))
                    && surface.suitable(cell)
            }) {
                accepted.push(field);
            }
        }
        self.clear_decorations(&accepted)?;
        Ok(accepted)
    }
}

fn validate(
    fields: &[DecorationClearance],
    bridge_clearance: &[DecorationClearance],
) -> Result<(), String> {
    if fields.len() > MAX_FIELDS || bridge_clearance.len() > MAX_WATERFRONT_STRUCTURES {
        return Err("wheat candidates or bridge clearances exceed the tile budget".into());
    }
    for rect in fields.iter().chain(bridge_clearance) {
        rect.validate()?;
        if rect
            .centre
            .iter()
            .any(|value| !(0. ..=ISLAND_WORLD_METRES).contains(value))
        {
            return Err(
                "wheat candidates and bridge clearances must be centred on the tile".into(),
            );
        }
    }
    if fields.iter().any(|field| {
        field
            .half_extents
            .iter()
            .any(|&half| half > MAX_FIELD_HALF_EXTENT_METRES)
    }) {
        return Err("wheat field dimensions cannot exceed 128 metres".into());
    }
    let cells = fields.iter().map(|field| {
        let [width, height] = dimensions(*field);
        width * height
    });
    if cells.sum::<usize>() > MAX_CELLS {
        return Err("wheat candidates exceed the total tile area budget".into());
    }
    Ok(())
}

struct FieldSurface<'a> {
    terrain: &'a Terrain,
    water: &'a Mesh,
    water_index: Option<TriangleIndex>,
    caves: Vec<FieldFrame>,
}

impl<'a> FieldSurface<'a> {
    fn new(island: &'a Island) -> Self {
        let water = island.river_mesh();
        let caves = island
            .town_protected_terrain()
            .into_iter()
            .skip(4) // The first four protect the tile edge from road earthworks.
            .map(|area| {
                let [min, max] = [Vec2::from_array(area.min), Vec2::from_array(area.max)];
                FieldFrame {
                    centre: (min + max) * 0.5,
                    axis: Vec2::X,
                    half: (max - min) * 0.5,
                }
            })
            .collect();
        Self {
            terrain: island.terrain(),
            water,
            water_index: (!water.triangles.is_empty()).then(|| TriangleIndex::new(water)),
            caves,
        }
    }

    fn suitable(&self, cell: FieldFrame) -> bool {
        let [min, max] = cell.bounds();
        if min.min_element() < 0.
            || max.max_element() > ISLAND_WORLD_METRES
            || self.terrain.road_corridor_overlaps(min, max)
            || self.terrain.cart_track_corridor_overlaps(min, max)
        {
            return false;
        }
        let [min, max] = [min, max].map(|point| point / ISLAND_WORLD_METRES);
        if self.water_index.as_ref().is_some_and(|index| {
            index.faces_in_bounds(min, max).any(|face| {
                let triangle = face_vertices(self.water, face).map(world_xy);
                cell.intersects_polygon(&triangle)
            })
        }) {
            return false;
        }
        let point = cell.centre / ISLAND_WORLD_METRES;
        self.terrain.sample_accurate_surface(point).is_some()
            && !self
                .terrain
                .triangle_index
                .faces_in_bounds(min, max)
                .any(|face| self.unsuitable_face(cell, face))
    }

    fn unsuitable_face(&self, cell: FieldFrame, face: u32) -> bool {
        let mesh = self.terrain.mesh();
        let vertices = face_vertices(mesh, face);
        let [a, b, c] = vertices;
        let cross = (b - a).cross(c - a);
        let wet = vertices.map(|vertex| MIN_HEIGHT - vertex.z);
        let steep = cross.z != 0. && cross.z.abs() < cross.length() * MIN_UP_NORMAL;
        let footpath = self.terrain.footpaths.as_ref().map(|paint| {
            face_indices(mesh, face).map(|index| {
                f32::from(u8::from(paint.contains_vertex(index))) - FOOTPATH_MIN_MARKED_WEIGHT
            })
        });
        let points = vertices.map(world_xy);
        (steep && cell.intersects_polygon(&points))
            || cell.intersects_marked_triangle(points, wet)
            || footpath.is_some_and(|weights| cell.intersects_marked_triangle(points, weights))
    }
}

fn face_indices(mesh: &Mesh, face: u32) -> [usize; 3] {
    let offset = face as usize * 3;
    [0, 1, 2].map(|index| mesh.triangles[offset + index] as usize)
}

fn face_vertices(mesh: &Mesh, face: u32) -> [Vec3; 3] {
    face_indices(mesh, face).map(|index| mesh.vertices[index])
}

fn world_xy(vertex: Vec3) -> Vec2 {
    vertex.truncate() * ISLAND_WORLD_METRES
}

#[derive(Clone, Copy)]
struct FieldFrame {
    centre: Vec2,
    axis: Vec2,
    half: Vec2,
}

impl From<DecorationClearance> for FieldFrame {
    fn from(field: DecorationClearance) -> Self {
        Self {
            centre: Vec2::from_array(field.centre),
            axis: Vec2::from_angle(field.yaw),
            half: Vec2::from_array(field.half_extents),
        }
    }
}

impl FieldFrame {
    fn world(self, local: Vec2) -> Vec2 {
        self.centre + self.axis * local.x + self.axis.perp() * local.y
    }

    fn bounds(self) -> [Vec2; 2] {
        let extent = self.axis.abs() * self.half.x + self.axis.perp().abs() * self.half.y;
        [self.centre - extent, self.centre + extent]
    }

    fn overlaps_bounds(self, min: Vec2, max: Vec2) -> bool {
        let [a, b] = self.bounds();
        a.cmple(max).all() && b.cmpge(min).all()
    }

    fn corners(self) -> [Vec2; 4] {
        let Vec2 { x, y } = self.half;
        [-self.half, Vec2::new(x, -y), self.half, Vec2::new(-x, y)].map(|point| self.world(point))
    }

    fn intersects_polygon(self, points: &[Vec2]) -> bool {
        let edges = points.iter().zip(points.iter().cycle().skip(1));
        points.len() >= 3
            && [self.axis, self.axis.perp()]
                .into_iter()
                .chain(edges.map(|(a, b)| (b - a).perp()))
                .all(|axis| {
                    let radius = self.half.x * self.axis.dot(axis).abs()
                        + self.half.y * self.axis.perp_dot(axis).abs();
                    let (min, max) =
                        points
                            .iter()
                            .fold((f32::INFINITY, f32::NEG_INFINITY), |range, p| {
                                let t = (p - self.centre).dot(axis);
                                (range.0.min(t), range.1.max(t))
                            });
                    min <= radius && max >= -radius
                })
    }

    fn intersects_marked_triangle(self, points: [Vec2; 3], margins: [f32; 3]) -> bool {
        if margins.iter().all(|&margin| margin <= 0.) {
            return false;
        }
        // Height and footpath coverage are affine inside a face. Clip to the
        // part above the threshold so a distant low or painted vertex cannot
        // disqualify the entire triangle.
        let mut polygon = [Vec2::ZERO; 4];
        let mut count = 0;
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            if margins[a] > 0. {
                polygon[count] = points[a];
                count += 1;
            }
            if (margins[a] > 0.) != (margins[b] > 0.) {
                let t = margins[a] / (margins[a] - margins[b]);
                polygon[count] = points[a].lerp(points[b], t);
                count += 1;
            }
        }
        self.intersects_polygon(&polygon[..count])
    }
}

fn dimensions(field: DecorationClearance) -> [usize; 2] {
    field
        .half_extents
        .map(|half| (half * 2. / CELL_METRES).ceil() as usize)
}

fn trim(
    field: DecorationClearance,
    is_suitable: impl Fn(FieldFrame) -> bool,
) -> Option<DecorationClearance> {
    let frame = FieldFrame::from(field);
    let [width, height] = dimensions(field);
    let cell = 2. * frame.half / Vec2::new(width as f32, height as f32);
    let minimum = (MIN_FIELD_SIZE_METRES / cell - 1e-5).ceil();
    if width < minimum.x as usize || height < minimum.y as usize {
        return None;
    }
    let mut column_heights = vec![0; width];
    let mut starts = Vec::with_capacity(width);
    let mut best = None::<GridRectangle>;
    for y in 0..height {
        for (x, count) in column_heights.iter_mut().enumerate() {
            let offset = (Vec2::new(x as f32, y as f32) + 0.5) * cell - frame.half;
            let cell_frame = FieldFrame {
                centre: frame.world(offset),
                half: cell * 0.5,
                ..frame
            };
            *count = if is_suitable(cell_frame) {
                *count + 1
            } else {
                0
            };
        }
        // Every maximal dry rectangle ends at a row where these column
        // heights can be combined. Each strip enters/leaves the stack once.
        for (x, count) in column_heights.iter().copied().chain([0]).enumerate() {
            let mut left = x;
            while starts.last().is_some_and(|&(_, previous)| previous > count) {
                let (start, rows) = starts.pop().expect("a taller strip is present");
                left = start;
                if x - start < minimum.x as usize || rows < minimum.y as usize {
                    continue;
                }
                let rectangle = GridRectangle {
                    x: start,
                    y: y + 1 - rows,
                    width: x - start,
                    height: rows,
                };
                if best.is_none_or(|best| rectangle.area() > best.area()) {
                    best = Some(rectangle);
                }
            }
            if count > 0 && starts.last().is_none_or(|&(_, previous)| previous < count) {
                starts.push((left, count));
            }
        }
    }
    best.map(|region| {
        if region.width == width && region.height == height {
            return field;
        }
        let half = Vec2::new(region.width as f32, region.height as f32) * cell * 0.5;
        let offset = Vec2::new(region.x as f32, region.y as f32) * cell + half - frame.half;
        DecorationClearance {
            centre: frame.world(offset).to_array(),
            half_extents: half.to_array(),
            yaw: field.yaw,
        }
    })
}

#[derive(Clone, Copy)]
struct GridRectangle {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl GridRectangle {
    fn area(self) -> usize {
        self.width * self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ForestOptions,
        roads::{RoadNetwork, RoadStation, RoadStrip},
    };

    fn field(x: f32, y: f32, length: f32, width: f32) -> DecorationClearance {
        DecorationClearance {
            centre: [x, y],
            half_extents: [length * 0.5, width * 0.5],
            yaw: 0.,
        }
    }

    fn fixture() -> Island {
        Island::shared_forest_fixture(ForestOptions::default())
    }

    fn road() -> RoadNetwork {
        RoadNetwork {
            strips: vec![RoadStrip {
                source: 0,
                start: 0,
                end: 1,
                half_width: 2.,
                stations: [Vec2::new(645., 650.), Vec2::new(675., 650.)]
                    .into_iter()
                    .map(|point| RoadStation { point, height: 39. })
                    .collect(),
            }],
            junctions: Vec::new(),
            shoulder: 2.,
        }
    }

    #[test]
    fn the_largest_dry_subrectangle_follows_the_proposed_row_axis() {
        let mut field = field(650., 650., 27.8, 12.2);
        field.yaw = 0.45;
        let frame = FieldFrame::from(field);
        let result = trim(field, |cell| {
            let delta = cell.centre - frame.centre;
            delta.dot(frame.axis).abs() > 1.5 && delta.dot(frame.axis.perp()) < 3.5
        })
        .unwrap();
        let offset = Vec2::from_array(result.centre) - frame.centre;
        assert!(offset.dot(frame.axis) < -5.);
        assert!(offset.dot(frame.axis.perp()) < -1.);
        assert!(result.half_extents[0] > 5.5 && result.half_extents[1] > 4.);
        assert!((result.yaw - field.yaw).abs() < f32::EPSILON);
        assert_eq!(trim(field, |_| true), Some(field));
    }

    #[test]
    fn rendered_water_road_and_bridge_corridors_limit_decoration_clearance() {
        let mut island = fixture();
        island.apply_town_roads(&road()).unwrap();
        island.river_mesh = Mesh {
            vertices: [
                Vec3::new(655.2, 655., 43.),
                Vec3::new(655.4, 655., 43.),
                Vec3::new(655.2, 670., 43.),
                Vec3::new(655.4, 670., 43.),
            ]
            .map(|point| point / ISLAND_WORLD_METRES)
            .into(),
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        };
        let planted = Vec3::new(646., 660., 40.) / ISLAND_WORLD_METRES;
        let bridge = Vec3::new(666., 660., 40.) / ISLAND_WORLD_METRES;
        let road = Vec3::new(646., 650., 40.) / ISLAND_WORLD_METRES;
        island.decorations.get_mut().unwrap().bushes = vec![planted, bridge, road];
        let proposal = field(658., 655., 38., 24.);
        let bridge_bounds = field(665., 660., 15., 14.);
        let accepted = island
            .prepare_coastal_wheat_fields(&[proposal], &[bridge_bounds])
            .unwrap();
        assert_eq!(accepted.len(), 1);
        let [_, max] = FieldFrame::from(accepted[0]).bounds();
        assert!(max.x < 655.2, "the sub-metre river must split the field");
        assert_eq!(island.decorations.get().unwrap().bushes, [bridge, road]);
        assert_eq!(
            island
                .prepare_coastal_wheat_fields(&[proposal], &[bridge_bounds])
                .unwrap(),
            accepted
        );
    }

    #[test]
    fn wheat_cells_reserve_existing_stair_footpaths() {
        let mut island = fixture();
        island.apply_town_roads(&road()).unwrap();
        let foot = crate::TownStairFoot {
            position: [660., 656.],
            house: [660., 660.],
        };
        let foundation = field(660., 660., 4., 6.);
        let vertices = island.paint_town_footpaths(&[foot], &[foundation]).unwrap();
        assert!(vertices >= 2);
        let terrain = island.terrain();
        let paint = terrain.footpaths.as_ref().unwrap();
        let centre = terrain
            .vertices()
            .iter()
            .enumerate()
            .filter(|&(index, vertex)| {
                paint.contains_vertex(index) && terrain.road_coverage_at(vertex.truncate()) < 0.3
            })
            .map(|(_, vertex)| world_xy(*vertex))
            .find(|centre| !terrain.road_corridor_overlaps(*centre - 0.01, *centre + 0.01))
            .unwrap();
        let cell = FieldFrame {
            centre,
            axis: Vec2::X,
            half: Vec2::splat(0.01),
        };
        assert!(!FieldSurface::new(&island).suitable(cell));
        island.terrain.footpaths = None;
        assert!(FieldSurface::new(&island).suitable(cell));
    }

    #[test]
    fn steep_wet_and_narrow_ground_cannot_form_a_field() {
        let mut island = fixture();
        let mut mesh = island.terrain().mesh().clone();
        for vertex in &mut mesh.vertices {
            vertex.z = (vertex.x - 0.3) * 0.3;
        }
        island.terrain = Terrain::new(mesh);
        assert!(
            island
                .prepare_coastal_wheat_fields(&[field(650., 650., 18., 10.)], &[])
                .unwrap()
                .is_empty()
        );
        assert!(trim(field(650., 650., 20., 4.), |_| true).is_none());
    }
}
