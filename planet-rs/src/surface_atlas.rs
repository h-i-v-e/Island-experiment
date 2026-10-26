//! Geometry-chart atlases carrying planet-space normal corrections and local AO.
//!
//! Source coordinates use the viewer's unit-radius planet. Physical AO distances
//! are converted using the supplied radius in metres. Geometry remains exact;
//! only vertices shared by different UV charts are duplicated.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::sync::OnceLock;

use glam::DVec2;

use crate::{DVec3, chunks::ChunkMesh};

const PADDING: u32 = 8;
const SLIVER_CELL: u32 = 24;
const MAX_DIMENSION: u32 = 8192;
const EMPTY: u32 = u32::MAX;
const OFFSETS: [(i32, i32); 15] = [
    (-8, -2),
    (-4, 2),
    (-2, 1),
    (-1, 1),
    (-1, -1),
    (-1, -4),
    (1, 2),
    (1, 1),
    (1, -1),
    (1, -2),
    (2, 4),
    (2, -1),
    (2, -8),
    (4, -2),
    (8, 2),
];

/// An independently unwrapped LOD. RGB encodes the planet-space correction
/// from interpolated target normal to fine source normal: `round(delta*63.5+128)`.
/// Decode `(byte-128)/63.5`, add to the normalized interpolated vertex normal,
/// then normalize again. Exact zero is RGB128, preserving detailed bank normals
/// already retained by protected geometry. Alpha is ambient visibility.
/// Upload linearly, clamp sampling, and use at most mip levels 0 through 2 to
/// stay inside the eight-texel gutters. Average residuals directly for mips.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceAtlas {
    pub mesh: ChunkMesh,
    pub uv: Vec<[f32; 2]>,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Antialiased river-water coverage, using the same chart UVs.
    pub rivers: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min: DVec3,
    max: DVec3,
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min: DVec3::splat(f64::INFINITY),
            max: DVec3::splat(f64::NEG_INFINITY),
        }
    }
    fn include(&mut self, point: DVec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }
    fn intersects(self, direction: DVec3) -> bool {
        let mut near: f64 = 0.0;
        let mut far = f64::INFINITY;
        for axis in 0..3 {
            if direction[axis].abs() < 1.0e-30 {
                if self.min[axis] > 0.0 || self.max[axis] < 0.0 {
                    return false;
                }
            } else {
                let a = self.min[axis] / direction[axis];
                let b = self.max[axis] / direction[axis];
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if far < near {
                    return false;
                }
            }
        }
        true
    }
}

#[derive(Debug)]
struct Node {
    bounds: Bounds,
    range: std::ops::Range<usize>,
    children: Option<[usize; 2]>,
}

/// Borrows the final finest terrain and owns only an acceleration index. One
/// sampler is reused for all chunks, so normal/AO probes cross chunk borders.
#[derive(Debug)]
pub struct SurfaceSampler<'a> {
    source: &'a ChunkMesh,
    faces: Vec<usize>,
    nodes: Vec<Node>,
    vertex_occlusion: Vec<OnceLock<(u64, u8)>>,
}

#[derive(Clone, Copy)]
struct Sample {
    radius: f64,
    normal: DVec3,
    face: usize,
    weights: [f64; 3],
}

impl<'a> SurfaceSampler<'a> {
    /// # Errors
    /// Rejects empty, invalid, non-finite terrain and vertices at the origin.
    pub fn new(source: &'a ChunkMesh) -> Result<Self, String> {
        validate_mesh(source)?;
        let mut result = Self {
            source,
            faces: (0..source.triangles.len()).collect(),
            nodes: Vec::new(),
            vertex_occlusion: (0..source.vertices.len())
                .map(|_| OnceLock::new())
                .collect(),
        };
        result.build(0..result.faces.len());
        Ok(result)
    }

    /// Samples the outermost displayed surface along a radial ray. The colour
    /// channels retain their source meaning (hardness, cover, river, coast).
    #[must_use]
    pub fn radial_surface(&self, direction: DVec3) -> Option<(f64, [f32; 4])> {
        let sample = self.sample(direction.try_normalize()?)?;
        let vertices = self.source.triangles[sample.face];
        let colour = std::array::from_fn(|channel| {
            let value = vertices
                .iter()
                .zip(sample.weights)
                .map(|(&i, weight)| {
                    f64::from(self.source.vertices[i as usize].colour[channel]) * weight
                })
                .sum::<f64>();
            #[allow(clippy::cast_possible_truncation)]
            {
                value as f32
            }
        });
        Some((sample.radius, colour))
    }

