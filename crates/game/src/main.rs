//! Binaire jouable. Un naturaliste se promène dans un monde procédural (relief, mer, plages,
//! déserts, forêts, montagnes, rivières, lacs), suivi par la caméra plongeante. Le feuillage
//! ondule au vent, du pollen flotte dans la lumière, des feuilles tombent.
//! Commandes (touches par position, ZQSD sur un clavier AZERTY) : Z Q S D pour marcher,
//! Maj pour courir, Espace pour sauter, E pour ramasser, 1 à 8 pour choisir dans le sac,
//! F pour manger, B pour boire ; glisser avec le bouton gauche pour tourner la caméra, molette
//! pour zoomer. T (maintenu) accélère la journée, R lance ou arrête la pluie.
//! `--seed N` choisit le monde ; `--capture fichier.png` enregistre une vue sans fenêtre.

mod ambient;
mod anomaly;
mod audio;
mod clock;
mod fauna;
mod hud;
mod items;
mod listen;
mod naturalist;
mod needs;
mod obstacles;
mod player;
mod scene;
mod sound;
mod state;
mod traces;
mod trample;
mod weather;

use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use render::{Gpu, OrbitCamera, Renderer};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};
use world::{World, WorldConfig};

use ambient::Ambient;
use audio::Audio;
use clock::Clock;
use fauna::Fauna;
use naturalist::Naturalist;
use player::Controls;
use scene::SceneData;
use state::{Command, Event, Failure, GameState, PlayerId};
use traces::Traces;
use trample::Trample;
use weather::Weather;

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
/// The camera looks at this point above the feet (about the chest).
const LOOK_HEIGHT: f32 = 1.2;
/// How fast the camera catches up with the player (1/s): a slight, smooth lag.
const FOLLOW_EASING: f32 = 6.0;
/// Radius of the scene framed around the player at start (sets the camera distance).
const FOLLOW_FRAME: f32 = 7.0;

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
    fauna: Fauna,
    traces: Traces,
    weather: Weather,
    /// Ambient and fauna particles together, rebuilt each frame.
    particles: Vec<render::ParticleInstance>,
    /// What every player shares (bodies, bags, needs, what was taken from the world).
    state: GameState,
    /// The local player.
    me: PlayerId,
    /// Commands from the keys, applied at the next fixed step.
    pending: Vec<Command>,
    /// Frame time not yet turned into fixed steps of the game, in seconds.
    accumulator: f32,
    /// Selected slot of the bag.
    selected: usize,
    /// A short message for the player, and the seconds it stays.
    message: Option<(String, f32)>,
    trample: Trample,
    /// How the anomaly is drawn.
    look: anomaly::Look,
    controls: Controls,
    clock: Clock,
    /// None when there is no sound (no device, or a capture).
    audio: Option<Audio>,
    /// Seconds until the ambient sounds are measured again.
    listen_in: f32,
    /// Walking cycle phase at the previous frame (footsteps fall when it crosses 0 or π).
    last_stride_phase: f32,
    naturalist: Naturalist,
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
        let ambient = Ambient::new(&world, seed ^ AMBIENT_SEED);
        let mut state = GameState::new(&world);
        let spawn = player::spawn_point(&world);
        let me = state.join(spawn);
        let camera = OrbitCamera::framing(spawn + Vec3::Y * LOOK_HEIGHT, FOLLOW_FRAME);
        // The anomaly grows in front of the first view.
        let ahead = glam::Vec2::new(-camera.yaw.sin(), -camera.yaw.cos());
        let covered = state.grow_anomaly(&world, spawn, ahead, seed ^ 0xa40);
        let mut data = data;
        for plant in covered {
            data.hide(plant);
        }
        if let Some(anomaly) = state.anomaly() {
            let o = anomaly.origin();
            println!(
                "Une anomalie pousse dans une clairière vers ({:.0}, {:.0}).",
                o.x, o.z
            );
        }
        let trample = Trample::new(data.pliable());
        Self {
            graphics: None,
            world,
            data,
            ambient,
            fauna: Fauna::new(seed ^ 0xfa_0a),
            traces: Traces::new(seed ^ 0x7ace),
            weather: Weather::new(seed ^ 0x3a7e),
            particles: Vec::new(),
            state,
            me,
            pending: Vec::new(),
            accumulator: 0.0,
            selected: 0,
            message: None,
            trample,
            look: anomaly::Look::Carpet,
            controls: Controls::default(),
            clock: Clock::new(clock::START_HOUR),
            audio: None,
            listen_in: 0.0,
            last_stride_phase: 0.0,
            naturalist: Naturalist::new(),
            start,
            last_frame: Instant::now(),
            camera,
            dragging: false,
            last_cursor: None,
        }
    }
}

