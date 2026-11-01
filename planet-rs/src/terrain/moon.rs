//! Dry impact geology on a sphere. Feature positions are independent of mesh
//! density, with old impacts resurfaced by basalt and younger craters on top.
use super::{feature_index::FeatureIndex, random::Random, refine};
use crate::{DVec3, PlanetError, PlanetMesh, PlanetOptions, generate, noise};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MoonOptions {
    /// Vertical exaggeration, relative to the reference lunar recipe.
    pub relief: f64,
    /// Uniform dry-surface subdivisions; independent of the wet planet stages.
    pub refinements: u32,
    pub crater_density: f64,
    /// Fraction of the large impact basins flooded by dark basalt.
    pub maria: f64,
    /// How strongly the oldest craters have softened through later impacts.
    pub weathering: f64,
}
impl Default for MoonOptions {
    fn default() -> Self {
        Self {
            relief: 1.0,
            refinements: 3,
            crater_density: 1.0,
            maria: 0.65,
            weathering: 0.65,
        }
    }
}
impl MoonOptions {
    /// # Errors
    /// Rejects nonfinite or excessive lunar relief and feature controls.
    pub fn validate(self) -> Result<(), PlanetError> {
        if self.refinements > 5 {
            return Err(PlanetError::InvalidOptions("moon refinements must be 0..5"));
        }
        for (value, min, max) in [
            (self.relief, 0.0, 2.0),
            (self.crater_density, 0.0, 3.0),
            (self.maria, 0.0, 1.0),
            (self.weathering, 0.0, 1.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(PlanetError::InvalidOptions(
                    "invalid moon relief, crater density, maria or weathering",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct Basin {
    centre: DVec3,
    radius: f64,
    basalt: bool,
}
#[derive(Clone, Copy, Debug)]
struct Crater {
    centre: DVec3,
    radius: f64,
    age: f64,
    phase: f64,
    reference: f64,
}

/// A sample owns no heap data. Height is relative to the moon's own radius;
/// albedo is linear RGB and can be baked at any mesh/texture resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoonSample {
    pub height: f64,
    pub basalt: f64,
    pub ejecta: f64,
    pub albedo: [f32; 4],
}

/// Construct once per seed and borrow for mesh, material and collision samples.
pub struct MoonField {
    seed: u64,
    options: MoonOptions,
    basins: Vec<Basin>,
    craters: Vec<Crater>,
    index: FeatureIndex,
}
impl MoonField {
    /// # Errors
    /// Rejects invalid recipe controls.
    pub fn new(seed: u64, options: MoonOptions) -> Result<Self, PlanetError> {
        options.validate()?;
        let mut random = Random::new(seed ^ 0xa38e_5642_d709_c1f0);
        let province = random.direction();
        let basins = (0..7)
            .map(|i| Basin {
                // More flooded basins on one hemisphere; no latitude/UV seam.
                centre: if i < 5 {
                    (province * 0.65 + random.direction()).normalize()
                } else {
                    random.direction()
                },
                radius: 0.3 + random.unit() * 0.32,
                basalt: random.unit() < options.maria,
            })
            .collect();
        let mut field = Self {
            seed,
            options,
            basins,
            craters: Vec::new(),
            index: FeatureIndex::new([]),
        };
        // Large old craters precede smaller impacts. Each logarithmic octave
        // contains more craters, giving a few landmarks and many small bowls.
        for (count, low, high) in [
            (26, 0.12_f64, 0.26_f64),
            (90, 0.05, 0.12),
            (300, 0.018, 0.05),
            (850, 0.006, 0.018),
        ] {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let count = (f64::from(count) * options.crater_density).round() as usize;
            for _ in 0..count {
                let centre = random.direction();
                field.craters.push(Crater {
                    centre,
                    radius: low * (high / low).powf(random.unit()),
                    age: random.unit(),
                    phase: random.unit() * TAU,
                    reference: field.substrate(centre).0,
                });
            }
        }
        field.index = FeatureIndex::new(field.craters.iter().map(|c| (c.centre, c.radius * 3.0)));
        Ok(field)
    }

    #[must_use]
    pub fn crater_count(&self) -> usize {
        self.craters.len()
    }

    fn substrate(&self, p: DVec3) -> (f64, f64) {
        let mut height = 0.004 + 0.002 * noise::fractal(self.seed, p * 5.0, 3);
        let mut basalt: f64 = 0.0;
        for basin in &self.basins {
            let q = angle(p, basin.centre) / basin.radius;
            if q >= 1.65 {
                continue;
            }
            let interior = 1.0 - smooth(0.65, 1.1, q);
            height -= 0.023 * interior;
            height += 0.009 * bump((q - 1.0) / 0.18) + 0.003 * bump((q - 1.32) / 0.15);
            if basin.basalt {
                basalt = basalt.max(1.0 - smooth(0.68, 0.91, q));
            }
        }
        (height, basalt)
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // Bounded linear albedo for GPU input.
    pub fn sample(&self, p: DVec3) -> MoonSample {
        let p = p.normalize_or_zero();
        let (mut height, basalt) = self.substrate(p);
        let mut ejecta: f64 = 0.0;
        let mut excavation: f64 = 0.0;
        for &id in self.index.at(p) {
            let c = self.craters[id];
            let q = angle(p, c.centre) / c.radius;
            if q >= 3.0 {
                continue;
            }
            let old = c.age * self.options.weathering;
            // Flooded maria bury old small craters; fresh impacts remain sharp.
            let preserved = 1.0 - basalt * smooth(0.35, 0.7, c.age) * 0.94;
            let depth = c.radius * 0.19 * (1.0 - 0.65 * old);
            let floor = if c.radius > 0.075 {
                // Broad floors, slumped wall terraces, and central uplift.
                -depth * (1.0 - smooth(0.45, 1.0, q))
                    + depth * 0.35 * bump(q / 0.22)
                    + depth * 0.045 * (q * PI * 8.0).sin() * bump((q - 0.72) / 0.28)
            } else {
                -depth * (1.0 - q.min(1.0).powi(2)).powi(2)
            };
            let inside = 1.0 - smooth(0.86, 1.0, q);
            // A later impact excavates the existing surface. Anchoring its
            // floor back to the uncratered substrate would lift cones inside
            // older bowls and flooded basins when impacts overlap.
            let floor_height = c.reference.min(height) + floor;
            height += ((floor_height - height) * inside
                + c.radius
                    * (0.037 * (1.0 - old * 0.5) * bump((q - 1.0) / (0.13 + old * 0.15))
                        + 0.009 * bump((q - 1.24) / 0.65)))
                * preserved;
            let tangent = p - c.centre * p.dot(c.centre);
            let axis = c.centre.any_orthonormal_vector();
            let azimuth = tangent.dot(c.centre.cross(axis)).atan2(tangent.dot(axis));
            let rays = ((azimuth * 11.0 + c.phase).sin() * 0.5 + 0.5).powi(12);
            let ray = rays * (1.0 - smooth(1.0, 3.0, q)) * smooth(0.9, 1.15, q);
            ejecta = ejecta.max((0.35 * bump((q - 1.0) / 0.25) + ray) * (1.0 - c.age) * preserved);
            excavation = excavation.max((1.0 - smooth(0.0, 1.05, q)) * (1.0 - c.age));
        }
        // Smooth basalt reduces regolith roughness; no water erosion or rivers.
        height += 0.0008 * (1.0 - basalt * 0.85) * noise::fractal(self.seed ^ 0x91f2, p * 90.0, 3);
        height = 0.12 * (height * self.options.relief / 0.12).tanh();
        let tone = (0.18 * (1.0 - basalt) + 0.065 * basalt + ejecta * 0.10 + excavation * 0.025)
            .clamp(0.045, 0.36);
        MoonSample {
            height,
            basalt,
            ejecta,
            albedo: [tone as f32, (tone * 0.98) as f32, (tone * 0.94) as f32, 1.0],
        }
    }
}
fn angle(a: DVec3, b: DVec3) -> f64 {
    a.cross(b).length().atan2(a.dot(b))
}
fn smooth(low: f64, high: f64, x: f64) -> f64 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn bump(x: f64) -> f64 {
    if x.abs() >= 1.0 {
        0.0
    } else {
        (1.0 - x * x).powi(2)
    }
}

pub struct MoonMesh {
    pub topology: PlanetMesh,
    pub heights: Vec<f64>,
    pub normals: Vec<DVec3>,
    pub albedo: Vec<[f32; 4]>,
}

/// Refine the full dry surface, sampling analytic impacts after subdivision so
/// fine craters are never smoothed away or skipped as submerged ocean floor.
/// # Errors
/// Propagates invalid recipe, topology or excessive subdivision settings.
pub fn generate_moon(
    planet: PlanetOptions,
    options: MoonOptions,
    refinements: u32,
) -> Result<MoonMesh, PlanetError> {
    if refinements > 5 {
        return Err(PlanetError::InvalidOptions("moon refinements must be 0..5"));
    }
    let field = MoonField::new(planet.seed, options)?;
    let mut mesh = generate(planet)?;
    let mut heights = vec![0.0; mesh.directions.len()];
    let mut unused_hardness = vec![0.0; mesh.directions.len()];
    let mut unused_soil = vec![0.0; mesh.directions.len()];
    for _ in 0..refinements {
        let selected = vec![true; mesh.triangles.len()];
        refine::subdivide(
            &mut mesh,
            &mut heights,
            &mut unused_hardness,
            &mut unused_soil,
            &selected,
            None,
        )?;
    }
    let mut albedo = Vec::with_capacity(mesh.directions.len());
    for (&p, height) in mesh.directions.iter().zip(&mut heights) {
        let sample = field.sample(p);
        *height = sample.height;
        albedo.push(sample.albedo);
    }
    let normals = super::normals(&mesh, &heights);
    Ok(MoonMesh {
        topology: mesh,
        heights,
        normals,
        albedo,
    })
}

pub(super) fn generate_terrain(
    planet: PlanetOptions,
    options: MoonOptions,
) -> Result<super::PlanetTerrain, PlanetError> {
    let mesh = generate_moon(planet, options, options.refinements)?;
    let count = mesh.heights.len();
    let terrain = super::PlanetTerrain {
        topology: mesh.topology,
        elevations: mesh.heights,
        normals: mesh.normals,
        lunar_albedo: mesh.albedo,
        bedrock_hardness: vec![0.8; count],
        soil_depth: vec![0.0005; count],
        drainage: super::Drainage {
            receivers: vec![None; count],
            catchment: vec![0.0; count],
            order: Vec::new(),
        },
        rivers: Vec::new(),
        incised_material: 0.0,
        channel_depths: vec![0.0; count],
        bank_lifts: vec![0.0; count],
        bank_added_material: 0.0,
        water_elevations: vec![0.0; count],
        river_surface: super::RiverSurface::default(),
        waterfalls: Vec::new(),
        refinement_vertices: (0..=options.refinements)
            .map(|i| (planet.vertex_count - 2) * 4_usize.pow(i) + 2)
            .collect(),
        ocean_passes: Vec::new(),
        volcanoes: Vec::new(),
        impact_craters: Vec::new(),
    };
    terrain.validate()?;
    Ok(terrain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lunar_samples_are_seeded_bounded_and_have_dark_maria_and_bright_ejecta() {
        let a = MoonField::new(42, MoonOptions::default()).unwrap();
        let b = MoonField::new(42, MoonOptions::default()).unwrap();
        let other = MoonField::new(43, MoonOptions::default()).unwrap();
        let mut random = Random::new(13);
        let mut maria = 0;
        let mut rays = 0;
        for _ in 0..4000 {
            let p = random.direction();
            let sample = a.sample(p);
            assert_eq!(sample, b.sample(p));
            assert!(sample.height.is_finite() && sample.height.abs() < 0.12);
            assert!(
                sample
                    .albedo
                    .iter()
                    .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
            );
            maria += usize::from(sample.basalt > 0.8);
            rays += usize::from(sample.ejecta > 0.2);
        }
        assert!(
            maria > 100 && maria < 1600,
            "maria should be local basins, not a global coating: {maria}"
        );
        assert!(rays > 5, "young ejecta rays must survive resurfacing");
        assert_ne!(a.sample(DVec3::X), other.sample(DVec3::X));
    }

    #[test]
    fn crater_has_a_depressed_floor_raised_rim_and_no_discontinuity_at_outer_edge() {
        let mut field = MoonField::new(
            1,
            MoonOptions {
                crater_density: 0.0,
                ..MoonOptions::default()
            },
        )
        .unwrap();
        field.basins.clear();
        field.craters.push(Crater {
            centre: DVec3::Y,
            radius: 0.1,
            age: 0.0,
            phase: 0.0,
            reference: 0.004,
        });
        field.index = FeatureIndex::new([(DVec3::Y, 0.3)]);
        let p = |angle: f64| DVec3::new(angle.sin(), angle.cos(), 0.0);
        assert!(field.sample(DVec3::Y).height < -0.007);
        assert!(field.sample(p(0.1)).height > 0.006);
        assert!(
            (field.sample(p(0.3 - 1e-8)).height - field.sample(p(0.3 + 1e-8)).height).abs() < 1e-7
        );
    }

    #[test]
    fn controls_change_geology_without_moving_impacts_or_adding_water() {
        let mut a = MoonField::new(42, MoonOptions::default()).unwrap();
        let b = MoonField::new(
            42,
            MoonOptions {
                maria: 0.0,
                ..MoonOptions::default()
            },
        )
        .unwrap();
        assert_eq!(a.crater_count(), 1266);
        assert_eq!(a.craters[20].centre, b.craters[20].centre);
        let centre = a.basins.iter().find(|b| b.basalt).unwrap().centre;
        assert!(a.sample(centre).basalt > 0.9);
        assert_eq!(b.sample(centre).basalt.to_bits(), 0.0_f64.to_bits());
        a.options.relief = 0.0;
        assert!(a.sample(centre).height.abs() < 1e-12);
    }

    #[test]
    fn dry_mesh_refines_basin_floors_and_keeps_geometry_identical_across_physical_radii() {
        let options = PlanetOptions {
            vertex_count: 64,
            radius: 350.0,
            ..PlanetOptions::default()
        };
        let a = generate_moon(options, MoonOptions::default(), 2).unwrap();
        let b = generate_moon(
            PlanetOptions {
                radius: 700.0,
                ..options
            },
            MoonOptions::default(),
            2,
        )
        .unwrap();
        assert_eq!(a.heights, b.heights);
        assert_eq!(a.topology.directions, b.topology.directions);
        assert_eq!(a.albedo, b.albedo);
        assert_eq!(a.heights.len(), 994);
        assert!(a.heights.iter().any(|h| *h < 0.0));
        assert!(
            a.normals
                .iter()
                .all(|n| n.is_finite() && (n.length() - 1.0).abs() < 1e-10)
        );
        refine::validate(&a.topology).unwrap();
    }

    #[test]
    fn retired_noise_settings_migrate_to_tectonics_and_invalid_moon_settings_fail() {
        // serde compatibility is also checked by the viewer preference tests.
        assert_eq!(
            super::super::TerrainSource::default(),
            super::super::TerrainSource::Tectonics
        );
        assert!("noise".parse::<super::super::TerrainSource>().is_err());
        for invalid in [f64::NAN, -1.0, 4.0] {
            assert!(
                MoonField::new(
                    42,
                    MoonOptions {
                        crater_density: invalid,
                        ..MoonOptions::default()
                    }
                )
                .is_err()
            );
        }
    }
    #[test]
    fn lunar_recipe_bypasses_wet_stages_and_retains_dry_basin_collision() {
        let options = MoonOptions {
            refinements: 1,
            ..MoonOptions::default()
        };
        let planet = PlanetOptions {
            vertex_count: 128,
            radius: 350.0,
            ..PlanetOptions::default()
        };
        let expected = generate_moon(planet, options, 1).unwrap();
        let terrain = crate::generate_terrain(
            planet,
            super::super::TerrainOptions {
                source: super::super::TerrainSource::Moon,
                moon: options,
                erosion_strength: 8.0,
                hydraulic_passes: 128,
                thermal_passes: 128,
                ..super::super::TerrainOptions::default()
            },
        )
        .unwrap();
        assert!(terrain.is_moon());
        assert_eq!(terrain.elevations, expected.heights);
        assert!(terrain.elevations.iter().any(|h| *h < 0.0));
        assert!(terrain.rivers.is_empty() && terrain.waterfalls.is_empty());
        assert!(terrain.ocean_passes.is_empty());
        assert!(terrain.channel_depths.iter().all(|d| *d == 0.0));
        assert_eq!(terrain.stats().submerged_vertices, 0);
        terrain.validate().unwrap();
    }
    #[test]
    fn overlapping_young_impacts_do_not_lift_mounds_from_old_basin_floors() {
        let mut field = MoonField::new(
            1,
            MoonOptions {
                crater_density: 0.0,
                ..MoonOptions::default()
            },
        )
        .unwrap();
        field.basins.clear();
        field.craters.push(Crater {
            centre: DVec3::Y,
            radius: 0.2,
            age: 0.7,
            phase: 0.0,
            reference: 0.004,
        });
        field.index = FeatureIndex::new([(DVec3::Y, 0.6)]);
        let before = field.sample(DVec3::Y).height;
        field.craters.push(Crater {
            centre: DVec3::Y,
            radius: 0.03,
            age: 0.0,
            phase: 0.0,
            reference: 0.004,
        });
        field.index = FeatureIndex::new([(DVec3::Y, 0.6), (DVec3::Y, 0.09)]);
        assert!(field.sample(DVec3::Y).height < before - 0.004);
    }
}
