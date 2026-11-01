//! Bounded, shared silhouettes: compact conifer whorls and low-poly broadleaf crowns.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
use motu::{Mesh, TreeMeshes, Vec3};
use std::f32::consts::TAU;

pub(super) fn conifer(variant: usize) -> TreeMeshes {
    let height = 13.5 + variant as f32 * 1.3;
    let mut wood = Mesh::default();
    whorl(
        &mut wood,
        Vec3::ZERO,
        0.19,
        height,
        6,
        variant as f32,
        false,
    );
    wood.calculate_normals();
    let foliage = conifer_crown(height, variant, 8, 9);
    let coarse = conifer_crown(height, variant, 5, 6);
    let mut tree = TreeMeshes::default();
    tree.lod0_wood = wood.clone();
    tree.lod1_wood = wood.clone();
    tree.lod2_wood = wood;
    tree.lod0_foliage = foliage;
    tree.lod1_foliage = coarse;
    tree
}
fn conifer_crown(height: f32, variant: usize, tiers: usize, sides: usize) -> Mesh {
    let mut mesh = Mesh::default();
    for i in 0..tiers {
        let t = i as f32 / (tiers - 1) as f32;
        let base = 1.6 + t * (height - 4.0);
        let spread = (3.0 + variant as f32 * 0.12) * (1.0 - t).powf(0.85) + 0.16;
        whorl(
            &mut mesh,
            Vec3::Z * base,
            spread,
            (height - base).min(3.7),
            sides,
            variant as f32 + i as f32 * 0.73,
            true,
        );
    }
    mesh.calculate_normals();
    mesh
}
fn whorl(
    mesh: &mut Mesh,
    centre: Vec3,
    radius: f32,
    height: f32,
    sides: usize,
    phase: f32,
    foliage: bool,
) {
    let offset = mesh.vertices.len() as u32;
    for ring in 0..3 {
        let (z, r) = if foliage {
            [(0.0, 0.72), (0.18, 1.0), (1.0, 0.025)][ring]
        } else {
            [(0.0, 1.2), (0.1, 0.95), (1.0, 0.04)][ring]
        };
        for side in 0..sides {
            let angle = side as f32 / sides as f32 * TAU + phase;
            let ragged = if foliage {
                1.0 + 0.12 * (angle * 3.0 + phase).sin()
            } else {
                1.0
            };
            let p = centre
                + Vec3::new(
                    angle.cos() * radius * r * ragged,
                    angle.sin() * radius * r * ragged,
                    height * z,
                );
            mesh.vertices.push(p / motu::ISLAND_WORLD_METRES);
        }
    }
    for ring in 0..2 {
        for side in 0..sides {
            let a = offset + (ring * sides + side) as u32;
            let b = offset + (ring * sides + (side + 1) % sides) as u32;
            let c = a + sides as u32;
            let d = b + sides as u32;
            mesh.triangles.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
}
/// Three overlapping lobes retain a spreading silhouette without carrying the
/// detailed Island crown's tessellation into every distant instance.
pub(super) fn broadleaf_crown(source: &Mesh) -> Mesh {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for &p in &source.vertices {
        min = min.min(p);
        max = max.max(p);
    }
    if source.vertices.is_empty() {
        return Mesh::default();
    }
    let centre = (min + max) * 0.5;
    let extent = (max - min) * 0.5;
    let mut mesh = Mesh::default();
    for (dx, dy, dz, scale) in [
        (-0.27, -0.12, -0.20, 0.84),
        (0.28, 0.12, -0.04, 0.74),
        (0.0, -0.04, 0.32, 0.75),
    ] {
        let start = mesh.vertices.len() as u32;
        for ring in 0..5 {
            let latitude = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * ring as f32 / 4.0;
            for side in 0..8 {
                let angle = TAU * side as f32 / 8.0;
                mesh.vertices.push(
                    centre
                        + extent * Vec3::new(dx, dy, dz)
                        + extent
                            * scale
                            * Vec3::new(
                                latitude.cos() * angle.cos(),
                                latitude.cos() * angle.sin(),
                                latitude.sin(),
                            ),
                );
            }
        }
        for ring in 0..4 {
            for side in 0..8 {
                let a = start + ring * 8 + side;
                let b = start + ring * 8 + (side + 1) % 8;
                mesh.triangles
                    .extend_from_slice(&[a, b, a + 8, b, b + 8, a + 8]);
            }
        }
    }
    mesh.calculate_normals();
    mesh
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conifers_have_tiered_tapered_crowns_and_bounded_geometry() {
        for variant in 0..4 {
            let tree = conifer(variant);
            assert!(tree.lod0_foliage.triangles.len() / 3 < 400);
            assert!(tree.lod1_foliage.triangles.len() / 3 < 150);
            assert!(tree.lod0_foliage.vertices.iter().all(|p| p.is_finite()));
            let bottom = tree
                .lod0_foliage
                .vertices
                .iter()
                .filter(|p| p.z * motu::ISLAND_WORLD_METRES < 5.0)
                .map(|p| p.truncate().length())
                .fold(0.0, f32::max);
            let top = tree
                .lod0_foliage
                .vertices
                .iter()
                .filter(|p| p.z * motu::ISLAND_WORLD_METRES > 12.0)
                .map(|p| p.truncate().length())
                .fold(0.0, f32::max);
            assert!(bottom > top);
        }
    }
}
