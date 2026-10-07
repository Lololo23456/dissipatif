//! The state of the game that matters to everyone (what would be shared in multiplayer): the
//! players' bodies, bags and needs, what has been taken from the world, and the objects laid
//! in it with their physics (heat, fire, firing).
//!
//! It changes only in two ways, so that it could one day run on a host and be mirrored:
//! - `Command`s: every action of a player (steering, picking up, eating, drinking) is a
//!   command applied by `GameState::apply`; a key press only makes a command;
//! - fixed steps of time (`GameState::step`), the same length on every machine.
//!
//! It tells what happened through `Event`s, for the presentation (sounds, hiding a picked
//! flower, messages). Purely visual things (wind, particles, trampled grass) live outside.

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use sim::thermal::KELVIN;
use world::{Material, World};

use crate::anomaly::{
    Anomalies, Anomaly, Around, Harm, Kind, Life, SpellState, Understanding, WATCH_RANGE,
};
use crate::deer::{self, Herd, HerdEvent, Observer};
use crate::ecology::{Change, Ecology};
use crate::items::{ClaySource, Harvest, Inventory, Matter, Refusal, harvest};
use crate::needs::{self, Exposure, Needs};
use crate::notebook::{Entry, Notebook, Spell};
use crate::objects::Objects;
use crate::obstacles::Obstacles;
use crate::player::{Controls, Player};

pub type PlayerId = u32;

/// Fixed length of a step of the game, in seconds.
pub const STEP: f32 = 1.0 / 60.0;
/// How far the hands reach, horizontally, from the feet.
const REACH: f32 = 1.4;
/// Pebbles one can gather at the foot of a boulder (enough for a hearth).
const PEBBLES_PER_STONE: u32 = 5;
/// Seconds of rubbing with a fire drill to get an ember.
pub const RUB_SECONDS: f32 = 8.0;
/// How far in front of the feet things are laid, cells.
const LAY_DISTANCE: f32 = 0.8;
/// How far from the feet the hands reach when pointing (cells).
pub const ARM_REACH: f32 = 2.2;
/// What the hands can make, from what is held. The result follows the matter: a fine-grained
/// dark stone flakes well, a coarse light one mostly crumbles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
    /// Two lumps of wet clay into a dish.
    ShapeDish,
    /// Strike the held stone with another: a sharp flake, or chips.
    Knap,
    /// Pull straight sticks out of a bundle of twigs.
    PullSticks,
    /// Bind a flake to a stick with fibre: a knife.
    Haft,
}

impl Work {
    pub fn describe(self) -> &'static str {
        match self {
            Work::ShapeDish => "Modeler une coupelle",
            Work::Knap => "Tailler la pierre (frapper avec une autre)",
            Work::PullSticks => "Tirer des baguettes du fagot",
            Work::Haft => "Emmancher l'éclat (baguette et fibre)",
        }
    }
}

/// Chance that a strike gives a sharp flake: fine-grained dark rock, coarse light stone.
const FLAKE_CHANCE_DARK: f32 = 0.7;
const FLAKE_CHANCE_LIGHT: f32 = 0.15;
/// Sticks pulled out of a bundle of twigs.
const STICKS_PER_BUNDLE: u32 = 3;

/// Lumps of clay a dish takes.
const CLAY_PER_DISH: u32 = 2;

pub struct PlayerState {
    pub id: PlayerId,
    pub body: Player,
    pub inventory: Inventory,
    pub needs: Needs,
    controls: Controls,
    camera_yaw: f32,
    /// Seconds of rubbing towards an ember, while the drill key is held.
    pub rubbing: f32,
    /// The notebook, once found.
    pub notebook: Option<Notebook>,
    /// Spells understood, for good: one whose anomaly dies stays here, lost, and answers
    /// again when it is born again (see `GameState::spell_state`).
    pub spells: Vec<Spell>,
    /// Shapes taken: each spell learnt the first time adds one, and nothing takes one back.
    /// The notebook loses words with each (the naturalist's human syntax goes).
    pub transformations: u32,
    /// Game seconds spent in the trial of each kind of anomaly (the rite: watching its height
    /// unseen).
    trials: Vec<(Kind, f32)>,
    /// The anomaly whose trial they are in, this step (not saved: found again at the next).
    attending: Option<Kind>,
}

impl PlayerState {
    /// Game seconds spent in the trial of `kind`.
    fn trial(&self, kind: Kind) -> f32 {
        self.trials
            .iter()
            .find(|t| t.0 == kind)
            .map_or(0.0, |t| t.1)
    }

    fn trial_mut(&mut self, kind: Kind) -> &mut f32 {
        let i = match self.trials.iter().position(|t| t.0 == kind) {
            Some(i) => i,
            None => {
                self.trials.push((kind, 0.0));
                self.trials.len() - 1
            }
        };
        &mut self.trials[i].1
    }

    /// Understands `spell`, the first time only: it joins the spells, the count of shapes
    /// taken grows, the notebook writes its page. The way every anomaly teaches.
    fn learn(&mut self, spell: Spell, now: &Conditions, events: &mut Vec<Event>) {
        if self.spells.contains(&spell) {
            return;
        }
        self.spells.push(spell);
        self.transformations += 1;
        events.push(Event::Learnt {
            player: self.id,
            spell,
        });
        self.write(Entry::Learnt(spell), now, events);
    }

    /// The notebook, if they have it, writes `entry` where they stand.
    fn write(&mut self, entry: Entry, now: &Conditions, events: &mut Vec<Event>) {
        let feet = Vec2::new(self.body.position.x, self.body.position.z);
        if let Some(book) = self.notebook.as_mut()
            && let Some(page) = book.write(entry, now, feet)
        {
            events.push(Event::Wrote {
                player: self.id,
                page,
                entry,
            });
        }
    }

    /// Ends what `spell` keeps going on them (a shape taken), now that it no longer answers.
    fn release(&mut self, spell: Spell) {
        match spell {
            Spell::DeerForm => self.body.deer = false,
            // Nothing of the owl's eye stays on a player once cast.
            Spell::OwlEye => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// What the player's keys hold this step, and where their camera looks from.
    Steer {
        player: PlayerId,
        controls: Controls,
        camera_yaw: f32,
    },
    /// Pick up the nearest thing within reach.
    Pick { player: PlayerId },
    /// Eat one of slot `slot` of the bag.
    Eat { player: PlayerId, slot: usize },
    /// Drink from water within reach.
    Drink { player: PlayerId },
    /// Lay one of slot `slot` on the ground just in front (on top of what is there).
    Lay { player: PlayerId, slot: usize },
    /// Lay one of slot `slot` at a point the player points at (the mouse), within arm's reach.
    LayAt {
        player: PlayerId,
        slot: usize,
        at: Vec2,
    },
    /// Take what lies at a point (a laid object, a plant, a handful of ground), within reach.
    PickAt { player: PlayerId, at: Vec2 },
    /// Work what slot `slot` holds with the hands (see `Work`).
    Work { player: PlayerId, slot: usize },
    /// Cast a spell (again to end one that lasts, like a shape).
    Cast { player: PlayerId, spell: Spell },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    NothingInReach,
    Bag(Refusal),
    NotEdible,
    NoWater,
    /// Too hot to take in the hand.
    TooHot,
    /// Nowhere to lay it in front (water, a drop).
    CannotLay,
    /// Nothing that can be shaped (or not enough of it).
    /// Nothing the hands can work from what is held.
    NothingToWork,
    /// The fire drill gives no ember in the rain.
    TooWet,
    /// In the shape of a deer: no hands.
    NoHands,
    /// The spell no longer answers: the anomaly that taught it is dead.
    SpellLost,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// `removed`: the plant (index in `World::plants`) taken away from the world, if any.
    Picked {
        player: PlayerId,
        matter: Matter,
        removed: Option<usize>,
    },
    Ate {
        player: PlayerId,
        matter: Matter,
    },
    Drank {
        player: PlayerId,
    },
    Laid {
        player: PlayerId,
        matter: Matter,
    },
    /// A laid object taken back into the bag.
    TookBack {
        player: PlayerId,
        matter: Matter,
    },
    Worked {
        player: PlayerId,
        work: Work,
        /// What came of it (a dish, a flake or only chips, sticks, a knife).
        made: Matter,
    },
    /// The fire drill gave an ember, on the fuel at hand.
    Ember {
        player: PlayerId,
    },
    /// A plant cleared away (laid on, dug up, burnt): index in the plant list.
    Cleared {
        plant: usize,
    },
    /// A plant sprouted, grew or shrank, or died of itself (`ecology::Change`).
    Plant(Change),
    /// A handful of ground was taken at `at`: the world must dig a micro-voxel out there
    /// (`World::dig`) and redraw the ground.
    Dig {
        at: Vec2,
    },
    /// An animal called (a deer barked, stamped, a stag belled), there.
    Call {
        call: crate::sound::AnimalCall,
        at: Vec2,
    },
    /// The surface of a column changed: worn to bare earth, or grown over again.
    Ground {
        x: usize,
        z: usize,
        material: Material,
    },
    /// Physics changed an object (fired, burst, burnt to ash).
    Changed {
        from: Matter,
        to: Matter,
    },
    Failed {
        player: PlayerId,
        failure: Failure,
    },
    /// The notebook wrote page `page` by itself (the first: it was just found).
    Wrote {
        player: PlayerId,
        page: usize,
        entry: Entry,
    },
    /// A spell understood.
    Learnt {
        player: PlayerId,
        spell: Spell,
    },
    /// A spell cast: `on` false when a lasting one ends.
    Cast {
        player: PlayerId,
        spell: Spell,
        on: bool,
    },
    /// A spell no longer answers: the anomaly that taught it died (a shape taken was left).
    SpellLost {
        player: PlayerId,
        spell: Spell,
    },
    /// A lost spell answers again: its anomaly was born again.
    SpellRegained {
        player: PlayerId,
        spell: Spell,
    },
}

/// The moment: time, sky and weather, the same for everything in the world.
#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    /// Hour of the day, in [0, 24).
    pub hour: f32,
    /// Days since the start, fractional.
    pub days: f64,
    /// Moon phase in [0, 1): 0 new, 0.5 full.
    pub moon: f32,
    /// Rain, in [0, 1].
    pub rain: f32,
    /// Direction the wind blows towards, unit.
    pub wind: Vec2,
    /// Phase of the year in [0, 1): 0 spring, 0.25 summer, 0.5 autumn, 0.75 winter.
    pub year: f32,
}

impl Conditions {
    /// A dry noon on the first day, under a full moon's phase: for tests.
    #[cfg(test)]
    pub fn noon() -> Self {
        Self {
            hour: 12.0,
            days: 0.5,
            moon: 0.5,
            rain: 0.0,
            wind: Vec2::new(0.89, 0.45),
            year: 0.375,
        }
    }
}

pub struct GameState {
    players: Vec<PlayerState>,
    /// The plants of the world, living (they grow, spread and die: see `ecology`); the first
    /// ones are `World::plants`, sprouted ones follow.
    plants: Vec<world::PlantInstance>,
    /// Plants gone (taken, dead), by index in `plants`.
    removed: Vec<bool>,
    /// How the plants live: growth, seeds, death.
    ecology: Ecology,
    /// How much faster than real time the day runs (fast-forward): the plants follow.
    pub time_scale: f32,
    /// Scratch list of the ecology's changes (reused).
    plant_changes: Vec<Change>,
    pebbles_taken: HashMap<usize, u32>,
    obstacles: Obstacles,
    /// Things that can be picked, by column (x, z).
    pickables: HashMap<(i64, i64), Vec<usize>>,
    next_id: PlayerId,
    /// Things laid in the world, and their physics.
    objects: Objects,
    /// Rain at the last step (whether a fire drill works).
    rain: f32,
    /// What happened during the steps, until the presentation reads it.
    events: Vec<Event>,
    /// Scratch list of the objects' changes (reused: no allocation per step).
    changes: Vec<(Vec3, Matter, Matter)>,
    /// Chance in the players' gestures (knapping), seeded: deterministic.
    rng: sim::rng::SplitMix64,
    /// The deer of the meadow, if the world has one.
    herd: Option<Herd>,
    /// The squirrels and their caches.
    squirrels: Option<crate::squirrels::Squirrels>,
    squirrel_events: Vec<crate::squirrels::SquirrelEvent>,
    /// Columns the game laid bare (worn, grazed out, burnt): they may grow over again.
    bared: std::collections::HashSet<(usize, usize)>,
    /// Game seconds until the ground is looked over again.
    ground_in: f32,
    herd_events: Vec<HerdEvent>,
    /// The notebook lying on the ground, until someone picks it up.
    notebook_lying: Option<Vec3>,
    /// The moment of the last step.
    now: Conditions,
    /// The anomalies of the world: their places, health, strength and nights.
    anomalies: Anomalies,
    /// Scratch lists of each step (reused): where something burns, the players as animals
    /// perceive them (in the order of `players`), the anomalies whose life changed.
    fires: Vec<Vec2>,
    observers: Vec<Observer>,
    life_changes: Vec<(Kind, Life)>,
}

impl GameState {
    pub fn new(world: &World) -> Self {
        let mut pickables: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        let plants: Vec<world::PlantInstance> = world.plants().to_vec();
        for (i, p) in plants.iter().enumerate() {
            if harvest(p).is_some() {
                let (x, z) = position(p);
                pickables
                    .entry((x.floor() as i64, z.floor() as i64))
                    .or_default()
                    .push(i);
            }
        }
        Self {
            players: Vec::new(),
            removed: vec![false; plants.len()],
            ecology: Ecology::new(world, &plants, world.config.seed ^ 0xec0),
            time_scale: 1.0,
            plant_changes: Vec::new(),
            plants,
            pebbles_taken: HashMap::new(),
            obstacles: Obstacles::from_world(world),
            pickables,
            next_id: 0,
            objects: Objects::default(),
            rain: 0.0,
            events: Vec::new(),
            changes: Vec::new(),
            rng: sim::rng::SplitMix64::new(world.config.seed ^ 0x4ea9),
            herd: None,
            squirrels: None,
            squirrel_events: Vec::new(),
            bared: std::collections::HashSet::new(),
            ground_in: 0.0,
            herd_events: Vec::new(),
            notebook_lying: None,
            now: Conditions {
                hour: 12.0,
                days: 0.5,
                moon: 0.0,
                rain: 0.0,
                wind: Vec2::X,
                year: 0.375,
            },
            anomalies: Anomalies::default(),
            fires: Vec::new(),
            observers: Vec::new(),
            life_changes: Vec::new(),
        }
    }

