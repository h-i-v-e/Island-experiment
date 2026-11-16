use super::contact::{ContactEnvironment, ContactPlan, join_contacts};
use super::drainage::Drainage;
use super::tracing::{calculate_flow_and_catchment, drainage_order};
use super::*;

struct Fixture {
    mesh: Mesh,
    adjacency: Adjacency,
    drainage: Drainage,
    ocean: Vec<bool>,
    excluded: Vec<bool>,
    sources: Vec<usize>,
    columns: usize,
}

impl Fixture {
    fn parallel(columns: usize) -> Self {
        let mut mesh = Mesh::default();
        for row in 0..5 {
            for column in 0..columns {
                let x = column as f32 * 12.0 / (columns - 1) as f32;
                let height = match row {
                    1 => 10.0,
                    2 => 11.0,
                    3 => 12.0,
                    _ => 30.0,
                } - x * 0.1;
                mesh.vertices
                    .push(Vec3::new(x, row as f32 * 2.0, height) / ISLAND_WORLD_METRES);
            }
        }
        for row in 0..4 {
            for column in 0..columns - 1 {
                let a = (row * columns + column) as u32;
                let b = a + columns as u32;
                mesh.triangles.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        let mut downstream: Vec<_> = (0..mesh.vertices.len()).collect();
        let mut ocean = vec![false; mesh.vertices.len()];
        for row in [1, 3] {
            let root = row * columns + columns - 1;
            mesh.vertices[root].z = -0.001;
            ocean[root] = true;
            for column in 0..columns - 1 {
                downstream[row * columns + column] += 1;
            }
        }
        for column in 0..columns {
            for row in [0, 2, 4] {
                downstream[row * columns + column] = if row == 4 {
                    3 * columns + column
                } else {
                    columns + column
                };
            }
        }
        let adjacency = mesh.adjacency();
        let excluded = vec![false; mesh.vertices.len()];
        let drainage = Drainage {
            downstream,
            reaches_ocean: vec![true; mesh.vertices.len()],
        };
        let (_, catchment) = calculate_flow_and_catchment(&mesh, &drainage.downstream);
        let mut sources: Vec<_> = [1, 3]
            .into_iter()
            .flat_map(|row| (0..columns - 1).map(move |column| row * columns + column))
            .collect();
        sources.sort_unstable_by(|&left, &right| {
            catchment[right]
                .total_cmp(&catchment[left])
                .then_with(|| left.cmp(&right))
        });
        Self {
            mesh,
            adjacency,
            drainage,
            ocean,
            excluded,
            sources,
            columns,
        }
    }

    fn join(&mut self, width_metres: f32) -> usize {
        let (flow, catchment) = calculate_flow_and_catchment(&self.mesh, &self.drainage.downstream);
        join_contacts(
            &ContactEnvironment {
                mesh: &self.mesh,
                adjacency: &self.adjacency,
                ocean: &self.ocean,
                excluded: &self.excluded,
                flow: &flow,
                catchment: &catchment,
                channel: RiverChannelSettings {
                    source_width: width_metres / ISLAND_WORLD_METRES,
                    maximum_width: width_metres / ISLAND_WORLD_METRES,
                    ..RiverChannelSettings::default()
                },
            },
            &mut self.drainage,
            &self.sources,
        )
    }
}

#[test]
fn touching_wide_channels_share_a_receiver_without_double_rainfall() {
    let mut fixture = Fixture::parallel(7);
    let (_, before) = calculate_flow_and_catchment(&fixture.mesh, &fixture.drainage.downstream);
    let roots = [2 * fixture.columns - 1, 4 * fixture.columns - 1];
    let before_total = before[roots[0]] + before[roots[1]];
    assert!(fixture.join(8.0) > 0);
    assert_eq!(
        drainage_order(&fixture.drainage.downstream).len(),
        fixture.mesh.vertices.len()
    );
    let (flow, after) = calculate_flow_and_catchment(&fixture.mesh, &fixture.drainage.downstream);
    assert!((after[roots[0]] + after[roots[1]] - before_total).abs() < 0.001);
    assert_eq!(
        flow[roots[0]] + flow[roots[1]],
        fixture.mesh.vertices.len() as u32
    );
    assert!(
        after[roots[0]] > before[roots[0]],
        "the established main stem receives the rerouted tributary area"
    );
}

#[test]
fn custom_narrow_channels_do_not_join_just_because_mesh_rings_touch() {
    let mut fixture = Fixture::parallel(7);
    let before = fixture.drainage.downstream.clone();
    assert_eq!(fixture.join(1.0), 0);
    assert_eq!(fixture.drainage.downstream, before);
}

#[test]
fn width_contact_is_independent_of_longitudinal_sampling_density() {
    for columns in [7, 13, 25] {
        assert!(Fixture::parallel(columns).join(8.0) > 0);
        assert_eq!(Fixture::parallel(columns).join(1.0), 0);
    }
}

#[test]
fn excluded_or_high_dry_separating_banks_are_not_confluence_connectors() {
    for excluded in [true, false] {
        let mut fixture = Fixture::parallel(7);
        for column in 0..fixture.columns {
            let middle = 2 * fixture.columns + column;
            if excluded {
                fixture.excluded[middle] = true;
            } else {
                fixture.mesh.vertices[middle].z = 100.0 / ISLAND_WORLD_METRES;
            }
        }
        let before = fixture.drainage.downstream.clone();
        assert_eq!(fixture.join(8.0), 0);
        assert_eq!(fixture.drainage.downstream, before);
    }
}

#[test]
fn higher_mainstem_continues_downhill_into_the_lower_touching_channel() {
    let mut fixture = Fixture::parallel(7);
    for column in 0..fixture.columns - 1 {
        fixture.mesh.vertices[fixture.columns + column].z += 4.0 / ISLAND_WORLD_METRES;
        fixture.mesh.vertices[2 * fixture.columns + column].z += 2.0 / ISLAND_WORLD_METRES;
    }
    let original = fixture.mesh.vertices.clone();
    let (_, before) = calculate_flow_and_catchment(&fixture.mesh, &fixture.drainage.downstream);
    let lower_root = 4 * fixture.columns - 1;
    assert!(fixture.join(8.0) > 0);
    assert_eq!(
        fixture.mesh.vertices, original,
        "contact planning cannot excavate or fill the established mainstem"
    );
    let (_, after) = calculate_flow_and_catchment(&fixture.mesh, &fixture.drainage.downstream);
    assert!(after[lower_root] > before[lower_root]);
    assert_eq!(
        drainage_order(&fixture.drainage.downstream).len(),
        fixture.mesh.vertices.len()
    );
    let mut vertex = fixture.columns;
    for _ in 0..fixture.mesh.vertices.len() {
        if fixture.drainage.downstream[vertex] == vertex {
            break;
        }
        vertex = fixture.drainage.downstream[vertex];
    }
    assert_eq!(
        vertex, lower_root,
        "the larger main continues through the physically lower outlet"
    );
}

#[test]
fn a_three_edge_connector_joins_adjacent_one_ring_surface_boundaries() {
    let mut fixture = Fixture::parallel(7);
    // Move the incoming centreline one row outward; its one-ring boundary is
    // now adjacent to the mainstem's boundary, without a shared ring vertex.
    for column in 0..fixture.columns - 1 {
        fixture.drainage.downstream[4 * fixture.columns + column] =
            4 * fixture.columns + column + 1;
        fixture.drainage.downstream[3 * fixture.columns + column] = 4 * fixture.columns + column;
        fixture.mesh.vertices[4 * fixture.columns + column].z =
            (12.0 - column as f32 * 0.2) / ISLAND_WORLD_METRES;
        fixture.mesh.vertices[3 * fixture.columns + column].z =
            (11.5 - column as f32 * 0.2) / ISLAND_WORLD_METRES;
    }
    // Add an outer bank row so that incoming centres remain interior vertices.
    let outer_start = fixture.mesh.vertices.len();
    for column in 0..fixture.columns {
        fixture
            .mesh
            .vertices
            .push(Vec3::new(column as f32 * 2.0, 10.0, 30.0) / ISLAND_WORLD_METRES);
        fixture
            .drainage
            .downstream
            .push(4 * fixture.columns + column);
        fixture.drainage.reaches_ocean.push(true);
        fixture.ocean.push(false);
        fixture.excluded.push(false);
        if column > 0 {
            let a = (4 * fixture.columns + column - 1) as u32;
            let b = (outer_start + column - 1) as u32;
            fixture
                .mesh
                .triangles
                .extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let root = 5 * fixture.columns - 1;
    fixture.ocean[4 * fixture.columns - 1] = false;
    fixture.ocean[root] = true;
    fixture.drainage.downstream[4 * fixture.columns - 1] = root;
    fixture.mesh.vertices[root].z = -0.001;
    fixture.drainage.downstream[root] = root;
    fixture.adjacency = fixture.mesh.adjacency();
    let (_, catchment) = calculate_flow_and_catchment(&fixture.mesh, &fixture.drainage.downstream);
    let columns = fixture.columns;
    fixture.sources = [1, 4]
        .into_iter()
        .flat_map(|row| (0..columns - 1).map(move |column| row * columns + column))
        .collect();
    fixture
        .sources
        .sort_unstable_by(|&left, &right| catchment[right].total_cmp(&catchment[left]));
    assert!(fixture.join(12.0) > 0);
}

#[test]
fn contact_width_feedback_converges_with_retained_junction_commitments() {
    for columns in [7, 13, 25] {
        let mut fixture = Fixture::parallel(columns);
        let mut converged = false;
        let mut plan = ContactPlan::new(fixture.mesh.vertices.len());
        let channel = RiverChannelSettings {
            source_width: 1.0 / ISLAND_WORLD_METRES,
            maximum_width: 10.0 / ISLAND_WORLD_METRES,
            ..RiverChannelSettings::default()
        };
        for _ in 0..=fixture.mesh.vertices.len() {
            let (flow, catchment) =
                calculate_flow_and_catchment(&fixture.mesh, &fixture.drainage.downstream);
            fixture.sources.sort_unstable_by(|&left, &right| {
                catchment[right]
                    .total_cmp(&catchment[left])
                    .then_with(|| left.cmp(&right))
            });
            let environment = ContactEnvironment {
                mesh: &fixture.mesh,
                adjacency: &fixture.adjacency,
                ocean: &fixture.ocean,
                excluded: &fixture.excluded,
                flow: &flow,
                catchment: &catchment,
                channel,
            };
            let changed = plan.join(&environment, &mut fixture.drainage, &fixture.sources);
            if changed == 0 {
                let previous = fixture.drainage.downstream.clone();
                assert_eq!(
                    plan.join(&environment, &mut fixture.drainage, &fixture.sources),
                    0,
                    "retained accepted junctions and final widths must be stable"
                );
                assert_eq!(fixture.drainage.downstream, previous);
                converged = true;
                break;
            }
        }
        assert!(converged);
        assert_eq!(
            drainage_order(&fixture.drainage.downstream).len(),
            fixture.mesh.vertices.len()
        );
        let (flow, _) = calculate_flow_and_catchment(&fixture.mesh, &fixture.drainage.downstream);
        let roots = [2 * columns - 1, 4 * columns - 1];
        assert_eq!(
            flow[roots[0]] + flow[roots[1]],
            fixture.mesh.vertices.len() as u32
        );
    }
}

#[test]
fn complete_same_input_resolution_is_repeatable_with_fresh_plan_state() {
    let fixture = Fixture::parallel(13);
    let settings = RiverGenerationSettings {
        source_rule: RiverSourceRule::new(0.001, 1.0, 0.0, 0.1),
        channel_settings: RiverChannelSettings {
            source_width: 1.0 / ISLAND_WORLD_METRES,
            maximum_width: 10.0 / ISLAND_WORLD_METRES,
            ..RiverChannelSettings::default()
        },
    };
    let run = || {
        super::contact::resolve(
            &fixture.mesh,
            &fixture.adjacency,
            settings,
            &fixture.ocean,
            &fixture.excluded,
            &[],
            42,
        )
    };
    let first = run();
    let second = run();
    assert!(!first.sources.is_empty());
    assert_eq!(first.drainage.downstream, second.drainage.downstream);
    assert_eq!(first.flow, second.flow);
    assert_eq!(first.sources, second.sources);
    assert!(
        first
            .catchment
            .iter()
            .zip(&second.catchment)
            .all(|(left, right)| left.to_bits() == right.to_bits())
    );
}