    fn build(&mut self, range: std::ops::Range<usize>) -> usize {
        let mut bounds = Bounds::empty();
        let mut centroids = Bounds::empty();
        for &face in &self.faces[range.clone()] {
            let points =
                self.source.triangles[face].map(|i| self.source.vertices[i as usize].position);
            for point in points {
                bounds.include(point);
            }
            centroids.include((points[0] + points[1] + points[2]) / 3.0);
        }
        // Outward roundoff padding keeps rays through shared edges supported.
        let epsilon = bounds
            .max
            .abs()
            .max(bounds.min.abs())
            .max_element()
            .max(1.0)
            * 1.0e-12;
        bounds.min -= DVec3::splat(epsilon);
        bounds.max += DVec3::splat(epsilon);
        let node = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            range: range.clone(),
            children: None,
        });
        if range.len() > 12 {
            let size = centroids.max - centroids.min;
            let axis = if size.x >= size.y && size.x >= size.z {
                0
            } else if size.y >= size.z {
                1
            } else {
                2
            };
            let source = self.source;
            self.faces[range.clone()].sort_unstable_by(|&a, &b| {
                let centre = |face: usize| {
                    source.triangles[face]
                        .iter()
                        .map(|&i| source.vertices[i as usize].position[axis])
                        .sum::<f64>()
                };
                centre(a).total_cmp(&centre(b)).then(a.cmp(&b))
            });
            let middle = range.start + range.len() / 2;
            let left = self.build(range.start..middle);
            let right = self.build(middle..range.end);
            self.nodes[node].children = Some([left, right]);
        }
        node
    }

    #[allow(clippy::many_single_char_names)] // Standard ray/triangle barycentric notation.
    fn sample(&self, direction: DVec3) -> Option<Sample> {
        let mut stack = [0_usize; 128];
        let mut length = 1;
        let mut result: Option<Sample> = None;
        while length > 0 {
            length -= 1;
            let node = &self.nodes[stack[length]];
            if !node.bounds.intersects(direction) {
                continue;
            }
            if let Some([left, right]) = node.children {
                stack[length] = left;
                stack[length + 1] = right;
                length += 2;
                continue;
            }
            for &face in &self.faces[node.range.clone()] {
                let vertices =
                    self.source.triangles[face].map(|i| self.source.vertices[i as usize]);
                let [a, b, c] = vertices.map(|v| v.position);
                let ab = b - a;
                let ac = c - a;
                let cross = direction.cross(ac);
                let determinant = ab.dot(cross);
                if determinant == 0.0 {
                    continue;
                }
                let u = -a.dot(cross) / determinant;
                let q = (-a).cross(ab);
                let v = direction.dot(q) / determinant;
                if u < -1.0e-9 || v < -1.0e-9 || u + v > 1.0 + 1.0e-9 {
                    continue;
                }
                let radius = ac.dot(q) / determinant;
                if radius <= 0.0 || result.is_some_and(|old| old.radius >= radius) {
                    continue;
                }
                let normal = (vertices[0].normal * (1.0 - u - v)
                    + vertices[1].normal * u
                    + vertices[2].normal * v)
                    .normalize_or_zero();
                result = Some(Sample {
                    radius,
                    face,
                    weights: [1.0 - u - v, u, v],
                    normal: if normal == DVec3::ZERO {
                        ab.cross(ac).normalize()
                    } else {
                        normal
                    },
                });
            }
        }
        result
    }

    fn occlusion(&self, up: DVec3, radius: f64, normal: DVec3, world_radius: f64) -> u8 {
        let reference = if up.z.abs() < 0.8 { DVec3::Z } else { DVec3::X };
        let first = up.cross(reference).normalize();
        let second = up.cross(first);
        let mut total = 0.0;
        let mut count = 0;
        for (x, y) in OFFSETS {
            // Match Island's metre-scale offsets, independently of texel size.
            let offset = (first * f64::from(x) + second * f64::from(y)) / world_radius;
            let direction = (up + offset / radius).normalize();
            let Some(neighbour) = self.sample(direction) else {
                continue;
            };
            // Remove spherical curvature: a smooth constant-radius planet has
            // the same AO as flat ground, including at either pole.
            let delta = offset + up * (neighbour.radius - radius);
            total += delta.normalize_or_zero().dot(normal).max(0.0);
            count += 1;
        }
        if count == 0 {
            255
        } else {
            ((1.0 - total / f64::from(count)) * 255.0).clamp(0.0, 255.0) as u8
        }
    }

    fn interpolated_occlusion(&self, sample: Sample, world_radius: f64) -> u8 {
        let vertices = self.source.triangles[sample.face];
        let mut total = 0.0;
        for (index, weight) in vertices.into_iter().zip(sample.weights) {
            let compute = || {
                let vertex = self.source.vertices[index as usize];
                let radius = vertex.position.length();
                self.occlusion(
                    vertex.position / radius,
                    radius,
                    vertex.normal.normalize_or_zero(),
                    world_radius,
                )
            };
            // All chunks/LODs normally share one radius. Retain the key so an
            // independent bake with another radius remains correct as well.
            let &(key, value) = self.vertex_occlusion[index as usize]
                .get_or_init(|| (world_radius.to_bits(), compute()));
            let value = if key == world_radius.to_bits() {
                value
            } else {
                compute()
            };
            total += f64::from(value) * weight;
        }
        total.clamp(0.0, 255.0).round() as u8
    }

    /// Unwraps the exact target geometry and samples the global finest source.
    /// `world_radius` is the unit planet's physical radius in metres.
    /// # Errors
    /// Returns errors for invalid geometry, texture sizes, radius or unwrap.
    pub fn bake(
        &self,
        target: &ChunkMesh,
        dimension: u32,
        world_radius: f64,
    ) -> Result<SurfaceAtlas, String> {
        self.bake_with_rivers(target, dimension, world_radius, None)
    }

    /// Bakes fine surface detail plus projected river coverage from a global
    /// water sampler. Four probes per texel retain narrow rivers when minified.
    /// # Errors
    /// Returns the same geometry, dimension and unwrap errors as [`Self::bake`].
    pub fn bake_with_rivers(
        &self,
        target: &ChunkMesh,
        dimension: u32,
        world_radius: f64,
        rivers: Option<&SurfaceSampler<'_>>,
    ) -> Result<SurfaceAtlas, String> {
        if !(32..=MAX_DIMENSION).contains(&dimension) {
            return Err("surface atlas dimension must be between 32 and 8192".into());
        }
        if !world_radius.is_finite() || world_radius <= 0.0 {
            return Err("surface atlas world radius must be positive and finite".into());
        }
        if target.triangles.is_empty() && target.vertices.is_empty() {
            return Ok(SurfaceAtlas {
                mesh: ChunkMesh::default(),
                uv: Vec::new(),
                width: 1,
                height: 1,
                pixels: vec![128, 128, 128, 255],
                rivers: vec![0],
            });
        }
        validate_mesh(target)?;
        let Unwrapped {
            mesh,
            uv,
            width,
            height,
        } = unwrap(target, dimension)?;
        let triangles: Vec<_> = mesh
            .triangles
            .iter()
            .map(|indices| {
                AtlasTriangle::new(
                    indices
                        .map(|i| DVec2::from_array(uv[i as usize].map(f64::from)))
                        .map(|p| {
                            p * DVec2::new(f64::from(width), f64::from(height)) - DVec2::splat(0.5)
                        }),
                    indices.map(|i| mesh.vertices[i as usize].position),
                    indices.map(|i| mesh.vertices[i as usize].normal),
                )
            })
            .collect();
        let triangles: Vec<_> = triangles
            .into_iter()
            .collect::<Option<_>>()
            .ok_or("surface unwrap produced a zero-area UV triangle")?;
        let owners = chart_owners(&triangles, width, height);
        let mut pixels = [128, 128, 128, 255].repeat(owners.len());
        let mut coverage = vec![0_u8; owners.len()];
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .min(8);
        let per_thread = owners.len().div_ceil(threads);
        std::thread::scope(|scope| {
            for (part, (pixels, coverage)) in pixels
                .chunks_mut(per_thread * 4)
                .zip(coverage.chunks_mut(per_thread))
                .enumerate()
            {
                let owners = &owners;
                let triangles = &triangles;
                scope.spawn(move || {
                    for (offset, pixel) in pixels.chunks_exact_mut(4).enumerate() {
                        let index = part * per_thread + offset;
                        let owner = owners[index];
                        if owner == EMPTY {
                            continue;
                        }
                        let triangle = &triangles[owner as usize];
                        let pixel_point = DVec2::new(
                            (index % width as usize) as f64,
                            (index / width as usize) as f64,
                        );
                        let (point, target_normal) = triangle.surface_point(pixel_point);
                        if let Some(rivers) = rivers {
                            coverage[offset] = river_coverage(triangle, pixel_point, rivers);
                        }
                        let up = point.normalize_or_zero();
                        let Some(sample) = self.sample(up) else {
                            continue;
                        };
                        for (byte, component) in pixel[..3]
                            .iter_mut()
                            .zip((sample.normal - target_normal).to_array())
                        {
                            *byte = (component * 63.5 + 128.0).clamp(0.0, 255.0).round() as u8;
                        }
                        pixel[3] = self.interpolated_occlusion(sample, world_radius);
                    }
                });
            }
        });
        Ok(SurfaceAtlas {
            mesh,
            uv,
            width,
            height,
            pixels,
            rivers: coverage,
        })
    }
}

