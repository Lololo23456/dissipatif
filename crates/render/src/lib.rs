//! Rendu wgpu : pipelines, buffers, maillage voxel, particules.
//! Le rendu lit l'état de la simulation, il ne le modifie jamais.
//! Shaders WGSL dans `crates/render/shaders/`.

pub mod camera;
pub mod field_texture;
pub mod gpu;
pub mod marks;
pub mod mesh;
pub mod mesher;
pub mod models;
pub mod palette;
pub mod particles;
pub mod png;
pub mod post;
pub mod renderer;
pub mod sky;
pub mod ui;
pub mod volume;
pub mod water_mesher;

pub use camera::OrbitCamera;
pub use gpu::Gpu;
pub use models::{ModelInstance, PartInstance};
pub use particles::ParticleInstance;
pub use renderer::{ModelId, PartId, Renderer, VolumeId};
pub use volume::{LifeStyle, VolumeStyle};
