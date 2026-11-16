use super::*;

fn straight(count: u32, angular_step: f64, rise: f64) -> (PlanetMesh, Vec<usize>, Vec<f64>) {
    let mesh = PlanetMesh {
        directions: (0..count)
            .map(|i| {
                let angle = f64::from(i) * angular_step;
                DVec3::new(angle.sin(), 0.0, angle.cos())
            })
            .collect(),
        triangles: Vec::new(),
        radius: 2000.0,
    };
    let path = (0..mesh.directions.len()).collect();
    let natural = (0..count)
        .map(|i| f64::from(count - 1 - i) * rise)
        .collect();
    (mesh, path, natural)
}

fn profile(mesh: &PlanetMesh, path: &[usize], natural: &[f64]) -> (Vec<f64>, Vec<Waterfall>) {
    let mut levels = natural.to_vec();
    let mut falls = Vec::new();
    stepped(
        mesh,
        path,
        natural,
        &|_| 0.00005,
        &mut levels,
        &mut falls,
        &[],
    );
    (levels, falls)
}

fn path_length(mesh: &PlanetMesh, path: &[usize], from: usize, to: usize) -> f64 {
    path[from..=to]
        .windows(2)
        .map(|pair| arc(mesh.directions[pair[0]], mesh.directions[pair[1]]))
        .sum()
}

#[test]
fn gradient_adaptive_reaches_keep_real_flats_and_supported_falls() {
    let (mesh, path, natural) = straight(61, 0.001, 0.000_125);
    let (levels, falls) = profile(&mesh, &path, &natural);
    assert!(falls.len() >= 2);
    for fall in &falls {
        let (minimum_drop, required_length) =
            cadence(&mesh, &path, &natural, fall.upper_vertex, fall.half_width);
        assert!(fall.upper_surface - fall.lower_surface >= minimum_drop - 1e-12);
        assert!((fall.upper_surface - natural[fall.upper_vertex]).abs() < 1e-12);
        assert!(
            path_length(&mesh, &path, 0, fall.upper_vertex)
                >= required_length.max(0.0025 * 2.0) - 1e-12
        );
        assert!((levels[fall.lower_vertex] - fall.lower_surface).abs() < 1e-12);
    }
    let flat_edges = levels
        .windows(2)
        .filter(|pair| (pair[0] - pair[1]).abs() < 1e-12)
        .count();
    assert!(
        flat_edges >= 10,
        "flat reaches must exist in the actual profile"
    );
    assert!(levels.windows(2).all(|pair| pair[0] >= pair[1]));
}

#[test]
fn gentle_grade_extends_until_the_legacy_gradient_drop_is_available() {
    let (mesh, path, natural) = straight(241, 0.001, 0.00002);
    let (_, falls) = profile(&mesh, &path, &natural);
    let first = falls
        .first()
        .expect("long gentle reach can supply its adaptive fall");
    let last_land = path.len() - 2;
    let length = path_length(&mesh, &path, first.upper_vertex, last_land);
    let (target, required_length) =
        cadence(&mesh, &path, &natural, first.upper_vertex, first.half_width);
    assert!(length >= target / 0.02 - 1e-12);
    assert!(
        length > required_length * 10.0,
        "a gentle river extends until its slope-dependent height is available"
    );
    assert!(first.upper_surface - first.lower_surface >= target - 1e-12);
    assert!(first.upper_surface - first.lower_surface <= target + 0.00002 + 1e-12);
}

#[test]
fn no_full_step_keeps_the_unfinished_profile_and_sea_edge_unchanged() {
    let (mesh, path, natural) = straight(12, 0.001, 0.00001);
    let (levels, falls) = profile(&mesh, &path, &natural);
    assert!(falls.is_empty());
    assert_eq!(levels, natural);

    let (mesh, path, natural) = straight(241, 0.001, 0.00002);
    let (levels, falls) = profile(&mesh, &path, &natural);
    let first_lip = falls.iter().map(|fall| fall.upper_vertex).min().unwrap();
    assert_eq!(&levels[..=first_lip], &natural[..=first_lip]);
    assert_eq!(&levels[levels.len() - 2..], &natural[natural.len() - 2..]);
}

