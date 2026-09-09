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