    /// Saves everything that lives and changes (the world itself is saved apart).
    pub fn save(&self, w: &mut crate::save::Writer) {
        w.put(&self.players);
        w.put(&self.plants);
        w.put(&self.removed);
        self.ecology.save(w);
        w.put(&self.pebbles_taken);
        w.put(&self.next_id);
        w.put(&self.objects);
        w.put(&self.rain);
        w.put(&self.rng);
        w.put(&self.herd);
        w.put(&self.squirrels);
        let mut bared: Vec<(usize, usize)> = self.bared.iter().copied().collect();
        bared.sort();
        w.put(&bared);
        w.put(&self.notebook_lying);
        w.put(&self.now);
        w.put(&self.anomalies);
    }

    /// Reads back what `save` wrote, in `world` (restored first).
    pub fn load(r: &mut crate::save::Reader, world: &World) -> crate::save::Result<Self> {
        let players: Vec<PlayerState> = r.get()?;
        let plants: Vec<world::PlantInstance> = r.get()?;
        let removed: Vec<bool> = r.get()?;
        if removed.len() != plants.len() {
            return Err("plantes incohérentes".into());
        }
        let ecology = Ecology::load(r, world, &plants)?;
        let mut state = Self::new(world);
        state.players = players;
        state.ecology = ecology;
        state.pebbles_taken = r.get()?;
        state.next_id = r.get()?;
        state.objects = r.get()?;
        state.rain = r.get()?;
        state.rng = r.get()?;
        state.herd = r.get()?;
        state.squirrels = r.get()?;
        state.bared = r.get::<Vec<(usize, usize)>>()?.into_iter().collect();
        state.notebook_lying = r.get()?;
        state.now = r.get()?;
        state.anomalies = r.get()?;
        // What is rebuilt rather than saved: what can be picked, and the stones in the way.
        state.pickables.clear();
        for (i, p) in plants.iter().enumerate() {
            if harvest(p).is_some() {
                let (x, z) = position(p);
                state
                    .pickables
                    .entry((x.floor() as i64, z.floor() as i64))
                    .or_default()
                    .push(i);
            }
        }
        for (i, &gone) in removed.iter().enumerate() {
            if gone {
                state.obstacles.remove(i);
            }
        }
        state.plants = plants;
        state.removed = removed;
        Ok(state)
    }

    /// Lets a herd of deer live in the world, and the rite it carries on its meadow: alive, the
    /// world being at its balance (its health is measured at the first step).
    pub fn add_herd(&mut self, herd: Herd) {
        self.anomalies.register(Anomaly::new(
            Kind::DeerRite,
            herd.ring,
            crate::anomaly::RITE_MEADOW,
            Life::Alive,
        ));
        self.herd = Some(herd);
    }

    pub fn anomalies(&self) -> &Anomalies {
        &self.anomalies
    }

    /// Sets the life of the anomaly of `kind` at once, with all that follows (spells lost or
    /// answering again): for tests and captures.
    pub fn force_life(&mut self, kind: Kind, life: Life) {
        if self.anomalies.force_life(kind, life, self.now.days) {
            self.spells_follow(kind, life);
        }
    }

    /// Where player `id` stands with `spell`: unknown, usable, or lost with its anomaly (a
    /// spell no anomaly of the world teaches stays usable).
    pub fn spell_state(&self, id: PlayerId, spell: Spell) -> SpellState {
        if !self.player(id).is_some_and(|p| p.spells.contains(&spell)) {
            return SpellState::Unknown;
        }
        let alive = Kind::teaching(spell)
            .and_then(|kind| self.anomalies.get(kind))
            .is_none_or(|a| a.life == Life::Alive);
        if alive {
            SpellState::Usable
        } else {
            SpellState::Lost
        }
    }

    /// Lets squirrels live in the world.
    pub fn add_squirrels(&mut self, squirrels: crate::squirrels::Squirrels) {
        self.squirrels = Some(squirrels);
    }

    pub fn squirrels(&self) -> Option<&crate::squirrels::Squirrels> {
        self.squirrels.as_ref()
    }

    /// Whether player `id` could dig up a squirrel's cache right in front of them.
    pub fn cache_in_reach(&self, id: PlayerId) -> bool {
        match (&self.squirrels, self.hands(id)) {
            (Some(s), Some(hands)) => s.cache_near(hands, CACHE_REACH),
            _ => false,
        }
    }

    pub fn herd(&self) -> Option<&Herd> {
        self.herd.as_ref()
    }

    /// Lays the notebook on the ground at `at`.
    pub fn place_notebook(&mut self, at: Vec3) {
        self.notebook_lying = Some(at);
    }

    /// Puts the notebook in player `id`'s hands at once, wherever it lies (for captures).
    pub fn give_notebook(&mut self, id: PlayerId) {
        if let Some(at) = self.notebook_lying.take() {
            let found = Vec2::new(at.x, at.z);
            let now = self.now;
            if let Some(p) = self.player_mut(id) {
                let mut book = Notebook::new(found);
                book.write(Entry::Found, &now, found);
                p.notebook = Some(book);
            }
        }
    }

    /// Gives player `id` a spell, without a page (for tests and captures): a shape taken all
    /// the same.
    pub fn teach(&mut self, id: PlayerId, spell: Spell) {
        if let Some(p) = self.player_mut(id)
            && !p.spells.contains(&spell)
        {
            p.spells.push(spell);
            p.transformations += 1;
        }
    }

    /// The anomaly player `id` is understanding now, how far, and where it is: `None` when not
    /// in a trial (for the light of it).
    pub fn understanding(&self, id: PlayerId) -> Option<Understanding> {
        let p = self.player(id)?;
        let kind = p.attending?;
        let anomaly = self.anomalies.get(kind)?;
        Some(Understanding {
            kind,
            progress: (p.trial(kind) / kind.trial_seconds()).min(1.0),
            site: anomaly.site,
        })
    }

    /// Whether player `id` can pick up the notebook lying there.
    pub fn notebook_in_reach(&self, id: PlayerId) -> bool {
        match (self.notebook_lying, self.hands(id)) {
            (Some(at), Some(hands)) => hands.distance(Vec2::new(at.x, at.z)) < ARM_REACH,
            _ => false,
        }
    }

    /// Where the notebook lies, if nobody has it yet.
    pub fn notebook_lying(&self) -> Option<Vec3> {
        self.notebook_lying
    }

    pub fn objects(&self) -> &Objects {
        &self.objects
    }

    /// What happened during the last steps (each event once).
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, Event> {
        self.events.drain(..)
    }

    /// Whether player `id` carries something that cuts.
    pub fn has_blade(&self, id: PlayerId) -> bool {
        self.player(id).is_some_and(|p| {
            p.inventory
                .stacks()
                .iter()
                .any(|s| matches!(s.matter, Matter::Knife { .. } | Matter::Flake))
        })
    }

    /// What plant `i` gives now, if anything (pebbles run out; a bush needs a blade).
    fn available(&self, i: usize, plant: &world::PlantInstance, blade: bool) -> Option<Matter> {
        match harvest(plant)? {
            Harvest::Whole(m) => Some(m),
            Harvest::Part(m) => {
                (self.pebbles_taken.get(&i).copied().unwrap_or(0) < PEBBLES_PER_STONE).then_some(m)
            }
            Harvest::NeedsBlade(m) => blade.then_some(m),
        }
    }

    /// What working slot `slot` with the hands would do now, if anything.
    pub fn work_plan(&self, id: PlayerId, slot: usize) -> Option<Work> {
        let p = self.player(id)?;
        let stacks = p.inventory.stacks();
        let held = stacks.get(slot)?;
        let count = |f: fn(Matter) -> bool| -> u32 {
            stacks.iter().filter(|s| f(s.matter)).map(|s| s.count).sum()
        };
        match held.matter {
            Matter::Clay { .. } if held.count >= CLAY_PER_DISH => Some(Work::ShapeDish),
            // Knapping: the held stone struck with another hard stone.
            Matter::Pebble { .. } if count(|m| matches!(m, Matter::Pebble { .. })) >= 2 => {
                Some(Work::Knap)
            }
            Matter::DeadTwigs => Some(Work::PullSticks),
            Matter::Flake
                if count(|m| m == Matter::Stick) >= 1
                    && count(|m| m.properties().flexibility >= 0.8) >= 1 =>
            {
                Some(Work::Haft)
            }
            _ => None,
        }
    }

    /// Where player `id` would lay something: on the ground (or on what lies there) a little
    /// in front of the feet. None over water or a drop.
    pub fn lay_point(&self, world: &World, id: PlayerId) -> Option<Vec3> {
        let body = &self.player(id)?.body;
        let facing = body.facing();
        let at = Vec2::new(body.position.x, body.position.z)
            + Vec2::new(facing.sin(), facing.cos()) * LAY_DISTANCE;
        let dims = world.dims();
        if at.x < 1.0 || at.y < 1.0 || at.x >= dims.nx as f32 - 1.0 || at.y >= dims.nz as f32 - 1.0
        {
            return None;
        }
        let (x, z) = (at.x as usize, at.y as usize);
        if world.water_level(x, z).is_some() {
            return None;
        }
        let ground = world.surface_height(at.x, at.y);
        if (ground - body.position.y).abs() > 1.1 {
            return None;
        }
        Some(self.objects.resting_point(at.x, at.y, ground))
    }

    /// Where something laid at `at` would rest, if `at` is within player `id`'s reach, on
    /// dry ground roughly level with them.
    pub fn lay_point_at(&self, world: &World, id: PlayerId, at: Vec2) -> Option<Vec3> {
        let body = &self.player(id)?.body;
        let feet = Vec2::new(body.position.x, body.position.z);
        if feet.distance(at) > ARM_REACH {
            return None;
        }
        let dims = world.dims();
        if at.x < 1.0 || at.y < 1.0 || at.x >= dims.nx as f32 - 1.0 || at.y >= dims.nz as f32 - 1.0
        {
            return None;
        }
        let (x, z) = (at.x as usize, at.y as usize);
        if world.water_level(x, z).is_some() {
            return None;
        }
        let ground = world.surface_height(at.x, at.y);
        if (ground - body.position.y).abs() > 1.6 {
            return None;
        }
        Some(self.objects.resting_point(at.x, at.y, ground))
    }