impl App {
    /// One frame of the world: the player moves, the camera follows, the air lives.
    fn tick(&mut self, dt: f32, time: f32) {
        self.clock.advance(dt);
        self.run_steps(dt);
        if let Some((_, left)) = &mut self.message {
            *left -= dt;
            if *left <= 0.0 {
                self.message = None;
            }
        }
        let body = self.state.body(self.me);
        let (feet, velocity) = (body.position, body.velocity());
        let goal = body.shown_position() + Vec3::Y * LOOK_HEIGHT;
        self.camera.target += (goal - self.camera.target) * (1.0 - (-FOLLOW_EASING * dt).exp());
        let around = [feet.x, feet.z];
        self.ambient.update(dt, time, around, &self.world);
        let hour = self.clock.hour();
        self.weather.update(dt, hour, feet, &self.world);
        let night = render::sky::sky(hour).night;
        let rain = self.weather.rain();
        self.fauna.update(dt, time, feet, night, rain, &self.world);
        self.particles.clear();
        self.particles.extend_from_slice(self.ambient.instances());
        self.particles.extend_from_slice(self.fauna.particles());
        self.particles.extend_from_slice(self.traces.instances());
        self.particles.extend_from_slice(self.weather.particles());
        if let Some(anomaly) = self.state.anomaly() {
            anomaly.draw(&self.world, night, self.look, &mut self.particles);
        }
        self.update_sound(dt, time);
        // Plants pushed aside by the body, springing back once free.
        let data = &mut self.data;
        self.trample
            .update(dt, feet, velocity, |plant, bend| data.set_bend(plant, bend));
    }

    /// Turns frame time into fixed steps of the game: each step, the keys held become a
    /// steering command, pending commands are applied, then time advances by `state::STEP`.
    fn run_steps(&mut self, dt: f32) {
        // After a long pause, do not try to catch up more than a quarter of a second.
        self.accumulator = (self.accumulator + dt).min(0.25);
        let hour = self.clock.hour();
        let rain = self.weather.rain();
        while self.accumulator >= state::STEP {
            self.accumulator -= state::STEP;
            let steer = Command::Steer {
                player: self.me,
                controls: self.controls,
                camera_yaw: self.camera.yaw,
            };
            self.state.apply(&self.world, steer);
            // A jump is one press, not held.
            self.controls.jump = false;
            for command in std::mem::take(&mut self.pending) {
                if let Some(event) = self.state.apply(&self.world, command) {
                    self.on_event(event);
                }
            }
            self.state.step(&self.world, hour, rain);
        }
    }

    /// What the presentation does when something happens in the game.
    fn on_event(&mut self, event: Event) {
        let text = match event {
            Event::Picked {
                matter, removed, ..
            } => {
                if let Some(plant) = removed {
                    self.data.hide(plant);
                }
                if let Some(audio) = &self.audio {
                    audio.shared.step(sound::Surface::Grass, 0.25);
                }
                format!("+ {}", matter.name())
            }
            Event::Ate { matter, .. } => format!("Vous mangez : {}", matter.name()),
            Event::Drank { .. } => "Vous buvez quelques gorgées.".to_owned(),
            Event::Laid { matter, .. } => format!("Vous posez : {}", matter.name()),
            Event::Failed { failure, .. } => match failure {
                Failure::NothingInReach => "Rien à ramasser à portée de main.",
                Failure::Bag(items::Refusal::TooHeavy) => "Le sac est trop lourd.",
                Failure::Bag(items::Refusal::Full) => "Le sac est plein.",
                Failure::NotEdible => "Ça ne se mange pas.",
                Failure::NoWater => "Pas d'eau à portée.",
                Failure::CannotLay => "Un caillou se pose seulement sur le tapis.",
            }
            .to_owned(),
        };
        self.message = Some((text, 2.5));
    }

