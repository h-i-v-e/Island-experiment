use island_building::{BuildingRecipe, GrubFilter, generate, validate_recipe};

#[test]
fn shader_settings_round_trip_without_changing_base_maps_geometry_or_collision() {
    let mut value = serde_json::to_value(BuildingRecipe::hut()).unwrap();
    value.as_object_mut().unwrap().remove("grub_filter");
    let mut recipe: BuildingRecipe = serde_json::from_value(value).unwrap();
    recipe.atlas_size = 128;
    assert!(!recipe.grub_filter.enabled);
    let clean = generate(&recipe).unwrap();
    recipe.grub_filter.enabled = true;
    recipe.grub_filter.moss = 0.9;
    recipe.grub_filter.patch_size = 2.;
    recipe.grub_filter.fine_patch_size = 0.25;
    recipe.grub_filter.fine_mix = 0.4;
    let grubby = generate(&recipe).unwrap();
    assert_ne!(clean.hash, grubby.hash);
    assert_eq!(clean.materials, grubby.materials);
    assert_eq!(clean.atlas, grubby.atlas);
    assert_eq!(clean.lods, grubby.lods);
    assert_eq!(clean.colliders, grubby.colliders);
    assert_eq!(clean.stair_colliders, grubby.stair_colliders);
    assert_eq!(clean.doors, grubby.doors);
    assert_eq!(
        serde_json::from_value::<BuildingRecipe>(serde_json::to_value(&recipe).unwrap()).unwrap(),
        recipe
    );
}

#[test]
fn invalid_grub_parameters_are_diagnosed_before_generation() {
    let mut recipe = BuildingRecipe::hut();
    for filter in [
        GrubFilter {
            patch_size: 0.,
            ..Default::default()
        },
        GrubFilter {
            height_falloff: f32::INFINITY,
            ..Default::default()
        },
        GrubFilter {
            moss: 1.1,
            ..Default::default()
        },
        GrubFilter {
            fine_patch_size: 0.,
            ..Default::default()
        },
        GrubFilter {
            fine_mix: 1.1,
            ..Default::default()
        },
        GrubFilter {
            dirt_colour: [f32::NAN, 0., 0.],
            ..Default::default()
        },
    ] {
        recipe.grub_filter = filter;
        assert!(
            validate_recipe(&recipe)
                .unwrap_err()
                .to_string()
                .contains("grub_filter")
        );
    }
}

#[test]
fn existing_filter_recipes_gain_a_separate_fine_layer_without_resetting_the_broad_layer() {
    let filter: GrubFilter = serde_json::from_value(serde_json::json!({
        "enabled": true, "patch_size": 2., "height_falloff": 5.
    }))
    .unwrap();
    assert_eq!(
        [filter.patch_size, filter.height_falloff].map(f32::to_bits),
        [2_f32, 5.].map(f32::to_bits)
    );
    assert!(filter.fine_patch_size < filter.patch_size * 0.25);
    assert!(filter.fine_mix > 0. && filter.fine_mix < 1.);
    let uniforms = filter.uniform_vectors(42);
    assert_eq!(uniforms[0][3].to_bits(), filter.patch_size.to_bits());
    assert_eq!(
        uniforms[4].map(f32::to_bits),
        [filter.fine_patch_size, filter.fine_mix, 0., 0.].map(f32::to_bits)
    );
}
