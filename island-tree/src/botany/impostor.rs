//! Deterministic far-distance image impostors derived from botanical organs.
//!
//! The atlas is renderer-neutral: eight evenly-spaced azimuth views share one
//! transparent texture, while the Bevy compiler decides how those views become
//! cards and blends between them.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use motu::Vec3;

use super::model::{BotanicalPrototype, BotanicalTexture, ReproductiveState};

const TILE_SIZE: u32 = 256;
pub(crate) const IMPOSTOR_VIEW_COUNT: usize = 8;
pub(crate) const IMPOSTOR_ATLAS_COLUMNS: u32 = 4;
const IMPOSTOR_ATLAS_ROWS: u32 = 2;
const ATLAS_WIDTH: u32 = TILE_SIZE * IMPOSTOR_ATLAS_COLUMNS;
const ATLAS_HEIGHT: u32 = TILE_SIZE * IMPOSTOR_ATLAS_ROWS;
const PADDING_PIXELS: f32 = 12.0;
const LEAF_SPINE_POINTS: usize = 7;

/// An eight-view transparent atlas and the physical bounds its cards occupy.
#[derive(Clone, Debug, PartialEq)]
pub struct BotanicalImpostor {
    pub albedo: BotanicalTexture,
    pub card_width_metres: f32,
    pub view_centres_metres: [f32; IMPOSTOR_VIEW_COUNT],
    pub bottom_metres: f32,
    pub top_metres: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct ImpostorView {
    pub right: [f32; 2],
    pub forward: [f32; 2],
    index: usize,
}

impl ImpostorView {
    pub(crate) fn at(index: usize) -> Self {
        debug_assert!(index < IMPOSTOR_VIEW_COUNT);
        let angle = index as f32 * std::f32::consts::TAU / IMPOSTOR_VIEW_COUNT as f32;
        Self {
            right: [angle.cos(), -angle.sin()],
            forward: [angle.sin(), angle.cos()],
            index,
        }
    }

    fn horizontal(self, point: Vec3) -> f32 {
        self.right[0].mul_add(point.x, self.right[1] * point.y)
    }

    fn depth(self, point: Vec3) -> f32 {
        self.forward[0].mul_add(point.x, self.forward[1] * point.y)
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    horizontal_min: [f32; IMPOSTOR_VIEW_COUNT],
    horizontal_max: [f32; IMPOSTOR_VIEW_COUNT],
    bottom: f32,
    top: f32,
}

impl Bounds {
    fn from_prototype(prototype: &BotanicalPrototype) -> Self {
        let mut bounds = Self {
            horizontal_min: [f32::INFINITY; IMPOSTOR_VIEW_COUNT],
            horizontal_max: [f32::NEG_INFINITY; IMPOSTOR_VIEW_COUNT],
            bottom: f32::INFINITY,
            top: f32::NEG_INFINITY,
        };
        prototype
            .graph
            .axes
            .iter()
            .flat_map(|axis| axis.points_metres)
            .chain(prototype.leaves.iter().flat_map(|leaf| {
                prototype.leaf_archetypes[usize::from(leaf.archetype)]
                    .vertices
                    .iter()
                    .map(|vertex| leaf_vertex(*leaf, *vertex))
            }))
            .chain(prototype.reproductive_organs.iter().flat_map(|organ| {
                [
                    organ.base_metres,
                    organ.base_metres + organ.direction * organ.length_metres,
                ]
            }))
            .for_each(|point| bounds.include(point));
        if !bounds.horizontal_min[0].is_finite() {
            return Self {
                horizontal_min: [-0.5; IMPOSTOR_VIEW_COUNT],
                horizontal_max: [0.5; IMPOSTOR_VIEW_COUNT],
                bottom: 0.0,
                top: 1.0,
            };
        }
        bounds
    }

    fn include(&mut self, point: Vec3) {
        for index in 0..IMPOSTOR_VIEW_COUNT {
            let horizontal = ImpostorView::at(index).horizontal(point);
            self.horizontal_min[index] = self.horizontal_min[index].min(horizontal);
            self.horizontal_max[index] = self.horizontal_max[index].max(horizontal);
        }
        self.bottom = self.bottom.min(point.z);
        self.top = self.top.max(point.z);
    }

