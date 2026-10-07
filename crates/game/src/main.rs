//! Binaire jouable. Un naturaliste se promène dans un monde procédural (relief, mer, plages,
//! déserts, forêts, montagnes, rivières, lacs, un marais près de la prairie des cerfs), suivi
//! par la caméra plongeante. Le feuillage ondule au vent, du pollen flotte dans la lumière, des
//! feuilles tombent.
//! Commandes (touches par position, ZQSD sur un clavier AZERTY) : Z Q S D pour marcher, Maj
//! pour courir, Espace pour sauter. Souris : clic gauche pose ce qu'on tient là où pointe le
//! curseur, clic droit le reprend (ou cueille), glisser avec le bouton droit tourne la
//! caméra, molette pour zoomer (de la vue plongeante jusqu'à hauteur d'homme). Clavier : E ramasser, P poser devant soi, 1 à 8 choisir dans
//! le sac, F manger ou modeler, B boire, G (maintenu) frotter le foret à feu ou souffler sur
//! la braise. T (maintenu) accélère la journée, R lance ou arrête la pluie.
//! `--seed N` choisit le monde ; `--capture fichier.png` enregistre une vue sans fenêtre
//! (`--marsh` la tourne vers le marais ; les autres options sont dans `Options::from_args`).

mod ambient;
mod audio;
mod camera_rig;
mod clock;
mod deer;
mod deer_view;
mod ecology;
mod fauna;
mod hud;
mod items;
mod listen;
mod naturalist;
mod needs;
mod notebook;
mod objects;
mod objects_view;
mod obstacles;
mod player;
mod save;
mod scene;
mod season;
mod sketch;
mod soil;
mod sound;
mod squirrel_view;
mod squirrels;
mod state;
mod traces;
mod trample;
mod weather;
mod wind;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use camera_rig::CameraRig;
use glam::Vec3;
use render::{Gpu, OrbitCamera, Renderer};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};
use world::{Marsh, World, WorldConfig};

use ambient::Ambient;
use audio::Audio;
use clock::Clock;
use deer_view::{DeerPose, DeerView};
use fauna::Fauna;
use items::Matter;
use naturalist::Motion;
use naturalist::{Gesture, GestureKind, Naturalist};
use objects_view::ObjectsView;
use player::Controls;
use render::palette::srgb_hex;
use scene::SceneData;
use sketch::Sketch;
use state::{Command, Event, Failure, GameState, PlayerId};
use traces::Traces;
use trample::Trample;
use weather::Weather;

/// Pixels the cursor must move, button held, to be a drag rather than a click.
const DRAG_THRESHOLD: f64 = 6.0;
/// Radians of rotation per pixel of mouse drag.
const ORBIT_SPEED: f32 = 0.005;
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
    /// The marsh dug near the deer's meadow (given again by the generation, never saved).
    marsh: Option<Marsh>,
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
    /// The camera following the naturalist (see `camera_rig`).
    rig: CameraRig,
    /// Where the naturalist's feet were drawn before the last fixed step: the body is drawn
    /// between that and where it is now, by how far the frame is into the next step.
    previous_feet: Vec3,
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
    deer_view: DeerView,
    squirrel_view: squirrel_view::SquirrelView,
    /// The notebook is open (N), at this page.
    notebook_open: bool,
    page: usize,
    /// Sketches of the notebook's pages, drawn from the screen when each was written; pages
    /// waiting for theirs (drawn after the next frame).
    sketches: HashMap<usize, Sketch>,
    sketch_pending: Vec<usize>,
    /// When a spell was last cast (seconds since start).
    cast_at: Option<f32>,
    /// Gait cycle of the naturalist in the shape of a deer (radians).
    deer_stride: f32,
    /// Real seconds until the next automatic save.
    save_in: f32,
    /// Frames counted and their time, for `DISSIPATIF_FPS`.
    frames: (u32, f32),
}

/// The game saves itself this often (real seconds), and when the window closes.
const AUTOSAVE_SECONDS: f32 = 120.0;

/// What a save holds beyond the world: the game state, the time, the notebook's sketches.
type Saved = (GameState, Clock, HashMap<usize, Sketch>);

/// A world as the game makes it from its seed: generated, the deer's meadow found and the
/// circle their rite has worn into it over the years, the marsh dug a walk away. The same every
/// time: a saved game is laid over it.
struct Generated {
    world: World,
    /// Where the naturalist starts a new game.
    spawn: Vec3,
    /// The deer's circle and the wood's edge where they lie up, if the world has a meadow.
    home: Option<(glam::Vec2, glam::Vec2)>,
    marsh: Option<Marsh>,
}

fn generate(config: WorldConfig) -> Generated {
    let seed = config.seed;
    let mut world = World::generate(config);
    let spawn = player::spawn_point(&world);
    let start = glam::Vec2::new(spawn.x, spawn.z);
    let home = deer::home(&world, start);
    if let Some((ring, _)) = home {
        deer::wear_ring(&mut world, ring, seed);
    }
    // The marsh, a walk from the meadow (or from the start, in a world without one): never
    // where the naturalist starts and finds the notebook, nor where the herd lies up.
    let near = home.map_or(start, |(ring, _)| ring);
    let mut dry = vec![start.to_array()];
    dry.extend(home.map(|(_, cover)| cover.to_array()));
    let marsh = world.make_marsh_away_from(near.to_array(), &dry, seed);
    Generated {
        world,
        spawn,
        home,
        marsh,
    }
}

/// Reads the save of world `seed`, if there is one, restoring `world` from it.
fn load_save(seed: u64, world: &mut World) -> save::Result<Option<Saved>> {
    let Ok(bytes) = std::fs::read(save::path(seed)) else {
        return Ok(None);
    };
    let mut r = save::open(&bytes, seed)?;
    world.restore(r.get()?)?;
    let state = GameState::load(&mut r, world)?;
    let clock = Clock::at_days(r.get()?);
    let sketches: Vec<(usize, Vec<u8>)> = r.get()?;
    let sketches = sketches
        .into_iter()
        .filter(|(_, ink)| ink.len() == sketch::WIDTH * sketch::HEIGHT)
        .map(|(page, ink)| (page, Sketch { ink }))
        .collect();
    Ok(Some((state, clock, sketches)))
}