#[test]
fn full_natural_height_reset_prevents_cumulative_upstream_loss() {
    let (mesh, path, natural) = straight(121, 0.001, 0.0003);
    let (levels, falls) = profile(&mesh, &path, &natural);
    assert!(falls.len() >= 4);
    let maximum_step = falls
        .iter()
        .map(|fall| fall.upper_surface - fall.lower_surface)
        .fold(0.0_f64, f64::max);
    assert!(
        levels
            .iter()
            .zip(&natural)
            .all(|(&level, &height)| height - level <= maximum_step + 1e-12)
    );
    assert!(
        falls
            .iter()
            .all(|fall| (fall.upper_surface - natural[fall.upper_vertex]).abs() < 1e-12)
    );
    assert_eq!(levels[0].to_bits(), natural[0].to_bits());
}

#[test]
fn blocked_support_keeps_natural_grade_instead_of_dragging_the_old_terrace_upstream() {
    let (mesh, path, natural) = straight(81, 0.001, 0.000_125);
    let junction = Junction {
        point: mesh.directions[40],
        half_width: 0.02,
    };
    let mut levels = natural.clone();
    let mut falls = Vec::new();
    stepped(
        &mesh,
        &path,
        &natural,
        &|_| 0.00005,
        &mut levels,
        &mut falls,
        std::slice::from_ref(&junction),
    );
    assert!(
        falls
            .iter()
            .all(|fall| !fall.footprint_contains(junction.point, junction.half_width * 2.5))
    );
    assert!((levels[40] - natural[40]).abs() < 1e-12);
    assert!(levels.windows(2).all(|pair| pair[0] >= pair[1]));
}

#[test]
fn legacy_gradient_cadence_preserves_normalized_results_when_radius_changes() {
    let (mut mesh, path, natural) = straight(81, 0.001, 0.000_125);
    let expected = profile(&mesh, &path, &natural);
    for radius in [1000.0, 6_371_000.0] {
        mesh.radius = radius;
        assert_eq!(profile(&mesh, &path, &natural), expected);
    }
}

#[test]
fn generated_world_keeps_actual_waterfalls_and_valid_channel_geometry() {
    let terrain = crate::generate_terrain(
        crate::PlanetOptions {
            seed: 42,
            vertex_count: 256,
            ..crate::PlanetOptions::default()
        },
        TerrainOptions {
            refinement_passes: 2,
            river_refinement_passes: 1,
            ..TerrainOptions::default()
        },
    )
    .unwrap();
    terrain.validate().unwrap();
    assert!(
        !terrain.waterfalls.is_empty(),
        "new step rules must survive final channel finishing"
    );
    assert!(
        terrain
            .waterfalls
            .iter()
            .all(|fall| fall.upper_surface - fall.lower_surface >= 0.2 * 0.0075 - 1e-12)
    );
    assert!(!terrain.river_surface.triangles.is_empty());
}

#[test]
fn legacy_cadence_responds_to_slope_and_width_instead_of_fixed_thresholds() {
    let (mesh, path, gentle) = straight(81, 0.001, 0.00002);
    let (_, _, steep) = straight(81, 0.001, 0.0005);
    let (gentle_drop, gentle_length) = cadence(&mesh, &path, &gentle, 40, 0.00005);
    let (steep_drop, steep_length) = cadence(&mesh, &path, &steep, 40, 0.00005);
    let (_, broad_length) = cadence(&mesh, &path, &gentle, 40, 0.006);
    assert!(steep_drop > gentle_drop);
    assert!(steep_length < gentle_length);
    assert!(broad_length > gentle_length);
    assert!((broad_length - (0.00075 + 0.006 * 2.5)).abs() < 1e-12);
}

#[test]
fn profile_prioritizes_true_catchment_and_keeps_parent_order_for_equal_areas() {
    let drainage = Drainage {
        receivers: vec![None, Some(0), Some(0), Some(2), Some(1), Some(4), Some(5)],
        catchment: vec![1.1, 1.0, 0.1, 0.05, 0.8, 0.8, 0.8],
        order: vec![0, 2, 3, 1, 4, 5, 6],
    };
    assert_eq!(
        profile_starts(&drainage, &[false, true, true, true, true, true, true]),
        [1, 4, 5, 6, 2, 3]
    );
}
