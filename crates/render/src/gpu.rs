//! GPU context: everything needed to talk to the graphics card and draw images, either in a
//! window or offscreen (into a texture that can be read back, e.g. to save a screenshot).
//!
//! Chain of objects (each one is obtained from the previous one):
//! - `Instance`: entry point of wgpu, picks the native API (Metal on macOS, Vulkan, DX12…).
//! - `Surface`: the part of the window we draw into (window target only).
//! - `Adapter`: a physical GPU (compatible with that surface, if any).
//! - `Device` + `Queue`: the logical connection to the GPU. The device creates resources
//!   (buffers, textures, pipelines), the queue sends work and data to the GPU.
//!
//! This module does not depend on winit: the window is passed as anything wgpu can draw into.

use std::fmt::Debug;

/// Format of offscreen images: 8-bit RGBA, sRGB like a window, so captures look like the game.
const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Where frames are drawn.
enum Target {
    Window {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
    /// A texture we own; it keeps the last frame, which `read_pixels` can copy back.
    Offscreen {
        texture: wgpu::Texture,
        width: u32,
        height: u32,
    },
}

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: Target,
}

impl Gpu {
    /// Creates the GPU context for a window of `width`×`height` physical pixels.
    ///
    /// `display` is the connection to the windowing system (winit's `OwnedDisplayHandle`),
    /// `target` the window itself (e.g. `Arc<winit::window::Window>`).
    /// Panics on failure: this only runs at start-up, and there is no game without a GPU.
    pub fn new<D, T>(display: D, target: T, width: u32, height: u32) -> Self
    where
        D: wgpu::rwh::HasDisplayHandle + Debug + Send + Sync + 'static,
        T: Into<wgpu::SurfaceTarget<'static>>,
    {
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display)).with_env(),
        );
        let surface = instance.create_surface(target).expect("surface creation");
        let (adapter, device, queue) = connect(&instance, Some(&surface));

        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .expect("surface not supported by the adapter");
        // An sRGB format makes the GPU convert our linear colours to screen colours on write:
        // shaders and palettes can then work in linear space, where light adds up correctly.
        let caps = surface.get_capabilities(&adapter);
        if let Some(srgb) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            config.format = srgb;
        }
        // Vsync, supported everywhere: no tearing and no wasted frames.
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        Self {
            device,
            queue,
            target: Target::Window { surface, config },
        }
    }

    /// Creates a GPU context without window, drawing into a `width`×`height` texture.
    /// For screenshots and automated checks.
    pub fn offscreen(width: u32, height: u32) -> Self {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
        let (_, device, queue) = connect(&instance, None);
        let texture = create_offscreen_texture(&device, width, height);
        Self {
            device,
            queue,
            target: Target::Offscreen {
                texture,
                width,
                height,
            },
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Texture format of the images drawn, needed to build render pipelines.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        match &self.target {
            Target::Window { config, .. } => config.format,
            Target::Offscreen { .. } => OFFSCREEN_FORMAT,
        }
    }

    /// Size of the images drawn, in pixels.
    pub fn size(&self) -> (u32, u32) {
        match &self.target {
            Target::Window { config, .. } => (config.width, config.height),
            Target::Offscreen { width, height, .. } => (*width, *height),
        }
    }

    /// Adapts to a new image size. A zero size (minimised window) is ignored.
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width == 0 || new_height == 0 {
            return;
        }
        match &mut self.target {
            Target::Window { surface, config } => {
                config.width = new_width;
                config.height = new_height;
                surface.configure(&self.device, config);
            }
            Target::Offscreen {
                texture,
                width,
                height,
            } => {
                *texture = create_offscreen_texture(&self.device, new_width, new_height);
                *width = new_width;
                *height = new_height;
            }
        }
    }

    /// Acquires the next image to draw into, or `None` if there is none this frame
    /// (window hidden, timeout, surface just reconfigured): the next redraw simply retries.
    pub fn acquire_frame(&mut self) -> Option<Frame> {
        let (surface, config) = match &self.target {
            Target::Window { surface, config } => (surface, config),
            Target::Offscreen { texture, .. } => {
                return Some(Frame {
                    view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    window_texture: None,
                    suboptimal: false,
                });
            }
        };
        let (texture, suboptimal) = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
            // Still usable this frame; reconfigured just after presenting.
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return None;
            }
            // The surface no longer matches the window: reconfigure, retry next frame.
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => {
                surface.configure(&self.device, config);
                return None;
            }
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        Some(Frame {
            view,
            window_texture: Some(texture),
            suboptimal,
        })
    }

    /// Shows the frame in the window (nothing to do offscreen: the texture keeps it).
    /// Work drawing into it must have been submitted before.
    pub fn present(&mut self, frame: Frame) {
        if let Some(texture) = frame.window_texture {
            self.queue.present(texture);
        }
        if let (true, Target::Window { surface, config }) = (frame.suboptimal, &self.target) {
            surface.configure(&self.device, config);
        }
    }

    /// Copies the last offscreen frame back to the CPU: RGBA, 8 bits per channel, rows from
    /// top to bottom, no padding. `None` for a window target.
    pub fn read_pixels(&self) -> Option<Vec<u8>> {
        let Target::Offscreen {
            texture,
            width,
            height,
        } = &self.target
        else {
            return None;
        };
        // Texture → buffer copies need rows aligned to 256 bytes: copy padded rows, then
        // drop the padding.
        let row = 4 * width;
        let padded_row = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback"),
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
                    rows_per_image: Some(*height),
                },
            },
            wgpu::Extent3d {
                width: *width,
                height: *height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        // Mapping is asynchronous: ask for it, then wait for the GPU to finish.
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            result.expect("mapping the readback buffer");
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("waiting for the GPU");
        let mapped = slice.get_mapped_range().ok()?;
        let mut pixels = Vec::with_capacity((row * height) as usize);
        for y in 0..*height as usize {
            let start = y * padded_row as usize;
            pixels.extend_from_slice(&mapped[start..start + row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Some(pixels)
    }
}

/// Picks an adapter (compatible with `surface` if given) and opens a device on it.
fn connect(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> (wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    // wgpu is async to also run in browsers. Natively we simply block with pollster.
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: surface,
        ..Default::default()
    }))
    .expect("no suitable GPU adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("device"),
        // Default limits are guaranteed on every WebGPU implementation: we start there
        // and raise them only when a feature needs it.
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .expect("GPU device creation");
    (adapter, device, queue)
}

fn create_offscreen_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OFFSCREEN_FORMAT,
        // Drawn into, then copied out.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// An image being drawn this frame.
pub struct Frame {
    /// View to use as colour attachment of a render pass.
    pub view: wgpu::TextureView,
    /// The window image to present, `None` offscreen.
    window_texture: Option<wgpu::SurfaceTexture>,
    suboptimal: bool,
}
