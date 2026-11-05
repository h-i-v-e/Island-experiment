//! Shared Planet-style individual crowns at every distance; only collider
//! trunks and small CPU occlusion proxies are baked per placement.
use super::{
    FOREST_HABIT_DOMAIN, ForestMeshes, ForestOptions, ForestTreeRanges, ISLAND_WORLD_METRES,
    Terrain, TreeMeshes, TreePlacement, append_transformed_to_terrain, stable_key,
};

/// Climate controls for the shared-prototype forest.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct HybridForestOptions {
    pub latitude_degrees: f32,
    /// Temperature decrease from sea level to the authored forest snowline.
    pub altitude_cooling: f32,
    pub variants_per_species: u32,
}
const _: () = assert!(size_of::<HybridForestOptions>() == 12);

impl HybridForestOptions {
    pub(super) fn validate(self) -> Result<(), String> {
        if !self.latitude_degrees.is_finite()
            || self.latitude_degrees.abs() > 90.0
            || !self.altitude_cooling.is_finite()
            || !(0.0..=40.0).contains(&self.altitude_cooling)
            || !(1..=8).contains(&self.variants_per_species)
        {
            return Err(
                "hybrid forest requires latitude -90..90, cooling 0..40 and 1..8 variants".into(),
            );
        }
        Ok(())
    }
    fn temperature(self, altitude: f32, snowline: f32) -> f32 {
        20.0 - 30.0 * self.latitude_degrees.to_radians().sin().powi(2)
            - altitude.max(0.0) / snowline * self.altitude_cooling
    }

    fn conifer_weight(self, altitude: f32, snowline: f32) -> f32 {
        smooth_unit((14.0 - self.temperature(altitude, snowline)) / 12.0)
    }

    fn palm_weight(self, altitude: f32, snowline: f32) -> f32 {
        let warmth = smooth_unit((self.temperature(altitude, snowline) - 12.0) / 8.0);
        let lowland = 1.0 - smooth_unit((altitude - 40.0) / 120.0);
        let tropical =
            1.0 - smooth_unit((self.latitude_degrees.to_radians().sin().powi(2) - 0.08) / 0.42);
        0.72 * warmth * lowland * tropical
    }

    fn species_offset(self, choice: f32, altitude: f32, snowline: f32) -> u8 {
        let conifer = self.conifer_weight(altitude, snowline);
        if choice < conifer {
            1
        } else if choice < conifer + (1.0 - conifer) * self.palm_weight(altitude, snowline) {
            2
        } else {
            0
        }
    }
}

fn smooth_unit(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HybridForest {
    pub(crate) prototypes: std::sync::Arc<[TreeMeshes]>,
    pub(crate) broadleaf_count: usize,
}

/// Shared geometry and shader-data contract, also embedded in generated snapshots.
pub(crate) const CATALOG_VERSION: u32 = 6;

#[derive(serde::Serialize, serde::Deserialize)]
struct CatalogMetadata {
    catalog_version: u32,
    variants_per_species: u8,
}

impl serde::Serialize for HybridForest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CatalogMetadata {
            catalog_version: CATALOG_VERSION,
            variants_per_species: u8::try_from(self.broadleaf_count)
                .map_err(serde::ser::Error::custom)?,
        }
        .serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for HybridForest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let metadata = CatalogMetadata::deserialize(deserializer)?;
        if metadata.catalog_version != CATALOG_VERSION {
            return Err(serde::de::Error::custom(format_args!(
                "unsupported forest catalog version {} (expected {CATALOG_VERSION})",
                metadata.catalog_version
            )));
        }
        if !(1..=8).contains(&metadata.variants_per_species) {
            return Err(serde::de::Error::custom(
                "forest catalog requires 1..8 variants per species",
            ));
        }
        Ok(Self {
            prototypes: shared_prototypes(metadata.variants_per_species),
            broadleaf_count: usize::from(metadata.variants_per_species),
        })
    }
}

