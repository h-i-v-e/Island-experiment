//! Planetary terrain in radial coordinates: the base radius is sea level,
//! positive elevations point outwards and gravity is the negative unit direction.
mod banks;
mod channels;
mod contact_banks;
mod drainage;
mod erosion;
mod graph;
mod ocean;
mod refine;
mod river_profile;
mod water_mesh;

use crate::{DVec3, PlanetError, PlanetMesh, PlanetOptions, generate, noise};
pub use drainage::Drainage;
use graph::SurfaceGraph;
pub use ocean::{OceanPass, OceanStage};
pub use river_profile::Waterfall;
pub use water_mesh::RiverSurface;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainOptions {
    /// Radial displacement amplitude, relative to the original sphere radius.
    pub relief: f64,
    /// Signed relative elevation offset; never recalibrates sea level.
    pub land_bias: f64,
    pub continental_frequency: f64,
    pub detail_frequency: f64,
    pub erosion_strength: f64,
    pub hydraulic_passes: u32,
    pub thermal_passes: u32,
    /// Join disconnected seas at LOD2/LOD1; later closures are allowed.
    pub connect_seas: bool,
    /// Rainfall catchment as a fraction of the whole sphere's surface area.
    pub river_catchment: f64,
    /// River incision depth relative to the original sphere radius.
    pub river_depth: f64,
    pub refinement_passes: u32,
    pub smoothing_passes: u32,
    pub river_refinement_passes: u32,
    /// Angular half-width at the sea-level radius, before catchment scaling.
    pub river_width: f64,
}

impl Default for TerrainOptions {
    fn default() -> Self {
        Self {
            relief: 0.16,
            land_bias: 0.0,
            continental_frequency: 2.2,
            detail_frequency: 12.0,
            erosion_strength: 1.0,
            hydraulic_passes: 1,
            thermal_passes: 8,
            connect_seas: true,
            river_catchment: 0.0015,
            river_depth: 0.0015,
            refinement_passes: 5,
            smoothing_passes: 3,
            river_refinement_passes: 3,
            river_width: 0.003,
        }
    }
}