impl App {
    /// A game on the world of `seed`: the saved one if `resume` and there is one, else a new
    /// one.
    fn new(seed: u64, resume: bool) -> Self {
        let start = Instant::now();
        let Generated {
            mut world,
            spawn,
            home,
            marsh,
        } = generate(WorldConfig::standard(seed));
        let saved = if resume {
            match load_save(seed, &mut world) {
                Ok(saved) => saved,
                Err(message) => {
                    eprintln!("Sauvegarde illisible ({message}) : nouvelle partie.");
                    // The world may have been half restored: start again from the seed.
                    world = generate(WorldConfig::standard(seed)).world;
                    None
                }
            }
        } else {
            None
        };
        let generated = start.elapsed();
        let mut data = SceneData::build(&world, marsh.as_ref());
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
        let resumed = saved.is_some();
        let (mut state, clock, sketches) = match saved {
            Some(saved) => saved,
            None => {
                let mut state = GameState::new(&world);
                if let Some((ring, cover)) = home {
                    state.add_herd(deer::Herd::new(ring, cover, &world, seed ^ 0xdee5));
                }
                state.add_squirrels(squirrels::Squirrels::new(
                    world.plants(),
                    &world,
                    glam::Vec2::new(spawn.x, spawn.z),
                    seed ^ 0x5c1,
                ));
                state.join(spawn);
                // The notebook, glowing on the grass just in front.
                let book = glam::Vec2::new(spawn.x, spawn.z + 1.4);
                state.place_notebook(Vec3::new(
                    book.x,
                    world.surface_height(book.x, book.y),
                    book.y,
                ));
                (state, Clock::new(clock::START_HOUR), HashMap::new())
            }
        };
        // The marsh is not saved: the game is told where it is again.
        state.set_marsh(marsh.clone());
        // The local player is the first.
        let me: PlayerId = 0;
        let feet = state.body(me).shown_position();
        let rig = CameraRig::new(feet);
        // Plants born since the world was made, and those gone; all drawn at their size.
        let mut sprouted = Vec::new();
        for i in world.plants().len()..state.plants().len() {
            let plant = state.plants()[i];
            sprouted.extend(data.add_plant(&world, &plant, state.plant_size(i)));
        }
        for i in 0..state.plants().len() {
            if state.is_removed(i) {
                data.hide(i);
            } else {
                data.resize_plant(i, state.plant_size(i));
            }
        }
        let mut trample = Trample::new(data.pliable());
        for pliable in sprouted {
            trample.add(pliable);
        }
        let message = resumed.then(|| (format!("Partie reprise : jour {}", clock.day()), 4.0));
        Self {
            graphics: None,
            world,
            marsh,
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
            message,
            objects_view: ObjectsView::new(),
            trample,
            controls: Controls::default(),
            clock,
            audio: None,
            listen_in: 0.0,
            last_stride_phase: 0.0,
            naturalist: Naturalist::new(),
            start,
            last_frame: Instant::now(),
            rig,
            previous_feet: feet,
            dragging: false,
            last_cursor: None,
            right_down: None,
            hover: None,
            bag_open: false,
            shift: false,
            dug_cells: Vec::new(),
            gesture: None,
            now: 0.0,
            deer_view: DeerView::new(),
            squirrel_view: squirrel_view::SquirrelView::new(),
            notebook_open: false,
            page: 0,
            sketches,
            save_in: AUTOSAVE_SECONDS,
            frames: (0, 0.0),
            sketch_pending: Vec::new(),
            cast_at: None,
            deer_stride: 0.0,
        }
    }
}

impl App {
    /// One frame of the world: the player moves, the camera follows, the air lives.
    fn tick(&mut self, dt: f32, time: f32) {
        self.now = time;
        self.clock.advance(dt);
        if self.graphics.is_some() {
            self.save_in -= dt;
            if self.save_in <= 0.0 {
                self.save_in = AUTOSAVE_SECONDS;
                self.save();
            }
        }
        self.run_steps(dt);
        if let Some((_, left)) = &mut self.message {
            *left -= dt;
            if *left <= 0.0 {
                self.message = None;
            }
        }
        let body = self.state.body(self.me);
        let (feet, velocity) = (body.position, body.velocity());
        let drawn = self.drawn_feet();
        self.rig.update(dt, drawn, &self.world);
        let around = [feet.x, feet.z];
        let wind = wind::direction(self.clock.days()).to_array();
        self.ambient.wind = wind;
        self.weather.wind = wind;
        // The air where the naturalist is: rain or snow, ice on the water.
        let feet_now = self.state.body(self.me).position;
        let year = season::year(self.clock.days());
        self.weather.temperature = needs::temperature(
            &self.world,
            feet_now,
            self.clock.hour(),
            self.weather.rain(),
            year,
        );
        let speed = if self.clock.fast { 60.0 } else { 1.0 };
        self.weather.seasons(dt * speed * 24.0 / clock::DAY_SECONDS);
        self.ambient.update(dt, time, around, &self.world);
        let hour = self.clock.hour();
        self.weather.update(dt, hour, feet, &self.world);
        let night = render::sky::sky(hour).night;
        let rain = self.weather.rain();
        self.fauna.year = year;
        self.fauna.update(dt, time, feet, night, rain, &self.world);
        self.particles.clear();
        self.particles.extend_from_slice(self.ambient.instances());
        self.particles.extend_from_slice(self.fauna.particles());
        self.particles.extend_from_slice(self.traces.instances());
        self.particles.extend_from_slice(self.weather.particles());
        // Plants on fire.
        for (k, (at, strength)) in self.state.burning_plants().into_iter().enumerate() {
            objects_view::plant_fire(at, strength, time, k as f32, &mut self.particles);
        }
        // The deer's gait, when in its shape.
        let speed = glam::Vec2::new(velocity.x, velocity.z).length();
        self.deer_stride = (self.deer_stride
            + speed * dt * std::f32::consts::TAU / deer::stride_length(speed, 1.15))
            % std::f32::consts::TAU;
        self.magic(time);
        self.update_sound(dt, time);
        // Plants pushed aside by the body, springing back once free.
        let data = &mut self.data;
        self.trample
            .update(dt, feet, velocity, |plant, bend| data.set_bend(plant, bend));
    }

