//! Engine-independent settlement clearance, applied before decoration export.
use super::Island;
use crate::ferns::MAX_FERN_OUTER_RADIUS_METRES;
use crate::forest::{ForestMeshes, MeshRange, TreePlacement, forest_floor_mask, log_site_keys};
use crate::{ISLAND_WORLD_METRES, Mesh, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Native XY metres; independent of the normalized terrain coordinate system.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecorationClearance {
    pub centre: [f32; 2],
    pub half_extents: [f32; 2],
    pub yaw: f32,
}

impl DecorationClearance {
    /// Reject invalid or unbounded requests before mutating resident geometry.
    ///
    /// # Errors
    /// Returns an error for nonfinite coordinates or invalid extents.
    pub fn validate(&self) -> Result<(), String> {
        if !self
            .centre
            .iter()
            .chain(&self.half_extents)
            .all(|v| v.is_finite())
            || !self.yaw.is_finite()
            || self
                .half_extents
                .iter()
                .any(|v| *v <= 0. || *v > ISLAND_WORLD_METRES)
        {
            return Err(
                "decoration clearance requires finite centres/yaw and positive extents <= 2000m"
                    .into(),
            );
        }
        Ok(())
    }

    #[must_use]
    pub fn intersects_circle(&self, centre: Vec2, radius: f32) -> bool {
        let (s, c) = self.yaw.sin_cos();
        let delta = centre - Vec2::from_array(self.centre);
        let local = Vec2::new(c * delta.x + s * delta.y, -s * delta.x + c * delta.y);
        let outside = (local.abs() - Vec2::from_array(self.half_extents)).max(Vec2::ZERO);
        outside.length_squared() <= radius * radius
    }

    fn intersects_bounds(&self, min: Vec2, max: Vec2) -> bool {
        let centre = (min + max) * 0.5;
        let half = (max - min) * 0.5;
        let delta = centre - Vec2::from_array(self.centre);
        let (s, c) = self.yaw.sin_cos();
        let extent = Vec2::from_array(self.half_extents);
        delta.x.abs() <= half.x + c.abs() * extent.x + s.abs() * extent.y
            && delta.y.abs() <= half.y + s.abs() * extent.x + c.abs() * extent.y
            && (c * delta.x + s * delta.y).abs() <= extent.x + c.abs() * half.x + s.abs() * half.y
            && (-s * delta.x + c * delta.y).abs() <= extent.y + s.abs() * half.x + c.abs() * half.y
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DecorationClearanceStats {
    pub trees_removed: usize,
    pub logs_removed: usize,
    pub boulders_removed: usize,
}

/// Circle inputs are normalized island units; authored clearances are metres.
pub(crate) fn intersects(clearance: &[DecorationClearance], point: Vec2, radius: f32) -> bool {
    clearance.iter().any(|area| {
        area.intersects_circle(point * ISLAND_WORLD_METRES, radius * ISLAND_WORLD_METRES)
    })
}

impl Island {
    /// Clear complete owners before any render/collision/shadow exports. The
    /// caller must hold exclusive access and use a fresh pre-clearance snapshot
    /// when changing the town recipe; removal is deliberately not reversible.
    ///
    /// # Errors
    /// Returns an error without changing the island if any clearance is invalid
    /// or the request contains more than 4096 rectangles.
    pub fn clear_decorations(
        &mut self,
        clearance: &[DecorationClearance],
    ) -> Result<DecorationClearanceStats, String> {
        if clearance.len() > 4096 {
            return Err("at most 4096 decoration clearances are supported".into());
        }
        for area in clearance {
            area.validate()?;
        }
        if clearance.is_empty() {
            return Ok(DecorationClearanceStats::default());
        }
        let before = self.forest.placements.len();
        let logs_before = self.forest.logs.len();
        let removed = cleared_trees(&self.forest, clearance);
        let ground_owners = (!removed.is_empty())
            .then(|| TreeGroundOwners::new(self.forest.placements(), &removed));
        clear_forest(&mut self.forest, clearance, self.seed, &removed);
        let trees_removed = before - self.forest.placements.len();
        self.forest_stats.accepted_trees = self
            .forest_stats
            .accepted_trees
            .saturating_sub(trees_removed);
        self.forest_stats.exclusion_zone += trees_removed;
        let mut boulders_removed = 0;
        if let Some(decorations) = self.decorations.get_mut() {
            decorations.set_tree_anchors(self.forest.placements.iter().map(|tree| tree.anchor));
            decorations
                .bushes
                .retain(|point| !intersects(clearance, point.truncate(), 1. / ISLAND_WORLD_METRES));
            let count = decorations.boulders.len();
            decorations
                .boulders
                .retain(|sphere| !intersects(clearance, sphere.truncate().truncate(), sphere.w));
            boulders_removed = count - decorations.boulders.len();
        }
        retain_mesh_components(&mut self.river_rock_mesh, clearance);
        self.reeds.clear_settlement(clearance);
        let ferns_removed = self.ferns.clear_settlement(
            clearance,
            |root| {
                ground_owners
                    .as_ref()
                    .is_some_and(|owners| owners.is_cleared(root))
            },
            &self.terrain,
        );
        self.clear_forest_floor(clearance, &removed, ferns_removed);
        #[cfg(feature = "surface-atlas")]
        {
            self.surface_atlases = super::surface_atlas::SurfaceAtlasCache::default();
        }
        Ok(DecorationClearanceStats {
            trees_removed,
            logs_removed: logs_before - self.forest.logs.len(),
            boulders_removed,
        })
    }

    fn clear_forest_floor(
        &mut self,
        clearance: &[DecorationClearance],
        removed_trees: &HashSet<u32>,
        ferns_removed: bool,
    ) {
        let surviving_floor = (!removed_trees.is_empty() || ferns_removed).then(|| {
            let mut floor =
                forest_floor_mask(self.seed, self.terrain.mesh(), self.forest.placements());
            for &vertex in self.ferns.support_vertices() {
                if let Some(value) = floor.get_mut(vertex as usize) {
                    *value = true;
                }
            }
            floor
        });
        let (min, max) = clearance.iter().fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(min, max), area| {
                let centre = Vec2::from_array(area.centre);
                let reach = Vec2::splat(Vec2::from_array(area.half_extents).length());
                (min.min(centre - reach), max.max(centre + reach))
            },
        );
        for (index, (value, vertex)) in self
            .environment
            .values
            .iter_mut()
            .zip(self.terrain.vertices())
            .enumerate()
        {
            if value.x <= 0. {
                continue;
            }
            let point = vertex.truncate() * ISLAND_WORLD_METRES;
            let cleared = point.cmpge(min).all()
                && point.cmple(max).all()
                && clearance
                    .iter()
                    .any(|area| area.intersects_circle(point, 0.));
            // A coarse face may share a retired planting vertex with its neighbour.
            let removed_tree =
                u32::try_from(index).is_ok_and(|index| removed_trees.contains(&index));
            if cleared
                || removed_tree
                || surviving_floor.as_ref().is_some_and(|floor| !floor[index])
            {
                // Keep the stone and road cover channel, including graded paving.
                value.x = 0.;
            }
        }
    }
}

struct TreeGroundOwner {
    anchor: Vec2,
    fern_radius: f32,
    cleared: bool,
}

struct TreeGroundOwners {
    cells: HashMap<[i32; 2], Vec<TreeGroundOwner>>,
    cell_size: f32,
}

impl TreeGroundOwners {
    fn new(trees: &[TreePlacement], cleared: &HashSet<u32>) -> Self {
        let radius = MAX_FERN_OUTER_RADIUS_METRES / ISLAND_WORLD_METRES;
        let cell_size = radius
            * trees
                .iter()
                .map(|tree| tree.scale.sqrt())
                .fold(1., f32::max);
        let mut owners = Self {
            cells: HashMap::new(),
            cell_size,
        };
        for tree in trees {
            let anchor = tree.anchor.truncate();
            owners
                .cells
                .entry(owners.cell(anchor))
                .or_default()
                .push(TreeGroundOwner {
                    anchor,
                    fern_radius: radius * tree.scale.sqrt(),
                    cleared: cleared.contains(&tree.terrain_vertex),
                });
        }
        owners
    }

    fn cell(&self, point: Vec2) -> [i32; 2] {
        (point / self.cell_size).floor().as_ivec2().to_array()
    }

    // Older and current snapshots store fern roots without tree indices.
    // The nearest trunk within the fern generation envelope owns the ground cover.
    fn is_cleared(&self, root: Vec2) -> bool {
        let [x, y] = self.cell(root);
        (y - 1..=y + 1)
            .flat_map(|row| (x - 1..=x + 1).map(move |column| [column, row]))
            .filter_map(|cell| self.cells.get(&cell))
            .flatten()
            .filter_map(|tree| {
                let distance = root.distance_squared(tree.anchor);
                (distance <= tree.fern_radius.powi(2)).then_some((distance, tree.cleared))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .is_some_and(|(_, cleared)| cleared)
    }
}

fn cleared_trees(forest: &ForestMeshes, clearance: &[DecorationClearance]) -> HashSet<u32> {
    let radii: Vec<_> = forest
        .hybrid
        .as_ref()
        .map(|catalog| {
            catalog
                .prototypes
                .iter()
                .map(|tree| {
                    tree.lod0_foliage
                        .vertices
                        .iter()
                        .chain(&tree.lod0_wood.vertices)
                        .map(|p| p.truncate().length())
                        .fold(0., f32::max)
                })
                .collect()
        })
        .unwrap_or_default();
    let mut removed: HashSet<_> = forest
        .placements
        .iter()
        .filter(|tree| {
            intersects(
                clearance,
                tree.anchor.truncate(),
                radii
                    .get(usize::from(tree.prototype))
                    .copied()
                    .unwrap_or(5. / ISLAND_WORLD_METRES)
                    * tree.scale,
            )
        })
        .map(|tree| tree.terrain_vertex)
        .collect();
    // A legacy crown is one generated surface shared by several trees. Removing
    // every member of a touched cluster avoids both floating canopy fragments
    // and bare surviving trunks; untouched complete clusters stay byte-identical.
    for cluster in &forest.clusters {
        let contact =
            range_bounds(&forest.lod0_foliage, cluster.lod0_foliage).is_some_and(|(min, max)| {
                clearance
                    .iter()
                    .any(|area| area.intersects_bounds(min, max))
            });
        if contact
            || cluster
                .member_tree_indices
                .iter()
                .any(|&i| removed.contains(&forest.trees[i].terrain_vertex))
        {
            removed.extend(
                cluster
                    .member_tree_indices
                    .iter()
                    .map(|&i| forest.trees[i].terrain_vertex),
            );
        }
    }
    removed
}

fn clear_forest(
    forest: &mut ForestMeshes,
    clearance: &[DecorationClearance],
    seed: u64,
    removed: &HashSet<u32>,
) {
    let cleared_log_sites: HashSet<_> = forest
        .placements
        .iter()
        .copied()
        .filter(|tree| removed.contains(&tree.terrain_vertex))
        .flat_map(|tree| log_site_keys(seed, tree))
        .collect();
    let mut remap = Vec::with_capacity(forest.trees.len());
    let mut next = 0;
    for tree in &forest.trees {
        remap.push((!removed.contains(&tree.terrain_vertex)).then_some(next));
        if remap.last().is_some_and(Option::is_some) {
            next += 1;
        }
    }
    if forest.hybrid.is_some() {
        compact_hybrid_occlusion(forest, removed);
    }
    forest
        .placements
        .retain(|tree| !removed.contains(&tree.terrain_vertex));
    forest.clusters.retain_mut(|cluster| {
        if cluster
            .member_tree_indices
            .iter()
            .any(|&index| remap[index].is_none())
        {
            return false;
        }
        for index in &mut cluster.member_tree_indices {
            *index = remap[*index].expect("retained complete cluster");
        }
        true
    });
    forest
        .trees
        .retain(|tree| !removed.contains(&tree.terrain_vertex));
    forest.logs.retain(|log| {
        !cleared_log_sites.contains(&log.site_key())
            && !intersects(
                clearance,
                log.anchor().truncate(),
                log.bottom.truncate().distance(log.top.truncate()) * 0.5 + log.radius,
            )
    });
    compact_forest_streams(forest);
}

fn compact_forest_streams(forest: &mut ForestMeshes) {
    compact_ranges(
        &mut forest.lod0_wood,
        forest
            .trees
            .iter_mut()
            .map(|tree| &mut tree.lod0_wood)
            .chain(forest.logs.iter_mut().map(|log| &mut log.ranges[0])),
    );
    compact_ranges(
        &mut forest.lod1_wood,
        forest
            .trees
            .iter_mut()
            .map(|tree| &mut tree.lod1_wood)
            .chain(forest.logs.iter_mut().map(|log| &mut log.ranges[1])),
    );
    let has_later_plantings = forest
        .trees
        .last()
        .zip(forest.logs.first())
        .is_some_and(|(tree, log)| tree.lod2_wood.vertex_start > log.ranges[2].vertex_start);
    let wood = forest
        .trees
        .iter_mut()
        .map(|tree| &mut tree.lod2_wood)
        .chain(forest.logs.iter_mut().map(|log| &mut log.ranges[2]));
    if has_later_plantings {
        // Roadside windbreak trunks are appended after the original log ranges.
        let mut ranges: Vec<_> = wood.collect();
        ranges.sort_unstable_by_key(|range| range.vertex_start);
        compact_ranges(&mut forest.lod2_wood, ranges.into_iter());
    } else {
        compact_ranges(&mut forest.lod2_wood, wood);
    }
    compact_ranges(
        &mut forest.lod0_foliage,
        forest
            .clusters
            .iter_mut()
            .map(|cluster| &mut cluster.lod0_foliage),
    );
    if forest.hybrid.is_none() {
        compact_ranges(
            &mut forest.lod1_foliage,
            forest
                .clusters
                .iter_mut()
                .map(|cluster| &mut cluster.lod1_foliage),
        );
    }
}

fn compact_hybrid_occlusion(forest: &mut ForestMeshes, removed: &HashSet<u32>) {
    // Hybrid generation appends one five-vertex / six-triangle crown proxy per
    // placement, in placement order. These CPU AO proxies have no cluster entry.
    let mut ranges: Vec<_> = forest
        .placements
        .iter()
        .enumerate()
        .filter(|(_, tree)| !removed.contains(&tree.terrain_vertex))
        .map(|(index, _)| MeshRange {
            vertex_start: u32::try_from(index * 5).expect("forest proxy vertex budget"),
            vertex_count: 5,
            triangle_start: u32::try_from(index * 18).expect("forest proxy index budget"),
            triangle_count: 18,
        })
        .collect();
    compact_ranges(&mut forest.lod1_foliage, ranges.iter_mut());
}

/// Generated tree ranges precede generated log ranges; all retained ranges stay
/// in source order. Copying towards the beginning preserves source attributes
/// exactly and needs no second mesh allocation or per-vertex remapping table.
fn compact_ranges<'a>(mesh: &mut Mesh, ranges: impl Iterator<Item = &'a mut MeshRange>) {
    let mut vertices = 0;
    let mut indices = 0;
    let normals = mesh.normals.len() == mesh.vertices.len();
    let uv = mesh.uv.len() == mesh.vertices.len();
    for range in ranges {
        let start = range.vertex_start as usize;
        let count = range.vertex_count as usize;
        let triangle_start = range.triangle_start as usize;
        let triangle_count = range.triangle_count as usize;
        mesh.vertices.copy_within(start..start + count, vertices);
        if normals {
            mesh.normals.copy_within(start..start + count, vertices);
        }
        if uv {
            mesh.uv.copy_within(start..start + count, vertices);
        }
        mesh.triangles
            .copy_within(triangle_start..triangle_start + triangle_count, indices);
        let new_start = u32::try_from(vertices).expect("source forest vertex budget");
        for index in &mut mesh.triangles[indices..indices + triangle_count] {
            *index = *index - range.vertex_start + new_start;
        }
        range.vertex_start = new_start;
        range.triangle_start = u32::try_from(indices).expect("source forest index budget");
        vertices += count;
        indices += triangle_count;
    }
    mesh.vertices.truncate(vertices);
    mesh.triangles.truncate(indices);
    if normals {
        mesh.normals.truncate(vertices);
    }
    if uv {
        mesh.uv.truncate(vertices);
    }
}

fn range_bounds(mesh: &Mesh, range: MeshRange) -> Option<(Vec2, Vec2)> {
    mesh.vertices
        .get(
            range.vertex_start as usize..range.vertex_start as usize + range.vertex_count as usize,
        )?
        .iter()
        .map(|point| point.truncate() * ISLAND_WORLD_METRES)
        .fold(None, |bounds, point| {
            Some(bounds.map_or((point, point), |(min, max): (Vec2, Vec2)| {
                (min.min(point), max.max(point))
            }))
        })
}

/// Removes whole connected decoration components, including normal/UV seams.
pub(crate) fn retain_mesh_components(mesh: &mut Mesh, clearance: &[DecorationClearance]) {
    let mut parents: Vec<_> = (0..mesh.vertices.len()).collect();
    let mut positions = HashMap::new();
    for (index, point) in mesh.vertices.iter().enumerate() {
        let key = point
            .to_array()
            .map(|v| if v == 0. { 0 } else { v.to_bits() });
        if let Some(&other) = positions.get(&key) {
            join(&mut parents, index, other);
        } else {
            positions.insert(key, index);
        }
    }
    for tri in mesh.triangles.chunks_exact(3) {
        join(&mut parents, tri[0] as usize, tri[1] as usize);
        join(&mut parents, tri[1] as usize, tri[2] as usize);
    }
    let mut bounds = HashMap::<usize, (Vec2, Vec2)>::new();
    for (index, point) in mesh.vertices.iter().enumerate() {
        let root = find(&mut parents, index);
        let xy = point.truncate() * ISLAND_WORLD_METRES;
        bounds
            .entry(root)
            .and_modify(|(min, max)| {
                *min = min.min(xy);
                *max = max.max(xy);
            })
            .or_insert((xy, xy));
    }
    let blocked: std::collections::HashSet<_> = bounds
        .into_iter()
        .filter_map(|(root, (min, max))| {
            clearance
                .iter()
                .any(|area| area.intersects_bounds(min, max))
                .then_some(root)
        })
        .collect();
    if blocked.is_empty() {
        return;
    }
    mesh.triangles
        .retain(|index| !blocked.contains(&find(&mut parents, *index as usize)));
    let mut used = vec![false; mesh.vertices.len()];
    for &index in &mesh.triangles {
        used[index as usize] = true;
    }
    compact_mesh(mesh, &used);
}

/// Fern fronds share a root sidecar but need not share mesh vertices. Treat the
/// complete clump as its owner so a contacting frond cannot leave half a plant.
pub(crate) fn retain_mesh_instances(
    mesh: &mut Mesh,
    roots: &[Vec2],
    clearance: &[DecorationClearance],
    cleared_owner: &impl Fn(Vec2) -> bool,
) -> Option<Vec<bool>> {
    let key = |point: Vec2| {
        point
            .to_array()
            .map(|v| if v == 0. { 0 } else { v.to_bits() })
    };
    let mut bounds = HashMap::<[u32; 2], (Vec2, Vec2)>::new();
    for (&point, &root) in mesh.vertices.iter().zip(roots) {
        let point = point.truncate() * ISLAND_WORLD_METRES;
        bounds
            .entry(key(root))
            .and_modify(|(min, max)| {
                *min = min.min(point);
                *max = max.max(point);
            })
            .or_insert((point, point));
    }
    let blocked: HashSet<_> = bounds
        .into_iter()
        .filter_map(|(root, (min, max))| {
            (cleared_owner(Vec2::from_array(root.map(f32::from_bits)))
                || clearance
                    .iter()
                    .any(|area| area.intersects_bounds(min, max)))
            .then_some(root)
        })
        .collect();
    if blocked.is_empty() {
        return None;
    }
    let used: Vec<_> = roots
        .iter()
        .map(|&root| !blocked.contains(&key(root)))
        .collect();
    mesh.triangles.retain(|index| used[*index as usize]);
    compact_mesh(mesh, &used);
    Some(used)
}

fn compact_mesh(mesh: &mut Mesh, used: &[bool]) {
    let mut remap = vec![0; mesh.vertices.len()];
    let mut next = 0_u32;
    for (old, &keep) in used.iter().enumerate() {
        if keep {
            remap[old] = next;
            next += 1;
        }
    }
    for index in &mut mesh.triangles {
        *index = remap[*index as usize];
    }
    compact_sidecar(&mut mesh.vertices, used);
    if mesh.normals.len() == used.len() {
        compact_sidecar(&mut mesh.normals, used);
    }
    if mesh.uv.len() == used.len() {
        compact_sidecar(&mut mesh.uv, used);
    }
}

pub(crate) fn compact_sidecar<T>(values: &mut Vec<T>, used: &[bool]) {
    let mut index = 0;
    values.retain(|_| {
        let keep = used[index];
        index += 1;
        keep
    });
}
fn find(parents: &mut [usize], mut index: usize) -> usize {
    while parents[index] != index {
        parents[index] = parents[parents[index]];
        index = parents[index];
    }
    index
}
fn join(parents: &mut [usize], left: usize, right: usize) {
    let a = find(parents, left);
    let b = find(parents, right);
    parents[a] = b;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec3;

    fn area(point: Vec3) -> DecorationClearance {
        DecorationClearance {
            centre: (point.truncate() * ISLAND_WORLD_METRES).to_array(),
            half_extents: [1., 1.],
            yaw: 0.3,
        }
    }

    fn signatures(forest: &ForestMeshes) -> [u64; 5] {
        use std::hash::{Hash, Hasher};
        [
            &forest.lod0_wood,
            &forest.lod1_wood,
            &forest.lod2_wood,
            &forest.lod0_foliage,
            &forest.lod1_foliage,
        ]
        .map(|mesh| {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            for point in mesh.vertices.iter().chain(&mesh.normals) {
                point.to_array().map(f32::to_bits).hash(&mut hash);
            }
            for uv in &mesh.uv {
                uv.to_array().map(f32::to_bits).hash(&mut hash);
            }
            mesh.triangles.hash(&mut hash);
            hash.finish()
        })
    }

    fn hybrid_fixture() -> Island {
        Island::shared_forest_fixture(crate::ForestOptions {
            noise_threshold: 0.,
            prototype_count: 1,
            hybrid: Some(crate::HybridForestOptions {
                variants_per_species: 1,
                latitude_degrees: 40.,
                altitude_cooling: 0.,
            }),
            ..crate::ForestOptions::default()
        })
    }

    fn valid_mesh(mesh: &Mesh) {
        assert!(mesh.triangles.len().is_multiple_of(3));
        assert!(
            mesh.triangles
                .iter()
                .all(|&index| (index as usize) < mesh.vertices.len())
        );
        assert!(mesh.normals.is_empty() || mesh.normals.len() == mesh.vertices.len());
        assert!(mesh.uv.is_empty() || mesh.uv.len() == mesh.vertices.len());
    }

    #[test]
    fn invalid_clearance_rejects_the_entire_request_without_mutation() {
        let mut island = hybrid_fixture();
        let signatures = signatures(&island.forest);
        let placements = island.forest.placements.len();
        let before_stats = island.forest_stats.clone();
        let valid = area(island.forest.placements[0].anchor);
        let invalid = DecorationClearance {
            half_extents: [-1., 1.],
            ..valid
        };
        assert!(island.clear_decorations(&[valid, invalid]).is_err());
        assert_eq!(signatures, self::signatures(&island.forest));
        assert_eq!(placements, island.forest.placements.len());
        assert_eq!(before_stats, island.forest_stats);
    }

    #[test]
    fn hybrid_clearance_updates_placements_trunks_cpu_occlusion_and_shadow() {
        let mut island = hybrid_fixture();
        assert!(island.forest.placements.len() > 1);
        let removed = island.forest.placements[0];
        let survivor = island.forest.placements.last().copied().unwrap();
        let survivor_collider = island.forest.trunk_colliders().last().unwrap().unwrap();
        let before = island.forest.placements.len();
        let shaded_before = island
            .forest_shadow_mask(2048)
            .unwrap()
            .pixels()
            .iter()
            .filter(|&&v| v != 0)
            .count();
        let stats = island.clear_decorations(&[area(removed.anchor)]).unwrap();
        assert_eq!(stats.trees_removed, 1);
        assert_eq!(island.forest.placements.len(), before - 1);
        assert!(
            !island
                .forest
                .placements
                .iter()
                .any(|p| p.terrain_vertex == removed.terrain_vertex)
        );
        assert!(island.forest.placements.contains(&survivor));
        assert_eq!(island.forest.lod1_foliage.vertices.len(), (before - 1) * 5);
        assert_eq!(
            island.forest.lod1_foliage.triangles.len(),
            (before - 1) * 18
        );
        assert_eq!(island.forest.trunk_colliders().count(), before - 1);
        assert_eq!(
            island.forest.trunk_colliders().last().unwrap().unwrap(),
            survivor_collider
        );
        assert!(island.forest.trunk_colliders().all(|c| c.is_ok()));
        let shaded_after = island
            .forest_shadow_mask(2048)
            .unwrap()
            .pixels()
            .iter()
            .filter(|&&v| v != 0)
            .count();
        assert!(shaded_after < shaded_before);
        valid_mesh(&island.forest.lod2_wood);
        valid_mesh(&island.forest.lod1_foliage);
        let before_repeat = signatures(&island.forest);
        assert_eq!(
            island.clear_decorations(&[area(removed.anchor)]).unwrap(),
            DecorationClearanceStats::default()
        );
        assert_eq!(before_repeat, signatures(&island.forest));
    }

    #[test]
    fn legacy_clearance_compacts_authoritative_streams_and_complete_owner_ranges() {
        let mut island = Island::shared_forest_fixture(crate::ForestOptions {
            noise_threshold: 0.,
            prototype_count: 1,
            ..crate::ForestOptions::default()
        });
        assert!(!island.forest.clusters.is_empty());
        let before = signatures(&island.forest);
        let outside = DecorationClearance {
            centre: [-200., -200.],
            half_extents: [1., 1.],
            yaw: 0.,
        };
        assert_eq!(
            island.clear_decorations(&[outside]).unwrap(),
            DecorationClearanceStats::default()
        );
        assert_eq!(before, signatures(&island.forest));
        let removed = island.forest.placements[0];
        let before_vertices = island.forest.lod0_wood.vertices.len();
        let stats = island.clear_decorations(&[area(removed.anchor)]).unwrap();
        assert!(stats.trees_removed > 0);
        assert!(island.forest.lod0_wood.vertices.len() < before_vertices);
        assert!(
            island
                .forest
                .trunk_colliders()
                .all(|collider| collider.is_ok())
        );
        for cluster in &island.forest.clusters {
            assert!(!cluster.member_tree_indices.is_empty());
            assert!(
                cluster
                    .member_tree_indices
                    .iter()
                    .all(|&i| i < island.forest.trees.len())
            );
        }
        let bounds = crate::BoundingBox {
            min: Vec3::ZERO,
            max: Vec3::ONE,
        };
        for lod in 0..3 {
            for kind in [
                crate::forest::ForestMeshKind::Wood,
                crate::forest::ForestMeshKind::Foliage,
            ] {
                let source = island.forest.mesh(kind, lod).unwrap();
                valid_mesh(source);
                let tiles = island.forest.mesh_grid(kind, lod, bounds, 2).unwrap();
                assert_eq!(
                    tiles
                        .iter()
                        .map(|tile| tile.mesh.vertices.len())
                        .sum::<usize>(),
                    source.vertices.len()
                );
                assert_eq!(
                    tiles
                        .iter()
                        .map(|tile| tile.mesh.triangles.len())
                        .sum::<usize>(),
                    source.triangles.len()
                );
            }
        }
    }

    #[test]
    fn complete_fern_clumps_and_parallel_sidecars_are_compacted_together() {
        use crate::ferns::{FernMeshTile, FernMeshes};
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.01, 0.01, 0.),
                Vec3::new(0.011, 0.01, 0.),
                Vec3::new(0.01, 0.011, 0.),
                Vec3::new(0.012, 0.01, 0.),
                Vec3::new(0.013, 0.01, 0.),
                Vec3::new(0.012, 0.011, 0.),
                Vec3::new(0.1, 0.1, 0.),
                Vec3::new(0.11, 0.1, 0.),
                Vec3::new(0.1, 0.11, 0.),
            ],
            normals: vec![Vec3::Z; 9],
            uv: vec![Vec2::ZERO; 9],
            triangles: (0..9).collect(),
        };
        let mut environment = vec![Vec2::new(0.011, 0.01); 6];
        environment.extend([Vec2::new(0.1, 0.1); 3]);
        let tile = FernMeshTile {
            mesh,
            material: (0_u16..9)
                .map(|i| crate::Vec4::splat(f32::from(i)))
                .collect(),
            environment,
        };
        // Private persisted metadata is intentionally constructed through its
        // serialization contract; the real clear helper owns all mutation.
        let mut ferns: FernMeshes = serde_json::from_value(
            serde_json::json!({"tiles":[tile],"support_vertices":[],"clump_count":2}),
        )
        .unwrap();
        let clearance = DecorationClearance {
            centre: [20., 20.],
            half_extents: [0.1, 0.1],
            yaw: 0.,
        };
        let fixture = hybrid_fixture();
        ferns.clear_settlement(&[clearance], |_| false, &fixture.terrain);
        let tile = &ferns.tiles()[0];
        assert_eq!(tile.mesh.vertices.len(), 3);
        assert_eq!(tile.material.len(), 3);
        assert_eq!(tile.environment.len(), 3);
        assert_eq!(tile.material[0], crate::Vec4::splat(6.));
        assert!(
            tile.environment
                .iter()
                .all(|&root| root == Vec2::new(0.1, 0.1))
        );
        assert_eq!(tile.mesh.triangles, [0, 1, 2]);
        assert_eq!(serde_json::to_value(&ferns).unwrap()["clump_count"], 1);
        ferns.clear_settlement(&[clearance], |_| false, &fixture.terrain);
        assert_eq!(ferns.tiles()[0].mesh.vertices.len(), 3);
    }

    #[test]
    fn removing_trees_clears_their_ground_cover_and_nearby_forest_decorations() {
        use crate::ferns::{FernMeshTile, FernMeshes};

        let mut island = hybrid_fixture();
        let tree = island.forest.placements[0];
        let clearance = area(tree.anchor);
        let removed = cleared_trees(&island.forest, &[clearance]);
        let retained = *island
            .forest
            .placements
            .iter()
            .find(|tree| !removed.contains(&tree.terrain_vertex))
            .unwrap();
        let original = forest_floor_mask(
            island.seed,
            island.terrain.mesh(),
            island.forest.placements(),
        );
        for (&floor, env) in original.iter().zip(&mut island.environment.values) {
            *env = Vec2::new(f32::from(floor), 0.375);
        }

        let log_site = log_site_keys(island.seed, tree)
            .find(|site| {
                let [x, y, end_x, end_y, radius] = site.map(f32::from_bits);
                let start = Vec2::new(x, y);
                let end = Vec2::new(end_x, end_y);
                !intersects(
                    &[clearance],
                    (start + end) * 0.5,
                    start.distance(end) * 0.5 + radius,
                )
            })
            .expect("a fallen log beyond the clearance footprint");
        let [x, y, end_x, end_y, radius] = log_site.map(f32::from_bits);
        let ranges = [MeshRange::default(); 3];
        island.forest.logs.push(
            serde_json::from_value(serde_json::json!({
                "bottom": [x, y, tree.anchor.z], "top": [end_x, end_y, tree.anchor.z],
                "radius": radius, "ranges": ranges
            }))
            .unwrap(),
        );

        let roots = [
            tree.anchor + Vec3::X * (2.5 / ISLAND_WORLD_METRES),
            retained.anchor,
        ];
        let tile = FernMeshTile {
            mesh: Mesh {
                vertices: roots
                    .into_iter()
                    .flat_map(|root| [root, root + Vec3::X * 0.0001, root + Vec3::Y * 0.0001])
                    .collect(),
                triangles: (0..6).collect(),
                ..Mesh::default()
            },
            material: vec![crate::Vec4::ZERO; 6],
            environment: roots
                .into_iter()
                .flat_map(|root| [root.truncate(); 3])
                .collect(),
        };
        assert!(!intersects(&[clearance], roots[0].truncate(), 0.0001));
        island.ferns = serde_json::from_value::<FernMeshes>(serde_json::json!({
            "tiles":[tile],"support_vertices":[],"clump_count":2
        }))
        .unwrap();

        let stats = island.clear_decorations(&[clearance]).unwrap();
        assert!(stats.trees_removed > 0);
        assert!(stats.logs_removed > 0);
        assert!(
            island
                .forest
                .logs
                .iter()
                .all(|log| log.site_key() != log_site)
        );
        assert_eq!(
            island.ferns.tiles()[0].environment,
            [retained.anchor.truncate(); 3]
        );
        assert!(original[tree.terrain_vertex as usize]);
        assert!(
            island.environment.values[tree.terrain_vertex as usize]
                .x
                .abs()
                < f32::EPSILON
        );
        assert!(
            (island.environment.values[retained.terrain_vertex as usize].x - 1.).abs()
                < f32::EPSILON
        );
        assert!(
            island
                .environment
                .values
                .iter()
                .all(|value| value.y.to_bits() == 0.375_f32.to_bits())
        );
    }

    #[test]
    fn reed_instance_offsets_and_cached_sidecars_follow_removed_owners() {
        let clump = |root| crate::reeds::ReedInstanceData {
            root,
            ground_normal: Vec3::Z,
            height_metres: 1.,
            width_metres: 0.5,
            variant: 0.2,
            tint: 0.5,
            stiffness: 0.5,
            phase: 0.3,
        };
        let instances = vec![
            clump(Vec3::new(0.01, 0.01, 0.)),
            clump(Vec3::new(0.1, 0.1, 0.)),
        ];
        let mut offsets = vec![2; 4097];
        offsets[0] = 0;
        offsets[1] = 1;
        let mut reeds: crate::reeds::ReedMeshes =
            serde_json::from_value(serde_json::json!({"instances":instances,
            "tile_offsets":offsets,"forest_exclusion_vertices":[]}))
            .unwrap();
        assert!(!reeds.tiles()[0].mesh.vertices.is_empty());
        let clearance = DecorationClearance {
            centre: [20., 20.],
            half_extents: [1., 1.],
            yaw: 0.,
        };
        reeds.clear_settlement(&[clearance]);
        assert_eq!(reeds.instances().len(), 1);
        assert_eq!(&reeds.tile_offsets()[..3], &[0, 0, 1]);
        assert_eq!(reeds.tile_offsets()[4096], 1);
        assert!(reeds.tiles()[0].mesh.vertices.is_empty());
        let tile = &reeds.tiles()[1];
        assert_eq!(tile.material.len(), tile.mesh.vertices.len());
        assert_eq!(tile.environment.len(), tile.mesh.vertices.len());
        assert!(
            tile.environment
                .iter()
                .all(|&root| root == Vec2::new(0.1, 0.1))
        );
        reeds.clear_settlement(&[clearance]);
        assert_eq!(reeds.instances().len(), 1);
    }

    #[test]
    fn cleared_logs_compact_all_three_wood_ranges_without_ghost_geometry() {
        let mut forest = ForestMeshes::default();
        for anchor in [Vec3::new(0.01, 0.01, 0.), Vec3::new(0.1, 0.1, 0.)] {
            let mut ranges = [MeshRange::default(); 3];
            for (mesh, range) in [
                &mut forest.lod0_wood,
                &mut forest.lod1_wood,
                &mut forest.lod2_wood,
            ]
            .into_iter()
            .zip(&mut ranges)
            {
                *range = MeshRange {
                    vertex_start: u32::try_from(mesh.vertices.len()).unwrap(),
                    vertex_count: 3,
                    triangle_start: u32::try_from(mesh.triangles.len()).unwrap(),
                    triangle_count: 3,
                };
                mesh.vertices
                    .extend([anchor, anchor + Vec3::X * 0.001, anchor + Vec3::Y * 0.001]);
                mesh.normals.extend([Vec3::Z; 3]);
                mesh.uv.extend([Vec2::ZERO; 3]);
                mesh.triangles.extend([
                    range.vertex_start,
                    range.vertex_start + 1,
                    range.vertex_start + 2,
                ]);
            }
            forest.logs.push(
                serde_json::from_value(serde_json::json!({"bottom":anchor-Vec3::X*0.001,
                "top":anchor+Vec3::X*0.001,"radius":0.0002,"ranges":ranges}))
                .unwrap(),
            );
        }
        let clearance = area(Vec3::new(0.01, 0.01, 0.));
        clear_forest(&mut forest, &[clearance], 0, &HashSet::new());
        assert_eq!(forest.logs.len(), 1);
        assert_eq!(forest.logs[0].anchor(), Vec3::new(0.1, 0.1, 0.));
        assert!(
            forest.logs[0]
                .ranges
                .iter()
                .all(|range| range.vertex_start == 0 && range.triangle_start == 0)
        );
        for mesh in [&forest.lod0_wood, &forest.lod1_wood, &forest.lod2_wood] {
            assert_eq!(mesh.vertices.len(), 3);
            assert_eq!(mesh.triangles, [0, 1, 2]);
            valid_mesh(mesh);
            assert!(mesh.vertices.iter().all(|point| point.x >= 0.1));
        }
        let before = signatures(&forest);
        clear_forest(&mut forest, &[clearance], 0, &HashSet::new());
        assert_eq!(before, signatures(&forest));
    }

    #[test]
    fn boulder_colliders_and_seamed_rock_components_are_removed_whole() {
        let mut island = hybrid_fixture();
        island.river_rock_mesh = Mesh {
            vertices: vec![
                Vec3::new(0.01, 0.01, 0.),
                Vec3::new(0.02, 0.01, 0.),
                Vec3::new(0.01, 0.02, 0.),
                Vec3::new(0.02, 0.01, 0.),
                Vec3::new(0.03, 0.01, 0.),
                Vec3::new(0.02, 0.02, 0.),
                Vec3::new(0.1, 0.1, 0.),
                Vec3::new(0.11, 0.1, 0.),
                Vec3::new(0.1, 0.11, 0.),
            ],
            normals: vec![Vec3::Z; 9],
            uv: vec![Vec2::ZERO; 9],
            triangles: (0..9).collect(),
        };
        island.decorations.get_mut().unwrap().boulders = vec![
            crate::Vec4::new(0.02, 0.01, 0., 0.012),
            crate::Vec4::new(0.1, 0.1, 0., 0.01),
        ];
        let clearance = DecorationClearance {
            centre: [21., 21.],
            half_extents: [2., 2.],
            yaw: 0.,
        };
        let stats = island.clear_decorations(&[clearance]).unwrap();
        assert_eq!(stats.boulders_removed, 1);
        assert_eq!(island.decorations().boulders.len(), 1);
        assert_eq!(island.river_rock_mesh.vertices.len(), 3);
        assert_eq!(island.river_rock_mesh.vertices[0], Vec3::new(0.1, 0.1, 0.));
        assert_eq!(island.river_rock_mesh.triangles, [0, 1, 2]);
        valid_mesh(&island.river_rock_mesh);
        assert_eq!(
            island.clear_decorations(&[clearance]).unwrap(),
            DecorationClearanceStats::default()
        );
    }
    #[test]
    fn rotated_clearance_checks_complete_circle_footprints() {
        let rect = DecorationClearance {
            centre: [10., 20.],
            half_extents: [4., 1.],
            yaw: std::f32::consts::FRAC_PI_2,
        };
        assert!(rect.intersects_circle(Vec2::new(10., 24.5), 0.6));
        assert!(!rect.intersects_circle(Vec2::new(12., 20.), 0.5));
    }
    #[test]
    fn contacting_component_is_removed_whole_and_unrelated_geometry_compacts() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.01, 0.01, 0.),
                Vec3::new(0.02, 0.01, 0.),
                Vec3::new(0.02, 0.02, 0.),
                Vec3::new(0.1, 0.1, 0.),
                Vec3::new(0.11, 0.1, 0.),
                Vec3::new(0.1, 0.11, 0.),
            ],
            triangles: vec![0, 1, 2, 3, 4, 5],
            ..Mesh::default()
        };
        retain_mesh_components(
            &mut mesh,
            &[DecorationClearance {
                centre: [21., 21.],
                half_extents: [2., 2.],
                yaw: 0.,
            }],
        );
        assert_eq!(mesh.triangles, [0, 1, 2]);
        assert_eq!(mesh.vertices.len(), 3);
    }
}