fn chart_owners(triangles: &[AtlasTriangle], width: u32, height: u32) -> Vec<u32> {
    let mut owners = vec![EMPTY; width as usize * height as usize];
    for conservative in [false, true] {
        for (owner, triangle) in triangles.iter().enumerate() {
            triangle.rasterize(width as usize, height as usize, conservative, |pixel| {
                if !conservative || owners[pixel] == EMPTY {
                    owners[pixel] = owner as u32;
                }
            });
        }
    }
    dilate(&mut owners, width as usize, height as usize);
    owners
}

fn river_coverage(triangle: &AtlasTriangle, pixel: DVec2, rivers: &SurfaceSampler<'_>) -> u8 {
    let mut covered = 0_u16;
    for x in [-0.25, 0.25] {
        for y in [-0.25, 0.25] {
            let (point, _) = triangle.surface_point(pixel + DVec2::new(x, y));
            covered += u16::from(rivers.sample(point.normalize_or_zero()).is_some());
        }
    }
    ((covered * 255 + 2) / 4) as u8
}

fn validate_mesh(mesh: &ChunkMesh) -> Result<(), String> {
    if mesh.vertices.is_empty() || mesh.triangles.is_empty() {
        return Err("surface atlas requires nonempty geometry".into());
    }
    if mesh.vertices.iter().any(|v| {
        !v.position.is_finite() || v.position.length_squared() <= 0.0 || !v.normal.is_finite()
    }) {
        return Err("surface atlas vertices must be finite and outside the origin".into());
    }
    for face in &mesh.triangles {
        if face.iter().any(|&i| i as usize >= mesh.vertices.len()) {
            return Err("surface atlas triangle index is out of range".into());
        }
        let [a, b, c] = face.map(|i| mesh.vertices[i as usize].position);
        if (b - a).cross(c - a).length_squared() == 0.0 {
            return Err("surface atlas triangle is degenerate".into());
        }
    }
    Ok(())
}