    /// The laid object at `at` (within a few centimetres of its centre), if any.
    pub fn object_at(&self, at: Vec2) -> Option<usize> {
        self.objects
            .placed()
            .iter()
            .enumerate()
            .map(|(i, p)| (i, Vec2::new(p.base.x, p.base.z).distance(at)))
            .filter(|&(i, d)| d < self.objects.body(i).radius + 0.12)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// The plant to gather nearest to `at` (within half a cell), if any.
    pub fn plant_at(&self, id: PlayerId, at: Vec2) -> Option<(usize, Matter)> {
        let blade = self.has_blade(id);
        let (cx, cz) = (at.x.floor() as i64, at.y.floor() as i64);
        let mut best: Option<(f32, usize, Matter)> = None;
        for z in cz - 1..=cz + 1 {
            for x in cx - 1..=cx + 1 {
                for &i in self.pickables.get(&(x, z)).into_iter().flatten() {
                    if self.removed[i] {
                        continue;
                    }
                    let plant = &self.plants[i];
                    let Some(matter) = self.available(i, plant, blade) else {
                        continue;
                    };
                    let (px, pz) = position(plant);
                    let d = Vec2::new(px, pz).distance(at);
                    if d < 0.5 && best.is_none_or(|b| d < b.0) {
                        best = Some((d, i, matter));
                    }
                }
            }
        }
        best.map(|(_, i, m)| (i, m))
    }

    /// The laid object nearest player `id`'s hands, if within reach.
    pub fn object_in_reach(&self, id: PlayerId) -> Option<usize> {
        let body = &self.player(id)?.body;
        let facing = body.facing();
        let hands = Vec2::new(body.position.x, body.position.z)
            + Vec2::new(facing.sin(), facing.cos()) * (LAY_DISTANCE * 0.6);
        self.objects
            .placed()
            .iter()
            .enumerate()
            .map(|(i, p)| (i, Vec2::new(p.base.x, p.base.z).distance(hands)))
            .filter(|&(_, d)| d < 0.9)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Where player `id`'s hands are, on the ground in front.
    pub fn hands(&self, id: PlayerId) -> Option<Vec2> {
        let body = &self.player(id)?.body;
        let facing = body.facing();
        Some(
            Vec2::new(body.position.x, body.position.z)
                + Vec2::new(facing.sin(), facing.cos()) * (LAY_DISTANCE * 0.6),
        )
    }

    /// Whether player `id` would blow (something glows at their hands) rather than rub.
    pub fn would_blow(&self, id: PlayerId) -> bool {
        self.hands(id)
            .is_some_and(|h| self.objects.glowing_near(h.x, h.y))
    }

    /// Where a fire drill's ember would fall: into the tinder, the dry fuel within reach that
    /// catches most easily (lowest ignition temperature), nearest the hands if several.
    pub fn rub_target(&self, id: PlayerId) -> Option<usize> {
        let body = &self.player(id)?.body;
        let facing = body.facing();
        let hands = Vec2::new(body.position.x, body.position.z)
            + Vec2::new(facing.sin(), facing.cos()) * (LAY_DISTANCE * 0.6);
        self.objects
            .placed()
            .iter()
            .enumerate()
            .filter(|&(i, _)| {
                let b = self.objects.body(i);
                b.fuel.is_some_and(|f| f.mass > 0.0) && b.water < 1e-4 && !b.burning
            })
            .map(|(i, p)| (i, Vec2::new(p.base.x, p.base.z).distance(hands)))
            .filter(|&(_, d)| d < 0.9)
            .min_by(|a, b| {
                let ignition =
                    |i: usize| self.objects.body(i).fuel.map_or(f32::MAX, |f| f.ignition);
                ignition(a.0)
                    .total_cmp(&ignition(b.0))
                    .then(a.1.total_cmp(&b.1))
            })
            .map(|(i, _)| i)
    }

    /// What player `id` could take from the ground under their feet: clay on a river bank or
    /// in a clay soil, sand on a beach or in the desert.
    pub fn ground_sample(&self, world: &World, id: PlayerId) -> Option<Matter> {
        self.ground_sample_at(world, self.dig_point(id)?)
    }

    /// Where player `id` digs a handful: on the ground in front, where things are laid.
    pub fn dig_point(&self, id: PlayerId) -> Option<Vec2> {
        let body = &self.player(id)?.body;
        let facing = body.facing();
        Some(
            Vec2::new(body.position.x, body.position.z)
                + Vec2::new(facing.sin(), facing.cos()) * LAY_DISTANCE,
        )
    }

    /// What a handful of the ground at `at` would be: clay on a river bank or in a clay
    /// soil, sand on a beach or in the desert.
    pub fn ground_sample_at(&self, world: &World, at: Vec2) -> Option<Matter> {
        let feet = Vec3::new(at.x, 0.0, at.y);
        let dims = world.dims();
        let (x, z) = (feet.x.floor(), feet.z.floor());
        if x < 1.0 || z < 1.0 || x >= dims.nx as f32 - 1.0 || z >= dims.nz as f32 - 1.0 {
            return None;
        }
        let (x, z) = (x as usize, z as usize);
        let top = world.ground_top(x, z);
        if top == 0 {
            return None;
        }
        let near_water = (0..9).any(|k| world.water_level(x + k % 3 - 1, z + k / 3 - 1).is_some());
        match world.block(x, top - 1, z) {
            Material::Sand | Material::DesertSand => Some(Matter::Sand),
            Material::Clay | Material::DryGrass => Some(Matter::Clay {
                source: ClaySource::RedEarth,
            }),
            Material::Grass | Material::ForestFloor | Material::Dirt if near_water => {
                Some(Matter::Clay {
                    source: ClaySource::Bank,
                })
            }
            _ => None,
        }
    }

    /// A new player standing at `feet`.
    pub fn join(&mut self, feet: Vec3) -> PlayerId {
        let id = self.next_id;
        self.next_id += 1;
        self.players.push(PlayerState {
            id,
            body: Player::new(feet),
            inventory: Inventory::default(),
            needs: Needs::default(),
            controls: Controls::default(),
            camera_yaw: 0.0,
            rubbing: 0.0,
            notebook: None,
            spells: Vec::new(),
            transformations: 0,
            trials: Vec::new(),
            attending: None,
        });
        id
    }

    /// The body of player `id`. Ids are indices in `players`, and players are never removed:
    /// any id returned by `join` is valid.
    pub fn body(&self, id: PlayerId) -> &Player {
        &self.players[id as usize].body
    }

    pub fn player(&self, id: PlayerId) -> Option<&PlayerState> {
        self.players.iter().find(|p| p.id == id)
    }

    fn player_mut(&mut self, id: PlayerId) -> Option<&mut PlayerState> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    /// Puts `matter` straight into player `id`'s bag, bypassing the world: a development tool
    /// (captures, tests), never used by the game itself.
    pub fn give(&mut self, id: PlayerId, matter: Matter) {
        if let Some(p) = self.player_mut(id) {
            let _ = p.inventory.add(matter);
        }
    }

    /// Moves a player's body to `feet` (tools, captures).
    pub fn place(&mut self, id: PlayerId, feet: Vec3) {
        if let Some(p) = self.player_mut(id) {
            p.body = Player::new(feet);
        }
    }

    /// Whether plant `plant` is gone (taken, dead).
    pub fn is_removed(&self, plant: usize) -> bool {
        self.removed.get(plant).copied().unwrap_or(false)
    }

    /// The thing player `id` would pick up now: the nearest within reach, those in front
    /// first. Its index in `World::plants` and what it would give.
    pub fn target(&self, id: PlayerId) -> Option<(usize, Matter)> {
        let blade = self.has_blade(id);
        let p = self.player(id)?;
        let feet = p.body.position;
        let facing = p.body.facing();
        let ahead = Vec2::new(facing.sin(), facing.cos());
        let (cx, cz) = (feet.x.floor() as i64, feet.z.floor() as i64);
        let mut best: Option<(f32, usize, Matter)> = None;
        for z in cz - 2..=cz + 2 {
            for x in cx - 2..=cx + 2 {
                let Some(list) = self.pickables.get(&(x, z)) else {
                    continue;
                };
                for &i in list {
                    if self.removed[i] {
                        continue;
                    }
                    let plant = &self.plants[i];
                    let Some(matter) = self.available(i, plant, blade) else {
                        continue;
                    };
                    let (px, pz) = position(plant);
                    let offset = Vec2::new(px - feet.x, pz - feet.z);
                    let distance = offset.length();
                    let height = plant.base[1] as f32 - feet.y;
                    if distance > REACH || !(-1.5..=1.0).contains(&height) {
                        continue;
                    }
                    // Prefer what lies in front of the naturalist.
                    let front = offset.normalize_or_zero().dot(ahead);
                    let score = distance - 0.5 * front;
                    if best.is_none_or(|b| score < b.0) {
                        best = Some((score, i, matter));
                    }
                }
            }
        }
        best.map(|(_, i, m)| (i, m))
    }

    /// Whether player `id` can drink here: water at their feet or within reach, not far
    /// below them.
    pub fn can_drink(&self, world: &World, id: PlayerId) -> bool {
        let Some(p) = self.player(id) else {
            return false;
        };
        let feet = p.body.position;
        let dims = world.dims();
        for dz in [-1.0f32, 0.0, 1.0] {
            for dx in [-1.0f32, 0.0, 1.0] {
                let (x, z) = (feet.x + dx, feet.z + dz);
                if x < 0.0 || z < 0.0 || x >= dims.nx as f32 || z >= dims.nz as f32 {
                    continue;
                }
                if let Some(level) = world.water_level(x as usize, z as usize)
                    && level > feet.y - 1.2
                {
                    return true;
                }
            }
        }
        false
    }

    /// Applies a command. Returns what happened, if anything worth telling.
    pub fn apply(&mut self, world: &World, command: Command) -> Option<Event> {
        // In the shape of a deer, nothing can be held or worked.
        if let Command::Pick { player }
        | Command::PickAt { player, .. }
        | Command::Lay { player, .. }
        | Command::LayAt { player, .. }
        | Command::Work { player, .. }
        | Command::Eat { player, .. } = command
            && self.player(player).is_some_and(|p| p.body.deer)
        {
            return Some(Event::Failed {
                player,
                failure: Failure::NoHands,
            });
        }
        match command {
            Command::Cast { player, spell } => {
                let state = self.spell_state(player, spell);
                let p = self.player_mut(player)?;
                match state {
                    SpellState::Unknown => return None,
                    // A lost spell no longer answers; a shape still worn can always be left.
                    SpellState::Lost if !(spell == Spell::DeerForm && p.body.deer) => {
                        return Some(Event::Failed {
                            player,
                            failure: Failure::SpellLost,
                        });
                    }
                    SpellState::Lost | SpellState::Usable => {}
                }
                match spell {
                    Spell::DeerForm => {
                        p.body.deer = !p.body.deer;
                        Some(Event::Cast {
                            player,
                            spell,
                            on: p.body.deer,
                        })
                    }
                    // The owl's eye has no effect of its own yet: it opens with the spell
                    // wheel, the way to see and hear the night (see `docs/phase-2.md`).
                    Spell::OwlEye => None,
                }
            }
            Command::Steer {
                player,
                controls,
                camera_yaw,
            } => {
                let p = self.player_mut(player)?;
                // A jump is kept until a step uses it.
                let jump = p.controls.jump || controls.jump;
                p.controls = Controls { jump, ..controls };
                p.camera_yaw = camera_yaw;
                None
            }
            Command::Pick { player } => {
                // A squirrel's cache right in front: dig up the nut.
                if self.cache_in_reach(player) {
                    let hands = self.hands(player)?;
                    let matter = match self.squirrels.as_mut()?.dig_up(hands, CACHE_REACH)? {
                        crate::squirrels::Nut::Acorn => Matter::Acorn,
                        crate::squirrels::Nut::Hazelnut => Matter::Hazelnut,
                    };
                    let p = self.player_mut(player)?;
                    if let Err(refusal) = p.inventory.add(matter) {
                        return Some(Event::Failed {
                            player,
                            failure: Failure::Bag(refusal),
                        });
                    }
                    return Some(Event::Picked {
                        player,
                        matter,
                        removed: None,
                    });
                }
                // The notebook, if it lies there.
                if let (Some(at), Some(hands)) = (self.notebook_lying, self.hands(player))
                    && hands.distance(Vec2::new(at.x, at.z)) < ARM_REACH
                {
                    self.notebook_lying = None;
                    let found = Vec2::new(at.x, at.z);
                    let now = self.now;
                    let p = self.player_mut(player)?;
                    let mut book = Notebook::new(found);
                    let page = book.write(Entry::Found, &now, found)?;
                    p.notebook = Some(book);
                    return Some(Event::Wrote {
                        player,
                        page,
                        entry: Entry::Found,
                    });
                }
                // A laid object within reach first: taking it back.
                if let Some(i) = self.object_in_reach(player) {
                    return self.take_back(player, i);
                }
                let Some((plant, matter)) = self.target(player) else {
                    // Nothing to gather: a handful of the ground, if worth taking.
                    let Some(matter) = self.ground_sample(world, player) else {
                        return Some(Event::Failed {
                            player,
                            failure: Failure::NothingInReach,
                        });
                    };
                    let p = self.player_mut(player)?;
                    if let Err(refusal) = p.inventory.add(matter) {
                        return Some(Event::Failed {
                            player,
                            failure: Failure::Bag(refusal),
                        });
                    }
                    let at = self.dig_point(player)?;
                    self.dig(world, at);
                    return Some(Event::Picked {
                        player,
                        matter,
                        removed: None,
                    });
                };
                self.gather(player, plant, matter)
            }
            Command::Eat { player, slot } => {
                let p = self.player_mut(player)?;
                let matter = p.inventory.stacks().get(slot)?.matter;
                if !matter.edible() {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::NotEdible,
                    });
                }
                p.inventory.take(slot);
                let properties = matter.properties();
                p.needs.eat(properties.nutrition, properties.toxicity);
                Some(Event::Ate { player, matter })
            }
            Command::LayAt { player, slot, at } => {
                // The point as seen from the player, across the edges if need be.
                let feet = self.player(player)?.body.position;
                let at = Vec2::from(world.nearest([feet.x, feet.z], at.to_array()));
                let Some(at) = self.lay_point_at(world, player, at) else {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::CannotLay,
                    });
                };
                self.lay_matter(world, player, slot, at)
            }
            Command::PickAt { player, at } => {
                let feet = self.player(player)?.body.position;
                let at = Vec2::from(world.nearest([feet.x, feet.z], at.to_array()));
                if Vec2::new(feet.x, feet.z).distance(at) > ARM_REACH {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::NothingInReach,
                    });
                }
                if let Some(i) = self.object_at(at) {
                    return self.take_back(player, i);
                }
                if let Some((plant, matter)) = self.plant_at(player, at) {
                    return self.gather(player, plant, matter);
                }
                if let Some(matter) = self.ground_sample_at(world, at) {
                    let p = self.player_mut(player)?;
                    if let Err(refusal) = p.inventory.add(matter) {
                        return Some(Event::Failed {
                            player,
                            failure: Failure::Bag(refusal),
                        });
                    }
                    self.dig(world, at);
                    return Some(Event::Picked {
                        player,
                        matter,
                        removed: None,
                    });
                }
                Some(Event::Failed {
                    player,
                    failure: Failure::NothingInReach,
                })
            }
            Command::Lay { player, slot } => {
                let Some(at) = self.lay_point(world, player) else {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::CannotLay,
                    });
                };
                self.lay_matter(world, player, slot, at)
            }
            Command::Work { player, slot } => {
                let Some(work) = self.work_plan(player, slot) else {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::NothingToWork,
                    });
                };
                let roll = self.rng.next_f32();
                let p = self.player_mut(player)?;
                let held = p.inventory.stacks()[slot].matter;
                // Effort: working with the hands costs a little food.
                p.needs.food = (p.needs.food - 0.003).max(0.0);
                let take = |p: &mut PlayerState, f: fn(Matter) -> bool| {
                    let i = p.inventory.stacks().iter().position(|s| f(s.matter))?;
                    p.inventory.take(i)
                };
                let made = match work {
                    Work::ShapeDish => {
                        let Matter::Clay { source } = held else {
                            return None;
                        };
                        for _ in 0..CLAY_PER_DISH {
                            p.inventory.take(slot);
                        }
                        let dish = Matter::RawDish { source };
                        let _ = p.inventory.add(dish);
                        dish
                    }
                    Work::Knap => {
                        let Matter::Pebble { dark } = held else {
                            return None;
                        };
                        p.inventory.take(slot);
                        let chance = if dark {
                            FLAKE_CHANCE_DARK
                        } else {
                            FLAKE_CHANCE_LIGHT
                        };
                        let made = if roll < chance {
                            Matter::Flake
                        } else {
                            Matter::Chips
                        };
                        let _ = p.inventory.add(made);
                        made
                    }
                    Work::PullSticks => {
                        p.inventory.take(slot);
                        for _ in 0..STICKS_PER_BUNDLE {
                            let _ = p.inventory.add(Matter::Stick);
                        }
                        Matter::Stick
                    }
                    Work::Haft => {
                        p.inventory.take(slot);
                        take(p, |m| m == Matter::Stick)?;
                        take(p, |m| m.properties().flexibility >= 0.8)?;
                        let knife = Matter::Knife {
                            uses: crate::items::KNIFE_USES,
                        };
                        let _ = p.inventory.add(knife);
                        knife
                    }
                };
                Some(Event::Worked { player, work, made })
            }
            Command::Drink { player } => {
                if !self.can_drink(world, player) {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::NoWater,
                    });
                }
                self.player_mut(player)?.needs.drink();
                Some(Event::Drank { player })
            }
        }
    }

    /// Lays one of slot `slot` at `at` (already checked), clearing the low plants around.
    fn lay_matter(
        &mut self,
        world: &World,
        player: PlayerId,
        slot: usize,
        at: Vec3,
    ) -> Option<Event> {
        let air = KELVIN + needs::temperature(world, at, 12.0, self.rain, self.now.year);
        let matter = self.player_mut(player)?.inventory.take(slot)?;
        self.objects.lay(matter, at, air);
        // Laying something clears the low plants around it (as one clears a spot
        // for a fire).
        let (cx, cz) = (at.x.floor() as i64, at.z.floor() as i64);
        for z in cz - 1..=cz + 1 {
            for x in cx - 1..=cx + 1 {
                let Some(list) = self.pickables.get(&(x, z)) else {
                    continue;
                };
                for &plant in list {
                    let p = &self.plants[plant];
                    let (px, pz) = position(p);
                    let close = Vec2::new(px - at.x, pz - at.z).length() < 0.45;
                    if close
                        && !self.removed[plant]
                        && matches!(harvest(p), Some(Harvest::Whole(_)))
                    {
                        self.removed[plant] = true;
                        self.ecology.remove(plant, &self.plants);
                        self.ecology.remove(plant, &self.plants);
                        self.events.push(Event::Cleared { plant });
                    }
                }
            }
        }
        Some(Event::Laid { player, matter })
    }

    /// Digs a handful at `at`: the world takes a micro-voxel out there (event `Dig`).
    fn dig(&mut self, world: &World, at: Vec2) {
        self.events.push(Event::Dig { at });
        // What grew there is dug up with the soil.
        self.clear_plants(at, 0.4);
        // Digging into an anomaly's place wounds it a little.
        for kind in Kind::ALL {
            if self.anomalies.get(kind).is_some_and(|a| a.holds(world, at)) {
                self.anomalies.disturb(kind, kind.harm(Harm::Dig));
            }
        }
    }

    /// Living plants within `radius` of `at` are gone (dug up with the soil).
    fn clear_plants(&mut self, at: Vec2, radius: f32) {
        for plant in self.ecology.near(at, radius, &self.plants) {
            if !self.removed[plant] {
                self.removed[plant] = true;
                self.obstacles.remove(plant);
                let stood = self.ecology.remove(plant, &self.plants);
                self.events
                    .push(Event::Plant(Change::Died { plant, stood }));
            }
        }
    }

    /// Plants on fire now: where, and how fiercely (0 to 1).
    pub fn burning_plants(&self) -> Vec<(glam::Vec3, f32)> {
        self.ecology
            .burning(&self.plants)
            .map(|(i, strength)| {
                let p = &self.plants[i];
                let at = crate::ecology::place(p);
                (
                    glam::Vec3::new(at.x, p.base[1] as f32 + 0.2, at.y),
                    strength,
                )
            })
            .collect()
    }

    /// Plants and the life of each (size 1 for what does not grow).
    pub fn plants(&self) -> &[world::PlantInstance] {
        &self.plants
    }

    pub fn plant_size(&self, plant: usize) -> f32 {
        self.ecology.size(plant)
    }

    /// A cut wears the blade: a knife loses a use, and when its binding gives way, the flake
    /// and the stick come apart; a bare flake may break.
    fn wear_blade(&mut self, player: PlayerId) {
        let roll = self.rng.next_f32();
        let Some(p) = self.player_mut(player) else {
            return;
        };
        let stacks = p.inventory.stacks();
        if let Some(i) = stacks
            .iter()
            .position(|s| matches!(s.matter, Matter::Knife { .. }))
        {
            let Some(Matter::Knife { uses }) = p.inventory.take(i) else {
                return;
            };
            if uses > 1 {
                let _ = p.inventory.add(Matter::Knife { uses: uses - 1 });
            } else {
                let _ = p.inventory.add(Matter::Flake);
                let _ = p.inventory.add(Matter::Stick);
            }
        } else if let Some(i) = stacks.iter().position(|s| s.matter == Matter::Flake) {
            // Held in bare fingers, a flake often snaps.
            if roll < 0.4 {
                p.inventory.take(i);
                let _ = p.inventory.add(Matter::Chips);
            }
        }
    }

    /// Takes laid object `i` back into player's bag, unless too hot.
    fn take_back(&mut self, player: PlayerId, i: usize) -> Option<Event> {
        if !self.objects.handleable(i) {
            return Some(Event::Failed {
                player,
                failure: Failure::TooHot,
            });
        }
        let matter = self.objects.placed()[i].matter;
        let p = self.player_mut(player)?;
        if let Err(refusal) = p.inventory.add(matter) {
            return Some(Event::Failed {
                player,
                failure: Failure::Bag(refusal),
            });
        }
        self.objects.take(i);
        Some(Event::TookBack { player, matter })
    }

    /// Gathers `matter` from plant `plant` into player's bag.
    fn gather(&mut self, player: PlayerId, plant: usize, matter: Matter) -> Option<Event> {
        let p = self.player_mut(player)?;
        if let Err(refusal) = p.inventory.add(matter) {
            return Some(Event::Failed {
                player,
                failure: Failure::Bag(refusal),
            });
        }
        let removed = match harvest(&self.plants[plant]) {
            Some(Harvest::Whole(_)) => {
                self.removed[plant] = true;
                self.ecology.remove(plant, &self.plants);
                self.obstacles.remove(plant);
                Some(plant)
            }
            Some(Harvest::NeedsBlade(_)) => {
                self.removed[plant] = true;
                self.ecology.remove(plant, &self.plants);
                self.obstacles.remove(plant);
                self.wear_blade(player);
                Some(plant)
            }
            _ => {
                *self.pebbles_taken.entry(plant).or_default() += 1;
                None
            }
        };
        Some(Event::Picked {
            player,
            matter,
            removed,
        })
    }

    /// One fixed step of time: bodies move as steered, needs follow the conditions.
    pub fn step(&mut self, world: &World, now: &Conditions) {
        self.now = *now;
        let (hour, rain) = (now.hour, now.rain);
        self.rain = rain;
        // Objects' physics, in the air around the first player (the objects lie near them).
        let around = self.players.first().map_or(Vec3::ZERO, |p| p.body.position);
        let air = KELVIN + needs::temperature(world, around, hour, rain, now.year);
        // Blowing (the drill key held over something glowing): fresh air for this step.
        for p in &self.players {
            if p.controls.rub {
                let facing = p.body.facing();
                let hands = Vec2::new(p.body.position.x, p.body.position.z)
                    + Vec2::new(facing.sin(), facing.cos()) * (LAY_DISTANCE * 0.6);
                if self.objects.glowing_near(hands.x, hands.y) {
                    self.objects.blow(hands.x, hands.y);
                }
            }
        }
        self.changes.clear();
        self.objects.step(STEP, air, &mut self.changes);
        // A fire lit by the player reaches the plants it touches; fire runs through them.
        self.plant_changes.clear();
        let burning: Vec<Vec2> = (0..self.objects.placed().len())
            .filter(|&i| self.objects.body(i).burning)
            .map(|i| {
                let b = self.objects.placed()[i].base;
                Vec2::new(b.x, b.z)
            })
            .collect();
        for at in burning {
            // A chance per step, not a certainty: about one try a second.
            if self.rng.next_f32() < STEP {
                self.ecology
                    .ignite(at, 0.45, &self.plants, &mut self.plant_changes);
            }
        }
        self.ecology
            .update_fire(STEP, rain, now.wind, &self.plants, &mut self.plant_changes);
        // Plants live: growth, seeds, death.
        let objects = &self.objects;
        let free = |p: Vec2| {
            objects
                .placed()
                .iter()
                .all(|o| Vec2::new(o.base.x, o.base.z).distance(p) > 0.3)
        };
        self.ecology.set_year(now.year);
        // The deer graze the plants they stand among; what they eat feeds them.
        if let Some(herd) = self.herd.as_mut() {
            self.ecology.set_grazers(&herd.grazers());
        }
        self.ecology.update(
            STEP * self.time_scale,
            world,
            &mut self.plants,
            free,
            &mut self.plant_changes,
        );
        if let Some(herd) = self.herd.as_mut() {
            herd.feed(&self.ecology.take_eaten());
            choose_pasture(world, &self.ecology, herd);
        }
        for &change in &self.plant_changes {
            match change {
                Change::Sprouted(i) => {
                    self.removed.push(false);
                    let p = &self.plants[i];
                    if harvest(p).is_some() {
                        let (x, z) = position(p);
                        self.pickables
                            .entry((x.floor() as i64, z.floor() as i64))
                            .or_default()
                            .push(i);
                    }
                }
                Change::Died { plant, .. } => {
                    self.removed[plant] = true;
                    self.obstacles.remove(plant);
                }
                Change::Resized(_) | Change::Stood(_) | Change::Ignited(_) => {}
            }
            self.events.push(Event::Plant(change));
        }
        for &(_, from, to) in &self.changes {
            self.events.push(Event::Changed { from, to });
        }
        let mut embers = Vec::new();
        for p in &mut self.players {
            p.body.pace = p.needs.pace();
            p.body
                .update(&p.controls, p.camera_yaw, STEP, world, &self.obstacles);
            p.controls.jump = false;
            let moving = p.body.velocity().length() > 0.5;
            // A fire close by warms the air.
            let feet = p.body.position;
            let fire = self.objects.warmth(feet.x, feet.z, 3.0);
            let exposure = Exposure {
                temperature: needs::temperature(world, feet, hour, rain, now.year) + fire,
                running: p.controls.run && moving,
                in_water: p.body.in_water(),
            };
            p.needs.update(STEP, &exposure);
            // The fire drill: held still, rubbing turns effort into heat, then an ember. Not
            // while blowing on something already glowing.
            let facing = p.body.facing();
            let hands = Vec2::new(feet.x, feet.z)
                + Vec2::new(facing.sin(), facing.cos()) * (LAY_DISTANCE * 0.6);
            let blowing = self.objects.glowing_near(hands.x, hands.y);
            if p.controls.rub && !moving && !blowing {
                p.rubbing += STEP;
                p.needs.food = (p.needs.food - STEP * 0.002).max(0.0);
                if p.rubbing >= RUB_SECONDS {
                    p.rubbing = 0.0;
                    embers.push(p.id);
                }
            } else {
                p.rubbing = 0.0;
            }
        }
        for player in embers {
            let event = match self.rub_target(player) {
                _ if rain > 0.5 => Event::Failed {
                    player,
                    failure: Failure::TooWet,
                },
                Some(i) => {
                    self.objects.drop_ember(i);
                    Event::Ember { player }
                }
                None => Event::Failed {
                    player,
                    failure: Failure::NothingInReach,
                },
            };
            self.events.push(event);
        }
        // What burns, and the players as the animals perceive them, for all that follows.
        self.fires.clear();
        for (i, placed) in self.objects.placed().iter().enumerate() {
            if self.objects.body(i).burning {
                self.fires.push(Vec2::new(placed.base.x, placed.base.z));
            }
        }
        self.fires.extend(
            self.ecology
                .burning(&self.plants)
                .map(|(i, _)| crate::ecology::place(&self.plants[i])),
        );
        self.observers.clear();
        for p in &self.players {
            self.observers
                .push(observer(world, &self.ecology, &self.plants, p));
        }
        // The anomalies tell their bearers what tonight holds, before they move.
        if let (Some(herd), Some(rite)) = (self.herd.as_mut(), self.anomalies.get(Kind::DeerRite)) {
            herd.set_order(rite.order(now, world.config.seed));
        }
        self.step_deer(world, now);
        self.step_squirrels(world, now);
        self.step_anomalies(world, now);
        self.ground_in -= STEP * self.time_scale;
        if self.ground_in <= 0.0 {
            self.ground_in = GROUND_EVERY;
            self.step_ground(world, now);
        }
    }

    /// The ground shows the state of the soil: a patch stripped of its plants shows bare
    /// earth, which grows over again as the plants come back; the circle of the rite stays
    /// trodden while the rite is kept, and grows over when it is not.
    fn step_ground(&mut self, world: &World, now: &Conditions) {
        let dims = world.dims();
        let seed = world.config.seed;
        let mut ring_columns = std::collections::HashMap::new();
        if let Some(herd) = &self.herd {
            // Unkept for two moons, the circle grows over in a third, cell after cell.
            let unkept = ((now.days - herd.last_rite) as f32 - 2.0 * crate::clock::LUNAR_DAYS)
                / crate::clock::LUNAR_DAYS;
            let trodden = 0.8 * (1.0 - unkept.clamp(0.0, 1.0));
            for (x, z, wear) in deer::ring_cells(herd.ring, seed) {
                ring_columns.insert((x, z), wear < trodden);
            }
        }
        let soil = self.ecology.soil();
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                let surface = world.surface(x, z);
                if !matches!(
                    surface,
                    Material::Grass | Material::ForestFloor | Material::DryGrass | Material::Dirt
                ) {
                    continue;
                }
                let natural = world.biome(x, z).surface();
                let target = if let Some(&worn) = ring_columns.get(&(x, z)) {
                    if worn {
                        Material::Dirt
                    } else {
                        Material::Grass
                    }
                } else {
                    let Some(k) = soil.patch(Vec2::new(x as f32 + 0.5, z as f32 + 0.5)) else {
                        continue;
                    };
                    let bare = world::noise::hash_unit(seed ^ 0xba7e, &[x as i64, z as i64])
                        < soil.bareness(k);
                    if bare && surface == natural {
                        self.bared.insert((x, z));
                        Material::Dirt
                    } else if !bare && self.bared.remove(&(x, z)) {
                        natural
                    } else {
                        continue;
                    }
                };
                if target != surface {
                    self.events.push(Event::Ground {
                        x,
                        z,
                        material: target,
                    });
                }
            }
        }
    }

    /// The squirrels hoard, dig up and lose their nuts; what they lose goes to the soil.
    fn step_squirrels(&mut self, world: &World, now: &Conditions) {
        let Some(squirrels) = self.squirrels.as_mut() else {
            return;
        };
        let players: Vec<Vec3> = self.players.iter().map(|p| p.body.position).collect();
        let mut sow = Vec::new();
        self.squirrel_events.clear();
        for _ in 0..self.time_scale.round().max(1.0) as usize {
            squirrels.update(
                STEP,
                world,
                now,
                &players,
                &mut self.squirrel_events,
                &mut sow,
            );
        }
        for (plant, at) in sow {
            self.ecology.bank_seed(plant, at);
        }
        // The notebook: a squirrel seen burying a nut, or digging one up in the lean months.
        for p in &mut self.players {
            let Some(book) = p.notebook.as_mut() else {
                continue;
            };
            let feet = Vec2::new(p.body.position.x, p.body.position.z);
            for &event in &self.squirrel_events {
                let (entry, at) = match event {
                    crate::squirrels::SquirrelEvent::Buried { at } => (Entry::Cache, at),
                    crate::squirrels::SquirrelEvent::DugUp { at } => (Entry::Recovery, at),
                };
                if at.distance(feet) < 15.0
                    && let Some(page) = book.write(entry, now, feet)
                {
                    self.events.push(Event::Wrote {
                        player: p.id,
                        page,
                        entry,
                    });
                }
            }
        }
    }

    /// The deer live, sensing the players; the notebooks write what their owners witnessed.
    fn step_deer(&mut self, world: &World, now: &Conditions) {
        let Some(herd) = self.herd.as_mut() else {
            return;
        };
        self.herd_events.clear();
        // The deer live in game time: when the day runs faster, so do they (a step each).
        for _ in 0..self.time_scale.round().max(1.0) as usize {
            herd.update(
                STEP,
                world,
                now,
                &self.observers,
                &self.fires,
                &mut self.herd_events,
            );
        }

        // What the herd makes heard.
        for &event in &self.herd_events {
            let call = match event {
                HerdEvent::Alarm { at, .. } => Some((crate::sound::AnimalCall::Bark, at)),
                HerdEvent::Stamp { at } => Some((crate::sound::AnimalCall::Stamp, at)),
                HerdEvent::Bell { at } => Some((crate::sound::AnimalCall::Bell, at)),
                _ => None,
            };
            if let Some((call, at)) = call {
                self.events.push(Event::Call { call, at });
            }
        }
        // Shed antlers lie in the wood until someone finds them.
        for &event in &self.herd_events {
            if let HerdEvent::AntlerShed { at } = event {
                let base = Vec3::new(at.x, world.surface_height(at.x, at.y), at.y);
                let air = KELVIN + needs::temperature(world, base, now.hour, now.rain, now.year);
                self.objects.lay(Matter::Antler, base, air);
            }
        }

        let herd = &*herd;
        let light = deer::daylight(now.hour).max(crate::clock::moon_light(now.moon) * 0.8);
        for p in &mut self.players {
            let feet = Vec2::new(p.body.position.x, p.body.position.z);
            let mut entries = Vec::new();
            // The herd in sight (close enough, light enough to make them out).
            let nearest = herd
                .deer
                .iter()
                .map(|d| Vec2::new(d.position.x, d.position.z).distance(feet))
                .fold(f32::INFINITY, f32::min);
            if nearest < 20.0 && light > 0.3 {
                entries.push(Entry::Herd);
            }
            for &event in &self.herd_events {
                let entry = match event {
                    HerdEvent::Alarm { .. } => Entry::Alarm,
                    HerdEvent::Fled { cause } => Entry::Fled(cause),
                    HerdEvent::Born => Entry::Calf,
                    HerdEvent::Starved => Entry::Starved,
                    // The bell carries far.
                    HerdEvent::Bell { .. } if nearest < 70.0 => {
                        entries.push(Entry::Bell);
                        continue;
                    }
                    HerdEvent::Bell { .. }
                    | HerdEvent::AntlerShed { .. }
                    | HerdEvent::Stamp { .. } => continue,
                };
                if nearest < 35.0 {
                    entries.push(entry);
                }
            }
            // The circle, as seen from here (across the edges of the world if nearer).
            let to_ring =
                Vec2::from(world.nearest(feet.to_array(), herd.ring.to_array())).distance(feet);
            if to_ring < deer::RING_RADIUS + 1.5 && herd.glow < 0.05 && light > 0.4 {
                entries.push(Entry::Ring);
            }
            if herd.foretelling() && to_ring < 30.0 {
                entries.push(Entry::Turning);
            }
            // The rite, watched without being noticed (understanding it is its anomaly's trial,
            // see `step_anomalies`).
            if herd.glow > 0.3 && to_ring < WATCH_RANGE && !herd.frightened(now) {
                entries.push(Entry::Rite);
            }
            // The notebook, if they have it, writes what they saw.
            let Some(book) = p.notebook.as_mut() else {
                continue;
            };
            for entry in entries {
                if let Some(page) = book.write(entry, now, feet) {
                    self.events.push(Event::Wrote {
                        player: p.id,
                        page,
                        entry,
                    });
                }
            }
        }
    }

    /// The anomalies live (health, strength, their nights); the players in their trials
    /// understand them, or wait for them in vain; spells follow the life of their anomalies.
    fn step_anomalies(&mut self, world: &World, now: &Conditions) {
        let seconds = STEP * self.time_scale;
        let around = Around {
            world,
            now,
            soil: self.ecology.soil(),
            herd: self.herd.as_ref(),
            fires: &self.fires,
        };
        self.life_changes.clear();
        self.anomalies
            .step(&around, seconds, &mut self.life_changes);
        for (p, who) in self.players.iter_mut().zip(&self.observers) {
            p.attending = None;
            let feet = Vec2::new(p.body.position.x, p.body.position.z);
            for a in self.anomalies.iter() {
                if a.life != Life::Alive {
                    // Its night, at its hours, and nothing comes.
                    if a.kind.due(now)
                        && a.kind.in_window(now.hour)
                        && a.within(world, feet, WATCH_RANGE)
                    {
                        p.write(Entry::Unkept(a.kind), now, &mut self.events);
                    }
                    continue;
                }
                let Some(spell) = a.kind.spell() else {
                    continue;
                };
                if p.spells.contains(&spell) || !a.kind.attends(a, &around, who) {
                    continue;
                }
                p.attending = Some(a.kind);
                let seconds = {
                    let trial = p.trial_mut(a.kind);
                    *trial += seconds;
                    *trial
                };
                if a.kind.understood(&around, seconds) {
                    p.learn(spell, now, &mut self.events);
                }
            }
        }
        for i in 0..self.life_changes.len() {
            let (kind, life) = self.life_changes[i];
            self.spells_follow(kind, life);
        }
    }

    /// The anomaly of `kind` now has `life`: the spell it taught follows. Dead, it no longer
    /// answers (a shape taken is left, the notebook writes the loss); born again, it answers
    /// again (the words lost meanwhile do not come back).
    fn spells_follow(&mut self, kind: Kind, life: Life) {
        let Some(spell) = kind.spell() else {
            return;
        };
        let now = self.now;
        for p in &mut self.players {
            if !p.spells.contains(&spell) {
                continue;
            }
            let player = p.id;
            if life == Life::Alive {
                self.events.push(Event::SpellRegained { player, spell });
            } else {
                p.release(spell);
                self.events.push(Event::SpellLost { player, spell });
                p.write(Entry::Lost(spell), &now, &mut self.events);
            }
        }
    }
}

