//! Draws the voxel scene: owns the render pipelines, the frame data (camera, atmosphere,
//! materials, shadow map), the volumes (terrain, water…) and the depth buffer.
//!
//! A frame is three passes: the shadow pass draws the opaque volumes seen from the sun, keeping
//! only depth (the shadow map); the main pass draws everything seen from the camera into the
//! HDR scene image, asking the shadow map, for each pixel, whether the sun reaches it; the post
//! pass reworks that image into the final one (see `post`).

use glam::Vec3;
use sim::grid::Field3;
use wgpu::util::DeviceExt;

use crate::camera::{CameraUniform, OrbitCamera, light_view_proj};
use crate::effects::{EffectsUniform, MARSH_FIELD_SIZE};
use crate::field_texture::FieldTexture;
use crate::gpu::Gpu;
use crate::marks::{MarksUniform, Print, Ripple};
use crate::mesh::{MeshData, Vertex};
use crate::models::{GpuModel, ModelInstance, PartInstance};
use crate::palette::{self, AtmosphereUniform, MaterialsUniform};
use crate::particles::{CubeVertex, ParticleInstance, unit_cube};
use crate::post::{Post, SCENE_FORMAT};
use crate::sky::Sky;
use crate::ui::{UiPass, UiVertex};
use crate::volume::{Volume, VolumeStyle};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Side of the shadow map in texels. Must match `SHADOW_TEXEL` in the shader.
const SHADOW_SIZE: u32 = 2048;
/// On a planet, shadows are cast over this many cells around the eye (each way).
const SHADOW_REACH: f32 = 72.0;

/// Whether a mesh comes within `reach` of `target` (x, z), on a world of `size` that closes
/// on itself (0: with edges, everything is near).
fn near(bounds: [f32; 4], target: Vec3, size: [f32; 2], reach: f32) -> bool {
    if size[0] <= 0.0 || bounds[0] > bounds[2] {
        return true;
    }
    // Distance from a coordinate to an interval, around the world.
    let gap = |t: f32, lo: f32, hi: f32, n: f32| {
        [-n, 0.0, n]
            .iter()
            .map(|&k| {
                let t = t + k;
                if t < lo {
                    lo - t
                } else if t > hi {
                    t - hi
                } else {
                    0.0
                }
            })
            .fold(f32::INFINITY, f32::min)
    };
    let dx = gap(target.x, bounds[0], bounds[2], size[0]);
    let dz = gap(target.z, bounds[1], bounds[3], size[1]);
    dx * dx + dz * dz < reach * reach
}

/// Handle to a model added with `Renderer::add_model`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelId(usize);

/// Handle to an articulated part added with `Renderer::add_part`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartId(usize);

/// Handle to a volume added with `Renderer::add_volume`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeId(usize);