fn page_size(width: u32, height: u32, pages: u32) -> Result<[u32; 2], String> {
    if width == 0 || height == 0 || pages == 0 || height > MAX_DIMENSION {
        return Err("surface unwrap returned invalid pages".into());
    }
    (1..=pages.min(MAX_DIMENSION / width))
        .map(|columns| (columns, pages.div_ceil(columns)))
        .filter(|&(_, rows)| rows <= MAX_DIMENSION / height)
        .map(|(columns, rows)| [columns * width, rows * height])
        .min_by_key(|&[w, h]| (u64::from(w) * u64::from(h), w.max(h)))
        .ok_or_else(|| {
            format!(
                "surface unwrap needs {pages} pages of {width}x{height}, exceeding texture limits"
            )
        })
}

struct Unwrapped {
    mesh: ChunkMesh,
    uv: Vec<[f32; 2]>,
    width: u32,
    height: u32,
}

fn generate_uvs(source: &ChunkMesh, dimension: u32) -> Result<uvunwrap::Atlas, String> {
    let positions: Vec<_> = source
        .vertices
        .iter()
        .map(|v| v.position.as_vec3().to_array())
        .collect();
    let indices: Vec<_> = source.triangles.iter().flatten().copied().collect();
    let options = uvunwrap::Options {
        density: uvunwrap::Density::Auto {
            page_size: dimension,
        },
        padding: PADDING,
        max_chart_angle: 65.0,
        max_area_distortion: 2.0,
        packing: uvunwrap::Packing::Fast,
        ..Default::default()
    };
    uvunwrap::generate(&[uvunwrap::MeshInput::new(&positions, &indices)], &options)
        .map_err(|error| format!("surface UV unwrap failed: {error}"))
}