/// Where the herd grazes: its meadow (round its circle), unless it is grazed out compared
/// with the best grassland within 40 cells; back to it once it has grown again.
fn choose_pasture(world: &World, ecology: &Ecology, herd: &mut Herd) {
    let soil = ecology.soil();
    let Some(home) = soil.patch(herd.ring) else {
        return;
    };
    let grassland = |k: usize| {
        let c = soil.centre(k);
        let (x, z) = (c.x as usize, c.y as usize);
        let dims = world.dims();
        x < dims.nx
            && z < dims.nz
            && world.water_level(x, z).is_none()
            && matches!(
                world.biome(x, z),
                world::Biome::Plains | world::Biome::Savanna
            )
    };
    let best = (0..soil.patches())
        .filter(|&k| soil.centre(k).distance(herd.ring) < 40.0 && grassland(k))
        .max_by(|&a, &b| soil.forage(a).total_cmp(&soil.forage(b)));
    let Some(best) = best else {
        return;
    };
    // Home while it holds well enough; elsewhere, the best there is, until home has grown
    // back (the gaps keep the herd from wavering).
    let top = soil.forage(best);
    let here = soil.forage(home);
    let current = soil.patch(herd.pasture()).map_or(0.0, |k| soil.forage(k));
    let away = herd.pasture().distance(herd.ring) > 1.0;
    if here > 0.6 * top || (!away && here >= 0.3 * top) {
        herd.set_pasture(herd.ring);
    } else if !away || current < 0.3 * top {
        herd.set_pasture(soil.centre(best));
    }
}

