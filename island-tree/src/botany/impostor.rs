//! Deterministic far-distance image impostors derived from botanical organs.
//!
//! The atlas is renderer-neutral: two orthogonal views share one transparent
//! texture, while the Bevy compiler decides how those views become cards.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use motu::Vec3;

use super::model::{BotanicalPrototype, BotanicalTexture, ReproductiveState};

const TILE_SIZE: u32 = 256;
const ATLAS_WIDTH: u32 = TILE_SIZE * 2;
const PADDING_PIXELS: f32 = 12.0;
const LEAF_SPINE_POINTS: usize = 7;

/// A two-view transparent atlas and the physical bounds its cards occupy.
#[derive(Clone, Debug, PartialEq)]
pub struct BotanicalImpostor {
    pub albedo: BotanicalTexture,
    pub front_width_metres: f32,
    pub side_width_metres: f32,
    pub front_centre_metres: f32,
    pub side_centre_metres: f32,
    pub bottom_metres: f32,
    pub top_metres: f32,
}

#[derive(Clone, Copy)]
enum View {
    Front,
    Side,
}

impl View {
    fn horizontal(self, point: Vec3) -> f32 {
        match self {
            Self::Front => point.x,
            Self::Side => point.y,
        }
    }

    fn depth(self, point: Vec3) -> f32 {
        match self {
            Self::Front => point.y,
            Self::Side => point.x,
        }
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    front_min: f32,
    front_max: f32,
    side_min: f32,
    side_max: f32,
    bottom: f32,
    top: f32,
}

impl Bounds {
    fn from_prototype(prototype: &BotanicalPrototype) -> Self {
        let mut bounds = Self {
            front_min: f32::INFINITY,
            front_max: f32::NEG_INFINITY,
            side_min: f32::INFINITY,
            side_max: f32::NEG_INFINITY,
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
        if !bounds.front_min.is_finite() {
            return Self {
                front_min: -0.5,
                front_max: 0.5,
                side_min: -0.5,
                side_max: 0.5,
                bottom: 0.0,
                top: 1.0,
            };
        }
        bounds
    }

    fn include(&mut self, point: Vec3) {
        self.front_min = self.front_min.min(point.x);
        self.front_max = self.front_max.max(point.x);
        self.side_min = self.side_min.min(point.y);
        self.side_max = self.side_max.max(point.y);
        self.bottom = self.bottom.min(point.z);
        self.top = self.top.max(point.z);
    }

    fn horizontal(self, view: View) -> (f32, f32) {
        match view {
            View::Front => (self.front_min, self.front_max),
            View::Side => (self.side_min, self.side_max),
        }
    }
}

#[derive(Clone, Copy)]
struct Projection {
    view: View,
    tile_left: f32,
    horizontal_centre: f32,
    vertical_centre: f32,
    scale: f32,
}

impl Projection {
    fn scale(bounds: Bounds) -> f32 {
        let front_span = (bounds.front_max - bounds.front_min).max(0.1);
        let side_span = (bounds.side_max - bounds.side_min).max(0.1);
        let vertical_span = (bounds.top - bounds.bottom).max(0.1);
        let drawable = TILE_SIZE as f32 - PADDING_PIXELS * 2.0;
        drawable / front_span.max(side_span).max(vertical_span)
    }

    fn new(view: View, bounds: Bounds, scale: f32) -> Self {
        let (minimum, maximum) = bounds.horizontal(view);
        Self {
            view,
            tile_left: match view {
                View::Front => 0.0,
                View::Side => TILE_SIZE as f32,
            },
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
            TILE_SIZE as f32 * 0.5 - (point.z - self.vertical_centre) * self.scale,
        ]
    }

