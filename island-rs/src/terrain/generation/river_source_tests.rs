use super::*;

fn drainage_fixture() -> (Mesh, SurfaceMaterial, Vec2) {
    const SIDE: usize = 9;
    let points: Vec<_> = (0..SIDE)
        .flat_map(|y| (0..SIDE).map(move |x| Vec2::new(x as f32 * 0.01, y as f32 * 0.01)))
        .collect();
    let mut mesh = Mesh::delaunay(&points);
    for vertex in &mut mesh.vertices {
        vertex.z = vertex.x * 0.5 - 0.015 + (vertex.y - 0.04).abs() * 0.05;
    }
    let material = SurfaceMaterial::empty(mesh.vertices.len());
    (mesh, material, Vec2::new(0.07, 0.04))
}

fn inherited_only_context() -> GenerationContext {
    GenerationContext::new(
        42,
        IslandOptions {
            // Make new catchment-selected sources impossible for this fixture.
            river_source_catchment_hectares: 1.0e10,
            hydraulic_erosion_strength: 0.0,
            // A high thermal talus leaves the test's drainage valley intact.
            max_height: 100.0,
            ..IslandOptions::default()
        },
    )
}

#[test]
fn accepted_sources_survive_every_lod_river_pass_and_final_generation() {
    let (mut mesh, mut material, source) = drainage_fixture();
    let context = inherited_only_context();
    let mut scratch = GenerationScratch::new(GenerationMethod::Cpu);
    scratch.river_sources.push(source);
    let stages = [
        generate_lod2,
        generate_first_lod1,
        refine_lod1_again,
        generate_broad_lod0,
    ];
    for stage in stages {
        // An unusable old anchor must disappear when accepted traces replace it.
        scratch.river_sources.push(Vec2::splat(f32::NAN));
        (mesh, material) = stage(&mesh, material, context, &mut scratch).unwrap();
        assert!(!scratch.river_sources.is_empty());
        assert!(
            scratch
                .river_sources
                .iter()
                .all(|source| source.is_finite())
        );
        assert!(scratch.river_sources.iter().all(|&anchor| {
            mesh.vertices
                .iter()
                .any(|vertex| vertex.truncate().distance(anchor) < 0.02)
        }));
    }
    (mesh, material) = generate_detail_lod0(mesh, material, context, &mut scratch).unwrap();
    let without_inheritance = generate_final_rivers(
        context.seed,
        &mesh,
        &material,
        context.river_source_rule,
        context.options.river_channel_settings(),
        None,
        &[],
    )
    .unwrap();
    assert!(without_inheritance.rivers.is_empty());
    let inherited = generate_final_rivers(
        context.seed,
        &mesh,
        &material,
        context.river_source_rule,
        context.options.river_channel_settings(),
        None,
        &scratch.river_sources,
    )
    .unwrap();
    assert!(!inherited.rivers.is_empty());
}

#[test]
fn retained_sources_retrace_after_vertex_reordering_and_drop_when_submerged() {
    let (mut mesh, _, source) = drainage_fixture();
    let context = inherited_only_context();
    let mut retained_sources = vec![source];
    let adjacency = mesh.adjacency();
    let initial = context.rivers(&mut mesh, &adjacency, &mut retained_sources);
    assert_eq!(initial.source_positions().collect::<Vec<_>>(), [source]);

    let vertex_count = mesh.vertices.len();
    mesh.vertices.reverse();
    for vertex in &mut mesh.triangles {
        *vertex = u32::try_from(vertex_count - 1 - *vertex as usize).unwrap();
    }
    let adjacency = mesh.adjacency();
    let retraced = context.rivers(&mut mesh, &adjacency, &mut retained_sources);
    assert_eq!(retraced.source_positions().collect::<Vec<_>>(), [source]);
    assert_eq!(retained_sources, [source]);

    mesh.vertices.iter_mut().for_each(|vertex| vertex.z = -0.01);
    let submerged = context.rivers(&mut mesh, &adjacency, &mut retained_sources);
    assert!(submerged.source_positions().next().is_none());
    assert!(retained_sources.is_empty());
}