/// Game seconds between two looks at the ground (bare or grown over).
const GROUND_EVERY: f32 = 10.0;

/// How close to the hands a buried nut can be found by digging.
const CACHE_REACH: f32 = 0.9;

/// A player as the deer perceive them: moving or still, crouched, hidden by plants, on loud
/// or soft ground, in their own shape or a deer's.
fn observer(
    world: &World,
    ecology: &Ecology,
    plants: &[world::PlantInstance],
    p: &PlayerState,
) -> Observer {
    let feet = p.body.position;
    let at = Vec2::new(feet.x, feet.z);
    // Cover: ferns, bushes and tall dry shrubs close around.
    let cover = ecology
        .near(at, 1.3, plants)
        .into_iter()
        .filter(|&i| {
            matches!(
                plants[i].plant,
                world::Plant::Fern | world::Plant::Bush | world::Plant::DryShrub
            )
        })
        .count() as f32
        * 0.3;
    let dims = world.dims();
    let (x, z) = (
        (feet.x.max(0.0) as usize).min(dims.nx - 1),
        (feet.z.max(0.0) as usize).min(dims.nz - 1),
    );
    let below = world.block(x, (feet.y - 0.5).max(0.0) as usize, z);
    let ground_noise = if p.body.in_water() {
        1.0
    } else {
        match below {
            Material::Gravel | Material::Stone | Material::Rock => 1.0,
            Material::ForestFloor => 0.7,
            Material::DryGrass | Material::Snow => 0.55,
            Material::Sand | Material::DesertSand => 0.45,
            _ => 0.35,
        }
    };
    let velocity = p.body.velocity();
    Observer {
        at: feet,
        speed: Vec2::new(velocity.x, velocity.z).length(),
        crouched: p.body.crouched() > 0.5,
        cover: cover.min(1.0),
        ground_noise,
        disguised: p.body.deer,
    }
}

