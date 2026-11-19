use super::{CartTrackBlocker, Crossing, HALF_WIDTH, WHEEL_OFFSET};
use crate::{ISLAND_WORLD_METRES, Island, Terrain, Vec2, Vec3};

const GRID: usize = 96;
const GRADE_LIMIT: f32 = 0.60;
const CROSS_GRADE_LIMIT: f32 = 0.46;

/// XY metres, clipped to the fixed native tile. Bins only accelerate geometry queries.
#[derive(Debug)]
pub(super) struct SpatialBins<T> {
    items: Vec<T>,
    cells: Vec<Vec<usize>>,
}

impl<T> SpatialBins<T> {
    pub(super) fn new() -> Self {
        Self {
            items: Vec::new(),
            cells: vec![Vec::new(); GRID * GRID],
        }
    }

    fn cells(min: Vec2, max: Vec2) -> impl Iterator<Item = usize> {
        let bin = |v: f32| ((v / ISLAND_WORLD_METRES * GRID as f32) as usize).min(GRID - 1);
        let (x0, x1) = (bin(min.x), bin(max.x));
        (bin(min.y)..=bin(max.y)).flat_map(move |y| (x0..=x1).map(move |x| y * GRID + x))
    }

    pub(super) fn insert(&mut self, min: Vec2, max: Vec2, item: T) {
        let index = self.items.len();
        self.items.push(item);
        for cell in Self::cells(min, max) {
            self.cells[cell].push(index);
        }
    }

    pub(super) fn nearby(&self, min: Vec2, max: Vec2) -> impl Iterator<Item = &T> {
        Self::cells(min, max).flat_map(|cell| self.cells[cell].iter().map(|&id| &self.items[id]))
    }
}

#[derive(Clone, Copy)]
struct Segment {
    a: Vec2,
    b: Vec2,
    radius: f32,
}

impl Segment {
    fn overlaps(self, a: Vec2, b: Vec2, radius: f32) -> bool {
        super::super::road_grade::segment_distance_squared(a, b, self.a, self.b)
            <= (self.radius + radius).powi(2)
    }
}

impl SpatialBins<Segment> {
    fn add_segment(&mut self, a: Vec2, b: Vec2, radius: f32) {
        self.insert(
            a.min(b) - radius,
            a.max(b) + radius,
            Segment { a, b, radius },
        );
    }

    fn overlaps(&self, a: Vec2, b: Vec2, radius: f32) -> bool {
        self.nearby(a.min(b) - radius, a.max(b) + radius)
            .any(|s| s.overlaps(a, b, radius))
    }
}

#[derive(Clone, Copy)]
struct Footprint {
    centre: Vec2,
    half: Vec2,
    axis: Vec2,
}

impl Footprint {
    fn overlaps(self, a: Vec2, b: Vec2) -> bool {
        let local = |p: Vec2| {
            let d = p - self.centre;
            Vec2::new(self.axis.dot(d), self.axis.perp_dot(d))
        };
        let a = local(a);
        let b = local(b);
        let extent = self.half + HALF_WIDTH;
        // Separating axes of the segment and the expanded oriented rectangle.
        if a.min(b).cmpgt(extent).any() || a.max(b).cmplt(-extent).any() {
            return false;
        }
        let normal = (b - a).perp();
        normal.dot(a).abs() <= normal.abs().dot(extent)
    }
}

struct WaterFace([Vec2; 3]);

impl WaterFace {
    fn overlaps(&self, start: Vec2, end: Vec2, radius: f32) -> bool {
        let [a, b, c] = self.0;
        let min = a.min(b).min(c) - radius;
        let max = a.max(b).max(c) + radius;
        if start.min(end).cmpgt(max).any() || start.max(end).cmplt(min).any() {
            return false;
        }
        let sides = [(a, b), (b, c), (c, a)];
        let crosses = sides.map(|(p, q)| (q - p).perp_dot(start - p));
        if (b - a).perp_dot(c - a).abs() > f32::EPSILON
            && (crosses.iter().all(|s| *s >= 0.) || crosses.iter().all(|s| *s <= 0.))
        {
            return true;
        }
        sides
            .into_iter()
            .any(|(a, b)| Segment { a, b, radius: 0. }.overlaps(start, end, radius))
    }
}

