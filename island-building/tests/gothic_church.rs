#![allow(clippy::many_single_char_names)]
use island_building::{BuildingRecipe, MaterialSlot, generate, validate};

fn church() -> BuildingRecipe {
    let mut r = BuildingRecipe::gothic_church();
    r.atlas_size = 128;
    r.foundation.stairs = false;
    r
}
fn contains(b: &island_building::collision::BoxCollider, p: [f32; 3]) -> bool {
    (0..3).all(|i| p[i] > b.min[i] && p[i] < b.max[i])
}

#[test]
fn cruciform_floors_connect_the_nave_transepts_chancel_and_two_towers() {
    let r = church();
    let p = generate(&r).unwrap();
    let c = p.layout.church.as_ref().unwrap();
    assert_eq!(c.outline.len(), 16);
    assert_eq!(p.layout.room_count, 1);
    for point in [
        [0., -10., -0.1],
        [10., 5., -0.1],
        [-10., 5., -0.1],
        [0., 14., -0.1],
        [-7., -17., -0.1],
        [7., -17., -0.1],
    ] {
        assert!(
            p.support_colliders.iter().any(|c| contains(c, point)),
            "missing floor at {point:?}"
        );
    }
    assert!(
        !p.support_colliders
            .iter()
            .any(|c| contains(c, [10., -8., -0.1]))
    );
    for i in -24_i16..=24 {
        let point = [f32::from(i) * 0.5, 5., 1.6];
        assert!(
            !p.colliders.iter().any(|c| contains(c, point)),
            "crossing blocked at {point:?}"
        );
    }
    for i in -34_i16..=34 {
        assert!(
            !p.colliders
                .iter()
                .any(|c| contains(c, [0., f32::from(i) * 0.5, 1.6]))
        );
    }
    assert!((c.towers[0].max[2] - 20.).abs() < 0.00001);
    assert!((c.towers[1].max[2] - 20.).abs() < 0.00001);
    assert!(p.layout.bounds_max[2] > 22.);
    assert_eq!(p.doors.len(), 2);
    assert!(p.doors.iter().all(|d| d.exterior));
}