/// Where a plant stands, horizontally (cell centre plus its offset).
fn position(p: &world::PlantInstance) -> (f32, f32) {
    (
        p.base[0] as f32 + 0.5 + p.offset[0],
        p.base[2] as f32 + 0.5 + p.offset[1],
    )
}

crate::save::persist_struct!(Conditions {
    hour,
    days,
    moon,
    rain,
    wind,
    year
});

impl crate::save::Persist for PlayerState {
    fn write(&self, w: &mut crate::save::Writer) {
        w.put(&self.id);
        w.put(&self.body);
        w.put(&self.inventory);
        w.put(&self.needs);
        w.put(&self.notebook);
        w.put(&self.spells);
        w.put(&self.trials);
        w.put(&self.transformations);
    }
    fn read(r: &mut crate::save::Reader) -> crate::save::Result<Self> {
        Ok(Self {
            id: r.get()?,
            body: r.get()?,
            inventory: r.get()?,
            needs: r.get()?,
            controls: Controls::default(),
            camera_yaw: 0.0,
            rubbing: 0.0,
            notebook: r.get()?,
            spells: r.get()?,
            trials: r.get()?,
            transformations: r.get()?,
            attending: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    fn setup() -> (World, GameState, PlayerId) {
        let world = World::generate(WorldConfig::small(6));
        let mut state = GameState::new(&world);
        let id = state.join(crate::player::spawn_point(&world));
        (world, state, id)
    }

    /// Puts the player right next to the first pickable thing giving `want`.
    fn next_to(
        world: &World,
        state: &mut GameState,
        id: PlayerId,
        want: fn(Matter) -> bool,
    ) -> usize {
        // One with nothing else to pick close by, so the hands find it and nothing else.
        let plants = world.plants();
        let pickable = |p: &world::PlantInstance| harvest(p).is_some();
        let (i, p) = plants
            .iter()
            .enumerate()
            .find(|&(i, p)| {
                matches!(harvest(p), Some(Harvest::Whole(m) | Harvest::Part(m)) if want(m))
                    && plants.iter().enumerate().all(|(j, q)| {
                        j == i || !pickable(q) || {
                            let (a, b) = (position(p), position(q));
                            (a.0 - b.0).hypot(a.1 - b.1) > 0.7
                        }
                    })
            })
            .expect("no such plant in the world");
        // Facing +z (as a new body does), the hands right on it.
        let (x, z) = position(p);
        state.place(
            id,
            Vec3::new(x, p.base[1] as f32 + 0.001, z - LAY_DISTANCE * 0.6),
        );
        i
    }

    #[test]
    fn picking_a_flower_takes_it_from_the_world_into_the_bag() {
        let (world, mut state, id) = setup();
        let plant = next_to(&world, &mut state, id, |m| matches!(m, Matter::Flower(_)));
        let event = state.apply(&world, Command::Pick { player: id });
        let Some(Event::Picked {
            removed, matter, ..
        }) = event
        else {
            panic!("{event:?}");
        };
        assert!(matches!(matter, Matter::Flower(_)));
        assert_eq!(removed, Some(plant));
        assert!(state.is_removed(plant));
        assert_eq!(state.player(id).unwrap().inventory.stacks().len(), 1);
    }

    #[test]
    fn a_boulder_stays_but_gives_a_few_pebbles() {
        let (world, mut state, id) = setup();
        let stone = next_to(&world, &mut state, id, |m| {
            matches!(m, Matter::Pebble { .. })
        });
        let mut pebbles = 0;
        for _ in 0..10 {
            if let Some(Event::Picked { removed, .. }) =
                state.apply(&world, Command::Pick { player: id })
            {
                assert_eq!(removed, None);
                pebbles += 1;
            }
        }
        assert!(!state.is_removed(stone));
        // Other things nearby may have been picked too; at most 3 pebbles from this stone.
        assert!(pebbles >= PEBBLES_PER_STONE);
        assert_eq!(state.pebbles_taken[&stone], PEBBLES_PER_STONE);
    }

    #[test]
    fn eating_a_mushroom_feeds_and_a_spotted_one_makes_sick() {
        let (world, mut state, id) = setup();
        next_to(&world, &mut state, id, |m| {
            m == Matter::Mushroom { spotted: true }
        });
        state.apply(&world, Command::Pick { player: id });
        let slot = state
            .player(id)
            .unwrap()
            .inventory
            .stacks()
            .iter()
            .position(|s| s.matter == Matter::Mushroom { spotted: true })
            .unwrap();
        let before = state.player(id).unwrap().needs.food;
        let event = state.apply(&world, Command::Eat { player: id, slot });
        assert!(matches!(event, Some(Event::Ate { .. })));
        let needs = state.player(id).unwrap().needs;
        assert!(needs.food > before && needs.sick > 0.0);
    }

    #[test]
    fn one_drinks_only_near_water() {
        let (world, mut state, id) = setup();
        // Far from water at first? Find a dry spot away from any water, then a shore.
        let dims = world.dims();
        let mut dry = None;
        let mut shore = None;
        for z in 2..dims.nz - 2 {
            for x in 2..dims.nx - 2 {
                let wet = world.water_level(x, z).is_some();
                let near =
                    (0..9).any(|k| world.water_level(x + k % 3 - 1, z + k / 3 - 1).is_some());
                let feet = Vec3::new(
                    x as f32 + 0.5,
                    world.ground_top(x, z) as f32,
                    z as f32 + 0.5,
                );
                if !near && dry.is_none() {
                    dry = Some(feet);
                }
                if !wet && near && shore.is_none() {
                    shore = Some(feet);
                }
            }
        }
        state.place(id, dry.unwrap());
        assert!(matches!(
            state.apply(&world, Command::Drink { player: id }),
            Some(Event::Failed {
                failure: Failure::NoWater,
                ..
            })
        ));
        state.place(id, shore.unwrap());
        assert!(matches!(
            state.apply(&world, Command::Drink { player: id }),
            Some(Event::Drank { .. })
        ));
    }

    #[test]
    fn steering_moves_the_body_by_fixed_steps() {
        let (world, mut state, id) = setup();
        let start = state.player(id).unwrap().body.position;
        let controls = Controls {
            forward: true,
            ..Controls::default()
        };
        state.apply(
            &world,
            Command::Steer {
                player: id,
                controls,
                camera_yaw: 0.0,
            },
        );
        for _ in 0..60 {
            state.step(&world, &Conditions::noon());
        }
        assert!(state.player(id).unwrap().body.position.distance(start) > 1.0);
    }

    /// A fire from scratch, through commands, the way it is really made: a nest of dry grass,
    /// an ember from the drill, the grass flames, twigs laid on it catch.
    #[test]
    fn a_fire_from_scratch() {
        let (world, mut state, id) = setup();
        // Somewhere open and level.
        let dims = world.dims();
        let spot = (20..dims.nz - 20)
            .flat_map(|z| (20..dims.nx - 20).map(move |x| (x, z)))
            .find(|&(x, z)| {
                let top = world.ground_top(x, z);
                (0..3).all(|d| {
                    world.ground_top(x, z + d) == top && world.water_level(x, z + d).is_none()
                })
            })
            .expect("no flat ground");
        let feet = Vec3::new(
            spot.0 as f32 + 0.5,
            world.ground_top(spot.0, spot.1) as f32,
            spot.1 as f32 + 0.5,
        );
        state.place(id, feet);
        let lay = |state: &mut GameState, matter: Matter| {
            state.give(id, matter);
            let slot = state
                .player(id)
                .unwrap()
                .inventory
                .stacks()
                .iter()
                .position(|s| s.matter == matter)
                .unwrap();
            state.apply(&world, Command::Lay { player: id, slot })
        };
        // A nest of dry grass, then the drill over it.
        assert!(matches!(
            lay(&mut state, Matter::GrassFibre),
            Some(Event::Laid { .. })
        ));
        lay(&mut state, Matter::GrassFibre);
        let steer = |state: &mut GameState, rub: bool| {
            let controls = Controls {
                rub,
                ..Controls::default()
            };
            state.apply(
                &world,
                Command::Steer {
                    player: id,
                    controls,
                    camera_yaw: 0.0,
                },
            );
        };
        steer(&mut state, true);
        for _ in 0..(60.0 * (RUB_SECONDS + 0.5)) as usize {
            state.step(&world, &Conditions::noon());
        }
        assert!(
            state
                .drain_events()
                .any(|e| matches!(e, Event::Ember { .. }))
        );
        steer(&mut state, false);
        let any_burning = |state: &GameState| {
            (0..state.objects().placed().len()).any(|i| state.objects().body(i).burning)
        };
        // Blow until the grass flames, then lay the twigs on it.
        let mut seconds = 0;
        while !any_burning(&state) && seconds < 120 {
            for _ in 0..60 {
                state.step(&world, &Conditions::noon());
            }
            seconds += 1;
        }
        assert!(any_burning(&state), "the grass never caught");
        for _ in 0..3 {
            lay(&mut state, Matter::DeadTwigs);
        }
        let mut twigs_burning = false;
        for _ in 0..60 * 90 {
            state.step(&world, &Conditions::noon());
            let objects = state.objects();
            twigs_burning |= objects
                .placed()
                .iter()
                .enumerate()
                .any(|(i, p)| p.matter == Matter::DeadTwigs && objects.body(i).burning);
        }
        assert!(twigs_burning, "the twigs never caught");
    }

    /// From stones and twigs to a knife that cuts bushes and wears out.
    #[test]
    fn knapping_hafting_and_cutting() {
        let (world, mut state, id) = setup();
        let slot_of = |state: &GameState, f: fn(Matter) -> bool| {
            state
                .player(id)
                .unwrap()
                .inventory
                .stacks()
                .iter()
                .position(|s| f(s.matter))
        };
        let work = |state: &mut GameState, slot: usize| {
            state.apply(&world, Command::Work { player: id, slot })
        };
        // Knap dark stones until a flake comes (fine-grained: most strikes do).
        for _ in 0..6 {
            state.give(id, Matter::Pebble { dark: true });
        }
        let mut flakes = 0;
        for _ in 0..5 {
            let slot = slot_of(&state, |m| matches!(m, Matter::Pebble { .. })).unwrap();
            if let Some(Event::Worked {
                made: Matter::Flake,
                ..
            }) = work(&mut state, slot)
            {
                flakes += 1;
            }
        }
        assert!(
            flakes >= 2,
            "only {flakes} flakes in 5 strikes of dark stone"
        );
        // Sticks from a bundle, grass for the binding, then the knife.
        state.give(id, Matter::DeadTwigs);
        let slot = slot_of(&state, |m| m == Matter::DeadTwigs).unwrap();
        work(&mut state, slot);
        state.give(id, Matter::GrassFibre);
        let slot = slot_of(&state, |m| m == Matter::Flake).unwrap();
        assert_eq!(state.work_plan(id, slot), Some(Work::Haft));
        assert!(matches!(
            work(&mut state, slot),
            Some(Event::Worked {
                made: Matter::Knife { .. },
                ..
            })
        ));
        // A bush can now be cut; each cut wears the knife.
        let (bush, p) = world
            .plants()
            .iter()
            .enumerate()
            .find(|(_, p)| p.plant == world::Plant::Bush)
            .expect("no bush");
        let (x, z) = position(p);
        state.place(id, Vec3::new(x, p.base[1] as f32, z - 0.6));
        let event = state.apply(
            &world,
            Command::PickAt {
                player: id,
                at: Vec2::new(x, z),
            },
        );
        assert!(
            matches!(event, Some(Event::Picked { matter: Matter::GreenWood, removed: Some(b), .. }) if b == bush),
            "{event:?}"
        );
        assert!(
            slot_of(&state, |m| m
                == Matter::Knife {
                    uses: crate::items::KNIFE_USES - 1
                })
            .is_some()
        );
    }

    #[test]
    fn light_stone_mostly_crumbles() {
        let (world, mut state, id) = setup();
        let mut flakes = 0;
        for _ in 0..20 {
            state.give(id, Matter::Pebble { dark: false });
            state.give(id, Matter::Pebble { dark: false });
            let slot = state
                .player(id)
                .unwrap()
                .inventory
                .stacks()
                .iter()
                .position(|s| matches!(s.matter, Matter::Pebble { .. }))
                .unwrap();
            if let Some(Event::Worked {
                made: Matter::Flake,
                ..
            }) = state.apply(&world, Command::Work { player: id, slot })
            {
                flakes += 1;
            }
            // Empty the bag between tries.
            while state.players[0].inventory.take(0).is_some() {}
        }
        assert!(flakes <= 8, "{flakes} flakes out of 20 from coarse stone");
    }

    /// Taking a handful of ground asks the world to dig there.
    #[test]
    fn a_handful_of_ground_digs_the_world() {
        let (mut world, mut state, id) = setup();
        let dims = world.dims();
        let (x, z) = (2..dims.nz - 2)
            .flat_map(|z| (2..dims.nx - 2).map(move |x| (x, z)))
            .find(|&(x, z)| {
                world.water_level(x, z).is_none()
                    && world.block(x, world.ground_top(x, z) - 1, z) == Material::Sand
            })
            .expect("no sand");
        let top = world.ground_top(x, z);
        let at = Vec2::new(x as f32 + 0.6, z as f32 + 0.6);
        state.place(id, Vec3::new(at.x, top as f32, at.y - 1.0));
        assert!(matches!(
            state.apply(&world, Command::PickAt { player: id, at }),
            Some(Event::Picked {
                matter: Matter::Sand,
                ..
            })
        ));
        let events: Vec<Event> = state.drain_events().collect();
        assert!(events.contains(&Event::Dig { at }), "{events:?}");
        let dug = world.dig(at.x, at.y).unwrap();
        assert_eq!(dug.material, Material::Sand);
        assert!(world.surface_height(at.x, at.y) < top as f32);
    }

    /// Watching the full-moon rite from downwind, unseen, teaches the deer's shape: in real
    /// time as when the day runs fast (the herd lives in game time).
    #[test]
    fn watching_the_rite_unseen_teaches_the_deer_shape_even_fast_forwarded() {
        for fast in [false, true] {
            let mut world = World::generate(WorldConfig::small(1));
            let spawn = crate::player::spawn_point(&world);
            let (ring, cover) = deer::home(&world, Vec2::new(spawn.x, spawn.z)).expect("a meadow");
            deer::wear_ring(&mut world, ring, 1);
            let mut state = GameState::new(&world);
            state.add_herd(Herd::new(ring, cover, &world, 7));
            let mut clock = crate::clock::Clock::on_day(3, 21.0);
            let wind = crate::wind::direction(clock.days() + 0.1);
            // Downwind of the circle, 15 cells off, standing still.
            let at = ring + wind * 15.0;
            let feet = Vec3::new(at.x, world.surface_height(at.x, at.y), at.y);
            let me = state.join(feet);
            state.time_scale = if fast { 60.0 } else { 1.0 };
            clock.fast = fast;
            let steps = if fast { 6 * 60 } else { 150 * 60 };
            for _ in 0..steps {
                clock.advance(STEP);
                state.step(&world, &clock.conditions(0.0));
            }
            let p = state.player(me).expect("joined");
            assert!(
                p.spells.contains(&Spell::DeerForm),
                "fast {fast}: not learnt"
            );
        }
    }

    /// Several weeks of the meadow with its herd, sped up: herd size, the meadow's cover and
    /// forage. A measurement, not a check (slow): `cargo test --release -p game meadow_and_herd
    /// -- --ignored --nocapture`.
    #[test]
    #[ignore = "mesure lente"]
    fn meadow_and_herd_over_weeks() {
        let mut world = World::generate(WorldConfig::small(1));
        let spawn = crate::player::spawn_point(&world);
        let (ring, cover) = deer::home(&world, Vec2::new(spawn.x, spawn.z)).expect("a meadow");
        deer::wear_ring(&mut world, ring, 1);
        let mut state = GameState::new(&world);
        state.add_herd(Herd::new(ring, cover, &world, 7));
        let mut clock = crate::clock::Clock::on_day(1, 12.0);
        clock.fast = true;
        state.time_scale = 60.0;
        let patch = state.ecology.soil().patch(ring).expect("in the world");
        for day in 1..=66 {
            for _ in 0..20 * 60 {
                clock.advance(STEP);
                state.step(&world, &clock.conditions(0.0));
            }
            let herd = state.herd().expect("herd");
            let energy: f32 =
                herd.deer.iter().map(|d| d.energy).sum::<f32>() / herd.deer.len().max(1) as f32;
            let soil = state.ecology.soil();
            println!(
                "jour {day:2} ({:?}) : {} cerfs, énergie {energy:.2}, prairie : couverture {:.2}, fourrage {:.1}, eau/humus {:?}, pâture {:?}",
                crate::season::season(clock.conditions(0.0).year),
                herd.deer.len(),
                soil.cover(patch),
                soil.forage(patch),
                soil.state(patch),
                herd.pasture()
            );
            if day % 6 != 0 {
                continue;
            }
            println!(
                "   {:?}",
                herd.deer
                    .iter()
                    .map(|d| (format!("{:.2}", d.size), format!("{:.2}", d.energy)))
                    .collect::<Vec<_>>()
            );
        }
    }

    /// A game saved and read back in a world generated again goes on exactly as the one that
    /// was saved: same plants, same herd, same player, step after step.
    #[test]
    fn a_saved_game_goes_on_exactly_as_before() {
        let setup = || {
            let mut world = World::generate(WorldConfig::small(1));
            let spawn = crate::player::spawn_point(&world);
            let (ring, cover) = deer::home(&world, Vec2::new(spawn.x, spawn.z)).expect("a meadow");
            deer::wear_ring(&mut world, ring, 1);
            (world, spawn, ring, cover)
        };
        let (mut world, spawn, ring, cover) = setup();
        let mut state = GameState::new(&world);
        state.add_herd(Herd::new(ring, cover, &world, 7));
        let me = state.join(spawn);
        state.place_notebook(spawn + Vec3::new(0.0, 0.0, 0.5));
        state.give_notebook(me);
        state.teach(me, Spell::DeerForm);
        state.time_scale = 60.0;
        let mut clock = crate::clock::Clock::on_day(1, 18.0);
        clock.fast = true;
        let run = |state: &mut GameState,
                   world: &mut World,
                   clock: &mut crate::clock::Clock,
                   steps: usize| {
            for _ in 0..steps {
                clock.advance(STEP);
                state.step(world, &clock.conditions(0.0));
                for event in state.drain_events().collect::<Vec<_>>() {
                    if let Event::Dig { at } = event {
                        world.dig(at.x, at.y);
                    }
                }
            }
        };
        run(&mut state, &mut world, &mut clock, 600);
        state.apply(&world, Command::Pick { player: me });
        // The rite disturbed: its strength comes back, the same in both games.
        state.anomalies.disturb(Kind::DeerRite, 0.4);
        run(&mut state, &mut world, &mut clock, 60);

        let mut w = crate::save::header(1);
        w.put(&world.state());
        state.save(&mut w);
        let bytes = w.bytes;
        if std::env::var_os("DISSIPATIF_SAVE_SIZE").is_some() {
            let start = std::time::Instant::now();
            let mut w = crate::save::header(1);
            w.put(&world.state());
            state.save(&mut w);
            println!(
                "sauvegarde : {} Ko en {:?}",
                w.bytes.len() / 1024,
                start.elapsed()
            );
        }
        let (mut again, ..) = setup();
        let mut r = crate::save::open(&bytes, 1).expect("our save");
        again.restore(r.get().expect("world")).expect("same size");
        let mut loaded = GameState::load(&mut r, &again).expect("state");
        loaded.time_scale = 60.0;
        let mut clock_again = crate::clock::Clock::at_days(clock.days());
        clock_again.fast = true;

        run(&mut state, &mut world, &mut clock, 300);
        run(&mut loaded, &mut again, &mut clock_again, 300);
        assert_eq!(state.plants().len(), loaded.plants().len());
        for i in 0..state.plants().len() {
            assert_eq!(state.plant_size(i), loaded.plant_size(i), "plant {i}");
        }
        let (a, b) = (state.herd().expect("herd"), loaded.herd().expect("herd"));
        assert_eq!(a.deer.len(), b.deer.len());
        for (x, y) in a.deer.iter().zip(&b.deer) {
            assert_eq!(x.position, y.position);
            assert_eq!(x.energy, y.energy);
        }
        assert_eq!(state.body(me).position, loaded.body(me).position);
        let pages = |s: &GameState| {
            s.player(me)
                .and_then(|p| p.notebook.as_ref())
                .map(|b| b.pages.len())
        };
        assert_eq!(pages(&state), pages(&loaded));
        assert!(world.state().blocks == again.state().blocks);
        // The rite, measured and weakened as before; the player's shapes and trials.
        let (x, y) = (rite(&state), rite(&loaded));
        assert!(x.strength < 1.0, "the rite never weakened");
        assert_eq!(
            (x.life, x.health, x.strength, x.healthy_for, x.since),
            (y.life, y.health, y.strength, y.healthy_for, y.since)
        );
        assert_eq!((x.encore, x.last_shown), (y.encore, y.last_shown));
        let shapes = |s: &GameState| {
            s.player(me)
                .map(|p| (p.spells.clone(), p.transformations, p.trials.clone()))
        };
        assert_eq!(shapes(&state), shapes(&loaded));
    }

    /// The circle of the rite grows over when the rite is no longer kept, and is trodden
    /// bare again when it is.
    #[test]
    fn an_unkept_circle_grows_over() {
        let mut world = World::generate(WorldConfig::small(1));
        let spawn = crate::player::spawn_point(&world);
        let (ring, cover) = deer::home(&world, Vec2::new(spawn.x, spawn.z)).expect("a meadow");
        deer::wear_ring(&mut world, ring, 1);
        let mut state = GameState::new(&world);
        state.add_herd(Herd::new(ring, cover, &world, 7));
        let mut now = Conditions::noon();
        let apply = |state: &mut GameState, world: &mut World, now: &Conditions| {
            state.step_ground(world, now);
            let mut changed = 0;
            for event in state.drain_events().collect::<Vec<_>>() {
                if let Event::Ground { x, z, material } = event
                    && deer::ring_cells(ring, world.config.seed)
                        .iter()
                        .any(|c| (c.0, c.1) == (x, z))
                    && world.set_surface(x, z, material).is_some()
                {
                    changed += 1;
                }
            }
            changed
        };
        let dirt = |world: &World| {
            deer::ring_cells(ring, world.config.seed)
                .iter()
                .filter(|c| world.surface(c.0, c.1) == Material::Dirt)
                .count()
        };
        let worn = dirt(&world);
        assert!(worn > 10, "{worn}");
        // Three moons without the rite: grown over.
        now.days = 3.5 * crate::clock::LUNAR_DAYS as f64;
        apply(&mut state, &mut world, &now);
        assert_eq!(dirt(&world), 0);
        // The rite kept again: trodden bare.
        state.herd.as_mut().expect("herd").last_rite = now.days;
        apply(&mut state, &mut world, &now);
        assert_eq!(dirt(&world), worn);
    }

    /// The world of the deer tests (seed 1), its herd and its rite, alive.
    fn rite_world() -> (World, GameState, Vec2) {
        let mut world = World::generate(WorldConfig::small(1));
        let spawn = crate::player::spawn_point(&world);
        let (ring, cover) = deer::home(&world, Vec2::new(spawn.x, spawn.z)).expect("a meadow");
        deer::wear_ring(&mut world, ring, 1);
        let mut state = GameState::new(&world);
        state.add_herd(Herd::new(ring, cover, &world, 7));
        (world, state, ring)
    }

    /// Steps the game until `until` (days since the start), dry, applying to the world what the
    /// game asks of it (as `App::on_event` does); the events, in order.
    fn run_until(
        state: &mut GameState,
        world: &mut World,
        clock: &mut crate::clock::Clock,
        until: f64,
    ) -> Vec<Event> {
        let mut seen = Vec::new();
        while clock.days() < until {
            clock.advance(STEP);
            state.step(world, &clock.conditions(0.0));
            let events: Vec<Event> = state.drain_events().collect();
            for &event in &events {
                match event {
                    Event::Dig { at } => {
                        world.dig(at.x, at.y);
                    }
                    Event::Ground { x, z, material } => {
                        world.set_surface(x, z, material);
                    }
                    Event::Plant(Change::Stood(i)) => world.stamp_plant(&state.plants[i]),
                    Event::Plant(Change::Died { plant, stood: true }) => {
                        world.unstamp_plant(&state.plants[plant]);
                    }
                    _ => {}
                }
            }
            seen.extend(events);
        }
        seen
    }

    /// A dry cell `distance` cells from the circle, along +x, and the feet there.
    fn away_from(world: &World, ring: Vec2, distance: f32) -> Vec3 {
        let at = Vec2::from(world.nearest(ring.to_array(), (ring + Vec2::X * distance).to_array()));
        let (x, z) = world.wrap(at.x, at.y);
        Vec3::new(x, world.surface_height(x, z), z)
    }

    /// Sets the meadow round the circle on fire, as a grass fire would take it: each living
    /// plant of the rite's place gets a few chances to catch (dry grass catches, damp things
    /// less).
    fn burn_meadow(state: &mut GameState, ring: Vec2) {
        for _ in 0..6 {
            state.ecology.ignite(
                ring,
                crate::anomaly::RITE_MEADOW,
                &state.plants,
                &mut state.plant_changes,
            );
        }
    }

    fn rite(state: &GameState) -> &Anomaly {
        state.anomalies().get(Kind::DeerRite).expect("the rite")
    }

    /// Frightened on the full moon's night, the herd keeps away: the rite is missed, and kept
    /// the next night instead, by fewer hinds; once only.
    #[test]
    fn a_rite_missed_comes_back_the_next_night_weaker() {
        let (mut world, mut state, ring) = rite_world();
        // The naturalist stands in the open right by the circle as the herd comes to graze.
        let me = state.join(away_from(&world, ring, 2.0));
        let mut clock = crate::clock::Clock::on_day(3, 20.0);
        clock.fast = true;
        state.time_scale = 60.0;
        let events = run_until(&mut state, &mut world, &mut clock, 2.0 + 23.0 / 24.0);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Call {
                    call: crate::sound::AnimalCall::Bark,
                    ..
                }
            )),
            "the herd never took fright"
        );
        let a = rite(&state);
        assert_eq!(a.encore, Some(4), "no encore after a missed night");
        // A fright costs 0.35; the hours since gave back a little (0.5 a day in full health).
        assert!(
            a.strength < 1.0 - 0.35 + 0.1,
            "a fright in its vigil cost it nothing: {}",
            a.strength
        );
        assert_eq!(a.last_shown, None, "it rose to its height all the same");
        // Gone far away; the next night, in the middle of the incantation.
        state.place(me, away_from(&world, ring, 60.0));
        run_until(&mut state, &mut world, &mut clock, 3.0 + 22.3 / 24.0);
        let herd = state.herd().expect("herd");
        let order = herd.order();
        assert!(order.tonight, "the night after, no rite");
        assert!(
            order.strength <= crate::anomaly::ENCORE_STRENGTH + 1e-4,
            "the encore at full strength: {}",
            order.strength
        );
        let circling = herd
            .deer
            .iter()
            .filter(|d| d.activity == deer::Activity::Circling)
            .count();
        let n = herd.deer.len();
        assert_eq!(
            circling,
            (order.strength * n as f32).ceil() as usize,
            "{circling} of {n} on the circle at strength {}",
            order.strength
        );
        assert!(circling < n, "all of them came");
        assert!(herd.glow > 0.6, "the encore never rose: glow {}", herd.glow);
        // Kept: no third night.
        run_until(&mut state, &mut world, &mut clock, 3.0 + 23.5 / 24.0);
        let a = rite(&state);
        assert_eq!(a.encore, None);
        assert!(a.last_shown.is_some_and(|d| d > 3.0));
    }

