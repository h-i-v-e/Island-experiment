use std::f32::consts::{FRAC_PI_2, TAU};

use crate::{Mesh, Vec2, Vec3};

const LONGITUDE_SEGMENTS: usize = 64;
const LATITUDE_SEGMENTS: usize = 16;
const LONGITUDE_STEP: f32 = 1.0 / 64.0;
const LATITUDE_STEP: f32 = 1.0 / 16.0;

/// Radius of the generated sky hemisphere in normalized island coordinates.
pub const SKY_DOME_RADIUS: f32 = 1.0;

/// Depth of the cylindrical horizon skirt below normalized sea level.
///
/// The overlap hides the finite engine water plane when a camera above the
/// surface looks down through the geometric horizon.
pub const SKY_DOME_SKIRT_DEPTH: f32 = 0.25;

fn mesh_index(index: usize) -> u32 {
    u32::try_from(index).expect("sky dome vertex count must fit a 32-bit mesh index")
}

/// Builds an inward-facing hemisphere centred over the normalized island,
/// closed below sea level by a short cylindrical horizon skirt.
///
/// The island occupies `[0, 1]` on both horizontal axes, so a radius of one
/// becomes exactly one island width after an engine applies its world scale.
#[must_use]
pub fn generate_sky_dome() -> Mesh {
    let ring_width = LONGITUDE_SEGMENTS + 1;
    let ring_vertex_count = LATITUDE_SEGMENTS * ring_width;
    let skirt_vertex_count = ring_width;
    let vertex_count = ring_vertex_count + skirt_vertex_count + LONGITUDE_SEGMENTS;
    let triangle_count = LATITUDE_SEGMENTS * LONGITUDE_SEGMENTS * 2 + LONGITUDE_SEGMENTS;
    let centre = Vec3::new(0.5, 0.5, 0.0);
    let mut mesh = Mesh {
        vertices: Vec::with_capacity(vertex_count),
        normals: Vec::with_capacity(vertex_count),
        triangles: Vec::with_capacity(triangle_count * 3),
        uv: Vec::with_capacity(vertex_count),
    };

    // Keep both longitude seam vertices. They share a position and normal but
    // carry U values zero and one, preventing interpolation across the texture.
    let mut v = 0.0;
    for _latitude in 0..LATITUDE_SEGMENTS {
        let elevation = FRAC_PI_2 * v;
        let horizontal_radius = elevation.cos();
        let height = elevation.sin();
        let mut u = 0.0;
        for _longitude in 0..=LONGITUDE_SEGMENTS {
            let azimuth = TAU * u;
            let offset = Vec3::new(
                horizontal_radius * azimuth.cos(),
                horizontal_radius * azimuth.sin(),
                height,
            ) * SKY_DOME_RADIUS;
            mesh.vertices.push(centre + offset);
            mesh.normals.push(-offset.normalize());
            mesh.uv.push(Vec2::new(u, v));
            u += LONGITUDE_STEP;
        }
        v += LATITUDE_STEP;
    }

    let skirt_ring = mesh.vertices.len();
    let mut skirt_u = 0.0;
    for _longitude in 0..=LONGITUDE_SEGMENTS {
        let azimuth = TAU * skirt_u;
        let horizontal_offset = Vec3::new(azimuth.cos(), azimuth.sin(), 0.0) * SKY_DOME_RADIUS;
        mesh.vertices
            .push(centre + horizontal_offset - Vec3::Z * SKY_DOME_SKIRT_DEPTH);
        mesh.normals.push(-horizontal_offset.normalize());
        mesh.uv.push(Vec2::new(skirt_u, 0.0));
        skirt_u += LONGITUDE_STEP;
    }

    for longitude in 0..LONGITUDE_SEGMENTS {
        let lower_left = mesh_index(skirt_ring + longitude);
        let lower_right = lower_left + 1;
        let upper_left = mesh_index(longitude);
        let upper_right = upper_left + 1;
        mesh.triangles.extend([
            lower_left,
            upper_left,
            lower_right,
            lower_right,
            upper_left,
            upper_right,
        ]);
    }

    for latitude in 0..LATITUDE_SEGMENTS - 1 {
        let lower = latitude * ring_width;
        let upper = lower + ring_width;
        for longitude in 0..LONGITUDE_SEGMENTS {
            let lower_left = mesh_index(lower + longitude);
            let lower_right = lower_left + 1;
            let upper_left = mesh_index(upper + longitude);
            let upper_right = upper_left + 1;
            // Reverse the conventional sphere winding so the visible face is
            // inside the dome where the camera lives.
            mesh.triangles.extend([
                lower_left,
                upper_left,
                lower_right,
                lower_right,
                upper_left,
                upper_right,
            ]);
        }
    }

    let top_ring = (LATITUDE_SEGMENTS - 1) * ring_width;
    let mut pole_u = LONGITUDE_STEP * 0.5;
    for longitude in 0..LONGITUDE_SEGMENTS {
        let pole = mesh_index(mesh.vertices.len());
        mesh.vertices.push(centre + Vec3::Z * SKY_DOME_RADIUS);
        mesh.normals.push(-Vec3::Z);
        mesh.uv.push(Vec2::new(pole_u, 1.0));
        pole_u += LONGITUDE_STEP;
        let lower_left = mesh_index(top_ring + longitude);
        let lower_right = lower_left + 1;
        mesh.triangles.extend([lower_left, pole, lower_right]);
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dome_has_inward_hemisphere_horizon_skirt_and_a_split_uv_seam() {
        let mesh = generate_sky_dome();
        let centre = Vec3::new(0.5, 0.5, 0.0);

        assert_eq!(mesh.vertices.len(), mesh.normals.len());
        assert_eq!(mesh.vertices.len(), mesh.uv.len());
        assert_eq!(mesh.triangles.len() % 3, 0);
        assert!(mesh.vertices.iter().all(|vertex| vertex.is_finite()));
        assert!(mesh.normals.iter().all(|normal| normal.is_finite()));
        assert!(mesh.uv.iter().all(|uv| uv.is_finite()));
        for (vertex, normal) in mesh.vertices.iter().zip(&mesh.normals) {
            let offset = *vertex - centre;
            if offset.z < 0.0 {
                let horizontal_offset = offset.truncate().extend(0.0);
                assert!((horizontal_offset.length() - SKY_DOME_RADIUS).abs() < 1.0e-5);
                assert!(offset.z >= -SKY_DOME_SKIRT_DEPTH - 1.0e-5);
                assert!(normal.dot(horizontal_offset) < -0.999);
            } else {
                assert!((offset.length() - SKY_DOME_RADIUS).abs() < 1.0e-5);
                assert!(normal.dot(offset) < -0.999);
            }
        }

        for triangle in mesh.triangles.chunks_exact(3) {
            let a = mesh.vertices[triangle[0] as usize];
            let b = mesh.vertices[triangle[1] as usize];
            let c = mesh.vertices[triangle[2] as usize];
            let geometric_normal = (b - a).cross(c - a);
            let radial = (a + b + c) / 3.0 - centre;
            assert!(geometric_normal.dot(radial) < 0.0);
        }

        let seam_end = LONGITUDE_SEGMENTS;
        assert!(mesh.vertices[0].distance(mesh.vertices[seam_end]) < 1.0e-5);
        assert!(mesh.normals[0].distance(mesh.normals[seam_end]) < 1.0e-5);
        assert_eq!(mesh.uv[0], Vec2::new(0.0, 0.0));
        assert_eq!(mesh.uv[seam_end], Vec2::new(1.0, 0.0));
    }
}