pub struct Renderer {
    gpu: Gpu,
    pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    particle_pipeline: wgpu::RenderPipeline,
    /// The cube every particle is a copy of.
    cube_buffer: wgpu::Buffer,
    cube_vertex_count: u32,
    /// One `ParticleInstance` per particle, regrown when there are more particles than room.
    particle_buffer: wgpu::Buffer,
    particle_count: u32,
    camera_buffer: wgpu::Buffer,
    atmosphere_buffer: wgpu::Buffer,
    materials_buffer: wgpu::Buffer,
    marks_buffer: wgpu::Buffer,
    /// Effects drawn over the world (see `effects`): the marsh's medium, mist, a wave, sight.
    effects_buffer: wgpu::Buffer,
    /// Litter on the ground, one texel per soil patch (1 × 1 × 1 and empty until uploaded).
    litter_field: FieldTexture,
    /// The marsh's medium, in the corner of a fixed texture (see `MARSH_FIELD_SIZE`).
    marsh_field: FieldTexture,
    /// Group 0: camera, atmosphere, materials, shadow map, marks, effects and their fields.
    /// Rebuilt with its layout when a field of it is replaced by one of another size.
    frame_layout: wgpu::BindGroupLayout,
    frame_bind_group: wgpu::BindGroup,
    shadow_sampler: wgpu::Sampler,
    /// Draws depth only, from the sun.
    shadow_pipeline: wgpu::RenderPipeline,
    /// Models (plants): drawn by instancing, in the main pass and in the shadow pass.
    model_pipeline: wgpu::RenderPipeline,
    model_shadow_pipeline: wgpu::RenderPipeline,
    models: Vec<GpuModel>,
    /// Articulated parts (characters): each instance has its own full transform.
    part_pipeline: wgpu::RenderPipeline,
    part_shadow_pipeline: wgpu::RenderPipeline,
    parts: Vec<GpuModel>,
    /// Group 1 for models: a material volume with no fields of its own (models carry their
    /// materials in their vertices), for the volume uniform the fragment shader reads.
    model_volume: Volume,
    /// Group 0 of the shadow pass: the camera only. The shadow map cannot be bound for reading
    /// while it is being drawn into.
    shadow_bind_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    /// Towards the sun, and the box the shadow map must cover.
    sun_direction: Vec3,
    scene_bounds: (Vec3, Vec3),
    /// Mist, wetness of the ground, height of the mist floor (see `set_weather`).
    weather: [f32; 3],
    /// Background, the colour of the haze (linear RGB).
    clear_color: wgpu::Color,
    /// Group 1: one bind group per volume, all with this layout.
    volume_layout: wgpu::BindGroupLayout,
    volumes: Vec<Volume>,
    depth_view: wgpu::TextureView,
    /// The scene image and the last pass that turns it into the final image.
    post: Post,
    /// The interface, drawn over the final image.
    ui: UiPass,
    /// Size of the world when it closes on itself (0: it has edges).
    world_size: [f32; 2],
    /// Feet of the one the camera follows: what hides them is cut away (see `set_focus`).
    focus: Vec3,
    /// Fills the background with the sky, before the scene is drawn over it.
    sky_pipeline: wgpu::RenderPipeline,
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