fn unwrap(source: &ChunkMesh, dimension: u32) -> Result<Unwrapped, String> {
    let atlas = generate_uvs(source, dimension)?;
    let output = atlas
        .meshes
        .into_iter()
        .next()
        .ok_or("surface unwrap returned no mesh")?;
    if output.indices.len() != source.triangles.len() * 3 {
        return Err("surface unwrap changed the source face count".into());
    }
    let fallback: Vec<_> = output
        .indices
        .chunks_exact(3)
        .map(|face| {
            let vertices = [face[0], face[1], face[2]].map(|i| &output.vertices[i as usize]);
            let points = vertices.map(|v| DVec2::from_array(v.uv.map(f64::from)));
            vertices.iter().any(|v| v.page.is_none())
                || (points[1] - points[0]).perp_dot(points[2] - points[0]) == 0.0
        })
        .collect();
    let count = fallback.iter().filter(|&&value| value).count() as u32;
    // uvunwrap deliberately excludes triangles below its f32 UV altitude
    // threshold. Give only those faces independent, enlarged triangle charts;
    // their geometry stays exact and rasterization still maps barycentrically
    // to the real surface. Existing charts retain their original texel scale.
    let page_width = if count > 0 {
        dimension.max(atlas.width)
    } else {
        atlas.width
    };
    let page_height = if count > 0 {
        dimension.max(atlas.height)
    } else {
        atlas.height
    };
    let per_row = page_width / SLIVER_CELL;
    let per_page = per_row * (page_height / SLIVER_CELL);
    let extra_pages = if count > 0 {
        count.div_ceil(per_page)
    } else {
        0
    };
    let [width, height] = page_size(page_width, page_height, atlas.page_count + extra_pages)?;
    let columns = width / page_width;
    let coordinate = |point: [f32; 2], page: u32| {
        [
            (point[0] + ((page % columns) * page_width) as f32) / width as f32,
            (point[1] + ((page / columns) * page_height) as f32) / height as f32,
        ]
    };
    let mut mesh = ChunkMesh {
        vertices: Vec::with_capacity(output.vertices.len() + count as usize * 3),
        triangles: Vec::with_capacity(source.triangles.len()),
    };
    let mut uv = Vec::with_capacity(mesh.vertices.capacity());
    for vertex in &output.vertices {
        mesh.vertices.push(source.vertices[vertex.xref as usize]);
        uv.push(vertex.page.map_or([0.0; 2], |page| {
            coordinate(
                [
                    vertex.uv[0] * atlas.width as f32,
                    vertex.uv[1] * atlas.height as f32,
                ],
                page,
            )
        }));
    }
    let mut sliver = 0;
    for (face, (indices, &fallback)) in output.indices.chunks_exact(3).zip(&fallback).enumerate() {
        if !fallback {
            mesh.triangles.push([indices[0], indices[1], indices[2]]);
            continue;
        }
        let page = atlas.page_count + sliver / per_page;
        let cell = sliver % per_page;
        let x = (cell % per_row) * SLIVER_CELL + PADDING;
        let y = (cell / per_row) * SLIVER_CELL + PADDING;
        let triangle_uv = [
            [x as f32, y as f32],
            [(x + 7) as f32, y as f32],
            [x as f32, (y + 7) as f32],
        ];
        let base = mesh.vertices.len() as u32;
        for (&index, point) in source.triangles[face].iter().zip(triangle_uv) {
            mesh.vertices.push(source.vertices[index as usize]);
            uv.push(coordinate(point, page));
        }
        mesh.triangles.push([base, base + 1, base + 2]);
        sliver += 1;
    }
    Ok(Unwrapped {
        mesh,
        uv,
        width,
        height,
    })
}