fn ray_hits(mesh: &island_building::BuildingMesh, origin: [f32; 3], length: f32) -> bool {
    // X-directed ray, projected into YZ; checks actual triangles across the window.
    mesh.indices.chunks_exact(3).any(|ids| {
        let [a, b, c] = [ids[0], ids[1], ids[2]].map(|i| mesh.positions[i as usize]);
        let det = (b[1] - a[1]) * (c[2] - a[2]) - (c[1] - a[1]) * (b[2] - a[2]);
        if det.abs() < 0.00001 {
            return false;
        }
        let u = ((origin[1] - a[1]) * (c[2] - a[2]) - (origin[2] - a[2]) * (c[1] - a[1])) / det;
        let v = ((b[1] - a[1]) * (origin[2] - a[2]) - (b[2] - a[2]) * (origin[1] - a[1])) / det;
        let x = a[0] + u * (b[0] - a[0]) + v * (c[0] - a[0]);
        u >= 0. && v >= 0. && u + v <= 1. && x < origin[0] && x > origin[0] - length
    })
}
#[test]
fn pointed_windows_are_real_holes_at_lod0_and_opaque_at_lower_lods() {
    let p = generate(&church()).unwrap();
    let origin = [5.5, -12.5625, 3.0];
    let part = |level: usize, slot| {
        &p.lods[level]
            .parts
            .iter()
            .find(|p| p.material == slot)
            .unwrap()
            .mesh
    };
    assert!(!ray_hits(part(0, MaterialSlot::Wall), origin, 2.));
    assert!(ray_hits(part(0, MaterialSlot::Glass), origin, 2.));
    assert!(ray_hits(part(1, MaterialSlot::Wall), origin, 2.));
    assert!(
        p.lods[1]
            .parts
            .iter()
            .any(|p| p.material == MaterialSlot::Door),
        "the opaque middle shell needs a closed entrance proxy"
    );
    let wall = part(0, MaterialSlot::Wall);
    for ((point, normal), uv) in wall.positions.iter().zip(&wall.normals).zip(&wall.uvs) {
        if (point[0] - 4.5).abs() < 0.0001
            && normal[0].abs() > 0.99
            && (-14.4..0.9).contains(&point[1])
        {
            assert!(
                (uv[1] - point[2] * 0.5).abs() < 0.0001,
                "stone courses must stay level above arches"
            );
        }
    }
    for lod in &p.lods[1..] {
        assert!(!lod.parts.iter().any(|p| matches!(
            p.material,
            MaterialSlot::Glass | MaterialSlot::Floor | MaterialSlot::Ceiling
        )));
    }
    assert!(
        p.lods[2]
            .parts
            .iter()
            .all(|p| p.material == MaterialSlot::Atlas)
    );
    assert!(p.lods[0].triangle_count() > p.lods[1].triangle_count() * 2);
    assert!(p.lods[1].triangle_count() > p.lods[2].triangle_count() * 4);
    assert!(
        p.atlas
            .albedo
            .chunks_exact(4)
            .any(|c| c == [47, 70, 78, 255])
    );
}
#[test]
fn flying_buttresses_leave_open_space_beneath_their_arches() {
    let p = generate(&church()).unwrap();
    let y = -14.15;
    assert!(!p.colliders.iter().any(|c| contains(c, [6., y, 1.6])));
    assert!(p.colliders.iter().any(|c| contains(c, [6.05, y, 7.7])));
    assert!(p.colliders.iter().any(|c| contains(c, [7.5, y, 1.6])));
}
#[test]
fn church_meshes_are_finite_and_non_degenerate_and_recipes_round_trip() {
    let r = church();
    validate(&r).unwrap();
    let p = generate(&r).unwrap();
    for lod in &p.lods {
        for part in &lod.parts {
            assert_eq!(part.mesh.positions.len(), part.mesh.normals.len());
            for normal in &part.mesh.normals {
                assert!(normal.iter().all(|v| v.is_finite()));
                assert!((normal.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 0.0001);
            }
            for uv in &part.mesh.uvs {
                assert!(uv.iter().all(|v| v.is_finite()));
            }
            for ids in part.mesh.indices.chunks_exact(3) {
                let [a, b, c] = [ids[0], ids[1], ids[2]].map(|i| part.mesh.positions[i as usize]);
                let u = std::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
                let v = std::array::from_fn::<_, 3, _>(|i| c[i] - a[i]);
                let cross = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                assert!(
                    cross.iter().map(|v| v * v).sum::<f32>() > 1e-12,
                    "degenerate {:?} triangle {a:?} {b:?} {c:?}",
                    part.material
                );
            }
        }
    }
    let json = serde_json::to_vec(&p.recipe).unwrap();
    let loaded = serde_json::from_slice(&json).unwrap();
    let again = generate(&loaded).unwrap();
    assert_eq!(p.hash, again.hash);
    assert_eq!(p.lods, again.lods);
    assert_eq!(p.layout, again.layout);
    let mut preset: BuildingRecipe =
        serde_json::from_str(include_str!("../recipes/gothic-church.json")).unwrap();
    preset.atlas_size = 128;
    assert!(generate(&preset).is_ok());
}
#[test]
fn invalid_church_proportions_fail_before_generating_geometry() {
    let mut r = church();
    r.opening_defaults.door.height = 9.;
    assert!(validate(&r).is_err());
    let mut r = church();
    r.church.as_mut().unwrap().nave_width = 30.;
    assert!(validate(&r).is_err());
    let mut r = church();
    r.opening_defaults.window.height = 12.;
    assert!(validate(&r).is_err());
    let mut r = church();
    r.church.as_mut().unwrap().tower_height = f32::NAN;
    assert!(validate(&r).is_err());
}

#[test]
fn church_dimensions_and_tower_heights_can_vary_independently() {
    for (width, depth, height, nave, transept, offset, tower, tower_height, reach) in [
        (18., 26., 8., 6., 6., 2., 3., 18., 2.),
        (32., 40., 12., 10., 10., 5., 5., 24., 3.5),
    ] {
        let mut r = church();
        r.width = width;
        r.depth = depth;
        r.wall_height = height;
        r.roof.rise = 4.;
        r.opening_defaults.window.width = 1.6;
        r.opening_defaults.window.height = 4.8;
        r.opening_defaults.window.sill = 1.3;
        let c = r.church.as_mut().unwrap();
        c.nave_width = nave;
        c.transept_depth = transept;
        c.crossing_offset = offset;
        c.tower_width = tower;
        c.tower_height = tower_height;
        c.buttress_reach = reach;
        let p = generate(&r).unwrap();
        assert!(p.lods.iter().all(|lod| {
            lod.parts.iter().flat_map(|p| &p.mesh.normals).all(|n| {
                n.iter().all(|v| v.is_finite())
                    && (n.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 0.0001
            })
        }));
        assert!((p.layout.church.unwrap().towers[0].max[2] - tower_height).abs() < 0.00001);
    }
}

#[test]
fn a_church_package_regenerates_its_geometry_materials_and_cruciform_layout() {
    let p = generate(&church()).unwrap();
    let target = std::env::temp_dir().join(format!("gothic-church-package-{}", std::process::id()));
    let manifest =
        island_building::export::export_package(&p, &target, std::path::Path::new(".")).unwrap();
    let recipe =
        serde_json::from_slice(&std::fs::read(target.join("recipe.json")).unwrap()).unwrap();
    let regenerated = island_building::generate_with_base(&recipe, &target).unwrap();
    assert_eq!(p.hash, regenerated.hash);
    assert_eq!(p.lods, regenerated.lods);
    assert_eq!(p.doors, regenerated.doors);
    assert_eq!(p.layout, regenerated.layout);
    assert!(manifest.floor_policy.contains("cruciform"));
    std::fs::remove_dir_all(target).unwrap();
}

#[test]
fn eaves_project_from_every_roof_arm_without_moving_the_ceiling_or_collision() {
    let mut r = church();
    r.roof.overhang = 0.;
    let flush = generate(&r).unwrap();
    for overhang in [0., 0.45, 1.2] {
        r.roof.overhang = overhang;
        let p = generate(&r).unwrap();
        for lod in &p.lods[..2] {
            let roof = &lod
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Roof)
                .unwrap()
                .mesh;
            for (axis, extent) in [(0, r.width * 0.5 + overhang), (1, r.depth * 0.5 + overhang)] {
                let maximum = roof
                    .positions
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::NEG_INFINITY, f32::max);
                let minimum = roof
                    .positions
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::INFINITY, f32::min);
                assert!((maximum - extent).abs() < 0.0001);
                assert!((minimum + extent).abs() < 0.0001);
            }
        }
        let ceiling = &p.lods[0]
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Ceiling)
            .unwrap()
            .mesh;
        let original = &flush.lods[0]
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::Ceiling)
            .unwrap()
            .mesh;
        assert_eq!(ceiling, original);
        assert_eq!(p.colliders, flush.colliders);
    }
}