        // The atmosphere follows the hour of the day: rewritten by `set_sky`.
        let atmosphere = palette::golden_hour();
        let atmosphere_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("atmosphere"),
            contents: bytemuck::bytes_of(&AtmosphereUniform::new(&atmosphere, 0.0)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let materials_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("materials"),
            contents: bytemuck::bytes_of(&MaterialsUniform::new(&palette::materials())),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        // Ripples and footprints, rewritten when they change (see `set_marks`).
        let marks_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("marks"),
            contents: bytemuck::bytes_of(&MarksUniform::new(&[], &[])),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        // Effects over the world, all off until the game sets them (see `set_effects`).
        let effects_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("effects"),
            contents: bytemuck::bytes_of(&EffectsUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        // Their fields: textures start at zero (wgpu clears them), so nothing shows. The
        // litter's size follows the world's, known once it is uploaded.
        let one = sim::grid::Dims {
            nx: 1,
            ny: 1,
            nz: 1,
        };
        let litter_field = FieldTexture::new(device, one);
        let marsh_field = FieldTexture::new(
            device,
            sim::grid::Dims {
                nx: MARSH_FIELD_SIZE,
                ny: 1,
                nz: MARSH_FIELD_SIZE,
            },
        );
        // Shadow map: a depth texture drawn by the shadow pass and read by the main pass.
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d {
                width: SHADOW_SIZE,
                height: SHADOW_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Comparison sampler: returns "is the given depth ≤ the stored one" instead of the
        // depth itself; with linear filtering, the GPU blends 4 such comparisons for free.
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let [r, g, b] = atmosphere.fog_color.map(f64::from);
        let clear_color = wgpu::Color { r, g, b, a: 1.0 };

        // Bind group layout: the "signature" of a group of resources, i.e. what the shader
        // expects at each `@binding` of a `@group`. Group 0, the frame: the camera (binding 0),
        // the atmosphere (1), the material colours (2), the shadow map and its sampler (3, 4),
        // the marks (5), the effects (6) and their two fields, litter and marsh (7, 8).
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
        // A field of group 0, read texel by texel by the fragment shader (as volumes' fields).
        let frame_field_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D3,
                multisampled: false,
            },
            count: None,
        };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame layout"),
            entries: &[
                // The fragment shader also reads the eye position, for the haze.
                uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                // The vertex shader reads the wind from the atmosphere (plants bending).
                uniform_entry(1, wgpu::ShaderStages::VERTEX_FRAGMENT),
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                uniform_entry(5, wgpu::ShaderStages::FRAGMENT),
                // Effects may move vertices (a wave) as well as colour fragments.
                uniform_entry(6, wgpu::ShaderStages::VERTEX_FRAGMENT),
                frame_field_entry(7),
                frame_field_entry(8),
            ],
        });
        // Bind group: the actual resources plugged into that signature.
        let frame_bind_group = create_frame_bind_group(
            device,
            &frame_layout,
            FrameResources {
                camera: &camera_buffer,
                atmosphere: &atmosphere_buffer,
                materials: &materials_buffer,
                shadow_view: &shadow_view,
                shadow_sampler: &shadow_sampler,
                marks: &marks_buffer,
                effects: &effects_buffer,
                litter: &litter_field,
                marsh: &marsh_field,
            },
        );
        let shadow_frame_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shadow frame layout"),
                // The camera, and the atmosphere for the wind: plants cast the shadow of their
                // swaying shape.
                entries: &[
                    uniform_entry(0, wgpu::ShaderStages::VERTEX),
                    uniform_entry(1, wgpu::ShaderStages::VERTEX),
                ],
            });
        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow frame"),
            layout: &shadow_frame_layout,
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
        // The base field is also read by the vertex stage (foliage sways by material).
        let field_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
        // Opaque volumes write their depth; transparent ones (water) are drawn afterwards,
        // blended over what is behind them, and do not write depth so that what lies beneath
        // the surface stays visible through it.
        // The scene passes draw into the HDR scene image, not into the final image.
        let format = SCENE_FORMAT;
        let pipeline =
            create_voxel_pipeline(device, &pipeline_layout, &shader, format, VoxelPass::Opaque);
        let transparent_pipeline = create_voxel_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            VoxelPass::Transparent,
        );

        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow layout"),
            bind_group_layouts: &[Some(&shadow_frame_layout), Some(&volume_layout)],
            immediate_size: 0,
        });
        let shadow_pipeline =
            create_shadow_pipeline(device, &shadow_layout, &shader, VoxelPass::Opaque);
        let model_shadow_pipeline =
            create_shadow_pipeline(device, &shadow_layout, &shader, VoxelPass::Model);
        let part_shadow_pipeline =
            create_shadow_pipeline(device, &shadow_layout, &shader, VoxelPass::Part);
        let model_pipeline =
            create_voxel_pipeline(device, &pipeline_layout, &shader, format, VoxelPass::Model);
        let part_pipeline =
            create_voxel_pipeline(device, &pipeline_layout, &shader, format, VoxelPass::Part);
        let mut model_volume = Volume::new(
            device,
            &VolumeStyle {
                origin: [0.0; 3],
                palette: palette::earth(),
                value_range: (0.0, 1.0),
                materials: true,
                life: crate::volume::LifeStyle {
                    palette: palette::foam(),
                    fade: (1.0, 2.0),
                    value_range: (1.0, 2.0),
                },
                transparent: false,
            },
        );
        // A 1×1×1 field only to complete the bind group: never read by models.
        let empty = Field3::filled(
            sim::grid::Dims {
                nx: 1,
                ny: 1,
                nz: 1,
            },
            0.0,
        );
        model_volume.upload_base(device, gpu.queue(), &volume_layout, &empty);

        // Particles: only group 0 (camera, atmosphere), two vertex buffers (the cube, per
        // vertex; the instances, per cube).
        let particle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle layout"),
            bind_group_layouts: &[Some(&frame_layout)],
            immediate_size: 0,
        });
        let particle_pipeline = create_particle_pipeline(device, &particle_layout, &shader, format);
        let sky_pipeline = create_sky_pipeline(device, &particle_layout, &shader, format);
        let cube = unit_cube();
        let cube_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("particle cube"),
            contents: bytemuck::cast_slice(&cube),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let particle_buffer = create_particle_buffer(device, 1024);

        let (width, height) = gpu.size();
        let depth_view = create_depth_view(device, width, height);
        let post = Post::new(device, gpu.surface_format(), width, height);
        let ui = UiPass::new(device, gpu.surface_format());

        Self {
            gpu,
            pipeline,
            transparent_pipeline,
            particle_pipeline,
            cube_buffer,
            cube_vertex_count: cube.len() as u32,
            particle_buffer,
            particle_count: 0,
            camera_buffer,
            atmosphere_buffer,
            materials_buffer,
            marks_buffer,
            effects_buffer,
            litter_field,
            marsh_field,
            frame_layout,
            frame_bind_group,
            shadow_sampler,
            shadow_pipeline,
            model_pipeline,
            model_shadow_pipeline,
            models: Vec::new(),
            part_pipeline,
            part_shadow_pipeline,
            parts: Vec::new(),
            model_volume,
            shadow_bind_group,
            shadow_view,
            sun_direction: Vec3::from(atmosphere.sun_direction),
            scene_bounds: (Vec3::ZERO, Vec3::splat(64.0)),
            world_size: [0.0, 0.0],
            focus: Vec3::ZERO,
            sky_pipeline,
            weather: [0.0, 0.0, 0.0],
            clear_color,
            volume_layout,
            volumes: Vec::new(),
            depth_view,
            post,
            ui,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.gpu.resize(width, height);
        self.depth_view = create_depth_view(self.gpu.device(), width, height);
        self.post.resize(self.gpu.device(), width, height);
    }

    /// Light, haze and grading for the hour of the day (see `sky::sky`).
    pub fn set_sky(&mut self, sky: &Sky) {
        let atmosphere = &sky.atmosphere;
        let uniform = AtmosphereUniform::new(atmosphere, sky.stars)
            .with_moon(sky.moon)
            .with_wind(sky.wind)
            .with_season(sky.season, sky.dry);
        self.gpu
            .queue()
            .write_buffer(&self.atmosphere_buffer, 0, bytemuck::bytes_of(&uniform));
        self.sun_direction = Vec3::from(atmosphere.sun_direction).normalize();
        let [r, g, b] = atmosphere.fog_color.map(f64::from);
        self.clear_color = wgpu::Color { r, g, b, a: 1.0 };
        self.post.set_grade(&sky.grade, sky.night);
    }

    /// Feet of the one the camera follows (at their copy nearest the point looked at, on a
    /// planet): what stands between them and the eye is not drawn.
    pub fn set_focus(&mut self, feet: Vec3) {
        self.focus = feet;
    }

    /// How weary the naturalist is, in [0, 1]: the image darkens at its edges and dulls.
    pub fn set_weariness(&mut self, weariness: f32) {
        self.post.set_weariness(weariness);
    }

    /// The interface to draw over the next frames (see `ui::Ui`).
    pub fn set_ui(&mut self, vertices: &[UiVertex]) {
        let (device, queue) = (self.gpu.device(), self.gpu.queue());
        self.ui.upload(device, queue, vertices);
    }

    /// Size of the images drawn, in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.gpu.size()
    }

    /// Ripples on the water and footprints in the sand (see `marks`).
    pub fn set_marks(&mut self, ripples: &[Ripple], prints: &[Print]) {
        let uniform = MarksUniform::new(ripples, prints);
        self.gpu
            .queue()
            .write_buffer(&self.marks_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// The effects drawn over the world (see `effects::EffectsUniform`).
    pub fn set_effects(&mut self, effects: &EffectsUniform) {
        self.gpu
            .queue()
            .write_buffer(&self.effects_buffer, 0, bytemuck::bytes_of(effects));
    }

    /// The litter lying on the ground, one value per soil patch: a 2D grid (`ny` = 1) covering
    /// the world evenly. Cheap (a few thousand texels): send it again when it changes.
    pub fn upload_litter(&mut self, field: &Field3) {
        if self.litter_field.dims() != field.dims {
            self.litter_field = FieldTexture::new(self.gpu.device(), field.dims);
            self.frame_bind_group = create_frame_bind_group(
                self.gpu.device(),
                &self.frame_layout,
                FrameResources {
                    camera: &self.camera_buffer,
                    atmosphere: &self.atmosphere_buffer,
                    materials: &self.materials_buffer,
                    shadow_view: &self.shadow_view,
                    shadow_sampler: &self.shadow_sampler,
                    marks: &self.marks_buffer,
                    effects: &self.effects_buffer,
                    litter: &self.litter_field,
                    marsh: &self.marsh_field,
                },
            );
        }
        self.litter_field.upload(self.gpu.queue(), field);
    }

    /// The marsh's medium: a 2D grid (`ny` = 1) of at most `MARSH_FIELD_SIZE` texels each way,
    /// written in the corner of its texture. Where it lies and how it shows is in the effects
    /// (`EffectsUniform::with_marsh`). Cheap: send it after each step of the medium.
    pub fn upload_marsh(&mut self, field: &Field3) {
        self.marsh_field
            .upload_region(self.gpu.queue(), [0, 0, 0], field);
    }

    /// Mist (0 to 1) lying around `floor` (height in cells: thick below it, thinning above),
    /// and how wet the ground is (0 dry to 1 soaked: darker, glossy).
    pub fn set_weather(&mut self, mist: f32, wet: f32, floor: f32) {
        self.weather = [mist, wet, floor];
    }

    /// The world closes on itself, `nx` × `nz` cells (a planet): what lies across its edges is
    /// drawn next to the eye, and shadows follow the eye.
    pub fn set_world_size(&mut self, nx: f32, nz: f32) {
        self.world_size = [nx, nz];
    }

    /// The box (world coordinates) that casts and receives shadows. Outside it, everything is
    /// lit. Tighter is sharper: the shadow map's texels are spread over this box.
    pub fn set_scene_bounds(&mut self, min: Vec3, max: Vec3) {
        self.scene_bounds = (min, max);
    }

    /// Adds a model (a mesh made with `mesher::mesh_materials`). Nothing is drawn until it has
    /// instances.
    pub fn add_model(&mut self, mesh: &MeshData) -> ModelId {
        let (device, queue) = (self.gpu.device(), self.gpu.queue());
        self.models.push(GpuModel::new(device, queue, mesh));
        ModelId(self.models.len() - 1)
    }

    /// Where a model is drawn: one copy per instance.
    pub fn set_model_instances(&mut self, id: ModelId, instances: &[ModelInstance]) {
        let (device, queue) = (self.gpu.device(), self.gpu.queue());
        self.models[id.0].set_instances(device, queue, instances);
    }

    /// Adds an articulated part (a mesh made with `mesher::mesh_materials`, its origin at its
    /// pivot). Nothing is drawn until it has instances.
    pub fn add_part(&mut self, mesh: &MeshData) -> PartId {
        let (device, queue) = (self.gpu.device(), self.gpu.queue());
        self.parts.push(GpuModel::new(device, queue, mesh));
        PartId(self.parts.len() - 1)
    }

    /// Where a part is drawn this frame: one copy per instance.
    pub fn set_part_instances(&mut self, id: PartId, instances: &[PartInstance]) {
        let (device, queue) = (self.gpu.device(), self.gpu.queue());
        self.parts[id.0].set_instances(device, queue, instances);
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
        self.upload_mesh_part(id, 0, data);
    }

    /// Replaces part `part` of a volume's mesh: a large volume (the ground) is meshed in
    /// chunks, and only the chunk that changed is sent again.
    pub fn upload_mesh_part(&mut self, id: VolumeId, part: usize, data: &MeshData) {
        let (device, queue) = (self.gpu.device(), self.gpu.queue());
        self.volumes[id.0].upload_mesh(device, queue, part, data);
    }

    /// Copies the base field of a volume (its inert colour) to the GPU. Must be called once
    /// before `upload_life`.
    pub fn upload_base(&mut self, id: VolumeId, field: &Field3) {
        let (device, queue) = (self.gpu.device(), self.gpu.queue());
        self.volumes[id.0].upload_base(device, queue, &self.volume_layout, field);
    }

    /// Copies `region`, a packed box of a volume's base field, with its first cell at `at`: a
    /// few cells changed, without sending the whole field again (the ground's is 67 MB). The
    /// base field must have been uploaded whole first.
    pub fn upload_base_region(&mut self, id: VolumeId, at: [usize; 3], region: &Field3) {
        self.volumes[id.0].upload_base_region(self.gpu.queue(), at, region);
    }

    /// Copies the life field of a volume to the GPU. Same dimensions as the base field.
    /// Cheap enough to call every simulation step.
    pub fn upload_life(&mut self, id: VolumeId, field: &Field3) {
        self.volumes[id.0].upload_life(self.gpu.queue(), field);
    }

    /// Replaces the particles drawn every frame.
    pub fn upload_particles(&mut self, particles: &[ParticleInstance]) {
        let needed = std::mem::size_of_val(particles) as u64;
        if needed > self.particle_buffer.size() {
            // Room for 50 % more, so a growing cloud does not reallocate every frame.
            self.particle_buffer =
                create_particle_buffer(self.gpu.device(), particles.len() * 3 / 2);
        }
        self.gpu
            .queue()
            .write_buffer(&self.particle_buffer, 0, bytemuck::cast_slice(particles));
        self.particle_count = particles.len() as u32;
    }

    /// The last frame drawn offscreen, as RGBA bytes (see `Gpu::read_pixels`), with its size.
    /// `None` when drawing in a window.
    pub fn capture(&self) -> Option<(u32, u32, Vec<u8>)> {
        let (width, height) = self.gpu.size();
        self.gpu.read_pixels().map(|pixels| (width, height, pixels))
    }

    /// The last scene drawn, before grading and the interface: linear RGB per pixel, rows
    /// from top to bottom, with its size. Waits for the GPU: for a rare moment (a sketch in
    /// the notebook), not every frame.
    pub fn snapshot(&self) -> Option<(u32, u32, Vec<[f32; 3]>)> {
        let device = self.gpu.device();
        let texture = self.post.scene_view().texture();
        let (width, height) = (texture.width(), texture.height());
        // 4 half floats a pixel; rows padded to 256 bytes for the copy.
        let row = 8 * width;
        let padded_row = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("snapshot"),
            size: (padded_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("snapshot"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.gpu.queue().submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let mapped = slice.get_mapped_range().ok()?;
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height as usize {
            let start = y * padded_row as usize;
            for px in mapped[start..start + row as usize].chunks_exact(8) {
                let half = |k: usize| f16_to_f32(u16::from_le_bytes([px[k], px[k + 1]]));
                pixels.push([half(0), half(2), half(4)]);
            }
        }
        drop(mapped);
        buffer.unmap();
        Some((width, height, pixels))
    }

    /// Width / height of the window images.
    pub fn aspect(&self) -> f32 {
        let (width, height) = self.gpu.size();
        width as f32 / height as f32
    }

    /// Draws a frame. `time` (seconds) drives animations such as water ripples.
    pub fn render(&mut self, camera: &OrbitCamera, time: f32) {
        let Some(frame) = self.gpu.acquire_frame() else {
            return;
        };
        let (mut min, mut max) = self.scene_bounds;
        // On a planet, the shadows cover what is around the eye (across the edges too).
        if self.world_size[0] > 0.0 {
            let t = camera.target;
            min = Vec3::new(t.x - SHADOW_REACH, min.y, t.z - SHADOW_REACH);
            max = Vec3::new(t.x + SHADOW_REACH, max.y, t.z + SHADOW_REACH);
        }
        let light = light_view_proj(self.sun_direction, min, max);
        // On a planet, only the ground within reach of the point looked at is drawn, fading
        // into the haze at the edge.
        let reach = camera.reach(self.aspect());
        let uniform = CameraUniform::new(
            camera,
            self.aspect(),
            time,
            self.weather,
            light,
            self.world_size,
            self.focus,
            reach,
        );
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
            // Shadow pass: depth of the opaque volumes seen from the sun. No colour target.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        // Kept: the main pass reads it.
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.shadow_bind_group, &[]);
            for volume in self.volumes.iter().filter(|v| !v.transparent) {
                let Some(fields) = &volume.fields else {
                    continue;
                };
                pass.set_bind_group(1, &fields.bind_group, &[]);
                for mesh in volume.meshes.iter().flatten().filter(|m| {
                    m.index_count > 0
                        && near(m.bounds, camera.target, self.world_size, SHADOW_REACH * 1.5)
                }) {
                    pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                }
            }
            pass.set_pipeline(&self.model_shadow_pipeline);
            self.draw_models(&mut pass);
            pass.set_pipeline(&self.part_shadow_pipeline);
            self.draw_parts(&mut pass);
        }
        {
            // A render pass = a series of draws into the same target images.
            // `LoadOp::Clear` fills the image before drawing, `StoreOp::Store` keeps the result.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.post.scene_view(),
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
            // Opaque first, transparent last: blending needs what is behind to be drawn.
            pass.set_bind_group(0, &self.frame_bind_group, &[]);
            // The sky first, behind everything: one triangle covering the screen.
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            for (pipeline, transparent) in
                [(&self.pipeline, false), (&self.transparent_pipeline, true)]
            {
                // Particles are opaque: drawn after the opaque volumes, before the water, so
                // those under the surface show through it.
                if transparent && self.particle_count > 0 {
                    pass.set_pipeline(&self.particle_pipeline);
                    pass.set_vertex_buffer(0, self.cube_buffer.slice(..));
                    pass.set_vertex_buffer(1, self.particle_buffer.slice(..));
                    pass.draw(0..self.cube_vertex_count, 0..self.particle_count);
                }
                pass.set_pipeline(pipeline);
                for volume in self.volumes.iter().filter(|v| v.transparent == transparent) {
                    let Some(fields) = &volume.fields else {
                        continue;
                    };
                    pass.set_bind_group(1, &fields.bind_group, &[]);
                    for mesh in volume.meshes.iter().flatten().filter(|m| {
                        m.index_count > 0 && near(m.bounds, camera.target, self.world_size, reach)
                    }) {
                        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        pass.set_index_buffer(
                            mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                    }
                }
                // Models are opaque: drawn with the opaque volumes.
                if !transparent {
                    pass.set_pipeline(&self.model_pipeline);
                    self.draw_models(&mut pass);
                    pass.set_pipeline(&self.part_pipeline);
                    self.draw_parts(&mut pass);
                }
            }
        }
        self.post
            .draw(self.gpu.queue(), &mut encoder, &frame.view, time, camera);
        self.ui.draw(&mut encoder, &frame.view);
        self.gpu.queue().submit([encoder.finish()]);
        self.gpu.present(frame);
    }
}

/// What group 0 holds (see the frame layout in `Renderer::new`).
struct FrameResources<'a> {
    camera: &'a wgpu::Buffer,
    atmosphere: &'a wgpu::Buffer,
    materials: &'a wgpu::Buffer,
    shadow_view: &'a wgpu::TextureView,
    shadow_sampler: &'a wgpu::Sampler,
    marks: &'a wgpu::Buffer,
    effects: &'a wgpu::Buffer,
    litter: &'a FieldTexture,
    marsh: &'a FieldTexture,
}

/// Group 0's bind group: the resources plugged into the frame layout, binding by binding.
fn create_frame_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    r: FrameResources,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: r.camera.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: r.atmosphere.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: r.materials.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(r.shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(r.shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: r.marks.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: r.effects.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(&r.litter.view),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(&r.marsh.view),
            },
        ],
    })
}

