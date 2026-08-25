//! GPU compute implementation of terrain generation stages.
//!
//! This named facade is the only module the generation pipeline sees. The
//! implementation and shaders live under `gpu_generation/`, while the CPU
//! implementation remains in the established terrain modules.

mod erosion;
mod rivers;
mod rocks;

pub(super) use erosion::{GpuParticleErosionScratch, erode_particle_batches_gpu};
pub(super) use rivers::generate_gpu_rivers;
pub(super) use rocks::simulate_rock_bodies_gpu;
