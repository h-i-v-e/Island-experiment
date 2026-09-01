#[cfg(feature = "gpu-generation")]
use super::GpuParticleErosionScratch;
use super::coastal_uplift;
use super::erosion::{projected_face_area, projected_face_area_with_vertex};
use super::lod::regenerate_lods;
use super::{
    Adjacency, BinaryHeap, BoundingBox, DETAIL_DISPLACEMENT_RATIO, Decorations, File,
    GenerationMethod, GeologyField, HashSet, HydraulicScratch, ISLAND_WORLD_METRES,
    IndexedParallelIterator, IslandOptions, Mesh, MeshClipper, OnceLock, Ordering,
    ParallelIterator, ParallelSliceMut, Path, Raster, Read, River, RiverChannelSettings,
    RiverNetwork, RiverSourceRule, Rng, SHARP_ROCK_DISPLACEMENT_RATIO, StageTimer, SurfaceMaps,
    SurfaceMaterial, TERRAIN_RENDER_FLOOR, Terrain, TerrainEnvironmentField, TerrainMaterialField,
    Vec2, Vec3, Vec4, Write, append_settled_rocks, bake_surface_maps, bury_river_banks,
    encode_bank_distance_in_uv, erode_mesh, fix_inland_seas, geology, hydraulic_erode_stage,
    hydraulic_erode_stage_depositing_across_sea, io, legacy_catchment_hectares, mem, noise,
    sample_grid,
};
use crate::ferns::{FernMeshTile, FernMeshes, FernOptions, FernSurface, generate_ferns};
use crate::forest::{
    ForestGenerationStats, ForestMeshKind, ForestMeshes, ForestOptions, forest_floor_mask,
    generate_forest,
};
use crate::reeds::{ReedMeshTile, ReedMeshes, ReedOptions, ReedSurface, generate_reeds};
use crate::rivers::WaterfallFoot;

const SEA_PROXIMITY_FULL_STRENGTH_METRES: f32 = 2.0;
const SEA_PROXIMITY_ZERO_STRENGTH_METRES: f32 = 20.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Island {
    pub(super) seed: u64,
    pub(super) options: IslandOptions,
    pub(super) generation_method: GenerationMethod,
    pub(super) terrain: Terrain,
    pub(super) material: TerrainMaterialField,
    pub(super) environment: TerrainEnvironmentField,
    pub(super) coarser_lods: [Mesh; 2],
    pub(super) rivers: Vec<River>,
    pub(super) distance_to_land: Vec<f32>,
    pub(super) river_mesh: Mesh,
    pub(super) river_rock_mesh: Mesh,
    pub(super) waterfall_feet: Vec<WaterfallFoot>,
    pub(super) reeds: ReedMeshes,
    pub(super) ferns: FernMeshes,
    pub(super) forest: ForestMeshes,
    pub(super) forest_stats: ForestGenerationStats,
    pub(super) forest_options: ForestOptions,
    pub(super) decorations: OnceLock<Decorations>,
}

pub(super) struct FinalRiverGeneration {
    pub(super) lod0: Mesh,
    pub(super) material: SurfaceMaterial,
    pub(super) rivers: Vec<River>,
    pub(super) river_mesh: Mesh,
    pub(super) river_bed: Vec<bool>,
    pub(super) river_rock_mesh: Mesh,
    pub(super) waterfall_feet: Vec<WaterfallFoot>,
}

struct SavedIslandReader<R> {
    source: R,
    version: u8,
}

