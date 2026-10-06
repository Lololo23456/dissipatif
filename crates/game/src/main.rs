//! Binaire jouable. Un naturaliste se promène dans un monde procédural (relief, mer, plages,
//! déserts, forêts, montagnes, rivières, lacs), suivi par la caméra plongeante. Le feuillage
//! ondule au vent, du pollen flotte dans la lumière, des feuilles tombent.
//! Commandes (touches par position, ZQSD sur un clavier AZERTY) : Z Q S D pour marcher, Maj
//! pour courir, Espace pour sauter. Souris : clic gauche pose ce qu'on tient là où pointe le
//! curseur, clic droit le reprend (ou cueille), glisser avec le bouton droit tourne la
//! caméra, molette pour zoomer. Clavier : E ramasser, P poser devant soi, 1 à 8 choisir dans
//! le sac, F manger ou modeler, B boire, G (maintenu) frotter le foret à feu ou souffler sur
//! la braise. T (maintenu) accélère la journée, R lance ou arrête la pluie.
//! `--seed N` choisit le monde ; `--capture fichier.png` enregistre une vue sans fenêtre.

mod ambient;
mod audio;
mod clock;
mod fauna;
mod hud;
mod items;
mod listen;
mod naturalist;
mod needs;
mod objects;
mod objects_view;
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
use items::Matter;
use naturalist::{Gesture, GestureKind, Naturalist};
use objects_view::ObjectsView;
use player::Controls;
use scene::SceneData;
use state::{Command, Event, Failure, GameState, PlayerId};
use traces::Traces;
use trample::Trample;
use weather::Weather;

/// Pixels the cursor must move, button held, to be a drag rather than a click.
const DRAG_THRESHOLD: f64 = 6.0;
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
    objects_view: ObjectsView,
    trample: Trample,
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
    /// Where the right button went down (a click or the start of a drag).
    right_down: Option<PhysicalPosition<f64>>,
    /// The ground point under the cursor, updated each frame.
    hover: Option<Vec3>,
    /// The bag is open (Tab or I).
    bag_open: bool,
    /// Shift is held.
    shift: bool,
    /// Cells dug since the last frame (and whether water flowed in): their chunks are
    /// remeshed before the next frame.
    dug_cells: Vec<([usize; 3], bool)>,
    /// A one-off gesture being played (laying, eating…) and when it began.
    gesture: Option<(GestureKind, f32)>,
    /// Seconds since start, as of the last frame.
    now: f32,
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
            objects_view: ObjectsView::new(),
            trample,
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
            right_down: None,
            hover: None,
            bag_open: false,
            shift: false,
            dug_cells: Vec::new(),
            gesture: None,
            now: 0.0,
        }
    }
}

impl App {
    /// One frame of the world: the player moves, the camera follows, the air lives.
    fn tick(&mut self, dt: f32, time: f32) {
        self.now = time;
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
        // What the physics did meanwhile (an ember, a dish fired or burst…).
        let events: Vec<Event> = self.state.drain_events().collect();
        for event in events {
            self.on_event(event);
        }
    }

    /// What the hands are doing now: a one-off gesture (rising then falling over its
    /// duration), or rubbing or blowing while the key is held.
    fn current_gesture(&self, time: f32) -> Option<Gesture> {
        if let Some((kind, start)) = self.gesture {
            let duration = match kind {
                GestureKind::Drink => 1.4,
                GestureKind::Eat | GestureKind::Shape => 1.0,
                _ => 0.7,
            };
            let t = (time - start) / duration;
            if (0.0..1.0).contains(&t) {
                return Some(Gesture {
                    kind,
                    amount: (t * std::f32::consts::PI).sin(),
                });
            }
        }
        if self.controls.rub {
            let kind = if self.state.would_blow(self.me) {
                GestureKind::Blow
            } else {
                GestureKind::Rub
            };
            return Some(Gesture { kind, amount: 1.0 });
        }
        None
    }

