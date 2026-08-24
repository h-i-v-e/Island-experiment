use std::{
    collections::VecDeque,
    f32::consts::{FRAC_PI_4, TAU},
};

use crate::{
    ISLAND_WORLD_METRES, Mesh, Vec3,
    clustered_foliage::{FoliageCrown, generate_cluster_foliage},
    rng::Rng,
};

const TREE_SEED_SALT: u64 = 0x7472_6565_5f77_6f6f;
const MINIMUM_UPWARD_DIRECTION: f32 = 0.12;
const CROSS_SECTION_VERTICES: usize = 4;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeMeshes {
    pub lod0_wood: Mesh,
    pub lod0_foliage: Mesh,
    pub lod1_wood: Mesh,
    pub lod1_foliage: Mesh,
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
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            maximum_child_branches: 8,
            trunk_sections: 10,
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
            tip_radius_scale: 0.2,
        }
    }
}

impl TreeOptions {
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
            ]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
            && self.branch_probability[0] <= self.branch_probability[1]
            && self.branch_probability[1] <= 1.0
            && self.phototropism <= 1.0
            && self.maximum_twist_radians <= FRAC_PI_4
            && self.tip_radius_scale < 1.0
    }
}

#[derive(Debug)]
struct GrowingAxis {
    ring: [u32; CROSS_SECTION_VERTICES],
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
    radius: f32,
    parent_section_length: f32,
    section_length: f32,
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
    branches: Vec<BranchRecord>,
    directions: Vec<DirectionRecord>,
    growth_rates: Vec<GrowthRateRecord>,
    taper: Vec<TaperRecord>,
}

struct TreeGenerator {
    options: TreeOptions,
    rng: Rng,
    pending: VecDeque<GrowingAxis>,
    taper_rings: Vec<TaperRing>,
    terminal_rings: Vec<[u32; CROSS_SECTION_VERTICES]>,
    wood: Mesh,
    stats: TreeGenerationStats,
}

