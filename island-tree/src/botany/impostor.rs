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

use super::model::{BotanicalPrototype, BotanicalTexture};

const TILE_SIZE: u32 = 256;
const ATLAS_WIDTH: u32 = TILE_SIZE * 2;
const PADDING_PIXELS: f32 = 12.0;

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
                [
                    leaf.blade_base_metres,
                    leaf.blade_base_metres + leaf.direction * leaf.length_metres,
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
        rasterize_view(&mut rgba, prototype, Projection::new(view, bounds, scale));
    }
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

fn rasterize_view(rgba: &mut [u8], prototype: &BotanicalPrototype, projection: Projection) {
    for axis in &prototype.graph.axes {
        let colour = match axis.order {
            0 => [112, 101, 84, 255],
            1 => [96, 89, 73, 255],
            _ => [78, 75, 61, 245],
        };
        for (segment, points) in axis.points_metres.windows(2).enumerate() {
            let radius =
                axis.radii_metres[segment].max(axis.radii_metres[segment + 1]) * projection.scale;
            draw_capsule(
                rgba,
                projection,
                projection.point(points[0]),
                projection.point(points[1]),
                radius.max(0.65),
                colour,
            );
        }
    }
    for leaf in &prototype.leaves {
        let tip = leaf.blade_base_metres + leaf.direction * leaf.length_metres;
        let green = (82.0 + leaf.light_exposure * 42.0 - leaf.age * 13.0).clamp(55.0, 132.0);
        let colour = [(green * 0.48) as u8, green as u8, (green * 0.38) as u8, 190];
        draw_capsule(
            rgba,
            projection,
            projection.point(leaf.blade_base_metres),
            projection.point(tip),
            (leaf.width_metres * projection.scale * 0.55).max(0.72),
            colour,
        );
    }
}

fn draw_capsule(
    rgba: &mut [u8],
    projection: Projection,
    start: [f32; 2],
    end: [f32; 2],
    radius: f32,
    colour: [u8; 4],
) {
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
                blend_pixel(rgba, x as u32, y as u32, colour, coverage);
            }
        }
    }
}

fn blend_pixel(rgba: &mut [u8], x: u32, y: u32, colour: [u8; 4], coverage: f32) {
    let index = ((y * ATLAS_WIDTH + x) * 4) as usize;
    let source_alpha = f32::from(colour[3]) / 255.0 * coverage;
    let destination_alpha = f32::from(rgba[index + 3]) / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha <= f32::EPSILON {
        return;
    }
    for channel in 0..3 {
        let source = f32::from(colour[channel]) / 255.0;
        let destination = f32::from(rgba[index + channel]) / 255.0;
        rgba[index + channel] = (((source * source_alpha
            + destination * destination_alpha * (1.0 - source_alpha))
            / output_alpha)
            * 255.0)
            .round() as u8;
    }
    rgba[index + 3] = (output_alpha * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
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
        assert!(first.front_width_metres > 1.0);
        assert!(first.side_width_metres > 1.0);
        assert!((first.front_width_metres - first.side_width_metres).abs() < f32::EPSILON);
        assert!(first.bottom_metres < 0.0);
        assert!(first.top_metres > first.bottom_metres);
    }
}
