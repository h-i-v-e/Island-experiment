//! Bounded circular contacts matching Unity's ocean impact shader.
use bevy::{prelude::*, render::render_resource::ShaderType};
use std::collections::VecDeque;
#[derive(Clone, Copy, Debug, Reflect, ShaderType)]
pub struct ImpactUniform {
    pub count: Vec4,
    pub centres: [Vec4; 32],
    pub shapes: [Vec4; 32],
}
impl Default for ImpactUniform {
    fn default() -> Self {
        Self {
            count: Vec4::ZERO,
            centres: [Vec4::ZERO; 32],
            shapes: [Vec4::ZERO; 32],
        }
    }
}
struct Stamp {
    centre: Vec3,
    age: f32,
    strength: f32,
}
#[derive(Resource, Default)]
pub struct WaterImpacts {
    stamps: VecDeque<Stamp>,
    last: Option<Vec3>,
    key: Option<(u64, usize)>,
}
impl WaterImpacts {
    pub fn stamp(&mut self, point: Vec3, strength: f32) {
        let Some(centre) = point.try_normalize() else {
            return;
        };
        if self.stamps.len() == 32 {
            self.stamps.pop_front();
        }
        self.stamps.push_back(Stamp {
            centre,
            age: 0.0,
            strength: strength.clamp(0.0, 1.0),
        });
    }
    fn step(&mut self, delta: f32) {
        for stamp in &mut self.stamps {
            stamp.age += delta;
        }
        self.stamps.retain(|s| s.age < 3.0);
    }
    fn uniform(&self) -> ImpactUniform {
        let mut data = ImpactUniform::default();
        data.count.x = f32::from(u16::try_from(self.stamps.len()).unwrap());
        for (i, stamp) in self.stamps.iter().enumerate() {
            data.centres[i] = stamp.centre.extend(stamp.age * 1.4);
            data.shapes[i] = Vec4::new(
                (1.0 - stamp.age / 3.0).powi(2) * stamp.strength,
                0.22,
                0.14,
                0.0,
            );
        }
        data
    }
}
#[allow(clippy::cast_possible_truncation)]
pub fn update(
    time: Res<Time>,
    scene: Res<crate::Scene>,
    ocean: Res<crate::ocean::OceanSettings>,
    explorer: Res<crate::explore::Explorer>,
    mut impacts: ResMut<WaterImpacts>,
    mut materials: ResMut<Assets<crate::ocean::OceanMaterial>>,
) {
    let key = Some((scene.seed, scene.terrain.elevations.as_ptr() as usize));
    if impacts.key != key {
        impacts.stamps.clear();
        impacts.last = None;
        impacts.key = key;
    }
    if !ocean.paused {
        impacts.step(time.delta_secs() * ocean.speed);
        let contact = (!scene.no_ocean)
            .then(|| explorer.sea_contact(scene.terrain.topology.radius as f32))
            .flatten();
        if let Some(point) = contact {
            if impacts.last.is_none_or(|last| {
                last.distance(point) * scene.terrain.topology.radius as f32 > 0.75
            }) {
                impacts.stamp(point, 0.9);
                impacts.last = Some(point);
            }
        } else {
            impacts.last = None;
        }
    }
    let uniform = impacts.uniform();
    for (_, material) in materials.iter_mut() {
        material.extension.impacts = uniform;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contacts_are_bounded_normalized_and_expire() {
        let mut impacts = WaterImpacts::default();
        impacts.stamp(Vec3::ZERO, 1.0);
        assert!(impacts.stamps.is_empty());
        for _ in 0..40 {
            impacts.stamp(Vec3::Y * 2.0, 1.0);
        }
        assert_eq!(impacts.stamps.len(), 32);
        assert!((impacts.uniform().centres[0].truncate().length() - 1.0).abs() < f32::EPSILON);
        impacts.step(1.0);
        assert!(impacts.uniform().shapes[0].x < 1.0);
        impacts.step(3.0);
        assert!(impacts.stamps.is_empty());
    }
}