    /// Dead, the rite takes back the shape it taught (the naturalist turned back into
    /// themself, the spell refused); born again, it gives it back, but not the words: the
    /// count of shapes taken stays.
    #[test]
    fn a_rite_born_again_gives_its_spell_back_but_not_the_words() {
        let (mut world, mut state, ring) = rite_world();
        let feet = away_from(&world, ring, 40.0);
        let me = state.join(feet);
        state.place_notebook(feet);
        state.give_notebook(me);
        state.teach(me, Spell::DeerForm);
        let mut clock = crate::clock::Clock::on_day(1, 12.0);
        clock.fast = true;
        state.time_scale = 60.0;
        state.apply(
            &world,
            Command::Cast {
                player: me,
                spell: Spell::DeerForm,
            },
        );
        run_until(&mut state, &mut world, &mut clock, 0.6);
        assert!(state.body(me).deer);
        assert_eq!(
            state.spell_state(me, Spell::DeerForm),
            crate::anomaly::SpellState::Usable
        );

        state.force_life(Kind::DeerRite, Life::Dead);
        let events: Vec<Event> = state.drain_events().collect();
        assert!(
            events.contains(&Event::SpellLost {
                player: me,
                spell: Spell::DeerForm
            }),
            "{events:?}"
        );
        assert!(!state.body(me).deer, "still a deer, its rite dead");
        assert_eq!(
            state.spell_state(me, Spell::DeerForm),
            crate::anomaly::SpellState::Lost
        );
        assert_eq!(
            state.apply(
                &world,
                Command::Cast {
                    player: me,
                    spell: Spell::DeerForm
                }
            ),
            Some(Event::Failed {
                player: me,
                failure: Failure::SpellLost
            })
        );
        let book = |state: &GameState| {
            state
                .player(me)
                .and_then(|p| p.notebook.as_ref())
                .map(|b| b.pages.iter().map(|p| p.entry).collect::<Vec<_>>())
                .expect("notebook")
        };
        assert!(book(&state).contains(&Entry::Lost(Spell::DeerForm)));

        // The meadow and the herd are well: born again after its ripening days.
        let events = run_until(&mut state, &mut world, &mut clock, 3.2);
        assert_eq!(rite(&state).life, Life::Alive);
        assert!(
            events.contains(&Event::SpellRegained {
                player: me,
                spell: Spell::DeerForm
            }),
            "no spell given back"
        );
        assert_eq!(
            state.spell_state(me, Spell::DeerForm),
            crate::anomaly::SpellState::Usable
        );
        let p = state.player(me).expect("player");
        assert_eq!(p.transformations, 1, "a shape given back counted again");
        assert_eq!(p.spells, vec![Spell::DeerForm]);
        let pages = book(&state);
        assert!(
            pages.contains(&Entry::Lost(Spell::DeerForm)),
            "the loss unwritten"
        );
        assert!(
            !pages.contains(&Entry::Learnt(Spell::DeerForm)),
            "learnt again"
        );
        assert!(matches!(
            state.apply(
                &world,
                Command::Cast {
                    player: me,
                    spell: Spell::DeerForm
                }
            ),
            Some(Event::Cast { on: true, .. })
        ));
    }

