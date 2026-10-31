//! Retain the largest submerged component as the global ocean and fill inland
//! seas before final river carving. Sea level remains the original sphere.
use super::graph::SurfaceGraph;
use crate::PlanetMesh;

const LAND: usize = usize::MAX;
const MINIMUM_FILL_METRES: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OceanStage {
    Initial,
    InitialErosion,
    Impact(u32),
    Lod2,
    Lod1,
    Refinement(u32),
    FinalTerrain,
}

/// Record even no-op passes so callers can verify every enabled terrain stage.
pub(super) fn fill_stage(
    mesh: &PlanetMesh,
    graph: &SurfaceGraph,
    heights: &mut [f64],
    soil: &mut [f64],
    enabled: bool,
    stage: OceanStage,
    reports: &mut Vec<OceanPass>,
) {
    if enabled {
        reports.push(fill(mesh, graph, heights, soil, stage));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanPass {
    pub stage: OceanStage,
    pub vertices: usize,
    pub seas_before: usize,
    pub seas_after: usize,
    pub filled_seas: usize,
    pub filled_vertices: usize,
    /// Area-weighted relative height added as loose basin fill, not incision.
    pub added_material: f64,
}

struct Seas {
    labels: Vec<usize>,
    areas: Vec<f64>,
}
impl Seas {
    fn new(graph: &SurfaceGraph, heights: &[f64]) -> Self {
        let mut labels = vec![LAND; heights.len()];
        let mut areas = Vec::new();
        let mut stack = Vec::new();
        for start in 0..heights.len() {
            if heights[start] >= 0.0 || labels[start] != LAND {
                continue;
            }
            let label = areas.len();
            let mut area = 0.0;
            labels[start] = label;
            stack.push(start);
            while let Some(i) = stack.pop() {
                area += graph.areas[i];
                for &j in &graph.neighbours[i] {
                    if heights[j] < 0.0 && labels[j] == LAND {
                        labels[j] = label;
                        stack.push(j);
                    }
                }
            }
            areas.push(area);
        }
        Self { labels, areas }
    }

    fn largest(&self) -> Option<usize> {
        // Stable first-component tie break. Area is spherical, independent of
        // physical planet radius and local tessellation density.
        self.areas
            .iter()
            .enumerate()
            .fold(None, |largest, (id, area)| match largest {
                Some(previous) if *area <= self.areas[previous] => Some(previous),
                _ => Some(id),
            })
    }
}

pub(super) fn count(graph: &SurfaceGraph, heights: &[f64]) -> usize {
    Seas::new(graph, heights).areas.len()
}

fn fill(
    mesh: &PlanetMesh,
    graph: &SurfaceGraph,
    heights: &mut [f64],
    soil: &mut [f64],
    stage: OceanStage,
) -> OceanPass {
    let seas = Seas::new(graph, heights);
    let mut report = OceanPass {
        stage,
        vertices: heights.len(),
        seas_before: seas.areas.len(),
        seas_after: seas.areas.len(),
        filled_seas: seas.areas.len().saturating_sub(1),
        filled_vertices: 0,
        added_material: 0.0,
    };
    if seas.areas.len() <= 1 {
        return report;
    }
    let ocean = seas.largest().expect("at least two submerged components");
    let floor = (MINIMUM_FILL_METRES / mesh.radius).next_up();
    for (i, &label) in seas.labels.iter().enumerate() {
        if label != LAND && label != ocean {
            let added = floor - heights[i];
            heights[i] = floor;
            soil[i] += added;
            report.filled_vertices += 1;
            report.added_material += added * graph.areas[i];
        }
    }
    report.seas_after = 1;
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlanetOptions, generate};

    #[test]
    fn largest_ocean_is_selected_by_area_instead_of_vertex_count() {
        // A densely tessellated inland sea has more sites but less area than
        // the one-site ocean. Land between them must never be cut or raised.
        let mesh = PlanetMesh {
            directions: vec![crate::DVec3::Z; 6],
            triangles: Vec::new(),
            radius: 1000.0,
        };
        let graph = SurfaceGraph {
            neighbours: vec![
                vec![1],
                vec![0, 2],
                vec![1, 3],
                vec![2, 4],
                vec![3, 5],
                vec![4],
            ],
            areas: vec![0.2, 0.2, 0.2, 1.0, 2.0, 1.0],
        };
        let original = [-0.01, -0.02, -0.01, 0.08, -0.1, 0.02];
        let mut heights = original;
        let mut soil = [0.002; 6];
        let report = fill(&mesh, &graph, &mut heights, &mut soil, OceanStage::Initial);
        assert_eq!(report.seas_before, 2);
        assert_eq!(report.seas_after, 1);
        assert_eq!(report.filled_seas, 1);
        assert_eq!(report.filled_vertices, 3);
        assert_eq!(&heights[3..], &original[3..]);
        assert_eq!(&soil[3..], &[0.002; 3]);
        for i in 0..3 {
            assert!(heights[i] * mesh.radius >= 0.5);
            assert!((soil[i] - 0.002 - (heights[i] - original[i])).abs() < 1e-12);
        }
        let added: f64 = heights
            .iter()
            .zip(original)
            .zip(&graph.areas)
            .map(|((&after, before), area)| (after - before) * area)
            .sum();
        assert!((report.added_material - added).abs() < 1e-12);
        let second = fill(
            &mesh,
            &graph,
            &mut heights,
            &mut soil,
            OceanStage::InitialErosion,
        );
        assert_eq!(second.filled_seas, 0);
        assert_eq!(second.added_material.to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn isolated_basins_are_filled_half_a_physical_metre_on_different_planet_sizes() {
        for radius in [1000.0, 6_371_000.0] {
            let mesh = generate(PlanetOptions {
                vertex_count: 128,
                radius,
                ..PlanetOptions::default()
            })
            .unwrap();
            let graph = SurfaceGraph::new(&mesh);
            let mut heights = vec![0.08; 128];
            let mut basins = Vec::new();
            for (i, h) in heights.iter_mut().enumerate() {
                if basins.iter().all(|&j| !graph.neighbours[i].contains(&j)) {
                    basins.push(i);
                    *h = -0.02;
                    if basins.len() == 4 {
                        break;
                    }
                }
            }
            let original = heights.clone();
            let mut soil = vec![0.002; 128];
            let report = fill(&mesh, &graph, &mut heights, &mut soil, OceanStage::Lod1);
            assert_eq!(report.filled_seas, 3);
            assert_eq!(count(&graph, &heights), 1);
            for (&after, &before) in heights.iter().zip(&original) {
                assert!(after >= before);
                if after > before {
                    assert!(after * radius >= 0.5);
                    assert!((after * radius - 0.5).abs() < 1e-9);
                }
            }
            assert_eq!(mesh.radius.to_bits(), radius.to_bits());
        }
    }

    #[test]
    fn dry_wet_and_sea_level_worlds_stay_unchanged() {
        let mesh = generate(PlanetOptions {
            vertex_count: 32,
            ..PlanetOptions::default()
        })
        .unwrap();
        let graph = SurfaceGraph::new(&mesh);
        for height in [-0.1, 0.0, 0.1] {
            let mut heights = vec![height; 32];
            let mut soil = vec![0.002; 32];
            let report = fill(&mesh, &graph, &mut heights, &mut soil, OceanStage::Initial);
            assert_eq!(report.filled_seas, 0);
            assert!(heights.iter().all(|h| h.to_bits() == height.to_bits()));
            assert!(soil.iter().all(|s| s.to_bits() == 0.002_f64.to_bits()));
        }
    }
}
