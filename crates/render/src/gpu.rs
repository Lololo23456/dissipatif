//! GPU context: everything needed to talk to the graphics card and present images in a window.
//!
//! Chain of objects (each one is obtained from the previous one):
//! - `Instance`: entry point of wgpu, picks the native API (Metal on macOS, Vulkan, DX12…).
//! - `Surface`: the part of the window we draw into.
//! - `Adapter`: a physical GPU compatible with that surface.
//! - `Device` + `Queue`: the logical connection to the GPU. The device creates resources
//!   (buffers, textures, pipelines), the queue sends work and data to the GPU.
//!
//! This module does not depend on winit: the window is passed as anything wgpu can draw into.

use std::fmt::Debug;

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
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

        // wgpu is async to also run in browsers. Natively we simply block with pollster.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("no GPU adapter compatible with the window");

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("device"),
            // Default limits are guaranteed on every WebGPU implementation: we start there
            // and raise them only when a feature needs it.
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .expect("GPU device creation");

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
            surface,
            device,
            queue,
            config,
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Texture format of the window images, needed to build render pipelines.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Size of the window images in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Adapts the surface to a new window size. A zero size (minimised window) is ignored.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Acquires the next window image to draw into, or `None` if there is none this frame
    /// (window hidden, timeout, surface just reconfigured): the next redraw simply retries.
    pub fn acquire_frame(&mut self) -> Option<Frame> {
        let (texture, suboptimal) = match self.surface.get_current_texture() {
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
                self.surface.configure(&self.device, &self.config);
                return None;
            }
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        Some(Frame {
            texture,
            view,
            suboptimal,
        })
    }

    /// Shows the frame in the window. Work drawing into it must have been submitted before.
    pub fn present(&mut self, frame: Frame) {
        let suboptimal = frame.suboptimal;
        self.queue.present(frame.texture);
        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
    }
}

/// A window image being drawn this frame.
pub struct Frame {
    texture: wgpu::SurfaceTexture,
    /// View to use as colour attachment of a render pass.
    pub view: wgpu::TextureView,
    suboptimal: bool,
}