// Fixed Planet seeds make both native and GPU prototypes reusable across islands.
const BROADLEAF_SEEDS: [u64; 8] = [2018, 42, 1234, 98765, 56789, 137, 2718, 31415];
static CATALOGS: [std::sync::OnceLock<std::sync::Arc<[TreeMeshes]>>; 8] =
    [const { std::sync::OnceLock::new() }; 8];
fn shared_prototypes(count: u8) -> std::sync::Arc<[TreeMeshes]> {
    std::sync::Arc::clone(CATALOGS[usize::from(count - 1)].get_or_init(|| {
        let mut trees: Vec<_> = BROADLEAF_SEEDS[..usize::from(count)]
            .iter()
            .map(|&seed| {
                let mut tree = crate::generate_tree(seed);
                tree.lod2_foliage = crate::broadleaf_crown(&tree.lod1_foliage);
                tree
            })
            .collect();
        trees.extend((0..usize::from(count)).map(crate::generate_conifer));
        trees.extend((0..usize::from(count)).map(crate::generate_palm));
        trees.into()
    }))
}

pub(super) fn assemble(
    seed: u64,
    placements: &mut [TreePlacement],
    terrain: &Terrain,
    options: ForestOptions,
    climate: HybridForestOptions,
) -> Result<ForestMeshes, String> {
    let count = climate.variants_per_species as u8;
    let prototypes = shared_prototypes(count);
    for tree in placements.iter_mut() {
        let sample = stable_key(seed, u64::from(tree.seed_ordinal), FOREST_HABIT_DOMAIN);
        let choice = (sample >> 40) as f32 / (1_u32 << 24) as f32;
        let species = climate.species_offset(
            choice,
            tree.anchor.z * ISLAND_WORLD_METRES,
            options.snowline_metres,
        );
        tree.prototype = tree.prototype % count + species * count;
        bury_root(tree, &prototypes[usize::from(tree.prototype)], terrain);
    }
    let mut out = ForestMeshes {
        placements: placements.to_vec(),
        ..ForestMeshes::default()
    };
    for tree in placements.iter().copied() {
        let range = append_transformed_to_terrain(
            &mut out.lod2_wood,
            &prototypes[usize::from(tree.prototype)].lod2_wood,
            tree,
            terrain,
        )?;
        out.trees.push(ForestTreeRanges {
            terrain_vertex: tree.terrain_vertex,
            anchor: tree.anchor,
            prototype: tree.prototype,
            lod2_wood: range,
            ..ForestTreeRanges::default()
        });
    }
    out.lod2_wood.calculate_normals();
    // Five vertices per tree retain CPU ground occlusion. No render clusters
    // are exported: individual crowns remain visible at every distance.
    let proxies: Vec<_> = prototypes
        .iter()
        .map(|tree| {
            crown_occluder(if tree.lod2_foliage.vertices.is_empty() {
                &tree.lod1_foliage
            } else {
                &tree.lod2_foliage
            })
        })
        .collect();
    for tree in placements.iter().copied() {
        super::MeshAppender {
            destination: &mut out.lod1_foliage,
            anchor: tree.anchor,
            yaw_radians: tree.yaw_radians,
            scale: tree.scale,
            transform_uv_as_axis: false,
            terrain: None,
        }
        .append(&proxies[usize::from(tree.prototype)])?;
    }
    out.hybrid = Some(HybridForest {
        prototypes,
        broadleaf_count: usize::from(count),
    });
    Ok(out)
}

fn bury_root(tree: &mut TreePlacement, prototype: &TreeMeshes, terrain: &Terrain) {
    let (sin, cos) = tree.yaw_radians.sin_cos();
    let lowest = prototype
        .lod0_wood
        .vertices
        .iter()
        .chain(&prototype.lod1_wood.vertices)
        .chain(&prototype.lod2_wood.vertices)
        .filter(|position| position.z.abs() <= f32::EPSILON)
        .map(|&position| {
            let point = super::transform_position(position, tree.anchor, tree.scale, sin, cos);
            terrain.sample(point.x, point.y)
        })
        .fold(terrain.sample(tree.anchor.x, tree.anchor.y), f32::min);
    // The entire base rim must be below the downhill side, not just its centre.
    tree.anchor.z = lowest - 0.08 / ISLAND_WORLD_METRES;
}