    fn x_bounds(self) -> (i32, i32) {
        (
            self.tile_left as i32,
            self.tile_left as i32 + TILE_SIZE.cast_signed() - 1,
        )
    }
}

/// Rasterizes front and side views directly from the generated organ graph.
#[must_use]
pub fn generate_botanical_impostor(prototype: &BotanicalPrototype) -> BotanicalImpostor {
    let bounds = Bounds::from_prototype(prototype);
    let scale = Projection::scale(bounds);
    let mut rgba = vec![0_u8; (ATLAS_WIDTH * TILE_SIZE * 4) as usize];
    for view in [View::Front, View::Side] {
        let mut depth = vec![f32::NEG_INFINITY; (ATLAS_WIDTH * TILE_SIZE) as usize];
        rasterize_view(
            &mut rgba,
            &mut depth,
            prototype,
            Projection::new(view, bounds, scale),
        );
    }
    dilate_transparent_rgb(&mut rgba, 3);
    let card_span = TILE_SIZE as f32 / scale;
    let vertical_centre = f32::midpoint(bounds.bottom, bounds.top);
    BotanicalImpostor {
        albedo: BotanicalTexture {
            width: ATLAS_WIDTH,
            height: TILE_SIZE,
            rgba,
        },
        front_width_metres: card_span,
        side_width_metres: card_span,
        front_centre_metres: f32::midpoint(bounds.front_min, bounds.front_max),
        side_centre_metres: f32::midpoint(bounds.side_min, bounds.side_max),
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
    let height = (projection.scale.recip() * TILE_SIZE as f32).max(0.1);
    for axis in &prototype.graph.axes {
        for (segment, points) in axis.points_metres.windows(2).enumerate() {
            let radius =
                axis.radii_metres[segment].max(axis.radii_metres[segment + 1]) * projection.scale;
            let fraction = points[0].z / height;
            let sample = sample_texture(
                &prototype.bark_albedo,
                (f32::from(axis.order) * 0.173 + segment as f32 * 0.117).fract(),
                fraction.fract(),
            );
            let tangent = (points[1] - points[0]).normalize_or(Vec3::Z);
            let shade = (0.64 + tangent.z.abs() * 0.24 + axis.exposure * 0.12)
                * (1.0 - f32::from(axis.order.min(3)) * 0.055);
            let colour = shaded(sample, shade, 255);
            draw_capsule(
                rgba,
                depth,
                projection,
                points[0],
                points[1],
                radius.max(0.65),
                colour,
            );
        }
    }
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
            (organ.radius_metres * projection.scale * 0.38).max(0.72),
            colour,
        );
    }
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
        for y in 0..TILE_SIZE {
            for x in 0..ATLAS_WIDTH {
                let index = ((y * ATLAS_WIDTH + x) * 4) as usize;
                if source[index + 3] != 0 {
                    continue;
                }
                let tile_min = x / TILE_SIZE * TILE_SIZE;
                let tile_max = tile_min + TILE_SIZE - 1;
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
                            || !(0..TILE_SIZE.cast_signed()).contains(&neighbour_y)
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
    let min_y =
        ((start[1].min(end[1]) - radius).floor() as i32).clamp(0, TILE_SIZE.cast_signed() - 1);
    let max_y =
        ((start[1].max(end[1]) + radius).ceil() as i32).clamp(0, TILE_SIZE.cast_signed() - 1);
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
        assert_eq!(first.albedo.width, 512);
        assert_eq!(first.albedo.height, 256);
        let pixels = first.albedo.rgba.as_chunks::<4>().0;
        assert!(pixels.iter().any(|pixel| pixel[3] == 0));
        assert!(pixels.iter().any(|pixel| pixel[3] > 0));
        let visible_tones: HashSet<_> = pixels
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        assert!(visible_tones.len() > 64);
        assert!(first.front_width_metres > 1.0);
        assert!(first.side_width_metres > 1.0);
        assert!((first.front_width_metres - first.side_width_metres).abs() < f32::EPSILON);
        assert!(first.bottom_metres < 0.0);
        assert!(first.top_metres > first.bottom_metres);
    }
}
