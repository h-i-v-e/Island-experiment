use super::{
    Adjacency, BinaryHeap, BoundingBox, DETAIL_DISPLACEMENT_RATIO, Decorations, File, GeologyField,
    HashSet, HydraulicScratch, ISLAND_WORLD_METRES, IndexedParallelIterator, IslandOptions, Mesh,
    MeshClipper, NewVertexStencil, OnceLock, Ordering, ParallelIterator, ParallelSliceMut, Path,
    Raster, Read, River, RiverChannelSettings, RiverNetwork, RiverSourceRule, Rng,
    SHARP_ROCK_DISPLACEMENT_RATIO, StageTimer, SurfaceMaps, SurfaceMaterial, TERRAIN_RENDER_FLOOR,
    Terrain, TerrainMaterialField, TriangleIndex, Vec2, Vec3, Write, append_settled_rocks,
    bake_surface_maps, bury_river_banks, clear_loose_soil, encode_bank_distance_in_uv, erode_mesh,
    fix_inland_seas, geology, hydraulic_erode_stage, io, legacy_catchment_hectares, mem, noise,
    sample_grid, sample_mesh_surface,
};
use crate::forest::{
    ForestGenerationStats, ForestMeshKind, ForestMeshes, ForestOptions, generate_forest,
};

const SEA_PROXIMITY_FULL_STRENGTH_METRES: f32 = 2.0;
const SEA_PROXIMITY_ZERO_STRENGTH_METRES: f32 = 20.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Island {
    pub(super) seed: u64,
    pub(super) options: IslandOptions,
    pub(super) terrain: Terrain,
    pub(super) material: TerrainMaterialField,
    pub(super) coarser_lods: [Mesh; 2],
    pub(super) rivers: Vec<River>,
    pub(super) distance_to_land: Vec<f32>,
    pub(super) river_mesh: Mesh,
    pub(super) river_rock_mesh: Mesh,
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
}

struct SavedIslandReader<R> {
    source: R,
    version: u8,
}