/// Depth only, from the sun. Back faces are culled like in the main pass; a depth bias
/// pushes the stored depths slightly away from the sun, so that lit surfaces do not find
/// themselves "behind" their own depth (shadow acne).
fn create_shadow_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    pass: VoxelPass,
) -> wgpu::RenderPipeline {
    let (label, entry_point) = match pass {
        VoxelPass::Model => ("model shadow pipeline", "vs_model_shadow"),
        VoxelPass::Part => ("part shadow pipeline", "vs_part_shadow"),
        VoxelPass::Opaque | VoxelPass::Transparent => ("shadow pipeline", "vs_shadow"),
    };
    let models = pass == VoxelPass::Model;
    let buffers = vertex_buffers(pass);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(entry_point),
            compilation_options: Default::default(),
            buffers: &buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            // Mirrored models reverse their winding: no culling for them (see the main pass).
            cull_mode: (!models).then_some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        // No fragment stage: only the depth is written.
        fragment: None,
        multiview_mask: None,
        cache: None,
    })
}

impl Renderer {
    /// Draws every model with instances, with the pipeline already set on `pass`.
    fn draw_models(&self, pass: &mut wgpu::RenderPass<'_>) {
        Self::draw_instanced(pass, &self.models, &self.model_volume);
    }

    /// Draws every articulated part with instances, with the pipeline already set on `pass`.
    fn draw_parts(&self, pass: &mut wgpu::RenderPass<'_>) {
        Self::draw_instanced(pass, &self.parts, &self.model_volume);
    }

