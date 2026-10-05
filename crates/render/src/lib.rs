//! Rendu wgpu : pipelines, buffers, maillage voxel, particules.
//! Le rendu lit l'état de la simulation, il ne le modifie jamais.
//! Shaders WGSL dans `crates/render/shaders/`.

pub mod camera;
pub mod field_texture;
pub mod gpu;
pub mod mesh;
pub mod mesher;
pub mod models;
pub mod palette;
pub mod particles;
pub mod png;
pub mod renderer;
pub mod volume;
pub mod water_mesher;

pub use camera::OrbitCamera;
pub use gpu::Gpu;
pub use models::ModelInstance;
pub use particles::ParticleInstance;
pub use renderer::{ModelId, Renderer, VolumeId};
pub use volume::{LifeStyle, VolumeStyle};