    /// Where the naturalist's feet are drawn: between where they were before the last fixed
    /// step and where they are now, by how far the frame is into the next step. Steps come
    /// at a fixed rate and frames at the screen's: without this, the body (and the camera)
    /// would move by one step some frames and by none or two others, a visible stutter.
    fn drawn_feet(&self) -> Vec3 {
        let now = self.state.body(self.me).shown_position();
        let before = self.previous_feet;
        let [x, z] = self.world.nearest([now.x, now.z], [before.x, before.z]);
        Vec3::new(x, before.y, z).lerp(now, (self.accumulator / state::STEP).clamp(0.0, 1.0))
    }

    /// The naturalist's drawn feet, at their copy nearest the point looked at: what stands
    /// between them and the eye is cut away.
    fn focus(&self) -> Vec3 {
        let feet = self.drawn_feet();
        let t = self.rig.camera.target;
        let [x, z] = self.world.nearest([t.x, t.z], [feet.x, feet.z]);
        Vec3::new(x, feet.y, z)
    }

    /// Turns frame time into fixed steps of the game: each step, the keys held become a
    /// steering command, pending commands are applied, then time advances by `state::STEP`.
    fn run_steps(&mut self, dt: f32) {
        // After a long pause, do not try to catch up more than a quarter of a second.
        self.accumulator = (self.accumulator + dt).min(0.25);
        let now = self.clock.conditions(self.weather.rain());
        while self.accumulator >= state::STEP {
            self.accumulator -= state::STEP;
            self.previous_feet = self.state.body(self.me).shown_position();
            let steer = Command::Steer {
                player: self.me,
                controls: self.controls,
                camera_yaw: self.rig.camera.yaw,
            };
            self.state.apply(&self.world, steer);
            // A jump is one press, not held.
            self.controls.jump = false;
            for command in std::mem::take(&mut self.pending) {
                if let Some(event) = self.state.apply(&self.world, command) {
                    self.on_event(event);
                }
            }
            self.state.time_scale = if self.clock.fast { 60.0 } else { 1.0 };
            self.state.step(&self.world, &now);
        }
        // What the physics did meanwhile (an ember, a dish fired or burst…).
        let events: Vec<Event> = self.state.drain_events().collect();
        for event in events {
            self.on_event(event);
        }
    }

