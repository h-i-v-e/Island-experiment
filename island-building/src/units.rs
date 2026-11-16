//! Physical material modules used by the editor; recipes continue to store metres.
use crate::{BuildingError, BuildingRecipe, MaterialSlot};
use motu::procedural_textures::{MaterialModel, TextureRecipe};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialGrid {
    /// Metres per module along the texture's U and V axes.
    pub metres: [f32; 2],
    pub labels: [&'static str; 2],
}

impl MaterialGrid {
    pub fn load(
        building: &BuildingRecipe,
        base: &Path,
        slot: MaterialSlot,
    ) -> Result<Self, BuildingError> {
        let material = crate::materials::unit_material(building, base, slot)?;
        Self::from_material(&material)
    }

    pub fn from_material(material: &TextureRecipe) -> Result<Self, BuildingError> {
        motu::procedural_textures::validate_recipe(material)
            .map_err(|error| BuildingError::new("material grid", error.to_string()))?;
        let (counts, labels) = match &material.material {
            MaterialModel::WoodenPlanks(settings) => (
                [settings.planks, settings.joints_per_plank.max(1)],
                ["board width", "board length / repeat"],
            ),
            MaterialModel::Masonry(settings) => (
                [settings.cells_x, settings.cells_y],
                ["stone / brick width", "course height"],
            ),
            MaterialModel::SlateRoof(settings) => (
                [settings.courses, settings.slates_per_course],
                ["slate course", "slate width"],
            ),
            MaterialModel::ThatchRoof(settings) => (
                [settings.courses, settings.bundles],
                ["thatch course", "bundle width"],
            ),
            _ => ([1, 1], ["texture repeat", "texture repeat"]),
        };
        #[allow(clippy::cast_precision_loss)] // Material counts are validated and bounded.
        let metres = [
            material.physical_tile_width_m / counts[0] as f32,
            material.physical_tile_height_m / counts[1] as f32,
        ];
        Ok(Self { metres, labels })
    }
}

/// Round a physical distance about an explicit edge, including negative offsets.
#[must_use]
pub fn snap(metres: f32, step: f32, origin: f32) -> f32 {
    ((metres - origin) / step).round() * step + origin
}

/// Texture phase on a facade, measured from its left corner at floor level.
#[must_use]
pub fn wall_uv(length: f32, along: f32, height: f32) -> [f32; 2] {
    [(along + length * 0.5) * 0.5, height * 0.5]
}

/// Continuous box charts, with vertical courses anchored to floor level.
pub(crate) fn box_uv(bounds: crate::collision::BoxCollider, p: [f32; 3], n: [f32; 3]) -> [f32; 2] {
    let metres = if n[2].abs() > 0.5 {
        [p[0] - bounds.min[0], p[1] - bounds.min[1]]
    } else if n[1] < -0.5 {
        [p[0] - bounds.min[0], p[2]]
    } else if n[1] > 0.5 {
        [bounds.max[0] - p[0], p[2]]
    } else if n[0] > 0.5 {
        [p[1] - bounds.min[1], p[2]]
    } else {
        [bounds.max[1] - p[1], p[2]]
    };
    metres.map(|m| m * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_follow_authored_physical_sizes_and_material_axes() {
        let building = BuildingRecipe::hut();
        let floor = MaterialGrid::load(&building, Path::new("."), MaterialSlot::Floor).unwrap();
        assert!((floor.metres[0] - 0.2).abs() < 0.00001);
        assert!((floor.metres[1] - 2.4).abs() < 0.00001);
        let stone = MaterialGrid::load(&building, Path::new("."), MaterialSlot::Chimney).unwrap();
        assert!(
            stone
                .metres
                .iter()
                .zip([2., 2.])
                .all(|(a, b)| (a - b).abs() < 0.00001)
        );
        let mut custom: TextureRecipe = serde_json::from_str(include_str!(
            "../../island-rs/texture-recipes/slate-roof.json"
        ))
        .unwrap();
        custom.physical_tile_width_m = 3.;
        custom.physical_tile_height_m = 4.;
        let MaterialModel::SlateRoof(settings) = &custom.material else {
            panic!("slate")
        };
        let grid = MaterialGrid::from_material(&custom).unwrap();
        #[allow(clippy::cast_precision_loss)]
        let expected = [
            3. / settings.courses as f32,
            4. / settings.slates_per_course as f32,
        ];
        assert!(
            grid.metres
                .iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 0.00001)
        );
        custom.physical_tile_width_m = 0.;
        assert!(MaterialGrid::from_material(&custom).is_err());
    }

    #[test]
    fn edge_based_offsets_keep_both_sides_of_odd_width_openings_on_grid() {
        let width = snap(1.42, 0.5, 0.);
        let left = snap(-0.65 - width * 0.5, 0.5, -3.);
        let centre = left + width * 0.5;
        for edge in [centre - width * 0.5, centre + width * 0.5] {
            let uv = wall_uv(6., edge, 2.25);
            assert!((uv[0] * 2. / 0.5).fract().abs() < 0.00001);
            assert!((uv[1] * 2. / 0.25 - 9.).abs() < 0.00001);
        }
        assert!((snap(-0.63, 0.25, 0.) + 0.75).abs() < 0.00001);
    }
}
