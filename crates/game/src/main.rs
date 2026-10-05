//! Binaire jouable. Pour l'instant : une terre vivante. Le terrain ne change pas de forme ;
//! une réaction Gray-Scott tourne dans sa couche de surface et se lit dans la couleur du sol.
//! Commandes : glisser avec le bouton gauche pour tourner, molette pour zoomer,
//! Espace pour mettre en pause, R pour recommencer.
//! Prochaine étape : les réglages du joueur (flux F et dissipation k).

mod terrain;

use std::sync::Arc;

use glam::Vec3;
use render::mesh::MeshData;
use render::mesher::mesh_field;
use render::palette;
use render::{Gpu, LifeStyle, OrbitCamera, Renderer, VolumeId, VolumeStyle};
use sim::gray_scott::{Boundary, GrayScott, GrayScottParams};
use sim::grid::{Dims, Field3};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Radians of rotation per pixel of mouse drag.
const ORBIT_SPEED: f32 = 0.005;
/// Distance factor per wheel notch.
const ZOOM_STEP: f32 = 0.9;
/// Pixels of trackpad scroll counted as one wheel notch.
const PIXELS_PER_NOTCH: f32 = 50.0;

/// Size of the world, in cells.
const WORLD: Dims = Dims {
    nx: 72,
    ny: 16,
    nz: 72,
};
const TERRAIN_SEED: u64 = 7;
/// Initial seeds of life, as in docs/reactions/gray-scott.md (critical nucleus: 5 wide).
const SEED_COUNT: usize = 8;
const SEED_SIZE: usize = 5;
const SIM_SEED: u64 = 1;
/// Simulation steps per displayed frame. The surface layer is small (72×72), steps are cheap.
const STEPS_PER_FRAME: usize = 8;

/// How life shows on the ground, from the activator v (2D runs: v up to ~0.4).
/// The ground starts to be tinted at v = 0.05 and is fully covered at v = 0.2;
/// the living colour goes from plum (edge of a patch) to gold (its core).
fn life_style() -> LifeStyle {
    LifeStyle {
        palette: palette::concentration(),
        fade: (0.05, 0.2),
        value_range: (0.1, 0.4),
    }
}

/// Window and renderer are created together on `resumed`, so they are either both present or both absent.
struct Graphics {
    // `Arc` because the wgpu surface keeps its own reference to the window:
    // the window must live at least as long as the surface.
    window: Arc<Window>,
    renderer: Renderer,
    ground: VolumeId,
}

struct App {
    graphics: Option<Graphics>,
    terrain: terrain::Terrain,
    /// The living surface layer: one simulated cell per column of ground.
    sim: GrayScott,
    /// Life shown in the world grid: v copied onto the top cell of each column, 0 elsewhere.
    /// Kept between frames so updating it does not allocate.
    life: Field3,
    paused: bool,
    camera: OrbitCamera,
    dragging: bool,
    last_cursor: Option<PhysicalPosition<f64>>,
}

impl App {
    fn new() -> Self {
        let terrain = terrain::generate(WORLD, TERRAIN_SEED);
        // A skin over the ground: a 2D grid (height 1). Watertight edges: nothing leaves
        // the world. For now diffusion ignores the relief.
        let skin = Dims {
            nx: WORLD.nx,
            ny: 1,
            nz: WORLD.nz,
        };
        let mut sim = GrayScott::new(skin, GrayScottParams::reference(), Boundary::NoFlux);
        sim.reset_with_seeds(SEED_COUNT, SEED_SIZE, SIM_SEED);

        let centre = Vec3::new(WORLD.nx as f32 / 2.0, 6.0, WORLD.nz as f32 / 2.0);
        let radius = 0.55 * WORLD.nx.max(WORLD.nz) as f32;
        Self {
            graphics: None,
            terrain,
            sim,
            life: Field3::filled(WORLD, 0.0),
            paused: false,
            camera: OrbitCamera::framing(centre, radius),
            dragging: false,
            last_cursor: None,
        }
    }

    /// Copies the activator of the surface layer onto the top cell of each column.
    fn update_life(&mut self) {
        let v = self.sim.v();
        for z in 0..WORLD.nz {
            for x in 0..WORLD.nx {
                let [_, y, _] = self.terrain.surface(x, z);
                self.life.set(x, y, z, v.get(x, 0, z));
            }
        }
    }

    fn handle_key(&mut self, event: &KeyEvent) {
        if event.state != ElementState::Pressed || event.repeat {
            return;
        }
        // Physical keys: the same position on every keyboard layout.
        match event.physical_key {
            PhysicalKey::Code(KeyCode::Space) => self.paused = !self.paused,
            PhysicalKey::Code(KeyCode::KeyR) => {
                self.sim.reset_with_seeds(SEED_COUNT, SEED_SIZE, SIM_SEED)
            }
            _ => {}
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

        // The shape of the ground never changes: meshed and uploaded once. Only its life
        // field is uploaded every frame.
        let ground = renderer.add_volume(&VolumeStyle {
            origin: [0.0; 3],
            palette: palette::earth(),
            value_range: (0.0, 1.0),
            life: life_style(),
        });
        let mut mesh = MeshData::default();
        mesh_field(&self.terrain.solid, 0.5, &mut mesh);
        renderer.upload_mesh(ground, &mesh);
        renderer.upload_base(ground, &self.terrain.color);

        self.graphics = Some(Graphics {
            window,
            renderer,
            ground,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.graphics.is_none() {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if !self.paused {
                    for _ in 0..STEPS_PER_FRAME {
                        self.sim.step();
                    }
                }
                self.update_life();
                if let Some(graphics) = self.graphics.as_mut() {
                    graphics.renderer.upload_life(graphics.ground, &self.life);
                    graphics.renderer.render(&self.camera);
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(graphics) = self.graphics.as_mut() {
                    graphics.renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(&event),
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

fn main() {
    let event_loop = EventLoop::new().expect("création de la boucle d'événements");
    let mut app = App::new();
    event_loop
        .run_app(&mut app)
        .expect("exécution de la boucle d'événements");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn life_lands_on_the_surface_only() {
        let mut app = App::new();
        for _ in 0..500 {
            app.sim.step();
        }
        app.update_life();
        for z in 0..WORLD.nz {
            for x in 0..WORLD.nx {
                let [_, top, _] = app.terrain.surface(x, z);
                assert_eq!(app.life.get(x, top, z), app.sim.v().get(x, 0, z));
                for y in (0..WORLD.ny).filter(|&y| y != top) {
                    assert_eq!(
                        app.life.get(x, y, z),
                        0.0,
                        "life below or above the surface"
                    );
                }
            }
        }
    }
}
