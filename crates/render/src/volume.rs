//! A volume: a voxel grid placed in the world, with its mesh and two fields that colour it.
//!
//! - The **base** field gives the colour of the inert matter (e.g. earth strata).
//! - The **life** field gives the colour of what lives in it (e.g. a reaction's activator).
//!
//! The shader blends them: where life is absent the base shows, where it is strong the living
//! colour takes over. The mesh only depends on the shape, so a living but static terrain is
//! meshed once and only its life field is uploaded each frame.

use sim::grid::{Dims, Field3};
use wgpu::util::DeviceExt;

use crate::field_texture::FieldTexture;
use crate::mesh::{GpuMesh, MeshData};
use crate::palette::{Palette, STOP_COUNT};

/// Per-volume data read by the shaders.
///
/// WGSL side (`shaders/voxel.wgsl`):
/// ```wgsl
/// struct Volume {
///     origin: vec4<f32>,                // offset 0: world position of cell (0,0,0), w unused
///     base_stops: array<vec4<f32>, 5>,  // offset 16, size 80 (rgb + unused w)
///     life_stops: array<vec4<f32>, 5>,  // offset 96, size 80
///     base_range: vec4<f32>,            // offset 176: min, max, unused, unused
///     life_range: vec4<f32>,            // offset 192: fade start, fade end, colour min, colour max
/// }                                     // size 208
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VolumeUniform {
    /// `vec3` would be padded to 16 bytes anyway: use `vec4` and ignore `w`.
    pub origin: [f32; 4],
    pub base_stops: [[f32; 4]; STOP_COUNT],
    pub life_stops: [[f32; 4]; STOP_COUNT],
    pub base_range: [f32; 4],
    pub life_range: [f32; 4],
}

/// How the life field shows over the base.
#[derive(Clone, Copy, Debug)]
pub struct LifeStyle {
    pub palette: Palette,
    /// Life value where the base starts to be tinted, and where it is fully covered.
    pub fade: (f32, f32),
    /// Life values mapped to the first and last colours of the palette.
    pub value_range: (f32, f32),
}

/// How a volume looks: where it sits and how its fields map to colours.
#[derive(Clone, Copy, Debug)]
pub struct VolumeStyle {
    /// World position of the minimum corner of cell (0, 0, 0).
    pub origin: [f32; 3],
    pub palette: Palette,
    /// Base values mapped to the first and last colours. Values outside get the end colours.
    pub value_range: (f32, f32),
    pub life: LifeStyle,
}

impl VolumeUniform {
    pub fn new(style: &VolumeStyle) -> Self {
        let (base_min, base_max) = style.value_range;
        let (fade_start, fade_end) = style.life.fade;
        let (life_min, life_max) = style.life.value_range;
        assert!(base_min < base_max, "empty base colour range");
        assert!(fade_start < fade_end, "empty life fade");
        assert!(life_min < life_max, "empty life colour range");
        let [x, y, z] = style.origin;
        let stops = |palette: &Palette| palette.map(|[r, g, b]| [r, g, b, 1.0]);
        Self {
            origin: [x, y, z, 0.0],
            base_stops: stops(&style.palette),
            life_stops: stops(&style.life.palette),
            base_range: [base_min, base_max, 0.0, 0.0],
            life_range: [fade_start, fade_end, life_min, life_max],
        }
    }
}

/// The two field textures of a volume and the bind group pointing at them.
pub(crate) struct VolumeFields {
    base: FieldTexture,
    life: FieldTexture,
    pub(crate) bind_group: wgpu::BindGroup,
}

/// GPU side of a volume.
pub(crate) struct Volume {
    uniform_buffer: wgpu::Buffer,
    /// `None` until the base field is uploaded.
    pub(crate) fields: Option<VolumeFields>,
    /// `None` until a mesh is uploaded.
    pub(crate) mesh: Option<GpuMesh>,
}

impl Volume {
    pub(crate) fn new(device: &wgpu::Device, style: &VolumeStyle) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("volume"),
            contents: bytemuck::bytes_of(&VolumeUniform::new(style)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            uniform_buffer,
            fields: None,
            mesh: None,
        }
    }

    pub(crate) fn set_style(&self, queue: &wgpu::Queue, style: &VolumeStyle) {
        let uniform = VolumeUniform::new(style);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    pub(crate) fn upload_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &MeshData,
    ) {
        match &mut self.mesh {
            Some(mesh) => mesh.update(device, queue, data),
            None => self.mesh = Some(GpuMesh::new(device, queue, data)),
        }
    }

    /// Copies the base field. The first time (or if the grid size changes) both textures are
    /// created; the life texture starts at zero (wgpu zero-initialises textures): no life.
    pub(crate) fn upload_base(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        field: &Field3,
    ) {
        if self
            .fields
            .as_ref()
            .is_none_or(|fields| fields.base.dims() != field.dims)
        {
            self.fields = Some(self.create_fields(device, layout, field.dims));
        }
        if let Some(fields) = &self.fields {
            fields.base.upload(queue, field);
        }
    }

    /// Copies the life field. Must have the dimensions of the base field, uploaded before.
    pub(crate) fn upload_life(&self, queue: &wgpu::Queue, field: &Field3) {
        let fields = self
            .fields
            .as_ref()
            .expect("upload the base field before the life field");
        fields.life.upload(queue, field);
    }

    fn create_fields(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        dims: Dims,
    ) -> VolumeFields {
        let base = FieldTexture::new(device, dims);
        let life = FieldTexture::new(device, dims);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("volume"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&base.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&life.view),
                },
            ],
        });
        VolumeFields {
            base,
            life,
            bind_group,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<VolumeUniform>(), 208);
        assert_eq!(std::mem::offset_of!(VolumeUniform, base_stops), 16);
        assert_eq!(std::mem::offset_of!(VolumeUniform, life_stops), 96);
        assert_eq!(std::mem::offset_of!(VolumeUniform, base_range), 176);
        assert_eq!(std::mem::offset_of!(VolumeUniform, life_range), 192);
    }
}