impl<R: Read> SavedIslandReader<R> {
    fn new(mut source: R) -> io::Result<Self> {
        let mut magic = [0_u8; 8];
        source.read_exact(&mut magic)?;
        if &magic[..7] != b"MOTURS\0" || !matches!(magic[7], 3..=17) {
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
) -> FinalRiverGeneration {
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
            return FinalRiverGeneration {
                lod0: attempt_lod0,
                material: attempt_material,
                rivers: parts.rivers,
                river_mesh: parts.river_mesh,
                river_bed: parts.river_bed,
                river_rock_mesh: parts.river_rock_mesh,
            };
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
        let _timer = StageTimer::new("island.generate");
        let options = options.validate()?;
        let forest_options = forest_options.validate()?;
        let mut scratch = GenerationScratch::default();
        let (base, material) = generate_base(seed, options, &mut scratch);
        let context = GenerationContext::new(options);
        let (mut lod2, material) = generate_lod2(&base, material, context, &mut scratch);
        let (lod1, material) = generate_first_lod1(&lod2, material, context, &mut scratch);
        let (mut lod1, material) = refine_lod1_again(
            &lod1,
            material,
            options,
            context.river_source_rule,
            &mut scratch,
        );
        let (lod0, material) = generate_broad_lod0(&lod1, material, context, &mut scratch);
        let (lod0, material) = generate_detail_lod0(&lod0, material, context, &mut scratch);
        let FinalRiverGeneration {
            mut lod0,
            mut material,
            rivers,
            mut river_mesh,
            river_bed,
            mut river_rock_mesh,
        } = generate_final_rivers(
            seed,
            &lod0,
            &material,
            context.river_source_rule,
            options.river_channel_settings(),
        );
        let lod0_index = {
            let _timer = StageTimer::new("lod.correct");
            correct_lods(&mut lod0, &mut lod1, &mut lod2)
        };
        bury_river_banks(&mut river_mesh, &lod0, &lod0_index);
        river_mesh = river_mesh.clipped_above(0.0);
        encode_bank_distance_in_uv(&mut river_mesh);
        let forced_rock = sharp_rock_mask(&lod0);
        let provisional_material =
            TerrainMaterialField::from_surface(&material, &river_bed, &forced_rock);

        let terrain = {
            let _timer = StageTimer::new("terrain.index");
            Terrain::with_index(lod0, lod0_index)
        };
        let (mut decorations, settled_rocks) = Decorations::generate(
            seed,
            &terrain,
            &provisional_material,
            &rivers,
            options.terrain_size as usize * 4,
        );
        append_settled_rocks(seed, &settled_rocks, &mut river_rock_mesh);
        clear_loose_soil(&mut material, decorations.cleared_soil_vertices());
        let (forest, forest_stats) = generate_forest(
            seed,
            terrain.mesh(),
            crate::forest::ForestSurface {
                river_bed: &river_bed,
                deposited_depths: material.depths(),
                sea_proximity: material.sea_proximities(),
            },
            forest_options,
        )?;
        decorations.set_tree_anchors(forest.placements().iter().map(|placement| placement.anchor));
        let material = TerrainMaterialField::from_surface(&material, &river_bed, &forced_rock);
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
            terrain,
            material,
            coarser_lods: [lod1, lod2],
            rivers,
            distance_to_land,
            river_mesh,
            river_rock_mesh,
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
    ) -> Option<Vec<Mesh>> {
        self.forest.mesh_grid(kind, visual_lod, bounds, divisions)
    }

    #[allow(dead_code)]
    pub(crate) fn forest_wood_mesh_grid(
        &self,
        visual_lod: usize,
        bounds: BoundingBox,
        divisions: usize,
    ) -> Option<Vec<Mesh>> {
        self.forest_mesh_grid(ForestMeshKind::Wood, visual_lod, bounds, divisions)
    }

    #[allow(dead_code)]
    pub(crate) fn forest_foliage_mesh_grid(
        &self,
        visual_lod: usize,
        bounds: BoundingBox,
        divisions: usize,
    ) -> Option<Vec<Mesh>> {
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

    /// Derives sparse rough-water locations from the authoritative unsliced
    /// river mesh without retaining a second copy on the island.
    #[must_use]
    pub fn river_emitters(
        &self,
        sharpness_degrees: f32,
        spacing_metres: f32,
    ) -> Vec<crate::RiverEmitter> {
        crate::extract_river_emitters(&self.river_mesh, sharpness_degrees, spacing_metres)
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
        file.write_all(b"MOTURS\0\x11")?;
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
        file.write_all(&self.forest_options.maximum_scale.to_le_bytes())
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
        Self::generate_with_forest(seed, options, forest_options)
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

    /// Per-vertex material weights for `mesh`: x = bedrock hardness, y = loose
    /// cover, z = sea proximity. Sampled through each vertex's UV, so any mesh
    /// derived from this island (full, sliced, or tiled) is accepted.
    pub fn material_values_for(&self, mesh: &Mesh) -> Vec<Vec3> {
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
                    .clamp(Vec3::ZERO, Vec3::ONE)
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
    pub(super) options: IslandOptions,
    pub(super) river_source_rule: RiverSourceRule,
}

#[derive(Default)]
pub(super) struct GenerationScratch {
    pub(super) hydraulic: HydraulicScratch,
    pub(super) bedrock_rates: Vec<f32>,
}

impl GenerationContext {
    pub(super) fn new(options: IslandOptions) -> Self {
        Self {
            options,
            river_source_rule: options.river_source_rule(),
        }
    }
}

pub(super) fn generate_base(
    seed: u64,
    options: IslandOptions,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
    let _timer = StageTimer::new("generation.base");
    let points = create_seed_points(seed, options.terrain_size as usize);
    let mut mesh = Mesh::delaunay(&points);
    let mut material = SurfaceMaterial::empty(mesh.vertices.len());
    let adjacency = mesh.adjacency();
    let geology = assign_elevations(&mut mesh, &adjacency, seed, options);
    material.initialize_geology(&mesh, geology);
    hydraulic_erode_stage(&mut mesh, &adjacency, &mut material, 0.45, options, scratch);
    erode_mesh(&mut mesh, &adjacency, &mut material, options, 5);
    mesh.calculate_normals();
    (mesh, material)
}

pub(super) fn generate_lod2(
    base: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
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
    );
    erode_mesh(&mut mesh, &adjacency, &mut material, context.options, 4);
    let mut rivers = RiverNetwork::generate(&mut mesh, &adjacency, context.river_source_rule);
    rivers.shape_with_settings(
        &mut mesh,
        &adjacency,
        &mut material,
        true,
        true,
        context.options.river_channel_settings(),
    );
    mesh.calculate_normals();
    (mesh, material)
}

pub(super) fn generate_first_lod1(
    lod2: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
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
    );
    erode_mesh(&mut mesh, &adjacency, &mut material, context.options, 4);
    let mut rivers = RiverNetwork::generate(&mut mesh, &adjacency, context.river_source_rule);
    rivers.shape_with_settings(
        &mut mesh,
        &adjacency,
        &mut material,
        false,
        true,
        context.options.river_channel_settings(),
    );
    mesh.calculate_normals();
    (mesh, material)
}

pub(super) fn generate_broad_lod0(
    lod1: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
    let _timer = StageTimer::new("generation.lod0.broad");
    let tessellation = lod1.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(lod1, tessellation);
    let deposited_volume = material.volume(&mesh);
    mesh.optimize_surface_triangulation();
    material.rescale_to_volume(&mesh, deposited_volume);
    let adjacency = mesh.adjacency();
    mesh.smooth_with(&adjacency);
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.8,
        context.options,
        scratch,
    );
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
    );
    erode_mesh(&mut mesh, &adjacency, &mut material, context.options, 2);
    let mut rivers = RiverNetwork::generate(&mut mesh, &adjacency, context.river_source_rule);
    rivers.shape_with_settings(
        &mut mesh,
        &adjacency,
        &mut material,
        true,
        true,
        context.options.river_channel_settings(),
    );
    mesh.smooth_land_with(&adjacency);
    (mesh, material)
}

