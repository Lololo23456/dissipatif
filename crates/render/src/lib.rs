//! Rendu wgpu : pipelines, buffers, maillage voxel, particules.
//! Le rendu lit l'état de la simulation, il ne le modifie jamais.
//! Shaders WGSL dans `crates/render/shaders/`.

pub mod camera;
pub mod field_texture;
pub mod gpu;
pub mod mesh;
pub mod mesher;
pub mod palette;
pub mod renderer;
pub mod volume;

pub use camera::OrbitCamera;
pub use gpu::Gpu;
pub use renderer::{Renderer, VolumeId};
pub use volume::{LifeStyle, VolumeStyle};