fn crown_occluder(source: &crate::Mesh) -> crate::Mesh {
    let minimum = source
        .vertices
        .iter()
        .copied()
        .fold(crate::Vec3::splat(f32::INFINITY), crate::Vec3::min);
    let maximum = source
        .vertices
        .iter()
        .copied()
        .fold(crate::Vec3::splat(f32::NEG_INFINITY), crate::Vec3::max);
    let centre = (minimum + maximum) * 0.5;
    let mut mesh = crate::Mesh {
        vertices: vec![
            crate::Vec3::new(minimum.x, centre.y, minimum.z),
            crate::Vec3::new(centre.x, minimum.y, minimum.z),
            crate::Vec3::new(maximum.x, centre.y, minimum.z),
            crate::Vec3::new(centre.x, maximum.y, minimum.z),
            crate::Vec3::new(centre.x, centre.y, maximum.z),
        ],
        triangles: vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4, 0, 3, 2, 0, 2, 1],
        ..crate::Mesh::default()
    };
    mesh.calculate_normals();
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burial_covers_the_complete_rim_on_slopes_and_keeps_pine_and_palm_walking_clearance() {
        let centre = crate::Vec3::new(0.5, 0.5, 0.05);
        for slope in [-0.4, 0.0, 0.4] {
            let mut mesh = crate::Mesh {
                vertices: [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
                    .map(|(x, y)| crate::Vec3::new(x, y, centre.z + (x - 0.5) * slope))
                    .to_vec(),
                triangles: vec![0, 1, 2, 1, 3, 2],
                ..crate::Mesh::default()
            };
            mesh.calculate_normals();
            let terrain = Terrain::new(mesh);
            let prototypes = shared_prototypes(8);
            for (index, prototype) in prototypes.iter().enumerate() {
                for scale in [0.7, 1.0, 2.0] {
                    for yaw in [0.0_f32, 0.9, 2.4] {
                        let mut tree = TreePlacement {
                            seed_ordinal: 0,
                            terrain_vertex: 0,
                            anchor: centre,
                            yaw_radians: yaw,
                            scale,
                            prototype: u8::try_from(index).unwrap(),
                        };
                        bury_root(&mut tree, prototype, &terrain);
                        let first_anchor = tree.anchor;
                        bury_root(&mut tree, prototype, &terrain);
                        assert_eq!(tree.anchor, first_anchor, "burial must not accumulate");
                        let (sin, cos) = yaw.sin_cos();
                        for mesh in [
                            &prototype.lod0_wood,
                            &prototype.lod1_wood,
                            &prototype.lod2_wood,
                        ] {
                            for &point in mesh.vertices.iter().filter(|p| p.z.abs() <= f32::EPSILON)
                            {
                                let placed = super::super::transform_position(
                                    point,
                                    tree.anchor,
                                    scale,
                                    sin,
                                    cos,
                                );
                                assert!(placed.z < terrain.sample(placed.x, placed.y));
                            }
                        }
                        if index >= 8 {
                            let shells = if index < 16 {
                                crate::tree_shells(&prototype.lod0_foliage, true).mesh
                            } else {
                                crate::Mesh::default()
                            };
                            for p in prototype
                                .lod0_foliage
                                .vertices
                                .iter()
                                .chain(&shells.vertices)
                                .chain(&prototype.lod0_twigs.vertices)
                            {
                                let height =
                                    (tree.anchor.z - centre.z + p.z * scale) * ISLAND_WORLD_METRES;
                                assert!(
                                    height >= 2.0,
                                    "tree {index}, scale {scale}, height {height}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn palms_mix_with_broadleaf_only_in_warm_tropical_lowlands() {
        let tropical = HybridForestOptions {
            latitude_degrees: 0.0,
            altitude_cooling: 22.0,
            variants_per_species: 8,
        };
        assert_eq!(size_of::<HybridForestOptions>(), 12);
        assert_eq!(shared_prototypes(8).len(), 24);
        assert_eq!(tropical.palm_weight(0.0, 200.0), 0.72);
        assert!(tropical.palm_weight(40.0, 200.0) > tropical.palm_weight(70.0, 200.0));
        assert!(tropical.palm_weight(70.0, 200.0) > tropical.palm_weight(100.0, 200.0));
        let no_cooling = HybridForestOptions {
            altitude_cooling: 0.0,
            ..tropical
        };
        assert_eq!(no_cooling.palm_weight(160.0, 200.0), 0.0);
        for latitude_degrees in [-90.0, -60.0, 60.0, 90.0] {
            let cold = HybridForestOptions {
                latitude_degrees,
                ..tropical
            };
            assert_eq!(cold.palm_weight(0.0, 200.0), 0.0);
            assert!((0..1000).all(|i| cold.species_offset(i as f32 / 1000.0, 0.0, 200.0) != 2));
        }
        let mut counts = [0; 3];
        for i in 0..1000 {
            counts[usize::from(tropical.species_offset(i as f32 / 1000.0, 0.0, 200.0))] += 1;
        }
        assert_eq!(counts, [280, 0, 720]);
        assert_eq!(tropical.species_offset(0.5, 190.0, 200.0), 1);
        assert_eq!(
            tropical.palm_weight(0.0, 200.0),
            HybridForestOptions {
                latitude_degrees: -tropical.latitude_degrees,
                ..tropical
            }
            .palm_weight(0.0, 200.0)
        );
    }

    #[test]
    fn climate_increases_conifers_with_altitude_and_latitude() {
        let warm = HybridForestOptions {
            latitude_degrees: 0.0,
            altitude_cooling: 22.0,
            variants_per_species: 4,
        };
        let polar = HybridForestOptions {
            latitude_degrees: 70.0,
            ..warm
        };
        assert!(warm.conifer_weight(90.0, 100.0) > warm.conifer_weight(0.0, 100.0));
        assert!(polar.conifer_weight(0.0, 100.0) > warm.conifer_weight(0.0, 100.0));
        assert!(warm.validate().is_ok());
        assert!(
            HybridForestOptions {
                latitude_degrees: f32::NAN,
                ..warm
            }
            .validate()
            .is_err()
        );
    }
}

#[cfg(test)]
mod assembly_tests {
    use super::*;
    use crate::forest::ForestMeshKind;
    use crate::{BoundingBox, Mesh, Vec3};

    #[test]
    fn catalog_snapshot_contains_only_version_and_count_and_reuses_all_three_species() {
        let forest = HybridForest {
            prototypes: shared_prototypes(2),
            broadleaf_count: 2,
        };
        let encoded = bincode::serialize(&forest).unwrap();
        assert_eq!(
            encoded.len(),
            5,
            "shared tree geometry must never enter snapshots"
        );
        let restored: HybridForest = bincode::deserialize(&encoded).unwrap();
        assert_eq!(restored.broadleaf_count, 2);
        assert_eq!(restored.prototypes.len(), 6);
        assert!(std::sync::Arc::ptr_eq(
            &forest.prototypes,
            &restored.prototypes
        ));
        for prototype in restored.prototypes.iter() {
            assert!(!prototype.lod0_wood.vertices.is_empty());
            assert_eq!(
                prototype.lod0_wood.uv.len(),
                prototype.lod0_wood.vertices.len()
            );
            assert!(prototype.lod0_wood.uv.iter().all(|uv| uv.is_finite()));
            assert!(!prototype.lod0_foliage.vertices.is_empty());
        }
    }

    #[test]
    fn unsupported_catalog_versions_and_counts_fail_before_restoring_geometry() {
        for (catalog_version, variants_per_species) in [
            (CATALOG_VERSION - 1, 1),
            (CATALOG_VERSION + 1, 1),
            (CATALOG_VERSION, 0),
            (CATALOG_VERSION, 9),
        ] {
            let encoded = bincode::serialize(&CatalogMetadata {
                catalog_version,
                variants_per_species,
            })
            .unwrap();
            let error = bincode::deserialize::<HybridForest>(&encoded).unwrap_err();
            assert!(error.to_string().contains("forest catalog"));
        }
    }

    #[test]
    fn hybrid_retains_shared_geometry_and_colliders_without_baking_live_draw_meshes() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.02),
                Vec3::new(1.0, 0.0, 0.02),
                Vec3::new(0.0, 1.0, 0.02),
                Vec3::new(1.0, 1.0, 0.02),
            ],
            triangles: vec![0, 1, 2, 1, 3, 2],
            ..Mesh::default()
        };
        mesh.calculate_normals();
        let terrain = Terrain::new(mesh);
        let initial: Vec<_> = (0..12)
            .map(|i| TreePlacement {
                seed_ordinal: i,
                terrain_vertex: 0,
                anchor: Vec3::new(
                    0.3 + (i % 4) as f32 * 0.006,
                    0.3 + (i / 4) as f32 * 0.006,
                    0.02,
                ),
                yaw_radians: i as f32 * 0.37,
                scale: 1.0,
                prototype: 0,
            })
            .collect();
        let options = ForestOptions {
            prototype_count: 1,
            fallen_log_density: 0.0,
            ..ForestOptions::default()
        };
        for latitude_degrees in [0.0, 70.0] {
            let climate = HybridForestOptions {
                latitude_degrees,
                altitude_cooling: 0.0,
                variants_per_species: 1,
            };
            let mut placements = initial.clone();
            let forest = assemble(42, &mut placements, &terrain, options, climate).unwrap();
            let again = assemble(42, &mut initial.clone(), &terrain, options, climate).unwrap();
            assert_eq!(forest, again);
            assert!(std::sync::Arc::ptr_eq(
                &forest.hybrid.as_ref().unwrap().prototypes,
                &again.hybrid.as_ref().unwrap().prototypes
            ));
            let other = assemble(77, &mut initial.clone(), &terrain, options, climate).unwrap();
            assert!(std::sync::Arc::ptr_eq(
                &forest.hybrid.as_ref().unwrap().prototypes,
                &other.hybrid.as_ref().unwrap().prototypes
            ));
            assert_eq!(forest.hybrid.as_ref().unwrap().prototypes.len(), 3);
            assert!(forest.lod0_wood.vertices.is_empty());
            assert!(forest.lod1_wood.vertices.is_empty());
            assert!(forest.lod0_foliage.vertices.is_empty());
            assert_eq!(forest.lod2_wood.vertices.len(), initial.len() * 8);
            assert!(forest.trunk_colliders().all(|c| c.is_ok()));
            let tiles = forest
                .mesh_grid(
                    ForestMeshKind::Wood,
                    2,
                    BoundingBox {
                        min: Vec3::ZERO,
                        max: Vec3::ONE,
                    },
                    8,
                )
                .unwrap();
            assert!(tiles.iter().all(|t| t.mesh.vertices.is_empty()));
            if latitude_degrees > 60.0 {
                assert!(placements.iter().all(|p| p.prototype == 1));
                assert!(forest.clusters.is_empty());
                assert_eq!(
                    forest.lod1_foliage.vertices.len(),
                    initial.len() * 5,
                    "Tiny pine proxies retain baked ground occlusion without render clusters"
                );
            } else {
                assert!(placements.iter().all(|p| p.prototype != 1));
                assert!(placements.iter().any(|p| p.prototype == 2));
                assert!(placements.iter().any(|p| p.prototype == 0));
                assert!(forest.clusters.is_empty(), "No merged canopy handover");
                assert_eq!(forest.lod1_foliage.vertices.len(), initial.len() * 5);
                assert!(
                    !forest.hybrid.as_ref().unwrap().prototypes[0]
                        .lod2_foliage
                        .vertices
                        .is_empty()
                );
            }
        }
    }
}