    fn horizontal(self, view: ImpostorView) -> (f32, f32) {
        (
            self.horizontal_min[view.index],
            self.horizontal_max[view.index],
        )
    }
}

#[derive(Clone, Copy)]
struct Projection {
    view: ImpostorView,
    tile_left: f32,
    tile_top: f32,
    horizontal_centre: f32,
    vertical_centre: f32,
    scale: f32,
}

impl Projection {
    fn scale(bounds: Bounds) -> f32 {
        let horizontal_span = bounds
            .horizontal_min
            .iter()
            .zip(bounds.horizontal_max)
            .map(|(minimum, maximum)| maximum - minimum)
            .fold(0.1_f32, f32::max);
        let vertical_span = (bounds.top - bounds.bottom).max(0.1);
        let drawable = TILE_SIZE as f32 - PADDING_PIXELS * 2.0;
        drawable / horizontal_span.max(vertical_span)
    }

    fn new(view: ImpostorView, bounds: Bounds, scale: f32) -> Self {
        let (minimum, maximum) = bounds.horizontal(view);
        Self {
            view,
            tile_left: (view.index as u32 % IMPOSTOR_ATLAS_COLUMNS) as f32 * TILE_SIZE as f32,
            tile_top: (view.index as u32 / IMPOSTOR_ATLAS_COLUMNS) as f32 * TILE_SIZE as f32,
            horizontal_centre: f32::midpoint(minimum, maximum),
            vertical_centre: f32::midpoint(bounds.bottom, bounds.top),
            scale,
        }
    }

    fn point(self, point: Vec3) -> [f32; 2] {
        [
            self.tile_left
                + TILE_SIZE as f32 * 0.5
                + (self.view.horizontal(point) - self.horizontal_centre) * self.scale,
            self.tile_top + TILE_SIZE as f32 * 0.5 - (point.z - self.vertical_centre) * self.scale,
        ]
    }

    fn x_bounds(self) -> (i32, i32) {
        (
            self.tile_left as i32,
            self.tile_left as i32 + TILE_SIZE.cast_signed() - 1,
        )
    }

