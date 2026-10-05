//! Binaire jouable. Pour l'instant : un monde procédural (relief, mer, plages, déserts, forêts,
//! montagnes, rivières, lacs), vu par la caméra plongeante. Le feuillage ondule au vent, du
//! pollen flotte dans la lumière, des feuilles tombent.
//! Commandes : glisser avec le bouton gauche pour tourner, molette pour zoomer.
//! `--seed N` choisit le monde ; `--capture fichier.png` enregistre une vue sans fenêtre.
//! Prochaine étape : remettre la vie (eau qui coule, érosion, végétation qui pousse).

mod ambient;
mod scene;

use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use render::{Gpu, OrbitCamera, Renderer};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};
use world::{World, WorldConfig};

use ambient::Ambient;
use scene::SceneData;

/// Radians of rotation per pixel of mouse drag.
const ORBIT_SPEED: f32 = 0.005;
/// Distance factor per wheel notch.
const ZOOM_STEP: f32 = 0.9;
/// Pixels of trackpad scroll counted as one wheel notch.
const PIXELS_PER_NOTCH: f32 = 50.0;
const DEFAULT_SEED: u64 = 1;
/// Seed offset of the particles in the air.
const AMBIENT_SEED: u64 = 0xa1b;
/// Frame duration used to advance the air when capturing (60 frames per second).
const CAPTURE_STEP: f32 = 1.0 / 60.0;

/// Window and renderer are created together on `resumed`, so they are either both present or both absent.
struct Graphics {
    // `Arc` because the wgpu surface keeps its own reference to the window:
    // the window must live at least as long as the surface.
    window: Arc<Window>,
    renderer: Renderer,
}

struct App {
    graphics: Option<Graphics>,
    world: World,
    data: SceneData,
    ambient: Ambient,
    start: Instant,
    last_frame: Instant,
    camera: OrbitCamera,
    dragging: bool,
    last_cursor: Option<PhysicalPosition<f64>>,
}

impl App {
    fn new(seed: u64) -> Self {
        let start = Instant::now();
        let config = WorldConfig::standard(seed);
        let world = World::generate(config);
        let generated = start.elapsed();
        let data = SceneData::build(&world);
        println!(
            "Monde {seed} : généré en {:.0} ms, préparé en {:.0} ms ({} faces, {} faces d'eau, \
             {} plantes, {} éléments au sol, {} faces de modèles)",
            generated.as_secs_f64() * 1000.0,
            (start.elapsed() - generated).as_secs_f64() * 1000.0,
            data.solid_faces(),
            data.water_faces(),
            world.plant_count(),
            world.ground_cover_count(),
            data.plant_model_faces(),
        );
        let dims = config.dims;
        let centre = Vec3::new(dims.nx as f32 / 2.0, config.sea_level, dims.nz as f32 / 2.0);
        let ambient = Ambient::new(&world, seed ^ AMBIENT_SEED);
        Self {
            graphics: None,
            world,
            data,
            ambient,
            start,
            last_frame: Instant::now(),
            camera: OrbitCamera::framing(centre, 0.45 * dims.nx.max(dims.nz) as f32),
            dragging: false,
            last_cursor: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.graphics.is_some() {
            return;
        }
        let attributes = Window::default_attributes().with_title("Dissipatif");
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("création de la fenêtre"),
        );
        let size = window.inner_size();
        let gpu = Gpu::new(
            event_loop.owned_display_handle(),
            window.clone(),
            size.width,
            size.height,
        );
        let mut renderer = Renderer::new(gpu);
        self.data.install(&mut renderer);
        self.graphics = Some(Graphics { window, renderer });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                // Clamped: after a pause of the window (dragging, sleep), no huge jump.
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;
                let time = (now - self.start).as_secs_f32();
                let target = [self.camera.target.x, self.camera.target.z];
                self.ambient.update(dt, time, target, &self.world);
                graphics.renderer.upload_particles(self.ambient.instances());
                graphics.renderer.render(&self.camera, time);
            }
            WindowEvent::Resized(size) => graphics.renderer.resize(size.width, size.height),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.dragging = state == ElementState::Pressed,
            WindowEvent::CursorMoved { position, .. } => {
                if let (true, Some(last)) = (self.dragging, self.last_cursor) {
                    let dx = (position.x - last.x) as f32;
                    let dy = (position.y - last.y) as f32;
                    // Dragging right turns the scene right; dragging down raises the camera.
                    self.camera.orbit(-dx * ORBIT_SPEED, dy * ORBIT_SPEED);
                }
                self.last_cursor = Some(position);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / PIXELS_PER_NOTCH,
                };
                self.camera.zoom(ZOOM_STEP.powf(notches));
            }
            _ => {}
        }
    }

    /// Called once all pending events are handled: ask for the next frame (continuous rendering,
    /// paced by vsync).
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(graphics) = &self.graphics {
            graphics.window.request_redraw();
        }
    }
}