pub(super) fn generate_detail_lod0(
    lod0: &Mesh,
    material: SurfaceMaterial,
    context: GenerationContext,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
    let _timer = StageTimer::new("generation.lod0.detail");
    let tessellation = lod0.tessellated_displaced_attributed(DETAIL_DISPLACEMENT_RATIO);
    let (mut mesh, mut material) = material.into_tessellated(lod0, tessellation);
    let adjacency = mesh.adjacency();
    hydraulic_erode_stage(
        &mut mesh,
        &adjacency,
        &mut material,
        0.5,
        context.options,
        scratch,
    );
    mesh.smooth_land_with(&adjacency);
    mesh.smooth_seabed_with(&adjacency);
    (mesh, material)
}

/// Runs the second adaptive LOD1 shaping pass while keeping flatter faces at
/// their existing density.
pub(super) fn refine_lod1_again(
    lod1: &Mesh,
    material: SurfaceMaterial,
    options: IslandOptions,
    river_source_rule: RiverSourceRule,
    scratch: &mut GenerationScratch,
) -> (Mesh, SurfaceMaterial) {
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
        options,
        scratch,
    );
    erode_mesh(&mut refined, &adjacency, &mut material, options, 3);
    let mut rivers = RiverNetwork::generate(&mut refined, &adjacency, river_source_rule);
    rivers.shape_with_settings(
        &mut refined,
        &adjacency,
        &mut material,
        false,
        true,
        options.river_channel_settings(),
    );
    refined.calculate_normals();
    (refined, material)
}

pub(super) fn correct_lods(lod0: &mut Mesh, lod1: &mut Mesh, lod2: &mut Mesh) -> TriangleIndex {
    let lod1_refinement = lod1.tessellated_attributed();
    let lod2_refinement = lod2.tessellated_attributed();
    *lod1 = lod1_refinement.mesh;
    *lod2 = lod2_refinement.mesh;

    let lod0_index = TriangleIndex::new(lod0);
    pin_refined_lod(lod1, &lod1_refinement.new_vertices, lod0, &lod0_index);
    pin_refined_lod(lod2, &lod2_refinement.new_vertices, lod0, &lod0_index);

    for mesh in [lod0, lod1, lod2] {
        mesh.uv
            .iter_mut()
            .zip(&mesh.vertices)
            .for_each(|(uv, vertex)| *uv = vertex.truncate());
        mesh.calculate_normals();
    }
    lod0_index
}

pub(super) fn pin_refined_lod(
    mesh: &mut Mesh,
    new_vertices: &[NewVertexStencil],
    lod0: &Mesh,
    lod0_index: &TriangleIndex,
) {
    let shared_vertex_count = mesh.vertices.len() - new_vertices.len();
    debug_assert!(lod0.vertices.len() >= shared_vertex_count);
    mesh.vertices[..shared_vertex_count].copy_from_slice(&lod0.vertices[..shared_vertex_count]);

    for stencil in new_vertices {
        let [a, b] = [
            stencil.surrounding[0] as usize,
            stencil.surrounding[1] as usize,
        ];
        debug_assert!(a < shared_vertex_count && b < shared_vertex_count);
        let point = (mesh.vertices[a].truncate() + mesh.vertices[b].truncate()) * 0.5;
        let elevation = sample_mesh_surface(lod0, lod0_index, point.x, point.y).0;
        mesh.vertices[stencil.vertex as usize] = point.extend(elevation);
    }
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
}
