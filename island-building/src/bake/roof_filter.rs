//! Roof-only CPU mip filtering; source material maps remain untouched.
use super::{Sample, encode, srgb};
use crate::MaterialTexture;

#[derive(Clone, Copy)]
struct Pixel {
    albedo: [f32; 4],
    normal: [f32; 3],
    masks: [f32; 3],
}
impl Pixel {
    fn blend(self, other: Self, weight: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * weight;
        Self {
            albedo: std::array::from_fn(|i| mix(self.albedo[i], other.albedo[i])),
            normal: std::array::from_fn(|i| mix(self.normal[i], other.normal[i])),
            masks: std::array::from_fn(|i| mix(self.masks[i], other.masks[i])),
        }
    }
    fn normalized(mut self) -> Self {
        let length = self.normal.iter().map(|v| v * v).sum::<f32>().sqrt();
        self.normal = if length > 0.00001 {
            self.normal.map(|v| v / length)
        } else {
            [0., 0., 1.]
        };
        self
    }
    fn sample(self) -> Sample {
        let value = self.normalized();
        Sample {
            albedo: [
                srgb(value.albedo[0]),
                srgb(value.albedo[1]),
                srgb(value.albedo[2]),
                byte(value.albedo[3]),
            ],
            normal: [
                encode(value.normal[0]),
                encode(value.normal[1]),
                encode(value.normal[2]),
                255,
            ],
            ao: byte(value.masks[0]),
            roughness: byte(value.masks[1]),
            metallic: byte(value.masks[2]),
            height: 0.,
        }
    }
}
fn byte(value: f32) -> u8 {
    (value * 255.).round().clamp(0., 255.) as u8
}
fn linear(value: u8) -> f32 {
    let value = f32::from(value) / 255.;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
struct Level {
    size: usize,
    pixels: Vec<Pixel>,
}
impl Level {
    fn source(texture: &MaterialTexture) -> Self {
        let pixels = texture
            .albedo
            .chunks_exact(4)
            .enumerate()
            .map(|(index, colour)| {
                let offset = index * 4;
                Pixel {
                    albedo: [
                        linear(colour[0]),
                        linear(colour[1]),
                        linear(colour[2]),
                        f32::from(colour[3]) / 255.,
                    ],
                    normal: std::array::from_fn(|i| {
                        f32::from(texture.normal[offset + i]) / 127.5 - 1.
                    }),
                    masks: std::array::from_fn(|i| f32::from(texture.masks[offset + i]) / 255.),
                }
                .normalized()
            })
            .collect();
        Self {
            size: texture.size as usize,
            pixels,
        }
    }
    fn reduced(&self) -> Self {
        let size = self.size.div_ceil(2);
        let pixels = (0..size * size)
            .map(|index| {
                let x = index % size * 2;
                let y = index / size * 2;
                let x1 = (x + 1).min(self.size - 1);
                let y1 = (y + 1).min(self.size - 1);
                self.pixels[y * self.size + x]
                    .blend(self.pixels[y * self.size + x1], 0.5)
                    .blend(
                        self.pixels[y1 * self.size + x]
                            .blend(self.pixels[y1 * self.size + x1], 0.5),
                        0.5,
                    )
                    .normalized()
            })
            .collect();
        Self { size, pixels }
    }
    fn sample(&self, u: f32, v: f32) -> Pixel {
        let x = u.rem_euclid(1.) * self.size as f32 - 0.5;
        let y = v.rem_euclid(1.) * self.size as f32 - 0.5;
        let wrap = |value: f32| (value.rem_euclid(self.size as f32)) as usize;
        let x0 = wrap(x.floor());
        let x1 = (x0 + 1) % self.size;
        let y0 = wrap(y.floor());
        let y1 = (y0 + 1) % self.size;
        self.pixels[y0 * self.size + x0]
            .blend(self.pixels[y0 * self.size + x1], x - x.floor())
            .blend(
                self.pixels[y1 * self.size + x0]
                    .blend(self.pixels[y1 * self.size + x1], x - x.floor()),
                y - y.floor(),
            )
    }
}

pub(super) struct FilteredRoof {
    physical_size: [f32; 2],
    levels: Vec<Level>,
    lod: f32,
}
impl FilteredRoof {
    /// Physical atlas-pixel dimensions select a conservative isotropic footprint.
    pub(super) fn new(texture: &MaterialTexture, footprint: [f32; 2]) -> Self {
        let source = Level::source(texture);
        let mut levels = vec![source];
        while levels.last().expect("source level").size > 1 {
            levels.push(levels.last().expect("previous level").reduced());
        }
        let texels = (footprint[0] / texture.physical_size[0])
            .max(footprint[1] / texture.physical_size[1])
            * texture.size as f32;
        let lod = texels.max(1.).log2().min((levels.len() - 1) as f32);
        Self {
            physical_size: texture.physical_size,
            levels,
            lod,
        }
    }
    pub(super) fn sample(&self, x: f32, y: f32) -> Sample {
        let u = x / self.physical_size[0];
        let v = y / self.physical_size[1];
        let index = self.lod.floor() as usize;
        let next = (index + 1).min(self.levels.len() - 1);
        self.levels[index]
            .sample(u, v)
            .blend(self.levels[next].sample(u, v), self.lod.fract())
            .sample()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MaterialSlot;

    fn checker() -> MaterialTexture {
        let mut texture = MaterialTexture {
            slot: MaterialSlot::Roof,
            size: 8,
            albedo: Vec::new(),
            normal: Vec::new(),
            masks: Vec::new(),
            physical_size: [2., 2.],
            source_hash: "filter-checker".into(),
        };
        for y in 0..8 {
            for x in 0..8 {
                let bright = (x + y) % 2 == 0;
                texture.albedo.extend([if bright { 255 } else { 0 }; 3]);
                texture.albedo.push(255);
                texture.normal.extend(if bright {
                    [230, 128, 204, 255]
                } else {
                    [25, 128, 204, 255]
                });
                texture.masks.extend(if bright {
                    [255, 200, 100, 255]
                } else {
                    [0, 100, 0, 255]
                });
            }
        }
        texture
    }

    #[test]
    fn minified_checker_preserves_linear_mean_and_unit_normals_at_every_phase() {
        let original = checker();
        let roof = FilteredRoof::new(&original, [0.5, 0.5]);
        for phase in [0., 0.031, 0.24, 0.499, 0.91, 1.7] {
            let sample = roof.sample(phase, phase * 0.37);
            assert_eq!(sample.albedo, [188, 188, 188, 255]);
            assert_eq!(
                [sample.ao, sample.roughness, sample.metallic],
                [128, 150, 50]
            );
            let normal: [f32; 3] =
                std::array::from_fn(|i| f32::from(sample.normal[i]) / 127.5 - 1.);
            let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt();
            assert!((length - 1.).abs() < 0.01);
            assert!(normal[0].abs() < 0.01 && normal[2] > 0.99);
        }
        assert_eq!(original.albedo[0], 255);
        assert_eq!(original.albedo[4], 0);
    }

    #[test]
    fn constant_colour_survives_fractional_mips_and_repeat_filtering() {
        let mut texture = checker();
        for colour in texture.albedo.chunks_exact_mut(4) {
            colour.copy_from_slice(&[47, 139, 201, 255]);
        }
        let roof = FilteredRoof::new(&texture, [0.83, 0.31]);
        for position in [-7.13, 0., 0.29, 1.99, 5.7] {
            assert_eq!(
                roof.sample(position, position * 1.3).albedo,
                [47, 139, 201, 255]
            );
        }
    }

    #[test]
    fn bilinear_sampling_keeps_source_texel_centres_and_periodic_phase() {
        let mut texture = checker();
        for (index, colour) in texture.albedo.chunks_exact_mut(4).enumerate() {
            let value = [24, 90, 160, 240][index % 4];
            colour.copy_from_slice(&[value, value, value, 255]);
        }
        let roof = FilteredRoof::new(&texture, [0., 0.]);
        for (index, value) in [24, 90, 160, 240].into_iter().enumerate() {
            let x = (index as f32 + 0.5) * 0.25;
            assert_eq!(roof.sample(x, 0.125).albedo, [value, value, value, 255]);
            assert_eq!(
                roof.sample(x - 2., 2.125).albedo,
                [value, value, value, 255]
            );
        }
        let expected = srgb((linear(90) + linear(160)) * 0.5);
        assert_eq!(
            roof.sample(0.5, 0.125).albedo,
            [expected, expected, expected, 255]
        );
    }
}