    /// What the presentation does when something happens in the game.
    fn on_event(&mut self, event: Event) {
        let gesture = match event {
            Event::Laid { .. } | Event::TookBack { .. } | Event::Picked { .. } => {
                Some(GestureKind::Reach)
            }
            Event::Ate { .. } => Some(GestureKind::Eat),
            Event::Drank { .. } => Some(GestureKind::Drink),
            Event::Worked { .. } => Some(GestureKind::Shape),
            _ => None,
        };
        if let Some(kind) = gesture {
            self.gesture = Some((kind, self.now));
        }
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
            Event::Dig { at } => {
                if let Some(dug) = self.world.dig(at.x, at.y) {
                    self.dug_cells.push((dug.cell, dug.flooded));
                }
                return;
            }
            Event::Cleared { plant } => {
                self.data.hide(plant);
                return;
            }
            Event::Laid { matter, .. } => format!("Vous posez : {}", matter.name()),
            Event::TookBack { matter, .. } => format!("Vous reprenez : {}", matter.name()),
            Event::Worked { work, made, .. } => match (work, made) {
                (state::Work::Knap, Matter::Flake) => {
                    "La pierre se fend net : un éclat tranchant.".to_owned()
                }
                (state::Work::Knap, _) => "La pierre s'effrite en débris.".to_owned(),
                (state::Work::Haft, _) => {
                    "L'éclat tient sur la baguette, ligaturé : un couteau.".to_owned()
                }
                _ => format!("Vous obtenez : {}", made.name()),
            },
            Event::Ember { .. } => "À force de frotter, une braise tombe.".to_owned(),
            Event::Changed { from, to } => match (from, to) {
                (Matter::RawDish { .. }, Matter::Shards) => {
                    "Crac ! La coupelle encore humide éclate dans le feu.".to_owned()
                }
                (Matter::RawDish { .. }, Matter::FiredDish { .. }) => {
                    "La coupelle a cuit : de la terre cuite.".to_owned()
                }
                (_, Matter::Ash) => format!("{} : il n'en reste que des cendres.", from.name()),
                _ => format!("{} devient {}", from.name(), to.name()),
            },
            Event::Failed { failure, .. } => match failure {
                Failure::NothingInReach => "Rien à ramasser à portée de main.",
                Failure::Bag(items::Refusal::TooHeavy) => "Le sac est trop lourd.",
                Failure::Bag(items::Refusal::Full) => "Le sac est plein.",
                Failure::NotEdible => "Ça ne se mange pas.",
                Failure::NoWater => "Pas d'eau à portée.",
                Failure::TooHot => "Trop chaud pour le prendre à la main.",
                Failure::CannotLay => "Impossible de poser ici.",
                Failure::NothingToWork => "Rien à travailler avec ce que vous tenez.",
                Failure::TooWet => "Le bois est trop humide : la friction ne donne rien.",
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
            KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                self.controls.run = pressed;
                self.shift = pressed;
            }
            KeyCode::Tab | KeyCode::KeyI if pressed && !event.repeat => {
                self.bag_open = !self.bag_open;
            }
            KeyCode::Escape if pressed => self.bag_open = false,
            // X: throw one of the selected thing on the ground in front; Shift+X: all of them.
            KeyCode::KeyX if pressed && !event.repeat => {
                let count = self
                    .state
                    .player(self.me)
                    .and_then(|p| p.inventory.stacks().get(self.selected).map(|s| s.count))
                    .unwrap_or(0);
                let times = if self.shift { count } else { count.min(1) };
                for _ in 0..times {
                    self.pending.push(Command::Lay {
                        player: self.me,
                        slot: self.selected,
                    });
                }
            }
            KeyCode::Space if pressed && !event.repeat => self.controls.jump = true,
            // Hold T to fast-forward the day.
            KeyCode::KeyT => self.clock.fast = pressed,
            // R: rain on / off (for testing).
            KeyCode::KeyR if pressed && !event.repeat => self.weather.forced = !self.weather.forced,
            KeyCode::KeyE if pressed && !event.repeat => {
                self.pending.push(Command::Pick { player: self.me });
            }
            KeyCode::KeyF if pressed && !event.repeat => {
                // In the hands: work what can be worked, eat what can be eaten.
                let (player, slot) = (self.me, self.selected);
                let command = if self.state.work_plan(player, slot).is_some() {
                    Command::Work { player, slot }
                } else {
                    Command::Eat { player, slot }
                };
                self.pending.push(command);
            }
            KeyCode::KeyP if pressed && !event.repeat => {
                self.pending.push(Command::Lay {
                    player: self.me,
                    slot: self.selected,
                });
            }
            // Hold G: rub a fire drill.
            KeyCode::KeyG => self.controls.rub = pressed,
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
        self.objects_view.install(&mut renderer);
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
                let mut motion = self.state.body(self.me).motion(time);
                motion.gesture = self.current_gesture(time);
                if let Some(graphics) = self.graphics.as_mut() {
                    let renderer = &mut graphics.renderer;
                    self.naturalist.pose(renderer, &motion);
                    for (cell, flooded) in std::mem::take(&mut self.dug_cells) {
                        self.data
                            .ground_dug(&self.world, cell, flooded, Some(renderer));
                    }
                    self.data.upload_changes(renderer);
                    self.weather.draw(renderer);
                    set_sky(
                        &self.clock,
                        &self.weather,
                        self.state.body(self.me).position,
                        renderer,
                    );
                    self.fauna.draw(renderer);
                    self.objects_view.draw(
                        self.state.objects(),
                        time,
                        renderer,
                        &mut self.particles,
                    );
                    draw_hud(
                        &self.state,
                        &self.world,
                        self.me,
                        self.selected,
                        self.clock.hour(),
                        &self.message,
                        self.bag_open,
                        renderer,
                    );
                    let (ripples, prints) = self.traces.marks();
                    renderer.set_marks(ripples, prints);
                    // What the cursor points at, and a small mark there.
                    let (width, height) = renderer.size();
                    self.hover = self.last_cursor.and_then(|c| {
                        let ndc = glam::Vec2::new(
                            (2.0 * c.x / width as f64 - 1.0) as f32,
                            (1.0 - 2.0 * c.y / height as f64) as f32,
                        );
                        pointed_ground(&self.world, &self.camera, renderer.aspect(), ndc)
                    });
                    if let Some(at) = self.hover {
                        let laying = self
                            .state
                            .player(self.me)
                            .is_some_and(|p| p.inventory.stacks().get(self.selected).is_some())
                            && self
                                .state
                                .lay_point_at(&self.world, self.me, glam::Vec2::new(at.x, at.z))
                                .is_some();
                        let (color, glow) = if laying {
                            ([1.0, 0.92, 0.75], -0.8)
                        } else {
                            ([0.5, 0.5, 0.5], 0.0)
                        };
                        self.particles.push(render::ParticleInstance {
                            centre_size: [at.x, at.y + 0.02, at.z, 0.06],
                            color: [color[0], color[1], color[2], glow],
                        });
                    }
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
            // Left click: in the open bag, choose a thing; otherwise lay what is in hand
            // where the cursor points.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if self.bag_open {
                    if let (Some(c), Some(graphics)) = (self.last_cursor, &self.graphics) {
                        let (w, h) = graphics.renderer.size();
                        if let Some(k) =
                            hud::bag_slot_at(w as f32, h as f32, c.x as f32, c.y as f32)
                        {
                            self.selected = k;
                        }
                    }
                } else if let Some(at) = self.hover {
                    self.pending.push(Command::LayAt {
                        player: self.me,
                        slot: self.selected,
                        at: glam::Vec2::new(at.x, at.z),
                    });
                }
            }
            // Right button: a click takes what is under the cursor, a drag turns the camera.
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => {
                if state == ElementState::Pressed {
                    self.right_down = self.last_cursor;
                    self.dragging = false;
                } else {
                    if !self.dragging
                        && let Some(at) = self.hover
                    {
                        self.pending.push(Command::PickAt {
                            player: self.me,
                            at: glam::Vec2::new(at.x, at.z),
                        });
                    }
                    self.right_down = None;
                    self.dragging = false;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (Some(down), Some(last)) = (self.right_down, self.last_cursor) {
                    let moved =
                        ((position.x - down.x).powi(2) + (position.y - down.y).powi(2)).sqrt();
                    if moved > DRAG_THRESHOLD {
                        self.dragging = true;
                    }
                    if self.dragging {
                        let dx = (position.x - last.x) as f32;
                        let dy = (position.y - last.y) as f32;
                        // Dragging right turns the scene right; dragging down raises the camera.
                        self.camera.orbit(-dx * ORBIT_SPEED, dy * ORBIT_SPEED);
                    }
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

/// A small fire made the way a player would, in front of the naturalist on level ground: a
/// ring of pebbles, dry grass in the middle, twigs around it, a raw dish set a little aside to
/// dry in the heat; then the fire drill until an ember falls on the grass. For captures.
/// The nearest level, dry ground (5 × 5 cells) to the naturalist: the centre of its middle
/// cell, on the ground.
fn level_spot(app: &App) -> Option<Vec3> {
    let start = app.state.body(app.me).position;
    let world = &app.world;
    let dims = world.dims();
    let level = |x: usize, z: usize| {
        let top = world.ground_top(x, z);
        (0..25).all(|k| {
            let (cx, cz) = (x + k % 5 - 2, z + k / 5 - 2);
            world.ground_top(cx, cz) == top && world.water_level(cx, cz).is_none()
        })
    };
    let (x, z) = (3..dims.nz - 3)
        .flat_map(|z| (3..dims.nx - 3).map(move |x| (x, z)))
        .filter(|&(x, z)| level(x, z))
        .min_by(|a, b| {
            let d =
                |p: (usize, usize)| (p.0 as f32 - start.x).powi(2) + (p.1 as f32 - start.z).powi(2);
            d(*a).total_cmp(&d(*b))
        })?;
    Some(Vec3::new(
        x as f32 + 0.5,
        world.ground_top(x, z) as f32,
        z as f32 + 0.5,
    ))
}

/// Test mode (`--test`): a stock of materials laid on level ground in front of the
/// naturalist, ready to pick up: dark and light pebbles, bundles of twigs, dry grass, clay,
/// sticks. Laid through the game's commands, like a player would.
fn sandbox(app: &mut App) {
    let me = app.me;
    let Some(centre) = level_spot(app) else {
        return;
    };
    let rows: [(Matter, usize); 7] = [
        (Matter::Pebble { dark: true }, 6),
        (Matter::Pebble { dark: false }, 4),
        (Matter::DeadTwigs, 5),
        (Matter::GrassFibre, 5),
        (Matter::Stick, 3),
        (
            Matter::Clay {
                source: items::ClaySource::Bank,
            },
            4,
        ),
        (
            Matter::Clay {
                source: items::ClaySource::RedEarth,
            },
            2,
        ),
    ];
    for (row, &(matter, count)) in rows.iter().enumerate() {
        for k in 0..count {
            // Rows across, 35 cm apart, starting a cell in front of the naturalist.
            let at = centre
                + Vec3::new(
                    (k as f32 - (count as f32 - 1.0) / 2.0) * 0.35,
                    0.0,
                    0.4 + row as f32 * 0.32,
                );
            // Stand right behind the point to lay it (the body faces +z at first).
            app.state.place(me, at - Vec3::new(0.0, 0.0, 0.8));
            app.state.give(me, matter);
            let slot = app.state.player(me).map_or(0, |p| {
                p.inventory
                    .stacks()
                    .iter()
                    .position(|s| s.matter == matter)
                    .unwrap_or(0)
            });
            let at = glam::Vec2::new(at.x, at.z);
            app.state.apply(
                &app.world,
                Command::LayAt {
                    player: me,
                    slot,
                    at,
                },
            );
        }
    }
    // Facing the stock, a step back.
    app.state.place(me, centre - Vec3::new(0.0, 0.0, 0.6));
    app.camera.target = centre + Vec3::Y * LOOK_HEIGHT;
    app.message = Some((
        "Mode test : des matériaux sont posés devant vous.".to_owned(),
        5.0,
    ));
}

fn demo_fire(app: &mut App) {
    let me = app.me;
    let Some(centre) = level_spot(app) else {
        return;
    };
    app.camera.target = centre + Vec3::Y * LOOK_HEIGHT;
    // Laying at a point: stand 0.8 behind it (the body faces +z at first).
    let lay_at = |app: &mut App, at: Vec3, matter: items::Matter| {
        app.state.place(me, at - Vec3::new(0.0, 0.0, 0.8));
        app.state.give(me, matter);
        let slot = app.state.player(me).map_or(0, |p| {
            p.inventory
                .stacks()
                .iter()
                .position(|s| s.matter == matter)
                .unwrap_or(0)
        });
        if let Some(event) = app
            .state
            .apply(&app.world, Command::Lay { player: me, slot })
        {
            app.on_event(event);
        }
    };
    for k in 0..7 {
        let a = std::f32::consts::TAU * k as f32 / 7.0;
        lay_at(
            app,
            centre + Vec3::new(0.38 * a.cos(), 0.0, 0.38 * a.sin()),
            items::Matter::Pebble { dark: k % 2 == 0 },
        );
    }
    // The tinder alone, then the fire drill with the hands right over it (0.48 in front of
    // the feet): twigs piled on the grass at once would take its heat and its air.
    // A nest of two handfuls of dry grass: one alone burns out before the twigs catch.
    lay_at(app, centre, items::Matter::GrassFibre);
    lay_at(app, centre, items::Matter::GrassFibre);
    app.state.place(me, centre - Vec3::new(0.0, 0.0, 0.48));
    app.controls.rub = true;
    let mut frame = 0;
    let mut wait = |app: &mut App, seconds: f32| {
        for _ in 0..(60.0 * seconds) as usize {
            app.tick(CAPTURE_STEP, frame as f32 * CAPTURE_STEP);
            frame += 1;
        }
    };
    wait(app, state::RUB_SECONDS + 0.3);
    app.controls.rub = false;
    // Blowing on the ember until the grass flames (a minute or so), then the twigs on it.
    for _ in 0..120 {
        let objects = app.state.objects();
        if (0..objects.placed().len()).any(|i| objects.body(i).burning) {
            break;
        }
        wait(app, 1.0);
    }
    for k in 0..3 {
        let a = std::f32::consts::TAU * k as f32 / 3.0 + 0.4;
        lay_at(
            app,
            centre + Vec3::new(0.12 * a.cos(), 0.0, 0.12 * a.sin()),
            items::Matter::DeadTwigs,
        );
    }
    // A raw dish set a little aside, to dry in the heat.
    lay_at(
        app,
        centre + Vec3::new(0.75, 0.0, 0.0),
        items::Matter::RawDish {
            source: items::ClaySource::RedEarth,
        },
    );
    // Step back to watch.
    app.state.place(me, centre + Vec3::new(-1.2, 0.0, 1.0));
}

/// The interface over the image, and the weariness of the local player. A free function
/// (not a method of `App`) so that the renderer, held by `App`, can be borrowed beside it.
#[allow(clippy::too_many_arguments)]
fn draw_hud(
    state: &GameState,
    world: &World,
    me: PlayerId,
    selected: usize,
    hour: f32,
    message: &Option<(String, f32)>,
    bag_open: bool,
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
            bag_open,
        },
    );
    renderer.set_ui(ui.vertices());
    let weariness = state.player(me).map_or(0.0, |p| p.needs.weariness());
    renderer.set_weariness(weariness);
}

/// The ground (or water) the cursor points at: a ray from the eye through the pixel at `ndc`
/// (−1 to 1), marched until it meets solid ground or water. Leaves and plants are seen
/// through, so one can point under a tree.
fn pointed_ground(
    world: &World,
    camera: &OrbitCamera,
    aspect: f32,
    ndc: glam::Vec2,
) -> Option<Vec3> {
    let inverse = camera.view_proj(aspect).inverse();
    let near = inverse.project_point3(Vec3::new(ndc.x, ndc.y, 0.0));
    let far = inverse.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
    let direction = (far - near).normalize_or_zero();
    let dims = world.dims();
    let mut p = camera.eye();
    let step = 0.05;
    for _ in 0..12_000 {
        p += direction * step;
        if p.x < 0.0 || p.z < 0.0 || p.y < 0.0 {
            return None;
        }
        let (x, y, z) = (p.x as usize, p.y as usize, p.z as usize);
        if x >= dims.nx || z >= dims.nz {
            return None;
        }
        if y >= dims.ny {
            continue;
        }
        if world.water_level(x, z).is_some_and(|level| p.y <= level) {
            return Some(p);
        }
        let material = world.block(x, y, z);
        if material.is_solid() && !material.is_plant() {
            // Back to the surface: the top of the cell entered.
            return Some(Vec3::new(p.x, (y + 1) as f32, p.z));
        }
    }
    None
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
    /// Makes a small fire in front of the naturalist, the way a player would (see
    /// `demo_fire`): to look at fire and objects.
    demo_fire: bool,
    /// Shows the bag open.
    bag: bool,
    /// Holds the naturalist in a gesture (`--pose rub|blow|reach|eat|drink|shape`).
    pose: Option<GestureKind>,
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
                    demo_fire: args.iter().any(|a| a == "--demo-fire"),
                    bag: args.iter().any(|a| a == "--bag"),
                    pose: match value("--pose") {
                        None => None,
                        Some("rub") => Some(GestureKind::Rub),
                        Some("blow") => Some(GestureKind::Blow),
                        Some("reach") => Some(GestureKind::Reach),
                        Some("eat") => Some(GestureKind::Eat),
                        Some("drink") => Some(GestureKind::Drink),
                        Some("shape") => Some(GestureKind::Shape),
                        Some(other) => return Err(format!("--pose {other} : geste inconnu")),
                    },
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
    if std::env::args().any(|a| a == "--test") {
        sandbox(&mut app);
    }
    // Built first, so the fire has `time` to catch.
    if options.demo_fire {
        demo_fire(&mut app);
    }
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
    for (cell, flooded) in std::mem::take(&mut app.dug_cells) {
        app.data.ground_dug(&app.world, cell, flooded, None);
    }
    let mut renderer = Renderer::new(Gpu::offscreen(options.width, options.height));
    app.data.install(&mut renderer);
    app.naturalist.install(&mut renderer);
    let mut motion = app.state.body(app.me).motion(time);
    motion.gesture = match options.pose {
        Some(kind) => Some(Gesture { kind, amount: 1.0 }),
        None => app.current_gesture(time),
    };
    app.naturalist.pose(&mut renderer, &motion);
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
    app.objects_view.install(&mut renderer);
    app.objects_view
        .draw(app.state.objects(), time, &mut renderer, &mut app.particles);
    draw_hud(
        &app.state,
        &app.world,
        app.me,
        app.selected,
        app.clock.hour(),
        &app.message,
        app.bag_open || options.bag,
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
    if let Some(capture_options) = &options.capture {
        if let Err(message) = capture(options.seed, capture_options) {
            eprintln!("Capture impossible : {message}");
            std::process::exit(1);
        }
        return;
    }
    let event_loop = EventLoop::new().expect("création de la boucle d'événements");
    let mut app = App::new(options.seed);
    if args.iter().any(|a| a == "--test") {
        sandbox(&mut app);
    }
    event_loop
        .run_app(&mut app)
        .expect("exécution de la boucle d'événements");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_centre_of_the_screen_points_at_the_ground_the_camera_looks_at() {
        let world = World::generate(WorldConfig::standard(1));
        let feet = player::spawn_point(&world);
        let camera = OrbitCamera::framing(feet + Vec3::Y * LOOK_HEIGHT, FOLLOW_FRAME);
        let hit = pointed_ground(&world, &camera, 1.6, glam::Vec2::ZERO).expect("no ground");
        // The camera looks at the chest: the ray meets the ground just beyond the feet.
        let offset = glam::Vec2::new(hit.x - feet.x, hit.z - feet.z).length();
        assert!(offset < 3.0, "pointed {offset} cells from the feet");
        assert!((hit.y - feet.y).abs() < 2.5);
    }
}
