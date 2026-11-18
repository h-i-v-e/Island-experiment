#![allow(clippy::float_cmp)] // Authored threshold and axis-aligned join planes.
use island_building::{BuildingRecipe, MaterialSlot, StairAxis, StairwellSide, stairs};
use std::path::{Path, PathBuf};

struct Materials(PathBuf);
impl Materials {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "stairwell-uv-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let mut material: serde_json::Value = serde_json::from_str(include_str!(
            "../../island-rs/texture-recipes/wattle-daub.json"
        ))
        .unwrap();
        material["physical_tile_width_m"] = 1.0.into();
        std::fs::write(
            path.join("wall.json"),
            serde_json::to_vec(&material).unwrap(),
        )
        .unwrap();
        Self(path)
    }
    fn recipe(axis: StairAxis) -> BuildingRecipe {
        let mut recipe = BuildingRecipe::procedural(20., 20.);
        recipe.storeys = 3;
        recipe.atlas_size = 128;
        recipe.chimney = None;
        recipe.staircase.axis = axis;
        recipe.staircase.position = Some([0.; 2]);
        recipe.opening_defaults.door.width = 0.95;
        recipe.materials.wall_recipe = Some("wall.json".into());
        recipe
    }
}
impl Drop for Materials {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn at_gap(
    mut recipe: BuildingRecipe,
    base: &Path,
    side: StairwellSide,
    gap: f32,
) -> BuildingRecipe {
    let centre = stairs::resolve_with_base(&recipe, base).unwrap().unwrap();
    let normal = side.outward();
    let origin = centre.point(0., 0., 0.);
    let tip = centre.point(normal[0], normal[1], 0.);
    let radius = if side.across() {
        centre.length() * 0.5
    } else {
        centre.span() * 0.5
    };
    let shift = recipe.width * 0.5 - recipe.wall_thickness - 0.12 - gap - radius;
    recipe.staircase.position = Some([(tip[0] - origin[0]) * shift, (tip[1] - origin[1]) * shift]);
    recipe
}

#[test]
fn every_side_reuses_the_exterior_strictly_below_one_authored_wall_unit_on_both_axes() {
    let materials = Materials::new();
    for axis in [StairAxis::X, StairAxis::Y] {
        for side in StairwellSide::ALL {
            for (gap, shared) in [(0.999, true), (1., false), (1.001, false)] {
                let recipe = at_gap(Materials::recipe(axis), &materials.0, side, gap);
                let stairs = stairs::resolve_with_base(&recipe, &materials.0)
                    .unwrap()
                    .unwrap();
                let room = stairs.stairwell.as_ref().unwrap();
                let plan = room.enclosure.unwrap();
                assert_eq!(plan.merge_unit, 1.);
                assert_eq!(
                    plan.shared_sides[side.index()],
                    shared,
                    "{axis:?} {side:?} gap {gap}"
                );
                assert_eq!(
                    plan.shared_sides.iter().filter(|s| **s).count(),
                    usize::from(shared)
                );
                if shared {
                    let centre = plan.surface(
                        side,
                        if side.across() {
                            stairs.span() * 0.5
                        } else {
                            stairs.length() * 0.5
                        },
                    );
                    let edge = stairs.point(centre[0], centre[1], 0.);
                    assert!(
                        (edge[0].abs().max(edge[1].abs()) - (10. - recipe.wall_thickness)).abs()
                            < 0.00001
                    );
                }
            }
        }
    }
}

#[test]
fn remaining_walls_join_the_reused_facade_without_a_second_wall_or_lost_glass() {
    let materials = Materials::new();
    for axis in [StairAxis::X, StairAxis::Y] {
        let recipe = at_gap(
            Materials::recipe(axis),
            &materials.0,
            StairwellSide::Left,
            0.5,
        );
        island_building::validation::validate_with_base(&recipe, &materials.0).unwrap();
        let prototype = island_building::generate_with_base(&recipe, &materials.0).unwrap();
        let stairs = prototype.layout.staircase.as_ref().unwrap();
        let room = stairs.stairwell.as_ref().unwrap();
        let plan = room.enclosure.unwrap();
        let removed = stairs.point(-room.wall_thickness * 0.5, stairs.length() * 0.5, 1.2);
        assert!(!prototype.colliders.iter().any(|c| c.contains(removed)));
        let join = stairs.point(plan.min[0], -room.wall_thickness * 0.5, 1.2);
        assert!(prototype.colliders.iter().any(|c| c.contains(join)));
        let extension = stairs.point(plan.min[0] * 0.5, -room.wall_thickness * 0.5, 1.2);
        assert!(prototype.colliders.iter().any(|c| c.contains(extension)));
        let mut open_recipe = recipe;
        open_recipe.staircase.enclosed = false;
        let open = island_building::generate_with_base(&open_recipe, &materials.0).unwrap();
        let glass = |p: &island_building::BuildingPrototype| {
            p.lods[0]
                .parts
                .iter()
                .find(|p| p.material == MaterialSlot::Glass)
                .unwrap()
                .mesh
                .positions
                .len()
        };
        assert_eq!(glass(&prototype), glass(&open));
        let path = materials.0.join("building.json");
        std::fs::write(&path, serde_json::to_vec(&prototype.recipe).unwrap()).unwrap();
        let document = island_building::document::StudioDocument::open(&path).unwrap();
        assert_eq!(
            document.recipe().materials.wall_recipe.as_deref(),
            Some("wall.json")
        );
    }
}

fn covers_plane_point(
    prototype: &island_building::BuildingPrototype,
    slot: MaterialSlot,
    axis: usize,
    point: [f32; 3],
) -> bool {
    let along = 1 - axis;
    prototype.lods[0]
        .parts
        .iter()
        .filter(|part| part.material == slot)
        .any(|part| {
            let mesh = &part.mesh;
            mesh.indices.chunks_exact(3).any(|tri| {
                let points = [tri[0], tri[1], tri[2]].map(|i| mesh.positions[i as usize]);
                if points
                    .iter()
                    .any(|p| (p[axis] - point[axis]).abs() > 0.00001)
                {
                    return false;
                }
                let [origin, first, second] = points;
                let area = |end: [f32; 3], other: [f32; 3]| {
                    (end[along] - origin[along]) * (other[2] - origin[2])
                        - (end[2] - origin[2]) * (other[along] - origin[along])
                };
                let total = area(first, second);
                if total.abs() < 0.000_001 {
                    return false;
                }
                let u = area(point, second) / total;
                let v = area(first, point) / total;
                u >= -0.00001 && v >= -0.00001 && u + v <= 1.00001
            })
        })
}

#[test]
fn stairwell_returns_share_a_timber_corner_without_overlapping_the_daub() {
    for axis in [StairAxis::X, StairAxis::Y] {
        let mut recipe = BuildingRecipe::procedural(12., 12.);
        recipe.chimney = None;
        recipe.storeys = 2;
        recipe.atlas_size = 128;
        recipe.staircase.axis = axis;
        recipe.staircase.position = Some([0.; 2]);
        recipe.walls.style = island_building::WallStyle::WattleAndDaub;
        let prototype = island_building::generate(&recipe).unwrap();
        let stairs = prototype.layout.staircase.as_ref().unwrap();
        let room = stairs.stairwell.as_ref().unwrap();
        let plan = room.enclosure.unwrap();
        assert_eq!(plan.shared_sides, [false; 4]);
        let t = room.wall_thickness;
        let p = recipe.walls.frame_projection;
        let origin = stairs.point(0., 0., 0.);
        let across = usize::from((stairs.point(1., 0., 0.)[1] - origin[1]).abs() > 0.5);
        for (side, x) in [(-1., plan.min[0]), (1., plan.max[0])] {
            for (end, y) in [(-1., plan.min[1]), (1., plan.max[1])] {
                let clash = stairs.point(x + side * t, y + end * t * 0.5, 1.17);
                assert!(covers_plane_point(
                    &prototype,
                    MaterialSlot::Interior,
                    across,
                    clash
                ));
                assert!(
                    !covers_plane_point(&prototype, MaterialSlot::Timber, across, clash),
                    "timber cap is coplanar with the plaster at {clash:?}"
                );
                for step in [-1.5, 0.5, 2.] {
                    let outer = stairs.point(x + side * (t + p), y + end * step * p, 1.17);
                    assert!(
                        covers_plane_point(&prototype, MaterialSlot::Timber, across, outer),
                        "missing outer timber at {outer:?}"
                    );
                }
                let hidden_end = stairs.point(x + side * t * 0.5, y, 1.17);
                assert!(
                    !covers_plane_point(&prototype, MaterialSlot::Interior, 1 - across, hidden_end),
                    "hidden plaster cap at {hidden_end:?}"
                );
            }
        }
    }
}