    fn y_bounds(self) -> (i32, i32) {
        (
            self.tile_top as i32,
            self.tile_top as i32 + TILE_SIZE.cast_signed() - 1,
        )
    }
}

/// Rasterizes eight azimuth views directly from the generated organ graph.
#[must_use]
pub fn generate_botanical_impostor(prototype: &BotanicalPrototype) -> BotanicalImpostor {
    let bounds = Bounds::from_prototype(prototype);
    let scale = Projection::scale(bounds);
    let mut rgba = vec![0_u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize];
    let mut depth = vec![f32::NEG_INFINITY; (ATLAS_WIDTH * ATLAS_HEIGHT) as usize];
    for index in 0..IMPOSTOR_VIEW_COUNT {
        rasterize_view(
            &mut rgba,
            &mut depth,
            prototype,
            Projection::new(ImpostorView::at(index), bounds, scale),
        );
    }
    dilate_transparent_rgb(&mut rgba, 3);
    let card_span = TILE_SIZE as f32 / scale;
    let vertical_centre = f32::midpoint(bounds.bottom, bounds.top);
    BotanicalImpostor {
        albedo: BotanicalTexture {
            width: ATLAS_WIDTH,
            height: ATLAS_HEIGHT,
            rgba,
        },
        card_width_metres: card_span,
        view_centres_metres: std::array::from_fn(|index| {
            f32::midpoint(bounds.horizontal_min[index], bounds.horizontal_max[index])
        }),
        bottom_metres: vertical_centre - card_span * 0.5,
        top_metres: vertical_centre + card_span * 0.5,
    }
}

fn rasterize_view(
    rgba: &mut [u8],
    depth: &mut [f32],
    prototype: &BotanicalPrototype,
    projection: Projection,
) {
    rasterize_wood_mesh(
        rgba,
        depth,
        projection,
        &prototype.wood,
        &prototype.bark_albedo,
    );
    rasterize_wood_mesh(
        rgba,
        depth,
        projection,
        &prototype.microtwigs,
        &prototype.bark_albedo,
    );
    let leaf_spines = prototype.leaf_archetypes.each_ref().map(leaf_spine);
    for leaf in &prototype.leaves {
        let spine = leaf_spines[usize::from(leaf.archetype)];
        for (segment, points) in spine.windows(2).enumerate() {
            let fraction = (segment as f32 + 0.5) / (LEAF_SPINE_POINTS - 1) as f32;
            let taper = (fraction * std::f32::consts::PI)
                .sin()
                .max(0.0)
                .powf(0.34)
                .max(0.24);
            let start = leaf_vertex(*leaf, points[0]);
            let end = leaf_vertex(*leaf, points[1]);
            let tile = f32::from(leaf.archetype % 4);
            let atlas_u = f32::midpoint(tile % 2.0, 0.5);
            let atlas_v = f32::midpoint((tile / 2.0).floor(), fraction);
            let sample = sample_texture(&prototype.leaf_albedo, atlas_u, atlas_v);
            let normal = leaf.normal.normalize_or(Vec3::Z);
            let sun = Vec3::new(-0.42, -0.58, 0.70).normalize_or(Vec3::Z);
            let facing = normal.dot(sun).abs();
            let shade = 0.50 + facing * 0.24 + leaf.light_exposure * 0.24 - leaf.age * 0.055;
            let colour = shaded(sample, shade, sample[3].max(210));
            draw_capsule(
                rgba,
                depth,
                projection,
                start,
                end,
                (leaf.width_metres * projection.scale * 0.52 * taper).max(0.62),
                colour,
            );
        }
    }
    for organ in &prototype.reproductive_organs {
        let start = organ.base_metres;
        let end = start + organ.direction * organ.length_metres;
        let colour = match organ.state {
            ReproductiveState::Flower => [150, 38, 28, 245],
            ReproductiveState::Fruit => [82, 48, 28, 255],
        };
        draw_capsule(
            rgba,
            depth,
            projection,
            start,
            end,
            (organ.radius_metres * projection.scale * 0.38).clamp(0.72, 2.4),
            colour,
        );
    }
}

fn rasterize_wood_mesh(
    rgba: &mut [u8],
    depth: &mut [f32],
    projection: Projection,
    mesh: &motu::Mesh,
    texture: &BotanicalTexture,
) {
    let (tile_left, tile_right) = projection.x_bounds();
    let (tile_top, tile_bottom) = projection.y_bounds();
    let sun = Vec3::new(-0.42, -0.58, 0.70).normalize_or(Vec3::Z);
    for triangle in mesh.triangles.as_chunks::<3>().0 {
        let indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let (Some(first), Some(second), Some(third)) = (
            mesh.vertices.get(indices[0]),
            mesh.vertices.get(indices[1]),
            mesh.vertices.get(indices[2]),
        ) else {
            continue;
        };
        let points = [*first, *second, *third];
        let screen = points.each_ref().map(|point| projection.point(*point));
        let area = edge(screen[0], screen[1], screen[2]);
        if area.abs() < 1.0e-5 {
            continue;
        }
        let min_x = screen
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min)
            .floor() as i32;
        let max_x = screen
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil() as i32;
        let min_y = screen
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min)
            .floor() as i32;
        let max_y = screen
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil() as i32;
        let depths = points.each_ref().map(|point| projection.view.depth(*point));
        let uvs = indices.map(|index| mesh.uv.get(index).copied().unwrap_or(motu::Vec2::ZERO));
        let normals = indices.map(|index| mesh.normals.get(index).copied().unwrap_or(Vec3::Z));
        for y in min_y.clamp(tile_top, tile_bottom)..=max_y.clamp(tile_top, tile_bottom) {
            for x in min_x.clamp(tile_left, tile_right)..=max_x.clamp(tile_left, tile_right) {
                let pixel_centre = [x as f32 + 0.5, y as f32 + 0.5];
                let barycentric = [
                    edge(screen[1], screen[2], pixel_centre) / area,
                    edge(screen[2], screen[0], pixel_centre) / area,
                    edge(screen[0], screen[1], pixel_centre) / area,
                ];
                if barycentric.iter().any(|weight| *weight < -0.001) {
                    continue;
                }
                let surface_depth = barycentric[0].mul_add(
                    depths[0],
                    barycentric[1].mul_add(depths[1], barycentric[2] * depths[2]),
                );
                let pixel = (y as u32 * ATLAS_WIDTH + x as u32) as usize;
                if surface_depth < depth[pixel] {
                    continue;
                }
                let uv =
                    uvs[0] * barycentric[0] + uvs[1] * barycentric[1] + uvs[2] * barycentric[2];
                let normal = (normals[0] * barycentric[0]
                    + normals[1] * barycentric[1]
                    + normals[2] * barycentric[2])
                    .normalize_or(Vec3::Z);
                let diffuse = normal.dot(sun).abs();
                let upward = normal.z.abs();
                let shade = 0.56 + diffuse * 0.30 + upward * 0.10;
                let sample = sample_texture(texture, uv.x.rem_euclid(1.0), uv.y.rem_euclid(1.0));
                depth[pixel] = surface_depth;
                write_pixel(rgba, x as u32, y as u32, shaded(sample, shade, 255), 1.0);
            }
        }
    }
}

