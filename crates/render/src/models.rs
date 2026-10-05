//! Models: one mesh drawn at many places by instancing (plants). The mesh carries its
//! materials in its vertices (`mesh::pack_material`); each instance says where it stands and
//! how it is turned.

use crate::mesh::{GpuMesh, MeshData};

/// One placement of a model, as seen by the GPU.
///
/// WGSL side (`shaders/voxel.wgsl`, `ModelInstance`, per-instance attribute):
/// ```wgsl
/// @location(4) position_turns: vec4<f32>,  // offset 0: position xyz, quarter turns w
/// @location(5) scale_mirror: vec4<f32>,    // offset 16: size, mirrored (1) or not (0), unused ×2
/// @location(6) tint: vec4<f32>,            // offset 32: foliage colour multiplier rgb, flexibility
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelInstance {
    /// World position of the model's origin (xyz) and quarter turns around the vertical axis
    /// (w, 0 to 3).
    pub position_turns: [f32; 4],
    /// Size (x), mirrored before turning (y: 1 or 0).
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
            instances: create_instance_buffer(device, 1),
            instance_count: 0,
        }
    }

    pub(crate) fn set_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[ModelInstance],
    ) {
        if std::mem::size_of_val(instances) as u64 > self.instances.size() {
            self.instances = create_instance_buffer(device, instances.len());
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
        self.instance_count = instances.len() as u32;
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model instances"),
        size: (capacity.max(1) * std::mem::size_of::<ModelInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<ModelInstance>(), 48);
        let instance = ModelInstance::new([1.0, 2.0, 3.0], 5, true, 1.1, [0.9, 1.0, 1.2], 0.5);
        assert_eq!(instance.position_turns, [1.0, 2.0, 3.0, 1.0]);
        assert_eq!(instance.scale_mirror, [1.1, 1.0, 0.0, 0.0]);
        assert_eq!(std::mem::offset_of!(ModelInstance, tint), 32);
    }
}
