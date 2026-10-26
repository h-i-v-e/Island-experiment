//! Make the clipped river contact part of the terrain before grading it. Water
//! and terrain then share the same vertices; no later normal blend can move it.
use super::{
    Drainage, RiverSegment, TerrainOptions,
    channels::{BankStation, Footprints},
    drainage,
    graph::SurfaceGraph,
    refine,
    water_mesh::{self, BuiltSurface, RiverSurface, Source},
};
use crate::{PlanetError, PlanetMesh};
use std::collections::BTreeMap;

pub(super) struct Fields<'a> {
    pub heights: &'a mut Vec<f64>,
    pub hardness: &'a mut Vec<f64>,
    pub soil: &'a mut Vec<f64>,
    pub original: &'a mut Vec<f64>,
    pub water: &'a mut Vec<f64>,
}

pub(super) struct Finished {
    pub surface: RiverSurface,
    pub drainage: Drainage,
    pub rivers: Vec<RiverSegment>,
}

fn upstream_edge(a: usize, b: usize, stations: &[Option<BankStation>]) -> Option<(usize, usize)> {
    let (first, second) = (stations[a]?, stations[b]?);
    if first.outlet != second.outlet {
        return None;
    }
    let delta = first.distance - second.distance;
    if delta > 1.0e-12 {
        Some((b, a))
    } else if delta < -1.0e-12 {
        Some((a, b))
    } else {
        None
    }
}

/// Preserve the original blend/clearance while lifting the explicit contact.
#[allow(clippy::too_many_lines)]
pub(super) fn finish(
    mesh: &mut PlanetMesh,
    mut fields: Fields<'_>,
    footprints: &Footprints<'_>,
    banks: &[(usize, usize)],
    old_drainage: Drainage,
    old_rivers: Vec<RiverSegment>,
    settings: TerrainOptions,
) -> Result<Finished, PlanetError> {
    let built = water_mesh::build_with_sources(mesh, fields.heights, fields.water);
    let edges = built.surface.dry_bank_edges();
    let mut stations = vec![None; built.surface.positions.len()];
    for &(a, b) in &edges {
        for id in [a, b] {
            let id = id as usize;
            if stations[id].is_none() {
                stations[id] = footprints.bank_station_at(built.surface.positions[id].normalize());
            }
            if stations[id].is_none() {
                return Err(PlanetError::InvalidTopology(
                    "river contact has no hydraulic coordinate",
                ));
            }
        }
    }
    let needs_grading = edges.iter().any(|&(a, b)| {
        upstream_edge(a as usize, b as usize, &stations).is_some_and(|(down, up)| {
            built.surface.positions[up].length() + 1.0e-10 < built.surface.positions[down].length()
        })
    });
    if !needs_grading {
        return Ok(Finished {
            surface: built.surface,
            drainage: old_drainage,
            rivers: old_rivers,
        });
    }
    drop(old_drainage);
    drop(old_rivers);
    let contacts = insert_contacts(mesh, &mut fields, &built, &edges)?;
    let mut constraints = banks.to_vec();
    let mut frozen = vec![None; mesh.directions.len()];
    for &(a, b) in &edges {
        for id in [a, b] {
            let vertex = contacts[&(id as usize)];
            frozen[vertex] = stations[id as usize];
        }
        if let Some((down, up)) = upstream_edge(a as usize, b as usize, &stations) {
            let (down, up) = (contacts[&down], contacts[&up]);
            if down != up {
                constraints.push((down, up));
            }
        }
    }
    constraints.sort_unstable();
    constraints.dedup();
    // A snapshot is required to carry exactly the old water clearance through
    // the lift-only flood; recomputing normals here would recreate the dips.
    let before = fields.heights.clone();
    let graph = SurfaceGraph::new(mesh);
    let (drainage, rivers) = drainage::lift_banks(
        &mesh.directions,
        &graph,
        fields.heights,
        fields.soil,
        &constraints,
        settings,
    )?;
    for ((water, &height), &old) in fields
        .water
        .iter_mut()
        .zip(fields.heights.iter())
        .zip(&before)
    {
        *water += height - old;
    }
    let final_surface = water_mesh::build_with_sources(mesh, fields.heights, fields.water);
    for (a, b) in final_surface.surface.dry_bank_edges() {
        let (Source::Terrain(a), Source::Terrain(b)) = (
            final_surface.sources[a as usize],
            final_surface.sources[b as usize],
        ) else {
            return Err(PlanetError::InvalidTopology(
                "river contact escaped inserted bank vertices",
            ));
        };
        if let Some((down, up)) = upstream_edge(a, b, &frozen)
            && fields.heights[up] + 1.0e-10 < fields.heights[down]
        {
            return Err(PlanetError::InvalidTopology(
                "upstream dip on final river contact",
            ));
        }
    }
    Ok(Finished {
        surface: final_surface.surface,
        drainage,
        rivers,
    })
}

