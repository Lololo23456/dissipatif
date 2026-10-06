//! Particles drawn as small cubes by instancing: the cube is sent to the GPU once, and a list
//! of instances (centre, size, colour) says where to draw a copy of it. One draw call draws
//! them all.
//!
//! The particles themselves (spawning, motion) live in the game; this module only draws them.

use crate::mesh::FACE_NORMALS;

/// One particle as seen by the GPU.
///
/// WGSL side (`shaders/voxel.wgsl`, `ParticleInput`, per-instance attributes):
/// ```wgsl
/// @location(2) centre_size: vec4<f32>,  // offset 0: centre xyz, edge length w
/// @location(3) color: vec4<f32>,        // offset 16: linear rgb, a = glow
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleInstance {
    pub centre_size: [f32; 4],
    /// Linear colour (rgb), and how much the particle glows when the sun catches it (a: 0 a
    /// leaf, about 1 a speck of dust or pollen lit against the light; negative: shines by
    /// itself with strength −a, whatever the light, like a firefly).
    pub color: [f32; 4],
}

impl ParticleInstance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4];

    /// `step_mode: Instance`: the GPU moves to the next element once per cube drawn, not once
    /// per vertex.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ParticleInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// A vertex of the shared unit cube.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CubeVertex {
    /// In [-0.5, 0.5]³.
    pub corner: [f32; 3],
    pub normal: [f32; 3],
}

impl CubeVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CubeVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// The 36 vertices (12 triangles, no index buffer) of a unit cube centred on the origin,
/// counter-clockwise seen from outside.
pub fn unit_cube() -> Vec<CubeVertex> {
    let mut mesh = crate::mesh::MeshData::default();
    for normal in FACE_NORMALS {
        mesh.push_face([0, 0, 0], normal, [1.0; 4]);
    }
    mesh.indices
        .iter()
        .map(|&i| {
            let v = mesh.vertices[i as usize];
            CubeVertex {
                corner: v.position.map(|c| c - 0.5),
                normal: v.normal,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<ParticleInstance>(), 32);
        assert_eq!(std::mem::offset_of!(ParticleInstance, color), 16);
    }

    #[test]
    fn unit_cube_is_centred() {
        let cube = unit_cube();
        assert_eq!(cube.len(), 36);
        assert!(
            cube.iter()
                .all(|v| v.corner.iter().all(|&c| c == -0.5 || c == 0.5))
        );
    }
}
