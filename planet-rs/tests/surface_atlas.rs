#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp
)]

use planet_rs::{
    DVec3,
    chunks::{ChunkMesh, ChunkVertex},
    surface_atlas::{SurfaceAtlas, SurfaceSampler},
};

fn patch(side: u32, surface: impl Fn(f64, f64) -> (f64, DVec3)) -> ChunkMesh {
    let vertices = (0..side)
        .flat_map(|y| {
            let surface = &surface;
            (0..side).map(move |x| {
                let x = f64::from(x) / f64::from(side - 1) * 0.04 - 0.02;
                let y = f64::from(y) / f64::from(side - 1) * 0.04 - 0.02;
                let (height, normal) = surface(x, y);
                ChunkVertex {
                    position: DVec3::new(x, y, 1.0 + height),
                    normal,
                    colour: [x as f32 + 0.4, y as f32 + 0.6, 0.2, 1.0],
                    protected: x.abs() > 0.019 || y.abs() > 0.019,
                }
            })
        })
        .collect();
    let triangles = (0..side - 1)
        .flat_map(|y| {
            (0..side - 1).flat_map(move |x| {
                let a = y * side + x;
                [[a, a + 1, a + side], [a + 1, a + side + 1, a + side]]
            })
        })
        .collect();
    ChunkMesh {
        vertices,
        triangles,
    }
}

fn at_face(atlas: &SurfaceAtlas, face: usize) -> &[u8] {
    let uv = atlas.mesh.triangles[face].map(|i| atlas.uv[i as usize]);
    let x = ((uv.iter().map(|p| p[0]).sum::<f32>() / 3.0) * atlas.width as f32).floor() as usize;
    let y = ((uv.iter().map(|p| p[1]).sum::<f32>() / 3.0) * atlas.height as f32).floor() as usize;
    let offset = (y * atlas.width as usize + x) * 4;
    &atlas.pixels[offset..offset + 4]
}

#[test]
fn charts_preserve_exact_geometry_attributes_and_are_deterministic() {
    let source = patch(5, |x, _| {
        (
            x.abs() * 4.0,
            DVec3::new(-x.signum() * 4.0, 0.0, 1.0).normalize(),
        )
    });
    let sampler = SurfaceSampler::new(&source).unwrap();
    let atlas = sampler.bake(&source, 64, 1000.0).unwrap();
    assert_eq!(atlas, sampler.bake(&source, 64, 1000.0).unwrap());
    assert_eq!(atlas.mesh.triangles.len(), source.triangles.len());
    for (source_face, atlas_face) in source.triangles.iter().zip(&atlas.mesh.triangles) {
        for (&original, &unwrapped) in source_face.iter().zip(atlas_face) {
            assert_eq!(
                source.vertices[original as usize],
                atlas.mesh.vertices[unwrapped as usize]
            );
        }
    }
    assert!(
        atlas
            .uv
            .iter()
            .flatten()
            .all(|&v| v.is_finite() && (0.0..=1.0).contains(&v))
    );
    assert_eq!(
        atlas.pixels.len(),
        atlas.width as usize * atlas.height as usize * 4
    );
    assert!(
        atlas
            .pixels
            .chunks_exact(4)
            .all(|pixel| pixel[..3] == [128; 3]),
        "unchanged steep banks must keep exact zero residuals through interiors and gutters"
    );
}

#[test]
fn a_flat_coarse_mesh_receives_fine_normals_including_its_gutters() {
    let fine = patch(33, |x, _| {
        let frequency = 800.0;
        (
            (x * frequency).sin() * 0.0007,
            DVec3::new(-0.0007 * frequency * (x * frequency).cos(), 0.0, 1.0).normalize(),
        )
    });
    let coarse = patch(2, |_, _| (0.0, DVec3::Z));
    let atlas = SurfaceSampler::new(&fine)
        .unwrap()
        .bake(&coarse, 128, 1000.0)
        .unwrap();
    let normals: Vec<_> = atlas
        .pixels
        .chunks_exact(4)
        .filter(|p| p[..3] != [128; 3])
        .collect();
    assert!(
        normals.len() > 128,
        "fine detail must cover interiors and filtering gutters"
    );
    let minimum = normals.iter().map(|p| p[0]).min().unwrap();
    let maximum = normals.iter().map(|p| p[0]).max().unwrap();
    assert!(
        maximum - minimum > 35,
        "coarse vertex normals must not replace finest terrain normals"
    );
}

