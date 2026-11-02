//! Debounce colour edits and replace the shared albedo array atomically.
#![allow(clippy::float_cmp)] // Exact setting identity controls stale-worker rejection.
use super::{TerrainAppearance, TerrainTextures, mipmapped_array};
use crate::{details::GrassMaterial, lod_material::LodMaterial};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use std::time::{Duration, Instant};

#[derive(Resource)]
pub(crate) struct BakedPalette(pub [f32; 3]);
impl Default for BakedPalette {
    fn default() -> Self {
        Self(TerrainAppearance::default().dirt_colour)
    }
}
type BakeJob = ([f32; 3], Task<Result<Image, String>>);
#[derive(Resource, Default)]
pub(crate) struct ColourBaker {
    requested: Option<([f32; 3], Instant)>,
    job: Option<BakeJob>,
    pending: bool,
}
impl ColourBaker {
    pub fn settled(&self) -> bool {
        self.job.is_none() && !self.pending
    }
}
pub(crate) fn rebake_albedo(
    appearance: Res<TerrainAppearance>,
    mut textures: ResMut<TerrainTextures>,
    mut reference: ResMut<BakedPalette>,
    mut state: ResMut<ColourBaker>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<LodMaterial>>,
    mut grass: ResMut<Assets<GrassMaterial>>,
) {
    let target = appearance.dirt_colour;
    if state
        .requested
        .as_ref()
        .is_none_or(|(colour, _)| *colour != target)
    {
        state.requested = Some((target, Instant::now()));
        state.pending = target != reference.0;
    }
    if let Some((colour, job)) = &mut state.job
        && let Some(result) = block_on(poll_once(job))
    {
        let colour = *colour;
        state.job = None;
        if colour == target {
            match result {
                Ok(image) => {
                    // Bevy 0.19's in-place image upload writes mip 0 only and
                    // treats this layer-major mip chain as contiguous base layers.
                    // A fresh GPU image uploads every layer/mip correctly. Rebind
                    // existing materials as well as the shared source for new chunks.
                    let previous = std::mem::replace(&mut textures.albedo, images.add(image));
                    rebind_albedo(&previous, &textures.albedo, &mut materials, &mut grass);
                    reference.0 = colour;
                }
                Err(error) => warn!("Ground colour bake: {error}"),
            }
            state.pending = false;
        }
    }
    let ready = state
        .requested
        .is_some_and(|(_, when)| when.elapsed() >= Duration::from_millis(300));
    if state.job.is_none() && state.pending && ready {
        state.job = Some((
            target,
            AsyncComputeTaskPool::get().spawn(async move { bake(target, 512) }),
        ));
    }
}
fn rebind_albedo(
    previous: &Handle<Image>,
    current: &Handle<Image>,
    materials: &mut Assets<LodMaterial>,
    grass: &mut Assets<GrassMaterial>,
) {
    // Collect only dependent IDs before taking mutable asset borrows. iter_mut()
    // marks every asset dirty, including unrelated water and rock materials.
    let terrain_ids: Vec<_> = materials
        .iter()
        .filter_map(|(id, material)| {
            (material.extension.albedo.as_ref() == Some(previous)).then_some(id)
        })
        .collect();
    for id in terrain_ids {
        materials.get_mut(id).unwrap().extension.albedo = Some(current.clone());
    }
    let grass_ids: Vec<_> = grass
        .iter()
        .filter_map(|(id, material)| {
            (material.base.extension.albedo.as_ref() == Some(previous)).then_some(id)
        })
        .collect();
    for id in grass_ids {
        grass.get_mut(id).unwrap().base.extension.albedo = Some(current.clone());
    }
}

