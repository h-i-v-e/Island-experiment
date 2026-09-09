use super::*;

fn fixture(height: impl Fn(f32, f32) -> f32) -> Terrain {
    let mut mesh = Mesh::default();
    for y in 0..101 {
        for x in 0..101 {
            let p = Vec2::new(900.0 + x as f32 * 2.0, 900.0 + y as f32 * 2.0);
            mesh.vertices
                .push(p.extend(height(p.x, p.y)) / ISLAND_WORLD_METRES);
            mesh.uv.push(p / ISLAND_WORLD_METRES);
        }
    }
    for y in 0..100 {
        for x in 0..100 {
            let a = y * 101 + x;
            mesh.triangles
                .extend([a, a + 1, a + 101, a + 1, a + 102, a + 101]);
        }
    }
    mesh.calculate_normals();
    Terrain::new(mesh)
}

fn cliff() -> Terrain {
    fixture(|x, _| 10.0 + ((x - 1000.0) / 4.0).clamp(0.0, 1.0) * 30.0)
}
fn options() -> CaveOptions {
    CaveOptions {
        enabled: 1,
        length_min: 30.0,
        length_max: 30.0,
        ..CaveOptions::default()
    }
}

#[test]
fn disabled_caves_are_empty_and_options_are_bounded() {
    assert!(
        CaveSet::generate(7, &cliff(), CaveOptions::default(), |_| false)
            .unwrap()
            .caves
            .is_empty()
    );
    assert!(
        CaveOptions {
            voxel_size: f32::NAN,
            ..options()
        }
        .validate()
        .is_err()
    );
    assert!(
        CaveOptions {
            maximum_caves: 5,
            ..options()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn flat_island_has_no_entrance() {
    let result = CaveSet::generate(7, &fixture(|_, _| 10.0), options(), |_| false).unwrap();
    assert!(result.caves.is_empty());
    assert_eq!(result.stats.examined, 0);
}

#[test]
fn approach_checks_entire_landing() {
    assert!(placement::approach_valid(
        &cliff(),
        Vec3::new(999.0, 1000.0, 10.0),
        Vec2::X,
        &options()
    ));
    assert!(!placement::approach_valid(
        &fixture(|x, _| 10.0 + (x - 900.0) * 0.5),
        Vec3::new(999.0, 1000.0, 59.5),
        Vec2::X,
        &options()
    ));
}

#[test]
fn approach_accepts_configured_steps_but_not_a_sustained_steep_slope() {
    let terrain = fixture(|x, _| 10.0 + (x - 999.0) * 0.22 + if x > 995.0 { 0.18 } else { 0.0 });
    let entrance = Vec3::new(999.0, 1000.0, 10.18);
    assert!(placement::approach_valid(
        &terrain,
        entrance,
        Vec2::X,
        &options()
    ));
    assert!(!placement::approach_valid(
        &terrain,
        entrance,
        Vec2::X,
        &CaveOptions {
            approach_step: 0.0,
            ..options()
        }
    ));
}

#[test]
fn cliff_with_a_rounded_toe_can_have_an_entrance() {
    let terrain = fixture(|x, _| {
        10.0 + (x - 1000.0).clamp(0.0, 2.0) * 0.2 + (x - 1002.0).clamp(0.0, 18.0) * 2.0
    });
    let result = CaveSet::generate(7, &terrain, options(), |_| false).unwrap();
    assert_eq!(result.caves.len(), 1, "{:?}", result.stats);
    result.validate().unwrap();
}

#[test]
fn entrance_cut_preserves_roof_and_passage_is_only_underground() {
    let terrain = cliff();
    let o = options();
    let mut cave = placement::layout(
        17,
        &terrain,
        Vec3::new(999.0, 1000.0, 10.0),
        Vec2::X,
        &o,
        &|_| false,
    )
    .unwrap()
    .unwrap();
    cave.chunks = vec![passage::build(&cave, &o)];
    assert!(clearance::walkable(&cave, &o));
    // The throat has no ambient or normal discontinuity at the material join.
    let passage = &cave.chunks[0];
    for (i, vertex) in passage
        .vertices
        .iter()
        .take(cave.portal.section.len())
        .enumerate()
    {
        let p = *vertex * ISLAND_WORLD_METRES;
        assert!(cave.surface_attributes(&o, p).x > 0.999);
        assert!(passage.normals[i].distance(cave.portal.passage_normal(p, i <= 8)) < 0.0001);
    }
    // Bound the visible arch's deviation from an ellipse to three millimetres.
    for pair in cave.portal.section[9..].windows(2) {
        let middle = (pair[0] + pair[1]) * 0.5;
        let ellipse = Vec2::new(
            middle.x / (o.entrance_width * 0.5),
            (middle.y - o.entrance_height * 0.35) / (o.entrance_height * 0.65),
        );
        assert!((1.0 - ellipse.length()) * o.entrance_height < 0.003);
    }

    assert!(
        cave.chunks
            .iter()
            .flat_map(|m| &m.vertices)
            .all(|v| v.z * ISLAND_WORLD_METRES < 18.0)
    );
    let set = CaveSet {
        options: o,
        caves: vec![cave],
        stats: CaveStats::default(),
        network_options: CaveNetworkOptions::default(),
        walk_options: CaveWalkOptions::default(),
    };
    set.validate().unwrap();
    let original = terrain.mesh();
    let cut = set.cut_surface(original.clone());
    // Original positions and roof surface are retained. A neighbouring edge
    // may gain subdivisions to match the clipped face without moving the roof.
    assert_eq!(&cut.vertices[..original.vertices.len()], &original.vertices);
    for t in original.triangles.chunks_exact(3) {
        if t.iter()
            .all(|&i| original.vertices[i as usize].z * ISLAND_WORLD_METRES > 20.0)
        {
            let centre = t
                .iter()
                .map(|&i| original.vertices[i as usize])
                .sum::<Vec3>()
                / 3.0;
            assert!(
                cut.triangles.chunks_exact(3).any(|other| {
                    let p = [other[0], other[1], other[2]]
                        .map(|i| cut.vertices[i as usize] * ISLAND_WORLD_METRES);
                    let centre = centre * ISLAND_WORLD_METRES;
                    let normal = (p[1] - p[0]).cross(p[2] - p[0]);
                    normal.length_squared() > 0.0
                        && (centre - p[0]).dot(normal).abs() <= 0.001 * normal.length()
                        && (0..3).all(|i| {
                            (p[(i + 1) % 3] - p[i]).cross(centre - p[i]).dot(normal)
                                >= -1.0e-6 * normal.length_squared()
                        })
                }),
                "original roof surface lost at {centre:?}"
            );
        }
    }
    let c = &set.caves[0];
    // The clipped terrain and passage share exact, fully subdivided edges,
    // including the boundary between their separate render meshes.
    let mut edge_uses = std::collections::HashMap::new();
    for mesh in std::iter::once(&cut).chain(c.chunks.iter()) {
        for t in mesh.triangles.chunks_exact(3) {
            for i in 0..3 {
                let a = mesh.vertices[t[i] as usize];
                let b = mesh.vertices[t[(i + 1) % 3] as usize];
                let mut key = [
                    a.to_array().map(f32::to_bits),
                    b.to_array().map(f32::to_bits),
                ];
                key.sort_unstable();
                let entry = edge_uses
                    .entry(key)
                    .or_insert((0, (a + b) * 0.5 * ISLAND_WORLD_METRES));
                entry.0 += 1;
            }
        }
    }
    let (minimum, maximum) = c.portal.bounds();
    for (_, (uses, p)) in edge_uses {
        if p.cmpge(minimum).all()
            && p.cmple(maximum).all()
            && (p - c.portal.origin).truncate().dot(c.inward) < c.portal.length + 0.005
        {
            assert!(uses >= 2, "unstitched entrance edge at {p:?}");
        }
    }

    for t in cut.triangles.chunks_exact(3) {
        let centre =
            t.iter().map(|&i| cut.vertices[i as usize]).sum::<Vec3>() / 3.0 * ISLAND_WORLD_METRES;
        if c.portal.contains(centre) {
            // The only geometry inside the opening is the lip; there is no
            // original cliff face left across its middle at head height.
            let across = (centre - c.entrance).y.abs();
            assert!(
                across > 1.3 || centre.z < 10.5 || centre.z > 12.3,
                "Blocked aperture: {centre:?}"
            );
        }
    }
    let bytes = bincode::serialize(&set).unwrap();
    assert_eq!(set, bincode::deserialize::<CaveSet>(&bytes).unwrap());
    if let Some(path) = std::env::var_os("MOTU_CAVE_FIXTURE_OUTPUT") {
        let pack = |mesh: &Mesh, collision_only: bool| {
            serde_json::json!({
            "vertices":mesh.vertices.iter().flat_map(|v|[(v.x-0.5)*ISLAND_WORLD_METRES,v.z*ISLAND_WORLD_METRES,(v.y-0.5)*ISLAND_WORLD_METRES]).collect::<Vec<_>>(),
            "normals":mesh.normals.iter().flat_map(|v|[v.x,v.z,v.y]).collect::<Vec<_>>(),
            "triangles":mesh.triangles.chunks_exact(3).flat_map(|t|[t[0],t[2],t[1]]).collect::<Vec<_>>(),
            "collisionOnly":collision_only})
        };
        let fixture = serde_json::json!({"minimum":[c.minimum.x-1000.0,c.minimum.y-1000.0],"maximum":[c.maximum.x-1000.0,c.maximum.y-1000.0],
            "entrance":[-1.0,10.0,0.0],"chamber":[c.nodes.last().unwrap().floor.x-1000.0,c.nodes.last().unwrap().floor.z,c.nodes.last().unwrap().floor.y-1000.0],
            "chunks":c.chunks.iter().map(|m|pack(m,false)).chain(std::iter::once(pack(&c.entrance_collider,true))).collect::<Vec<_>>(),
            "terrain":pack(&cut,false)});
        std::fs::write(path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    }
}

#[test]
fn candidate_selection_is_deterministic_and_respects_hazards() {
    let terrain = cliff();
    let a = CaveSet::generate(7, &terrain, options(), |_| false).unwrap();
    assert_eq!(a.caves.len(), 1, "{:?}", a.stats);
    assert_eq!(
        a,
        CaveSet::generate(7, &terrain, options(), |_| false).unwrap()
    );
    assert!(
        CaveSet::generate(7, &terrain, options(), |_| true)
            .unwrap()
            .caves
            .is_empty()
    );
}

#[test]
fn thin_roof_and_narrow_ridge_are_rejected() {
    for terrain in [
        fixture(|x, _| 10.0 + ((x - 1000.0) / 4.0).clamp(0.0, 1.0) * 6.0),
        fixture(|x, y| {
            if (y - 1000.0).abs() < 3.0 {
                10.0 + ((x - 1000.0) / 4.0).clamp(0.0, 1.0) * 30.0
            } else {
                10.0
            }
        }),
    ] {
        assert!(
            placement::layout(
                17,
                &terrain,
                Vec3::new(999.0, 1000.0, 10.0),
                Vec2::X,
                &options(),
                &|_| false
            )
            .unwrap()
            .is_none()
        );
    }
}

#[test]
fn side_passages_connect_to_walkable_larger_chambers() {
    let terrain = cliff();
    let o = CaveOptions {
        length_min: 50.0,
        length_max: 50.0,
        ..options()
    };
    let mut cave = placement::layout(
        17,
        &terrain,
        Vec3::new(999.0, 1000.0, 10.0),
        Vec2::X,
        &o,
        &|_| false,
    )
    .unwrap()
    .unwrap();
    cave.chunks = vec![passage::build(&cave, &o)];
    network::expand(
        &mut cave,
        &terrain,
        &o,
        CaveNetworkOptions {
            maximum_branches: 2,
            ..CaveNetworkOptions::default()
        },
        &|_| false,
        &[],
    );
    let cut = cave
        .portal
        .cut(terrain.mesh().clone(), &cave.entrance_collider.vertices);
    let mut edges = std::collections::HashMap::new();
    for mesh in std::iter::once(&cut).chain(cave.chunks.iter()) {
        for t in mesh.triangles.chunks_exact(3) {
            for i in 0..3 {
                let a = mesh.vertices[t[i] as usize];
                let b = mesh.vertices[t[(i + 1) % 3] as usize];
                let mut key = [
                    a.to_array().map(f32::to_bits),
                    b.to_array().map(f32::to_bits),
                ];
                key.sort_unstable();
                let entry = edges
                    .entry(key)
                    .or_insert((0, (a + b) * 0.5 * ISLAND_WORLD_METRES));
                entry.0 += 1;
            }
        }
    }
    let open_edges: Vec<_> = edges
        .into_iter()
        .filter(|(_, (uses, p))| {
            *uses < 2
                && p.truncate().cmpge(cave.minimum).all()
                && p.truncate().cmple(cave.maximum).all()
        })
        .collect();
    if let Some(path) = std::env::var_os("MOTU_CAVE_NETWORK_FIXTURE_OUTPUT") {
        let pack = |mesh: &Mesh, collision_only: bool| {
            serde_json::json!({
            "vertices":mesh.vertices.iter().flat_map(|v|[(v.x-0.5)*ISLAND_WORLD_METRES,v.z*ISLAND_WORLD_METRES,(v.y-0.5)*ISLAND_WORLD_METRES]).collect::<Vec<_>>(),
            "normals":mesh.normals.iter().flat_map(|v|[v.x,v.z,v.y]).collect::<Vec<_>>(),
            "triangles":mesh.triangles.chunks_exact(3).flat_map(|t|[t[0],t[2],t[1]]).collect::<Vec<_>>(),
            "collisionOnly":collision_only})
        };
        let c = &cave;
        let data = serde_json::json!({"minimum":[c.minimum.x-1000.0,c.minimum.y-1000.0],"maximum":[c.maximum.x-1000.0,c.maximum.y-1000.0],
            "entrance":[-1.0,10.0,0.0],"chamber":[c.nodes.last().unwrap().floor.x-1000.0,c.nodes.last().unwrap().floor.z,c.nodes.last().unwrap().floor.y-1000.0],
            "chunks":c.chunks.iter().map(|m|pack(m,false)).chain(std::iter::once(pack(&c.entrance_collider,true))).collect::<Vec<_>>(),
            "branches":c.branches.iter().map(|b|serde_json::json!({"nodes":b.nodes.iter().flat_map(|n|[n.floor.x-1000.0,n.floor.z,n.floor.y-1000.0]).collect::<Vec<_>>()})).collect::<Vec<_>>(),
            "terrain":pack(&cut,false)});
        std::fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
    }
    assert!(
        open_edges.is_empty(),
        "{} open edges; throats {:?}; first {:?}",
        open_edges.len(),
        cave.branches
            .iter()
            .map(|b| b.nodes[1].floor)
            .collect::<Vec<_>>(),
        &open_edges[..open_edges.len().min(5)]
    );
    assert_eq!(cave.branches.len(), 2);
    assert!(clearance::walkable(&cave, &o));
    assert!(
        cave.branches
            .iter()
            .all(|b| b.nodes.last().unwrap().width > o.chamber_width)
    );
}

#[test]
fn network_options_and_snapshot_topology_are_bounded() {
    let o = options();
    for bad in [
        CaveNetworkOptions {
            maximum_branches: 4,
            ..CaveNetworkOptions::default()
        },
        CaveNetworkOptions {
            branch_length_min: f32::NAN,
            ..CaveNetworkOptions::default()
        },
        CaveNetworkOptions {
            chamber_scale: f32::INFINITY,
            ..CaveNetworkOptions::default()
        },
    ] {
        assert!(bad.validate(&o).is_err());
    }
    let terrain = cliff();
    let legacy = CaveSet::generate(7, &terrain, o, |_| false).unwrap();
    let disabled =
        CaveSet::generate_networks(7, &terrain, o, CaveNetworkOptions::default(), |_| false)
            .unwrap();
    assert_eq!(legacy, disabled);
    let network = CaveNetworkOptions {
        maximum_branches: 2,
        ..CaveNetworkOptions::default()
    };
    let first = CaveSet::generate_networks(7, &terrain, o, network, |_| false).unwrap();
    assert!(!first.caves[0].branches.is_empty());
    assert_eq!(
        first,
        CaveSet::generate_networks(7, &terrain, o, network, |_| false).unwrap()
    );
    let data = bincode::serialize(&first).unwrap();
    let mut restored: CaveSet = bincode::deserialize(&data).unwrap();
    assert_eq!(restored, first);
    restored.validate().unwrap();
    restored.caves[0].branches[0].junction_node = u32::MAX;
    assert!(restored.validate().is_err());
}

#[test]
fn wandering_volume_preserves_entrance_and_all_routes() {
    let terrain = cliff();
    let o = options();
    let mut cave = placement::layout(
        17,
        &terrain,
        Vec3::new(999.0, 1000.0, 10.0),
        Vec2::X,
        &o,
        &|_| false,
    )
    .unwrap()
    .unwrap();
    wandering::expand(
        &mut cave,
        &terrain,
        &o,
        CaveWalkOptions {
            enabled: 1,
            ..CaveWalkOptions::default()
        },
        &|_| false,
        &[],
    )
    .unwrap();
    eprintln!(
        "walk paths {} nodes {} tris {}",
        cave.branches.len() + 1,
        cave.paths().map(<[Node]>::len).sum::<usize>(),
        cave.chunks
            .iter()
            .map(|m| m.triangles.len() / 3)
            .sum::<usize>()
    );
    assert_consistent_facing(&cave.chunks);
    let cut = cave
        .portal
        .cut(terrain.mesh().clone(), &cave.entrance_collider.vertices);
    let mut edges = std::collections::HashMap::new();
    for mesh in std::iter::once(&cut).chain(cave.chunks.iter()) {
        for t in mesh.triangles.chunks_exact(3) {
            for i in 0..3 {
                let a = mesh.vertices[t[i] as usize];
                let b = mesh.vertices[t[(i + 1) % 3] as usize];
                let mut key = [
                    a.to_array().map(f32::to_bits),
                    b.to_array().map(f32::to_bits),
                ];
                key.sort_unstable();
                let entry = edges
                    .entry(key)
                    .or_insert((0, (a + b) * 0.5 * ISLAND_WORLD_METRES));
                entry.0 += 1;
            }
        }
    }
    let open_edges: Vec<_> = edges
        .into_iter()
        .filter(|(_, (uses, p))| {
            *uses < 2
                && p.truncate().cmpge(cave.minimum).all()
                && p.truncate().cmple(cave.maximum).all()
        })
        .collect();
    if let Some(path) = std::env::var_os("MOTU_CAVE_WALK_FIXTURE_OUTPUT") {
        let pack = |mesh: &Mesh, collision_only: bool| {
            serde_json::json!({
            "vertices":mesh.vertices.iter().flat_map(|v|[(v.x-0.5)*ISLAND_WORLD_METRES,v.z*ISLAND_WORLD_METRES,(v.y-0.5)*ISLAND_WORLD_METRES]).collect::<Vec<_>>(),
            "normals":mesh.normals.iter().flat_map(|v|[v.x,v.z,v.y]).collect::<Vec<_>>(),
            "triangles":mesh.triangles.chunks_exact(3).flat_map(|t|[t[0],t[2],t[1]]).collect::<Vec<_>>(),
            "collisionOnly":collision_only})
        };
        let c = &cave;
        let data = serde_json::json!({"minimum":[c.minimum.x-1000.0,c.minimum.y-1000.0],"maximum":[c.maximum.x-1000.0,c.maximum.y-1000.0],
            "entrance":[-1.0,10.0,0.0],"chamber":[c.nodes.last().unwrap().floor.x-1000.0,c.nodes.last().unwrap().floor.z,c.nodes.last().unwrap().floor.y-1000.0],
            "chunks":c.chunks.iter().map(|m|pack(m,false)).chain(std::iter::once(pack(&c.entrance_collider,true))).collect::<Vec<_>>(),
            "branches":c.branches.iter().map(|b|serde_json::json!({"nodes":b.nodes.iter().flat_map(|n|[n.floor.x-1000.0,n.floor.z,n.floor.y-1000.0]).collect::<Vec<_>>()})).collect::<Vec<_>>(),
            "terrain":pack(&cut,false)});
        std::fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
    }
    assert!(
        open_edges.is_empty(),
        "{} open edges; throats {:?}; first {:?}",
        open_edges.len(),
        cave.branches
            .iter()
            .map(|b| b.nodes[1].floor)
            .collect::<Vec<_>>(),
        &open_edges[..open_edges.len().min(5)]
    );
    assert!(clearance::walkable(&cave, &o));
}

#[test]
fn wandering_is_deterministic_and_ends_by_probability() {
    let terrain = cliff();
    let o = options();
    let mut cave = placement::layout(
        17,
        &terrain,
        Vec3::new(999.0, 1000.0, 10.0),
        Vec2::X,
        &o,
        &|_| false,
    )
    .unwrap()
    .unwrap();
    let walk = CaveWalkOptions {
        enabled: 1,
        ..CaveWalkOptions::default()
    };
    let first = wandering::layout(&cave, &terrain, &o, walk, &|_| false, &[]).unwrap();
    assert_eq!(
        first,
        wandering::layout(&cave, &terrain, &o, walk, &|_| false, &[]).unwrap()
    );
    let short = wandering::layout(
        &cave,
        &terrain,
        &o,
        CaveWalkOptions {
            end_probability: 1.0,
            branch_probability: 0.0,
            ..walk
        },
        &|_| false,
        &[],
    )
    .unwrap();
    assert_eq!(short.0.len(), 4);
    assert!(short.1.is_empty());
    assert!(first.0.len() > short.0.len());
    assert!(
        first
            .0
            .windows(2)
            .any(|p| (p[0].width - p[1].width).abs() > 0.1)
    );
    let mut lengths = std::collections::BTreeSet::new();
    let mut recursive = false;
    for seed in 0..30 {
        cave.id = seed;
        let (nodes, branches) =
            wandering::layout(&cave, &terrain, &o, walk, &|_| false, &[]).unwrap();
        lengths.insert(nodes.len());
        recursive |= branches.iter().any(|b| b.parent_path > 0);
    }
    assert!(lengths.len() > 8);
    assert!(recursive);
    for bad in [
        CaveWalkOptions {
            end_probability: f32::NAN,
            ..walk
        },
        CaveWalkOptions {
            branch_probability: 1.01,
            ..walk
        },
        CaveWalkOptions {
            end_probability: 0.0,
            ..walk
        },
    ] {
        assert!(bad.validate().is_err());
    }
}

#[test]
fn frequent_branches_end_sooner_with_each_generation() {
    let terrain = cliff();
    let o = options();
    let mut cave = placement::layout(
        17,
        &terrain,
        Vec3::new(999.0, 1000.0, 10.0),
        Vec2::X,
        &o,
        &|_| false,
    )
    .unwrap()
    .unwrap();
    let mut counts = [0; 2];
    let mut deepest = 0;
    for (index, branching) in [0.06, 1.0].into_iter().enumerate() {
        let walk = CaveWalkOptions {
            enabled: 1,
            end_probability: 0.125,
            branch_probability: branching,
            ..CaveWalkOptions::default()
        }
        .validate()
        .unwrap();
        for seed in 0..64 {
            cave.id = seed;
            let (_, branches) =
                wandering::layout(&cave, &terrain, &o, walk, &|_| false, &[]).unwrap();
            counts[index] += branches.len();
            let mut depths = vec![0];
            for branch in &branches {
                let depth = depths[branch.parent_path as usize] + 1;
                depths.push(depth);
                deepest = deepest.max(depth);
                // 12.5% -> 25% -> 50% -> 100%. Terminal branches have one
                // segment, and cannot create another generation even at 100% branching.
                assert!(depth <= 3);
                if depth == 3 {
                    assert_eq!(branch.nodes.len(), 2);
                }
            }
        }
    }
    assert_eq!(deepest, 3, "Exercise recursive and terminal branches.");
    assert!(
        counts[1] > counts[0] * 2,
        "High branching must create more junctions: {counts:?}"
    );
}

#[test]
fn volume_crossings_are_air_and_loops_leave_rock_pillars() {
    let terrain = cliff();
    let o = options();
    let mut cave = placement::layout(
        17,
        &terrain,
        Vec3::new(999.0, 1000.0, 10.0),
        Vec2::X,
        &o,
        &|_| false,
    )
    .unwrap()
    .unwrap();
    cave.nodes.truncate(2);
    for (x, y) in [
        (1013., 1000.),
        (1033., 1000.),
        (1033., 1020.),
        (1013., 1020.),
        (1013., 1000.),
    ] {
        cave.nodes.push(Node {
            floor: Vec3::new(x, y, 10.),
            width: 4.,
            height: 3.,
        });
    }
    let rock = Vec3::new(1023., 1010., 11.5);
    assert!(volume::density(&cave, &o, rock) < 0.0);
    // A second passage crosses the first at a right angle. Their union has air
    // through both corridors, including the position of the old first wall.
    cave.branches.push(CaveBranch {
        parent_path: 0,
        junction_node: 3,
        nodes: vec![
            Node {
                floor: Vec3::new(1033., 1000., 10.),
                width: 4.,
                height: 3.,
            },
            Node {
                floor: Vec3::new(1033., 990., 10.),
                width: 4.,
                height: 3.,
            },
        ],
    });
    assert!(volume::density(&cave, &o, Vec3::new(1033., 998., 11.5)) > 0.0);
    assert!(volume::density(&cave, &o, rock) < 0.0);
    cave.surface = surface::SurfaceGrid::new(
        &terrain,
        Vec2::new(995., 985.),
        Vec2::new(1040., 1025.),
        o.voxel_size,
    )
    .unwrap();
    cave.minimum = cave.surface.minimum;
    cave.maximum = cave.surface.maximum();
    cave.chunks = volume::build(&mut cave, &o).unwrap();
    assert_consistent_facing(&cave.chunks);
    assert!(clearance::walkable(&cave, &o));
}

#[test]
fn oblique_volume_has_closed_interior() {
    let mut mesh = cliff().mesh().clone();
    let direction = Vec2::new(0.21715347, 0.97613746).normalize();
    let right = Vec2::new(-direction.y, direction.x);
    let origin = Vec2::new(1580.9542, 1459.4552);
    for p in &mut mesh.vertices {
        let local = *p * ISLAND_WORLD_METRES;
        let horizontal = origin + direction * (local.x - 1000.) + right * (local.y - 1000.);
        *p = horizontal.extend(local.z + 2.2612) / ISLAND_WORLD_METRES;
    }
    mesh.calculate_normals();
    let terrain = Terrain::new(mesh);
    let o = options();
    let mut cave = placement::layout(
        17,
        &terrain,
        (origin - direction).extend(12.2612),
        direction,
        &o,
        &|_| false,
    )
    .unwrap()
    .unwrap();
    wandering::expand(
        &mut cave,
        &terrain,
        &o,
        CaveWalkOptions {
            enabled: 1,
            ..CaveWalkOptions::default()
        },
        &|_| false,
        &[],
    )
    .unwrap();
    let cut = cave
        .portal
        .cut(terrain.mesh().clone(), &cave.entrance_collider.vertices);
    let mut edges = std::collections::HashMap::new();
    for mesh in std::iter::once(&cut).chain(cave.chunks.iter()) {
        for t in mesh.triangles.chunks_exact(3) {
            for side in 0..3 {
                let a = mesh.vertices[t[side] as usize];
                let b = mesh.vertices[t[(side + 1) % 3] as usize];
                let mut key = [
                    a.to_array().map(f32::to_bits),
                    b.to_array().map(f32::to_bits),
                ];
                key.sort_unstable();
                let e = edges
                    .entry(key)
                    .or_insert((0, (a + b) * 0.5 * ISLAND_WORLD_METRES));
                e.0 += 1;
            }
        }
    }
    for (uses, p) in edges.into_values() {
        // The old exterior cutter has unmatched edges reaching the throat on
        // this rotated fixture (also reproduced with the swept mesher). The
        // interior must be closed; the existing exterior issue is tracked separately.
        if p.truncate().cmpge(cave.minimum).all()
            && p.truncate().cmple(cave.maximum).all()
            && (p - cave.portal.throat()).truncate().dot(cave.inward) > 0.01
        {
            assert_eq!(uses, 2, "non-conforming oblique join at {p:?}");
        }
    }
    assert!(clearance::walkable(&cave, &o));
}

fn assert_consistent_facing(meshes: &[Mesh]) {
    let mut edges = std::collections::HashMap::new();
    for mesh in meshes {
        for triangle in mesh.triangles.chunks_exact(3) {
            for side in 0..3 {
                let a = mesh.vertices[triangle[side] as usize]
                    .to_array()
                    .map(f32::to_bits);
                let b = mesh.vertices[triangle[(side + 1) % 3] as usize]
                    .to_array()
                    .map(f32::to_bits);
                let key = if a < b { [a, b] } else { [b, a] };
                let entry = edges.entry(key).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += if a < b { 1 } else { -1 };
            }
        }
    }
    let wrong = edges
        .values()
        .filter(|(uses, balance)| *uses == 2 && *balance != 0)
        .count();
    assert_eq!(
        wrong, 0,
        "adjacent cave triangles face inconsistently at {wrong} shared edges"
    );
}
