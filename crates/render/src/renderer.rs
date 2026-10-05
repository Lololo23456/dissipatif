//! Draws the voxel scene: owns the render pipeline, the frame data (camera, atmosphere),
//! the volumes (terrain, parcels) and the depth buffer.

use sim::grid::Field3;
use wgpu::util::DeviceExt;

use crate::camera::{CameraUniform, OrbitCamera};
use crate::gpu::Gpu;
use crate::mesh::{MeshData, Vertex};
use crate::palette::{self, AtmosphereUniform};
use crate::volume::{Volume, VolumeStyle};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Handle to a volume added with `Renderer::add_volume`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeId(usize);

pub struct Renderer {
    gpu: Gpu,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    /// Group 0: camera and atmosphere.
    frame_bind_group: wgpu::BindGroup,
    /// Background, the colour of the haze (linear RGB).
    clear_color: wgpu::Color,
    /// Group 1: one bind group per volume, all with this layout.
    volume_layout: wgpu::BindGroupLayout,
    volumes: Vec<Volume>,
    depth_view: wgpu::TextureView,
}

impl Renderer {
    pub fn new(gpu: Gpu) -> Self {
        let device = gpu.device();

        // Uniform buffer: small, read-only data shared by every shader invocation of a draw.
        // COPY_DST lets us overwrite it from the CPU each frame with `queue.write_buffer`.
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<CameraUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // The atmosphere does not change during the game (for now): written once at creation.
        let atmosphere = palette::golden_hour();
        let atmosphere_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("atmosphere"),
            contents: bytemuck::bytes_of(&AtmosphereUniform::new(&atmosphere)),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let [r, g, b] = atmosphere.fog_color.map(f64::from);
        let clear_color = wgpu::Color { r, g, b, a: 1.0 };

        // Bind group layout: the "signature" of a group of resources, i.e. what the shader
        // expects at each `@binding` of a `@group`. Group 0: two uniform buffers, the camera
        // (binding 0) and the atmosphere (binding 1).
        let uniform_entry = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame layout"),
            entries: &[
                // The fragment shader also reads the eye position, for the haze.
                uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                uniform_entry(1, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        // Bind group: the actual resources plugged into that signature.
        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &frame_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: atmosphere_buffer.as_entire_binding(),
                },
            ],
        });

        // Group 1: a volume. Two 3D textures of f32 (base and life) read with `textureLoad`
        // (exact texels, hence `filterable: false` and no sampler), and the volume uniform
        // (origin, palettes).
        let field_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D3,
                multisampled: false,
            },
            count: None,
        };
        let volume_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("volume layout"),
            entries: &[
                field_entry(0),
                // The vertex shader reads the origin, the fragment shader the colours.
                uniform_entry(1, wgpu::ShaderStages::VERTEX_FRAGMENT),
                field_entry(2),
            ],
        });
        // Pipeline layout: the list of bind group layouts, one per `@group` index.
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("voxel layout"),
            bind_group_layouts: &[Some(&frame_layout), Some(&volume_layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("../shaders/voxel.wgsl"));

        // Render pipeline: the whole fixed configuration of a draw, compiled once.
        // Shaders, vertex format, triangle assembly, depth test, output format.
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(Vertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                // Faces seen from behind are never visible on closed voxels: skip them.
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            // Depth test: each pixel keeps the closest surface, whatever the drawing order.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.surface_format(),
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let (width, height) = gpu.size();
        let depth_view = create_depth_view(device, width, height);

        Self {
            gpu,
            pipeline,
            camera_buffer,
            frame_bind_group,
            clear_color,
            volume_layout,
            volumes: Vec::new(),
            depth_view,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.gpu.resize(width, height);
        self.depth_view = create_depth_view(self.gpu.device(), width, height);
    }

    /// Adds a volume to the scene. Nothing is drawn for it until it has a mesh and a field.
    pub fn add_volume(&mut self, style: &VolumeStyle) -> VolumeId {
        self.volumes.push(Volume::new(self.gpu.device(), style));
        VolumeId(self.volumes.len() - 1)
    }

    /// Changes where a volume sits or how it is coloured.
    pub fn set_volume_style(&mut self, id: VolumeId, style: &VolumeStyle) {
        self.volumes[id.0].set_style(self.gpu.queue(), style);
    }

    /// Replaces the mesh of a volume. Buffers are reused, so this can run every frame.
    pub fn upload_mesh(&mut self, id: VolumeId, data: &MeshData) {
        let (device, queue) = (self.gpu.device(), self.gpu.queue());
        self.volumes[id.0].upload_mesh(device, queue, data);
    }

    /// Copies the base field of a volume (its inert colour) to the GPU. Must be called once
    /// before `upload_life`.
    pub fn upload_base(&mut self, id: VolumeId, field: &Field3) {
        let (device, queue) = (self.gpu.device(), self.gpu.queue());
        self.volumes[id.0].upload_base(device, queue, &self.volume_layout, field);
    }

    /// Copies the life field of a volume to the GPU. Same dimensions as the base field.
    /// Cheap enough to call every simulation step.
    pub fn upload_life(&mut self, id: VolumeId, field: &Field3) {
        self.volumes[id.0].upload_life(self.gpu.queue(), field);
    }

    /// Width / height of the window images.
    pub fn aspect(&self) -> f32 {
        let (width, height) = self.gpu.size();
        width as f32 / height as f32
    }

    pub fn render(&mut self, camera: &OrbitCamera) {
        let Some(frame) = self.gpu.acquire_frame() else {
            return;
        };
        let uniform = CameraUniform::new(camera, self.aspect());
        self.gpu
            .queue()
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));

        // Commands are recorded in an encoder, then submitted to the queue in one go.
        let mut encoder =
            self.gpu
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame"),
                });
        {
            // A render pass = a series of draws into the same target images.
            // `LoadOp::Clear` fills the image before drawing, `StoreOp::Store` keeps the result.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    // 1.0 = farthest possible depth: anything drawn is closer.
                    // Depth is only needed during the pass, no need to keep it.
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Group 0 is set once; only group 1 and the buffers change between volumes.
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.frame_bind_group, &[]);
            for volume in &self.volumes {
                if let (Some(mesh), Some(fields)) = (&volume.mesh, &volume.fields)
                    && mesh.index_count > 0
                {
                    pass.set_bind_group(1, &fields.bind_group, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                }
            }
        }
        self.gpu.queue().submit([encoder.finish()]);
        self.gpu.present(frame);
    }
}

/// Depth buffer: one depth value per pixel, same size as the window images.
fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