impl TerrainOptions {
    /// # Errors
    /// Rejects nonfinite/unbounded inputs and displacement through the centre.
    pub fn validate(self) -> Result<(), PlanetError> {
        if self.refinement_passes > 6
            || self.river_refinement_passes > 3
            || self.smoothing_passes > 12
            || !self.river_width.is_finite()
            || !(0.0001..=0.03).contains(&self.river_width)
        {
            return Err(PlanetError::InvalidOptions(
                "global refinement passes must be 0..6, river refinement 0..3, smoothing 0..12, and river width 0.0001..0.03",
            ));
        }
        if !self.relief.is_finite()
            || !self.land_bias.is_finite()
            || self.relief < 0.0
            || self.relief + self.land_bias.abs() > 0.45
        {
            return Err(PlanetError::InvalidOptions(
                "relief plus absolute land bias must be finite and at most 0.45 radii",
            ));
        }
        for frequency in [self.continental_frequency, self.detail_frequency] {
            if !frequency.is_finite() || !(0.01..=100.0).contains(&frequency) {
                return Err(PlanetError::InvalidOptions(
                    "noise frequencies must be between 0.01 and 100",
                ));
            }
        }
        if !self.erosion_strength.is_finite()
            || !(0.0..=4.0).contains(&self.erosion_strength)
            || self.hydraulic_passes > 128
            || self.thermal_passes > 128
        {
            return Err(PlanetError::InvalidOptions(
                "erosion strength must be 0..4 and pass counts at most 128",
            ));
        }
        if !self.river_catchment.is_finite()
            || !(1.0e-6..=1.0).contains(&self.river_catchment)
            || !self.river_depth.is_finite()
            || !(0.0..=0.02).contains(&self.river_depth)
        {
            return Err(PlanetError::InvalidOptions(
                "invalid river catchment fraction or incision depth",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiverSegment {
    pub from: usize,
    pub to: usize,
    pub catchment: f64,
}

#[derive(Debug, PartialEq)]
pub struct PlanetTerrain {
    pub topology: PlanetMesh,
    /// Signed radial elevation / original radius. Sea level is always zero.
    pub elevations: Vec<f64>,
    pub normals: Vec<DVec3>,
    pub bedrock_hardness: Vec<f64>,
    pub soil_depth: Vec<f64>,
    pub drainage: Drainage,
    pub rivers: Vec<RiverSegment>,
    /// Area-weighted relative material removed by drainage/channel incision.
    /// This is treated as sediment exported into the water system.
    pub incised_material: f64,
    /// Height removed by the final cross-section carve, for direct A/B inspection.
    pub channel_depths: Vec<f64>,
    /// Net terrain lift at river banks/supports, kept separately from incision.
    pub bank_lifts: Vec<f64>,
    /// Area-weighted relative material added by net bank/support shaping.
    pub bank_added_material: f64,
    pub water_elevations: Vec<f64>,
    pub river_surface: RiverSurface,
    pub waterfalls: Vec<Waterfall>,
    /// Base mesh count followed by each global and channel refinement count.
    pub refinement_vertices: Vec<usize>,
    pub ocean_passes: Vec<OceanPass>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainStats {
    pub land_vertices: usize,
    pub submerged_vertices: usize,
    pub river_segments: usize,
    pub minimum_elevation: f64,
    pub maximum_elevation: f64,
    pub carved_vertices: usize,
    pub maximum_channel_depth: f64,
    pub water_triangles: usize,
    pub sea_components: usize,
}

/// Full CPU pipeline: spherical topology, continental/detail noise, geology,
/// hydraulic/thermal erosion, drainage, river carving, and final normals.
///
/// # Errors
/// Propagates invalid options, topology failures, and radial/drainage failures.
pub fn generate_terrain(
    planet: PlanetOptions,
    settings: TerrainOptions,
) -> Result<PlanetTerrain, PlanetError> {
    settings.validate()?;
    let mesh = generate(planet)?;
    let elevations = mesh
        .directions
        .iter()
        .map(|&p| {
            let continental = noise::fractal(
                planet.seed ^ 0x243f_6a88_85a3_08d3,
                p * settings.continental_frequency,
                5,
            );
            let detail = noise::fractal(
                planet.seed ^ 0x1319_8a2e_0370_7344,
                p * settings.detail_frequency,
                4,
            );
            settings.land_bias + settings.relief * (0.78 * continental + 0.22 * detail)
        })
        .collect();
    PlanetTerrain::from_elevations(mesh, elevations, planet.seed, settings)
}

impl PlanetTerrain {
    /// Applies the terrain pipeline to explicit relative elevations. Useful for
    /// authored worlds and reproducible drainage fixtures as well as noise.
    ///
    /// # Errors
    /// Rejects invalid settings, mismatched elevations, or invalid final terrain.
    pub fn from_elevations(
        mut topology: PlanetMesh,
        mut elevations: Vec<f64>,
        seed: u64,
        settings: TerrainOptions,
    ) -> Result<Self, PlanetError> {
        settings.validate()?;
        if elevations.len() != topology.directions.len()
            || elevations.iter().any(|h| !h.is_finite() || h.abs() > 0.45)
        {
            return Err(PlanetError::InvalidOptions(
                "one finite relative elevation in [-0.45, 0.45] is required per vertex",
            ));
        }
        topology.validate()?;
        let graph = SurfaceGraph::new(&topology);
        let mut bedrock_hardness = topology
            .directions
            .iter()
            .map(|&p| {
                let sample = (noise::fractal(seed ^ 0xa409_3822_299f_31d0, p * 5.0, 4) + 1.0) * 0.5;
                sample * sample * (3.0 - 2.0 * sample)
            })
            .collect::<Vec<_>>();
        let mut soil_depth = vec![0.002; elevations.len()];
        erosion::erode(
            &topology.directions,
            &graph,
            &mut elevations,
            &bedrock_hardness,
            &mut soil_depth,
            settings,
        );
        let (mut refinement_vertices, mut incised_material, ocean_passes) = refine_stages(
            &mut topology,
            &mut elevations,
            &mut bedrock_hardness,
            &mut soil_depth,
            seed,
            settings,
        )?;
        refine::validate(&topology)?;
        let graph = SurfaceGraph::new(&topology);
        refine::smooth(&graph, &mut elevations, 1.min(settings.smoothing_passes));
        erosion::erode(
            &topology.directions,
            &graph,
            &mut elevations,
            &bedrock_hardness,
            &mut soil_depth,
            settings,
        );
        drop(graph);
        let channels = finish_channels(
            &mut topology,
            &mut elevations,
            &mut bedrock_hardness,
            &mut soil_depth,
            settings,
            &mut refinement_vertices,
        )?;
        refine::validate(&topology)?;
        incised_material += channels.incised_material;
        let normals = normals(&topology, &elevations);
        let terrain = Self {
            topology,
            elevations,
            normals,
            bedrock_hardness,
            soil_depth,
            drainage: channels.drainage,
            rivers: channels.rivers,
            incised_material,
            channel_depths: channels.channel_depths,
            bank_lifts: channels.bank_lifts,
            bank_added_material: channels.bank_added_material,
            water_elevations: channels.water_elevations,
            river_surface: channels.river_surface,
            waterfalls: channels.waterfalls,
            refinement_vertices,
            ocean_passes,
        };
        terrain.validate()?;
        Ok(terrain)
    }

    #[must_use]
    pub fn position(&self, vertex: usize) -> DVec3 {
        self.normalized_position(vertex) * self.topology.radius
    }

    /// Render-space point in units of the original sea-level radius.
    #[must_use]
    pub fn normalized_position(&self, vertex: usize) -> DVec3 {
        self.topology.directions[vertex] * (1.0 + self.elevations[vertex])
    }

    #[must_use]
    pub fn down(&self, vertex: usize) -> DVec3 {
        -self.topology.directions[vertex]
    }

    #[must_use]
    pub fn stats(&self) -> TerrainStats {
        TerrainStats {
            land_vertices: self.elevations.iter().filter(|&&h| h >= 0.0).count(),
            submerged_vertices: self.elevations.iter().filter(|&&h| h < 0.0).count(),
            river_segments: self.rivers.len(),
            minimum_elevation: self
                .elevations
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min),
            maximum_elevation: self
                .elevations
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max),
            carved_vertices: self.channel_depths.iter().filter(|&&d| d > 1.0e-10).count(),
            maximum_channel_depth: self.channel_depths.iter().copied().fold(0.0, f64::max),
            water_triangles: self.river_surface.triangles.len(),
            sea_components: ocean::count(&SurfaceGraph::new(&self.topology), &self.elevations),
        }
    }

    /// Validates radial terrain and every drainage route. Concave terrain must
    /// not be subjected to the original sphere's convex-hull predicate.
    ///
    /// # Errors
    /// Reports nonfinite data, folded faces, or a river that cannot drain to sea.
    pub fn validate(&self) -> Result<(), PlanetError> {
        let count = self.topology.directions.len();
        if [
            self.elevations.len(),
            self.normals.len(),
            self.bedrock_hardness.len(),
            self.soil_depth.len(),
            self.drainage.receivers.len(),
            self.drainage.catchment.len(),
            self.channel_depths.len(),
            self.bank_lifts.len(),
            self.water_elevations.len(),
        ]
        .iter()
        .any(|&n| n != count)
        {
            return Err(PlanetError::InvalidTopology(
                "mismatched terrain field lengths",
            ));
        }
        for i in 0..count {
            if !self.elevations[i].is_finite()
                || 1.0 + self.elevations[i] <= 0.0
                || !self.position(i).is_finite()
                || !self.normals[i].is_finite()
                || (self.normals[i].length_squared() - 1.0).abs() > 1.0e-8
                || self.normals[i].dot(self.topology.directions[i]) <= 0.0
                || !self.bedrock_hardness[i].is_finite()
                || !(0.0..=1.0).contains(&self.bedrock_hardness[i])
                || !self.soil_depth[i].is_finite()
                || self.soil_depth[i] < 0.0
                || !self.bank_lifts[i].is_finite()
                || self.bank_lifts[i] < 0.0
            {
                return Err(PlanetError::InvalidTopology(
                    "invalid radial terrain/material data",
                ));
            }
        }
        for face in &self.topology.triangles {
            let [a, b, c] = face.map(|i| self.normalized_position(i as usize));
            if (b - a).cross(c - a).dot(a + b + c) <= 0.0 {
                return Err(PlanetError::InvalidTopology("inverted radial terrain face"));
            }
        }
        validate_drainage(self)?;
        self.river_surface
            .validate(&self.topology, &self.elevations)?;
        Ok(())
    }
}

struct FinishedChannels {
    drainage: Drainage,
    rivers: Vec<RiverSegment>,
    channel_depths: Vec<f64>,
    bank_lifts: Vec<f64>,
    bank_added_material: f64,
    water_elevations: Vec<f64>,
    river_surface: RiverSurface,
    waterfalls: Vec<Waterfall>,
    incised_material: f64,
}

// Keep the finishing stages together: their order prevents late incision from
// undoing support smoothing and the final upstream bank lifts.
#[allow(clippy::too_many_lines)]
fn finish_channels(
    topology: &mut PlanetMesh,
    elevations: &mut Vec<f64>,
    hardness: &mut Vec<f64>,
    soil_depth: &mut Vec<f64>,
    settings: TerrainOptions,
    refinement_vertices: &mut Vec<usize>,
) -> Result<FinishedChannels, PlanetError> {
    let graph = SurfaceGraph::new(topology);
    let routing = TerrainOptions {
        river_depth: 0.0,
        ..settings
    };
    let mut incised_material = 0.0;
    let (drainage, rivers, removed) = drainage::carve(
        &topology.directions,
        &graph,
        elevations,
        soil_depth,
        routing,
    );
    incised_material += removed;
    let profile = river_profile::build(topology, elevations, &drainage, &rivers, settings);
    let footprints = channels::Footprints::new(topology, &rivers, &profile.levels, settings, 0.0)
        .with_waterfalls(&profile.waterfalls);
    // Freeze the profile/ownership while adding detail, as Island does. Do not
    // re-route or erode between tessellations: that changes the river being refined.
    drop(drainage);
    drop(rivers);
    let mut graph = graph;
    for _ in 0..settings.river_refinement_passes {
        let selected = footprints.refinement_faces(topology, &graph, elevations);
        if !selected.iter().any(|&selected| selected)
            || !refine::subdivision_fits(topology, &selected, refine::MAX_REFINED_VERTICES)
        {
            break;
        }
        refine::subdivide(topology, elevations, hardness, soil_depth, &selected, None)?;
        graph = SurfaceGraph::new(topology);
        refinement_vertices.push(topology.directions.len());
    }
    if settings.river_refinement_passes > 0 {
        for face_detail in [false, true] {
            let selected = footprints.waterfall_refinement_faces(topology, face_detail);
            if !selected.iter().any(|&selected| selected)
                || !refine::subdivision_fits(topology, &selected, refine::MAX_REFINED_VERTICES)
            {
                break;
            }
            refine::subdivide(topology, elevations, hardness, soil_depth, &selected, None)?;
            graph = SurfaceGraph::new(topology);
            refinement_vertices.push(topology.directions.len());
        }
    }
    // Snapshot radial heights for the original sea mask and the B comparison.
    let mut original = elevations.clone();
    footprints.carve(topology, elevations, soil_depth);
    let pins = footprints.bank_pins(topology, &graph, elevations);
    pins.lift(topology, &graph, &original, elevations, soil_depth);
    footprints.relax_bed(topology, &graph, elevations, soil_depth, &pins);
    let protected = footprints.hydraulic_protection(topology);
    pins.smooth_outline(topology, &protected);
    graph = SurfaceGraph::new(topology);
    pins.smooth_waterfall_approach(&graph, elevations, soil_depth);
    // Outline smoothing and bank lifting alter geometry/saddles. Route rainfall
    // on that final terrain, then bind water to the repaired banks.
    let (mut drainage, mut rivers, _) = drainage::carve(
        &topology.directions,
        &graph,
        elevations,
        soil_depth,
        routing,
    );
    // Keep the extra face tessellation local, but smooth the complete river
    // and bank apron so no smoothing boundary remains at a waterfall lip/foot.
    // Hydraulic bounds and upstream lifts preserve the existing downhill paths.
    pins.relax_river_support(&graph, &drainage, elevations, soil_depth);
    // Keep the hydraulic river order stable while the final wet outline and
    // bank grading settle. Waterfall shaping retains its original face run.
    let bank_stations = footprints.bank_stations(topology, &pins);
    // Last height operation: follow each bank upstream and lift dips. Rebuild
    // rainfall routing with a lift-only flood, so neither drainage repair nor
    // smoothing can cut holes back into the finished banks.
    // Inclination binding and grading can expose a new dry outline. Keep
    // checking the final water field until every resulting
    // bank edge is included, retaining all preceding height constraints.
    let mut bank_edges = Vec::new();
    loop {
        let previous = bank_edges.len();
        bank_edges.extend(channels::Footprints::upstream_banks(
            topology,
            &pins,
            &original,
            elevations,
            &bank_stations,
        ));
        bank_edges.sort_unstable();
        bank_edges.dedup();
        if bank_edges.len() == previous {
            if bank_edges
                .iter()
                .any(|&(down, up)| elevations[up] < elevations[down])
            {
                return Err(PlanetError::InvalidTopology(
                    "upstream river bank dip after final water binding",
                ));
            }
            break;
        }
        (drainage, rivers) = drainage::lift_banks(
            &topology.directions,
            &graph,
            elevations,
            soil_depth,
            &bank_edges,
            routing,
        )?;
    }
    if pins
        .sea_openings
        .iter()
        .zip(elevations.iter())
        .any(|(&opening, &height)| opening && height >= 0.0)
    {
        return Err(PlanetError::InvalidTopology(
            "sea-level river opening was refilled",
        ));
    }
    let mut water_elevations = pins.into_water(topology, elevations);
    let previous_vertices = topology.directions.len();
    let contacts = contact_banks::finish(
        topology,
        contact_banks::Fields {
            heights: elevations,
            hardness,
            soil: soil_depth,
            original: &mut original,
            water: &mut water_elevations,
        },
        &footprints,
        &bank_edges,
        drainage,
        rivers,
        routing,
    )?;
    if topology.directions.len() != previous_vertices {
        refinement_vertices.push(topology.directions.len());
    }
    drainage = contacts.drainage;
    rivers = contacts.rivers;
    graph = SurfaceGraph::new(topology);
    let channel_depths: Vec<_> = original
        .iter()
        .zip(elevations.iter())
        .map(|(before, after)| (before - after).max(0.0))
        .collect();
    let bank_lifts: Vec<_> = original
        .iter()
        .zip(elevations.iter())
        .map(|(before, after)| (after - before).max(0.0))
        .collect();
    incised_material += channel_depths
        .iter()
        .zip(&graph.areas)
        .map(|(depth, area)| depth * area)
        .sum::<f64>();
    let bank_added_material = bank_lifts
        .iter()
        .zip(&graph.areas)
        .map(|(lift, area)| lift * area)
        .sum();
    let river_surface = contacts.surface;
    Ok(FinishedChannels {
        drainage,
        rivers,
        channel_depths,
        bank_lifts,
        bank_added_material,
        water_elevations,
        river_surface,
        waterfalls: profile.waterfalls,
        incised_material,
    })
}

fn normals(mesh: &PlanetMesh, elevations: &[f64]) -> Vec<DVec3> {
    let mut normals = vec![DVec3::ZERO; elevations.len()];
    for face in &mesh.triangles {
        let [a, b, c] = face.map(|i| mesh.directions[i as usize] * (1.0 + elevations[i as usize]));
        let normal = (b - a).cross(c - a);
        for &i in face {
            normals[i as usize] += normal;
        }
    }
    for normal in &mut normals {
        *normal = normal.normalize();
    }
    normals
}

fn validate_drainage(terrain: &PlanetTerrain) -> Result<(), PlanetError> {
    let count = terrain.elevations.len();
    let graph = SurfaceGraph::new(&terrain.topology);
    let has_sea = terrain.elevations.iter().any(|&h| h < 0.0);
    if has_sea && terrain.drainage.order.len() != count {
        return Err(PlanetError::InvalidTopology(
            "drainage order must cover the whole sphere",
        ));
    }
    let mut ranks = vec![usize::MAX; count];
    for (rank, &i) in terrain.drainage.order.iter().enumerate() {
        if i >= count || ranks[i] != usize::MAX {
            return Err(PlanetError::InvalidTopology("invalid drainage order"));
        }
        ranks[i] = rank;
    }
    for i in 0..count {
        match terrain.drainage.receivers[i] {
            Some(to)
                if to < count
                    && graph.neighbours[i].contains(&to)
                    && ranks[to] < ranks[i]
                    && terrain.elevations[i] >= 0.0
                    && terrain.elevations[to] < terrain.elevations[i] => {}
            None if terrain.elevations[i] < 0.0 || !has_sea => {}
            _ => {
                return Err(PlanetError::InvalidTopology(
                    "drainage must descend through adjacent vertices to submerged roots",
                ));
            }
        }
        if !terrain.drainage.catchment[i].is_finite() || terrain.drainage.catchment[i] < 0.0 {
            return Err(PlanetError::InvalidTopology("invalid rainfall catchment"));
        }
    }
    for river in &terrain.rivers {
        if river.from >= count
            || terrain.drainage.receivers[river.from] != Some(river.to)
            || !river.catchment.is_finite()
            || (river.catchment - terrain.drainage.catchment[river.from]).abs() > 1.0e-12
        {
            return Err(PlanetError::InvalidTopology(
                "river does not follow drainage",
            ));
        }
    }
    Ok(())
}

fn refine_stages(
    topology: &mut PlanetMesh,
    elevations: &mut Vec<f64>,
    bedrock_hardness: &mut Vec<f64>,
    soil_depth: &mut Vec<f64>,
    seed: u64,
    settings: TerrainOptions,
) -> Result<(Vec<usize>, f64, Vec<OceanPass>), PlanetError> {
    let mut refinement_vertices = vec![topology.directions.len()];
    let mut ocean_passes = Vec::new();
    let mut incised_material = 0.0;
    for pass in 0..settings.refinement_passes {
        let selected = if pass < 2 {
            vec![true; topology.triangles.len()]
        } else {
            refine::curved_faces(topology, elevations)
        };
        if !selected.iter().any(|&selected| selected) {
            break;
        }
        // Reserve half the total budget for final river-corridor refinement.
        // Stop optional adaptive stages before allocation, retaining a closed
        // mesh at the previous level instead of failing a larger base sphere.
        if pass >= 2
            && !refine::subdivision_fits(topology, &selected, refine::MAX_REFINED_VERTICES / 2)
        {
            break;
        }
        refine::subdivide(
            topology,
            elevations,
            bedrock_hardness,
            soil_depth,
            &selected,
            (pass < 2).then_some((seed.wrapping_add(u64::from(pass) + 900), settings)),
        )?;
        refine::relax(topology, elevations, soil_depth, settings.smoothing_passes);
        let graph = SurfaceGraph::new(topology);
        erosion::erode(
            &topology.directions,
            &graph,
            elevations,
            bedrock_hardness,
            soil_depth,
            settings,
        );
        refinement_vertices.push(topology.directions.len());
        if settings.connect_seas && (pass == 0 || pass == 2) {
            let stage = if pass == 0 {
                OceanStage::Lod2
            } else {
                OceanStage::Lod1
            };
            let report = ocean::connect(topology, &graph, elevations, soil_depth, stage)?;
            incised_material += report.removed_material;
            ocean_passes.push(report);
        }
    }
    Ok((refinement_vertices, incised_material, ocean_passes))
}