    /// The rite dead, the full moon comes and nobody walks the circle; the notebook of one
    /// who waited there says so.
    #[test]
    fn a_dead_rite_leaves_the_full_moon_night_empty() {
        let (mut world, mut state, ring) = rite_world();
        let wind = crate::wind::direction(2.0 + 21.5 / 24.0);
        let at = ring + wind * 15.0;
        let feet = Vec3::new(at.x, world.surface_height(at.x, at.y), at.y);
        let me = state.join(feet);
        state.place_notebook(feet);
        state.give_notebook(me);
        let mut clock = crate::clock::Clock::on_day(3, 21.0);
        clock.fast = true;
        state.time_scale = 60.0;
        run_until(&mut state, &mut world, &mut clock, 2.0 + 21.1 / 24.0);
        state.force_life(Kind::DeerRite, Life::Dead);
        let mut highest: f32 = 0.0;
        while clock.days() < 2.0 + 22.6 / 24.0 {
            let next = clock.days() + 0.002;
            run_until(&mut state, &mut world, &mut clock, next);
            highest = highest.max(state.herd().expect("herd").glow);
        }
        assert_eq!(highest, 0.0, "the rite glowed, dead");
        let p = state.player(me).expect("player");
        assert!(p.spells.is_empty());
        let book = p.notebook.as_ref().expect("notebook");
        assert!(
            book.has(Entry::Unkept(Kind::DeerRite)),
            "nothing written of the empty night"
        );
        assert!(!book.has(Entry::Rite));
    }

    /// Fire in the meadow weakens the rite by a quarter a game minute, digging there by a
    /// little a handful; elsewhere, neither.
    #[test]
    fn fire_and_digging_in_its_meadow_weaken_the_rite() {
        let (world, mut state, ring) = rite_world();
        let now = crate::clock::Clock::on_day(1, 12.0).conditions(0.0);
        let mut changes = Vec::new();
        let mut burn = |state: &mut GameState, fire: Vec2| {
            let fires = [fire];
            let around = Around {
                world: &world,
                now: &now,
                soil: state.ecology.soil(),
                herd: state.herd.as_ref(),
                fires: &fires,
            };
            // A game minute.
            for _ in 0..60 * 60 {
                state.anomalies.step(&around, STEP, &mut changes);
            }
        };
        burn(&mut state, ring + Vec2::new(5.0, 3.0));
        let strength = rite(&state).strength;
        assert!(
            (0.74..0.8).contains(&strength),
            "a minute of fire in the meadow: strength {strength}"
        );
        let far =
            Vec2::from(world.nearest(ring.to_array(), (ring + Vec2::new(40.0, 0.0)).to_array()));
        burn(&mut state, far);
        assert!(
            rite(&state).strength > strength,
            "a fire far off wounded it"
        );
        let before = rite(&state).strength;
        for _ in 0..10 {
            state.dig(&world, ring + Vec2::new(6.0, -2.0));
        }
        let dug = before - rite(&state).strength;
        assert!((dug - 0.3).abs() < 1e-4, "ten handfuls dug cost {dug}");
        state.dig(&world, far);
        assert!(
            (before - rite(&state).strength - dug).abs() < 1e-6,
            "digging far off wounded it"
        );
    }

    /// A burnt meadow kills its rite within days (not at once: its health is the meadow's of
    /// the last days): the spell it taught no longer answers, and one who wore the deer's
    /// shape is themself again.
    #[test]
    fn burning_the_meadow_kills_the_rite_takes_the_spell_and_the_shape() {
        let (mut world, mut state, ring) = rite_world();
        let feet = away_from(&world, ring, 70.0);
        let me = state.join(feet);
        state.place_notebook(feet);
        state.give_notebook(me);
        state.teach(me, Spell::DeerForm);
        state.apply(
            &world,
            Command::Cast {
                player: me,
                spell: Spell::DeerForm,
            },
        );
        let mut clock = crate::clock::Clock::on_day(1, 12.0);
        clock.fast = true;
        state.time_scale = 60.0;
        run_until(&mut state, &mut world, &mut clock, 0.6);
        assert!(
            rite(&state).health > crate::anomaly::BIRTH,
            "an ailing meadow at the start"
        );
        burn_meadow(&mut state, ring);
        let events = run_until(&mut state, &mut world, &mut clock, 1.2);
        assert_eq!(
            rite(&state).life,
            Life::Alive,
            "dead the very day of the fire"
        );
        assert!(!events.iter().any(|e| matches!(e, Event::SpellLost { .. })));
        let events = run_until(&mut state, &mut world, &mut clock, 3.6);
        assert_eq!(
            rite(&state).life,
            Life::Dead,
            "alive three days after: health {}",
            rite(&state).health
        );
        assert!(
            events.contains(&Event::SpellLost {
                player: me,
                spell: Spell::DeerForm
            }),
            "the spell was not taken back"
        );
        assert!(!state.body(me).deer, "still in the shape of a deer");
        assert_eq!(
            state.spell_state(me, Spell::DeerForm),
            crate::anomaly::SpellState::Lost
        );
        assert_eq!(
            state.apply(
                &world,
                Command::Cast {
                    player: me,
                    spell: Spell::DeerForm
                }
            ),
            Some(Event::Failed {
                player: me,
                failure: Failure::SpellLost
            })
        );
        let p = state.player(me).expect("player");
        assert!(
            p.notebook
                .as_ref()
                .is_some_and(|b| b.has(Entry::Lost(Spell::DeerForm)))
        );
        assert_eq!(
            p.transformations, 1,
            "a shape lost took back a transformation"
        );
        // Dead, the rite asks nothing of the herd.
        assert_eq!(
            state.herd().expect("herd").order(),
            crate::anomaly::Order::NONE
        );
    }

    /// The rite's life over weeks: twelve days as the world goes (the herd grazing its meadow),
    /// then the meadow burnt, and what follows. A measurement, not a check (slow): `cargo test
    /// --release -p game rite_through -- --ignored --nocapture`.
    #[test]
    #[ignore = "mesure lente"]
    fn the_rite_through_a_burnt_meadow_over_weeks() {
        let (mut world, mut state, ring) = rite_world();
        let me = state.join(away_from(&world, ring, 60.0));
        state.teach(me, Spell::DeerForm);
        let mut clock = crate::clock::Clock::on_day(1, 12.0);
        clock.fast = true;
        state.time_scale = 60.0;
        let tell = |state: &GameState, world: &World, clock: &crate::clock::Clock| {
            let now = clock.conditions(0.0);
            let around = Around {
                world,
                now: &now,
                soil: state.ecology.soil(),
                herd: state.herd.as_ref(),
                fires: &[],
            };
            let raw = Kind::DeerRite.measure(ring, crate::anomaly::RITE_MEADOW, &around);
            let a = rite(state);
            let herd = state.herd().expect("herd");
            let energy =
                herd.deer.iter().map(|d| d.energy).sum::<f32>() / herd.deer.len().max(1) as f32;
            let plants = |radius: f32| state.ecology.near(ring, radius, &state.plants).len();
            println!(
                "jour {:5.2} : {:?}, santé {:.2} (mesure {:.2}, saine depuis {:.1} j), force {:.2}, \
                 {} cerfs (réserves {:.2}), plantes à 16 / 40 cases : {} / {}, sort {:?}",
                clock.days() + 1.0,
                a.life,
                a.health,
                raw,
                a.healthy_for,
                a.strength,
                herd.deer.len(),
                energy,
                plants(crate::anomaly::RITE_MEADOW),
                plants(40.0),
                state.spell_state(me, Spell::DeerForm)
            );
        };
        let mut day: f64 = 0.5;
        while day < 64.0 {
            if (day - 12.5).abs() < 1e-6 {
                println!("-- la prairie brûle --");
                burn_meadow(&mut state, ring);
            }
            day += if (12.5..16.5).contains(&day) {
                0.25
            } else {
                1.0
            };
            let events = run_until(&mut state, &mut world, &mut clock, day);
            for e in events {
                if matches!(e, Event::SpellLost { .. } | Event::SpellRegained { .. }) {
                    println!("   {e:?}");
                }
            }
            tell(&state, &world, &clock);
        }
    }
}
