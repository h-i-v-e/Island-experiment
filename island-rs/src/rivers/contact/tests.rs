//! Regression fixtures whose physical spacing is independent of graph density.
use super::super::RiverChannelSettings;
use super::*;
use crate::{ISLAND_WORLD_METRES, Vec3};

struct Grid {
    mesh: Mesh,
    downstream: Vec<usize>,
    ocean: Vec<bool>,
    sources: Vec<usize>,
}
impl Grid {
    fn channels(columns: usize, rows: usize, longitudinal_step: f32, stagger: bool) -> Self {
        let low = rows / 4;
        let high = rows * 3 / 4;
        let gap_step = 4. / (high - low) as f32;
        let mut mesh = Mesh::default();
        for row in 0..rows {
            for column in 0..columns {
                let x = column as f32 * longitudinal_step
                    + if stagger && row >= high {
                        longitudinal_step * 0.5
                    } else {
                        0.
                    };
                let y = row as f32 * gap_step;
                let height = if row >= low && row <= high {
                    10. + (row - low) as f32 * 2. / (high - low) as f32
                } else {
                    30.
                };
                mesh.vertices
                    .push(Vec3::new(x, y, height - x * 0.001) / ISLAND_WORLD_METRES);
            }
        }
        for row in 0..rows - 1 {
            for column in 0..columns - 1 {
                let a = (row * columns + column) as u32;
                let b = a + columns as u32;
                mesh.triangles.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        let mut downstream: Vec<_> = (0..mesh.vertices.len()).collect();
        let mut ocean = vec![false; mesh.vertices.len()];
        for row in 0..rows {
            for column in 0..columns {
                let vertex = row * columns + column;
                if row == low || row == high {
                    if column == columns - 1 {
                        ocean[vertex] = true;
                        mesh.vertices[vertex].z = -0.001;
                    } else {
                        downstream[vertex] = vertex + 1;
                    }
                } else {
                    downstream[vertex] = if row < usize::midpoint(low, high) {
                        low * columns + column
                    } else {
                        high * columns + column
                    };
                }
            }
        }
        Self {
            mesh,
            downstream,
            ocean,
            sources: vec![low * columns, high * columns],
        }
    }
    fn join(&mut self, width: f32) -> usize {
        let adjacency = self.mesh.adjacency();
        let (flow, catchment) = calculate_flow_and_catchment(&self.mesh, &self.downstream);
        let mut drainage = Drainage {
            downstream: self.downstream.clone(),
            reaches_ocean: vec![true; self.downstream.len()],
        };
        let environment = ContactEnvironment {
            mesh: &self.mesh,
            adjacency: &adjacency,
            ocean: &self.ocean,
            excluded: &[],
            flow: &flow,
            catchment: &catchment,
            channel: RiverChannelSettings {
                source_width: width / ISLAND_WORLD_METRES,
                maximum_width: width / ISLAND_WORLD_METRES,
                ..RiverChannelSettings::default()
            },
        };
        let changed = ContactPlan::new(self.mesh.vertices.len()).join(
            &environment,
            &mut drainage,
            &self.sources,
        );
        self.downstream = drainage.downstream;
        changed
    }
}

#[test]
fn near_parallel_channels_join_beyond_three_graph_rings() {
    for rows in [17, 33, 65] {
        let mut grid = Grid::channels(13, rows, 1., false);
        assert!(
            grid.join(8.) > 0,
            "physical contact must not depend on the {rows}-row terrain sampling"
        );
        assert_eq!(
            drainage_order(&grid.downstream).len(),
            grid.mesh.vertices.len()
        );
        let (flow, _) = calculate_flow_and_catchment(&grid.mesh, &grid.downstream);
        let total: u32 = grid
            .ocean
            .iter()
            .enumerate()
            .filter(|(_, sea)| **sea)
            .map(|(i, _)| flow[i])
            .sum();
        assert_eq!(total, grid.mesh.vertices.len() as u32);
    }
}

#[test]
fn a_long_segment_interior_is_visible_even_when_its_endpoints_are_far() {
    let mut grid = Grid::channels(5, 17, 100., true);
    assert!(
        grid.join(8.) > 0,
        "segment interiors must be indexed rather than just their endpoints"
    );
    assert_eq!(
        drainage_order(&grid.downstream).len(),
        grid.mesh.vertices.len()
    );
}

#[test]
fn long_graph_connectors_are_traced_inline_through_the_shared_join() {
    let mut grid = Grid::channels(13, 65, 1., false);
    assert!(grid.join(8.) > 0);
    let adjacency = grid.mesh.adjacency();
    let drainage = Drainage {
        downstream: grid.downstream.clone(),
        reaches_ocean: vec![true; grid.downstream.len()],
    };
    let (flow, catchment) = calculate_flow_and_catchment(&grid.mesh, &grid.downstream);
    let (rivers, joins) = super::super::tracing::trace_drainage(
        &grid.mesh,
        &drainage,
        &flow,
        &catchment,
        &grid.sources,
        &grid.ocean,
    );
    assert!(rivers.iter().any(|river| river.join.is_some()));
    for (river, join) in rivers.iter().zip(joins) {
        if let Some(join) = join {
            assert_eq!(river.nodes.last().unwrap().vertex, join);
        }
        assert!(
            river
                .nodes
                .windows(2)
                .all(|pair| adjacency[pair[0].vertex].contains(&pair[1].vertex)),
            "shaping must receive each terrain-adjacent connector step"
        );
    }
}

#[test]
fn close_early_merge_margin_does_not_join_distant_narrow_channels() {
    let mut grid = Grid::channels(13, 33, 1., false);
    let before = grid.downstream.clone();
    assert_eq!(grid.join(1.), 0);
    assert_eq!(grid.downstream, before);
}

#[test]
fn nearby_channels_merge_before_their_configured_widths_overlap() {
    // Centreline gap is 4m. Each fixed full width contributes half its width,
    // so 3.5m channels have a 0.5m dry gap inside the 1m early-merge margin.
    let mut nearby = Grid::channels(13, 33, 1., false);
    assert!(
        nearby.join(3.5) > 0,
        "nearby channels must merge before their widths overlap"
    );

    // A 1.5m dry gap exceeds that margin and must retain separate channels.
    let mut separate = Grid::channels(13, 33, 1., false);
    let before = separate.downstream.clone();
    assert_eq!(separate.join(2.5), 0);
    assert_eq!(separate.downstream, before);
}