    /// Saves the game (the world's changes, the state, the time, the sketches).
    fn save(&self) {
        let seed = self.world.config.seed;
        let mut w = save::header(seed);
        w.put(&self.world.state());
        self.state.save(&mut w);
        w.put(&self.clock.days());
        let mut sketches: Vec<(usize, Vec<u8>)> = self
            .sketches
            .iter()
            .map(|(&page, s)| (page, s.ink.clone()))
            .collect();
        sketches.sort_by_key(|&(page, _)| page);
        w.put(&sketches);
        let path = save::path(seed);
        if let Err(e) = save::write_file(&path, &w.bytes) {
            eprintln!("Sauvegarde impossible ({}) : {e}", path.display());
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
            Event::Wrote { page, entry, .. } => {
                self.sketch_pending.push(page);
                match entry {
                    notebook::Entry::Found => {
                        "Un carnet. Il brillait. (N pour l'ouvrir)".to_owned()
                    }
                    notebook::Entry::Learnt(_) => return,
                    _ => "Le carnet s'est rempli d'une page.".to_owned(),
                }
            }
            Event::Learnt { spell, .. } => {
                format!("Vous avez compris : {}. (V pour le lancer)", spell.name())
            }
            Event::Cast { on, .. } => {
                self.cast_at = Some(self.now);
                if on {
                    "Vous prenez la forme du cerf.".to_owned()
                } else {
                    "Vous reprenez forme humaine.".to_owned()
                }
            }
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
            Event::Call { call, at } => {
                // Heard as far as it carries, softer with distance, on its side.
                let reach = match call {
                    sound::AnimalCall::Bark => 50.0,
                    sound::AnimalCall::Stamp => 20.0,
                    sound::AnimalCall::Bell => 100.0,
                };
                let feet = self.state.body(self.me).position;
                let offset = at - glam::Vec2::new(feet.x, feet.z);
                let loudness = (1.0 - offset.length() / reach).clamp(0.0, 1.0).powi(2);
                let right = glam::Vec2::new(self.rig.camera.yaw.cos(), -self.rig.camera.yaw.sin());
                let pan = (offset.normalize_or_zero().dot(right)).clamp(-1.0, 1.0) * 0.8;
                if loudness > 0.0
                    && let Some(audio) = &self.audio
                {
                    audio.shared.cry(call, loudness, pan);
                }
                return;
            }
            Event::Ground { x, z, material } => {
                if let Some(cell) = self.world.set_surface(x, z, material) {
                    self.data.surface_changed(&self.world, cell);
                }
                return;
            }
            Event::Dig { at } => {
                if let Some(dug) = self.world.dig(at.x, at.y) {
                    self.dug_cells.push((dug.cell, dug.flooded));
                }
                return;
            }
            Event::Plant(change) => {
                match change {
                    ecology::Change::Sprouted(i) => {
                        let plant = self.state.plants()[i];
                        let size = self.state.plant_size(i);
                        if let Some(pliable) = self.data.add_plant(&self.world, &plant, size) {
                            self.trample.add(pliable);
                        }
                    }
                    ecology::Change::Resized(i) => {
                        self.data.resize_plant(i, self.state.plant_size(i));
                    }
                    ecology::Change::Stood(i) => {
                        let plant = self.state.plants()[i];
                        self.world.stamp_plant(&plant);
                    }
                    ecology::Change::Ignited(_) => {}
                    ecology::Change::Died { plant, stood } => {
                        self.data.hide(plant);
                        if stood {
                            let instance = self.state.plants()[plant];
                            self.world.unstamp_plant(&instance);
                        }
                    }
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
                Failure::NoHands => "Pas de mains sous cette forme.",
            }
            .to_owned(),
        };
        // What is understood stays long enough to be read.
        let seconds = if matches!(event, Event::Learnt { .. }) {
            8.0
        } else {
            2.5
        };
        self.message = Some((text, seconds));
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

    /// The naturalist (or the deer they turned into), the herd, the notebook lying there.
    fn draw_creatures(&self, renderer: &mut Renderer, motion: &Motion, time: f32) {
        let body = self.state.body(self.me);
        let mut deer: Vec<DeerPose> = self
            .state
            .herd()
            .map(|h| {
                // The hinds, and the stag of the rut in autumn.
                h.deer
                    .iter()
                    .enumerate()
                    .map(|(i, d)| DeerPose::of(d, i, time))
                    .chain(h.stag.iter().map(|d| DeerPose {
                        stag: true,
                        ..DeerPose::of(d, 99, time)
                    }))
                    .collect()
            })
            .unwrap_or_default();
        if body.deer {
            self.naturalist.hide(renderer);
            let v = body.velocity();
            let speed = glam::Vec2::new(v.x, v.z).length();
            deer.push(DeerPose {
                position: motion.position,
                heading: body.facing(),
                head: if speed > 5.5 { 0.6 } else { 0.4 },
                lying: 0.0,
                stride: self.deer_stride,
                speed,
                size: 1.15,
                stamp: 0.0,
                alarmed: false,
                airborne: motion.airborne,
                stag: true,
                time,
                seed: 0.0,
            });
        } else {
            self.naturalist.pose(renderer, motion);
        }
        self.deer_view.draw(renderer, &deer);
        if let Some(squirrels) = self.state.squirrels() {
            self.squirrel_view
                .draw(renderer, &squirrels.squirrels, time);
        }
        self.naturalist
            .pose_book(renderer, self.state.notebook_lying(), time);
    }

    /// Sketches for the pages just written, from the frame just drawn.
    fn take_sketches(&mut self, renderer: &Renderer) {
        if self.sketch_pending.is_empty() {
            return;
        }
        if let Some((width, height, pixels)) = renderer.snapshot() {
            let sketch = sketch::draw(width, height, &pixels);
            for page in self.sketch_pending.drain(..) {
                self.sketches.insert(page, sketch.clone());
            }
        }
    }

    /// Light that is not of this world: the notebook glowing where it lies, the rite's motes
    /// over the circle, a spell's swirl.
    fn magic(&mut self, time: f32) {
        let out = &mut self.particles;
        if let Some(at) = self.state.notebook_lying() {
            let gold = srgb_hex(0xffe3a0);
            for i in 0..28 {
                let phase = (time * 0.3 + i as f32 * 0.0357) % 1.0;
                let angle = i as f32 * 2.399 + time * 0.5;
                let r = 0.15 + 0.3 * phase;
                out.push(render::ParticleInstance {
                    centre_size: [
                        at.x + angle.cos() * r,
                        at.y + 0.12 + 0.9 * phase,
                        at.z + angle.sin() * r,
                        0.015 + 0.03 * (1.0 - phase),
                    ],
                    color: [gold[0], gold[1], gold[2], -4.0 * (1.0 - phase)],
                });
            }
            out.push(render::ParticleInstance {
                centre_size: [at.x, at.y + 0.45, at.z, 0.04],
                color: [gold[0], gold[1], gold[2], -3.0],
            });
        }
        if let Some(herd) = self.state.herd()
            && herd.glow > 0.01
        {
            let silver = srgb_hex(0xd6e2ff);
            let g = herd.glow;
            let ring = herd.ring;
            let ground = self.world.surface_height(ring.x, ring.y);
            for i in 0..(48.0 * g) as usize {
                let phase = (time * 0.1 + i as f32 * 0.618) % 1.0;
                let angle = i as f32 * 2.399 + time * 0.05;
                let r = deer::RING_RADIUS * (0.15 + 0.85 * ((i * 7 % 10) as f32 / 10.0));
                out.push(render::ParticleInstance {
                    centre_size: [
                        ring.x + angle.cos() * r,
                        ground + 0.2 + 5.0 * phase,
                        ring.y + angle.sin() * r,
                        0.01 + 0.03 * (1.0 - phase),
                    ],
                    color: [silver[0], silver[1], silver[2], -2.5 * g * (1.0 - phase)],
                });
            }
            // The incantation's movements, each with its light.
            if let Some(u) = herd.rite {
                let seconds = u * deer::RITE_HOURS * clock::DAY_SECONDS / 24.0;
                let tau = std::f32::consts::TAU;
                let point =
                    |out: &mut Vec<render::ParticleInstance>, p: Vec3, size: f32, glow: f32| {
                        out.push(render::ParticleInstance {
                            centre_size: [p.x, p.y, p.z, size],
                            color: [silver[0], silver[1], silver[2], -glow],
                        });
                    };
                let centre = Vec3::new(ring.x, ground, ring.y);
                if (0.12..0.45).contains(&u) {
                    // Each bow sends a wave of light out from the centre over the grass.
                    let wave = (seconds / 3.0 + 0.5).fract();
                    for k in 0..40 {
                        let a = k as f32 / 40.0 * tau;
                        let r = deer::RING_RADIUS * 1.15 * wave;
                        point(
                            out,
                            centre + Vec3::new(a.cos() * r, 0.08, a.sin() * r),
                            0.04,
                            3.0 * (1.0 - wave),
                        );
                    }
                } else if (0.45..0.8).contains(&u) {
                    // The procession: light winding up from the circle.
                    for k in 0..60 {
                        let phase = (seconds * 0.15 + k as f32 / 60.0) % 1.0;
                        let a = k as f32 * 0.7 + seconds * 0.6;
                        let r = deer::RING_RADIUS * (1.0 - 0.7 * phase);
                        point(
                            out,
                            centre + Vec3::new(a.cos() * r, 0.2 + 6.0 * phase, a.sin() * r),
                            0.03,
                            3.0 * (1.0 - phase),
                        );
                    }
                } else if (0.8..0.97).contains(&u) {
                    // Heads raised to the moon: threads from each to the centre, and a column
                    // of light rising to the sky.
                    let top = centre + Vec3::Y * 2.5;
                    for d in &herd.deer {
                        let head = d.position + Vec3::Y * 1.7 * d.size;
                        for k in 0..10 {
                            let t = ((k as f32 + seconds * 3.0) / 10.0).fract();
                            point(out, head.lerp(top, t), 0.025, 3.5);
                        }
                    }
                    for k in 0..70 {
                        let phase = (seconds * 0.6 + k as f32 / 70.0) % 1.0;
                        let a = k as f32 * 2.399;
                        let r = 0.25 + 0.15 * (seconds * 3.0 + k as f32).sin();
                        point(
                            out,
                            centre + Vec3::new(a.cos() * r, 0.5 + 20.0 * phase, a.sin() * r),
                            0.05,
                            5.0 * (1.0 - phase * 0.7),
                        );
                    }
                } else if u >= 0.97 {
                    // The end: the light bursts and scatters.
                    let t = (u - 0.97) / 0.03;
                    for k in 0..90 {
                        let a = k as f32 * 2.399;
                        let rise = (k % 9) as f32 / 9.0;
                        let r = 0.5 + 9.0 * t * (0.5 + 0.5 * rise);
                        point(
                            out,
                            centre + Vec3::new(a.cos() * r, 2.5 + 3.0 * rise * t, a.sin() * r),
                            0.04,
                            5.0 * (1.0 - t),
                        );
                    }
                }
            }
            for k in 0..56 {
                let angle = k as f32 / 56.0 * std::f32::consts::TAU;
                let at = ring + glam::Vec2::new(angle.cos(), angle.sin()) * deer::RING_RADIUS;
                let pulse = 0.5 + 0.5 * (time * 1.5 + k as f32 * 0.7).sin();
                out.push(render::ParticleInstance {
                    centre_size: [at.x, ground + 0.04, at.y, 0.035],
                    color: [silver[0], silver[1], silver[2], -1.2 * g * pulse],
                });
            }
        }
        // Understanding the rite: its light comes to the watcher, more and more of it.
        let understanding = self.state.understanding(self.me);
        if understanding > 0.0
            && let Some(herd) = self.state.herd()
        {
            let silver = srgb_hex(0xd6e2ff);
            let feet = self.state.body(self.me).shown_position();
            let ring = herd.ring;
            let from = Vec3::new(
                ring.x,
                self.world.surface_height(ring.x, ring.y) + 1.0,
                ring.y,
            );
            for i in 0..(60.0 * understanding) as usize {
                let phase = (time * 0.25 + i as f32 * 0.618) % 1.0;
                let angle = i as f32 * 2.399 + time * 1.5;
                // From the circle towards the naturalist, then winding round them.
                let swirl = 0.25 + 0.5 * (1.0 - phase);
                let target =
                    feet + Vec3::new(angle.cos() * swirl, 0.6 + 0.8 * phase, angle.sin() * swirl);
                let at = from.lerp(target, smooth(phase));
                out.push(render::ParticleInstance {
                    centre_size: [at.x, at.y, at.z, 0.015 + 0.02 * phase],
                    color: [silver[0], silver[1], silver[2], -2.5 * phase],
                });
            }
        }
        if let Some(start) = self.cast_at {
            let t = time - start;
            if (0.0..1.8).contains(&t) {
                let feet = self.state.body(self.me).shown_position();
                let green = srgb_hex(0xc8f0b0);
                let fade = 1.0 - t / 1.8;
                for i in 0..40 {
                    let k = i as f32 / 40.0;
                    let angle = k * std::f32::consts::TAU * 3.0 + t * 5.0;
                    let r = 0.3 + 0.9 * (1.0 - fade) * (0.5 + 0.5 * k);
                    out.push(render::ParticleInstance {
                        centre_size: [
                            feet.x + angle.cos() * r,
                            feet.y + 0.1 + 1.8 * k * (0.4 + 0.6 * fade),
                            feet.z + angle.sin() * r,
                            0.02 + 0.03 * fade,
                        ],
                        color: [green[0], green[1], green[2], -2.5 * fade],
                    });
                }
            }
        }
    }

    fn handle_key(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        // Physical keys: the same positions on every layout (Z Q S D on AZERTY).
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        // The open notebook takes the arrows to turn its pages.
        if self.notebook_open && pressed {
            let pages = self
                .state
                .player(self.me)
                .and_then(|p| p.notebook.as_ref())
                .map_or(0, |b| b.pages.len());
            match code {
                KeyCode::ArrowLeft | KeyCode::KeyA => {
                    self.page = self.page.saturating_sub(1);
                    return;
                }
                KeyCode::ArrowRight | KeyCode::KeyD => {
                    self.page = (self.page + 1).min(pages.saturating_sub(1));
                    return;
                }
                KeyCode::Escape => {
                    self.notebook_open = false;
                    return;
                }
                _ => {}
            }
        }
        match code {
            // C (held): crouch.
            KeyCode::KeyC => self.controls.crouch = pressed,
            KeyCode::KeyN if pressed && !event.repeat => {
                let pages = self
                    .state
                    .player(self.me)
                    .and_then(|p| p.notebook.as_ref())
                    .map(|b| b.pages.len());
                if let Some(pages) = pages {
                    self.notebook_open = !self.notebook_open;
                    self.page = pages.saturating_sub(1);
                }
            }
            // V: cast the spell known (the first; more to come).
            KeyCode::KeyV if pressed && !event.repeat => {
                let spell = self
                    .state
                    .player(self.me)
                    .and_then(|p| p.spells.first().copied());
                if let Some(spell) = spell {
                    self.pending.push(Command::Cast {
                        player: self.me,
                        spell,
                    });
                }
            }
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
        let dims = self.world.dims();
        renderer.set_world_size(dims.nx as f32, dims.nz as f32);
        self.data.install(&mut renderer);
        self.naturalist.install(&mut renderer);
        self.fauna.install(&mut renderer);
        self.weather.install(&mut renderer);
        self.objects_view.install(&mut renderer);
        self.deer_view.install(&mut renderer);
        self.squirrel_view.install(&mut renderer);
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
            WindowEvent::CloseRequested => {
                self.save();
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                // Clamped: after a pause of the window (dragging, sleep), no huge jump.
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;
                let time = (now - self.start).as_secs_f32();
                // `DISSIPATIF_FPS=1`: the mean frame time, every two seconds.
                if std::env::var_os("DISSIPATIF_FPS").is_some() {
                    self.frames.0 += 1;
                    self.frames.1 += dt;
                    if self.frames.1 >= 2.0 {
                        println!(
                            "image : {:.1} ms ({:.0} i/s)",
                            1000.0 * self.frames.1 / self.frames.0 as f32,
                            self.frames.0 as f32 / self.frames.1
                        );
                        self.frames = (0, 0.0);
                    }
                }
                self.tick(dt, time);
                let mut motion = self.state.body(self.me).motion(time);
                motion.position = self.drawn_feet();
                motion.gesture = self.current_gesture(time);
                // Out of `self` while drawing, so `self`'s methods can draw with it.
                if let Some(mut graphics) = self.graphics.take() {
                    let renderer = &mut graphics.renderer;
                    self.draw_creatures(renderer, &motion, time);
                    for (cell, flooded) in std::mem::take(&mut self.dug_cells) {
                        self.data
                            .ground_dug(&self.world, cell, flooded, Some(renderer));
                    }
                    let t = self.rig.camera.target;
                    let reach = self.rig.camera.reach(renderer.aspect());
                    self.data.set_view(glam::Vec2::new(t.x, t.z), reach);
                    renderer.set_focus(self.focus());
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
                        &self.clock,
                        self.rig.camera.yaw,
                        &self.message,
                        self.bag_open,
                        self.notebook_open
                            .then(|| (self.page, self.sketches.get(&self.page))),
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
                        pointed_ground(&self.world, &self.rig.camera, renderer.aspect(), ndc)
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
                    renderer.render(&self.rig.camera, time);
                    self.take_sketches(renderer);
                    self.graphics = Some(graphics);
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
                        self.rig.orbit(-dx * ORBIT_SPEED, dy * ORBIT_SPEED);
                    }
                }
                self.last_cursor = Some(position);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / PIXELS_PER_NOTCH,
                };
                self.rig.zoom_by(notches);
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
    app.rig.snap(centre);
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
    app.rig.snap(centre);
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
    clock: &Clock,
    camera_yaw: f32,
    message: &Option<(String, f32)>,
    bag_open: bool,
    notebook_page: Option<(usize, Option<&Sketch>)>,
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
            hour: clock.hour(),
            day: clock.day(),
            moon_phase: clock.moon_phase(),
            season: season::season(season::year(clock.days())).name(),
            wind: wind::direction(clock.days()),
            camera_yaw,
            message: message.as_ref().map(|(text, left)| (text.as_str(), *left)),
            bag_open,
            notebook_page,
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
        if p.y < 0.0 {
            return None;
        }
        let y = p.y as usize;
        if y >= dims.ny {
            continue;
        }
        // The world closes on itself: the point, brought back into it.
        let (wx, wz) = world.wrap(p.x, p.z);
        let (x, z) = (wx as usize, wz as usize);
        if world.water_level(x, z).is_some_and(|level| p.y <= level) {
            return Some(Vec3::new(wx, p.y, wz));
        }
        let material = world.block(x, y, z);
        if material.is_solid() && !material.is_plant() {
            // Back to the surface: the top of the cell entered.
            return Some(Vec3::new(wx, (y + 1) as f32, wz));
        }
    }
    None
}

/// Light and weather of the moment, to the renderer.
fn set_sky(clock: &Clock, weather: &Weather, feet: Vec3, renderer: &mut Renderer) {
    let hour = clock.hour();
    let mut sky = render::sky::sky(hour).with_moon(clock.moon_light());
    sky.wind = crate::wind::direction(clock.days()).to_array();
    weather.apply(&mut sky, hour, season::look(season::year(clock.days())));
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
    /// Day of the moon cycle to start on (from 1): the full moon is on the third night.
    day: u32,
    /// Shows the notebook open at its last page.
    notebook: bool,
    /// The naturalist knows the deer's shape and takes it (to look at it).
    deer: bool,
    /// Snow lying on the ground (and ice on the water), 0 to 1.
    snow: f32,
    /// Looks at the marsh, the naturalist standing on its bank.
    marsh: bool,
}

impl Options {
    /// `[--seed N] [--capture file.png [--size WxH] [--yaw DEG] [--pitch DEG] [--zoom F]
    /// [--at X,Z] [--time SECONDS] [--walk SECONDS] [--hour H] [--weather rain] [--start X,Z]
    /// [--pick N] [--demo-fire] [--bag] [--pose GESTURE] [--day N] [--notebook] [--deer]
    /// [--snow F] [--marsh]]`.
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
                    day: number("--day")?.unwrap_or(1.0) as u32,
                    notebook: args.iter().any(|a| a == "--notebook"),
                    deer: args.iter().any(|a| a == "--deer"),
                    snow: number("--snow")?.unwrap_or(0.0),
                    marsh: args.iter().any(|a| a == "--marsh"),
                })
            }
            None => None,
        };
        Ok(Self { seed, capture })
    }
}