fn edge_source(source: Source, mesh: &PlanetMesh, heights: &[f64]) -> Source {
    let Source::Edge {
        vertices: [a, b],
        fraction,
    } = source
    else {
        return source;
    };
    let start = mesh.directions[a as usize] * (1.0 + heights[a as usize]);
    let end = mesh.directions[b as usize] * (1.0 + heights[b as usize]);
    let point = start.lerp(end, fraction);
    if point.distance_squared(start) <= 1.0e-20 {
        Source::Terrain(a as usize)
    } else if point.distance_squared(end) <= 1.0e-20 {
        Source::Terrain(b as usize)
    } else if a < b {
        source
    } else {
        Source::Edge {
            vertices: [b, a],
            fraction: 1.0 - fraction,
        }
    }
}

fn insert_contacts(
    mesh: &mut PlanetMesh,
    fields: &mut Fields<'_>,
    built: &BuiltSurface,
    edges: &[(u32, u32)],
) -> Result<BTreeMap<usize, usize>, PlanetError> {
    let mut sources = BTreeMap::new();
    let mut fractions = BTreeMap::new();
    for &(a, b) in edges {
        for id in [a, b] {
            let source = edge_source(built.sources[id as usize], mesh, fields.heights);
            if let Source::Edge {
                vertices: [a, b],
                fraction,
            } = source
            {
                fractions.insert((a, b), fraction);
            }
            sources.insert(id as usize, source);
        }
    }
    let first = mesh.directions.len();
    let splits: BTreeMap<_, _> = fractions
        .iter()
        .enumerate()
        .map(|(offset, (&edge, &t))| (edge, (first + offset, t)))
        .collect();
    refine::split_edges(
        mesh,
        fields.heights,
        fields.hardness,
        fields.soil,
        fractions,
        None,
    )?;
    for (&(a, b), &(vertex, t)) in &splits {
        let (a, b) = (a as usize, b as usize);
        let start = mesh.directions[a] * (1.0 + fields.heights[a]);
        let end = mesh.directions[b] * (1.0 + fields.heights[b]);
        let point = start.lerp(end, t);
        mesh.directions[vertex] = point.normalize();
        fields.heights[vertex] = point.length() - 1.0;
        fields
            .original
            .push(fields.original[a] + (fields.original[b] - fields.original[a]) * t);
        fields.water.push(fields.heights[vertex]);
    }
    sources
        .into_iter()
        .map(|(id, source)| {
            let vertex = match source {
                Source::Terrain(vertex) => vertex,
                Source::Edge {
                    vertices: [a, b], ..
                } => splits[&(a, b)].0,
                Source::Sea => {
                    return Err(PlanetError::InvalidTopology(
                        "sea contact selected as a dry river bank",
                    ));
                }
            };
            fields.water[vertex] = fields.heights[vertex];
            Ok((id, vertex))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DVec3;

    fn dips(surface: &RiverSurface, footprints: &Footprints<'_>) -> usize {
        let stations: Vec<_> = surface
            .positions
            .iter()
            .map(|point| footprints.bank_station_at(point.normalize()))
            .collect();
        surface
            .dry_bank_edges()
            .into_iter()
            .filter(|&(a, b)| {
                upstream_edge(a as usize, b as usize, &stations).is_some_and(|(down, up)| {
                    surface.positions[up].length() + 1.0e-10 < surface.positions[down].length()
                })
            })
            .count()
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::float_cmp)] // Complete radial contact fixture and exact sea preservation.
    fn clipped_contacts_are_graded_without_floating_or_losing_the_thin_face_clearance() {
        for pole in [1.0, -1.0] {
            let mut mesh = PlanetMesh {
                directions: Vec::new(),
                triangles: Vec::new(),
                radius: 1000.0,
            };
            let mut heights = Vec::new();
            let mut water = Vec::new();
            for row in 0..7 {
                for col in 0..5 {
                    mesh.directions.push(
                        DVec3::new(f64::from(row) * 0.001, f64::from(col - 2) * 0.001, pole)
                            .normalize(),
                    );
                    let height = if row == 0 || row == 6 || col == 0 || col == 4 {
                        -0.005
                    } else if col == 2 {
                        0.02
                    } else {
                        0.025
                    };
                    heights.push(height);
                    water.push(if height < 0.0 {
                        height - 0.001
                    } else if col == 2 && row == 3 {
                        height + super::super::banks::FALL_CLEARANCE
                    } else if col == 2 {
                        0.021
                    } else if row % 2 == 0 {
                        0.0249
                    } else {
                        0.0216
                    });
                }
            }
            for row in 0..6 {
                for col in 0..4 {
                    let a = row * 5 + col;
                    let faces = [[a, a + 5, a + 1], [a + 1, a + 5, a + 6]];
                    mesh.triangles.extend(faces.map(|mut f| {
                        if pole < 0.0 {
                            f.swap(1, 2);
                        }
                        f
                    }));
                }
            }
            let settings = TerrainOptions {
                river_depth: 0.0,
                river_width: 0.0015,
                ..TerrainOptions::default()
            };
            let threshold = settings.river_catchment * 4.0 * std::f64::consts::PI;
            let rivers: Vec<_> = (1..5)
                .map(|row| RiverSegment {
                    from: row * 5 + 2,
                    to: (row + 1) * 5 + 2,
                    catchment: threshold,
                })
                .collect();
            let levels = vec![0.0215; heights.len()];
            let footprints = Footprints::new(&mesh, &rivers, &levels, settings, 0.0);
            let mut hardness = vec![0.5; heights.len()];
            let mut soil = vec![0.002; heights.len()];
            let graph = SurfaceGraph::new(&mesh);
            let (drainage, _, _) =
                drainage::carve(&mesh.directions, &graph, &mut heights, &mut soil, settings);
            let before_surface = water_mesh::build(&mesh, &heights, &water);
            assert!(
                dips(&before_surface, &footprints) > 0,
                "the interpolated contacts dip despite perfectly level outer banks"
            );
            let before = heights.clone();
            let clearance: Vec<_> = water.iter().zip(&heights).map(|(w, h)| w - h).collect();
            let mut original = heights.clone();
            let banks: Vec<_> = (1..5)
                .flat_map(|row| [1, 3].map(move |col| ((row + 1) * 5 + col, row * 5 + col)))
                .collect();
            let result = finish(
                &mut mesh,
                Fields {
                    heights: &mut heights,
                    hardness: &mut hardness,
                    soil: &mut soil,
                    original: &mut original,
                    water: &mut water,
                },
                &footprints,
                &banks,
                drainage,
                rivers,
                settings,
            )
            .unwrap();
            assert!(
                mesh.directions.len() > before.len(),
                "clipped contacts must become real shared terrain vertices"
            );
            assert_eq!(dips(&result.surface, &footprints), 0);
            result.surface.validate(&mesh, &heights).unwrap();
            for i in 0..before.len() {
                assert!(heights[i] >= before[i], "contact grading is lift-only");
                if before[i] < 0.0 {
                    assert_eq!(heights[i], before[i], "sea openings stay unchanged");
                }
                if i % 5 == 2 && (1..=5).contains(&(i / 5)) {
                    assert!(
                        (water[i] - heights[i] - clearance[i]).abs() < 1.0e-12,
                        "carry the river depth and 3cm face gap through the lift: vertex {i}, old clearance {}, new clearance {}, before {}, after {}",
                        clearance[i],
                        water[i] - heights[i],
                        before[i],
                        heights[i]
                    );
                }
            }
            assert_eq!(mesh.directions.len(), heights.len());
            assert_eq!(heights.len(), original.len());
            assert_eq!(heights.len(), water.len());
            assert_eq!(heights.len(), hardness.len());
            assert_eq!(heights.len(), soil.len());
        }
    }
}