fn edge(start: [f32; 2], end: [f32; 2], point: [f32; 2]) -> f32 {
    (point[0] - start[0]).mul_add(
        end[1] - start[1],
        -(point[1] - start[1]) * (end[0] - start[0]),
    )
}

fn sample_texture(texture: &BotanicalTexture, u: f32, v: f32) -> [u8; 4] {
    let x = (u.clamp(0.0, 1.0) * texture.width.saturating_sub(1) as f32).round() as u32;
    let y = (v.clamp(0.0, 1.0) * texture.height.saturating_sub(1) as f32).round() as u32;
    let index = ((y * texture.width + x) * 4) as usize;
    texture.rgba[index..index + 4]
        .try_into()
        .expect("botanical textures contain complete RGBA pixels")
}

fn shaded(sample: [u8; 4], intensity: f32, alpha: u8) -> [u8; 4] {
    let mut colour = sample;
    for channel in &mut colour[..3] {
        *channel = (f32::from(*channel) * intensity).clamp(0.0, 255.0) as u8;
    }
    colour[3] = alpha;
    colour
}

fn leaf_spine(archetype: &motu::Mesh) -> [Vec3; LEAF_SPINE_POINTS] {
    std::array::from_fn(|sample| {
        let target = sample as f32 / (LEAF_SPINE_POINTS - 1) as f32;
        let nearest = archetype
            .vertices
            .iter()
            .map(|vertex| (vertex.x - target).abs())
            .fold(f32::INFINITY, f32::min);
        let (sum, count) =
            archetype
                .vertices
                .iter()
                .fold((Vec3::ZERO, 0_u32), |(sum, count), vertex| {
                    if ((vertex.x - target).abs() - nearest).abs() < 0.000_1 {
                        (sum + *vertex, count + 1)
                    } else {
                        (sum, count)
                    }
                });
        if count == 0 {
            Vec3::new(target, 0.0, 0.0)
        } else {
            sum / count as f32
        }
    })
}

fn leaf_vertex(leaf: super::model::LeafOrgan, vertex: Vec3) -> Vec3 {
    let direction = leaf.direction.normalize_or(Vec3::X);
    let normal = leaf.normal.normalize_or(Vec3::Z);
    let transverse = direction.cross(normal).normalize_or(Vec3::Y);
    leaf.blade_base_metres
        + direction * vertex.x * leaf.length_metres
        + normal * vertex.z * leaf.length_metres
        + transverse * vertex.y * leaf.width_metres
}

fn dilate_transparent_rgb(rgba: &mut [u8], iterations: usize) {
    let mut source = rgba.to_vec();
    for _ in 0..iterations {
        source.copy_from_slice(rgba);
        for y in 0..ATLAS_HEIGHT {
            for x in 0..ATLAS_WIDTH {
                let index = ((y * ATLAS_WIDTH + x) * 4) as usize;
                if source[index + 3] != 0 {
                    continue;
                }
                let tile_min = x / TILE_SIZE * TILE_SIZE;
                let tile_max = tile_min + TILE_SIZE - 1;
                let tile_top = y / TILE_SIZE * TILE_SIZE;
                let tile_bottom = tile_top + TILE_SIZE - 1;
                let mut sum = [0_u32; 3];
                let mut count = 0_u32;
                for offset_y in -1_i32..=1 {
                    for offset_x in -1_i32..=1 {
                        if offset_x == 0 && offset_y == 0 {
                            continue;
                        }
                        let neighbour_x = x.cast_signed() + offset_x;
                        let neighbour_y = y.cast_signed() + offset_y;
                        if neighbour_x < tile_min.cast_signed()
                            || neighbour_x > tile_max.cast_signed()
                            || neighbour_y < tile_top.cast_signed()
                            || neighbour_y > tile_bottom.cast_signed()
                        {
                            continue;
                        }
                        let neighbour = ((neighbour_y.cast_unsigned() * ATLAS_WIDTH
                            + neighbour_x.cast_unsigned())
                            * 4) as usize;
                        if source[neighbour + 3] == 0
                            && source[neighbour..neighbour + 3]
                                .iter()
                                .all(|channel| *channel == 0)
                        {
                            continue;
                        }
                        for channel in 0..3 {
                            sum[channel] += u32::from(source[neighbour + channel]);
                        }
                        count += 1;
                    }
                }
                for channel in 0..3 {
                    if let Some(average) = sum[channel].checked_div(count) {
                        rgba[index + channel] = average as u8;
                    }
                }
            }
        }
    }
}