impl<R: Read> SavedIslandReader<R> {
    fn new(mut source: R) -> io::Result<Self> {
        let mut magic = [0_u8; 8];
        source.read_exact(&mut magic)?;
        if &magic[..7] != b"MOTURS\0" || !matches!(magic[7], 3..=18) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid Motu Rust free-form mesh file",
            ));
        }
        Ok(Self {
            source,
            version: magic[7],
        })
    }

    fn read_options(&mut self) -> io::Result<(IslandOptions, ForestOptions)> {
        let mut options = self.read_terrain_options()?;
        self.read_river_options(&mut options)?;
        if self.version == 8 {
            self.discard_f32()?;
        }
        options.terrain_size = self.read_u32()?;
        let forest_options = if self.version >= 17 {
            ForestOptions {
                patch_size_metres: self.read_f32()?,
                noise_threshold: self.read_f32()?,
                noise_octaves: self.read_u8()?,
                snowline_metres: self.read_f32()?,
                prototype_count: self.read_u8()?,
                minimum_scale: self.read_f32()?,
                maximum_scale: self.read_f32()?,
            }
        } else {
            ForestOptions::default()
        };
        Ok((options, forest_options))
    }

    fn read_generation_method(&mut self) -> io::Result<GenerationMethod> {
        if self.version < 18 {
            return Ok(GenerationMethod::Cpu);
        }
        let tag = self.read_u8()?;
        GenerationMethod::from_tag(tag).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown generation method tag {tag}"),
            )
        })
    }

    fn read_terrain_options(&mut self) -> io::Result<IslandOptions> {
        let max_height = self.read_f32()?;
        let water_ratio = self.read_f32()?;
        let slope_multiplier = self.read_f32()?;
        let coastal_slope_multiplier = self.read_f32()?;
        if self.version <= 9 {
            self.discard_f32()?;
        }
        if (7..=11).contains(&self.version) {
            self.discard_f32()?;
            self.discard_f32()?;
        }

        let defaults = IslandOptions::default();
        let hydraulic_erosion_strength =
            self.read_f32_since(4, defaults.hydraulic_erosion_strength)?;
        let hydraulic_deposition_strength =
            self.read_f32_since(6, defaults.hydraulic_deposition_strength)?;
        let hydraulic_deposition_slope_degrees =
            self.read_f32_since(6, defaults.hydraulic_deposition_slope_degrees)?;
        Ok(IslandOptions {
            max_height,
            water_ratio,
            slope_multiplier,
            coastal_slope_multiplier,
            hydraulic_erosion_strength,
            hydraulic_deposition_strength,
            hydraulic_deposition_slope_degrees,
            ..defaults
        })
    }

    fn read_river_options(&mut self, options: &mut IslandOptions) -> io::Result<()> {
        if self.version >= 11 {
            let stored_catchment = self.read_f32()?;
            options.river_source_catchment_hectares = if self.version >= 14 {
                stored_catchment
            } else {
                legacy_catchment_hectares(stored_catchment, options.water_ratio)
            };
            options.river_source_steep_multiplier = self.read_f32()?;
        } else if self.version >= 5 {
            for _ in 0..5 {
                self.discard_f32()?;
            }
        }
        if self.version >= 13 {
            let stored_elevation_parameter = self.read_f32()?;
            if self.version >= 15 {
                options.river_source_elevation_boost = stored_elevation_parameter;
            }
        }
        if self.version >= 16 {
            options.river_source_width_metres = self.read_f32()?;
            options.river_maximum_width_metres = self.read_f32()?;
            options.river_source_depth_metres = self.read_f32()?;
            options.river_maximum_depth_metres = self.read_f32()?;
        }
        Ok(())
    }

    fn read_f32_since(&mut self, version: u8, default: f32) -> io::Result<f32> {
        if self.version >= version {
            self.read_f32()
        } else {
            Ok(default)
        }
    }

    fn discard_f32(&mut self) -> io::Result<()> {
        self.read_f32().map(drop)
    }

    fn read_u64(&mut self) -> io::Result<u64> {
        let mut bytes = [0_u8; 8];
        self.source.read_exact(&mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        let mut bytes = [0_u8; 4];
        self.source.read_exact(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        let mut byte = [0_u8; 1];
        self.source.read_exact(&mut byte)?;
        Ok(byte[0])
    }

    fn read_f32(&mut self) -> io::Result<f32> {
        let mut bytes = [0_u8; 4];
        self.source.read_exact(&mut bytes)?;
        Ok(f32::from_le_bytes(bytes))
    }
}

pub(super) fn generate_final_rivers(
    seed: u64,
    lod0: &Mesh,
    material: &SurfaceMaterial,
    source_rule: RiverSourceRule,
    channel_settings: RiverChannelSettings,
) -> Result<FinalRiverGeneration, String> {
    let _timer = StageTimer::new("rivers.final");
    let mut prepared_lod0 = lod0.clone();
    let detail_adjacency = prepared_lod0.adjacency();
    let ocean = fix_inland_seas(&mut prepared_lod0, &detail_adjacency);
    let mut prepared_material = material.clone();
    prepared_material.set_sea_proximity(sea_proximity_strengths(
        &prepared_lod0,
        &detail_adjacency,
        &ocean,
    ));
    let mut rejected_waterfall_vertices = HashSet::new();
    loop {
        let mut attempt_lod0 = prepared_lod0.clone();
        let mut attempt_material = prepared_material.clone();
        let mut network = RiverNetwork::generate_with_ocean(
            &mut attempt_lod0,
            &detail_adjacency,
            source_rule,
            ocean.clone(),
            seed,
        );
        network.shape_with_settings_and_waterfall_rejections(
            &mut attempt_lod0,
            &detail_adjacency,
            &mut attempt_material,
            true,
            false,
            channel_settings,
            &rejected_waterfall_vertices,
        );
        let parts = network.into_parts_with_waterfall_failures(
            &mut attempt_lod0,
            &mut attempt_material,
            seed,
        );
        let rejected_before = rejected_waterfall_vertices.len();
        rejected_waterfall_vertices.extend(parts.failed_waterfalls);
        if rejected_waterfall_vertices.len() == rejected_before {
            return Ok(FinalRiverGeneration {
                lod0: attempt_lod0,
                material: attempt_material,
                rivers: parts.rivers,
                river_mesh: parts.river_mesh,
                river_bed: parts.river_bed,
                river_rock_mesh: parts.river_rock_mesh,
                waterfall_feet: parts.waterfall_feet,
            });
        }
    }
}

impl Island {
    /// Generates an island and all derived assets.
    ///
    /// # Errors
    ///
    /// Returns an error when an option is non-finite or outside its supported
    /// range.
    pub fn generate(seed: u64, options: IslandOptions) -> Result<Self, String> {
        Self::generate_with_forest(seed, options, ForestOptions::default())
    }

    /// Generates an island with an explicit CPU or GPU implementation.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested method is unavailable, an option is
    /// invalid, or the selected implementation cannot complete generation.
    pub fn generate_with_method(
        seed: u64,
        options: IslandOptions,
        method: GenerationMethod,
    ) -> Result<Self, String> {
        Self::generate_with_forest_and_method(seed, options, ForestOptions::default(), method)
    }

    /// Generates an island with explicit deterministic forest controls.
    ///
    /// Keeping the forest value separate from the historical native terrain
    /// option block lets callers tune forest coverage without changing the
    /// native terrain fields.  It is owned by the generated island and is
    /// included in the versioned save format.
    ///
    /// # Errors
    ///
    /// Returns an error when either terrain or forest options are invalid, or
    /// when final mesh data cannot satisfy the forest ownership contracts.
    pub fn generate_with_forest(
        seed: u64,
        options: IslandOptions,
        forest_options: ForestOptions,
    ) -> Result<Self, String> {
        Self::generate_with_forest_and_method(seed, options, forest_options, GenerationMethod::Cpu)
    }

    /// Generates an island with explicit forest controls and implementation.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested method is unavailable, either
    /// option block is invalid, or the selected implementation cannot
    /// complete generation.
    pub fn generate_with_forest_and_method(
        seed: u64,
        options: IslandOptions,
        forest_options: ForestOptions,
        method: GenerationMethod,
    ) -> Result<Self, String> {
        Self::generate_with_forest_reeds_and_method(
            seed,
            options,
            forest_options,
            ReedOptions::default(),
            method,
        )
    }

    /// Generates an island with explicit forest and riverbank vegetation controls.
    ///
    /// # Errors
    ///
    /// Returns an error when any option block is invalid or generation fails.
    pub fn generate_with_forest_and_reeds(
        seed: u64,
        options: IslandOptions,
        forest_options: ForestOptions,
        reed_options: ReedOptions,
    ) -> Result<Self, String> {
        Self::generate_with_forest_reeds_and_method(
            seed,
            options,
            forest_options,
            reed_options,
            GenerationMethod::Cpu,
        )
    }

    /// Generates an island with explicit forest, riverbank vegetation, and
    /// tree-trunk fern controls.
    ///
    /// # Errors
    ///
    /// Returns an error when any option block is invalid or generation fails.
    pub fn generate_with_forest_reeds_and_ferns(
        seed: u64,
        options: IslandOptions,
        forest_options: ForestOptions,
        reed_options: ReedOptions,
        fern_options: FernOptions,
    ) -> Result<Self, String> {
        Self::generate_with_forest_reeds_ferns_and_method(
            seed,
            options,
            forest_options,
            reed_options,
            fern_options,
            GenerationMethod::Cpu,
        )
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn generate_with_forest_reeds_and_method(
        seed: u64,
        options: IslandOptions,
        forest_options: ForestOptions,
        reed_options: ReedOptions,
        method: GenerationMethod,
    ) -> Result<Self, String> {
        Self::generate_with_forest_reeds_ferns_and_method(
            seed,
            options,
            forest_options,
            reed_options,
            FernOptions::default(),
            method,
        )
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn generate_with_forest_reeds_ferns_and_method(
        seed: u64,
        options: IslandOptions,
        forest_options: ForestOptions,
        reed_options: ReedOptions,
        fern_options: FernOptions,
        method: GenerationMethod,
    ) -> Result<Self, String> {
        method.require_available()?;
        let _timer = StageTimer::new("island.generate");
        let options = options.validate()?;
        let forest_options = forest_options.validate()?;
        let reed_options = reed_options.validate()?;
        let fern_options = fern_options.validate()?;
        let mut scratch = GenerationScratch::new(method);
        let (base, material) = generate_base(seed, options, &mut scratch)?;
        let context = GenerationContext::new(seed, options);
        let (mut lod2, material) = generate_lod2(&base, material, context, &mut scratch)?;
        let (lod1, material) = generate_first_lod1(&lod2, material, context, &mut scratch)?;
        let (mut lod1, material) = refine_lod1_again(&lod1, material, context, &mut scratch)?;
        let (lod0, material) = generate_broad_lod0(&lod1, material, context, &mut scratch)?;
        let (lod0, material) = generate_detail_lod0(lod0, material, context, &mut scratch)?;
        let FinalRiverGeneration {
            mut lod0,
            mut material,
            rivers,
            mut river_mesh,
            river_bed,
            mut river_rock_mesh,
            waterfall_feet,
        } = generate_final_rivers(
            seed,
            &lod0,
            &material,
            context.river_source_rule,
            options.river_channel_settings(),
        )?;
        optimize_finished_terrain_surface(&mut lod0, &mut material, &river_bed);
        let lod0_index = {
            let _timer = StageTimer::new("lod.simplify");
            regenerate_lods(&mut lod0, &mut lod1, &mut lod2)
        };
        bury_river_banks(&mut river_mesh, &lod0, &lod0_index);
        river_mesh = river_mesh.clipped_above(0.0);
        encode_bank_distance_in_uv(&mut river_mesh);
        let forced_rock = sharp_rock_mask(&lod0);
        let terrain = {
            let _timer = StageTimer::new("terrain.index");
            Terrain::with_index(lod0, lod0_index)
        };
        let (mut decorations, settled_rocks) = Decorations::generate(
            seed,
            &terrain,
            &rivers,
            options.terrain_size as usize * 4,
            method,
        )?;
        append_settled_rocks(seed, &settled_rocks, &mut river_rock_mesh);
        let reeds = generate_reeds(
            seed,
            terrain.mesh(),
            ReedSurface {
                river_bed: &river_bed,
                deposited_depths: material.depths(),
                sea_proximity: material.sea_proximities(),
                forced_rock: &forced_rock,
                stones: decorations.stone_vertices(),
                waterfall_feet: &waterfall_feet,
            },
            reed_options,
        )?;
        let (forest, forest_stats) = generate_forest(
            seed,
            &terrain,
            crate::forest::ForestSurface {
                river_bed: &river_bed,
                stones: decorations.stone_vertices(),
                reeds: reeds.forest_exclusion_vertices(),
                deposited_depths: material.depths(),
                sea_proximity: material.sea_proximities(),
            },
            forest_options,
        )?;
        decorations.set_tree_anchors(forest.placements().iter().map(|placement| placement.anchor));
        let ferns = generate_ferns(
            seed,
            &terrain,
            &forest,
            FernSurface {
                river_bed: &river_bed,
                deposited_depths: material.depths(),
                sea_proximity: material.sea_proximities(),
                forced_rock: &forced_rock,
                stones: decorations.stone_vertices(),
                reeds: reeds.forest_exclusion_vertices(),
                snowline_metres: forest_options.snowline_metres,
            },
            fern_options,
        )?;
        let mut forest_floor = forest_floor_mask(seed, terrain.mesh(), forest.placements());
        for &vertex in ferns.support_vertices() {
            if let Some(value) = forest_floor.get_mut(vertex as usize) {
                *value = true;
            }
        }
        let environment =
            TerrainEnvironmentField::from_masks(&forest_floor, decorations.stone_vertices());
        let mut material = TerrainMaterialField::from_surface(&material, &river_bed, &forced_rock);
        material.suppress_grass_at_vertices(reeds.forest_exclusion_vertices());
        material.suppress_grass_at_vertices(ferns.support_vertices());
        let distance_to_land = {
            let _timer = StageTimer::new("sea_mask.distance_to_land");
            let land: Vec<bool> = terrain
                .mesh()
                .vertices
                .iter()
                .map(|vertex| vertex.z >= 0.0)
                .collect();
            graph_distances(terrain.mesh(), &terrain.mesh().adjacency(), &land)
        };
        Ok(Self {
            seed,
            options,
            generation_method: method,
            terrain,
            material,
            environment,
            coarser_lods: [lod1, lod2],
            rivers,
            distance_to_land,
            river_mesh,
            river_rock_mesh,
            waterfall_feet,
            reeds,
            ferns,
            forest,
            forest_stats,
            forest_options,
            decorations: OnceLock::from(decorations),
        })
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn options(&self) -> IslandOptions {
        self.options
    }

    #[must_use]
    pub const fn generation_method(&self) -> GenerationMethod {
        self.generation_method
    }

    #[must_use]
    pub const fn forest_options(&self) -> ForestOptions {
        self.forest_options
    }

    #[must_use]
    pub const fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    #[must_use]
    pub fn lod(&self, level: usize) -> Option<&Mesh> {
        match level {
            0 => Some(self.terrain.mesh()),
            1 => Some(&self.coarser_lods[0]),
            2 => Some(&self.coarser_lods[1]),
            _ => None,
        }
    }

    /// Returns the corrected support mesh intended for display.
    #[must_use]
    pub fn render_lod(&self, level: usize) -> Option<&Mesh> {
        self.lod(level)
    }

    #[must_use]
    pub const fn river_mesh(&self) -> &Mesh {
        &self.river_mesh
    }

    #[must_use]
    pub const fn river_rock_mesh(&self) -> &Mesh {
        &self.river_rock_mesh
    }

    pub(crate) fn forest_mesh_grid(
        &self,
        kind: ForestMeshKind,
        visual_lod: usize,
        bounds: BoundingBox,
        divisions: usize,
    ) -> Option<Vec<crate::forest::ForestMeshTile>> {
        self.forest.mesh_grid(kind, visual_lod, bounds, divisions)
    }

    #[must_use]
    pub(crate) fn reed_mesh_tiles(&self) -> &[ReedMeshTile] {
        self.reeds.tiles()
    }

    #[must_use]
    pub(crate) fn fern_mesh_tiles(&self) -> &[FernMeshTile] {
        self.ferns.tiles()
    }

    #[allow(dead_code)]
    pub(crate) fn forest_wood_mesh_grid(
        &self,
        visual_lod: usize,
        bounds: BoundingBox,
        divisions: usize,
    ) -> Option<Vec<crate::forest::ForestMeshTile>> {
        self.forest_mesh_grid(ForestMeshKind::Wood, visual_lod, bounds, divisions)
    }

    #[allow(dead_code)]
    pub(crate) fn forest_foliage_mesh_grid(
        &self,
        visual_lod: usize,
        bounds: BoundingBox,
        divisions: usize,
    ) -> Option<Vec<crate::forest::ForestMeshTile>> {
        self.forest_mesh_grid(ForestMeshKind::Foliage, visual_lod, bounds, divisions)
    }

    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn forest_meshes(&self) -> &ForestMeshes {
        &self.forest
    }

    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn forest_stats(&self) -> &ForestGenerationStats {
        &self.forest_stats
    }

    #[must_use]
    pub(crate) fn waterfall_feet(&self) -> &[WaterfallFoot] {
        &self.waterfall_feet
    }

    #[must_use]
    pub fn rivers(&self) -> &[River] {
        &self.rivers
    }

    #[must_use]
    /// Returns the decorations generated with this island.
    ///
    /// # Panics
    ///
    /// Panics only if the island was constructed without completing generation.
    pub fn decorations(&self) -> &Decorations {
        self.decorations
            .get()
            .expect("generated islands always contain decorations")
    }

    #[must_use]
    pub fn render(&self, width: u32, height: u32) -> Raster {
        let mut raster = Raster::new(width, height);
        raster.render(self);
        raster
    }

    #[must_use]
    pub fn height_map(&self, width: u32, height: u32) -> Vec<f32> {
        if width == 0 || height == 0 {
            return Vec::new();
        }

        let width_usize = width as usize;
        let mut output = vec![0.0; width_usize * height as usize];
        output
            .par_chunks_mut(width_usize)
            .enumerate()
            .for_each(|(y, row)| {
                let v = y as f32 / height.saturating_sub(1).max(1) as f32;
                for (x, value) in row.iter_mut().enumerate() {
                    let u = x as f32 / width.saturating_sub(1).max(1) as f32;
                    *value = self.terrain.sample(u, v);
                }
            });
        output
    }

    #[must_use]
    pub fn sea_depth_map(&self, width: u32, height: u32) -> Vec<f32> {
        sample_grid(width, height, |u, v| {
            (-self.terrain.sample(u, v) / (self.options.max_height * 0.28)).clamp(0.0, 1.0)
        })
    }

    /// Bakes a linear interleaved RG8 texture for coastal waves and distance to land.
    #[must_use]
    pub fn sea_mask(&self, width: u32, height: u32) -> Option<crate::SeaMask> {
        crate::sea_mask::bake_sea_mask(&self.terrain, &self.distance_to_land, width, height)
    }

    /// Bakes high-detail normal corrections, directional terrain occlusion,
    /// and vertically projected low-poly canopy occlusion for a target LOD.
    #[must_use]
    pub fn surface_maps(&self, lod: usize, width: u32, height: u32) -> Option<SurfaceMaps> {
        let target = self.lod(lod)?;
        Some(bake_surface_maps(
            &self.terrain,
            (lod != 0).then_some(target),
            self.forest.mesh(ForestMeshKind::Foliage, 2),
            width.max(1),
            height.max(1),
        ))
    }

    #[must_use]
    pub fn normal_map(&self, width: u32, height: u32) -> Vec<u8> {
        let mut output = Vec::with_capacity(width as usize * height as usize * 3);
        for y in 0..height {
            let v = y as f32 / height.saturating_sub(1).max(1) as f32;
            for x in 0..width {
                let u = x as f32 / width.saturating_sub(1).max(1) as f32;
                let normal = self.terrain.sample_normal(u, v);
                output.extend([
                    ((normal.x * 0.5 + 0.5) * 255.0) as u8,
                    ((normal.y * 0.5 + 0.5) * 255.0) as u8,
                    ((normal.z * 0.5 + 0.5) * 255.0) as u8,
                ]);
            }
        }
        output
    }

    #[must_use]
    pub fn foliage_map(&self, dimension: u32) -> Vec<u32> {
        let mut map = vec![0_u32; dimension as usize * dimension as usize];
        let put = |map: &mut [u32], points: &[Vec3], shift: u32, value: u32| {
            for point in points {
                let x = (point.x * (dimension - 1) as f32).round() as usize;
                let y = (point.y * (dimension - 1) as f32).round() as usize;
                map[y * dimension as usize + x] |= value << shift;
            }
        };
        let decorations = self.decorations();
        put(&mut map, &decorations.trees, 24, 255);
        put(&mut map, &decorations.bushes, 16, 210);
        for y in 0..dimension {
            for x in 0..dimension {
                let u = x as f32 / dimension.saturating_sub(1).max(1) as f32;
                let v = y as f32 / dimension.saturating_sub(1).max(1) as f32;
                let richness = noise::fractal(self.seed, u * 5.0, v * 5.0, 3);
                map[y as usize * dimension as usize + x as usize] |=
                    ((richness * 0.5 + 0.5) * 255.0) as u32;
            }
        }
        map
    }

    /// Saves the reproducible seed and generation options.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the destination cannot be created or written.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(b"MOTURS\0\x12")?;
        file.write_all(&self.seed.to_le_bytes())?;
        for value in [
            self.options.max_height,
            self.options.water_ratio,
            self.options.slope_multiplier,
            self.options.coastal_slope_multiplier,
            self.options.hydraulic_erosion_strength,
            self.options.hydraulic_deposition_strength,
            self.options.hydraulic_deposition_slope_degrees,
            self.options.river_source_catchment_hectares,
            self.options.river_source_steep_multiplier,
            self.options.river_source_elevation_boost,
            self.options.river_source_width_metres,
            self.options.river_maximum_width_metres,
            self.options.river_source_depth_metres,
            self.options.river_maximum_depth_metres,
        ] {
            file.write_all(&value.to_le_bytes())?;
        }
        file.write_all(&self.options.terrain_size.to_le_bytes())?;
        file.write_all(&self.forest_options.patch_size_metres.to_le_bytes())?;
        file.write_all(&self.forest_options.noise_threshold.to_le_bytes())?;
        file.write_all(&[self.forest_options.noise_octaves])?;
        file.write_all(&self.forest_options.snowline_metres.to_le_bytes())?;
        file.write_all(&[self.forest_options.prototype_count])?;
        file.write_all(&self.forest_options.minimum_scale.to_le_bytes())?;
        file.write_all(&self.forest_options.maximum_scale.to_le_bytes())?;
        file.write_all(&[self.generation_method.tag()])
    }

    /// Loads a saved seed/options file and deterministically regenerates it.
    ///
    /// # Errors
    ///
    /// Returns an I/O error for unreadable, truncated, or invalid input.
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut reader = SavedIslandReader::new(File::open(path)?)?;
        let seed = reader.read_u64()?;
        let (options, forest_options) = reader.read_options()?;
        let method = reader.read_generation_method()?;
        Self::generate_with_forest_and_method(seed, options, forest_options, method)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    #[must_use]
    pub fn mesh_in(&self, lod: usize, bounds: BoundingBox) -> Option<Mesh> {
        self.lod(lod)
            .map(|mesh| mesh.sliced(bounds).clipped_above(TERRAIN_RENDER_FLOOR))
    }

    /// Clips a display mesh while retaining corrected LOD transitions.
    #[must_use]
    pub fn render_mesh_in(&self, lod: usize, bounds: BoundingBox, clamp_sides: u8) -> Option<Mesh> {
        let mesh = match lod {
            0 => MeshClipper::new(self.terrain.mesh()).sliced(
                bounds,
                (clamp_sides != 0).then_some(&self.coarser_lods[0]),
                clamp_sides,
            ),
            1 | 2 => {
                let mesh = self.lod(lod)?;
                self.lod(lod + 1).filter(|_| clamp_sides != 0).map_or_else(
                    || Some(mesh.sliced(bounds)),
                    |coarser| {
                        mesh.sliced_grid_clamped(bounds, 1, coarser, clamp_sides)
                            .pop()
                    },
                )?
            }
            _ => return None,
        };
        Some(mesh.clipped_above(TERRAIN_RENDER_FLOOR))
    }

    /// Clips a display LOD into one tile batch. The global render mesh is
    /// borrowed and processed once; only returned tile buffers are allocated.
    #[must_use]
    pub fn render_mesh_grid(
        &self,
        lod: usize,
        bounds: BoundingBox,
        divisions: usize,
        clamp_sides: u8,
    ) -> Option<Vec<Mesh>> {
        let mut tiles = match lod {
            0 => MeshClipper::new(self.terrain.mesh()).sliced_grid(
                bounds,
                divisions,
                (clamp_sides != 0).then_some(&self.coarser_lods[0]),
                clamp_sides,
            ),
            1 | 2 => {
                let mesh = self.lod(lod)?;
                self.lod(lod + 1).filter(|_| clamp_sides != 0).map_or_else(
                    || mesh.sliced_grid(bounds, divisions),
                    |coarser| mesh.sliced_grid_clamped(bounds, divisions, coarser, clamp_sides),
                )
            }
            _ => return None,
        };
        for tile in &mut tiles {
            *tile = mem::take(tile).clipped_above(TERRAIN_RENDER_FLOOR);
        }
        Some(tiles)
    }

    /// Per-vertex material weights for `mesh`: x = bedrock hardness or forced
    /// rock, y = loose cover, z = river bed, w = sea proximity. Sampled through
    /// each vertex's UV, so any mesh derived from this island is accepted.
    pub fn material_values_for(&self, mesh: &Mesh) -> Vec<Vec4> {
        mesh.vertices
            .iter()
            .enumerate()
            .map(|(index, vertex)| {
                let point = mesh
                    .uv
                    .get(index)
                    .copied()
                    .unwrap_or_else(|| vertex.truncate())
                    .clamp(Vec2::ZERO, Vec2::ONE);
                self.material
                    .sample(&self.terrain, point)
                    .clamp(Vec4::ZERO, Vec4::ONE)
            })
            .collect()
    }

    /// Per-vertex environment values for `mesh`: x = forest floor, y = stones.
    /// Values use the same authoritative LOD0 sampling path as the
    /// established material channels.
    pub fn environment_values_for(&self, mesh: &Mesh) -> Vec<Vec2> {
        mesh.vertices
            .iter()
            .enumerate()
            .map(|(index, vertex)| {
                let point = mesh
                    .uv
                    .get(index)
                    .copied()
                    .unwrap_or_else(|| vertex.truncate())
                    .clamp(Vec2::ZERO, Vec2::ONE);
                self.environment
                    .sample(&self.terrain, point)
                    .clamp(Vec2::ZERO, Vec2::ONE)
            })
            .collect()
    }
}

pub(super) fn sharp_rock_mask(mesh: &Mesh) -> Vec<bool> {
    let _timer = StageTimer::new("material.sharp_rock");
    let adjacency = mesh.adjacency();
    let perimeter = mesh.perimeter_mask();
    mesh.vertices
        .iter()
        .enumerate()
        .map(|(vertex, position)| {
            position.z > 0.0
                && !perimeter[vertex]
                && adjacency[vertex].len() >= 3
                && mesh
                    .normal_displacement_ratio(&adjacency, vertex)
                    .is_some_and(|ratio| ratio > SHARP_ROCK_DISPLACEMENT_RATIO)
        })
        .collect()
}

#[derive(Clone, Copy)]
pub(super) struct GenerationContext {
    pub(super) seed: u64,
    pub(super) options: IslandOptions,
    pub(super) river_source_rule: RiverSourceRule,
}

#[derive(Default)]
pub(super) struct GenerationScratch {
    pub(super) method: GenerationMethod,
    pub(super) hydraulic: HydraulicScratch,
    #[cfg(feature = "gpu-generation")]
    pub(super) gpu_particle_erosion: GpuParticleErosionScratch,
    pub(super) bedrock_rates: Vec<f32>,
}

impl GenerationScratch {
    fn new(method: GenerationMethod) -> Self {
        Self {
            method,
            ..Self::default()
        }
    }
}

impl GenerationContext {
    pub(super) fn new(seed: u64, options: IslandOptions) -> Self {
        Self {
            seed,
            options,
            river_source_rule: options.river_source_rule(),
        }
    }
}

pub(super) fn generate_base(
    seed: u64,
    options: IslandOptions,
    scratch: &mut GenerationScratch,
) -> Result<(Mesh, SurfaceMaterial), String> {
    let _timer = StageTimer::new("generation.base");
    let points = create_seed_points(seed, options.terrain_size as usize);
    let mut mesh = Mesh::delaunay(&points);
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let adjacency = mesh.adjacency();
    let geology = assign_elevations(&mut mesh, &adjacency, seed, options);
    material.initialize_geology(&mesh, geology);
    hydraulic_erode_stage(&mut mesh, &adjacency, &mut material, 0.45, options, scratch)?;
    erode_mesh(&mut mesh, &adjacency, &mut material, options, 5);
    mesh.calculate_normals();
    Ok((mesh, material))
}

fn mutate_surface_preserving_material_volume(
    mesh: &mut Mesh,
    material: &mut SurfaceMaterial,
    mutation: impl FnOnce(&mut Mesh) -> usize,
) -> usize {
    let deposited_volume = material.volume(mesh);
    let mutations = mutation(mesh);
    if mutations > 0 {
        material.rescale_to_volume(mesh, deposited_volume);
    }
    mutations
}

fn optimize_finished_terrain_surface(
    mesh: &mut Mesh,
    material: &mut SurfaceMaterial,
    river_bed: &[bool],
) -> usize {
    const PROTECTED_COAST_HEIGHT_METRES: f32 = 1.0;

    debug_assert_eq!(river_bed.len(), mesh.vertices.len());
    let perimeter = mesh.perimeter_mask();
    let centroid_repairs = mutate_surface_preserving_material_volume(mesh, material, |mesh| {
        repair_projected_foldover_vertices(mesh, &perimeter)
    });

    let protected_height = PROTECTED_COAST_HEIGHT_METRES / ISLAND_WORLD_METRES;
    let mut protected_vertices = vec![false; mesh.vertices.len()];
    let mut protected_edges = HashSet::<(u32, u32)>::new();
    for triangle in mesh.triangles.chunks_exact(3).filter(|triangle| {
        triangle.iter().any(|&vertex| {
            river_bed[vertex as usize] || mesh.vertices[vertex as usize].z <= protected_height
        })
    }) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
        for vertex in [a, b, c] {
            protected_vertices[vertex as usize] = true;
        }
        protected_edges.extend([
            (a.min(b), a.max(b)),
            (b.min(c), b.max(c)),
            (c.min(a), c.max(a)),
        ]);
    }
    let refinements = mutate_surface_preserving_material_volume(mesh, material, |mesh| {
        let repairs = mesh.repair_projected_foldovers_preserving(|a, b| {
            protected_edges.contains(&(a.min(b), a.max(b)))
        });
        let vertex_repairs = repair_projected_foldover_vertices(mesh, &protected_vertices);
        repairs + vertex_repairs
    });
    centroid_repairs + refinements
}

fn repair_projected_foldover_vertices(mesh: &mut Mesh, protected: &[bool]) -> usize {
    const MAXIMUM_REPAIR_PASSES: usize = 32;

    debug_assert_eq!(protected.len(), mesh.vertices.len());
    let mut incident_faces = vec![Vec::<usize>::new(); mesh.vertices.len()];
    for (face, triangle) in mesh.triangles.chunks_exact(3).enumerate() {
        for vertex in triangle.iter().map(|&vertex| vertex as usize) {
            incident_faces[vertex].push(face);
        }
    }
    let mut total_repairs = 0;
    for _ in 0..MAXIMUM_REPAIR_PASSES {
        let folded_faces: Vec<usize> = (0..mesh.triangles.len() / 3)
            .filter(|&face| projected_face_area(mesh, face) <= 0.0)
            .collect();
        if folded_faces.is_empty() {
            break;
        }

        let mut pass_repairs = 0;
        for face in folded_faces {
            if projected_face_area(mesh, face) > 0.0 {
                continue;
            }
            let offset = face * 3;
            let candidate = mesh.triangles[offset..offset + 3]
                .iter()
                .map(|&vertex| vertex as usize)
                .filter(|&vertex| !protected[vertex])
                .filter_map(|vertex| {
                    one_ring_centroid_candidate(mesh, &incident_faces[vertex], vertex)
                        .map(|(position, minimum_area)| (vertex, position, minimum_area))
                })
                .max_by(
                    |(left_vertex, left, left_area), (right_vertex, right, right_area)| {
                        left_area
                            .total_cmp(right_area)
                            .then_with(|| {
                                right
                                    .distance_squared(mesh.vertices[*right_vertex])
                                    .total_cmp(&left.distance_squared(mesh.vertices[*left_vertex]))
                            })
                            .then_with(|| right_vertex.cmp(left_vertex))
                    },
                );
            if let Some((vertex, candidate, _)) = candidate {
                mesh.vertices[vertex] = candidate;
                pass_repairs += 1;
            }
        }
        total_repairs += pass_repairs;
        if pass_repairs == 0 {
            break;
        }
    }

    if total_repairs > 0 {
        if !mesh.uv.is_empty() {
            mesh.uv
                .iter_mut()
                .zip(&mesh.vertices)
                .for_each(|(uv, vertex)| *uv = vertex.truncate());
        }
        mesh.calculate_normals();
    }
    total_repairs
}

fn one_ring_centroid_candidate(mesh: &Mesh, faces: &[usize], vertex: usize) -> Option<(Vec3, f32)> {
    const MAXIMUM_PROJECTION_PASSES: usize = 64;
    const MINIMUM_AREA_FRACTION: f32 = 0.01;
    const MINIMUM_AREA: f32 = 1.0e-12;

    let mut neighbours: Vec<usize> = faces
        .iter()
        .flat_map(|&face| {
            let offset = face * 3;
            mesh.triangles[offset..offset + 3]
                .iter()
                .map(|&candidate| candidate as usize)
        })
        .filter(|&candidate| candidate != vertex)
        .collect();
    neighbours.sort_unstable();
    neighbours.dedup();
    if neighbours.len() < 2 {
        return None;
    }
    let centroid = neighbours
        .iter()
        .map(|&neighbour| mesh.vertices[neighbour].truncate())
        .sum::<Vec2>()
        / neighbours.len() as f32;
    let candidate = Vec3::new(centroid.x, centroid.y, mesh.vertices[vertex].z);
    let current_minimum = faces
        .iter()
        .map(|&face| projected_face_area(mesh, face))
        .fold(f32::INFINITY, f32::min);
    let candidate_minimum = faces
        .iter()
        .map(|&face| projected_face_area_with_vertex(mesh, face, vertex, candidate))
        .fold(f32::INFINITY, f32::min);
    if !candidate.is_finite() || !candidate_minimum.is_finite() {
        return None;
    }
    if candidate_minimum > 0.0 {
        return Some((candidate, candidate_minimum));
    }

    let target_area = (faces
        .iter()
        .map(|&face| projected_face_area(mesh, face).abs())
        .sum::<f32>()
        / faces.len().max(1) as f32
        * MINIMUM_AREA_FRACTION)
        .max(MINIMUM_AREA);
    let mut projected = candidate;
    for _ in 0..MAXIMUM_PROJECTION_PASSES {
        let mut complete = true;
        for &face in faces {
            let area = projected_face_area_with_vertex(mesh, face, vertex, projected);
            if area >= target_area {
                continue;
            }
            let gradient = Vec2::new(
                projected_face_area_with_vertex(mesh, face, vertex, projected + Vec3::X) - area,
                projected_face_area_with_vertex(mesh, face, vertex, projected + Vec3::Y) - area,
            );
            let gradient_length_squared = gradient.length_squared();
            if gradient_length_squared <= f32::MIN_POSITIVE || !gradient_length_squared.is_finite()
            {
                return None;
            }
            projected += (gradient * ((target_area - area) / gradient_length_squared)).extend(0.0);
            complete = false;
        }
        if complete {
            let minimum_area = faces
                .iter()
                .map(|&face| projected_face_area_with_vertex(mesh, face, vertex, projected))
                .fold(f32::INFINITY, f32::min);
            return (projected.is_finite() && minimum_area > 0.0)
                .then_some((projected, minimum_area));
        }
    }

    (candidate_minimum > current_minimum + 1.0e-12).then_some((candidate, candidate_minimum))
}

pub(super) fn generate_lod2(
    base: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> Result<(Mesh, SurfaceMaterial), String> {
    let _timer = StageTimer::new("generation.lod2");
    let tessellation = base.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(base, tessellation);
    let adjacency = mesh.adjacency();
    mesh.smooth_with(&adjacency);
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.55,
        context.options,
        scratch,
    )?;
    erode_mesh(&mut mesh, &adjacency, &mut material, context.options, 4);
    let mut rivers = RiverNetwork::generate(
        &mut mesh,
        &adjacency,
        context.river_source_rule,
        context.seed,
    );
    rivers.shape_with_settings(
        &mut mesh,
        &adjacency,
        &mut material,
        true,
        true,
        context.options.river_channel_settings(),
    );
    mesh.calculate_normals();
    Ok((mesh, material))
}

pub(super) fn generate_first_lod1(
    lod2: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> Result<(Mesh, SurfaceMaterial), String> {
    let _timer = StageTimer::new("generation.lod1.first");
    let tessellation = lod2.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(lod2, tessellation);
    let adjacency = mesh.adjacency();
    mesh.smooth_with(&adjacency);
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.65,
        context.options,
        scratch,
    )?;
    erode_mesh(&mut mesh, &adjacency, &mut material, context.options, 4);
    let mut rivers = RiverNetwork::generate(
        &mut mesh,
        &adjacency,
        context.river_source_rule,
        context.seed,
    );
    rivers.shape_with_settings(
        &mut mesh,
        &adjacency,
        &mut material,
        false,
        true,
        context.options.river_channel_settings(),
    );
    mesh.calculate_normals();
    Ok((mesh, material))
}

pub(super) fn generate_broad_lod0(
    lod1: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> Result<(Mesh, SurfaceMaterial), String> {
    let _timer = StageTimer::new("generation.lod0.broad");
    let tessellation = lod1.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(lod1, tessellation);
    mutate_surface_preserving_material_volume(
        &mut mesh,
        &mut material,
        Mesh::optimize_surface_triangulation,
    );
    let adjacency = mesh.adjacency();
    mesh.smooth_with(&adjacency);
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.8,
        context.options,
        scratch,
    )?;
    mesh.calculate_normals();

    let tessellation = mesh.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    (mesh, material) = material.into_tessellated(&mesh, tessellation);
    let adjacency = mesh.adjacency();
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.75,
        context.options,
        scratch,
    )?;
    erode_mesh(&mut mesh, &adjacency, &mut material, context.options, 2);
    let mut rivers = RiverNetwork::generate(
        &mut mesh,
        &adjacency,
        context.river_source_rule,
        context.seed,
    );
    rivers.shape_with_settings(
        &mut mesh,
        &adjacency,
        &mut material,
        true,
        true,
        context.options.river_channel_settings(),
    );
    mesh.smooth_land_with(&adjacency);
    Ok((mesh, material))
}

pub(super) fn generate_detail_lod0(
    mut lod0: Mesh,
    mut material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> Result<(Mesh, SurfaceMaterial), String> {
    let _timer = StageTimer::new("generation.lod0.detail");
    coastal_uplift::prepare(&mut lod0, &mut material);
    let tessellation = lod0.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(&lod0, tessellation);
    let adjacency = mesh.adjacency();
    hydraulic_erode_stage_depositing_across_sea(
        &mut mesh,
        &adjacency,
        &mut material,
        0.5,
        context.options,
        scratch,
    )?;
    mesh.smooth_land_with(&adjacency);
    mesh.smooth_seabed_with(&adjacency);
    Ok((mesh, material))
}

/// Runs the second adaptive LOD1 shaping pass while keeping flatter faces at
/// their existing density.
pub(super) fn refine_lod1_again(
    lod1: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> Result<(Mesh, SurfaceMaterial), String> {
    let _timer = StageTimer::new("generation.lod1.refine");
    let tessellation = lod1.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut refined, mut material) = material.into_tessellated(lod1, tessellation);
    let adjacency = refined.adjacency();
    refined.smooth_with(&adjacency);
    hydraulic_erode_stage(
        &mut refined,
        &adjacency,
        &mut material,
        0.7,
        context.options,
        scratch,
    )?;
    erode_mesh(&mut refined, &adjacency, &mut material, context.options, 3);
    let mut rivers = RiverNetwork::generate(
        &mut refined,
        &adjacency,
        context.river_source_rule,
        context.seed,
    );
    rivers.shape_with_settings(
        &mut refined,
        &adjacency,
        &mut material,
        false,
        true,
        context.options.river_channel_settings(),
    );
    refined.calculate_normals();
    Ok((refined, material))
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DistanceState {
    pub(super) cost: f32,
    pub(super) vertex: usize,
}

impl PartialEq for DistanceState {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex && self.cost.to_bits() == other.cost.to_bits()
    }
}

impl Eq for DistanceState {}

impl PartialOrd for DistanceState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DistanceState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

pub(super) fn create_seed_points(seed: u64, count: usize) -> Vec<Vec2> {
    let mut rng = Rng::new(seed);
    let mut points = Vec::with_capacity(count);
    for _ in 0..count.saturating_sub(8) {
        points.push(Vec2::new(rng.unit(), rng.unit()));
    }
    points.extend([
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 0.5),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.5, 1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(1.0, 0.5),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.5, 0.0),
    ]);
    points
}

