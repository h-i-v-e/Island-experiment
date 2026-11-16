use island_building::{
    BuildingPrototype, BuildingRecipe, MaterialSlot, generate, geometry::chimney::hearth_bounds,
};

fn texture(prototype: &BuildingPrototype, slot: MaterialSlot) -> &island_building::MaterialTexture {
    prototype.materials.iter().find(|m| m.slot == slot).unwrap()
}

#[test]
fn legacy_recipes_gain_joint_free_slabs_and_recolour_them_independently() {
    let mut value = serde_json::to_value(BuildingRecipe::hut()).unwrap();
    value["materials"]
        .as_object_mut()
        .unwrap()
        .remove("stone_slab");
    value["materials"]
        .as_object_mut()
        .unwrap()
        .remove("stone_slab_recipe");
    let mut recipe: BuildingRecipe = serde_json::from_value(value).unwrap();
    recipe.atlas_size = 128;
    assert!(recipe.materials.stone_slab_recipe.is_none());
    let original = generate(&recipe).unwrap();
    recipe.materials.stone_slab = [0.4, 0.3, 0.2];
    let changed = generate(&recipe).unwrap();
    for slot in [
        MaterialSlot::Foundation,
        MaterialSlot::Chimney,
        MaterialSlot::Floor,
    ] {
        assert_eq!(texture(&original, slot), texture(&changed, slot));
    }
    let (a, b) = (
        texture(&original, MaterialSlot::StoneSlab),
        texture(&changed, MaterialSlot::StoneSlab),
    );
    assert_ne!(a.albedo, b.albedo);
    assert_eq!(a.normal, b.normal);
    assert_ne!(original.hash, changed.hash);
    let material: motu::procedural_textures::TextureRecipe = serde_json::from_str(include_str!(
        "../../island-rs/texture-recipes/stone-slab.json"
    ))
    .unwrap();
    assert!(matches!(
        material.material,
        motu::procedural_textures::MaterialModel::LayeredNoise { .. }
    ));
    assert!(material.layers.is_empty(), "no joint/cell layers");
}

#[test]
fn hearth_is_one_solid_stone_piece_and_stair_caps_have_their_own_groups_at_every_lod() {
    let mut recipe = BuildingRecipe::hut();
    recipe.atlas_size = 128;
    let prototype = generate(&recipe).unwrap();
    let bounds = hearth_bounds(&recipe).unwrap();
    let hearth = &prototype.lods[0]
        .parts
        .iter()
        .find(|p| p.material == MaterialSlot::StoneSlab)
        .unwrap()
        .mesh;
    assert_eq!(hearth.triangle_count(), 12);
    assert_eq!(hearth.positions.len(), 24);
    assert!(hearth.positions.iter().all(|p| bounds.contains(*p)));
    assert!(prototype.colliders.contains(&bounds));
    for lod in &prototype.lods {
        assert!(
            lod.parts
                .iter()
                .any(|p| p.material == MaterialSlot::StairTreads)
        );
    }
    assert!(prototype.lods[1..].iter().all(|lod| {
        lod.parts
            .iter()
            .all(|p| p.material != MaterialSlot::StoneSlab)
    }));
    recipe.foundation.stairs = false;
    let no_stairs = generate(&recipe).unwrap();
    assert!(
        no_stairs
            .lods
            .iter()
            .all(|lod| lod.parts.iter().all(|p| !p.material.is_stairs()))
    );
    assert_eq!(
        &no_stairs.lods[0]
            .parts
            .iter()
            .find(|p| p.material == MaterialSlot::StoneSlab)
            .unwrap()
            .mesh,
        hearth
    );
    assert!(no_stairs.stair_colliders.is_empty());
}
