use island_building::{
    BuildingLod, BuildingRecipe, ExtensionKind, ExtensionRecipe, Facade, MaterialSlot,
    RoofCovering, WallStyle, generate,
};
use motu::Vec3;

fn house(facade: Facade, covering: RoofCovering) -> BuildingRecipe {
    let mut recipe = BuildingRecipe::hut();
    recipe.width = 9.;
    recipe.depth = 7.;
    recipe.wall_height = 2.8;
    recipe.roof.rise = 2.;
    recipe.roof.overhang = 0.35;
    recipe.roof.covering = covering;
    recipe.roof.thickness = if covering == RoofCovering::Thatch {
        0.5
    } else {
        0.12
    };
    recipe.walls.style = WallStyle::WattleAndDaub;
    recipe.chimney = None;
    recipe.atlas_size = 128;
    recipe.partitions.clear();
    recipe.openings.truncate(1);
    recipe.openings[0].facade = Facade::ALL[(facade.index() + 2) % 4];
    recipe.extensions = vec![ExtensionRecipe {
        id: "side-wing".into(),
        kind: ExtensionKind::Wing,
        facade,
        offset: 0.,
        width: 3.2,
        depth: 5.,
        wall_height: 2.8,
        roof_rise: 1.4,
        windows: false,
        ..ExtensionRecipe::default()
    }];
    recipe
}

fn ray_distance(lod: &BuildingLod, point: [f32; 3], up: bool, roof_only: bool) -> Option<f32> {
    let origin = Vec3::from_array(point);
    let direction = if up { Vec3::Z } else { -Vec3::Z };
    lod.parts
        .iter()
        .filter(|part| {
            !roof_only
                || matches!(
                    part.material,
                    MaterialSlot::Roof | MaterialSlot::ThatchEnd | MaterialSlot::Atlas
                )
        })
        .flat_map(|part| {
            part.mesh.indices.chunks_exact(3).filter_map(move |tri| {
                let [first, second, third] = [tri[0], tri[1], tri[2]]
                    .map(|index| Vec3::from_array(part.mesh.positions[index as usize]));
                let edge = second - first;
                let other_edge = third - first;
                let cross = direction.cross(other_edge);
                let determinant = edge.dot(cross);
                if determinant <= 1e-7 {
                    return None;
                }
                let offset = origin - first;
                let along = offset.dot(cross) / determinant;
                let offset_cross = offset.cross(edge);
                let across = direction.dot(offset_cross) / determinant;
                let distance = other_edge.dot(offset_cross) / determinant;
                (along >= -1e-5
                    && across >= -1e-5
                    && along + across <= 1. + 1e-5
                    && distance > 1e-5)
                    .then_some(distance)
            })
        })
        .min_by(f32::total_cmp)
}

#[test]
fn wing_ridge_continues_level_into_the_host_roof() {
    for covering in [
        RoofCovering::Thatch,
        RoofCovering::Slate,
        RoofCovering::StandingSeam,
    ] {
        for facade in [Facade::East, Facade::West] {
            let recipe = house(facade, covering);
            let prototype = generate(&recipe).unwrap();
            for (level, lod) in prototype.lods.iter().enumerate() {
                let outside = facade.point(&recipe, 0., 10., -1.);
                let reference = 10. - ray_distance(lod, outside, false, true).expect("wing ridge");
                for inward in [0.05, 0.5, 1., 2.] {
                    let point = facade.point(&recipe, 0., 10., inward);
                    let height =
                        10. - ray_distance(lod, point, false, true).expect("connected roof");
                    assert!(
                        (height - reference).abs() < 0.035,
                        "wing ridge droops at {facade:?} {covering:?} LOD{level}, inward{inward}: {height} versus {reference}"
                    );
                }
            }
        }
    }
}