#[test]
fn church_stone_is_joint_free_noise_with_relief_and_separate_clean_inner_faces() {
    let p = generate(&church()).unwrap();
    let recipe: motu::procedural_textures::TextureRecipe = serde_json::from_slice(include_bytes!(
        "../../island-rs/texture-recipes/rough-stone.json"
    ))
    .unwrap();
    assert!(matches!(
        recipe.material,
        motu::procedural_textures::MaterialModel::LayeredNoise { .. }
    ));
    for slot in [
        MaterialSlot::Wall,
        MaterialSlot::Interior,
        MaterialSlot::Trim,
        MaterialSlot::Ceiling,
        MaterialSlot::Foundation,
    ] {
        let map = p.materials.iter().find(|m| m.slot == slot).unwrap();
        let (low, high) = map
            .normal
            .chunks_exact(4)
            .fold((255_u8, 0_u8), |(lo, hi), pixel| {
                (lo.min(pixel[0]), hi.max(pixel[0]))
            });
        assert!(high - low > 10, "missing stone relief on {slot:?}");
    }
    let inner = &p.lods[0]
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::Interior)
        .unwrap()
        .mesh;
    assert!(!ray_hits(inner, [5.5, -12.5625, 3.], 2.));
    assert!(ray_hits(inner, [5.5, -11., 3.], 2.));
    assert!(island_building::grub::exterior_material(
        &p.recipe,
        MaterialSlot::Wall
    ));
    assert!(!island_building::grub::exterior_material(
        &p.recipe,
        MaterialSlot::Interior
    ));
}