pub(super) fn assign_elevations(
    mesh: &mut Mesh,
    adjacency: &Adjacency,
    seed: u64,
    options: IslandOptions,
) -> GeologyField {
    let geology =
        GeologyField::calibrated(seed, mesh.vertices.iter().map(|vertex| vertex.truncate()));
    let scores: Vec<f32> = mesh
        .vertices
        .iter()
        .map(|vertex| {
            let dx = vertex.x.mul_add(2.0, -1.0);
            let dy = vertex.y.mul_add(2.0, -1.0);
            let radius = dx.hypot(dy);
            geology::terrain_noise(seed, vertex.truncate()).height_component() + 0.82
                - radius.powf(1.65)
        })
        .collect();
    let mut ranked = scores.clone();
    ranked.sort_unstable_by(f32::total_cmp);
    let sea_index = ((ranked.len() - 1) as f32 * options.water_ratio) as usize;
    let sea_level = ranked[sea_index];
    let perimeter = mesh.perimeter_vertices();
    let candidate_sea: Vec<bool> = scores.iter().map(|score| *score < sea_level).collect();
    let mut sea = vec![false; mesh.vertices.len()];
    let mut fringe: Vec<usize> = perimeter
        .into_iter()
        .filter(|index| candidate_sea[*index])
        .collect();
    for &vertex in &fringe {
        sea[vertex] = true;
    }
    while let Some(vertex) = fringe.pop() {
        for &neighbour in &adjacency[vertex] {
            if candidate_sea[neighbour] && !sea[neighbour] {
                sea[neighbour] = true;
                fringe.push(neighbour);
            }
        }
    }

    let distance_to_sea = graph_distances(mesh, adjacency, &sea);
    let land: Vec<bool> = sea.iter().map(|value| !value).collect();
    let distance_to_land = graph_distances(mesh, adjacency, &land);
    let max_land = distance_to_sea
        .iter()
        .zip(&land)
        .filter_map(|(distance, is_land)| is_land.then_some(*distance))
        .fold(f32::EPSILON, f32::max);
    let max_sea = distance_to_land
        .iter()
        .zip(&sea)
        .filter_map(|(distance, is_sea)| is_sea.then_some(*distance))
        .fold(f32::EPSILON, f32::max);

    for (index, vertex) in mesh.vertices.iter_mut().enumerate() {
        vertex.z = if sea[index] {
            -distance_to_land[index] / max_sea * options.max_height * 0.28
        } else {
            let normalized = distance_to_sea[index] / max_land;
            normalized.powf(options.coastal_slope_multiplier.max(0.1)) * options.max_height
        };
    }
    geology
}

