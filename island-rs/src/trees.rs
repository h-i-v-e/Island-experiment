use std::{
    collections::VecDeque,
    f32::consts::{FRAC_PI_4, TAU},
};

use crate::{
    ISLAND_WORLD_METRES, Mesh, Vec2, Vec3,
    clustered_foliage::{FoliageCrown, generate_cluster_foliage},
    noise,
    rng::Rng,
};

const TREE_SEED_SALT: u64 = 0x7472_6565_5f77_6f6f;
const TREE_SHAPE_SEED_SALT: u64 = 0x7472_6565_5f73_6870;
const WOOD_IRREGULARITY_DOMAIN: u64 = 0x776f_6f64_5f69_7272;
const MINIMUM_UPWARD_DIRECTION: f32 = 0.12;
const CROSS_SECTION_VERTICES: usize = 4;
const TRUNK_BASE_FLARE_SCALE: f32 = 1.32;
const TRUNK_BUTTRESS_VARIATION: f32 = 0.12;
const BRANCH_OPENING_RADIUS_RATIO: f32 = 0.78;
const BRANCH_COLLAR_LENGTH_RATIO: f32 = 0.22;
const LOD0_TRIANGLE_DESCENDANT_MULTIPLIER: usize = 16;
const LOD0_WOOD_IRREGULARITY_METRES: f32 = 0.025;
const LOD0_WOOD_IRREGULARITY_SCALE_METRES: f32 = 0.42;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeMeshes {
    pub lod0_wood: Mesh,
    pub lod0_foliage: Mesh,
    pub lod1_wood: Mesh,
    pub lod1_foliage: Mesh,
    pub lod2_wood: Mesh,
    pub wood_lod1_to_lod0: Vec<u32>,
    pub foliage_lod1_to_lod0: Vec<u32>,
    pub(crate) foliage_supports: Vec<Vec3>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct MeshLods {
    lod0: Mesh,
    lod1: Mesh,
    lod1_to_lod0: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RingPlane {
    barycentre: Vec3,
    normal: Vec3,
}

impl RingPlane {
    fn project(self, position: Vec3) -> Vec3 {
        position - self.normal * (position - self.barycentre).dot(self.normal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TreeOptions {
    maximum_child_branches: u8,
    trunk_sections: u8,
    single_section_between_branches: bool,
    maximum_branch_depth: u8,
    trunk_radius_metres: f32,
    trunk_section_length_metres: f32,
    minimum_trunk_branch_height_metres: f32,
    minimum_radius_metres: f32,
    minimum_section_length_metres: f32,
    branch_probability: [f32; 2],
    bend: f32,
    phototropism: f32,
    maximum_twist_radians: f32,
    tip_radius_scale: f32,
    child_upward_bias: f32,
    child_section_length_ratio: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TreeHabit {
    Upright,
    Rounded,
    Spreading,
}

impl TreeHabit {
    pub(crate) fn from_index(index: u8) -> Self {
        match index % 3 {
            0 => Self::Upright,
            1 => Self::Rounded,
            _ => Self::Spreading,
        }
    }
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            maximum_child_branches: 8,
            trunk_sections: 10,
            single_section_between_branches: true,
            maximum_branch_depth: 4,
            trunk_radius_metres: 0.68,
            trunk_section_length_metres: 1.0,
            minimum_trunk_branch_height_metres: 2.0,
            minimum_radius_metres: 0.025,
            minimum_section_length_metres: 0.18,
            branch_probability: [0.05, 1.0],
            bend: 0.09,
            phototropism: 0.32,
            maximum_twist_radians: FRAC_PI_4,
            tip_radius_scale: 0.18,
            child_upward_bias: 0.36,
            child_section_length_ratio: 0.82,
        }
    }
}

impl TreeOptions {
    fn for_seed(seed: u64) -> Self {
        let mut rng = Rng::new(seed ^ TREE_SHAPE_SEED_SALT);
        let habit = TreeHabit::from_index(
            u8::try_from(rng.next_u64() % 3).expect("tree habit modulus fits u8"),
        );
        Self::for_habit_with_rng(habit, &mut rng)
    }

    fn for_habit(seed: u64, habit: TreeHabit) -> Self {
        let mut rng = Rng::new(seed ^ TREE_SHAPE_SEED_SALT);
        let _ = rng.next_u64();
        Self::for_habit_with_rng(habit, &mut rng)
    }

    fn for_habit_with_rng(habit: TreeHabit, rng: &mut Rng) -> Self {
        let mut options = match habit {
            TreeHabit::Upright => Self {
                maximum_child_branches: 10,
                trunk_sections: 12,
                maximum_branch_depth: 3,
                trunk_radius_metres: 0.48,
                trunk_section_length_metres: 1.05,
                minimum_trunk_branch_height_metres: 3.2,
                branch_probability: [0.03, 0.86],
                bend: 0.055,
                phototropism: 0.55,
                maximum_twist_radians: 0.58,
                tip_radius_scale: 0.12,
                child_upward_bias: 0.50,
                child_section_length_ratio: 0.76,
                ..Self::default()
            },
            TreeHabit::Rounded => Self {
                maximum_child_branches: 9,
                trunk_sections: 10,
                maximum_branch_depth: 3,
                trunk_radius_metres: 0.55,
                trunk_section_length_metres: 0.98,
                minimum_trunk_branch_height_metres: 2.5,
                branch_probability: [0.07, 0.92],
                bend: 0.09,
                phototropism: 0.38,
                maximum_twist_radians: FRAC_PI_4,
                tip_radius_scale: 0.14,
                child_upward_bias: 0.36,
                child_section_length_ratio: 0.82,
                ..Self::default()
            },
            TreeHabit::Spreading => Self {
                maximum_child_branches: 8,
                trunk_sections: 9,
                maximum_branch_depth: 3,
                trunk_radius_metres: 0.62,
                trunk_section_length_metres: 0.94,
                minimum_trunk_branch_height_metres: 2.0,
                branch_probability: [0.12, 0.96],
                bend: 0.13,
                phototropism: 0.24,
                maximum_twist_radians: FRAC_PI_4,
                tip_radius_scale: 0.16,
                child_upward_bias: 0.22,
                child_section_length_ratio: 0.90,
                ..Self::default()
            },
        };
        options.trunk_radius_metres *= rng.range(0.90, 1.10);
        options.trunk_section_length_metres *= rng.range(0.92, 1.08);
        options.bend *= rng.range(0.88, 1.12);
        debug_assert!(options.is_valid());
        options
    }

    fn is_valid(self) -> bool {
        self.maximum_child_branches <= 64
            && self.trunk_sections > 0
            && self.maximum_branch_depth > 0
            && [
                self.trunk_radius_metres,
                self.trunk_section_length_metres,
                self.minimum_trunk_branch_height_metres,
                self.minimum_radius_metres,
                self.minimum_section_length_metres,
                self.branch_probability[0],
                self.branch_probability[1],
                self.bend,
                self.phototropism,
                self.maximum_twist_radians,
                self.tip_radius_scale,
                self.child_upward_bias,
                self.child_section_length_ratio,
            ]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
            && self.branch_probability[0] <= self.branch_probability[1]
            && self.branch_probability[1] <= 1.0
            && self.phototropism <= 1.0
            && self.maximum_twist_radians <= FRAC_PI_4
            && self.tip_radius_scale < 1.0
            && self.child_upward_bias < 1.0
            && self.child_section_length_ratio <= 1.0
    }
}

#[derive(Debug)]
struct GrowingAxis {
    ring: [u32; CROSS_SECTION_VERTICES],
    unmeshed_sections: u8,
    centre: Vec3,
    direction: Vec3,
    x_axis: Vec3,
    radius: f32,
    section_length: f32,
    remaining_sections: u8,
    section_budget: u8,
    sections_grown: u8,
    direct_children: u8,
    previous_branch_face: Option<u8>,
    root_taper_scale: f32,
    taper_scale: f32,
    depth: u8,
}

impl GrowingAxis {
    fn continuation(
        &self,
        ring: [u32; CROSS_SECTION_VERTICES],
        unmeshed_sections: u8,
        step: AxisStep,
        spawned_face: Option<u8>,
    ) -> Self {
        Self {
            ring,
            unmeshed_sections,
            centre: step.centre,
            direction: step.direction,
            x_axis: step.x_axis,
            radius: step.radius,
            section_length: step.section_length,
            remaining_sections: step.remaining_sections,
            section_budget: self.section_budget,
            sections_grown: self.sections_grown.saturating_add(1),
            direct_children: self.direct_children + u8::from(spawned_face.is_some()),
            previous_branch_face: spawned_face.or(self.previous_branch_face),
            root_taper_scale: self.root_taper_scale,
            taper_scale: step.taper_scale,
            depth: self.depth,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct AxisStep {
    centre: Vec3,
    direction: Vec3,
    x_axis: Vec3,
    radius: f32,
    section_length: f32,
    taper_scale: f32,
    remaining_sections: u8,
    continues: bool,
}

#[derive(Clone, Copy)]
struct ChildSource<'a> {
    lower_ring: &'a [u32; CROSS_SECTION_VERTICES],
    upper_ring: &'a [u32; CROSS_SECTION_VERTICES],
    direction: Vec3,
    radius: f32,
    section_length: f32,
    remaining_sections: u8,
    previous_branch_face: Option<u8>,
    lower_taper_scale: f32,
    upper_taper_scale: f32,
    depth: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BranchRecord {
    parent_depth: u8,
    origin: Vec3,
    source_normal: Vec3,
    direction: Vec3,
    parent_radius: f32,
    opening_radius: f32,
    radius: f32,
    parent_section_length: f32,
    section_length: f32,
    collar_length: f32,
    root_taper_scale: f32,
    opening_radius_error: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TaperRecord {
    root: f32,
    previous: f32,
    current: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TaperRing {
    centre: Vec3,
    vertices: [u32; CROSS_SECTION_VERTICES],
    scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TubeSegment {
    lower: [u32; CROSS_SECTION_VERTICES],
    upper: [u32; CROSS_SECTION_VERTICES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TriangleSpan {
    index_start: usize,
    index_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BranchJunction {
    parent_lower: [u32; CROSS_SECTION_VERTICES],
    parent_upper: [u32; CROSS_SECTION_VERTICES],
    branch_ring: [u32; CROSS_SECTION_VERTICES],
    connector_triangles: TriangleSpan,
    parent_bark_axis: Vec2,
}

impl BranchJunction {
    fn owns_connector(self, edge: [u32; 2]) -> bool {
        let is_parent =
            |vertex| self.parent_lower.contains(&vertex) || self.parent_upper.contains(&vertex);
        (self.branch_ring.contains(&edge[0]) && is_parent(edge[1]))
            || (self.branch_ring.contains(&edge[1]) && is_parent(edge[0]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DirectionRecord {
    depth: u8,
    sections_grown: u8,
    previous: Vec3,
    current: Vec3,
    twist_radians: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GrowthRateRecord {
    depth: u8,
    direct_children: u8,
    nominal_length: f32,
    actual_length: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct TreeGenerationStats {
    child_branches: u8,
    collapsed_intermediate_rings: u16,
    branches: Vec<BranchRecord>,
    directions: Vec<DirectionRecord>,
    growth_rates: Vec<GrowthRateRecord>,
    taper: Vec<TaperRecord>,
}

struct TreeGenerator {
    seed: u64,
    options: TreeOptions,
    rng: Rng,
    pending: VecDeque<GrowingAxis>,
    taper_rings: Vec<TaperRing>,
    tube_segments: Vec<TubeSegment>,
    branch_junctions: Vec<BranchJunction>,
    terminal_rings: Vec<[u32; CROSS_SECTION_VERTICES]>,
    root_ring: [u32; CROSS_SECTION_VERTICES],
    trunk_terminal_ring: Option<[u32; CROSS_SECTION_VERTICES]>,
    wood: Mesh,
    stats: TreeGenerationStats,
}

impl TreeGenerator {
    fn new(seed: u64, options: TreeOptions) -> Self {
        debug_assert!(options.is_valid());
        let mut generator = Self {
            seed,
            options,
            rng: Rng::new(seed ^ TREE_SEED_SALT),
            pending: VecDeque::new(),
            taper_rings: Vec::new(),
            tube_segments: Vec::new(),
            branch_junctions: Vec::new(),
            terminal_rings: Vec::new(),
            root_ring: [0; CROSS_SECTION_VERTICES],
            trunk_terminal_ring: None,
            wood: Mesh::default(),
            stats: TreeGenerationStats::default(),
        };
        // Tree prototypes are local-origin assets.  Forest assembly can then
        // translate the root to the exact final terrain vertex without
        // carrying a hidden normalized-island offset through every append.
        let centre = Vec3::ZERO;
        let radius =
            options.trunk_radius_metres * generator.rng.range(0.82, 1.18) / ISLAND_WORLD_METRES;
        let section_length = options.trunk_section_length_metres * generator.rng.range(0.82, 1.18)
            / ISLAND_WORLD_METRES;
        let ring = generator.append_ring(
            centre,
            Vec3::Z,
            Vec3::X,
            Vec3::Y,
            radius,
            TRUNK_BASE_FLARE_SCALE,
        );
        for (side, &vertex) in ring.iter().enumerate() {
            let side = u16::try_from(side).expect("trunk side fits u16");
            let variation =
                noise::value(seed ^ WOOD_IRREGULARITY_DOMAIN, f32::from(side) * 1.37, 0.0);
            let position = generator.wood.vertices[vertex as usize];
            generator.wood.vertices[vertex as usize] =
                centre + (position - centre) * (1.0 + variation * TRUNK_BUTTRESS_VARIATION);
        }
        generator.terminal_rings.push(ring);
        generator.root_ring = ring;
        generator.pending.push_back(GrowingAxis {
            ring,
            unmeshed_sections: 0,
            centre,
            direction: Vec3::Z,
            x_axis: Vec3::X,
            radius,
            section_length,
            remaining_sections: options.trunk_sections,
            section_budget: options.trunk_sections,
            sections_grown: 0,
            direct_children: 0,
            previous_branch_face: None,
            root_taper_scale: 1.0,
            taper_scale: 1.0,
            depth: 0,
        });
        generator
    }

    fn generate(mut self, foliage_seed: u64) -> (TreeMeshes, TreeGenerationStats) {
        while let Some(axis) = self.pending.pop_front() {
            self.extend_axis(&axis);
        }
        self.apply_taper();
        // Terminal rings are the authoritative branch-tip supports. Capture
        // them after tapering but before wood LOD tube projection can move
        // newly tessellated vertices.
        let foliage_supports: Vec<Vec3> = self
            .terminal_rings
            .iter()
            .skip(1)
            .map(|ring| ring_barycentre(&self.wood, ring))
            .collect();
        let wood = build_mesh_lods(
            self.wood,
            &self.terminal_rings,
            &self.tube_segments,
            &self.branch_junctions,
            self.seed,
        );
        let lod2_wood = build_lod2_trunk(
            &wood.lod1,
            self.root_ring,
            self.trunk_terminal_ring
                .expect("tree generation always completes its central trunk"),
        );
        let foliage = generate_cluster_foliage(
            foliage_seed,
            &[FoliageCrown {
                trunk: Vec3::ZERO,
                tips: &foliage_supports,
                scale: 1.0,
            }],
        )
        .unwrap_or_else(|error| panic!("single-tree foliage generation failed: {error}"));
        (
            TreeMeshes {
                lod0_wood: wood.lod0,
                lod0_foliage: foliage.lod0,
                lod1_wood: wood.lod1,
                lod1_foliage: foliage.lod1,
                lod2_wood,
                wood_lod1_to_lod0: wood.lod1_to_lod0,
                foliage_lod1_to_lod0: foliage.lod1_to_lod0,
                foliage_supports,
            },
            self.stats,
        )
    }

    fn extend_axis(&mut self, axis: &GrowingAxis) {
        let tangent_bend = axis.x_axis * self.rng.range(-self.options.bend, self.options.bend);
        let y_axis = axis.direction.cross(axis.x_axis).normalize_or_zero();
        let sideways_bend = y_axis * self.rng.range(-self.options.bend, self.options.bend);
        let random_direction = upward_direction(axis.direction + tangent_bend + sideways_bend);
        let direction = match (axis.depth, axis.sections_grown) {
            (0, _) => random_direction,
            (_, 0) => axis.direction,
            _ => bend_toward_light(axis.direction, random_direction, self.options.phototropism),
        };
        let twist_radians = self.rng.range(
            -self.options.maximum_twist_radians,
            self.options.maximum_twist_radians,
        );
        self.stats.directions.push(DirectionRecord {
            depth: axis.depth,
            sections_grown: axis.sections_grown,
            previous: axis.direction,
            current: direction,
            twist_radians,
        });
        let transported_x_axis = transported_x_axis(axis.x_axis, direction);
        let x_axis = twisted_x_axis(transported_x_axis, direction, twist_radians);
        let next_y_axis = direction.cross(x_axis).normalize_or_zero();
        let actual_length = axis.section_length * branch_growth_rate(axis.direct_children);
        self.stats.growth_rates.push(GrowthRateRecord {
            depth: axis.depth,
            direct_children: axis.direct_children,
            nominal_length: axis.section_length,
            actual_length,
        });
        let centre = axis.centre + direction * actual_length;
        let radius = axis.radius;
        let section_length = axis.section_length;
        let taper_scale = self.next_taper_scale(axis);
        let remaining_sections = axis.remaining_sections.saturating_sub(1);
        let will_spawn_branch = self.should_spawn_child(axis, remaining_sections);

        let step = AxisStep {
            centre,
            direction,
            x_axis,
            radius,
            section_length,
            taper_scale,
            remaining_sections,
            continues: remaining_sections > 0
                && radius * ISLAND_WORLD_METRES >= self.options.minimum_radius_metres
                && section_length * ISLAND_WORLD_METRES
                    >= self.options.minimum_section_length_metres,
        };

        // Keep simulating hidden growth steps so bend, taper, and branch
        // placement remain unchanged. Their intermediate rings are omitted,
        // leaving one four-sided span between branch junctions before LOD 0
        // tessellation and weighted tube projection.
        if self.options.single_section_between_branches && !will_spawn_branch && step.continues {
            self.pending.push_back(axis.continuation(
                axis.ring,
                axis.unmeshed_sections.saturating_add(1),
                step,
                None,
            ));
            return;
        }

        let lower_ring = if will_spawn_branch && axis.unmeshed_sections > 0 {
            self.emit_branch_interval_end(axis)
        } else {
            axis.ring
        };
        let ring = self.append_ring(centre, direction, x_axis, next_y_axis, radius, taper_scale);

        let spawned_face = if will_spawn_branch {
            self.spawn_child(ChildSource {
                lower_ring: &lower_ring,
                upper_ring: &ring,
                direction,
                radius,
                section_length,
                remaining_sections,
                previous_branch_face: axis.previous_branch_face,
                lower_taper_scale: axis.taper_scale,
                upper_taper_scale: taper_scale,
                depth: axis.depth,
            })
        } else {
            None
        };
        if spawned_face.is_none() {
            self.connect_tube_segment(lower_ring, ring);
            if !will_spawn_branch {
                self.stats.collapsed_intermediate_rings = self
                    .stats
                    .collapsed_intermediate_rings
                    .saturating_add(u16::from(axis.unmeshed_sections));
            }
        }
        if step.continues {
            self.pending
                .push_back(axis.continuation(ring, 0, step, spawned_face));
        } else {
            self.terminal_rings.push(ring);
            if axis.depth == 0 {
                debug_assert!(self.trunk_terminal_ring.is_none());
                self.trunk_terminal_ring = Some(ring);
            }
        }
    }

    fn emit_branch_interval_end(&mut self, axis: &GrowingAxis) -> [u32; CROSS_SECTION_VERTICES] {
        let y_axis = axis.direction.cross(axis.x_axis).normalize_or_zero();
        let lower_ring = self.append_ring(
            axis.centre,
            axis.direction,
            axis.x_axis,
            y_axis,
            axis.radius,
            axis.taper_scale,
        );
        self.connect_tube_segment(axis.ring, lower_ring);
        self.stats.collapsed_intermediate_rings = self
            .stats
            .collapsed_intermediate_rings
            .saturating_add(u16::from(axis.unmeshed_sections.saturating_sub(1)));
        lower_ring
    }

    fn should_spawn_child(&mut self, axis: &GrowingAxis, remaining_sections: u8) -> bool {
        let probability = branch_probability(
            axis.sections_grown,
            axis.section_budget,
            self.options.branch_probability,
        );
        let branches_needed = self
            .options
            .maximum_child_branches
            .saturating_sub(self.stats.child_branches);
        let must_reach_cap = axis.depth == 0 && branches_needed > remaining_sections;
        let has_clearance = axis.depth > 0
            || axis.centre.z * ISLAND_WORLD_METRES
                >= self.options.minimum_trunk_branch_height_metres;
        self.stats.child_branches < self.options.maximum_child_branches
            && axis.depth < self.options.maximum_branch_depth
            && has_clearance
            && (must_reach_cap || self.rng.unit() < probability)
    }

    fn spawn_child(&mut self, parent: ChildSource<'_>) -> Option<u8> {
        let face_normals = branch_face_normals(&self.wood, parent);
        let upward_faces = eligible_branch_faces(face_normals, parent.depth);
        let face = branch_face(
            self.rng.next_u64(),
            parent.previous_branch_face,
            upward_faces,
        )?;
        let next = (face + 1) % CROSS_SECTION_VERTICES;
        let [lower_left, lower_right, upper_left, upper_right] =
            branch_face_corners(&self.wood, &parent, face, next);
        let across =
            ((lower_right - lower_left) + (upper_right - upper_left)).normalize_or(Vec3::X);
        let along = ((upper_left - lower_left) + (upper_right - lower_right))
            .normalize_or(parent.direction);
        let source_normal = face_normals[face];
        let direction = branch_direction(source_normal, self.options.child_upward_bias);
        let origin = (lower_left + lower_right + upper_left + upper_right) * 0.25;
        let radius = parent.radius * child_radius_ratio(parent.depth);
        let opening_radius = parent.radius * BRANCH_OPENING_RADIUS_RATIO;
        let section_length = parent.section_length * self.options.child_section_length_ratio;
        let collar_length = section_length * BRANCH_COLLAR_LENGTH_RATIO;
        let root_taper_scale = (parent.lower_taper_scale + parent.upper_taper_scale) * 0.5;
        let x_axis = (-across - along).normalize_or(Vec3::X);
        let y_axis = direction.cross(x_axis).normalize_or_zero();
        let opening_ring = self.append_ring(
            origin,
            direction,
            x_axis,
            y_axis,
            opening_radius,
            root_taper_scale,
        );
        let collar_centre = origin + direction * collar_length;
        let ring = self.append_ring(
            collar_centre,
            direction,
            x_axis,
            y_axis,
            radius,
            root_taper_scale,
        );
        let connector_triangles = connect_rings_with_opening(
            &mut self.wood.triangles,
            parent.lower_ring,
            parent.upper_ring,
            face,
            &opening_ring,
        );
        self.connect_tube_segment(opening_ring, ring);
        self.tube_segments.push(TubeSegment {
            lower: *parent.lower_ring,
            upper: *parent.upper_ring,
        });
        self.branch_junctions.push(BranchJunction {
            parent_lower: *parent.lower_ring,
            parent_upper: *parent.upper_ring,
            branch_ring: opening_ring,
            connector_triangles,
            parent_bark_axis: encode_bark_axis(parent.direction),
        });
        let opening_radius_error = opening_ring.iter().fold(0.0_f32, |error, &vertex| {
            error.max(
                ((self.wood.vertices[vertex as usize] - origin).length() - opening_radius).abs(),
            )
        });
        let section_budget = parent.remaining_sections.clamp(2, 5);
        self.pending.push_back(GrowingAxis {
            ring,
            unmeshed_sections: 0,
            centre: collar_centre,
            direction,
            x_axis,
            radius,
            section_length,
            remaining_sections: section_budget,
            section_budget,
            sections_grown: 0,
            direct_children: 0,
            previous_branch_face: None,
            root_taper_scale,
            taper_scale: root_taper_scale,
            depth: parent.depth + 1,
        });
        self.stats.child_branches += 1;
        self.stats.branches.push(BranchRecord {
            parent_depth: parent.depth,
            origin,
            source_normal,
            direction,
            parent_radius: parent.radius,
            opening_radius,
            radius,
            parent_section_length: parent.section_length,
            section_length,
            collar_length,
            root_taper_scale,
            opening_radius_error,
        });
        Some(u8::try_from(face).expect("branch face fits u8"))
    }

    fn append_ring(
        &mut self,
        centre: Vec3,
        direction: Vec3,
        x_axis: Vec3,
        y_axis: Vec3,
        radius: f32,
        taper_scale: f32,
    ) -> [u32; CROSS_SECTION_VERTICES] {
        let first = u32::try_from(self.wood.vertices.len()).expect("tree mesh fits u32 indices");
        self.wood.vertices.reserve(CROSS_SECTION_VERTICES);
        self.wood.uv.reserve(CROSS_SECTION_VERTICES);
        let bark_axis = encode_bark_axis(direction);
        let ring_size = u16::try_from(CROSS_SECTION_VERTICES).expect("ring size fits u16");
        for side in 0..CROSS_SECTION_VERTICES {
            let side_index = u16::try_from(side).expect("tree ring vertex count fits u16");
            let angle = f32::from(side_index) * TAU / f32::from(ring_size);
            self.wood
                .vertices
                .push(centre + (x_axis * angle.cos() + y_axis * angle.sin()) * radius);
            self.wood.uv.push(bark_axis);
        }
        let ring = std::array::from_fn(|offset| {
            first + u32::try_from(offset).expect("tree ring fits u32 indices")
        });
        self.taper_rings.push(TaperRing {
            centre,
            vertices: ring,
            scale: taper_scale,
        });
        ring
    }

    fn connect_tube_segment(
        &mut self,
        lower: [u32; CROSS_SECTION_VERTICES],
        upper: [u32; CROSS_SECTION_VERTICES],
    ) {
        connect_rings(&mut self.wood.triangles, &lower, &upper);
        self.tube_segments.push(TubeSegment { lower, upper });
    }

    fn apply_taper(&mut self) {
        for ring in &self.taper_rings {
            for &vertex in &ring.vertices {
                let position = self.wood.vertices[vertex as usize];
                self.wood.vertices[vertex as usize] =
                    ring.centre + (position - ring.centre) * ring.scale;
            }
        }
    }

    fn next_taper_scale(&mut self, axis: &GrowingAxis) -> f32 {
        let current = axis_taper_scale(
            axis.root_taper_scale,
            axis.sections_grown.saturating_add(1),
            axis.section_budget,
            self.options.tip_radius_scale,
        );
        self.stats.taper.push(TaperRecord {
            root: axis.root_taper_scale,
            previous: axis.taper_scale,
            current,
        });
        current
    }
}

fn branch_face_corners(
    mesh: &Mesh,
    parent: &ChildSource<'_>,
    face: usize,
    next: usize,
) -> [Vec3; 4] {
    [
        mesh.vertices[parent.lower_ring[face] as usize],
        mesh.vertices[parent.lower_ring[next] as usize],
        mesh.vertices[parent.upper_ring[face] as usize],
        mesh.vertices[parent.upper_ring[next] as usize],
    ]
}

fn branch_face_normals(mesh: &Mesh, parent: ChildSource<'_>) -> [Vec3; CROSS_SECTION_VERTICES] {
    std::array::from_fn(|face| {
        let next = (face + 1) % CROSS_SECTION_VERTICES;
        let lower_left = mesh.vertices[parent.lower_ring[face] as usize];
        let lower_right = mesh.vertices[parent.lower_ring[next] as usize];
        let upper_left = mesh.vertices[parent.upper_ring[face] as usize];
        let upper_right = mesh.vertices[parent.upper_ring[next] as usize];
        let across = (lower_right - lower_left) + (upper_right - upper_left);
        let along = (upper_left - lower_left) + (upper_right - lower_right);
        across.cross(along).normalize_or(parent.direction)
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TubeProjectionSegment {
    source: TubeSegment,
    lower_centre: Vec3,
    upper_centre: Vec3,
    lower_radius: f32,
    upper_radius: f32,
}

impl TubeProjectionSegment {
    fn from_mesh(mesh: &Mesh, source: TubeSegment) -> Self {
        Self {
            source,
            lower_centre: ring_barycentre(mesh, &source.lower),
            upper_centre: ring_barycentre(mesh, &source.upper),
            lower_radius: ring_average_radius(mesh, &source.lower),
            upper_radius: ring_average_radius(mesh, &source.upper),
        }
    }

    fn edge_membership(self, edge: [u32; 2]) -> u8 {
        edge.into_iter()
            .filter(|vertex| {
                self.source.lower.contains(vertex) || self.source.upper.contains(vertex)
            })
            .count()
            .try_into()
            .expect("a tessellated edge has two endpoints")
    }

    fn projected_surface(self, position: Vec3) -> Option<(Vec3, f32)> {
        let centreline = self.upper_centre - self.lower_centre;
        let length_squared = centreline.length_squared();
        if length_squared <= f32::EPSILON {
            return None;
        }
        let raw_progress = (position - self.lower_centre).dot(centreline) / length_squared;
        let progress = raw_progress.clamp(0.0, 1.0);
        let centre = self.lower_centre.lerp(self.upper_centre, progress);
        let axis = centreline / length_squared.sqrt();
        let from_centre = position - centre;
        let radial = from_centre - axis * from_centre.dot(axis);
        let radial_length = radial.length();
        let radius = (self.upper_radius - self.lower_radius)
            .mul_add(progress, self.lower_radius)
            .max(f32::EPSILON);
        if radial_length <= f32::EPSILON {
            return None;
        }
        let target = centre + radial * (radius / radial_length);
        let surface_error = (radial_length - radius).abs() / radius;
        let axial_error = (raw_progress - progress).abs() * centreline.length() / radius;
        let confidence = 1.0 / (1.0 + surface_error * surface_error + axial_error * axial_error);
        Some((target, confidence))
    }
}

fn build_mesh_lods(
    mut lod1: Mesh,
    terminal_rings: &[[u32; CROSS_SECTION_VERTICES]],
    tube_segments: &[TubeSegment],
    branch_junctions: &[BranchJunction],
    seed: u64,
) -> MeshLods {
    let mut lod1_to_lod0 = (0..lod1.vertices.len())
        .map(|vertex| u32::try_from(vertex).expect("tree mesh fits u32 indices"))
        .collect::<Vec<_>>();
    let terminal_planes = terminal_rings
        .iter()
        .map(|ring| ring_plane(&lod1, ring))
        .collect::<Vec<_>>();
    let tube_projections = tube_segments
        .iter()
        .map(|&segment| TubeProjectionSegment::from_mesh(&lod1, segment))
        .collect::<Vec<_>>();
    let tessellated = lod1.tessellated_attributed();
    let mut lod0 = tessellated.mesh;
    round_new_wood_vertices(
        &mut lod0,
        &tessellated.new_vertices,
        &tube_projections,
        branch_junctions,
    );
    pin_terminal_rings(
        &mut lod0,
        terminal_rings,
        &terminal_planes,
        &tessellated.new_vertices,
    );
    for (lod1_vertex, &lod0_vertex) in lod1_to_lod0.iter().enumerate() {
        lod1.vertices[lod1_vertex] = lod0.vertices[lod0_vertex as usize];
    }
    lod0 = lod0.tessellated();
    let bark_axes = lod0.uv.clone();
    lod0.smooth();
    lod0.uv = bark_axes;
    displace_lod0_wood(&mut lod0, seed);
    lod0.calculate_normals();
    lod1.calculate_normals();
    split_bark_connector_seams(&mut lod0, &mut lod1, &mut lod1_to_lod0, branch_junctions);
    MeshLods {
        lod0,
        lod1,
        lod1_to_lod0,
    }
}

fn build_lod2_trunk(
    lod1: &Mesh,
    root_ring: [u32; CROSS_SECTION_VERTICES],
    terminal_ring: [u32; CROSS_SECTION_VERTICES],
) -> Mesh {
    let trunk_axis =
        encode_bark_axis(ring_barycentre(lod1, &terminal_ring) - ring_barycentre(lod1, &root_ring));
    let mut mesh = Mesh {
        vertices: root_ring
            .iter()
            .chain(&terminal_ring)
            .map(|&vertex| lod1.vertices[vertex as usize])
            .collect(),
        uv: vec![trunk_axis; CROSS_SECTION_VERTICES * 2],
        ..Mesh::default()
    };
    let lower =
        std::array::from_fn(|side| u32::try_from(side).expect("LOD2 trunk ring fits u32 indices"));
    let upper = std::array::from_fn(|side| {
        u32::try_from(CROSS_SECTION_VERTICES + side).expect("LOD2 trunk ring fits u32 indices")
    });
    connect_rings(&mut mesh.triangles, &lower, &upper);
    mesh.triangles.extend([0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7]);
    mesh.calculate_normals();
    mesh
}

fn split_bark_connector_seams(
    lod0: &mut Mesh,
    lod1: &mut Mesh,
    lod1_to_lod0: &mut Vec<u32>,
    branch_junctions: &[BranchJunction],
) {
    for junction in branch_junctions {
        let lod1_duplicates = split_bark_connector_patch(lod1, *junction, 1);
        let lod0_duplicates =
            split_bark_connector_patch(lod0, *junction, LOD0_TRIANGLE_DESCENDANT_MULTIPLIER);

        for (lod1_source, lod1_duplicate) in lod1_duplicates {
            debug_assert_eq!(lod1_duplicate as usize, lod1_to_lod0.len());
            let lod0_source = lod1_to_lod0[lod1_source as usize];
            let lod0_duplicate = lod0_duplicates
                .iter()
                .find_map(|&(source, duplicate)| (source == lod0_source).then_some(duplicate))
                .expect("connector source vertex has a matching LOD0 duplicate");
            lod1_to_lod0.push(lod0_duplicate);
        }
    }
}

fn split_bark_connector_patch(
    mesh: &mut Mesh,
    junction: BranchJunction,
    triangle_descendant_multiplier: usize,
) -> Vec<(u32, u32)> {
    debug_assert_eq!(mesh.vertices.len(), mesh.normals.len());
    debug_assert_eq!(mesh.vertices.len(), mesh.uv.len());
    let index_start = junction.connector_triangles.index_start * triangle_descendant_multiplier;
    let index_count = junction.connector_triangles.index_count * triangle_descendant_multiplier;
    let index_end = index_start + index_count;
    debug_assert!(index_end <= mesh.triangles.len());

    let mut duplicates = Vec::<(u32, u32)>::new();
    for triangle_slot in index_start..index_end {
        let source = mesh.triangles[triangle_slot];
        let duplicate = duplicates
            .iter()
            .find_map(|&(candidate, duplicate)| (candidate == source).then_some(duplicate))
            .unwrap_or_else(|| {
                let source_index = source as usize;
                let duplicate = u32::try_from(mesh.vertices.len())
                    .expect("tree connector vertex count fits u32");
                mesh.vertices.push(mesh.vertices[source_index]);
                mesh.normals.push(mesh.normals[source_index]);
                mesh.uv.push(junction.parent_bark_axis);
                duplicates.push((source, duplicate));
                duplicate
            });
        mesh.triangles[triangle_slot] = duplicate;
    }
    duplicates
}

fn displace_lod0_wood(mesh: &mut Mesh, seed: u64) {
    mesh.calculate_normals();
    let perimeter = mesh.perimeter_mask();
    let frequency = ISLAND_WORLD_METRES / LOD0_WOOD_IRREGULARITY_SCALE_METRES;
    let amplitude = LOD0_WOOD_IRREGULARITY_METRES / ISLAND_WORLD_METRES;
    for ((position, normal), &is_perimeter) in
        mesh.vertices.iter_mut().zip(&mesh.normals).zip(&perimeter)
    {
        if is_perimeter {
            continue;
        }
        let point = *position * frequency;
        let signal = (noise::fractal(seed ^ WOOD_IRREGULARITY_DOMAIN, point.x, point.y, 3)
            + noise::fractal(
                seed ^ WOOD_IRREGULARITY_DOMAIN.rotate_left(19),
                point.y,
                point.z,
                3,
            )
            + noise::fractal(
                seed ^ WOOD_IRREGULARITY_DOMAIN.rotate_left(43),
                point.z,
                point.x,
                3,
            ))
            / 3.0;
        *position += *normal * amplitude * signal;
    }
}

fn ring_barycentre(mesh: &Mesh, ring: &[u32; CROSS_SECTION_VERTICES]) -> Vec3 {
    let total = ring.iter().fold(Vec3::ZERO, |total, &vertex| {
        total + mesh.vertices[vertex as usize]
    });
    let vertex_count = u16::try_from(CROSS_SECTION_VERTICES).expect("terminal ring size fits u16");
    total / f32::from(vertex_count)
}

fn ring_average_radius(mesh: &Mesh, ring: &[u32; CROSS_SECTION_VERTICES]) -> f32 {
    let centre = ring_barycentre(mesh, ring);
    let total = ring.iter().fold(0.0, |total, &vertex| {
        total + mesh.vertices[vertex as usize].distance(centre)
    });
    let vertex_count = u16::try_from(CROSS_SECTION_VERTICES).expect("tree ring size fits u16");
    total / f32::from(vertex_count)
}

fn ring_plane(mesh: &Mesh, ring: &[u32; CROSS_SECTION_VERTICES]) -> RingPlane {
    let first = mesh.vertices[ring[0] as usize];
    let second = mesh.vertices[ring[1] as usize];
    let third = mesh.vertices[ring[2] as usize];
    RingPlane {
        barycentre: ring_barycentre(mesh, ring),
        normal: (second - first).cross(third - first).normalize_or(Vec3::Z),
    }
}

fn pin_terminal_rings(
    mesh: &mut Mesh,
    rings: &[[u32; CROSS_SECTION_VERTICES]],
    planes: &[RingPlane],
    tessellated_vertices: &[crate::mesh::NewVertexStencil],
) {
    for (ring, &plane) in rings.iter().zip(planes) {
        for &vertex in ring {
            mesh.vertices[vertex as usize] = plane.project(mesh.vertices[vertex as usize]);
        }
        for side in 0..CROSS_SECTION_VERTICES {
            let edge = [ring[side], ring[(side + 1) % CROSS_SECTION_VERTICES]];
            let midpoint = tessellated_edge_vertex(edge, tessellated_vertices);
            mesh.vertices[midpoint as usize] = plane.project(mesh.vertices[midpoint as usize]);
        }
    }
}

fn tessellated_edge_vertex(
    edge: [u32; 2],
    tessellated_vertices: &[crate::mesh::NewVertexStencil],
) -> u32 {
    let edge = [edge[0].min(edge[1]), edge[0].max(edge[1])];
    tessellated_vertices
        .iter()
        .find(|stencil| stencil.surrounding[..2] == edge)
        .expect("constrained tessellated edge has a midpoint")
        .vertex
}

fn round_new_wood_vertices(
    mesh: &mut Mesh,
    new_vertices: &[crate::mesh::NewVertexStencil],
    tubes: &[TubeProjectionSegment],
    branch_junctions: &[BranchJunction],
) {
    for stencil in new_vertices {
        let edge = [stencil.surrounding[0], stencil.surrounding[1]];
        let position = mesh.vertices[stencil.vertex as usize];
        if let Some(target) = rounded_tube_target(position, edge, tubes, branch_junctions) {
            mesh.vertices[stencil.vertex as usize] = target;
        }
    }
}

fn rounded_tube_target(
    position: Vec3,
    edge: [u32; 2],
    tubes: &[TubeProjectionSegment],
    branch_junctions: &[BranchJunction],
) -> Option<Vec3> {
    if let Some(branch_ring) = branch_junctions
        .iter()
        .find(|junction| junction.owns_connector(edge))
        .map(|junction| junction.branch_ring)
        && let Some(branch_tube) = tubes.iter().find(|tube| tube.source.lower == branch_ring)
    {
        return branch_tube
            .projected_surface(position)
            .map(|(target, _)| target);
    }
    // Ordinary tessellation edges have both endpoints in one tube segment,
    // which must remain authoritative over neighbouring spans that share only
    // a ring vertex. Shared ring edges between consecutive spans retain the
    // confidence-weighted transition below.
    let strongest_membership = tubes
        .iter()
        .map(|tube| tube.edge_membership(edge))
        .max()
        .unwrap_or_default();
    if strongest_membership == 0 {
        return None;
    }
    let (weighted_target, total_weight) = tubes
        .iter()
        .filter(|tube| tube.edge_membership(edge) == strongest_membership)
        .filter_map(|tube| tube.projected_surface(position))
        .fold(
            (Vec3::ZERO, 0.0_f32),
            |(weighted_target, total_weight), (target, weight)| {
                (weighted_target + target * weight, total_weight + weight)
            },
        );
    (total_weight > f32::EPSILON).then_some(weighted_target / total_weight)
}

fn connect_rings(
    triangles: &mut Vec<u32>,
    lower: &[u32; CROSS_SECTION_VERTICES],
    upper: &[u32; CROSS_SECTION_VERTICES],
) {
    triangles.reserve(CROSS_SECTION_VERTICES * 6);
    for side in 0..CROSS_SECTION_VERTICES {
        let next = (side + 1) % CROSS_SECTION_VERTICES;
        triangles.extend([
            lower[side],
            lower[next],
            upper[side],
            lower[next],
            upper[next],
            upper[side],
        ]);
    }
}

fn connect_rings_with_opening(
    triangles: &mut Vec<u32>,
    lower: &[u32; CROSS_SECTION_VERTICES],
    upper: &[u32; CROSS_SECTION_VERTICES],
    opening_face: usize,
    opening: &[u32; CROSS_SECTION_VERTICES],
) -> TriangleSpan {
    triangles.reserve((CROSS_SECTION_VERTICES - 1) * 6 + CROSS_SECTION_VERTICES * 6);
    for side in 0..CROSS_SECTION_VERTICES {
        if side == opening_face {
            continue;
        }
        let next = (side + 1) % CROSS_SECTION_VERTICES;
        append_quad(
            triangles,
            lower[side],
            lower[next],
            upper[next],
            upper[side],
        );
    }

    let next = (opening_face + 1) % CROSS_SECTION_VERTICES;
    let [lower_left, lower_right, upper_right, upper_left] = [
        lower[opening_face],
        lower[next],
        upper[next],
        upper[opening_face],
    ];
    let [
        inner_lower_left,
        inner_lower_right,
        inner_upper_right,
        inner_upper_left,
    ] = *opening;
    let connector_start = triangles.len();
    append_quad(
        triangles,
        lower_left,
        lower_right,
        inner_lower_right,
        inner_lower_left,
    );
    append_quad(
        triangles,
        lower_right,
        upper_right,
        inner_upper_right,
        inner_lower_right,
    );
    append_quad(
        triangles,
        upper_right,
        upper_left,
        inner_upper_left,
        inner_upper_right,
    );
    append_quad(
        triangles,
        upper_left,
        lower_left,
        inner_lower_left,
        inner_upper_left,
    );
    TriangleSpan {
        index_start: connector_start,
        index_count: triangles.len() - connector_start,
    }
}

fn append_quad(triangles: &mut Vec<u32>, a: u32, b: u32, c: u32, d: u32) {
    triangles.extend([a, b, d, b, c, d]);
}

fn upward_direction(mut direction: Vec3) -> Vec3 {
    direction.z = direction.z.max(MINIMUM_UPWARD_DIRECTION);
    direction.normalize_or(Vec3::Z)
}

fn bend_toward_light(previous: Vec3, random_direction: Vec3, strength: f32) -> Vec3 {
    let previous = upward_direction(previous);
    let random_direction = upward_direction(random_direction);
    let minimum_z = (1.0 - previous.z).mul_add(strength, previous.z);
    let z = random_direction.z.max(minimum_z).min(1.0);
    let horizontal = Vec3::new(random_direction.x, random_direction.y, 0.0).normalize_or(Vec3::X);
    horizontal * (1.0 - z * z).max(0.0).sqrt() + Vec3::Z * z
}

fn branch_direction(source_normal: Vec3, upward_bias: f32) -> Vec3 {
    (source_normal * (1.0 - upward_bias) + Vec3::Z * upward_bias).normalize_or(Vec3::Z)
}

fn axis_taper_scale(
    root_scale: f32,
    sections_grown: u8,
    section_budget: u8,
    tip_radius_scale: f32,
) -> f32 {
    let progress = f32::from(sections_grown.min(section_budget)) / f32::from(section_budget.max(1));
    let tip_scale = root_scale * tip_radius_scale;
    let eased_progress = progress.powf(1.8);
    (tip_scale - root_scale).mul_add(eased_progress, root_scale)
}

fn child_radius_ratio(parent_depth: u8) -> f32 {
    (0.56 - f32::from(parent_depth) * 0.05).max(0.44)
}

pub(crate) fn encode_bark_axis(axis: Vec3) -> Vec2 {
    let axis = axis.normalize_or(Vec3::Z);
    let projected = axis / (axis.x.abs() + axis.y.abs() + axis.z.abs()).max(f32::EPSILON);
    let folded = if projected.z < 0.0 {
        Vec2::new(
            (1.0 - projected.y.abs()) * projected.x.signum(),
            (1.0 - projected.x.abs()) * projected.y.signum(),
        )
    } else {
        projected.truncate()
    };
    folded * 0.5 + Vec2::splat(0.5)
}

pub(crate) fn decode_bark_axis(encoded: Vec2) -> Vec3 {
    let unfolded = encoded * 2.0 - Vec2::ONE;
    let mut axis = Vec3::new(
        unfolded.x,
        unfolded.y,
        1.0 - unfolded.x.abs() - unfolded.y.abs(),
    );
    if axis.z < 0.0 {
        let x = (1.0 - axis.y.abs()) * axis.x.signum();
        let y = (1.0 - axis.x.abs()) * axis.y.signum();
        axis.x = x;
        axis.y = y;
    }
    axis.normalize_or(Vec3::Z)
}

fn branch_probability(sections_grown: u8, section_budget: u8, range: [f32; 2]) -> f32 {
    let last_section = section_budget.saturating_sub(1);
    let progress = if last_section == 0 {
        1.0
    } else {
        f32::from(sections_grown.min(last_section)) / f32::from(last_section)
    };
    (range[1] - range[0]).mul_add(progress, range[0])
}

fn branch_growth_rate(direct_children: u8) -> f32 {
    if direct_children == 0 {
        return 1.0;
    }
    let direct_children = f32::from(direct_children);
    (direct_children + 1.0) / (direct_children + 2.0)
}

fn eligible_branch_faces(
    face_normals: [Vec3; CROSS_SECTION_VERTICES],
    parent_depth: u8,
) -> [bool; CROSS_SECTION_VERTICES] {
    let mut eligible = face_normals.map(|normal| normal.z >= 0.0);
    if parent_depth > 0 {
        let top_face = (0..CROSS_SECTION_VERTICES)
            .max_by(|&left, &right| face_normals[left].z.total_cmp(&face_normals[right].z))
            .expect("branch cross-section has faces");
        eligible[top_face] = false;
    }
    eligible
}

fn branch_face(
    random: u64,
    previous: Option<u8>,
    upward_faces: [bool; CROSS_SECTION_VERTICES],
) -> Option<usize> {
    let is_eligible = |face: usize| {
        upward_faces[face] && previous.is_none_or(|previous| usize::from(previous) != face)
    };
    let eligible_count = (0..CROSS_SECTION_VERTICES)
        .filter(|&face| is_eligible(face))
        .count();
    let eligible_count = u64::try_from(eligible_count).expect("face count fits u64");
    if eligible_count == 0 {
        return None;
    }
    let selected = usize::try_from(random % eligible_count).unwrap_or_default();
    (0..CROSS_SECTION_VERTICES)
        .filter(|&face| is_eligible(face))
        .nth(selected)
}

fn transported_x_axis(previous: Vec3, direction: Vec3) -> Vec3 {
    let projected = previous - direction * previous.dot(direction);
    if projected.length_squared() > f32::EPSILON {
        projected.normalize()
    } else {
        let reference = if direction.z.abs() < 0.9 {
            Vec3::Z
        } else {
            Vec3::X
        };
        reference.cross(direction).normalize_or(Vec3::X)
    }
}

fn twisted_x_axis(x_axis: Vec3, direction: Vec3, radians: f32) -> Vec3 {
    (x_axis * radians.cos() + direction.cross(x_axis) * radians.sin()).normalize_or(x_axis)
}

#[must_use]
pub fn generate_tree(seed: u64) -> TreeMeshes {
    TreeGenerator::new(seed, TreeOptions::for_seed(seed))
        .generate(seed)
        .0
}

pub(crate) fn generate_tree_with_habit(seed: u64, habit: TreeHabit) -> TreeMeshes {
    TreeGenerator::new(seed, TreeOptions::for_habit(seed, habit))
        .generate(seed)
        .0
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    fn generated(seed: u64) -> (TreeMeshes, TreeGenerationStats) {
        TreeGenerator::new(seed, TreeOptions::for_seed(seed)).generate(seed)
    }

    #[test]
    fn tree_is_deterministic_and_has_terminal_foliage_supports() {
        let first = generated(42);
        let second = generated(42);

        assert_eq!(first, second);
        let options = TreeOptions::for_seed(42);
        assert_eq!(first.1.child_branches, options.maximum_child_branches);
        assert_eq!(
            first.1.branches.len(),
            usize::from(options.maximum_child_branches)
        );
        assert_eq!(first.0.foliage_supports.len(), first.1.branches.len() + 1);
        assert!(
            first
                .0
                .foliage_supports
                .iter()
                .all(|support| support.is_finite())
        );
        assert!(!first.0.lod1_wood.vertices.is_empty());
        assert!(first.0.lod0_wood.vertices.len() > first.0.lod1_wood.vertices.len());
        assert!(first.0.lod0_foliage.vertices.len() > first.0.lod1_foliage.vertices.len());
        for seed in 0..256 {
            let (_, stats) = generated(seed);
            assert_eq!(
                stats.child_branches,
                TreeOptions::for_seed(seed).maximum_child_branches,
                "seed {seed} did not reach the branch cap"
            );
        }
    }

    #[test]
    fn lod2_wood_is_one_closed_four_sided_trunk_span() {
        for seed in 0..64 {
            let tree = generate_tree(seed);
            let mesh = &tree.lod2_wood;
            assert_eq!(mesh.vertices.len(), 8, "seed {seed}");
            assert_eq!(mesh.normals.len(), 8, "seed {seed}");
            assert_eq!(mesh.uv.len(), 8, "seed {seed}");
            assert_eq!(mesh.triangles.len(), 36, "seed {seed}");
            assert!(mesh.vertices.iter().all(|vertex| vertex.is_finite()));
            assert!(mesh.normals.iter().all(|normal| normal.is_finite()));
            assert!(mesh.uv.iter().all(|axis| axis.is_finite()));

            let bottom = mesh.vertices[..4]
                .iter()
                .copied()
                .fold(Vec3::ZERO, |sum, vertex| sum + vertex)
                / 4.0;
            let top = mesh.vertices[4..]
                .iter()
                .copied()
                .fold(Vec3::ZERO, |sum, vertex| sum + vertex)
                / 4.0;
            assert!(top.z > bottom.z, "seed {seed}");

            let mut edge_uses = HashMap::<(u32, u32), usize>::new();
            for triangle in mesh.triangles.chunks_exact(3) {
                for edge in [
                    (triangle[0], triangle[1]),
                    (triangle[1], triangle[2]),
                    (triangle[2], triangle[0]),
                ] {
                    *edge_uses
                        .entry((edge.0.min(edge.1), edge.0.max(edge.1)))
                        .or_default() += 1;
                }
            }
            assert!(edge_uses.values().all(|&uses| uses == 2), "seed {seed}");
        }
    }

    #[test]
    fn prototype_seeds_cover_upright_rounded_and_spreading_habits() {
        let options = (0..128).map(TreeOptions::for_seed).collect::<Vec<_>>();
        let section_counts = options
            .iter()
            .map(|options| options.trunk_sections)
            .collect::<HashSet<_>>();

        assert_eq!(section_counts, HashSet::from([9, 10, 12]));
        assert!(options.iter().all(|options| options.is_valid()));
        assert!(
            options
                .iter()
                .any(|options| options.child_upward_bias >= 0.5)
        );
        assert!(
            options
                .iter()
                .any(|options| options.child_upward_bias <= 0.22)
        );
        assert!(
            options
                .iter()
                .all(|options| options.child_section_length_ratio < 1.0)
        );
    }

    #[test]
    fn single_tree_has_one_foliage_support_per_terminal_axis() {
        let (tree, stats) = generated(2018);

        assert_eq!(tree.foliage_supports.len(), stats.branches.len() + 1);
        assert!(
            tree.foliage_supports
                .windows(2)
                .all(|supports| { (supports[0] - supports[1]).length_squared() > f32::EPSILON })
        );
    }

    #[test]
    fn single_tree_foliage_is_not_cube_per_tip_topology() {
        let tree = generate_tree(2018);
        let support_count = tree.foliage_supports.len();

        assert!(support_count > 1);
        assert_ne!(tree.lod1_foliage.vertices.len(), support_count * 8);
        assert_ne!(tree.lod1_foliage.triangles.len(), support_count * 36);
    }

    #[test]
    fn branch_chance_increases_from_lower_to_upper_sections() {
        let range = TreeOptions::default().branch_probability;
        let probabilities = (0..5)
            .map(|section| branch_probability(section, 5, range))
            .collect::<Vec<_>>();

        assert_eq!(probabilities.first().copied(), Some(0.05));
        assert_eq!(probabilities.last().copied(), Some(1.0));
        assert!(probabilities.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn main_trunk_keeps_two_metres_clear_of_branches() {
        for seed in 0..256 {
            let options = TreeOptions::for_seed(seed);
            let (_, stats) = generated(seed);
            assert!(
                stats.branches.iter().all(|branch| {
                    branch.parent_depth > 0
                        || branch.origin.z * ISLAND_WORLD_METRES
                            >= options.minimum_trunk_branch_height_metres
                }),
                "seed {seed} has a main-trunk branch below the clearance height"
            );
        }
    }

    #[test]
    fn every_interval_between_branch_junctions_omits_intermediate_rings() {
        let compact_options = TreeOptions::default();
        let mut detailed_options = compact_options;
        detailed_options.single_section_between_branches = false;

        for seed in 0..64 {
            let (compact, compact_stats) = TreeGenerator::new(seed, compact_options).generate(seed);
            let (detailed, detailed_stats) =
                TreeGenerator::new(seed, detailed_options).generate(seed);
            let omitted_rings = usize::from(compact_stats.collapsed_intermediate_rings);
            let omitted_lod1_triangles = omitted_rings * CROSS_SECTION_VERTICES * 2;

            assert!(
                omitted_rings > 0,
                "seed {seed} did not simplify its branch intervals"
            );
            assert_eq!(compact_stats.branches, detailed_stats.branches);
            assert_eq!(compact_stats.directions, detailed_stats.directions);
            assert_eq!(compact_stats.growth_rates, detailed_stats.growth_rates);
            assert_eq!(compact_stats.taper, detailed_stats.taper);
            assert_eq!(compact.foliage_supports, detailed.foliage_supports);
            assert_eq!(
                detailed.lod1_wood.triangles.len() / 3 - compact.lod1_wood.triangles.len() / 3,
                omitted_lod1_triangles,
                "seed {seed} did not remove exactly one ring of faces per omitted ring"
            );
            assert_eq!(
                detailed.lod0_wood.triangles.len() / 3 - compact.lod0_wood.triangles.len() / 3,
                omitted_lod1_triangles * 16,
                "seed {seed} did not preserve the expected tessellated reduction"
            );
        }
    }

    #[test]
    fn consecutive_children_never_use_the_same_parent_face() {
        for previous in 0..CROSS_SECTION_VERTICES {
            let previous = u8::try_from(previous).expect("face fits u8");
            let selected: [usize; 3] = std::array::from_fn(|random| {
                branch_face(
                    u64::try_from(random).expect("candidate fits u64"),
                    Some(previous),
                    [true; CROSS_SECTION_VERTICES],
                )
                .expect("three alternative faces are eligible")
            });

            assert!(selected.iter().all(|&face| face != usize::from(previous)));
            assert_eq!(selected.len(), 3);
            assert!(selected.iter().all(|&face| face < CROSS_SECTION_VERTICES));
            assert!(selected.windows(2).all(|faces| faces[0] != faces[1]));
        }
    }

    #[test]
    fn downward_facing_sides_are_never_selected_for_children() {
        let upward_faces = [false, true, false, true];
        let selected: [usize; 8] = std::array::from_fn(|random| {
            branch_face(
                u64::try_from(random).expect("candidate fits u64"),
                None,
                upward_faces,
            )
            .expect("an upward face is available")
        });

        assert!(selected.iter().all(|&face| upward_faces[face]));
        assert!(selected.contains(&1));
        assert!(selected.contains(&3));
        assert_eq!(branch_face(0, Some(1), [false, true, false, false]), None);
    }

    #[test]
    fn top_face_is_excluded_only_for_branches_off_other_branches() {
        let face_normals = [Vec3::Z, Vec3::X, -Vec3::Z, -Vec3::X];

        assert_eq!(
            eligible_branch_faces(face_normals, 0),
            [true, true, false, true]
        );
        assert_eq!(
            eligible_branch_faces(face_normals, 1),
            [false, true, false, true]
        );
    }

    #[test]
    fn direct_subbranches_reduce_only_their_parents_growth_rate() {
        assert!((branch_growth_rate(0) - 1.0).abs() < f32::EPSILON);
        assert!((branch_growth_rate(1) - 2.0 / 3.0).abs() < f32::EPSILON);
        assert!((branch_growth_rate(2) - 3.0 / 4.0).abs() < f32::EPSILON);
        assert!((branch_growth_rate(3) - 4.0 / 5.0).abs() < f32::EPSILON);

        let (_, stats) = generated(2018);
        assert!(
            stats
                .growth_rates
                .iter()
                .any(|sample| sample.direct_children > 0)
        );
        assert!(stats.growth_rates.iter().all(|sample| {
            (sample.actual_length
                - sample.nominal_length * branch_growth_rate(sample.direct_children))
            .abs()
                < 1.0e-8
        }));
        assert!(stats.growth_rates.iter().any(|sample| {
            sample.depth > 0
                && sample.direct_children == 0
                && (sample.actual_length - sample.nominal_length).abs() < f32::EPSILON
        }));
    }

    #[test]
    fn branch_sections_bend_progressively_toward_the_light() {
        let (_, stats) = generated(2018);
        let branch_sections = stats.directions.iter().filter(|sample| sample.depth > 0);

        assert!(
            branch_sections
                .clone()
                .any(|sample| sample.sections_grown == 0)
        );
        assert!(
            branch_sections
                .clone()
                .any(|sample| sample.sections_grown > 0)
        );
        assert!(
            branch_sections
                .filter(|sample| sample.sections_grown == 0)
                .all(|sample| (sample.current - sample.previous).length() < 1.0e-7)
        );
        assert!(
            stats
                .directions
                .iter()
                .filter(|sample| sample.depth > 0 && sample.sections_grown > 0)
                .all(|sample| {
                    sample.current.z > sample.previous.z
                        || (sample.current.z - 1.0).abs() <= f32::EPSILON
                })
        );
    }

    #[test]
    fn every_extruded_ring_twists_within_its_local_45_degree_limit() {
        let (_, stats) = generated(2018);

        assert!(stats.directions.iter().all(|sample| {
            sample.twist_radians >= -FRAC_PI_4 && sample.twist_radians <= FRAC_PI_4
        }));
        assert!(
            stats
                .directions
                .iter()
                .any(|sample| sample.twist_radians < 0.0)
        );
        assert!(
            stats
                .directions
                .iter()
                .any(|sample| sample.twist_radians > 0.0)
        );

        let positive = twisted_x_axis(Vec3::X, Vec3::Z, FRAC_PI_4);
        let negative = twisted_x_axis(Vec3::X, Vec3::Z, -FRAC_PI_4);
        let diagonal = std::f32::consts::FRAC_1_SQRT_2;
        assert!((positive - Vec3::new(diagonal, diagonal, 0.0)).length() < 1.0e-7);
        assert!((negative - Vec3::new(diagonal, -diagonal, 0.0)).length() < 1.0e-7);
    }

    #[test]
    fn final_pass_tapers_every_axis_from_its_inherited_root_to_tip() {
        let options = TreeOptions::for_seed(2018);
        let (_, stats) = generated(2018);

        assert!((0.0..1.0).contains(&options.tip_radius_scale));
        assert!(!stats.taper.is_empty());
        assert!(stats.taper.iter().all(|sample| {
            sample.current < sample.previous
                && sample.current >= sample.root * options.tip_radius_scale - 1.0e-7
                && sample.current <= sample.root
        }));
        assert!(
            stats
                .branches
                .iter()
                .all(|branch| { branch.root_taper_scale > 0.0 && branch.root_taper_scale <= 1.0 })
        );
        assert!((axis_taper_scale(1.0, 0, 5, options.tip_radius_scale) - 1.0).abs() < f32::EPSILON);
        assert!(
            (axis_taper_scale(1.0, 5, 5, options.tip_radius_scale) - options.tip_radius_scale)
                .abs()
                < 1.0e-7
        );
        assert!(
            axis_taper_scale(1.0, 2, 4, options.tip_radius_scale)
                > (1.0 + options.tip_radius_scale) * 0.5
        );
    }

    #[test]
    fn lod0_wood_receives_two_tessellations_and_a_free_smooth() {
        let (tree, _) = generated(2018);
        assert_eq!(
            tree.lod0_wood.triangles.len(),
            tree.lod1_wood.triangles.len() * 16
        );
        assert_eq!(tree.wood_lod1_to_lod0.len(), tree.lod1_wood.vertices.len());
        assert!(
            tree.wood_lod1_to_lod0
                .iter()
                .all(|&lod0_vertex| (lod0_vertex as usize) < tree.lod0_wood.vertices.len())
        );
        assert!(
            tree.wood_lod1_to_lod0
                .iter()
                .enumerate()
                .all(|(lod1_vertex, &lod0_vertex)| {
                    tree.lod1_wood.uv[lod1_vertex] == tree.lod0_wood.uv[lod0_vertex as usize]
                })
        );
        assert!(
            tree.wood_lod1_to_lod0
                .iter()
                .enumerate()
                .any(|(lod1_vertex, &lod0_vertex)| lod0_vertex as usize != lod1_vertex)
        );
        assert!(
            tree.wood_lod1_to_lod0
                .iter()
                .enumerate()
                .any(|(lod1_vertex, &lod0_vertex)| {
                    tree.lod1_wood.vertices[lod1_vertex]
                        != tree.lod0_wood.vertices[lod0_vertex as usize]
                })
        );
    }

    #[test]
    fn lod0_foliage_preserves_lod1_equivalents() {
        let (tree, _) = generated(2018);
        assert!(tree.lod0_foliage.triangles.len() > tree.lod1_foliage.triangles.len());
        assert!(tree.lod0_foliage.triangles.len() < tree.lod1_foliage.triangles.len() * 4);
        assert_eq!(
            tree.foliage_lod1_to_lod0.len(),
            tree.lod1_foliage.vertices.len()
        );
        assert!(
            tree.foliage_lod1_to_lod0
                .iter()
                .enumerate()
                .all(|(lod1_vertex, &lod0_vertex)| {
                    lod0_vertex as usize == lod1_vertex
                        && tree.lod1_foliage.vertices[lod1_vertex]
                            == tree.lod0_foliage.vertices[lod0_vertex as usize]
                })
        );
    }

    #[test]
    fn tessellated_square_ring_edges_are_projected_out_to_the_tube_radius() {
        let lower = [0, 1, 2, 3];
        let upper = [4, 5, 6, 7];
        let mut source = Mesh {
            vertices: vec![
                Vec3::X,
                Vec3::Y,
                -Vec3::X,
                -Vec3::Y,
                Vec3::X + Vec3::Z * 2.0,
                Vec3::Y + Vec3::Z * 2.0,
                -Vec3::X + Vec3::Z * 2.0,
                -Vec3::Y + Vec3::Z * 2.0,
            ],
            ..Mesh::default()
        };
        connect_rings(&mut source.triangles, &lower, &upper);
        let segment = TubeSegment { lower, upper };
        let tube = TubeProjectionSegment::from_mesh(&source, segment);
        let tessellated = source.tessellated_attributed();
        let midpoint = tessellated_edge_vertex([lower[0], lower[1]], &tessellated.new_vertices);
        let midpoint_before = tessellated.mesh.vertices[midpoint as usize];
        let mut rounded = tessellated.mesh;

        round_new_wood_vertices(&mut rounded, &tessellated.new_vertices, &[tube], &[]);

        assert!((midpoint_before.length() - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-7);
        assert!((rounded.vertices[midpoint as usize].length() - 1.0).abs() < 1.0e-7);
        assert_eq!(&rounded.vertices[..source.vertices.len()], &source.vertices);
    }

    #[test]
    fn branch_ring_projection_owns_parent_child_junction_edges() {
        let parent = TubeProjectionSegment {
            source: TubeSegment {
                lower: [0, 1, 2, 3],
                upper: [4, 5, 6, 7],
            },
            lower_centre: -Vec3::Z,
            upper_centre: Vec3::Z,
            lower_radius: 1.0,
            upper_radius: 1.0,
        };
        let child = TubeProjectionSegment {
            source: TubeSegment {
                lower: [8, 9, 10, 11],
                upper: [12, 13, 14, 15],
            },
            lower_centre: Vec3::ZERO,
            upper_centre: Vec3::X * 2.0,
            lower_radius: 1.0,
            upper_radius: 1.0,
        };
        let position = Vec3::splat(0.5);
        let parent_target = parent
            .projected_surface(position)
            .expect("point projects onto parent tube")
            .0;
        let child_target = child
            .projected_surface(position)
            .expect("point projects onto child tube")
            .0;
        let junction = BranchJunction {
            parent_lower: parent.source.lower,
            parent_upper: parent.source.upper,
            branch_ring: child.source.lower,
            connector_triangles: TriangleSpan {
                index_start: 0,
                index_count: 0,
            },
            parent_bark_axis: encode_bark_axis(Vec3::Z),
        };
        let junction_target = rounded_tube_target(position, [0, 8], &[parent, child], &[junction])
            .expect("junction edge uses the branch ring target");

        assert!((junction_target - parent_target).length() > 0.1);
        assert!((junction_target - child_target).length() < 1.0e-7);
    }

    #[test]
    fn terminal_ring_vertices_are_pinned_to_their_pre_projection_plane() {
        let lower = [0, 1, 2, 3];
        let upper = [4, 5, 6, 7];
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(-1.0, 1.0, 0.0),
                Vec3::new(-0.5, -0.5, 1.0),
                Vec3::new(0.5, -0.5, 1.0),
                Vec3::new(0.5, 0.5, 1.0),
                Vec3::new(-0.5, 0.5, 1.0),
            ],
            ..Mesh::default()
        };
        connect_rings(&mut mesh.triangles, &lower, &upper);

        let lods = build_mesh_lods(mesh, &[lower], &[TubeSegment { lower, upper }], &[], 0);
        let mut edge_uses = HashMap::<(u32, u32), usize>::new();
        for triangle in lods.lod0.triangles.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                *edge_uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let boundary_edges = edge_uses
            .into_iter()
            .filter_map(|(edge, uses)| (uses == 1).then_some(edge))
            .collect::<Vec<_>>();
        let mut lower_boundary = HashSet::from([lower[0]]);
        while let Some((a, b)) = boundary_edges
            .iter()
            .copied()
            .find(|(a, b)| lower_boundary.contains(a) ^ lower_boundary.contains(b))
        {
            lower_boundary.extend([a, b]);
        }

        assert_eq!(lower_boundary.len(), CROSS_SECTION_VERTICES * 4);
        assert!(
            lower_boundary
                .iter()
                .all(|&vertex| lods.lod0.vertices[vertex as usize].z.abs() < 1.0e-7)
        );
        assert!(
            lower
                .iter()
                .all(|&vertex| lods.lod1.vertices[vertex as usize].z.abs() < 1.0e-7)
        );
    }

    #[test]
    fn different_seeds_generate_different_valid_trees() {
        let first = generate_tree(1);
        let second = generate_tree(2);

        assert_ne!(first.lod0_wood.vertices, second.lod0_wood.vertices);
        for tree in [&first, &second] {
            assert_eq!(tree.lod0_wood.uv.len(), tree.lod0_wood.vertices.len());
            assert_eq!(tree.lod1_wood.uv.len(), tree.lod1_wood.vertices.len());
            assert!(
                tree.lod0_wood
                    .uv
                    .iter()
                    .chain(&tree.lod1_wood.uv)
                    .all(|&axis| decode_bark_axis(axis).is_normalized())
            );
            for mesh in [
                &tree.lod0_wood,
                &tree.lod0_foliage,
                &tree.lod1_wood,
                &tree.lod1_foliage,
            ] {
                assert_eq!(mesh.vertices.len(), mesh.normals.len());
                assert!(mesh.vertices.iter().all(|vertex| vertex.is_finite()));
                assert!(mesh.normals.iter().all(|normal| normal.is_finite()));
                assert!(
                    mesh.triangles
                        .iter()
                        .all(|&index| (index as usize) < mesh.vertices.len())
                );
                assert!(mesh.triangles.chunks_exact(3).all(|triangle| {
                    let [a, b, c] = [
                        mesh.vertices[triangle[0] as usize],
                        mesh.vertices[triangle[1] as usize],
                        mesh.vertices[triangle[2] as usize],
                    ];
                    (b - a).cross(c - a).length_squared().is_normal()
                }));
            }
        }
    }

    #[test]
    fn branches_use_square_openings_and_rise_from_the_source_face() {
        let (_, stats) = generated(7);
        let options = TreeOptions::for_seed(7);

        assert_eq!(CROSS_SECTION_VERTICES, 4);
        assert!(stats.branches.iter().all(|branch| {
            branch.origin.is_finite()
                && branch.source_normal.is_normalized()
                && branch.source_normal.z >= 0.0
                && (branch.direction
                    - branch_direction(branch.source_normal, options.child_upward_bias))
                .length()
                    < 1.0e-7
                && branch.direction.z > branch.source_normal.z
                && (branch.opening_radius - branch.parent_radius * BRANCH_OPENING_RADIUS_RATIO)
                    .abs()
                    < f32::EPSILON
                && (branch.radius - branch.parent_radius * child_radius_ratio(branch.parent_depth))
                    .abs()
                    < f32::EPSILON
                && branch.radius < branch.opening_radius
                && branch.opening_radius < branch.parent_radius
                && (branch.section_length
                    - branch.parent_section_length * options.child_section_length_ratio)
                    .abs()
                    < f32::EPSILON
                && (branch.collar_length - branch.section_length * BRANCH_COLLAR_LENGTH_RATIO).abs()
                    < f32::EPSILON
                && branch.opening_radius_error < 1.0e-7
        }));
    }

    #[test]
    fn bark_axis_encoding_round_trips_branch_directions() {
        for direction in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            -Vec3::X,
            Vec3::new(0.3, -0.4, 0.8).normalize(),
            Vec3::new(-0.2, 0.7, -0.5).normalize(),
        ] {
            assert!((decode_bark_axis(encode_bark_axis(direction)) - direction).length() < 1.0e-6);
        }
    }

    #[test]
    fn connector_patch_duplicates_only_its_shading_vertices() {
        let original_uv = vec![
            encode_bark_axis(Vec3::X),
            encode_bark_axis(Vec3::Y),
            encode_bark_axis(Vec3::X),
            encode_bark_axis(Vec3::Y),
        ];
        let mut mesh = Mesh {
            vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::ONE],
            normals: vec![Vec3::Z; 4],
            triangles: vec![0, 1, 2, 0, 2, 3],
            uv: original_uv.clone(),
        };
        let parent_axis = encode_bark_axis(Vec3::Z);
        let duplicates = split_bark_connector_patch(
            &mut mesh,
            BranchJunction {
                parent_lower: [0; CROSS_SECTION_VERTICES],
                parent_upper: [0; CROSS_SECTION_VERTICES],
                branch_ring: [0; CROSS_SECTION_VERTICES],
                connector_triangles: TriangleSpan {
                    index_start: 0,
                    index_count: 3,
                },
                parent_bark_axis: parent_axis,
            },
            1,
        );

        assert_eq!(duplicates, vec![(0, 4), (1, 5), (2, 6)]);
        assert_eq!(&mesh.triangles[..3], &[4, 5, 6]);
        assert_eq!(&mesh.triangles[3..], &[0, 2, 3]);
        assert_eq!(&mesh.uv[..4], &original_uv);
        for &(source, duplicate) in &duplicates {
            assert_eq!(
                mesh.vertices[duplicate as usize],
                mesh.vertices[source as usize]
            );
            assert_eq!(
                mesh.normals[duplicate as usize],
                mesh.normals[source as usize]
            );
            assert_eq!(mesh.uv[duplicate as usize], parent_axis);
        }
    }

    #[test]
    fn generated_wood_is_geometrically_watertight_across_bark_seams() {
        for seed in 0..64 {
            let (tree, stats) = generated(seed);
            let vertex_key = |vertex: u32| {
                tree.lod1_wood.vertices[vertex as usize]
                    .to_array()
                    .map(f32::to_bits)
            };
            let mut edge_uses = HashMap::<([u32; 3], [u32; 3]), usize>::new();
            for triangle in tree.lod1_wood.triangles.chunks_exact(3) {
                for (a, b) in [
                    (triangle[0], triangle[1]),
                    (triangle[1], triangle[2]),
                    (triangle[2], triangle[0]),
                ] {
                    let a = vertex_key(a);
                    let b = vertex_key(b);
                    *edge_uses
                        .entry(if a < b { (a, b) } else { (b, a) })
                        .or_default() += 1;
                }
            }
            assert!(
                edge_uses.values().all(|uses| matches!(uses, 1 | 2)),
                "seed {seed} contains a non-manifold edge"
            );
            assert_eq!(
                edge_uses.values().filter(|&&uses| uses == 1).count(),
                (stats.branches.len() + 2) * CROSS_SECTION_VERTICES,
                "seed {seed} does not have exactly one open trunk base and one open tip per axis"
            );
        }
    }
}
