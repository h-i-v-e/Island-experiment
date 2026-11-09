//! Smooth metal door furniture, independent of the textured structural leaf.
use crate::BuildingMesh;

const SEGMENTS: u16 = 16;
const STACKS: u16 = 8;

#[derive(Clone, Copy)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

pub(super) fn door_knobs(width: f32, height: f32, thickness: f32) -> BuildingMesh {
    let mut mesh = BuildingMesh::default();
    let latch = width - 0.075;
    let elevation = 1.02_f32.min(height * 0.55);
    for face_sign in [-1., 1.] {
        let face = thickness * 0.5;
        cylinder(
            &mut mesh,
            [latch, elevation],
            face_sign,
            [face + 0.0005, face + 0.0055],
            0.034,
        );
        cylinder(
            &mut mesh,
            [latch, elevation],
            face_sign,
            [face + 0.0055, face + 0.019],
            0.010,
        );
        sphere(
            &mut mesh,
            [latch, face_sign * (face + 0.034), elevation],
            face_sign,
            0.025,
        );
    }
    mesh
}

fn cylinder(
    mesh: &mut BuildingMesh,
    centre: [f32; 2],
    face_sign: f32,
    ends: [f32; 2],
    radius: f32,
) {
    for segment in 0..SEGMENTS {
        let angles = [angle(segment), angle(segment + 1)];
        let vertices = std::array::from_fn::<_, 4, _>(|index| {
            let at_end = usize::from(index >= 2);
            let around = usize::from(index == 1 || index == 2);
            let (sine, cosine) = angles[around].sin_cos();
            Vertex {
                position: [
                    centre[0] + radius * cosine,
                    face_sign * ends[at_end],
                    centre[1] + radius * sine,
                ],
                normal: [cosine, 0., sine],
                uv: [
                    f32::from(segment + u16::from(around == 1)) / f32::from(SEGMENTS),
                    if at_end == 1 { 1. } else { 0. },
                ],
            }
        });
        triangle(mesh, [vertices[0], vertices[1], vertices[2]]);
        triangle(mesh, [vertices[0], vertices[2], vertices[3]]);
        for (end, sign) in [(0, -1.), (1, 1.)] {
            let centre_vertex = Vertex {
                position: [centre[0], face_sign * ends[end], centre[1]],
                normal: [0., face_sign * sign, 0.],
                uv: [0.5, 0.5],
            };
            let cap = angles.map(|theta| {
                let (sine, cosine) = theta.sin_cos();
                Vertex {
                    position: [
                        centre[0] + radius * cosine,
                        face_sign * ends[end],
                        centre[1] + radius * sine,
                    ],
                    normal: centre_vertex.normal,
                    uv: [cosine * 0.5 + 0.5, sine * 0.5 + 0.5],
                }
            });
            triangle(mesh, [centre_vertex, cap[0], cap[1]]);
        }
    }
}
fn sphere(mesh: &mut BuildingMesh, centre: [f32; 3], face_sign: f32, radius: f32) {
    for latitude in 0..STACKS {
        for segment in 0..SEGMENTS {
            let coordinates = [
                (latitude, segment),
                (latitude, segment + 1),
                (latitude + 1, segment + 1),
                (latitude + 1, segment),
            ];
            let vertices = coordinates.map(|(ring, around)| {
                let theta = std::f32::consts::PI * f32::from(ring) / f32::from(STACKS);
                let (phi_sine, phi_cosine) = angle(around).sin_cos();
                let (theta_sine, theta_cosine) = theta.sin_cos();
                let normal = [
                    theta_sine * phi_cosine,
                    face_sign * theta_cosine,
                    theta_sine * phi_sine,
                ];
                Vertex {
                    position: std::array::from_fn(|axis| centre[axis] + normal[axis] * radius),
                    normal,
                    uv: [
                        f32::from(around) / f32::from(SEGMENTS),
                        f32::from(ring) / f32::from(STACKS),
                    ],
                }
            });
            if latitude > 0 {
                triangle(mesh, [vertices[0], vertices[1], vertices[2]]);
            }
            if latitude + 1 < STACKS {
                triangle(mesh, [vertices[0], vertices[2], vertices[3]]);
            }
        }
    }
}
fn angle(segment: u16) -> f32 {
    std::f32::consts::TAU * f32::from(segment) / f32::from(SEGMENTS)
}

fn triangle(mesh: &mut BuildingMesh, mut vertices: [Vertex; 3]) {
    let surface_normal = super::normal(
        vertices[0].position,
        vertices[1].position,
        vertices[2].position,
    );
    let smooth_normal =
        std::array::from_fn::<_, 3, _>(|axis| vertices.iter().map(|v| v.normal[axis]).sum::<f32>());
    if surface_normal
        .iter()
        .zip(smooth_normal)
        .map(|(a, b)| a * b)
        .sum::<f32>()
        < 0.
    {
        vertices.swap(1, 2);
    }
    let start =
        u32::try_from(mesh.positions.len()).expect("bounded door furniture fits u32 indices");
    for vertex in vertices {
        mesh.positions.push(vertex.position);
        mesh.normals.push(vertex.normal);
        mesh.uvs.push(vertex.uv);
    }
    mesh.indices.extend([start, start + 1, start + 2]);
}