#[test]
fn physical_ao_darkens_concavity_but_leaves_flat_ground_clear() {
    let flat = patch(25, |_, _| (0.0, DVec3::Z));
    let valley = patch(25, |x, y| {
        let curvature = 40.0;
        (
            curvature * (x * x + y * y),
            DVec3::new(-2.0 * curvature * x, -2.0 * curvature * y, 1.0).normalize(),
        )
    });
    let coarse = patch(2, |_, _| (0.0, DVec3::Z));
    let clear = SurfaceSampler::new(&flat)
        .unwrap()
        .bake(&coarse, 64, 1000.0)
        .unwrap();
    let dark = SurfaceSampler::new(&valley)
        .unwrap()
        .bake(&coarse, 64, 1000.0)
        .unwrap();
    assert!(clear.pixels.chunks_exact(4).all(|p| p[3] >= 254));
    assert!(dark.pixels.chunks_exact(4).any(|p| p[3] < 240));
    // Larger physical planets reduce the physical extent of each probe. The
    // atlas UV layout is unchanged, but this metre-scale valley contributes less.
    let large = SurfaceSampler::new(&valley)
        .unwrap()
        .bake(&coarse, 64, 10_000.0)
        .unwrap();
    let mean = |atlas: &SurfaceAtlas| {
        atlas
            .pixels
            .chunks_exact(4)
            .map(|p| u64::from(p[3]))
            .sum::<u64>()
    };
    assert!(mean(&large) > mean(&dark));
}

#[test]
fn sphere_curvature_and_poles_do_not_create_an_ao_seam() {
    for rotation in [
        glam::DMat3::IDENTITY,
        glam::DMat3::from_rotation_y(std::f64::consts::FRAC_PI_2),
        glam::DMat3::from_rotation_x(std::f64::consts::PI),
    ] {
        let mut fine = patch(33, |x, y| {
            (
                (1.0 - x * x - y * y).sqrt() - 1.0,
                DVec3::new(x, y, (1.0 - x * x - y * y).sqrt()),
            )
        });
        for v in &mut fine.vertices {
            v.position = rotation * v.position;
            v.normal = rotation * v.normal;
        }
        let atlas = SurfaceSampler::new(&fine)
            .unwrap()
            .bake(&fine, 128, 1000.0)
            .unwrap();
        assert!(atlas.pixels.chunks_exact(4).all(|p| p[3] >= 254));
    }
}

#[test]
fn invalid_inputs_fail_explicitly_and_empty_targets_are_small() {
    let source = patch(2, |_, _| (0.0, DVec3::Z));
    let sampler = SurfaceSampler::new(&source).unwrap();
    assert!(sampler.bake(&source, 16, 1000.0).is_err());
    assert!(sampler.bake(&source, 64, 0.0).is_err());
    assert!(SurfaceSampler::new(&ChunkMesh::default()).is_err());
    let empty = sampler.bake(&ChunkMesh::default(), 64, 1000.0).unwrap();
    assert_eq!(empty.pixels.len(), 4);
}

#[test]
fn subpixel_sliver_charts_keep_their_actual_surface_values() {
    let normal = DVec3::new(-2.0, 0.0, 1.0).normalize();
    let fine = patch(5, |x, _| (x * 2.0, normal));
    let target = ChunkMesh {
        vertices: [
            DVec3::new(-0.015, 0.0, 0.97),
            DVec3::new(0.015, 0.0, 1.03),
            DVec3::new(0.015, 1.0e-8, 1.03),
        ]
        .into_iter()
        .map(|position| ChunkVertex {
            position,
            normal,
            colour: [0.3; 4],
            protected: true,
        })
        .collect(),
        triangles: vec![[0, 1, 2]],
    };
    let atlas = SurfaceSampler::new(&fine)
        .unwrap()
        .bake(&target, 64, 1000.0)
        .unwrap();
    assert!(
        atlas
            .pixels
            .chunks_exact(4)
            .all(|pixel| pixel[..3] == [128; 3]),
        "sliver charts must preserve exact identity residuals instead of smearing fine bank normals"
    );
    assert_eq!(atlas.mesh.triangles.len(), 1);
    for (original, unwrapped) in target.triangles[0].into_iter().zip(atlas.mesh.triangles[0]) {
        assert_eq!(
            target.vertices[original as usize],
            atlas.mesh.vertices[unwrapped as usize]
        );
    }
}