pub(super) struct Surface<'a> {
    terrain: &'a Terrain,
    shore: SpatialBins<Segment>,
    channels: SpatialBins<Segment>,
    water: SpatialBins<WaterFace>,
    buildings: SpatialBins<Footprint>,
}

impl<'a> Surface<'a> {
    pub(super) fn new(island: &'a Island, blockers: &[CartTrackBlocker]) -> Self {
        let mut surface = Self {
            terrain: island.terrain(),
            shore: SpatialBins::new(),
            channels: SpatialBins::new(),
            water: SpatialBins::new(),
            buildings: SpatialBins::new(),
        };
        for blocker in blockers {
            surface.add_blocker(blocker);
        }
        // The first four areas protect tile seams from earthworks; tracks perform no earthworks.
        for area in island.town_protected_terrain().into_iter().skip(4) {
            let min = Vec2::from_array(area.min);
            let max = Vec2::from_array(area.max);
            surface.add_blocker(&CartTrackBlocker {
                centre: ((max + min) * 0.5).to_array(),
                half_extents: ((max - min) * 0.5).to_array(),
                yaw: 0.,
            });
        }
        for river in island.rivers() {
            let options = island.options();
            let widths = river.target_half_widths(
                options.river_source_width_metres,
                options.river_maximum_width_metres,
            );
            for (nodes, widths) in river.nodes.windows(2).zip(widths.windows(2)) {
                let a = nodes[0].position.truncate() * ISLAND_WORLD_METRES;
                let b = nodes[1].position.truncate() * ISLAND_WORLD_METRES;
                surface.channels.add_segment(a, b, widths[0].max(widths[1]));
            }
        }
        // Include the rendered river footprint where a bend or confluence extends past the authored channel.
        let water = island.river_mesh();
        for face in water.triangles.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2]
                .map(|i| water.vertices[face[i] as usize].truncate() * ISLAND_WORLD_METRES);
            surface
                .water
                .insert(a.min(b).min(c), a.max(b).max(c), WaterFace([a, b, c]));
        }
        let mesh = island.terrain().mesh();
        for face in mesh.triangles.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[face[i] as usize]);
            let mut points = [(a, b), (b, c), (c, a)]
                .into_iter()
                .filter(|(p, q)| (p.z > 0.) != (q.z > 0.))
                .map(|(p, q)| {
                    p.truncate().lerp(q.truncate(), p.z / (p.z - q.z)) * ISLAND_WORLD_METRES
                });
            if let (Some(p), Some(q)) = (points.next(), points.next()) {
                surface.shore.add_segment(p, q, 0.);
            }
        }
        surface
    }

    fn add_blocker(&mut self, blocker: &CartTrackBlocker) {
        let centre = Vec2::from_array(blocker.centre);
        let half = Vec2::from_array(blocker.half_extents);
        let axis = Vec2::from_angle(blocker.yaw);
        let bounds = axis.abs() * half.x + axis.perp().abs() * half.y + HALF_WIDTH;
        self.buildings.insert(
            centre - bounds,
            centre + bounds,
            Footprint { centre, half, axis },
        );
    }

    pub(super) fn height(&self, p: Vec2) -> Option<f32> {
        let uv = p / ISLAND_WORLD_METRES;
        if !uv.is_finite() || uv.min_element() < 0. || uv.max_element() > 1. {
            return None;
        }
        let mesh = self.terrain.mesh();
        // Several final triangles may overhang the same XY. Match the vertical
        // accurate ray's uppermost hit while evaluating only scalar heights.
        self.terrain
            .triangle_index
            .candidates(uv)
            .iter()
            .filter_map(|&face| {
                let indices = &mesh.triangles[face as usize * 3..];
                super::super::sampling::precise_height(
                    uv,
                    [0, 1, 2].map(|i| mesh.vertices[indices[i] as usize]),
                )
            })
            .max_by(f32::total_cmp)
            .map(|z| z * ISLAND_WORLD_METRES)
    }

    fn clear_ground(&self, a: Vec2, b: Vec2) -> bool {
        let offset = (b - a).normalize_or_zero().perp().abs() * HALF_WIDTH;
        (a.min(b) - offset).min_element() >= 0.
            && (a.max(b) + offset).max_element() <= ISLAND_WORLD_METRES
            && !self.shore.overlaps(a, b, HALF_WIDTH)
            && !self
                .buildings
                .nearby(a.min(b), a.max(b))
                .any(|area| area.overlaps(a, b))
    }

    fn water_at(&self, a: Vec2, b: Vec2, radius: f32) -> bool {
        self.channels.overlaps(a, b, radius)
            || self
                .water
                .nearby(a.min(b) - radius, a.max(b) + radius)
                .any(|face| face.overlaps(a, b, radius))
    }

    pub(super) fn dry_cost(&self, a: Vec2, b: Vec2) -> Option<f32> {
        self.traverse(a, b, GRADE_LIMIT, CROSS_GRADE_LIMIT)
    }

    fn traverse(&self, a: Vec2, b: Vec2, grade_limit: f32, cross_grade_limit: f32) -> Option<f32> {
        if !self.clear_ground(a, b) || self.water_at(a, b, HALF_WIDTH) {
            return None;
        }
        let length = a.distance(b);
        let direction = (b - a).normalize_or_zero();
        let normal = direction.perp() * WHEEL_OFFSET;
        let steps = (length / 2.5).ceil().max(1.) as usize;
        let spacing = length / steps as f32;
        let mut previous: Option<([f32; 3], f32)> = None;
        let mut cost = length;
        for i in 0..=steps {
            let p = a.lerp(b, i as f32 / steps as f32);
            let heights = [
                self.height(p - normal)?,
                self.height(p)?,
                self.height(p + normal)?,
            ];
            let cross = (heights[2] - heights[0]).abs() / (2. * WHEEL_OFFSET);
            if heights.iter().any(|z| *z <= 0.) || cross > cross_grade_limit {
                return None;
            }
            // A terminal may start on pavement. Charge for the length actually
            // spent in a graded street so continuing into town is expensive.
            let pavement = self.terrain.road_coverage_at(p / ISLAND_WORLD_METRES);
            if let Some((prev, previous_pavement)) = previous {
                cost += spacing * 6. * (pavement + previous_pavement);
                let grade = (Vec3::from_array(heights) - Vec3::from_array(prev))
                    .abs()
                    .max_element()
                    / spacing.max(0.001);
                if grade > grade_limit {
                    return None;
                }
                let previous_cross = (prev[2] - prev[0]) / (2. * WHEEL_OFFSET);
                cost += spacing
                    * (10. * grade * grade
                        + 8. * (cross * cross + previous_cross * previous_cross));
            }
            previous = Some((heights, pavement));
        }
        Some(cost)
    }

    pub(super) fn crossing(&self, a: Vec2, b: Vec2) -> Option<Crossing> {
        let length = a.distance(b);
        if !(5.0..=50.0).contains(&length) || !self.clear_ground(a, b) {
            return None;
        }
        let dir = (b - a) / length;
        // An abutment must carry the wagon on a stable bank in line with the deck.
        self.traverse(a - dir * 3., a, 0.35, 0.30)?;
        self.traverse(b, b + dir * 3., 0.35, 0.30)?;
        let za = self.height(a)?;
        let zb = self.height(b)?;
        if (za - zb).abs() > length * 0.12 {
            return None;
        }
        let river = self.water_at(a, b, HALF_WIDTH);
        // A grazing chord along a bank cannot establish an opposite landing.
        if river
            && !self.channels.nearby(a.min(b), a.max(b)).any(|segment| {
                Segment {
                    radius: 0.,
                    ..*segment
                }
                .overlaps(a, b, 0.)
            })
        {
            return None;
        }
        let steps = length.ceil() as usize;
        let mut depth: f32 = 0.;
        let mut valley_length = 0.;
        for i in 1..steps {
            let progress = i as f32 / steps as f32;
            let ground = self.height(a.lerp(b, progress))?;
            let below = (zb - za).mul_add(progress, za) - ground;
            if ground <= 0. || below < -0.5 {
                return None;
            }
            depth = depth.max(below);
            if below > 1.5 {
                valley_length += length / steps as f32;
            }
        }
        if river {
            Some(Crossing::River)
        } else if depth >= 2.5 && valley_length >= 3. {
            Some(Crossing::Ravine)
        } else {
            None
        }
    }

    pub(super) fn near_river(&self, p: Vec2, radius: f32) -> bool {
        self.channels.overlaps(p, p, radius)
    }
}
