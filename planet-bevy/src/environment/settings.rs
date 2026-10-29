//! Shared metre-based atmosphere/weather contract; renderers borrow persistent GPU state.
use bevy::{
    prelude::*,
    render::storage::ShaderBuffer,
    render::{extract_resource::ExtractResource, render_resource::ShaderType},
};

#[allow(clippy::struct_excessive_bools)] // Independent atmosphere, cloud, placement and animation switches.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentSettings {
    pub atmosphere: bool,
    pub clouds: bool,
    pub clear_terrain: bool,
    pub temporal: bool,
    pub atmosphere_height: f32,
    pub haze: f32,
    pub coverage: f32,
    pub density: f32,
    pub cloud_base: f32,
    pub cloud_thickness: f32,
    pub cloud_scale: f32,
    pub wind_speed: f32,
    pub shadow_strength: f32,
    pub quality: u32,
    pub sun_azimuth: f32,
    pub sun_elevation: f32,
    pub paused: bool,
}
impl Default for EnvironmentSettings {
    fn default() -> Self {
        Self {
            atmosphere: true,
            clouds: true,
            clear_terrain: true,
            temporal: true,
            atmosphere_height: 200.0,
            haze: 1.0,
            coverage: 0.42,
            density: 1.2,
            cloud_base: 45.0,
            cloud_thickness: 55.0,
            cloud_scale: 90.0,
            wind_speed: 1.5,
            shadow_strength: 0.6,
            quality: 1,
            sun_azimuth: -30.963_757,
            sun_elevation: 34.449_9,
            paused: false,
        }
    }
}
impl EnvironmentSettings {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, low, high) in [
            ("atmosphere height", self.atmosphere_height, 20.0, 2000.0),
            ("haze", self.haze, 0.0, 5.0),
            ("cloud coverage", self.coverage, 0.0, 1.0),
            ("cloud density", self.density, 0.0, 8.0),
            ("cloud base", self.cloud_base, 1.0, 1500.0),
            ("cloud thickness", self.cloud_thickness, 5.0, 1000.0),
            ("cloud scale", self.cloud_scale, 10.0, 2000.0),
            ("wind speed", self.wind_speed, 0.0, 40.0),
            ("cloud shadow", self.shadow_strength, 0.0, 1.0),
            ("sun azimuth", self.sun_azimuth, -180.0, 180.0),
            ("sun elevation", self.sun_elevation, -90.0, 90.0),
        ] {
            if !value.is_finite() || !(low..=high).contains(&value) {
                return Err(format!("{name} must be {low}..{high}"));
            }
        }
        if self.quality > 2 {
            return Err("cloud quality must be 0..2".into());
        }
        Ok(())
    }
    pub fn sun_direction(&self) -> Vec3 {
        let a = self.sun_azimuth.to_radians();
        let e = self.sun_elevation.to_radians();
        Vec3::new(a.sin() * e.cos(), e.sin(), a.cos() * e.cos())
    }
}

#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct EnvironmentUniform {
    /// radius metres, conservative maximum base metres, thickness metres, noise scale metres.
    pub shape: Vec4,
    /// coverage, density, shadow strength, minimum cloud base metres (zero disables clouds).
    pub weather: Vec4,
    /// Inverse wind rotation quaternion, used to sample an anchored spherical weather field.
    pub wind: Vec4,
    /// World direction toward sun xyz, direct-light illuminance in lux w.
    pub sun: Vec4,
    /// Linear sun colour rgb, atmosphere enabled (0/1) w.
    pub light: Vec4,
    /// atmosphere height metres, haze, march samples, elapsed weather seconds.
    pub air: Vec4,
    /// Normalized render-space camera xyz, seed-derived spatial offset w.
    pub viewer: Vec4,
    /// Zero disables simulation; otherwise x=1+snapshot blend. Then simulation
    /// seconds, effective weather radius metres, and diagnostic mode.
    pub simulation: Vec4,
}
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct EnvironmentFrame {
    pub uniform: EnvironmentUniform,
    pub noise: Handle<Image>,
    pub weather: Handle<Image>,
    pub quality: u32,
    pub temporal: bool,
    pub enabled: bool,
    pub revision: u64,
}
#[derive(Resource, Default)]
pub struct EnvironmentAssets {
    pub uniform: Handle<ShaderBuffer>,
    pub cloud_noise: Handle<Image>,
}

/// Conservative global bounds; simulated clouds use the local terrain ceiling
/// within these bounds. Sliders remain requested minimum heights in metres.
pub fn effective_layers(settings: &EnvironmentSettings, maximum_terrain_height: f32) -> (f32, f32) {
    let base = if settings.clear_terrain {
        settings.cloud_base.max(maximum_terrain_height + 20.0)
    } else {
        settings.cloud_base
    };
    let top = base + settings.cloud_thickness * 2.5;
    (base, settings.atmosphere_height.max(top + 20.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_are_bounded_and_clouds_clear_high_terrain() {
        let mut s = EnvironmentSettings::default();
        assert!(s.validate().is_ok());
        let (base, top) = effective_layers(&s, 300.0);
        assert!(base >= 320.0 && top >= base + s.cloud_thickness + 20.0);
        assert!((s.sun_direction().length() - 1.0).abs() < 1.0e-6);
        s.coverage = f32::NAN;
        assert!(s.validate().is_err());
        s = EnvironmentSettings::default();
        s.quality = 3;
        assert!(s.validate().is_err());
    }
}