fn draw_capsule(
    rgba: &mut [u8],
    depth: &mut [f32],
    projection: Projection,
    world_start: Vec3,
    world_end: Vec3,
    radius: f32,
    colour: [u8; 4],
) {
    let start = projection.point(world_start);
    let end = projection.point(world_end);
    let start_depth = projection.view.depth(world_start);
    let end_depth = projection.view.depth(world_end);
    let (tile_min, tile_max) = projection.x_bounds();
    let min_x = ((start[0].min(end[0]) - radius).floor() as i32).clamp(tile_min, tile_max);
    let max_x = ((start[0].max(end[0]) + radius).ceil() as i32).clamp(tile_min, tile_max);
    let (tile_top, tile_bottom) = projection.y_bounds();
    let min_y = ((start[1].min(end[1]) - radius).floor() as i32).clamp(tile_top, tile_bottom);
    let max_y = ((start[1].max(end[1]) + radius).ceil() as i32).clamp(tile_top, tile_bottom);
    let delta = [end[0] - start[0], end[1] - start[1]];
    let length_squared = delta[0].mul_add(delta[0], delta[1] * delta[1]);
    let outer = radius + 0.75;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let along = if length_squared > f32::EPSILON {
                (((point[0] - start[0]) * delta[0] + (point[1] - start[1]) * delta[1])
                    / length_squared)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            let nearest = [
                delta[0].mul_add(along, start[0]),
                delta[1].mul_add(along, start[1]),
            ];
            let dx = point[0] - nearest[0];
            let dy = point[1] - nearest[1];
            let distance = dx.mul_add(dx, dy * dy).sqrt();
            if distance <= outer {
                let coverage = (outer - distance).clamp(0.0, 1.0);
                let pixel = (y as u32 * ATLAS_WIDTH + x as u32) as usize;
                let surface_depth = (end_depth - start_depth).mul_add(along, start_depth);
                if surface_depth >= depth[pixel] {
                    depth[pixel] = surface_depth;
                    write_pixel(rgba, x as u32, y as u32, colour, coverage);
                }
            }
        }
    }
}

fn write_pixel(rgba: &mut [u8], x: u32, y: u32, colour: [u8; 4], coverage: f32) {
    let index = ((y * ATLAS_WIDTH + x) * 4) as usize;
    rgba[index..index + 3].copy_from_slice(&colour[..3]);
    rgba[index + 3] = (f32::from(colour[3]) * coverage).round() as u8;
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{BotanicalRecipe, generate_botanical_prototype};

    #[test]
    fn impostor_is_deterministic_transparent_and_bounded() {
        let prototype = generate_botanical_prototype(42, BotanicalRecipe::default()).unwrap();
        let first = generate_botanical_impostor(&prototype);
        let second = generate_botanical_impostor(&prototype);
        assert_eq!(first, second);
        assert_eq!(first.albedo.width, 1024);
        assert_eq!(first.albedo.height, 512);
        let pixels = first.albedo.rgba.as_chunks::<4>().0;
        assert!(pixels.iter().any(|pixel| pixel[3] == 0));
        assert!(pixels.iter().any(|pixel| pixel[3] > 0));
        for view in 0..IMPOSTOR_VIEW_COUNT {
            let tile_left = view as u32 % IMPOSTOR_ATLAS_COLUMNS * TILE_SIZE;
            let tile_top = view as u32 / IMPOSTOR_ATLAS_COLUMNS * TILE_SIZE;
            assert!((tile_top..tile_top + TILE_SIZE).any(|y| {
                (tile_left..tile_left + TILE_SIZE).any(|x| {
                    let index = (y * ATLAS_WIDTH + x) as usize;
                    pixels[index][3] > 0
                })
            }));
        }
        let visible_tones: HashSet<_> = pixels
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        assert!(visible_tones.len() > 64);
        assert!(first.card_width_metres > 1.0);
        assert!(
            first
                .view_centres_metres
                .iter()
                .all(|centre| centre.is_finite())
        );
        assert!(first.bottom_metres < 0.0);
        assert!(first.top_metres > first.bottom_metres);
    }
}