struct AtlasTriangle {
    pixels: [DVec2; 3],
    world: [DVec3; 3],
    normals: [DVec3; 3],
    area: f64,
}
impl AtlasTriangle {
    fn new(pixels: [DVec2; 3], world: [DVec3; 3], normals: [DVec3; 3]) -> Option<Self> {
        let ab = pixels[1] - pixels[0];
        let ac = pixels[2] - pixels[0];
        let area = ab.perp_dot(ac);
        (area.is_finite() && area != 0.0).then_some(Self {
            pixels,
            world,
            normals,
            area,
        })
    }
    fn weights(&self, point: DVec2) -> [f64; 3] {
        let u = (point - self.pixels[0]).perp_dot(self.pixels[2] - self.pixels[0]) / self.area;
        let v = (self.pixels[1] - self.pixels[0]).perp_dot(point - self.pixels[0]) / self.area;
        [1.0 - u - v, u, v]
    }
    fn surface_point(&self, point: DVec2) -> (DVec3, DVec3) {
        // Residuals only exist on the actual target face. Extend boundary
        // values into gutters instead of inventing an extrapolated coarse
        // normal beyond that face (which destroys zero identity residuals).
        let mut weights = self.weights(point).map(|weight| weight.max(0.0));
        let total: f64 = weights.iter().sum();
        weights = weights.map(|weight| weight / total);
        let position = self
            .world
            .into_iter()
            .zip(weights)
            .map(|(p, w)| p * w)
            .sum();
        let normal = self
            .normals
            .into_iter()
            .zip(weights)
            .map(|(n, w)| n * w)
            .sum::<DVec3>()
            .normalize_or_zero();
        (position, normal)
    }
    fn rasterize(
        &self,
        width: usize,
        height: usize,
        conservative: bool,
        mut visit: impl FnMut(usize),
    ) {
        let extra = if conservative { 0.5 } else { 0.0 };
        let minimum = self.pixels[0].min(self.pixels[1]).min(self.pixels[2]) - DVec2::splat(extra);
        let maximum = self.pixels[0].max(self.pixels[1]).max(self.pixels[2]) + DVec2::splat(extra);
        let minimum = minimum.ceil().max(DVec2::ZERO);
        let maximum = maximum
            .floor()
            .min(DVec2::new((width - 1) as f64, (height - 1) as f64));
        for y in minimum.y as usize..=maximum.y as usize {
            for x in minimum.x as usize..=maximum.x as usize {
                let point = DVec2::new(x as f64, y as f64);
                let inside = if conservative {
                    (0..3).all(|edge| {
                        let a = self.pixels[edge];
                        let direction = self.pixels[(edge + 1) % 3] - a;
                        direction.perp_dot(point - a) * self.area.signum()
                            >= -0.5 * (direction.x.abs() + direction.y.abs()) - 1.0e-8
                    })
                } else {
                    self.weights(point)
                        .into_iter()
                        .all(|weight| weight >= -1.0e-8)
                };
                if inside {
                    visit(y * width + x);
                }
            }
        }
    }
}

fn dilate(owners: &mut [u32], width: usize, height: usize) {
    let neighbours = |pixel: usize| {
        let x = pixel % width;
        let y = pixel / width;
        [
            x.checked_sub(1).map(|_| pixel - 1),
            (x + 1 < width).then_some(pixel + 1),
            y.checked_sub(1).map(|_| pixel - width),
            (y + 1 < height).then_some(pixel + width),
            (x > 0 && y > 0).then(|| pixel - width - 1),
            (x + 1 < width && y > 0).then(|| pixel - width + 1),
            (x > 0 && y + 1 < height).then(|| pixel + width - 1),
            (x + 1 < width && y + 1 < height).then(|| pixel + width + 1),
        ]
    };
    let mut frontier: Vec<_> = owners
        .iter()
        .enumerate()
        .filter(|(i, owner)| {
            **owner != EMPTY
                && neighbours(*i)
                    .into_iter()
                    .flatten()
                    .any(|j| owners[j] == EMPTY)
        })
        .map(|(i, _)| i)
        .collect();
    for _ in 0..PADDING {
        let mut next = Vec::new();
        for pixel in frontier {
            for neighbour in neighbours(pixel).into_iter().flatten() {
                if owners[neighbour] == EMPTY {
                    owners[neighbour] = owners[pixel];
                    next.push(neighbour);
                }
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
}