impl TreeGenerator {
    fn new(seed: u64, options: TreeOptions) -> Self {
        debug_assert!(options.is_valid());
        let mut generator = Self {
            options,
            rng: Rng::new(seed ^ TREE_SEED_SALT),
            pending: VecDeque::new(),
            taper_rings: Vec::new(),
            terminal_rings: Vec::new(),
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
        let ring = generator.append_ring(centre, Vec3::X, Vec3::Y, radius, 1.0);
        generator.terminal_rings.push(ring);
        generator.pending.push_back(GrowingAxis {
            ring,
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
        // them after tapering but before wood LOD smoothing can move vertices.
        let foliage_supports: Vec<Vec3> = self
            .terminal_rings
            .iter()
            .skip(1)
            .map(|ring| ring_barycentre(&self.wood, ring))
            .collect();
        let wood = build_mesh_lods(self.wood, &self.terminal_rings);
        let foliage = generate_cluster_foliage(
            foliage_seed,
            &[FoliageCrown {
                trunk: Vec3::ZERO,
                tips: &foliage_supports,
            }],
        )
        .unwrap_or_else(|error| panic!("single-tree foliage generation failed: {error}"));
        (
            TreeMeshes {
                lod0_wood: wood.lod0,
                lod0_foliage: foliage.lod0,
                lod1_wood: wood.lod1,
                lod1_foliage: foliage.lod1,
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
        let probability = branch_probability(
            axis.sections_grown,
            axis.section_budget,
            self.options.branch_probability,
        );
        let branches_needed = self
            .options
            .maximum_child_branches
            .saturating_sub(self.stats.child_branches);
        let must_spawn_to_reach_cap = axis.depth == 0 && branches_needed > remaining_sections;
        let trunk_has_branch_clearance = axis.depth > 0
            || axis.centre.z * ISLAND_WORLD_METRES
                >= self.options.minimum_trunk_branch_height_metres;
        let will_spawn_branch = self.stats.child_branches < self.options.maximum_child_branches
            && axis.depth < self.options.maximum_branch_depth
            && trunk_has_branch_clearance
            && (must_spawn_to_reach_cap || self.rng.unit() < probability);
        let ring = self.append_ring(centre, x_axis, next_y_axis, radius, taper_scale);

        let spawned_face = if will_spawn_branch {
            self.spawn_child(ChildSource {
                lower_ring: &axis.ring,
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
            connect_rings(&mut self.wood.triangles, &axis.ring, &ring);
        }
        let direct_children = axis.direct_children + u8::from(spawned_face.is_some());
        let continues = remaining_sections > 0
            && radius * ISLAND_WORLD_METRES >= self.options.minimum_radius_metres
            && section_length * ISLAND_WORLD_METRES >= self.options.minimum_section_length_metres;
        if continues {
            self.pending.push_back(GrowingAxis {
                ring,
                centre,
                direction,
                x_axis,
                radius,
                section_length,
                remaining_sections,
                section_budget: axis.section_budget,
                sections_grown: axis.sections_grown.saturating_add(1),
                direct_children,
                previous_branch_face: spawned_face.or(axis.previous_branch_face),
                root_taper_scale: axis.root_taper_scale,
                taper_scale,
                depth: axis.depth,
            });
        } else {
            self.terminal_rings.push(ring);
        }
    }

    fn spawn_child(&mut self, parent: ChildSource<'_>) -> Option<u8> {
        let face_normals: [Vec3; CROSS_SECTION_VERTICES] = std::array::from_fn(|face| {
            let next = (face + 1) % CROSS_SECTION_VERTICES;
            let lower_left = self.wood.vertices[parent.lower_ring[face] as usize];
            let lower_right = self.wood.vertices[parent.lower_ring[next] as usize];
            let upper_left = self.wood.vertices[parent.upper_ring[face] as usize];
            let upper_right = self.wood.vertices[parent.upper_ring[next] as usize];
            let across = (lower_right - lower_left) + (upper_right - upper_left);
            let along = (upper_left - lower_left) + (upper_right - lower_right);
            across.cross(along).normalize_or(parent.direction)
        });
        let upward_faces = eligible_branch_faces(face_normals, parent.depth);
        let face = branch_face(
            self.rng.next_u64(),
            parent.previous_branch_face,
            upward_faces,
        )?;
        let next = (face + 1) % CROSS_SECTION_VERTICES;
        let lower_left = self.wood.vertices[parent.lower_ring[face] as usize];
        let lower_right = self.wood.vertices[parent.lower_ring[next] as usize];
        let upper_left = self.wood.vertices[parent.upper_ring[face] as usize];
        let upper_right = self.wood.vertices[parent.upper_ring[next] as usize];
        let across =
            ((lower_right - lower_left) + (upper_right - upper_left)).normalize_or(Vec3::X);
        let along = ((upper_left - lower_left) + (upper_right - lower_right))
            .normalize_or(parent.direction);
        let direction = face_normals[face];
        let origin = (lower_left + lower_right + upper_left + upper_right) * 0.25;
        let radius = parent.radius;
        let section_length = parent.section_length;
        let root_taper_scale = (parent.lower_taper_scale + parent.upper_taper_scale) * 0.5;
        let x_axis = (-across - along).normalize_or(Vec3::X);
        let y_axis = direction.cross(x_axis).normalize_or_zero();
        let ring = self.append_ring(origin, x_axis, y_axis, radius, root_taper_scale);
        connect_rings_with_opening(
            &mut self.wood.triangles,
            parent.lower_ring,
            parent.upper_ring,
            face,
            &ring,
        );
        let opening_radius_error = ring.iter().fold(0.0_f32, |error, &vertex| {
            error.max(((self.wood.vertices[vertex as usize] - origin).length() - radius).abs())
        });
        let section_budget = parent.remaining_sections.clamp(2, 5);
        self.pending.push_back(GrowingAxis {
            ring,
            centre: origin,
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
            source_normal: direction,
            direction,
            parent_radius: parent.radius,
            radius,
            parent_section_length: parent.section_length,
            section_length,
            root_taper_scale,
            opening_radius_error,
        });
        Some(u8::try_from(face).expect("branch face fits u8"))
    }

    fn append_ring(
        &mut self,
        centre: Vec3,
        x_axis: Vec3,
        y_axis: Vec3,
        radius: f32,
        taper_scale: f32,
    ) -> [u32; CROSS_SECTION_VERTICES] {
        let first = u32::try_from(self.wood.vertices.len()).expect("tree mesh fits u32 indices");
        self.wood.vertices.reserve(CROSS_SECTION_VERTICES);
        let ring_size = u16::try_from(CROSS_SECTION_VERTICES).expect("ring size fits u16");
        for side in 0..CROSS_SECTION_VERTICES {
            let side_index = u16::try_from(side).expect("tree ring vertex count fits u16");
            let angle = f32::from(side_index) * TAU / f32::from(ring_size);
            self.wood
                .vertices
                .push(centre + (x_axis * angle.cos() + y_axis * angle.sin()) * radius);
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

fn build_mesh_lods(mut lod1: Mesh, terminal_rings: &[[u32; CROSS_SECTION_VERTICES]]) -> MeshLods {
    let lod1_to_lod0 = (0..lod1.vertices.len())
        .map(|vertex| u32::try_from(vertex).expect("tree mesh fits u32 indices"))
        .collect::<Vec<_>>();
    let terminal_planes = terminal_rings
        .iter()
        .map(|ring| ring_plane(&lod1, ring))
        .collect::<Vec<_>>();
    let tessellated = lod1.tessellated_attributed();
    let mut lod0 = tessellated.mesh;
    smooth_all_vertices(&mut lod0);
    pin_terminal_rings(
        &mut lod0,
        terminal_rings,
        &terminal_planes,
        &tessellated.new_vertices,
    );
    for (lod1_vertex, &lod0_vertex) in lod1_to_lod0.iter().enumerate() {
        lod1.vertices[lod1_vertex] = lod0.vertices[lod0_vertex as usize];
    }
    lod0.calculate_normals();
    lod1.calculate_normals();
    MeshLods {
        lod0,
        lod1,
        lod1_to_lod0,
    }
}

fn ring_barycentre(mesh: &Mesh, ring: &[u32; CROSS_SECTION_VERTICES]) -> Vec3 {
    let total = ring.iter().fold(Vec3::ZERO, |total, &vertex| {
        total + mesh.vertices[vertex as usize]
    });
    let vertex_count = u16::try_from(CROSS_SECTION_VERTICES).expect("terminal ring size fits u16");
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

fn smooth_all_vertices(mesh: &mut Mesh) {
    let adjacency = mesh.adjacency();
    mesh.vertices = mesh
        .vertices
        .iter()
        .enumerate()
        .map(|(vertex, &position)| {
            let neighbours = &adjacency[vertex];
            let total = neighbours.iter().fold(position, |total, &neighbour| {
                total + mesh.vertices[neighbour]
            });
            let count = u16::try_from(neighbours.len() + 1).expect("tree vertex degree fits u16");
            total / f32::from(count)
        })
        .collect();
    mesh.calculate_normals();
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
) {
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

fn axis_taper_scale(
    root_scale: f32,
    sections_grown: u8,
    section_budget: u8,
    tip_radius_scale: f32,
) -> f32 {
    let progress = f32::from(sections_grown.min(section_budget)) / f32::from(section_budget.max(1));
    let tip_scale = root_scale * tip_radius_scale;
    (tip_scale - root_scale).mul_add(progress, root_scale)
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
    TreeGenerator::new(seed, TreeOptions::default())
        .generate(seed)
        .0
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    fn generated(seed: u64) -> (TreeMeshes, TreeGenerationStats) {
        TreeGenerator::new(seed, TreeOptions::default()).generate(seed)
    }

    #[test]
    fn tree_is_deterministic_and_has_terminal_foliage_supports() {
        let first = generated(42);
        let second = generated(42);

        assert_eq!(first, second);
        assert_eq!(first.1.child_branches, 8);
        assert_eq!(first.1.branches.len(), 8);
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
                TreeOptions::default().maximum_child_branches,
                "seed {seed} did not reach the branch cap"
            );
        }
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
        let options = TreeOptions::default();

        for seed in 0..256 {
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
        let options = TreeOptions::default();
        let (_, stats) = generated(2018);

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
    }

    #[test]
    fn lod0_tessellation_and_smoothing_projects_back_to_lod1_equivalents() {
        let (tree, _) = generated(2018);
        for (lod0, lod1, equivalents) in [
            (&tree.lod0_wood, &tree.lod1_wood, &tree.wood_lod1_to_lod0),
            (
                &tree.lod0_foliage,
                &tree.lod1_foliage,
                &tree.foliage_lod1_to_lod0,
            ),
        ] {
            assert_eq!(lod0.triangles.len(), lod1.triangles.len() * 4);
            assert_eq!(equivalents.len(), lod1.vertices.len());
            assert!(
                equivalents
                    .iter()
                    .enumerate()
                    .all(|(lod1_vertex, &lod0_vertex)| {
                        lod0_vertex as usize == lod1_vertex
                            && lod1.vertices[lod1_vertex] == lod0.vertices[lod0_vertex as usize]
                    })
            );
        }

        let mut triangle = Mesh {
            vertices: vec![
                Vec3::ZERO,
                Vec3::new(3.0, 0.0, 0.0),
                Vec3::new(6.0, 0.0, 0.0),
            ],
            triangles: vec![0, 1, 2],
            ..Mesh::default()
        };
        smooth_all_vertices(&mut triangle);
        assert!(
            triangle
                .vertices
                .iter()
                .all(|&vertex| vertex == Vec3::X * 3.0)
        );
    }

    #[test]
    fn terminal_ring_vertices_are_pinned_to_their_pre_smoothing_plane() {
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

        let lods = build_mesh_lods(mesh, &[lower]);
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

        assert_eq!(lower_boundary.len(), CROSS_SECTION_VERTICES * 2);
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
    fn branches_use_square_openings_and_source_face_normals() {
        let (_, stats) = generated(7);

        assert_eq!(CROSS_SECTION_VERTICES, 4);
        assert!(stats.branches.iter().all(|branch| {
            branch.origin.is_finite()
                && branch.source_normal.is_normalized()
                && branch.source_normal.z >= 0.0
                && (branch.direction - branch.source_normal).length() < 1.0e-7
                && (branch.radius - branch.parent_radius).abs() < f32::EPSILON
                && (branch.section_length - branch.parent_section_length).abs() < f32::EPSILON
                && branch.opening_radius_error < 1.0e-7
        }));
    }

    #[test]
    fn generated_wood_has_only_the_intended_open_axis_ends() {
        for seed in 0..64 {
            let (tree, stats) = generated(seed);
            let mut edge_uses = HashMap::<(u32, u32), usize>::new();
            for triangle in tree.lod1_wood.triangles.chunks_exact(3) {
                for (a, b) in [
                    (triangle[0], triangle[1]),
                    (triangle[1], triangle[2]),
                    (triangle[2], triangle[0]),
                ] {
                    *edge_uses.entry((a.min(b), a.max(b))).or_default() += 1;
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
