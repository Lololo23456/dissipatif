//! Post-processing. The scene is no longer drawn straight to the screen: it is drawn into an
//! offscreen image (the "scene image"), then a last full-screen pass reads that image and
//! writes the final one, reworked (`shaders/post.wgsl`: tilt-shift, bloom, colour grading,
//! vignette, grain).
//!
//! The scene image is HDR (`Rgba16Float`): 16-bit floats per channel, so lights brighter than
//! 1 (sun glints) survive until grading instead of being clipped, and gradients do not band.

use bytemuck::{Pod, Zeroable};

use crate::camera::OrbitCamera;
use crate::sky::Grade;

/// Format of the scene image the main pass draws into.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Must match `struct Post` in post.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PostUniform {
    /// x: seconds since start (grain, dust). y, z: the camera target along the screen's right
    /// and up directions on the ground, in cells (parallax of the foreground dust). w: the
    /// camera's distance to its target, in cells.
    pub time: [f32; 4],
    /// Split toning: tint of the darks (rgb) and saturation (w)…
    pub shadow_tint: [f32; 4],
    /// …tint of the lights (rgb) and exposure (w).
    pub light_tint: [f32; 4],
    /// x: darkness of the night, in [0, 1] (dims the foreground dust). y: weariness of the
    /// naturalist, in [0, 1] (darker edges, duller colours). z: how much the tilt-shift blurs,
    /// in [0, 1] (a miniature is seen from above, not from a person's height). w: height of
    /// the view per unit of distance, 2·tan(fov / 2) (parallax of the dust).
    pub night: [f32; 4],
}

pub struct Post {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    scene_view: wgpu::TextureView,
    /// Ties the scene image, its sampler and the uniform to the shader. Rebuilt with the
    /// scene image when the window is resized.
    bind_group: wgpu::BindGroup,
    grade: Grade,
    night: f32,
    weariness: f32,
}

impl Post {
    /// `output_format`: format of the final images (the window's, or the capture's).
    pub fn new(
        device: &wgpu::Device,
        output_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("../shaders/post.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        // No vertex buffer, no depth: the vertex shader makes one screen-covering triangle.
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_post"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // Linear filtering: blur taps between pixels blend their neighbours. Clamped at the
        // edges, so blur near a border does not wrap to the other side.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post uniform"),
            size: std::mem::size_of::<PostUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_view = create_scene_view(device, width, height);
        let bind_group = create_bind_group(device, &layout, &scene_view, &sampler, &uniform);
        Self {
            pipeline,
            layout,
            sampler,
            uniform,
            scene_view,
            bind_group,
            grade: crate::sky::sky(17.0).grade,
            night: 0.0,
            weariness: 0.0,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.scene_view = create_scene_view(device, width, height);
        self.bind_group = create_bind_group(
            device,
            &self.layout,
            &self.scene_view,
            &self.sampler,
            &self.uniform,
        );
    }

    pub fn set_grade(&mut self, grade: &Grade, night: f32) {
        self.grade = *grade;
        self.night = night;
    }

    pub fn set_weariness(&mut self, weariness: f32) {
        self.weariness = weariness;
    }

    /// Where the main pass draws the scene.
    pub fn scene_view(&self) -> &wgpu::TextureView {
        &self.scene_view
    }

    /// Records the post pass: reads the scene image, writes `target`.
    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        time: f32,
        camera: &OrbitCamera,
    ) {
        // Screen axes on the ground: the camera looks along −(sin yaw, cos yaw).
        let (sin_yaw, cos_yaw) = camera.yaw.sin_cos();
        let (x, z) = (camera.target.x, camera.target.z);
        let right = x * cos_yaw - z * sin_yaw;
        let up = -x * sin_yaw - z * cos_yaw;
        let g = &self.grade;
        let [sr, sg, sb] = g.shadow_tint;
        let [lr, lg, lb] = g.light_tint;
        let uniform = PostUniform {
            time: [time, right, up, camera.distance],
            shadow_tint: [sr, sg, sb, g.saturation],
            light_tint: [lr, lg, lb, g.exposure],
            night: [
                self.night,
                self.weariness,
                crate::sky::smoothstep(15.0_f32.to_radians(), 35.0_f32.to_radians(), camera.pitch),
                2.0 * (camera.fov_y / 2.0).tan(),
            ],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("post pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Every pixel is overwritten: nothing to load.
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// The scene image: drawn into by the main pass (RENDER_ATTACHMENT), then read by the post
/// pass (TEXTURE_BINDING); copied back to the CPU now and then for a sketch (COPY_SRC).
fn create_scene_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene image"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SCENE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    scene_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    uniform: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("post bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(scene_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform.as_entire_binding(),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_matches_the_shader_size() {
        assert_eq!(std::mem::size_of::<PostUniform>(), 64);
    }
}
