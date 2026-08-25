//! GPU compute implementation of selected terrain generation stages.
//!
//! This named facade is the only module the generation pipeline sees. The
//! implementation and shaders live under `gpu_generation/`, while the CPU
//! implementation remains in the established terrain modules. Production GPU
//! generation accelerates hydraulic erosion and settled-rock simulation only;
//! rivers and waterfalls always use the established CPU implementation.

mod erosion;
mod rocks;

pub(super) use erosion::{GpuParticleErosionScratch, erode_particle_batches_gpu};
pub(super) use rocks::simulate_rock_bodies_gpu;