    /// Ambient levels a few times a second; at each footstep, its sound and its trace.
    fn update_sound(&mut self, dt: f32, time: f32) {
        self.listen_in -= dt;
        if self.listen_in <= 0.0 {
            self.listen_in = 0.25;
            if let Some(audio) = &self.audio {
                let hour = self.clock.hour();
                let night = render::sky::sky(hour).night;
                let rain = self.weather.rain();
                let scene = listen::hear(
                    &self.world,
                    self.state.body(self.me).position,
                    time,
                    hour,
                    night,
                    rain,
                );
                audio.shared.set_scene(&scene);
            }
        }
        let feet = self.state.body(self.me).position;
        let level = listen::water_level_at(&self.world, feet);
        // A foot touches the ground each time the walking cycle passes 0 or π.
        let (phase, swing) = self.state.body(self.me).stride();
        let half = std::f32::consts::PI;
        let crossed = (self.last_stride_phase % half) > (phase % half);
        self.last_stride_phase = phase;
        let grounded = self.state.body(self.me).on_ground() || self.state.body(self.me).in_water();
        if crossed && swing > 0.15 && grounded {
            let in_water = self.state.body(self.me).in_water();
            let surface = listen::surface(&self.world, feet, in_water);
            let strength = (0.3 + 0.7 * swing).min(1.0);
            if let Some(audio) = &self.audio {
                audio.shared.step(surface, strength);
            }
            let soft = !in_water && listen::soft_ground(&self.world, feet);
            let facing = self.state.body(self.me).facing();
            self.traces
                .step(feet, facing, surface, strength, level, soft);
        }
        if let Some(level) = level
            && self.state.body(self.me).in_water()
            && swing > 0.1
        {
            self.traces.swim(Vec3::new(feet.x, level, feet.z), dt);
        }
        self.traces.update(dt, time);
    }