/// Where to stand to look at the marsh from a camera turned by `yaw`: dry ground on its bank,
/// near its middle and on the camera's side (in the foreground, not hiding the water), as a
/// point (x, z) in the world.
fn marsh_bank(world: &World, marsh: &Marsh, yaw: f32) -> [f32; 2] {
    let reach = marsh.size[0].max(marsh.size[1]) as i64;
    let (cx, cz) = (marsh.centre[0].floor() as i64, marsh.centre[1].floor() as i64);
    // Towards the eye, on the ground (see `OrbitCamera::eye`).
    let (ex, ez) = yaw.sin_cos();
    let mut best: Option<(f32, [f32; 2])> = None;
    for dz in -reach..=reach {
        for dx in -reach..=reach {
            let (x, z) = world.column(cx + dx, cz + dz);
            if marsh.contains(x, z) || world.water_level(x, z).is_some() {
                continue;
            }
            let (fx, fz) = (dx as f32, dz as f32);
            let score = fx * fx + fz * fz - 3.0 * (fx * ex + fz * ez);
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, [x as f32 + 0.5, z as f32 + 0.5]));
            }
        }
    }
    best.map_or(marsh.centre, |(_, at)| at)
}

/// Generates the world, renders one frame offscreen, saves it as PNG.
fn capture(seed: u64, options: &CaptureOptions) -> Result<(), String> {
    let mut app = App::new(seed, false);
    app.clock = Clock::on_day(options.day, options.hour);
    if options.notebook {
        app.state.give_notebook(app.me);
    }
    if options.deer {
        app.state.teach(app.me, notebook::Spell::DeerForm);
        app.pending.push(Command::Cast {
            player: app.me,
            spell: notebook::Spell::DeerForm,
        });
    }
    // `--marsh`: the naturalist on the bank nearest the middle of the marsh (unless `--start`
    // says where), the camera on the water.
    let yaw = options.yaw.map_or(app.rig.camera.yaw, f32::to_radians);
    let marsh_view = app.marsh.as_ref().filter(|_| options.marsh).map(|marsh| {
        let bank = marsh_bank(&app.world, marsh, yaw);
        (marsh.centre, bank)
    });
    if marsh_view.is_none() && options.marsh {
        eprintln!("Ce monde n'a pas de marais.");
    }
    let start = options
        .start
        .or(marsh_view.map(|(_, bank)| (bank[0], bank[1])));
    if let Some((x, z)) = start {
        let dims = app.world.dims();
        let (xi, zi) = (
            x.clamp(0.0, dims.nx as f32 - 1.0),
            z.clamp(0.0, dims.nz as f32 - 1.0),
        );
        let y = app.world.ground_top(xi as usize, zi as usize) as f32 + 0.001;
        app.state.place(app.me, Vec3::new(x, y, z));
        app.previous_feet = Vec3::new(x, y, z);
        app.rig.snap(Vec3::new(x, y, z));
    }
    if options.rain {
        app.weather.forced = true;
        app.weather.start_shower();
    }
    if let Some(yaw) = options.yaw {
        app.rig.set_yaw(yaw.to_radians());
    }
    app.rig.set_zoom_factor(options.zoom);
    if let Some(pitch) = options.pitch {
        app.rig.set_pitch(pitch.to_radians());
    }
    if std::env::args().any(|a| a == "--test") {
        sandbox(&mut app);
    }
    // `--fast`: the day (and the plants) run 60 times faster during `--time`.
    app.clock.fast = std::env::args().any(|a| a == "--fast");
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
    if std::env::var_os("DISSIPATIF_DEBUG_DEER").is_some()
        && let Some(squirrels) = app.state.squirrels()
    {
        for q in &squirrels.squirrels {
            eprintln!("écureuil {:?} {:?}", q.position, q.doing);
        }
        eprintln!("caches : {}", squirrels.caches.len());
    }
    if std::env::var_os("DISSIPATIF_DEBUG_DEER").is_some()
        && let Some(herd) = app.state.herd()
    {
        for d in &herd.deer {
            eprintln!(
                "cerf {:?} {:?} tête {:.2} couché {:.2}",
                d.position, d.activity, d.head, d.lying
            );
        }
        eprintln!(
            "cercle {:?} lueur {:.2} vent vers {:?}",
            herd.ring,
            herd.glow,
            wind::direction(app.clock.days())
        );
    }
    if options.snow > 0.0 {
        app.weather.set_cover(options.snow, options.snow);
    }
    if std::env::var_os("DISSIPATIF_DEBUG_ANOMALIES").is_some() {
        match app.state.marsh() {
            Some(m) => eprintln!(
                "marais : centre ({:.1}, {:.1}), niveau {:.2}, {} cases d'eau, boîte {:?} + {:?}",
                m.centre[0],
                m.centre[1],
                m.level,
                m.cells.len(),
                m.origin,
                m.size
            ),
            None => eprintln!("marais : aucun"),
        }
    }
    // `--at` overrides the follow camera; `--marsh` looks at the middle of the water.
    let look = options
        .at
        .or(marsh_view.map(|(centre, _)| (centre[0], centre[1])));
    if let Some((x, z)) = look {
        app.rig.camera.target = Vec3::new(x, app.rig.camera.target.y, z);
    }
    let time = (still + walking) as f32 * CAPTURE_STEP;
    for (cell, flooded) in std::mem::take(&mut app.dug_cells) {
        app.data.ground_dug(&app.world, cell, flooded, None);
    }
    let mut renderer = Renderer::new(Gpu::offscreen(options.width, options.height));
    let dims = app.world.dims();
    renderer.set_world_size(dims.nx as f32, dims.nz as f32);
    app.data.install(&mut renderer);
    app.naturalist.install(&mut renderer);
    app.deer_view.install(&mut renderer);
    app.squirrel_view.install(&mut renderer);
    let mut motion = app.state.body(app.me).motion(time);
    motion.gesture = match options.pose {
        Some(kind) => Some(Gesture { kind, amount: 1.0 }),
        None => app.current_gesture(time),
    };
    app.draw_creatures(&mut renderer, &motion, time);
    let t = app.rig.camera.target;
    let reach = app.rig.camera.reach(renderer.aspect());
    app.data.set_view(glam::Vec2::new(t.x, t.z), reach);
    renderer.set_focus(app.focus());
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
        &app.clock,
        app.rig.camera.yaw,
        &app.message,
        app.bag_open || options.bag,
        None,
        &mut renderer,
    );
    let (ripples, prints) = app.traces.marks();
    renderer.set_marks(ripples, prints);
    renderer.upload_particles(&app.particles);
    renderer.render(&app.rig.camera, time);
    // The notebook's sketches come from this frame; `--notebook` shows it open, drawn again.
    app.take_sketches(&renderer);
    if options.notebook {
        let page = app
            .state
            .player(app.me)
            .and_then(|p| p.notebook.as_ref())
            .map_or(0, |b| b.pages.len().saturating_sub(1));
        draw_hud(
            &app.state,
            &app.world,
            app.me,
            app.selected,
            &app.clock,
            app.rig.camera.yaw,
            &app.message,
            false,
            Some((page, app.sketches.get(&page))),
            &mut renderer,
        );
        renderer.render(&app.rig.camera, time);
    }
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
    // `--new`: a new game (the save is overwritten at the next save).
    let mut app = App::new(options.seed, !args.iter().any(|a| a == "--new"));
    if args.iter().any(|a| a == "--test") {
        sandbox(&mut app);
    }
    event_loop
        .run_app(&mut app)
        .expect("exécution de la boucle d'événements");
}

