//! A simulation field copied into a 3D texture, so shaders can read the value of any cell.

use sim::grid::{Dims, Field3};

/// One `f32` per texel. Not filterable without an optional feature: we read exact cells with
/// `textureLoad`, so no sampler and no filtering are needed.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

pub struct FieldTexture {
    texture: wgpu::Texture,
    /// How shaders see the texture (here: the whole texture, as a 3D texture).
    pub view: wgpu::TextureView,
    dims: Dims,
}

impl FieldTexture {
    pub fn new(device: &wgpu::Device, dims: Dims) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("field"),
            size: extent(dims),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: FORMAT,
            // Read by shaders, written from the CPU.
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            dims,
        }
    }

    pub fn dims(&self) -> Dims {
        self.dims
    }

    /// Copies the field into the texture. Its dimensions must match the texture's.
    pub fn upload(&self, queue: &wgpu::Queue, field: &Field3) {
        assert_eq!(field.dims, self.dims, "field and texture sizes differ");
        // `Field3` stores x fastest, then y, then z: exactly the texel order of a 3D texture,
        // so the whole grid is copied in one call, without any reordering.
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&field.data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.dims.nx as u32 * 4),
                rows_per_image: Some(self.dims.ny as u32),
            },
            extent(self.dims),
        );
    }

    /// Copies `region`, a packed box of texels, into the texture with its first texel at `at`
    /// (x, y, z): only what changed is sent (a cell of the ground, the marsh's grid in its
    /// corner), not the whole texture. The box must fit inside the texture.
    pub fn upload_region(&self, queue: &wgpu::Queue, at: [usize; 3], region: &Field3) {
        let d = region.dims;
        assert!(
            at[0] + d.nx <= self.dims.nx
                && at[1] + d.ny <= self.dims.ny
                && at[2] + d.nz <= self.dims.nz,
            "region {d:?} at {at:?} overruns the texture {:?}",
            self.dims
        );
        if d.is_empty() {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: at[0] as u32,
                    y: at[1] as u32,
                    z: at[2] as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&region.data),
            // Rows of the box are packed: no padding between them (unlike buffer copies,
            // `write_texture` asks for no alignment of rows).
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(d.nx as u32 * 4),
                rows_per_image: Some(d.ny as u32),
            },
            extent(d),
        );
    }
}

fn extent(dims: Dims) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: dims.nx as u32,
        height: dims.ny as u32,
        depth_or_array_layers: dims.nz as u32,
    }
}