pub(super) fn graph_distances(mesh: &Mesh, adjacency: &Adjacency, target: &[bool]) -> Vec<f32> {
    let mut distances = vec![f32::INFINITY; mesh.vertices.len()];
    let mut queue = BinaryHeap::new();
    for (vertex, &is_target) in target.iter().enumerate() {
        if is_target {
            distances[vertex] = 0.0;
            queue.push(DistanceState { cost: 0.0, vertex });
        }
    }
    while let Some(DistanceState { cost, vertex }) = queue.pop() {
        if cost > distances[vertex] {
            continue;
        }
        for &neighbour in &adjacency[vertex] {
            let edge =
                (mesh.vertices[vertex].truncate() - mesh.vertices[neighbour].truncate()).length();
            let next = cost + edge;
            if next < distances[neighbour] {
                distances[neighbour] = next;
                queue.push(DistanceState {
                    cost: next,
                    vertex: neighbour,
                });
            }
        }
    }
    distances
}

fn sea_proximity_strengths(mesh: &Mesh, adjacency: &Adjacency, sea: &[bool]) -> Vec<f32> {
    let full_strength_distance = SEA_PROXIMITY_FULL_STRENGTH_METRES / ISLAND_WORLD_METRES;
    let fade_distance = (SEA_PROXIMITY_ZERO_STRENGTH_METRES - SEA_PROXIMITY_FULL_STRENGTH_METRES)
        / ISLAND_WORLD_METRES;
    graph_distances(mesh, adjacency, sea)
        .into_iter()
        .map(|distance| (1.0 - (distance - full_strength_distance) / fade_distance).clamp(0.0, 1.0))
        .collect()
}