#[test]
fn independently_cut_neighbours_bake_from_the_same_global_source() {
    let fine = patch(33, |x, y| {
        (
            (x * 600.0).sin() * 0.0005 + (y * 250.0).cos() * 0.0005,
            DVec3::new(-0.3 * (x * 600.0).cos(), 0.125 * (y * 250.0).sin(), 1.0).normalize(),
        )
    });
    let mut left = patch(2, |_, _| (0.0, DVec3::Z));
    let mut right = left.clone();
    for vertex in &mut left.vertices {
        vertex.position.x = (vertex.position.x - 0.02) * 0.5;
    }
    for vertex in &mut right.vertices {
        vertex.position.x = (vertex.position.x + 0.02) * 0.5;
    }
    let sampler = SurfaceSampler::new(&fine).unwrap();
    let a = sampler.bake(&left, 128, 1000.0).unwrap();
    let b = sampler.bake(&right, 128, 1000.0).unwrap();
    // Select edge midpoints using each chart's independently packed UVs. The
    // underlying source and AO neighbours extend through that chunk boundary.
    let boundary_pixel = |atlas: &SurfaceAtlas| {
        let edge: Vec<_> = atlas
            .mesh
            .vertices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.position.x.abs() < 1.0e-12)
            .map(|(i, _)| atlas.uv[i])
            .collect();
        let x = ((edge[0][0] + edge[1][0]) * 0.5 * atlas.width as f32).floor() as usize;
        let y = ((edge[0][1] + edge[1][1]) * 0.5 * atlas.height as f32).floor() as usize;
        atlas.pixels[(y * atlas.width as usize + x) * 4..][..4].to_vec()
    };
    let a = boundary_pixel(&a);
    let b = boundary_pixel(&b);
    for (left, right) in a.into_iter().zip(b) {
        assert!((i16::from(left) - i16::from(right)).abs() < 8);
    }
}

#[test]
fn residuals_reconstruct_detail_removed_from_simplified_geometry() {
    let fine_normal = DVec3::new(-0.6, 0.3, 1.0).normalize();
    let fine = patch(17, |x, y| (0.6 * x - 0.3 * y, fine_normal));
    let coarse = patch(2, |_, _| (0.0, DVec3::Z));
    let atlas = SurfaceSampler::new(&fine)
        .unwrap()
        .bake(&coarse, 64, 1000.0)
        .unwrap();
    for face in 0..atlas.mesh.triangles.len() {
        let rgb = at_face(&atlas, face);
        let delta = DVec3::new(
            f64::from(rgb[0]) - 128.0,
            f64::from(rgb[1]) - 128.0,
            f64::from(rgb[2]) - 128.0,
        ) / 63.5;
        let reconstructed = (DVec3::Z + delta).normalize();
        assert!(
            reconstructed.dot(fine_normal) > 0.9999,
            "additive object-space residual must restore the fine slope normal"
        );
    }
}

#[test]
fn river_coverage_is_projected_onto_coarse_charts_without_adding_geometry() {
    let fine = patch(9, |_, _| (0.0, DVec3::Z));
    let coarse = patch(2, |_, _| (0.0, DVec3::Z));
    let mut river = patch(2, |_, _| (0.002, DVec3::Z));
    for vertex in &mut river.vertices {
        vertex.position.x = vertex.position.x * 0.1 + vertex.position.y * 0.23;
    }
    let fine = SurfaceSampler::new(&fine).unwrap();
    let water = SurfaceSampler::new(&river).unwrap();
    let dry = fine.bake(&coarse, 128, 1000.0).unwrap();
    let wet = fine
        .bake_with_rivers(&coarse, 128, 1000.0, Some(&water))
        .unwrap();
    assert_eq!(wet.mesh, dry.mesh);
    assert_eq!(wet.uv, dry.uv);
    assert_eq!(wet.pixels, dry.pixels);
    assert_eq!(wet.rivers.len(), (wet.width * wet.height) as usize);
    assert!(dry.rivers.iter().all(|&value| value == 0));
    assert!(wet.rivers.contains(&255));
    assert!(wet.rivers.contains(&0));
    assert!(
        wet.rivers.iter().any(|&value| value > 0 && value < 255),
        "narrow river edges are antialiased"
    );
}
