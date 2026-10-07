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
///     base_range: vec4<f32>,            // offset 176: min, max, materials (1 = yes), unused
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
    /// The base field holds materials instead of a continuous value: `id + variation`, the
    /// integer part picks the colour in the material table (`palette::materials`), the
    /// fractional part in [0, 1) slightly brightens or darkens it. `palette` and
    /// `value_range` are then unused.
    pub materials: bool,
    pub life: LifeStyle,
    /// Drawn after opaque volumes, blended over them (water). Fixed when the volume is added.
    pub transparent: bool,
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
            base_range: [
                base_min,
                base_max,
                if style.materials { 1.0 } else { 0.0 },
                0.0,
            ],
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
    pub(crate) transparent: bool,
    uniform_buffer: wgpu::Buffer,
    /// `None` until the base field is uploaded.
    pub(crate) fields: Option<VolumeFields>,
    /// The mesh, in parts (chunks of a large volume, remeshed separately). Empty until a mesh
    /// is uploaded; a part is `None` until its first upload.
    pub(crate) meshes: Vec<Option<GpuMesh>>,
}

impl Volume {
    pub(crate) fn new(device: &wgpu::Device, style: &VolumeStyle) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("volume"),
            contents: bytemuck::bytes_of(&VolumeUniform::new(style)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            transparent: style.transparent,
            uniform_buffer,
            fields: None,
            meshes: Vec::new(),
        }
    }

    pub(crate) fn set_style(&self, queue: &wgpu::Queue, style: &VolumeStyle) {
        let uniform = VolumeUniform::new(style);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Replaces part `part` of the mesh (part 0 for a volume in one piece).
    pub(crate) fn upload_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        part: usize,
        data: &MeshData,
    ) {
        if self.meshes.len() <= part {
            self.meshes.resize_with(part + 1, || None);
        }
        match &mut self.meshes[part] {
            Some(mesh) => mesh.update(device, queue, data),
            slot => *slot = Some(GpuMesh::new(device, queue, data)),
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

    /// Copies `region`, a packed box of the base field, at `at`: a few cells changed (the
    /// ground worn or grown over, a piece built). The base field must have been uploaded.
    pub(crate) fn upload_base_region(&self, queue: &wgpu::Queue, at: [usize; 3], region: &Field3) {
        let fields = self
            .fields
            .as_ref()
            .expect("upload the base field before a region of it");
        fields.base.upload_region(queue, at, region);
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