#[test]
fn lean_to_continues_its_pitch_into_the_host_roof() {
    for facade in Facade::ALL {
        let mut recipe = house(facade, RoofCovering::StandingSeam);
        let extension = &mut recipe.extensions[0];
        extension.kind = ExtensionKind::LeanTo;
        extension.roof_rise = 3.;
        let slope = extension.roof_rise / (extension.depth + recipe.wall_thickness * 2. + 0.04);
        let prototype = generate(&recipe).unwrap();
        for (level, lod) in prototype.lods.iter().enumerate() {
            // Offset from the centre avoids standing-seam ribs. The high
            // annex roof remains visible here, above the host's roof.
            let outside = facade.point(&recipe, 0.08, 10., -0.8);
            let reference = 10. - ray_distance(lod, outside, false, true).unwrap();
            for inward in [0.05, 0.5, 1.] {
                let point = facade.point(&recipe, 0.08, 10., inward);
                let height = 10. - ray_distance(lod, point, false, true).unwrap();
                let expected = reference + (inward + 0.8) * slope;
                assert!(
                    (height - expected).abs() < 0.02,
                    "lean-to pitch bends at {facade:?} LOD{level}, inward{inward}: {height} versus {expected}"
                );
            }
        }
    }
}

#[test]
fn overlapping_eaves_have_a_closed_visible_underside() {
    for covering in [RoofCovering::Thatch, RoofCovering::Slate] {
        for facade in Facade::ALL {
            let recipe = house(facade, covering);
            let prototype = generate(&recipe).unwrap();
            for (level, lod) in prototype.lods.iter().enumerate() {
                for side in [-1., 1.] {
                    for outward in [0.04, 0.14, 0.24] {
                        for overhang in [0.04, 0.14, 0.24] {
                            let point =
                                facade.point(&recipe, side * (1.6 + overhang), 2.2, -outward);
                            assert!(
                                ray_distance(lod, point, true, false).is_some(),
                                "open eave corner {facade:?} {covering:?} LOD{level} at {point:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn low_attachments_reach_the_wall_without_cutting_the_host_soffit() {
    for covering in [
        RoofCovering::Thatch,
        RoofCovering::Slate,
        RoofCovering::StandingSeam,
    ] {
        for facade in Facade::ALL {
            for kind in [ExtensionKind::Wing, ExtensionKind::LeanTo] {
                let mut recipe = house(facade, covering);
                recipe.storeys = 2;
                let extension = &mut recipe.extensions[0];
                extension.kind = kind;
                extension.roof_rise = 0.75;
                let prototype = generate(&recipe).unwrap();
                for (level, lod) in prototype.lods.iter().enumerate() {
                    for along in [-0.9, 0., 0.9] {
                        for gap in [0.015, 0.12, 0.23] {
                            let low = facade.point(&recipe, along, 5., -gap);
                            assert!(
                                ray_distance(lod, low, false, true)
                                    .is_some_and(|distance| distance < 2.5),
                                "extension roof stops short of wall: {facade:?} {kind:?} {covering:?} LOD{level} at {low:?}"
                            );
                            let high = facade.point(&recipe, along, 5., -gap);
                            assert!(
                                ray_distance(lod, high, true, false).is_some(),
                                "missing host soffit above extension: {facade:?} {kind:?} {covering:?} LOD{level} at {high:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn high_thatched_wing_keeps_roof_surface_immediately_below_the_soffit() {
    let mut recipe = house(Facade::East, RoofCovering::Thatch);
    recipe.width = 6.;
    recipe.depth = 8.;
    recipe.storeys = 2;
    recipe.wall_height = 2.9;
    recipe.roof.rise = 3.;
    recipe.roof.thickness = 0.12;
    let wing = &mut recipe.extensions[0];
    wing.width = 4.;
    wing.depth = 6.;
    wing.wall_height = 2.9;
    wing.roof_rise = 3.;
    let prototype = generate(&recipe).unwrap();
    for (level, lod) in prototype.lods.iter().enumerate() {
        let outside = Facade::East.point(&recipe, 0.24, 6.055, -0.5);
        let reference = ray_distance(lod, outside, false, true).expect("wing below eave level");
        assert!(
            reference < 0.07,
            "sample misses the upper wing slope at LOD{level}"
        );
        for gap in [0.05, 0.12, 0.21] {
            let under_eave = Facade::East.point(&recipe, 0.24, 6.055, -gap);
            let distance = ray_distance(lod, under_eave, false, true);
            assert!(
                distance.is_some_and(|distance| (distance - reference).abs() < 0.015),
                "wing slope was cut away below the soffit at LOD{level}: {distance:?} vs {reference}"
            );
        }
    }
}