    fn draw_instanced(pass: &mut wgpu::RenderPass<'_>, models: &[GpuModel], volume: &Volume) {
        if let Some(fields) = &volume.fields {
            pass.set_bind_group(1, &fields.bind_group, &[]);
        }
        for model in models {
            if model.instance_count == 0 || model.mesh.index_count == 0 {
                continue;
            }
            pass.set_vertex_buffer(0, model.mesh.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, model.instances.slice(..));
            pass.set_index_buffer(model.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..model.mesh.index_count, 0, 0..model.instance_count);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VoxelPass {
    Opaque,
    Transparent,
    /// Opaque models drawn by instancing (vertex + instance buffers).
    Model,
    /// Articulated parts: instancing with a full transform per instance.
    Part,
}

/// Vertex buffers of each kind of draw: the mesh, plus the instances for models and parts.
fn vertex_buffers(pass: VoxelPass) -> Vec<Option<wgpu::VertexBufferLayout<'static>>> {
    match pass {
        VoxelPass::Opaque | VoxelPass::Transparent => vec![Some(Vertex::layout())],
        VoxelPass::Model => vec![Some(Vertex::layout()), Some(ModelInstance::layout())],
        VoxelPass::Part => vec![Some(Vertex::layout()), Some(PartInstance::layout())],
    }
}

fn create_voxel_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    pass: VoxelPass,
) -> wgpu::RenderPipeline {
    let transparent = pass == VoxelPass::Transparent;
    let (label, entry_point) = match pass {
        VoxelPass::Opaque => ("voxel pipeline", "vs_main"),
        VoxelPass::Transparent => ("transparent voxel pipeline", "vs_main"),
        VoxelPass::Model => ("model pipeline", "vs_model"),
        VoxelPass::Part => ("part pipeline", "vs_part"),
    };
    let buffers = vertex_buffers(pass);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(entry_point),
            compilation_options: Default::default(),
            buffers: &buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            // Faces seen from behind are never visible on closed volumes: skip them. Not for
            // models: a mirrored instance reverses the winding of its triangles, and culling
            // would then remove its front faces (holes). Their meshes are closed, so depth
            // testing hides the back faces anyway.
            cull_mode: (pass != VoxelPass::Model).then_some(wgpu::Face::Back),
            ..Default::default()
        },
        // Depth test: each pixel keeps the closest surface, whatever the drawing order.
        // Transparent surfaces are still hidden behind opaque ones, but write no depth.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(!transparent),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if transparent { "fs_water" } else { "fs_main" }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                // Alpha blending: result = colour · alpha + what was there · (1 − alpha).
                blend: transparent.then_some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_particle_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("particle pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_particle"),
            compilation_options: Default::default(),
            buffers: &[Some(CubeVertex::layout()), Some(ParticleInstance::layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_particle"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The sky: a triangle covering the screen, behind everything (it writes no depth and passes
/// every depth test, and is drawn first).
fn create_sky_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_sky"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_sky"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_particle_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("particles"),
        size: (capacity.max(1) * std::mem::size_of::<ParticleInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
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

/// A 16-bit float (IEEE 754 half: 1 sign bit, 5 exponent bits, 10 mantissa bits) as an f32.
fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = ((h >> 10) & 0x1f) as i32;
    let mantissa = (h & 0x3ff) as f32;
    match exponent {
        0 => sign * mantissa * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        e => sign * (1.0 + mantissa / 1024.0) * 2f32.powi(e - 15),
    }
}

#[cfg(test)]
mod half_tests {
    #[test]
    fn halves_decode() {
        assert_eq!(super::f16_to_f32(0x3c00), 1.0);
        assert_eq!(super::f16_to_f32(0xc000), -2.0);
        assert_eq!(super::f16_to_f32(0x3555), 0.333_251_95);
        assert_eq!(super::f16_to_f32(0x0000), 0.0);
    }
}
