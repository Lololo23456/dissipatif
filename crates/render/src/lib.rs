//! Rendu wgpu : pipelines, buffers, maillage voxel, particules.
//! Le rendu lit l'état de la simulation, il ne le modifie jamais.
//! Shaders WGSL dans `crates/render/shaders/`.

pub mod palette;
