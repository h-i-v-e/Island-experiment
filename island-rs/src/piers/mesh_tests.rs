use super::*;

fn frame() -> Frame {
    Frame {
        shore: Vec2::new(412., -382.),
        axis: Vec2::new(0.6, 0.8),
        start: -11.,
        inland_height: 1.92,
        tip: 36.,
        quay_end: 3.8,
        deck: 1.28,
        sea: 0.,
        edge: 2.25,
        berth_side: -1.,
    }
}

fn seabed(frame: Frame, p: Vec2) -> f32 {
    let relative = p - frame.shore;
    let x = relative.dot(frame.axis);
    let y = relative.dot(frame.axis.perp());
    x.mul_add(-0.15, y * 0.08)
}

fn check_faces(mesh: &Mesh) {
    assert_eq!(mesh.vertices.len(), mesh.normals.len());
    assert_eq!(mesh.vertices.len(), mesh.uv.len());
    assert!(mesh.uv.iter().all(|uv| uv.is_finite()));
    assert!(mesh.vertices.iter().all(|p| p.is_finite()));
    assert!(!mesh.triangles.is_empty());
    for triangle in mesh.triangles.chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(|i| mesh.vertices[i as usize]);
        let cross = (b - a).cross(c - a);
        for &index in triangle {
            let normal = mesh.normals[index as usize];
            assert!(normal.is_normalized());
            assert!(
                cross.dot(normal) > 0.,
                "collapsed or inverted face {a}, {b}, {c}"
            );
        }
    }
}

#[test]
fn detail_batches_preserve_the_station_and_edge_of_the_deck_seam() {
    let frame = frame();
    let sample = |p| Some(seabed(frame, p));
    let mut meshes = PierMeshes::default();
    build_quay(&mut meshes, frame, &sample).unwrap();
    build_deck(&mut meshes, frame);
    build_bents(&mut meshes, frame, &sample).unwrap();
    build_moorings(&mut meshes, frame, 25., 18.);
    for mesh in [
        &meshes.timber,
        &meshes.stone,
        &meshes.metal,
        &meshes.coarse_timber,
        &meshes.coarse_stone,
    ] {
        check_faces(mesh);
    }
    for y in [-frame.edge, frame.edge] {
        let seam = Vec3::new(frame.quay_end, y, frame.deck);
        for mesh in [
            &meshes.timber,
            &meshes.stone,
            &meshes.coarse_timber,
            &meshes.coarse_stone,
        ] {
            assert!(mesh.vertices.iter().any(|p| p.distance(seam) < 1e-5));
        }
    }
    assert!(meshes.timber.triangles.len() > 2 * meshes.coarse_timber.triangles.len());
    assert!(meshes.stone.triangles.len() > 2 * meshes.coarse_stone.triangles.len());
}

#[test]
fn each_pile_foot_follows_the_local_seabed_and_open_bays_keep_hull_room() {
    let frame = frame();
    let sample = |p| Some(seabed(frame, p));
    let bent = Bent::new(frame, &sample, 20.).unwrap();
    assert!(bent.bottom[1].z - bent.bottom[0].z > 0.25);
    for foot in bent.bottom {
        let bed = seabed(frame, frame.world(foot).truncate());
        assert!(bed - foot.z >= FOUNDATION_EMBED);
    }

    let mut meshes = PierMeshes::default();
    build_deck(&mut meshes, frame);
    build_bents(&mut meshes, frame, &sample).unwrap();
    let mut bents = stations(frame.quay_end + 0.84, frame.tip - 0.43, PILE_BAY);
    let x = f32::midpoint(bents.next().unwrap(), bents.next().unwrap());
    assert!(
        meshes
            .coarse_timber
            .triangles
            .chunks_exact(3)
            .all(|triangle| {
                let p = [triangle[0], triangle[1], triangle[2]]
                    .map(|i| meshes.coarse_timber.vertices[i as usize]);
                p.iter().all(|v| v.z > frame.sea)
                    || p.iter().all(|v| v.x > x + 0.5)
                    || p.iter().all(|v| v.x < x - 0.5)
            })
    );
}

#[test]
fn navigation_contains_only_a_continuous_ramp_and_deck() {
    let frame = frame();
    let mut navigation = Mesh::default();
    build_walkway(&mut navigation, frame);
    check_faces(&navigation);
    assert_eq!(navigation.triangles.len(), 12);
    assert!(navigation.normals.iter().all(|n| n.z > 0.98));
    for p in &navigation.vertices {
        assert!((p.z - frame.walking_height(p.x)).abs() < 1e-6);
        assert!(p.y.abs() < frame.edge);
    }
    for y in [
        -frame.edge + BERTH_WALK_INSET,
        frame.edge - CARGO_WALK_INSET,
    ] {
        assert_eq!(
            navigation
                .vertices
                .iter()
                .filter(|p| p.distance(Vec3::new(0., y, frame.deck)) < 1e-6)
                .count(),
            2
        );
    }
}

#[test]
fn cargo_points_stay_along_the_quayside_edge_clear_of_navigation_and_boarding() {
    let frame = frame();
    let berth_station = 25.;
    let points = prop_points(frame, berth_station);
    assert_eq!(points.len(), 3);
    for point in points {
        let local = point.position.truncate() - frame.shore;
        let station = local.dot(frame.axis);
        let lateral = local.dot(frame.axis.perp());
        assert!((frame.quay_end..berth_station - 3.).contains(&station));
        assert!(lateral * frame.berth_side < 0.);
        assert!(lateral.abs() > frame.edge - CARGO_WALK_INSET + 0.45);
        assert!((point.position.z - frame.deck).abs() < 1e-6);
        assert!(point.heading.distance(frame.axis) < 1e-6);
        assert_eq!(point.kind, "cargo");
    }
}

#[test]
fn missing_or_invalid_terrain_rejects_the_fitted_structure() {
    let frame = frame();
    let mut meshes = PierMeshes::default();
    assert!(build_quay(&mut meshes, frame, &|_| None).is_none());
    assert!(build_bents(&mut meshes, frame, &|_| Some(f32::NAN)).is_none());
}