/// Smoothstep on [0, 1]: eases in and out.
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_centre_of_the_screen_points_at_the_ground_the_camera_looks_at() {
        let world = World::generate(WorldConfig::small(1));
        let feet = player::spawn_point(&world);
        let camera = CameraRig::new(feet).camera;
        let hit = pointed_ground(&world, &camera, 1.6, glam::Vec2::ZERO).expect("no ground");
        // The camera looks at the chest: the ray meets the ground just beyond the feet.
        let offset = glam::Vec2::new(hit.x - feet.x, hit.z - feet.z).length();
        assert!(offset < 3.0, "pointed {offset} cells from the feet");
        assert!((hit.y - feet.y).abs() < 2.5);
    }

    /// Whether the column under point `at` (x, z) is dry.
    fn dry(world: &World, at: glam::Vec2) -> bool {
        let (x, z) = world.wrap(at.x, at.y);
        world.water_level(x as usize, z as usize).is_none()
    }

    #[test]
    fn every_world_has_a_meadow_and_a_marsh_a_walk_away_that_floods_nothing_it_should_not() {
        for seed in 1..=6 {
            let Generated {
                world,
                spawn,
                home,
                marsh,
            } = generate(WorldConfig::small(seed));
            let (ring, cover) = home.unwrap_or_else(|| panic!("world {seed}: no meadow"));
            let marsh = marsh.unwrap_or_else(|| panic!("world {seed}: no marsh"));
            // The circle and the meadow around it stay as the deer found them: dry.
            for dz in -6..=6 {
                for dx in -6..=6 {
                    let at = ring + glam::Vec2::new(dx as f32, dz as f32);
                    assert!(dry(&world, at), "world {seed}: water on the meadow at {at}");
                }
            }
            for (x, z, _) in deer::ring_cells(ring, seed) {
                assert!(!marsh.contains(x, z), "world {seed}: the circle is flooded");
            }
            // Where one starts, where the notebook lies, where the herd lies up.
            let start = glam::Vec2::new(spawn.x, spawn.z);
            let book = start + glam::Vec2::new(0.0, 1.4);
            for (place, at) in [("start", start), ("notebook", book), ("cover", cover)] {
                assert!(dry(&world, at), "world {seed}: the {place} at {at} is flooded");
            }
            let [x, z] = world.nearest(ring.to_array(), marsh.centre);
            let distance = ring.distance(glam::Vec2::new(x, z));
            assert!(
                (20.0..=90.0).contains(&distance),
                "world {seed}: the marsh is {distance} cells from the circle"
            );
        }
    }

    /// Where the marsh lies in the worlds of the game: `cargo test --release -p game
    /// marsh_census -- --ignored --nocapture`.
    #[test]
    #[ignore = "relevé : cargo test --release -p game marsh_census -- --ignored --nocapture"]
    fn marsh_census() {
        for seed in 1..=6 {
            let started = std::time::Instant::now();
            let Generated {
                world,
                spawn,
                home,
                marsh,
            } = generate(WorldConfig::standard(seed));
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let Some((ring, _)) = home else {
                eprintln!("graine {seed} : pas de prairie");
                continue;
            };
            let Some(m) = marsh else {
                eprintln!("graine {seed} : pas de marais (cercle {ring})");
                continue;
            };
            let [x, z] = world.nearest(ring.to_array(), m.centre);
            let (cx, cz) = (m.centre[0] as usize, m.centre[1] as usize);
            eprintln!(
                "graine {seed} : départ ({:.0}, {:.0}), cercle ({:.0}, {:.0}), marais ({:.0}, \
                 {:.0}) à {:.0} cases, {:?}, {} cases d'eau, boîte {:?}, niveau {:.2} ; \
                 monde fait en {elapsed:.0} ms",
                spawn.x,
                spawn.z,
                ring.x,
                ring.y,
                m.centre[0],
                m.centre[1],
                ring.distance(glam::Vec2::new(x, z)),
                world.biome(cx, cz),
                m.cells.len(),
                m.size,
                m.level,
            );
        }
    }
}