#[cfg(test)]
mod sea_proximity_tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn tessellation_edge_flips_preserve_deposited_material_volume() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(2.0, 1.0, 8.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            triangles: vec![0, 1, 2, 0, 2, 3],
            ..Mesh::default()
        };
        mesh.calculate_normals();
        let mut material = SurfaceMaterial::empty(mesh.vertices.len());
        material.depths_mut().copy_from_slice(&[0.1, 0.2, 0.4, 0.8]);
        let volume = material.volume(&mesh);

        let flips = mutate_surface_preserving_material_volume(
            &mut mesh,
            &mut material,
            Mesh::optimize_surface_triangulation,
        );

        assert_eq!(flips, 1);
        assert_eq!(material.depths().len(), mesh.vertices.len());
        assert!((material.volume(&mesh) - volume).abs() <= volume * 1.0e-6);
    }

    #[test]
    fn local_vertex_repair_untangles_a_fold_with_no_valid_diagonal() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 2.0),
                Vec3::new(-0.1, 0.5, 3.0),
                Vec3::new(0.0, 1.0, 4.0),
            ],
            triangles: vec![0, 1, 2, 0, 2, 3],
            ..Mesh::default()
        };
        let original_heights: Vec<f32> = mesh.vertices.iter().map(|vertex| vertex.z).collect();

        assert_eq!(mesh.projected_foldover_count(), 1);
        assert_eq!(mesh.repair_projected_foldovers_preserving(|_, _| false), 0);
        let repairs = repair_projected_foldover_vertices(&mut mesh, &[false; 4]);

        assert_eq!(repairs, 1);
        assert_eq!(mesh.projected_foldover_count(), 0);
        assert_eq!(
            mesh.vertices
                .iter()
                .map(|vertex| vertex.z)
                .collect::<Vec<_>>(),
            original_heights
        );
    }

    #[test]
    fn local_vertex_repair_moves_a_folded_apex_to_its_one_ring_centroid() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.2, 0.5, 2.0),
            ],
            triangles: vec![4, 0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0],
            ..Mesh::default()
        };

        assert_eq!(mesh.projected_foldover_count(), 1);
        let repairs =
            repair_projected_foldover_vertices(&mut mesh, &[true, true, true, true, false]);

        assert_eq!(repairs, 1);
        assert_eq!(mesh.projected_foldover_count(), 0);
        assert_eq!(mesh.vertices[4], Vec3::new(0.5, 0.5, 2.0));
        assert_eq!(mesh.triangles.len(), 12);
    }

    #[test]
    fn sea_proximity_stays_full_for_two_metres_then_fades_to_twenty() {
        let distances_metres = [0.0_f32, 2.0, 11.0, 20.0, 30.0];
        let row_spacing = 1.0 / ISLAND_WORLD_METRES;
        let mesh = Mesh {
            vertices: distances_metres
                .into_iter()
                .flat_map(|distance_metres| {
                    let x = distance_metres / ISLAND_WORLD_METRES;
                    [Vec3::new(x, 0.0, 0.0), Vec3::new(x, row_spacing, 0.0)]
                })
                .collect(),
            triangles: vec![
                0, 2, 3, 0, 3, 1, 2, 4, 5, 2, 5, 3, 4, 6, 7, 4, 7, 5, 6, 8, 9, 6, 9, 7,
            ],
            ..Mesh::default()
        };
        let adjacency = mesh.adjacency();
        let sea = [
            true, true, false, false, false, false, false, false, false, false,
        ];

        let strengths = sea_proximity_strengths(&mesh, &adjacency, &sea);

        for (column, expected) in [1.0_f32, 1.0, 0.5, 0.0, 0.0].into_iter().enumerate() {
            for row in 0..2 {
                let actual = strengths[column * 2 + row];
                assert!((actual - expected).abs() < 1.0e-6);
            }
        }
    }

    #[test]
    fn version_sixteen_payload_uses_default_forest_options() {
        let mut bytes = b"MOTURS\0\x10".to_vec();
        bytes.extend(77_u64.to_le_bytes());
        for value in [
            0.2_f32, 0.6, 1.3, 1.0, 1.0, 1.5, 12.0, 0.05, 4.0, 9.0, 2.0, 14.0, 0.35, 2.0,
        ] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(24_u32.to_le_bytes());

        let mut reader = SavedIslandReader::new(Cursor::new(bytes)).unwrap();
        let (_, forest_options) = reader.read_options().unwrap();

        assert_eq!(forest_options, ForestOptions::default());
    }

    #[test]
    fn saved_generation_methods_are_backward_compatible() {
        let mut current = SavedIslandReader::new(Cursor::new(b"MOTURS\0\x12\x01")).unwrap();
        assert_eq!(
            current.read_generation_method().unwrap(),
            GenerationMethod::Gpu
        );

        let mut legacy = SavedIslandReader::new(Cursor::new(b"MOTURS\0\x11")).unwrap();
        assert_eq!(
            legacy.read_generation_method().unwrap(),
            GenerationMethod::Cpu
        );
    }

    #[test]
    fn saved_generation_method_rejects_unknown_tags() {
        let mut reader = SavedIslandReader::new(Cursor::new(b"MOTURS\0\x12\xff")).unwrap();
        let error = reader.read_generation_method().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
