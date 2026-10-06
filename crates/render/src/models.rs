//! Models: one mesh drawn at many places by instancing (plants). The mesh carries its
//! materials in its vertices (`mesh::pack_material`); each instance says where it stands and
//! how it is turned.

use crate::mesh::{GpuMesh, MeshData};

/// One placement of a model, as seen by the GPU.
///
/// WGSL side (`shaders/voxel.wgsl`, `ModelInstance`, per-instance attribute):
/// ```wgsl
/// @location(4) position_turns: vec4<f32>,  // offset 0: position xyz, quarter turns w
/// @location(5) scale_mirror: vec4<f32>,    // offset 16: size, mirrored (1/0), bend x, bend z
/// @location(6) tint: vec4<f32>,            // offset 32: foliage colour multiplier rgb, flexibility
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelInstance {
    /// World position of the model's origin (xyz) and quarter turns around the vertical axis
    /// (w, 0 to 3).
    pub position_turns: [f32; 4],
    /// Size (x), mirrored before turning (y: 1 or 0), and how the plant is bent by something
    /// pushing through it (zw: horizontal offset of its top per unit of height; 0, 0 upright).
    pub scale_mirror: [f32; 4],
    /// Colour multiplier of the foliage (rgb): each tree its own shade. Flexibility in the
    /// wind (a): 0 rigid, 1 a broadleaf tree.
    pub tint: [f32; 4],
}

impl ModelInstance {
    pub fn new(
        position: [f32; 3],
        quarter_turns: u32,
        mirrored: bool,
        scale: f32,
        tint: [f32; 3],
        flexibility: f32,
    ) -> Self {
        let [x, y, z] = position;
        let [r, g, b] = tint;
        Self {
            position_turns: [x, y, z, (quarter_turns % 4) as f32],
            scale_mirror: [scale, if mirrored { 1.0 } else { 0.0 }, 0.0, 0.0],
            tint: [r, g, b, flexibility],
        }
    }

    /// Bends the plant: its top moves by `bend` (x, z) per unit of height.
    pub fn set_bend(&mut self, bend: [f32; 2]) {
        self.scale_mirror[2] = bend[0];
        self.scale_mirror[3] = bend[1];
    }

    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4];

    /// Read once per drawn copy of the mesh (`step_mode: Instance`).
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ModelInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// One placement of an articulated part (a limb of a character): a full transform computed on
/// the CPU, so the part can turn freely and swing around its pivot.
///
/// WGSL side (`shaders/voxel.wgsl`, `PartInstance`, per-instance attributes):
/// ```wgsl
/// @location(4..7) columns of the model → world matrix (offset 0, 16, 32, 48)
/// @location(8) tint: vec4<f32>,  // offset 64: colour multiplier rgb (all materials), unused
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PartInstance {
    /// Model → world matrix, column-major. Rotation and uniform scale only (no shear), so the
    /// same matrix also turns the normals.
    pub transform: [[f32; 4]; 4],
    pub tint: [f32; 4],
}

impl PartInstance {
    pub fn new(transform: glam::Mat4) -> Self {
        Self {
            transform: transform.to_cols_array_2d(),
            tint: [1.0; 4],
        }
    }

    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4
    ];

    /// Read once per drawn copy of the mesh (`step_mode: Instance`).
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PartInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// GPU side of a model: its mesh and where to draw it.
pub(crate) struct GpuModel {
    pub(crate) mesh: GpuMesh,
    pub(crate) instances: wgpu::Buffer,
    pub(crate) instance_count: u32,
}

impl GpuModel {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, mesh: &MeshData) -> Self {
        Self {
            mesh: GpuMesh::new(device, queue, mesh),
            instances: create_instance_buffer(device, 256),
            instance_count: 0,
        }
    }

    /// Replaces the instances (`ModelInstance` for models, `PartInstance` for parts).
    pub(crate) fn set_instances<T: bytemuck::Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[T],
    ) {
        let bytes = std::mem::size_of_val(instances) as u64;
        if bytes > self.instances.size() {
            self.instances = create_instance_buffer(device, bytes);
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
        self.instance_count = instances.len() as u32;
    }
}

fn create_instance_buffer(device: &wgpu::Device, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model instances"),
        size: bytes.next_multiple_of(16).max(16),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_instance_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<PartInstance>(), 80);
        assert_eq!(std::mem::offset_of!(PartInstance, tint), 64);
    }

    #[test]
    fn instance_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<ModelInstance>(), 48);
        let instance = ModelInstance::new([1.0, 2.0, 3.0], 5, true, 1.1, [0.9, 1.0, 1.2], 0.5);
        assert_eq!(instance.position_turns, [1.0, 2.0, 3.0, 1.0]);
        assert_eq!(instance.scale_mirror, [1.1, 1.0, 0.0, 0.0]);
        assert_eq!(std::mem::offset_of!(ModelInstance, tint), 32);
    }
}