fn bake(dirt: [f32; 3], size: u32) -> Result<Image, String> {
    let colour = |srgb: [f32; 3]| {
        let [r, g, b, _] = Color::srgb(srgb[0], srgb[1], srgb[2])
            .to_linear()
            .to_f32_array();
        motu::LinearRgb::new(r, g, b)
    };
    let maps = motu::bake_island_materials(
        &motu::RuntimeMaterialInputs::new(
            dirt.into(),
            colour([0.30, 0.32, 0.29]),
            colour([0.62, 0.57, 0.34]),
        ),
        &motu::RuntimeMaterialBakeOptions {
            width: Some(size),
            height: Some(size),
            materials: motu::MaterialSelection::TERRAIN,
            ..default()
        },
    )
    .map_err(|e| e.to_string())?;
    let pixels = maps
        .materials
        .into_values()
        .flat_map(|maps| maps.albedo.into_pixels())
        .flat_map(|[r, g, b]| [r, g, b, 255])
        .collect();
    Ok(mipmapped_array(
        Image::new(
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        ),
        true,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[allow(clippy::too_many_lines)] // Existing terrain/grass, future chunks and stale results share one lifecycle.
    fn live_bake_uploads_a_fresh_complete_image_and_discards_stale_colour_results() {
        use bevy::asset::{AssetApp, AssetPlugin};
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<LodMaterial>()
            .init_asset::<GrassMaterial>()
            .init_resource::<TerrainAppearance>()
            .init_resource::<BakedPalette>()
            .init_resource::<ColourBaker>()
            .add_systems(Update, rebake_albedo);
        let initial = bake(TerrainAppearance::default().dirt_colour, 32).unwrap();
        let albedo = app.world_mut().resource_mut::<Assets<Image>>().add(initial);
        app.insert_resource(TerrainTextures {
            albedo: albedo.clone(),
            ..default()
        });
        let existing: Vec<_> = (0..4)
            .map(|_| {
                let ground = LodMaterial {
                    extension: app.world().resource::<TerrainTextures>().extension(2000.0),
                    ..default()
                };
                app.world_mut()
                    .resource_mut::<Assets<LodMaterial>>()
                    .add(ground)
            })
            .collect();
        let shell = GrassMaterial {
            base: LodMaterial {
                extension: app.world().resource::<TerrainTextures>().extension(2000.0),
                ..default()
            },
            ..default()
        };
        let shell = app
            .world_mut()
            .resource_mut::<Assets<GrassMaterial>>()
            .add(shell);
        let unrelated = app
            .world_mut()
            .resource_mut::<Assets<LodMaterial>>()
            .add(LodMaterial::default());
        app.update();
        app.world_mut()
            .resource_mut::<Messages<bevy::asset::AssetEvent<LodMaterial>>>()
            .clear();
        let target = [0.2, 0.05, 0.01];
        app.world_mut()
            .resource_mut::<TerrainAppearance>()
            .dirt_colour = target;
        let pixels = bake(target, 32).unwrap();
        let expected = pixels.data.clone();
        app.world_mut().resource_mut::<ColourBaker>().job = Some((
            target,
            AsyncComputeTaskPool::get().spawn(async move { Ok(pixels) }),
        ));
        for _ in 0..100 {
            app.update();
            if app.world().resource::<ColourBaker>().job.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.world().resource::<BakedPalette>().0, target);
        let uploaded = app.world().resource::<TerrainTextures>().albedo.clone();
        assert_ne!(uploaded, albedo, "in-place updates leave GPU mips stale");
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&uploaded)
                .unwrap()
                .data,
            expected
        );
        for handle in &existing {
            let material = app
                .world()
                .resource::<Assets<LodMaterial>>()
                .get(handle)
                .unwrap();
            assert_eq!(material.extension.albedo, Some(uploaded.clone()));
            assert_eq!(material.extension.normal, Some(Handle::default()));
            assert_eq!(material.extension.mask, Some(Handle::default()));
        }
        assert_eq!(
            app.world()
                .resource::<Assets<GrassMaterial>>()
                .get(&shell)
                .unwrap()
                .base
                .extension
                .albedo,
            Some(uploaded.clone())
        );
        assert!(
            app.world_mut()
                .resource_mut::<Messages<bevy::asset::AssetEvent<LodMaterial>>>()
                .drain()
                .all(|event| !event.is_modified(unrelated.id()))
        );
        let future = app.world().resource::<TerrainTextures>().extension(2000.0);
        assert_eq!(future.albedo, Some(uploaded.clone()));
        let newer = [0.01, 0.02, 0.3];
        app.world_mut()
            .resource_mut::<TerrainAppearance>()
            .dirt_colour = newer;
        let stale = bake(target, 32).unwrap();
        app.world_mut().resource_mut::<ColourBaker>().job = Some((
            target,
            AsyncComputeTaskPool::get().spawn(async move { Ok(stale) }),
        ));
        for _ in 0..100 {
            app.update();
            if app.world().resource::<ColourBaker>().job.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.world().resource::<BakedPalette>().0, target);
        assert!(app.world().resource::<ColourBaker>().pending);
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&uploaded)
                .unwrap()
                .data,
            expected
        );
    }
    #[test]
    fn soil_parameter_rebakes_composite_ground_colours_and_preserves_other_recipes() {
        let first = bake([0.01, 0.005, 0.002], 32).unwrap();
        let second = bake([0.35, 0.02, 0.01], 32).unwrap();
        let first = first.data.unwrap();
        let second = second.data.unwrap();
        let stride = (0..=5)
            .map(|level| (32_usize >> level).pow(2) * 4)
            .sum::<usize>();
        for layer in 0..6 {
            let range = layer * stride..(layer + 1) * stride;
            if [0, 1, 3, 5].contains(&layer) {
                assert_ne!(
                    first[range.clone()],
                    second[range],
                    "soil-bearing layer {layer} must update"
                );
            } else {
                assert_eq!(
                    first[range.clone()],
                    second[range],
                    "non-soil layer {layer} remains stable"
                );
            }
        }
    }
}