    fn handle_key(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        // Physical keys: the same positions on every layout (Z Q S D on AZERTY).
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        match code {
            KeyCode::KeyW | KeyCode::ArrowUp => self.controls.forward = pressed,
            KeyCode::KeyS | KeyCode::ArrowDown => self.controls.back = pressed,
            KeyCode::KeyA | KeyCode::ArrowLeft => self.controls.left = pressed,
            KeyCode::KeyD | KeyCode::ArrowRight => self.controls.right = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.controls.run = pressed,
            KeyCode::Space if pressed && !event.repeat => self.controls.jump = true,
            // Hold T to fast-forward the day.
            KeyCode::KeyT => self.clock.fast = pressed,
            // R: rain on / off (for testing).
            KeyCode::KeyR if pressed && !event.repeat => self.weather.forced = !self.weather.forced,
            KeyCode::KeyE if pressed && !event.repeat => {
                self.pending.push(Command::Pick { player: self.me });
            }
            KeyCode::KeyF if pressed && !event.repeat => {
                self.pending.push(Command::Eat {
                    player: self.me,
                    slot: self.selected,
                });
            }
            KeyCode::KeyP if pressed && !event.repeat => {
                self.pending.push(Command::Lay {
                    player: self.me,
                    slot: self.selected,
                });
            }
            KeyCode::KeyB if pressed && !event.repeat => {
                self.pending.push(Command::Drink { player: self.me });
            }
            KeyCode::Digit1 => self.selected = 0,
            KeyCode::Digit2 => self.selected = 1,
            KeyCode::Digit3 => self.selected = 2,
            KeyCode::Digit4 => self.selected = 3,
            KeyCode::Digit5 => self.selected = 4,
            KeyCode::Digit6 => self.selected = 5,
            KeyCode::Digit7 => self.selected = 6,
            KeyCode::Digit8 => self.selected = 7,
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
        self.data.install(&mut renderer);
        self.naturalist.install(&mut renderer);
        self.fauna.install(&mut renderer);
        self.weather.install(&mut renderer);
        self.graphics = Some(Graphics { window, renderer });
        match Audio::start(0x5eed) {
            Ok(audio) => self.audio = Some(audio),
            Err(message) => eprintln!("Son désactivé : {message}"),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.graphics.is_none() {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                // Clamped: after a pause of the window (dragging, sleep), no huge jump.
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;
                let time = (now - self.start).as_secs_f32();
                self.tick(dt, time);
                if let Some(graphics) = self.graphics.as_mut() {
                    let renderer = &mut graphics.renderer;
                    self.naturalist
                        .pose(renderer, &self.state.body(self.me).motion(time));
                    self.data.upload_changes(renderer);
                    self.weather.draw(renderer);
                    set_sky(
                        &self.clock,
                        &self.weather,
                        self.state.body(self.me).position,
                        renderer,
                    );
                    self.fauna.draw(renderer);
                    draw_hud(
                        &self.state,
                        &self.world,
                        self.me,
                        self.selected,
                        self.clock.hour(),
                        &self.message,
                        renderer,
                    );
                    let (ripples, prints) = self.traces.marks();
                    renderer.set_marks(ripples, prints);
                    renderer.upload_particles(&self.particles);
                    renderer.render(&self.camera, time);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(&event),
            WindowEvent::Focused(false) => {
                // Keys released while the window is not focused would stay pressed.
                self.controls = Controls::default();
            }
            WindowEvent::Resized(size) => {
                if let Some(graphics) = self.graphics.as_mut() {
                    graphics.renderer.resize(size.width, size.height);
                }
            }
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

/// The interface over the image, and the weariness of the local player.
fn draw_hud(
    state: &GameState,
    world: &World,
    me: PlayerId,
    selected: usize,
    hour: f32,
    message: &Option<(String, f32)>,
    renderer: &mut Renderer,
) {
    let (width, height) = renderer.size();
    let mut ui = render::ui::Ui::new(width, height);
    hud::build(
        &mut ui,
        &hud::HudInput {
            state,
            world,
            me,
            selected,
            hour,
            message: message.as_ref().map(|(text, left)| (text.as_str(), *left)),
        },
    );
    renderer.set_ui(ui.vertices());
    let weariness = state.player(me).map_or(0.0, |p| p.needs.weariness());
    renderer.set_weariness(weariness);
}

/// Light and weather of the moment, to the renderer.
fn set_sky(clock: &Clock, weather: &Weather, feet: Vec3, renderer: &mut Renderer) {
    let hour = clock.hour();
    let mut sky = render::sky::sky(hour);
    weather.apply(&mut sky, hour);
    renderer.set_sky(&sky);
    // Mist lies a little below the naturalist: in the hollows around and over the water.
    renderer.set_weather(weather.mist(hour), weather.wet(), feet.y - 2.0);
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
    /// Seconds the naturalist walks forward before the picture.
    walk: f32,
    /// Hour of the day (0 to 24).
    hour: f32,
    /// Raining from the start.
    rain: bool,
    /// Where the naturalist starts (x, z), instead of the spawn point.
    start: Option<(f32, f32)>,
    /// Times the naturalist tries to pick something up before the picture.
    pick: u32,
}

/// `--anomaly-look tapis|tiges|lichen|lueurs`.
fn look_option(args: &[String]) -> Result<Option<anomaly::Look>, String> {
    let Some(i) = args.iter().position(|a| a == "--anomaly-look") else {
        return Ok(None);
    };
    let name = args.get(i + 1).map(String::as_str).unwrap_or("");
    anomaly::Look::parse(name)
        .map(Some)
        .ok_or_else(|| format!("--anomaly-look {name} : attendu tapis, tiges, lichen ou lueurs"))
}

impl Options {
    /// `[--seed N] [--capture file.png [--size WxH] [--yaw DEG] [--pitch DEG] [--zoom F]
    /// [--at X,Z] [--time SECONDS] [--walk SECONDS] [--hour H] [--weather rain] [--start X,Z] [--pick N]]`.
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
                    walk: number("--walk")?.unwrap_or(0.0),
                    hour: number("--hour")?.unwrap_or(clock::START_HOUR),
                    rain: value("--weather") == Some("rain"),
                    start: pair("--start", ',')?,
                    pick: number("--pick")?.unwrap_or(0.0) as u32,
                })
            }
            None => None,
        };
        Ok(Self { seed, capture })
    }
}

/// Generates the world, renders one frame offscreen, saves it as PNG.
fn capture(seed: u64, look: anomaly::Look, options: &CaptureOptions) -> Result<(), String> {
    let mut app = App::new(seed);
    app.look = look;
    app.clock = Clock::new(options.hour);
    if let Some((x, z)) = options.start {
        let dims = app.world.dims();
        let (xi, zi) = (
            x.clamp(0.0, dims.nx as f32 - 1.0),
            z.clamp(0.0, dims.nz as f32 - 1.0),
        );
        let y = app.world.ground_top(xi as usize, zi as usize) as f32 + 0.001;
        app.state.place(app.me, Vec3::new(x, y, z));
        app.camera.target = Vec3::new(x, y, z) + Vec3::Y * LOOK_HEIGHT;
    }
    if options.rain {
        app.weather.forced = true;
        app.weather.start_shower();
    }
    if let Some(yaw) = options.yaw {
        app.camera.yaw = yaw.to_radians();
    }
    if let Some(pitch) = options.pitch {
        app.camera.orbit(0.0, pitch.to_radians() - app.camera.pitch);
    }
    app.camera.zoom(options.zoom);
    // Run the world frame by frame: standing still for `time`, then walking for `walk`.
    let still = (options.time / CAPTURE_STEP) as usize;
    let walking = (options.walk / CAPTURE_STEP) as usize;
    for frame in 0..still + walking {
        app.controls.forward = frame >= still;
        app.tick(CAPTURE_STEP, frame as f32 * CAPTURE_STEP);
    }
    // Picking up, a step apart.
    for _ in 0..options.pick {
        app.pending.push(Command::Pick { player: app.me });
        app.tick(CAPTURE_STEP, (still + walking) as f32 * CAPTURE_STEP);
    }
    // `--at` overrides the follow camera.
    if let Some((x, z)) = options.at {
        app.camera.target = Vec3::new(x, app.camera.target.y, z);
    }
    let time = (still + walking) as f32 * CAPTURE_STEP;
    let mut renderer = Renderer::new(Gpu::offscreen(options.width, options.height));
    app.data.install(&mut renderer);
    app.naturalist.install(&mut renderer);
    app.naturalist
        .pose(&mut renderer, &app.state.body(app.me).motion(time));
    app.data.upload_changes(&mut renderer);
    app.weather.install(&mut renderer);
    app.weather.draw(&mut renderer);
    set_sky(
        &app.clock,
        &app.weather,
        app.state.body(app.me).position,
        &mut renderer,
    );
    app.fauna.install(&mut renderer);
    app.fauna.draw(&mut renderer);
    draw_hud(
        &app.state,
        &app.world,
        app.me,
        app.selected,
        app.clock.hour(),
        &app.message,
        &mut renderer,
    );
    let (ripples, prints) = app.traces.marks();
    renderer.set_marks(ripples, prints);
    renderer.upload_particles(&app.particles);
    renderer.render(&app.camera, time);
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
    // `--sound-demo DIR`: one WAV file per kind of sound, to listen to them one by one.
    if let Some(i) = args.iter().position(|a| a == "--sound-demo") {
        let dir = args.get(i + 1).map_or("sons", String::as_str);
        match sound::demo(std::path::Path::new(dir)) {
            Ok(files) => files.iter().for_each(|f| println!("Son écrit : {f}")),
            Err(e) => eprintln!("Écriture des sons impossible : {e}"),
        }
        return;
    }
    let options = match Options::from_args(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("Option invalide : {message}");
            std::process::exit(2);
        }
    };
    let look = match look_option(&args) {
        Ok(look) => look.unwrap_or(anomaly::Look::Carpet),
        Err(message) => {
            eprintln!("Option invalide : {message}");
            std::process::exit(2);
        }
    };
    if let Some(capture_options) = &options.capture {
        if let Err(message) = capture(options.seed, look, capture_options) {
            eprintln!("Capture impossible : {message}");
            std::process::exit(1);
        }
        return;
    }
    let event_loop = EventLoop::new().expect("création de la boucle d'événements");
    let mut app = App::new(options.seed);
    app.look = look;
    event_loop
        .run_app(&mut app)
        .expect("exécution de la boucle d'événements");
}