/// Command-line options.
struct Options {
    seed: u64,
    capture: Option<CaptureOptions>,
}

/// `--capture`: render one image offscreen and save it, without opening a window.
struct CaptureOptions {
    path: String,
    width: u32,
    height: u32,
    /// Camera angles in degrees, zoom factor (< 1 closer), point looked at (x, z).
    yaw: Option<f32>,
    pitch: Option<f32>,
    zoom: f32,
    at: Option<(f32, f32)>,
    /// Seconds of animation (wind, particles) before the picture.
    time: f32,
}

impl Options {
    /// `[--seed N] [--capture file.png [--size WxH] [--yaw DEG] [--pitch DEG] [--zoom F]
    /// [--at X,Z] [--time SECONDS]]`.
    fn from_args(args: &[String]) -> Result<Self, String> {
        let value = |name: &str| -> Option<&str> {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .map(String::as_str)
        };
        let number = |name: &str| -> Result<Option<f32>, String> {
            value(name)
                .map(|v| v.parse::<f32>().map_err(|e| format!("{name} {v} : {e}")))
                .transpose()
        };
        let pair = |name: &str, separator: char| -> Result<Option<(f32, f32)>, String> {
            value(name)
                .map(|v| {
                    let (a, b) = v
                        .split_once(separator)
                        .ok_or_else(|| format!("{name} {v} : attendu A{separator}B"))?;
                    let parse = |s: &str| s.parse::<f32>().map_err(|e| format!("{name} {v} : {e}"));
                    Ok((parse(a)?, parse(b)?))
                })
                .transpose()
        };
        let seed = match value("--seed") {
            Some(v) => v.parse().map_err(|e| format!("--seed {v} : {e}"))?,
            None => DEFAULT_SEED,
        };
        let capture = match value("--capture") {
            Some(path) => {
                let (width, height) = pair("--size", 'x')?.unwrap_or((1280.0, 800.0));
                Some(CaptureOptions {
                    path: path.to_owned(),
                    width: width as u32,
                    height: height as u32,
                    yaw: number("--yaw")?,
                    pitch: number("--pitch")?,
                    zoom: number("--zoom")?.unwrap_or(1.0),
                    at: pair("--at", ',')?,
                    time: number("--time")?.unwrap_or(0.0),
                })
            }
            None => None,
        };
        Ok(Self { seed, capture })
    }
}

/// Generates the world, renders one frame offscreen, saves it as PNG.
fn capture(seed: u64, options: &CaptureOptions) -> Result<(), String> {
    let mut app = App::new(seed);
    if let Some((x, z)) = options.at {
        app.camera.target = Vec3::new(x, app.camera.target.y, z);
    }
    if let Some(yaw) = options.yaw {
        app.camera.yaw = yaw.to_radians();
    }
    if let Some(pitch) = options.pitch {
        app.camera.orbit(0.0, pitch.to_radians() - app.camera.pitch);
    }
    app.camera.zoom(options.zoom);
    // Let the air come to life: run the particles frame by frame up to the requested time.
    let target = [app.camera.target.x, app.camera.target.z];
    let frames = (options.time / CAPTURE_STEP) as usize;
    for frame in 0..frames {
        let time = frame as f32 * CAPTURE_STEP;
        app.ambient.update(CAPTURE_STEP, time, target, &app.world);
    }
    let mut renderer = Renderer::new(Gpu::offscreen(options.width, options.height));
    app.data.install(&mut renderer);
    renderer.upload_particles(app.ambient.instances());
    renderer.render(&app.camera, options.time);
    let (width, height, pixels) = renderer
        .capture()
        .ok_or("pas d'image hors écran à relire")?;
    std::fs::write(
        &options.path,
        render::png::encode_png(width, height, &pixels),
    )
    .map_err(|e| format!("{} : {e}", options.path))?;
    println!("Image enregistrée : {} ({width}×{height})", options.path);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match Options::from_args(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("Option invalide : {message}");
            std::process::exit(2);
        }
    };
    if let Some(capture_options) = &options.capture {
        if let Err(message) = capture(options.seed, capture_options) {
            eprintln!("Capture impossible : {message}");
            std::process::exit(1);
        }
        return;
    }
    let event_loop = EventLoop::new().expect("création de la boucle d'événements");
    let mut app = App::new(options.seed);
    event_loop
        .run_app(&mut app)
        .expect("exécution de la boucle d'événements");
}
