//! Renderer-independent, binary overhead shadows from the shared tree catalog.
use super::{TreePlacement, hybrid::HybridForest};
use crate::{Island, TreeMeshes, Vec2};

/// An island-local, square R8 canopy mask. Occupied texels are 255, clear texels
/// are zero. Row zero starts at normalized island Y=0; no engine Y/Z conversion
/// is applied. Renderers may generate filtered mipmaps after uploading it.
#[derive(Debug, Eq, PartialEq)]
pub struct ForestShadowMask {
    resolution: u32,
    pixels: Vec<u8>,
}

impl ForestShadowMask {
    #[must_use]
    pub const fn resolution(&self) -> u32 {
        self.resolution
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Transfer the allocation into a renderer's image without copying pixels.
    #[must_use]
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}

impl Island {
    /// Bake an overhead circle for every standing tree, without streaming or
    /// terrain generation. Returns `None` for legacy forests or a resolution
    /// that is not a power of two from 1 through 2048.
    #[must_use]
    pub fn forest_shadow_mask(&self, resolution: u32) -> Option<ForestShadowMask> {
        let length = pixel_count(resolution)?;
        let (catalog, placements) = self.hybrid_forest()?;
        let mut pixels = vec![0; length];
        rasterize(catalog, placements, resolution, &mut pixels);
        Some(ForestShadowMask { resolution, pixels })
    }

    /// Write into a renderer's existing R8 allocation. Invalid resolutions,
    /// legacy forests and incorrectly sized buffers return false without
    /// modifying the buffer. Valid empty forests clear the complete mask.
    #[must_use]
    pub fn write_forest_shadow_mask(&self, resolution: u32, pixels: &mut [u8]) -> bool {
        let Some(length) = pixel_count(resolution) else {
            return false;
        };
        let Some((catalog, placements)) = self.hybrid_forest() else {
            return false;
        };
        if pixels.len() != length {
            return false;
        }
        rasterize(catalog, placements, resolution, pixels);
        true
    }
}

pub(crate) fn pixel_count(resolution: u32) -> Option<usize> {
    (resolution.is_power_of_two() && resolution <= 2048)
        .then(|| resolution as usize * resolution as usize)
}

#[derive(Clone, Copy)]
struct Crown {
    centre: Vec2,
    radius: f32,
}

impl Crown {
    fn from_tree(tree: &TreeMeshes) -> Option<Self> {
        let vertices = &tree.lod0_foliage.vertices;
        let first = vertices.first()?.truncate();
        let (minimum, maximum) = vertices.iter().fold((first, first), |(min, max), p| {
            (min.min(p.truncate()), max.max(p.truncate()))
        });
        let extents = (maximum - minimum) * 0.5;
        let radius = extents.max_element();
        (radius > 0.0).then_some(Self {
            centre: (minimum + maximum) * 0.5,
            radius,
        })
    }
}

fn rasterize(
    catalog: &HybridForest,
    placements: &[TreePlacement],
    resolution: u32,
    pixels: &mut [u8],
) {
    // Catalog-sized scratch only: per-tree circles never need their own array.
    let crowns: Vec<_> = catalog.prototypes.iter().map(Crown::from_tree).collect();
    pixels.fill(0);
    for tree in placements {
        let Some(crown) = crowns[usize::from(tree.prototype)] else {
            continue;
        };
        let (sin, cos) = tree.yaw_radians.sin_cos();
        let offset = Vec2::new(
            cos * crown.centre.x - sin * crown.centre.y,
            sin * crown.centre.x + cos * crown.centre.y,
        ) * tree.scale;
        stamp(
            pixels,
            resolution,
            tree.anchor.truncate() + offset,
            crown.radius * tree.scale,
        );
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
fn stamp(pixels: &mut [u8], resolution: u32, centre: Vec2, radius: f32) {
    let centre = centre * resolution as f32 - Vec2::splat(0.5);
    let radius = radius * resolution as f32;
    let last = resolution as i32 - 1;
    let minimum_y = ((centre.y - radius).ceil() as i32).max(0);
    let maximum_y = ((centre.y + radius).floor() as i32).min(last);
    for y in minimum_y..=maximum_y {
        let offset_y = y as f32 - centre.y;
        let half_width = (radius * radius - offset_y * offset_y).max(0.0).sqrt();
        let minimum_x = ((centre.x - half_width).ceil() as i32).max(0);
        let maximum_x = ((centre.x + half_width).floor() as i32).min(last);
        if minimum_x <= maximum_x {
            let start = y as usize * resolution as usize + minimum_x as usize;
            pixels[start..start + (maximum_x - minimum_x + 1) as usize].fill(255);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mesh, Vec3};

    fn catalog() -> HybridForest {
        HybridForest {
            prototypes: vec![TreeMeshes {
                lod0_foliage: Mesh {
                    vertices: vec![Vec3::new(-0.02, -0.04, 0.1), Vec3::new(0.06, 0.04, 0.1)],
                    ..Mesh::default()
                },
                ..TreeMeshes::default()
            }]
            .into(),
            broadleaf_count: 1,
        }
    }

    fn tree(anchor: Vec2, scale: f32, yaw_radians: f32) -> TreePlacement {
        TreePlacement {
            anchor: anchor.extend(0.2),
            scale,
            yaw_radians,
            seed_ordinal: 0,
            terrain_vertex: 0,
            prototype: 0,
        }
    }

    #[test]
    fn binary_canopy_circles_apply_catalog_offsets_rotation_and_scale() {
        let mut pixels = vec![0; 256 * 256];
        let trees = [tree(Vec2::new(0.25, 0.7), 2.0, std::f32::consts::FRAC_PI_2)];
        rasterize(&catalog(), &trees, 256, &mut pixels);
        assert_eq!(pixels[189 * 256 + 64], 255, "rotated canopy centre");
        assert_eq!(pixels[189 * 256 + 81], 255, "scaled radius");
        assert_eq!(pixels[189 * 256 + 87], 0, "outside scaled radius");
        assert_eq!(pixels[128 * 256 + 128], 0);
        assert!(pixels.iter().all(|&value| value == 0 || value == 255));
    }

    #[test]
    fn clipped_edges_do_not_wrap_and_overlaps_are_a_union() {
        let mut once = vec![0; 256 * 256];
        let edge = tree(Vec2::new(0.99, 0.01), 1.0, 0.0);
        rasterize(&catalog(), &[edge], 256, &mut once);
        assert_eq!(once[255], 255);
        assert_eq!(once[0], 0);
        assert_eq!(once[255 * 256 + 255], 0);
        let mut twice = vec![0; once.len()];
        rasterize(&catalog(), &[edge, edge], 256, &mut twice);
        assert_eq!(once, twice);
        rasterize(&catalog(), &[], 256, &mut twice);
        assert!(twice.iter().all(|&value| value == 0));
    }

    #[test]
    fn supported_resolutions_bound_the_allocation() {
        for resolution in [1, 2, 256, 1024, 2048] {
            assert_eq!(
                pixel_count(resolution),
                Some((resolution * resolution) as usize)
            );
        }
        for resolution in [0, 3, 4096, u32::MAX] {
            assert_eq!(pixel_count(resolution), None);
        }
    }
}
