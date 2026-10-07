//! Red deer (Cervus elaphus): a herd of hinds led by an old hind, living as real deer do, and
//! the anomaly they carry: on full-moon nights they walk in a circle in their meadow. The rite
//! is not theirs to decide: whether it is kept tonight, its omens, and how strongly, are the
//! anomaly's (see `anomaly.rs`), which lives only while the meadow and the herd are well. The
//! herd reads its `Order` each step and carries it out.
//!
//! Real behaviour, outside the anomaly, after field studies:
//! - Crepuscular: they graze at dawn and dusk and through part of the night, and lie up in
//!   cover to ruminate during the day.
//! - The herd follows its lead hind in single file between cover and pasture.
//! - Grazing, each deer lifts its head now and then to look around (vigilance); the larger the
//!   herd, the less often each one looks up (more eyes share the watch).
//! - Senses: sight catches movement far better than still shapes; keen hearing; smell carried
//!   downwind, from far.
//! - Alarm: a deer that senses something it cannot make out stops, stares, stamps and barks;
//!   the herd then flees at a gallop, stops further off and looks back. A frightened herd keeps
//!   away for hours.
//!
//! Distances are compressed: real deer wind a person from hundreds of metres and flee at 50 to
//! 200 m; here a cell is a metre and the world 256 cells across.
//!
//! Deterministic: randomness from a seeded generator, time from the `Conditions`.

use glam::{Vec2, Vec3};
use sim::rng::SplitMix64;
use world::{Biome, Material, World};

use crate::anomaly::Order;
use crate::state::Conditions;

/// Hinds and calves in the herd.
const HERD: usize = 6;
/// Speeds, in cells per second: stepping while grazing, walking, trotting, galloping.
const GRAZE_SPEED: f32 = 0.35;
const WALK_SPEED: f32 = 1.3;
const TROT_SPEED: f32 = 3.2;
const GALLOP_SPEED: f32 = 9.0;
/// Fastest turn, radians per second.
const TURN_RATE: f32 = 3.0;
/// A deer sees movement this far, hears footsteps this far (the loudest), smells this far
/// downwind.
const SIGHT_RANGE: f32 = 32.0;
const HEARING_RANGE: f32 = 22.0;
const SCENT_RANGE: f32 = 45.0;
/// Cosine of the half-angle of the scent plume carried by the wind (about 30°).
const SCENT_CONE: f32 = 0.86;
/// Suspicion fades by this much a second when nothing feeds it.
const CALMING: f32 = 0.08;
/// Suspicion thresholds: looking up and staring, stamping and barking, fleeing.
const VIGILANT: f32 = 0.3;
const ALARMED: f32 = 0.65;
const FLEE: f32 = 1.0;
/// After a fright, the herd keeps away from its usual places this long (days: 3.6 hours).
const WARY_DAYS: f64 = 0.15;
/// Radius of the circle they walk, in cells.
pub const RING_RADIUS: f32 = 3.5;
/// The rite, the night of the full moon: the herd gathers on its circle from 21 h 40; the
/// incantation itself begins at 22 h and lasts 0.6 game hours (half a minute of play).
const GATHER_HOUR: f32 = 21.66;
pub const RITE_HOUR: f32 = 22.0;
pub const RITE_HOURS: f32 = 0.6;
/// One turn of the circle, in seconds (the procession, and the lone hind's foretelling).
const RING_TURN: f32 = 20.0;
/// Seconds for the light of the rite to gather once all stand in their places.
const GATHERING: f32 = 4.0;
/// The incantation's movements, as shares of it: gathered and still, bowing together (a stamp
/// running round the circle), the procession, heads raised to the moon (its height), the end.
const BOWING: f32 = 0.12;
const PROCESSION: f32 = 0.45;
pub const TO_THE_MOON: f32 = 0.8;
const ENDING: f32 = 0.97;
/// From dusk, when the herd comes to its meadow, the night of the rite is watched over: a fright
/// then wounds the anomaly (see `vigil_hours`).
const VIGIL_HOUR: f32 = 20.0;
/// The light of the rite never rises above this plus the anomaly's strength: a weak rite
/// glows dimly.
const GLOW_FLOOR: f32 = 0.4;

/// What a deer is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Lying,
    Grazing,
    Walking,
    /// Head up, staring at something it noticed.
    Vigilant,
    /// Stamping and barking.
    Alarmed,
    Fleeing,
    /// Walking the circle under the full moon.
    Circling,
}

/// What gave the naturalist away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Sight,
    Sound,
    Scent,
    Fire,
}

/// What happened in the herd, for the notebook.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HerdEvent {
    /// A deer stamped and barked: it sensed the naturalist without making them out.
    Alarm { cause: Cause, at: Vec2 },
    /// A hoof struck the ground (in alarm, or in the rite).
    Stamp { at: Vec2 },
    /// The herd fled.
    Fled { cause: Cause },
    /// A calf was born (the herd is well fed).
    Born,
    /// A deer starved.
    Starved,
    /// The stag bells (the rut, in autumn).
    Bell { at: Vec2 },
    /// A stag shed an antler there (the end of winter).
    AntlerShed { at: Vec2 },
}

pub struct Deer {
    /// Feet, on the ground.
    pub position: Vec3,
    /// Facing (radians): forward is (sin, cos) on (x, z), as for the naturalist.
    pub heading: f32,
    pub activity: Activity,
    /// Head carriage: −1 down grazing, 0 level, 1 up and alert, more raised to the sky.
    pub head: f32,
    /// 0 standing, 1 lying (eased).
    pub lying: f32,
    /// Walking cycle (radians) and ground speed (cells per second).
    pub stride: f32,
    pub speed: f32,
    /// 1 for a grown hind, less for a calf.
    pub size: f32,
    /// Seconds left of a foot stamp (drawn).
    pub stamp: f32,
    suspicion: f32,
    /// What it stares at, and why.
    noticed: Option<(Vec2, Cause)>,
    goal: Vec2,
    timer: f32,
    /// Seconds left with the head up, scanning, while grazing.
    scanning: f32,
    /// Reserves, 0 (starving) to 1 (well fed): grazing fills them, living spends them.
    pub energy: f32,
    /// With calf since the rut.
    pub pregnant: bool,
}

/// The year of a red deer (phases, see `season.rs`): the rut at the heart of autumn, calves
/// born at the end of spring (eight months later), antlers shed at the end of winter.
const RUT: std::ops::Range<f32> = 0.54..0.68;
const CALVING: std::ops::Range<f32> = 0.17..0.27;
const SHEDDING: f32 = 0.93;
/// Chance per game day for a pregnant hind to give birth in the calving season (most do), and
/// for a well-fed hind to conceive in the rut.
const BIRTH_RATE: f32 = 0.4;
const CONCEPTION_RATE: f32 = 0.5;
/// Seconds between two bellings of the stag.
/// Reserves a deer spends in a game day (a grown hind; a calf less), and what a unit of
/// forage eaten gives back: about five grown grass tufts a day keep a hind.
/// A full reserve (fat) lasts about eight days of fasting, fourteen in winter: a deer lives
/// through the lean months on what it put on in autumn.
const METABOLISM: f32 = 0.12;
const ENERGY_PER_BITE: f32 = 0.024;
const BELL_EVERY: f32 = 25.0;
/// Conception: hinds above `FED`, fewer as the herd nears `CROWDED` (fecundity falls with
/// density, as in real deer), the herd at most `MAX_HERD`. Calves grow to full size in a year.
const CROWDED: f32 = 9.0;
const FED: f32 = 0.6;
const MAX_HERD: usize = 12;
const CALF: f32 = 0.5;
const GROWTH_PER_DAY: f32 = 0.45 / crate::season::YEAR_DAYS;
/// Winter: the reserves are spent more slowly (red deer lower their metabolism by about
/// 40 % in the cold months).
const WINTER_METABOLISM: f32 = 0.6;

impl Deer {
    fn new(position: Vec3, size: f32, rng: &mut SplitMix64) -> Self {
        Self {
            position,
            heading: rng.next_f32() * std::f32::consts::TAU,
            activity: Activity::Lying,
            head: 0.0,
            lying: 1.0,
            stride: 0.0,
            speed: 0.0,
            size,
            stamp: 0.0,
            suspicion: 0.0,
            noticed: None,
            goal: Vec2::new(position.x, position.z),
            timer: rng.next_f32() * 3.0,
            scanning: 0.0,
            energy: 0.7,
            pregnant: false,
        }
    }
}

/// The naturalist as deer perceive them.
#[derive(Clone, Copy, Debug)]
pub struct Observer {
    pub at: Vec3,
    /// Horizontal speed, cells per second.
    pub speed: f32,
    pub crouched: bool,
    /// How hidden by plants around, 0 (open) to 1 (deep in ferns or bushes).
    pub cover: f32,
    /// How loud the ground underfoot is, 0 (soft) to 1 (gravel, splashing).
    pub ground_noise: f32,
    /// In the shape of a deer (the spell): the herd sees and hears one of its own.
    pub disguised: bool,
}

/// What the herd is about, from the hour and the moon.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Lie(Vec2),
    Graze(Vec2),
    Travel(Vec2),
    /// The lead hind walks part of the circle alone; the others graze nearby.
    Foretelling,
    Ritual,
}

pub struct Herd {
    pub deer: Vec<Deer>,
    /// Centre of the circle, in their meadow; their cover at the wood's edge.
    pub ring: Vec2,
    cover: Vec2,
    rng: SplitMix64,
    /// Fleeing from there, for this many more seconds.
    flight: Option<(Vec2, f32, Cause)>,
    /// Keeping away until then (days), where they ended up.
    wary_until: f64,
    refuge: Vec2,
    /// Angle of the circle as they walk it.
    turn: f32,
    /// How far the ritual has gathered, 0 to 1.
    pub glow: f32,
    /// How far into the incantation, while it lasts (0 to 1).
    pub rite: Option<f32>,
    /// Where they graze: their meadow, or elsewhere when it is grazed out.
    pasture: Vec2,
    /// What the anomaly asks of them now (set each step before they move: not saved).
    order: Order,
    /// Which deer were grazing when the grazers were last asked for (see `grazers`).
    grazing: Vec<usize>,
    /// The stag that joins the hinds for the rut, while it lasts.
    pub stag: Option<Deer>,
    /// Seconds until it bells again.
    bell_in: f32,
    /// The last year (count since the start) antlers were shed.
    shed_year: i64,
    /// When the rite was last kept (days): the circle stays trodden while it is.
    pub last_rite: f64,
}

/// Lit share of the moon at a phase (0 new, 0.5 full).
fn moonlight(phase: f32) -> f32 {
    crate::clock::moon_light(phase)
}

/// Daylight, 0 at night to 1 by day: as in the sky (the sun sets at 20 h).
pub fn daylight(hour: f32) -> f32 {
    1.0 - render::sky::sky(hour).night
}

/// The full moon's night: the rite is due (whether it is kept is the anomaly's to say).
pub fn rite_night(now: &Conditions) -> bool {
    moonlight(now.moon) >= 0.93
}

/// The rite's hours, from the gathering to the end of the incantation.
pub fn rite_hours(hour: f32) -> bool {
    (GATHER_HOUR..RITE_HOUR + RITE_HOURS).contains(&hour)
}

/// How far into the incantation (0 to 1), in its hours.
fn incantation(hour: f32) -> Option<f32> {
    let u = (hour - RITE_HOUR) / RITE_HOURS;
    (0.0..1.0).contains(&u).then_some(u)
}

/// The nights before and after the full moon: a gibbous moon, when the omens may show.
pub fn omen_night(now: &Conditions) -> bool {
    (0.55..0.93).contains(&moonlight(now.moon))
}

/// The omens' hours: around midnight.
fn omen_hours(hour: f32) -> bool {
    !(1.0..22.0).contains(&hour)
}

/// The hours a night of the rite (or of its omens) is watched over: from dusk to the end of the
/// omens.
pub fn vigil_hours(hour: f32) -> bool {
    hour >= VIGIL_HOUR || omen_hours(hour)
}

/// Head carriage of the deer during the incantation at `u` (share), `seconds` into it.
fn incantation_head(u: f32, seconds: f32) -> f32 {
    if u < BOWING {
        0.4
    } else if u < PROCESSION {
        // Bowing together, slowly, every three seconds.
        let bow = 0.5 - 0.5 * (std::f32::consts::TAU * seconds / 3.0).cos();
        0.4 - 1.3 * bow
    } else if u < TO_THE_MOON {
        0.8
    } else if u < ENDING {
        1.7
    } else {
        0.8
    }
}

impl Herd {
    pub fn new(ring: Vec2, cover: Vec2, world: &World, seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let deer = (0..HERD)
            .map(|i| {
                let angle = i as f32 * 2.4;
                let spot = cover + Vec2::new(angle.cos(), angle.sin()) * 2.2;
                // Two calves of the year among the hinds.
                let size = if i >= HERD - 2 {
                    0.68
                } else {
                    0.95 + 0.1 * rng.next_f32()
                };
                Deer::new(ground(world, spot), size, &mut rng)
            })
            .collect();
        Self {
            deer,
            ring,
            cover,
            rng,
            flight: None,
            wary_until: -1.0,
            refuge: cover,
            turn: 0.0,
            glow: 0.0,
            rite: None,
            pasture: ring,
            order: Order::NONE,
            grazing: Vec::new(),
            stag: None,
            bell_in: BELL_EVERY,
            shed_year: -1,
            last_rite: 0.0,
        }
    }

    /// What the anomaly asks of the herd now: the rite tonight or not, its omens, how strongly.
    pub fn set_order(&mut self, order: Order) {
        self.order = order;
    }

    pub fn order(&self) -> Order {
        self.order
    }

    /// Whether a deer is stamping, barking or fleeing now: the herd has taken fright.
    pub fn startled(&self) -> bool {
        self.flight.is_some()
            || self
                .deer
                .iter()
                .any(|d| matches!(d.activity, Activity::Alarmed | Activity::Fleeing))
    }

    /// Where the herd is, roughly: the lead hind.
    pub fn centre(&self) -> Vec2 {
        let Some(lead) = self.deer.first() else {
            return self.ring;
        };
        let p = lead.position;
        Vec2::new(p.x, p.z)
    }

    /// Whether the herd is afraid (fleeing, or keeping away after a fright).
    pub fn frightened(&self, now: &Conditions) -> bool {
        self.flight.is_some() || now.days < self.wary_until
    }

    /// Whether the lead hind is walking the circle alone (the foretelling).
    pub fn foretelling(&self) -> bool {
        self.deer
            .first()
            .is_some_and(|d| d.activity == Activity::Circling && self.glow < 0.05)
    }

    fn mode(&self, now: &Conditions) -> Mode {
        if now.days < self.wary_until {
            return Mode::Graze(self.refuge);
        }
        // The anomaly's night, at its hours; its omens on the nights around.
        if self.order.tonight && rite_hours(now.hour) {
            return Mode::Ritual;
        }
        if self.order.omens && omen_hours(now.hour) {
            return Mode::Foretelling;
        }
        let pasture = self.pasture;
        match now.hour {
            h if (9.5..16.5).contains(&h) => Mode::Lie(self.cover),
            h if (16.5..18.5).contains(&h) => Mode::Travel(pasture),
            h if (18.5..23.0).contains(&h) => Mode::Graze(pasture),
            h if (3.0..7.5).contains(&h) => Mode::Graze(pasture),
            h if (7.5..9.5).contains(&h) => Mode::Travel(self.cover),
            // Late night: lying in the open, between grazing bouts.
            _ => Mode::Lie(pasture),
        }
    }

    /// Where they graze, outside the rite (their meadow, or a better one when it is grazed
    /// out).
    pub fn set_pasture(&mut self, at: Vec2) {
        self.pasture = at;
    }

    pub fn pasture(&self) -> Vec2 {
        self.pasture
    }

    /// Where the deer grazing now are; `feed` gives each what it ate, in this order.
    pub fn grazers(&mut self) -> Vec<Vec2> {
        self.grazing = (0..self.deer.len())
            .filter(|&i| self.deer[i].activity == Activity::Grazing)
            .collect();
        self.grazing
            .iter()
            .map(|&i| Vec2::new(self.deer[i].position.x, self.deer[i].position.z))
            .collect()
    }

    /// What the grazers (as last asked for) ate: their reserves fill.
    pub fn feed(&mut self, eaten: &[f32]) {
        for (&i, &e) in self.grazing.iter().zip(eaten) {
            if let Some(d) = self.deer.get_mut(i) {
                d.energy = (d.energy + e * ENERGY_PER_BITE / d.size.max(0.5)).min(1.0);
            }
        }
    }

    /// Reserves spent, calves growing, conceptions in the rut, births at the end of spring,
    /// deaths, over `dt` seconds.
    fn live(&mut self, dt: f32, world: &World, now: &Conditions, events: &mut Vec<HerdEvent>) {
        let days = dt / crate::clock::DAY_SECONDS;
        let year = now.year;
        let mut births = Vec::new();
        let room = self.deer.len() < MAX_HERD;
        let fecundity = (1.0 - self.deer.len() as f32 / CROWDED).max(0.0);
        let rut = RUT.contains(&year) && self.stag.is_some();
        let calving = CALVING.contains(&year);
        let metabolism = if crate::season::season(year) == crate::season::Season::Winter {
            METABOLISM * WINTER_METABOLISM
        } else {
            METABOLISM
        };
        for d in &mut self.deer {
            d.energy -= metabolism * d.size * days;
            if d.size < 0.95 {
                d.size += GROWTH_PER_DAY * days;
            }
            if rut
                && !d.pregnant
                && d.size > 0.9
                && d.energy > FED
                && self.rng.next_f32() < CONCEPTION_RATE * fecundity * days
            {
                d.pregnant = true;
            }
            if calving && d.pregnant && room && self.rng.next_f32() < BIRTH_RATE * days {
                d.pregnant = false;
                births.push(d.position);
            }
            // A calf lost when the season has passed.
            if d.pregnant && (0.3..0.5).contains(&year) {
                d.pregnant = false;
            }
        }
        // Antlers shed at the end of winter, once a year, in the wood where stags winter.
        let year_count = (now.days / crate::season::YEAR_DAYS as f64).floor() as i64;
        if year >= SHEDDING && self.shed_year < year_count {
            self.shed_year = year_count;
            let angle = self.rng.next_f32() * std::f32::consts::TAU;
            let at = self.cover
                + Vec2::new(angle.cos(), angle.sin()) * (3.0 + 5.0 * self.rng.next_f32());
            events.push(HerdEvent::AntlerShed { at });
        }
        let before = self.deer.len();
        self.deer.retain(|d| d.energy > 0.0);
        for _ in self.deer.len()..before {
            events.push(HerdEvent::Starved);
        }
        for mother in births {
            if self.deer.len() >= MAX_HERD {
                break;
            }
            let at = Vec2::new(mother.x + 0.6, mother.z);
            let calf = Deer::new(ground(world, at), CALF, &mut self.rng);
            self.deer.push(Deer {
                lying: 0.0,
                energy: 0.6,
                ..calf
            });
            events.push(HerdEvent::Born);
        }
    }

    /// The rut: a stag comes from the wood, keeps near the hinds and bells; it leaves when
    /// the season is over.
    fn rut(&mut self, dt: f32, world: &World, now: &Conditions, events: &mut Vec<HerdEvent>) {
        let rut = RUT.contains(&now.year) && !self.deer.is_empty();
        match (&mut self.stag, rut) {
            (None, true) => {
                let mut stag = Deer::new(ground(world, self.cover), 1.2, &mut self.rng);
                stag.lying = 0.0;
                stag.energy = 1.0;
                self.stag = Some(stag);
            }
            (Some(_), false) => self.stag = None,
            (Some(stag), true) => {
                // Near the herd, a little apart; head up and neck stretched as it bells.
                let centre = self.deer[0].position;
                let centre = Vec2::new(centre.x, centre.z);
                let here = Vec2::new(stag.position.x, stag.position.z);
                let goal = centre + (here - centre).normalize_or(Vec2::X) * 4.0;
                let fleeing = self.flight.map(|(from, _, _)| from);
                let (goal, speed) = match fleeing {
                    Some(from) => (
                        here + (here - from).normalize_or(Vec2::X) * 10.0,
                        GALLOP_SPEED,
                    ),
                    None if here.distance(goal) > 1.0 => (goal, WALK_SPEED),
                    None => (goal, 0.0),
                };
                move_towards(stag, world, goal, speed, dt);
                stag.activity = if speed > 0.0 {
                    Activity::Walking
                } else {
                    Activity::Vigilant
                };
                self.bell_in -= dt;
                if self.bell_in <= 0.0 {
                    self.bell_in = BELL_EVERY * (0.7 + 0.6 * self.rng.next_f32());
                    stag.stamp = 0.0;
                    events.push(HerdEvent::Bell {
                        at: Vec2::new(stag.position.x, stag.position.z),
                    });
                }
                // Belling: the head raised for three seconds.
                let belling = self.bell_in > BELL_EVERY * 0.7 - 3.0;
                let head = if belling { 1.5 } else { 0.6 };
                stag.head += (head - stag.head) * (1.0 - (-dt * 4.0).exp());
            }
            (None, false) => {}
        }
    }

    /// One step of `dt` seconds. `fires`: where something burns.
    pub fn update(
        &mut self,
        dt: f32,
        world: &World,
        now: &Conditions,
        observers: &[Observer],
        fires: &[Vec2],
        events: &mut Vec<HerdEvent>,
    ) {
        self.live(dt, world, now, events);
        self.rut(dt, world, now, events);
        if self.deer.is_empty() {
            self.glow = 0.0;
            self.rite = None;
            return;
        }
        self.sense(dt, world, now, observers, fires, events);
        let mode = self.mode(now);
        let rite = incantation(now.hour).filter(|_| mode == Mode::Ritual);
        self.rite = rite;
        // A weak anomaly gathers fewer of them on the circle; the others graze close by.
        let n = self.deer.len();
        let circling = ((self.order.strength * n as f32).ceil() as usize).min(n);
        let seconds = rite.map_or(0.0, |u| u * RITE_HOURS * crate::clock::DAY_SECONDS / 24.0);
        // The circle turns in the procession (and for the lone hind); otherwise they stand.
        let turning = match mode {
            Mode::Foretelling => true,
            Mode::Ritual => rite.is_some_and(|u| (PROCESSION..TO_THE_MOON).contains(&u)),
            _ => false,
        };
        if turning {
            self.turn =
                (self.turn + dt * std::f32::consts::TAU / RING_TURN) % std::f32::consts::TAU;
        }
        // How fast a place on the turning circle goes round (cells per second): the hinds walk
        // with it, and keep their places (and the light of the rite) through the procession.
        let pace = if turning {
            RING_RADIUS * std::f32::consts::TAU / RING_TURN
        } else {
            0.0
        };
        // Fleeing: everyone away from the source, then a stop to look back.
        if let Some((from, left, cause)) = self.flight {
            let left = left - dt;
            if left <= 0.0 {
                self.flight = None;
                self.wary_until = now.days + WARY_DAYS;
                self.refuge = self.centre();
                for d in &mut self.deer {
                    d.suspicion = VIGILANT + 0.1;
                    d.noticed = Some((from, cause));
                }
            } else {
                self.flight = Some((from, left, cause));
            }
        }
        let mut in_place = 0;
        for i in 0..n {
            let ahead = if i > 0 {
                Some(self.deer[i - 1].position)
            } else {
                None
            };
            let slot = self.ring_slot(i, circling.max(1));
            let rng = &mut self.rng;
            let d = &mut self.deer[i];
            d.timer -= dt;
            d.stamp = (d.stamp - dt).max(0.0);
            let here = Vec2::new(d.position.x, d.position.z);
            // What it does and where it goes, at what speed; whether it ends up lying.
            let (activity, goal, speed, lie) = if let Some((from, left, _)) = self.flight {
                let away = (here - from).normalize_or(Vec2::X);
                let swerve = (i as f32 - n as f32 / 2.0) * 0.12;
                let away = Vec2::from_angle(swerve).rotate(away);
                let speed = if left > 6.0 { GALLOP_SPEED } else { TROT_SPEED };
                (Activity::Fleeing, here + away * 10.0, speed, false)
            } else if d.suspicion > ALARMED {
                (Activity::Alarmed, here, 0.0, false)
            } else if d.suspicion > VIGILANT {
                (Activity::Vigilant, here, 0.0, false)
            } else {
                match mode {
                    // A hungry deer grazes on in its resting hours.
                    Mode::Lie(centre) if d.energy < 0.4 => graze(d, rng, centre, dt),
                    Mode::Lie(centre) => {
                        let spot = centre + rest_offset(i);
                        if here.distance(spot) < 0.6 {
                            (Activity::Lying, spot, 0.0, true)
                        } else {
                            (Activity::Walking, spot, WALK_SPEED, false)
                        }
                    }
                    Mode::Graze(centre) => graze(d, rng, centre, dt),
                    Mode::Travel(target) => {
                        let goal = match ahead {
                            // Single file: a little behind the one in front.
                            Some(p) => {
                                let p = Vec2::new(p.x, p.z);
                                p + (here - p).normalize_or(Vec2::X) * 1.7
                            }
                            None => target,
                        };
                        if i == 0 && here.distance(target) < 3.0 {
                            graze(d, rng, target, dt)
                        } else {
                            (Activity::Walking, goal, WALK_SPEED, false)
                        }
                    }
                    Mode::Foretelling => {
                        // Turning half the time, grazing the rest: an odd habit, not yet a rite.
                        let turning = (self.turn / std::f32::consts::TAU * 2.0).fract() < 0.6;
                        if i == 0 && turning {
                            circle(here, slot, pace)
                        } else {
                            graze(d, rng, self.ring, dt)
                        }
                    }
                    Mode::Ritual if i >= circling => graze(d, rng, self.ring, dt),
                    Mode::Ritual => {
                        if here.distance(slot) < 0.8 {
                            in_place += 1;
                        }
                        // A stamp runs round the circle while they bow, one hind after the
                        // other, two beats a second.
                        let beat = seconds * 2.0;
                        if rite.is_some_and(|u| (BOWING..PROCESSION).contains(&u))
                            && beat.fract() < dt * 2.0
                            && (beat as usize) % circling == i
                        {
                            d.stamp = 0.6;
                            events.push(HerdEvent::Stamp { at: here });
                        }
                        circle(here, slot, pace)
                    }
                }
            };
            d.activity = activity;
            d.goal = goal;
            // Lying down and standing up take a couple of seconds.
            let k = 1.0 - (-dt * 1.5).exp();
            d.lying += ((if lie { 1.0 } else { 0.0 }) - d.lying) * k;
            let speed = if d.lying > 0.3 && !lie { 0.0 } else { speed };
            move_towards(d, world, goal, speed, dt);
            // Head: down to graze, up when alert, raised to the moon in the pauses of the rite.
            let head = match activity {
                Activity::Grazing if d.scanning > 0.0 => 0.8,
                Activity::Grazing => -1.0,
                Activity::Vigilant | Activity::Alarmed => 1.0,
                Activity::Circling if mode == Mode::Ritual => {
                    rite.map_or(0.3, |u| incantation_head(u, seconds))
                }
                Activity::Fleeing => 0.6,
                _ => 0.0,
            };
            d.head += (head - d.head) * (1.0 - (-dt * 4.0).exp());
            // In its place on the circle, standing still: facing the centre.
            if activity == Activity::Circling
                && mode == Mode::Ritual
                && !turning
                && here.distance(slot) < 0.8
            {
                let to = self.ring - here;
                turn_towards(d, to.x.atan2(to.y), dt);
            }
            // Facing what it noticed.
            if matches!(activity, Activity::Vigilant | Activity::Alarmed)
                && let Some((at, _)) = d.noticed
            {
                let to = at - here;
                turn_towards(d, to.x.atan2(to.y), dt);
            }
        }
        if self.glow > 0.5 {
            self.last_rite = now.days;
        }
        // The rite gathers while (nearly) all those called walk their places (one at least),
        // up to what the anomaly's strength allows.
        let gathering =
            mode == Mode::Ritual && self.flight.is_none() && in_place + 1 >= circling.max(2);
        let target = if gathering {
            (GLOW_FLOOR + self.order.strength).min(1.0)
        } else {
            0.0
        };
        self.glow = if self.glow < target {
            (self.glow + dt / GATHERING).min(target)
        } else {
            (self.glow - dt / 5.0).max(target)
        };
    }

    /// Where deer `i` of `n` walks on the circle now.
    fn ring_slot(&self, i: usize, n: usize) -> Vec2 {
        let angle = self.turn + i as f32 * std::f32::consts::TAU / n as f32;
        self.ring + Vec2::new(angle.cos(), angle.sin()) * RING_RADIUS
    }

    /// Senses: suspicion rises with what each deer sees, hears and smells; past thresholds it
    /// stares, stamps, then the whole herd flees.
    fn sense(
        &mut self,
        dt: f32,
        world: &World,
        now: &Conditions,
        observers: &[Observer],
        fires: &[Vec2],
        events: &mut Vec<HerdEvent>,
    ) {
        let light = {
            let night = 1.0 - daylight(now.hour);
            // Deer see well at dusk and by the moon (a reflective layer behind the retina).
            1.0 - night * (0.65 - 0.35 * moonlight(now.moon))
        };
        let scent_range = SCENT_RANGE * (1.0 - 0.5 * now.rain);
        let n = self.deer.len() as f32;
        let mut alarm: Option<(Vec2, Cause)> = None;
        for d in &mut self.deer {
            let here = Vec2::new(d.position.x, d.position.z);
            // A grazing deer, head down, sees less; one on the alert, more.
            let attention = match d.activity {
                Activity::Grazing if d.scanning <= 0.0 => 0.7,
                Activity::Lying => 0.6,
                Activity::Vigilant | Activity::Alarmed => 1.5,
                _ => 1.0,
            };
            let mut rate = -CALMING;
            let mut strongest = (0.0, Cause::Sight, here);
            for o in observers {
                // Where the naturalist is, as seen from here (across the edges if nearer).
                let at = Vec2::from(world.nearest(here.to_array(), [o.at.x, o.at.z]));
                let distance = here.distance(at);
                // Sight: movement, in the light, out in the open.
                let moving = (o.speed / WALK_SPEED_PERSON).clamp(0.04, 2.0);
                let exposure = (1.0 - 0.7 * o.cover) * if o.crouched { 0.55 } else { 1.0 };
                let seen = (1.0 - distance / SIGHT_RANGE).max(0.0).powi(2);
                let near = (1.0 - distance / 4.0).max(0.0) * 0.6;
                let mut sight = (0.9 * moving * seen + near) * light * exposure * attention;
                // Hearing: footsteps, louder running and on gravel; rain covers them.
                let loudness = o.ground_noise * (o.speed / WALK_SPEED_PERSON).powf(1.5);
                let heard = (1.0 - distance / (HEARING_RANGE * loudness.sqrt().max(0.05)))
                    .max(0.0)
                    .powi(2);
                let mut sound = 0.6 * loudness.min(3.0) * heard * (1.0 - 0.6 * now.rain);
                // Smell: in the plume the wind carries from the naturalist, or very close.
                let towards = (here - at).normalize_or_zero();
                let downwind = ((towards.dot(now.wind) - SCENT_CONE) / (1.0 - SCENT_CONE)).max(0.0);
                let plume = if distance < 2.5 { 1.0 } else { downwind };
                let mut scent = 4.0 * plume * (1.0 - distance / scent_range).max(0.0);
                if o.disguised {
                    // One of their own, to the eyes and ears; the nose is fooled less.
                    sight *= 0.0;
                    sound *= 0.0;
                    scent *= 0.15;
                }
                for (value, cause) in [
                    (sight, Cause::Sight),
                    (sound, Cause::Sound),
                    (scent, Cause::Scent),
                ] {
                    rate += value;
                    if value > strongest.0 {
                        strongest = (value, cause, at);
                    }
                }
            }
            // A fire at night: they keep wary of the glow.
            for &fire in fires {
                let fire = Vec2::from(world.nearest(here.to_array(), fire.to_array()));
                let value = 0.12 * (1.0 - here.distance(fire) / 30.0).max(0.0) * (1.0 - light);
                rate += value;
                if value > strongest.0 {
                    strongest = (value, Cause::Fire, fire);
                }
            }
            let before = d.suspicion;
            d.suspicion = (d.suspicion + rate * dt).clamp(0.0, 1.2);
            if strongest.0 > 0.0 && d.suspicion > VIGILANT {
                d.noticed = Some((strongest.2, strongest.1));
            }
            if before <= ALARMED && d.suspicion > ALARMED {
                d.stamp = 0.6;
                events.push(HerdEvent::Alarm {
                    cause: d.noticed.map_or(strongest.1, |n| n.1),
                    at: here,
                });
            }
            if d.suspicion >= FLEE && alarm.is_none() {
                alarm = d.noticed.or(Some((strongest.2, strongest.1)));
            }
            d.scanning -= dt;
        }
        // An alarmed deer warns the others: they look up.
        if self.deer.iter().any(|d| d.activity == Activity::Alarmed) {
            for d in &mut self.deer {
                d.suspicion = d.suspicion.max(VIGILANT + 0.05);
            }
        }
        if let (Some((from, cause)), None) = (alarm, self.flight) {
            self.flight = Some((from, 9.0, cause));
            events.push(HerdEvent::Fled { cause });
            for d in &mut self.deer {
                d.lying = d.lying.min(0.3);
            }
        }
        // Scanning bouts: each deer looks up about every 12 s alone, less often in a herd.
        let chance = dt / (6.0 * n.sqrt());
        for d in &mut self.deer {
            if d.activity == Activity::Grazing && d.scanning <= -1.0 && self.rng.next_f32() < chance
            {
                d.scanning = 1.5 + 2.0 * self.rng.next_f32();
            }
        }
    }
}

/// The naturalist's walking speed (cells per second): movement is judged against it.
const WALK_SPEED_PERSON: f32 = 3.2;

/// Ground covered by one full stride (all four feet), in cells, at `speed`: longer strides as
/// the gait quickens.
pub fn stride_length(speed: f32, size: f32) -> f32 {
    size * if speed > 5.5 {
        3.6
    } else if speed > 2.2 {
        2.2
    } else {
        1.3
    }
}

/// Where deer `i` lies, around a resting place.
fn rest_offset(i: usize) -> Vec2 {
    let angle = i as f32 * 2.4;
    Vec2::new(angle.cos(), angle.sin()) * (1.4 + 0.5 * (i % 3) as f32)
}

/// Grazing around `centre`: small steps from tuft to tuft, head down.
fn graze(
    d: &mut Deer,
    rng: &mut SplitMix64,
    centre: Vec2,
    _dt: f32,
) -> (Activity, Vec2, f32, bool) {
    let here = Vec2::new(d.position.x, d.position.z);
    if here.distance(centre) > 9.0 {
        return (Activity::Walking, centre, WALK_SPEED, false);
    }
    if d.timer <= 0.0 || here.distance(d.goal) < 0.3 || d.goal.distance(centre) > 8.0 {
        let angle = rng.next_f32() * std::f32::consts::TAU;
        let step = 0.8 + 2.0 * rng.next_f32();
        let mut goal = here + Vec2::new(angle.cos(), angle.sin()) * step;
        if goal.distance(centre) > 7.0 {
            goal = centre + (goal - centre).normalize_or_zero() * 6.0;
        }
        d.goal = goal;
        d.timer = 4.0 + 6.0 * rng.next_f32();
    }
    let speed = if here.distance(d.goal) < 0.3 {
        0.0
    } else {
        GRAZE_SPEED
    };
    (Activity::Grazing, d.goal, speed, false)
}

/// Walking the circle: to its place on the ring, at a slow, even pace; while the circle turns,
/// at its pace (`pace`, cells per second) and a little more to keep to its place.
fn circle(here: Vec2, slot: Vec2, pace: f32) -> (Activity, Vec2, f32, bool) {
    let gap = here.distance(slot);
    // Slowing down to stop on its place (or to walk along with it).
    let speed = if gap > 3.0 {
        WALK_SPEED.max(pace + 0.5)
    } else {
        pace + (0.45 + gap * 0.4).min(gap * 1.5)
    };
    (Activity::Circling, slot, speed, false)
}

fn turn_towards(d: &mut Deer, target: f32, dt: f32) {
    let tau = std::f32::consts::TAU;
    let mut delta = (target - d.heading) % tau;
    if delta > std::f32::consts::PI {
        delta -= tau;
    } else if delta < -std::f32::consts::PI {
        delta += tau;
    }
    d.heading += delta.clamp(-TURN_RATE * dt, TURN_RATE * dt);
}

/// Steps towards `goal` at `speed`, turning first, around water and steep steps.
fn move_towards(d: &mut Deer, world: &World, goal: Vec2, speed: f32, dt: f32) {
    let here = Vec2::new(d.position.x, d.position.z);
    // The shortest way, across the edges of the world if need be.
    let goal = Vec2::from(world.nearest(here.to_array(), goal.to_array()));
    let to = goal - here;
    let k = 1.0 - (-dt * 3.0).exp();
    if to.length() < 0.1 || speed <= 0.0 {
        d.speed += (0.0 - d.speed) * k;
    } else {
        turn_towards(d, to.x.atan2(to.y), dt);
        // Turning first, then going: slower while the goal is off to the side, so it never
        // circles round it.
        let along = Vec2::new(d.heading.sin(), d.heading.cos()).dot(to.normalize());
        let speed = speed * (0.2 + 0.8 * along.max(0.0));
        d.speed += (speed - d.speed) * k;
    }
    if d.speed < 0.01 {
        return;
    }
    let step = d.speed * dt;
    // Try straight on, then veering more and more; hemmed in by trunks, squeeze through them
    // (a deer slips between trees), never into water or up a cliff.
    let tries = [0.0, 0.5, -0.5, 1.1, -1.1, 1.8, -1.8];
    for (k, veer) in tries.iter().chain(tries.iter()).enumerate() {
        let veer = *veer;
        let angle = d.heading + veer;
        let next =
            here + Vec2::new(angle.sin(), angle.cos()) * step.min(to.length().max(step * 0.2));
        let free = if k < tries.len() {
            walkable(world, d.position.y, next)
        } else {
            open_ground(world, d.position.y, next)
        };
        if free {
            d.heading = if veer == 0.0 {
                d.heading
            } else {
                d.heading + veer * 0.2
            };
            // Back into the world (it closes on itself).
            let (x, z) = world.wrap(next.x, next.y);
            d.position = Vec3::new(x, d.position.y, z);
            let height = world.surface_height(next.x, next.y);
            d.position.y += (height - d.position.y) * (1.0 - (-dt * 10.0).exp());
            d.stride += step * std::f32::consts::TAU / stride_length(d.speed, d.size);
            return;
        }
    }
    d.speed = 0.0;
}

/// Dry ground no higher than a deer steps up (a cell and a bit), trunks or not.
fn open_ground(world: &World, from_height: f32, at: Vec2) -> bool {
    let (x, z) = world.wrap(at.x, at.y);
    world.water_level(x as usize, z as usize).is_none()
        && (world.surface_height(x, z) - from_height).abs() < 1.3
}

/// Dry ground no higher than a deer steps up (a cell and a bit), within the world, and no
/// trunk standing there.
fn walkable(world: &World, from_height: f32, at: Vec2) -> bool {
    let (x, z) = world.wrap(at.x, at.y);
    let (x, z) = (x as usize, z as usize);
    if world.water_level(x, z).is_some() {
        return false;
    }
    let ground = world.surface_height(at.x, at.y);
    if (ground - from_height).abs() >= 1.3 {
        return false;
    }
    let dims = world.dims();
    let trunk = |y: usize| {
        y < dims.ny
            && matches!(
                world.block(x, y, z),
                Material::Wood
                    | Material::BarkDark
                    | Material::BirchBark
                    | Material::PalmTrunk
                    | Material::DeadWood
                    | Material::Cactus
            )
    };
    let y = ground.max(0.0) as usize;
    !(trunk(y) || trunk(y + 1))
}

/// A point on the ground.
fn ground(world: &World, at: Vec2) -> Vec3 {
    Vec3::new(at.x, world.surface_height(at.x, at.y), at.y)
}

/// Where the herd lives, in a world: the meadow where they graze and walk their circle, and the
/// wood's edge where they lie up. A flat, open meadow some way from the start (found by
/// walking, not seen at once), with a wood nearby. `None` if the world has none.
pub fn home(world: &World, start: Vec2) -> Option<(Vec2, Vec2)> {
    let dims = world.dims();
    // Trees and bushes per column: the meadow must be open.
    let mut woody = vec![0u16; dims.nx * dims.nz];
    for p in world.plants() {
        if !p.plant.is_ground_cover() {
            woody[p.base[0] + dims.nx * p.base[2]] += 1;
        }
    }
    let open = |x: usize, z: usize, r: i64| {
        (-r..=r).all(|dz| {
            (-r..=r).all(|dx| {
                let (cx, cz) = (x as i64 + dx, z as i64 + dz);
                cx < 0
                    || cz < 0
                    || cx >= dims.nx as i64
                    || cz >= dims.nz as i64
                    || woody[cx as usize + dims.nx * cz as usize] == 0
            })
        })
    };
    // The most open meadow there is: clear of trees over 8 cells around, else 6, else 4.
    // The most open, flattest meadow near enough; else less open, less flat, farther, in
    // turn (every world has some place where deer graze).
    let tiers = [
        (8, 1, 90.0),
        (6, 1, 90.0),
        (4, 1, 130.0),
        (4, 2, 160.0),
        (3, 2, 220.0),
    ];
    let ring = tiers.into_iter().find_map(|(clear, slope, far)| {
        meadow(world, start, slope, far, |x, z| open(x, z, clear))
    })?;
    // The nearest wood: forest ground within 35 cells, else a spot beside the meadow.
    let mut cover: Option<(f32, Vec2)> = None;
    for dz in -35i64..=35 {
        for dx in -35i64..=35 {
            let (x, z) = (ring.x as i64 + dx, ring.y as i64 + dz);
            if x < 4 || z < 4 || x >= dims.nx as i64 - 4 || z >= dims.nz as i64 - 4 {
                continue;
            }
            let (x, z) = (x as usize, z as usize);
            let top = world.ground_top(x, z);
            if world.block(x, top.saturating_sub(1), z) != Material::ForestFloor
                || world.water_level(x, z).is_some()
            {
                continue;
            }
            let at = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            let d = at.distance(ring);
            if d > 12.0 && cover.is_none_or(|(best, _)| d < best) {
                cover = Some((d, at));
            }
        }
    }
    let cover = cover.map_or(ring + Vec2::new(14.0, 6.0), |(_, at)| at);
    Some((ring, cover))
}

/// A flat, dry, grassy place 30 to 90 cells from `start` where `open` holds, the nearest to
/// 55 cells away.
/// `slope`: how many cells the ground may rise or fall over the circle; `far`: farthest from
/// the start.
fn meadow(
    world: &World,
    start: Vec2,
    slope: i64,
    far: f32,
    open: impl Fn(usize, usize) -> bool,
) -> Option<Vec2> {
    let dims = world.dims();
    let height = |x: usize, z: usize| world.ground_top(x, z) as i64;
    let mut best: Option<(f32, Vec2)> = None;
    for z in (8..dims.nz - 8).step_by(2) {
        for x in (8..dims.nx - 8).step_by(2) {
            let at = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            let distance = at.distance(start);
            if !(30.0..far).contains(&distance)
                || !matches!(world.biome(x, z), Biome::Plains | Biome::Savanna)
            {
                continue;
            }
            // Flat and dry over the circle and around it.
            let h = height(x, z);
            let mut flat = true;
            'around: for dz in -5i64..=5 {
                for dx in -5i64..=5 {
                    let (cx, cz) = ((x as i64 + dx) as usize, (z as i64 + dz) as usize);
                    if (height(cx, cz) - h).abs() > slope || world.water_level(cx, cz).is_some() {
                        flat = false;
                        break 'around;
                    }
                }
            }
            let soil = world.block(x, (h as usize).saturating_sub(1), z);
            if !flat || !matches!(soil, Material::Grass | Material::DryGrass) || !open(x, z) {
                continue;
            }
            let score = (distance - 55.0).abs();
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, at));
            }
        }
    }
    best.map(|(_, at)| at)
}

/// The circle worn into the meadow by years of the rite: a ring of bare, trodden earth, not
/// quite closed (grass still holds here and there). A clue, seen from afar.
pub fn wear_ring(world: &mut World, ring: Vec2, seed: u64) {
    for (x, z, wear) in ring_cells(ring, seed) {
        if wear < 0.8 {
            world.wear(x, z);
        }
    }
}

/// The columns of the circle, each with its own number in [0, 1): those below 0.8 are worn
/// when the rite is kept (grass holds in the others); they grow over in turn when it is not.
pub fn ring_cells(ring: Vec2, seed: u64) -> Vec<(usize, usize, f32)> {
    let r = RING_RADIUS.ceil() as i64 + 1;
    let mut cells = Vec::new();
    for dz in -r..=r {
        for dx in -r..=r {
            let (x, z) = (ring.x.floor() as i64 + dx, ring.y.floor() as i64 + dz);
            let at = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            if x < 0 || z < 0 || (at.distance(ring) - RING_RADIUS).abs() >= 0.6 {
                continue;
            }
            let wear = world::noise::hash_unit(seed ^ 0x51ce, &[x, z]);
            cells.push((x as usize, z as usize, wear));
        }
    }
    cells
}

crate::save::persist_enum!(Cause {
    Sight,
    Sound,
    Scent,
    Fire
});
crate::save::persist_enum!(Activity {
    Lying,
    Grazing,
    Walking,
    Vigilant,
    Alarmed,
    Fleeing,
    Circling
});
crate::save::persist_struct!(Deer {
    position,
    heading,
    activity,
    head,
    lying,
    stride,
    speed,
    size,
    stamp,
    suspicion,
    noticed,
    goal,
    timer,
    scanning,
    energy,
    pregnant,
});

impl crate::save::Persist for Herd {
    fn write(&self, w: &mut crate::save::Writer) {
        w.put(&self.deer);
        w.put(&self.ring);
        w.put(&self.cover);
        w.put(&self.rng);
        w.put(&self.flight);
        w.put(&self.wary_until);
        w.put(&self.refuge);
        w.put(&self.turn);
        w.put(&self.glow);
        w.put(&self.rite);
        w.put(&self.pasture);
        w.put(&self.shed_year);
        w.put(&self.last_rite);
    }
    fn read(r: &mut crate::save::Reader) -> crate::save::Result<Self> {
        Ok(Self {
            stag: None,
            bell_in: BELL_EVERY,
            deer: r.get()?,
            ring: r.get()?,
            cover: r.get()?,
            rng: r.get()?,
            flight: r.get()?,
            wary_until: r.get()?,
            refuge: r.get()?,
            turn: r.get()?,
            glow: r.get()?,
            rite: r.get()?,
            pasture: r.get()?,
            order: Order::NONE,
            grazing: Vec::new(),
            shed_year: r.get()?,
            last_rite: r.get()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    fn setup() -> (World, Herd) {
        let mut world = World::generate(WorldConfig::small(1));
        let start = crate::player::spawn_point(&world);
        let (ring, cover) = home(&world, Vec2::new(start.x, start.z)).expect("a meadow");
        wear_ring(&mut world, ring, 1);
        let herd = Herd::new(ring, cover, &world, 1);
        (world, herd)
    }

    fn at(hour: f32, day: f64, moon: f32, wind: Vec2) -> Conditions {
        Conditions {
            hour,
            days: day + hour as f64 / 24.0,
            moon,
            rain: 0.0,
            wind,
            year: 0.375,
        }
    }

    /// The full moon's night, from the gathering to the end of the incantation.
    fn ritual_time(now: &Conditions) -> bool {
        rite_night(now) && rite_hours(now.hour)
    }

    /// The nights before and after, around midnight.
    fn foretelling_time(now: &Conditions) -> bool {
        omen_night(now) && omen_hours(now.hour)
    }

    /// What a living rite in full strength asks of the herd under this moon: the herd alone,
    /// as the anomaly would lead it (see `anomaly.rs`).
    fn of_the_moon(now: &Conditions) -> Order {
        Order {
            tonight: rite_night(now),
            omens: omen_night(now),
            strength: 1.0,
        }
    }

    fn run(
        herd: &mut Herd,
        world: &World,
        now: Conditions,
        seconds: f32,
        me: Option<Observer>,
    ) -> Vec<HerdEvent> {
        let mut events = Vec::new();
        let observers: Vec<Observer> = me.into_iter().collect();
        herd.set_order(of_the_moon(&now));
        for _ in 0..(seconds * 30.0) as usize {
            herd.update(1.0 / 30.0, world, &now, &observers, &[], &mut events);
        }
        events
    }

    #[test]
    fn they_lie_up_by_day_and_graze_the_meadow_at_dusk() {
        let (world, mut herd) = setup();
        run(&mut herd, &world, at(12.0, 0.0, 0.25, Vec2::X), 30.0, None);
        assert!(herd.deer.iter().all(|d| d.activity == Activity::Lying));
        run(&mut herd, &world, at(20.0, 0.0, 0.25, Vec2::X), 120.0, None);
        let grazing = herd
            .deer
            .iter()
            .filter(|d| d.activity == Activity::Grazing)
            .count();
        assert!(grazing >= 4, "{grazing}");
        assert!(herd.centre().distance(herd.ring) < 10.0);
    }

    #[test]
    fn downwind_they_smell_you_upwind_they_do_not() {
        let (world, mut herd) = setup();
        let dusk = |wind| at(20.0, 0.0, 0.25, wind);
        run(&mut herd, &world, dusk(Vec2::X), 120.0, None);
        let still = |at: Vec2| Observer {
            at: Vec3::new(at.x, 0.0, at.y),
            speed: 0.0,
            crouched: true,
            cover: 0.5,
            ground_noise: 0.3,
            disguised: false,
        };
        // Standing still, 25 cells away, the wind blowing from the deer to the naturalist.
        let spot = herd.centre() + Vec2::new(25.0, 0.0);
        let events = run(&mut herd, &world, dusk(Vec2::X), 20.0, Some(still(spot)));
        assert!(events.is_empty(), "{events:?}");
        // The wind turns: from the naturalist to the deer.
        let events = run(&mut herd, &world, dusk(-Vec2::X), 20.0, Some(still(spot)));
        assert!(
            events.contains(&HerdEvent::Fled {
                cause: Cause::Scent
            }),
            "{events:?}"
        );
    }

    /// The naturalist walks straight at the herd's centre for `seconds`.
    fn approach(
        herd: &mut Herd,
        world: &World,
        now: Conditions,
        seconds: f32,
        mut me: Observer,
    ) -> Vec<HerdEvent> {
        let mut events = Vec::new();
        herd.set_order(of_the_moon(&now));
        for _ in 0..(seconds * 30.0) as usize {
            let to = herd.centre() - Vec2::new(me.at.x, me.at.z);
            if to.length() > 6.0 {
                let step = to.normalize() * me.speed / 30.0;
                me.at += Vec3::new(step.x, 0.0, step.y);
            }
            herd.update(1.0 / 30.0, world, &now, &[me], &[], &mut events);
        }
        events
    }

    #[test]
    fn walking_up_in_the_open_they_see_you_stalking_they_do_not() {
        let (world, mut herd) = setup();
        let dusk = at(19.0, 0.0, 0.25, Vec2::new(0.0, 1.0));
        run(&mut herd, &world, dusk, 150.0, None);
        let from = herd.centre() + Vec2::new(-22.0, 0.0);
        let mut walker = Observer {
            at: Vec3::new(from.x, 0.0, from.y),
            speed: 3.2,
            crouched: false,
            cover: 0.0,
            ground_noise: 0.35,
            disguised: false,
        };
        let events = approach(&mut herd, &world, dusk, 5.0, walker);
        assert!(!events.is_empty(), "walking in the open, unseen?");
        // A fresh herd: crouched, slow, in cover.
        let (world, mut herd) = setup();
        run(&mut herd, &world, dusk, 150.0, None);
        walker.at = Vec3::new(herd.centre().x - 22.0, 0.0, herd.centre().y);
        walker.speed = 0.8;
        walker.crouched = true;
        walker.cover = 0.6;
        let events = approach(&mut herd, &world, dusk, 10.0, walker);
        assert!(events.is_empty(), "{events:?}");
    }

    #[test]
    fn under_the_full_moon_they_walk_the_circle_and_a_fright_breaks_it() {
        let (world, mut herd) = setup();
        let night = at(22.0, 2.0, 0.5, Vec2::X);
        assert!(ritual_time(&night));
        run(&mut herd, &world, at(20.0, 2.0, 0.5, Vec2::X), 60.0, None);
        run(&mut herd, &world, night, 60.0, None);
        assert!(herd.deer.iter().all(|d| d.activity == Activity::Circling));
        for (i, d) in herd.deer.iter().enumerate() {
            let r = Vec2::new(d.position.x, d.position.z).distance(herd.ring);
            let slot = herd.ring_slot(i, herd.deer.len());
            assert!(
                (r - RING_RADIUS).abs() < 1.0,
                "{r} at {:?} slot {slot:?} speed {} walkable {}",
                d.position,
                d.speed,
                walkable(&world, d.position.y, slot)
            );
        }
        assert!(herd.glow > 0.9, "{}", herd.glow);
        // Downwind of them, close: they bolt, and keep away the rest of the night.
        let spot = herd.ring - Vec2::X * 15.0;
        let me = Observer {
            at: Vec3::new(spot.x, 0.0, spot.y),
            speed: 0.0,
            crouched: false,
            cover: 0.0,
            ground_noise: 0.3,
            disguised: false,
        };
        run(&mut herd, &world, night, 15.0, Some(me));
        run(&mut herd, &world, at(23.0, 2.0, 0.5, Vec2::X), 30.0, None);
        assert!(herd.glow < 0.1);
        assert!(herd.centre().distance(herd.ring) > 20.0);
    }

    /// The light of the rite holds through all its movements: bowing, the procession (the
    /// circle turning, each hind walking with its place), heads raised to the moon, the end.
    #[test]
    fn the_light_of_the_rite_holds_through_its_procession() {
        let (world, mut herd) = setup();
        run(&mut herd, &world, at(20.0, 2.0, 0.5, Vec2::X), 60.0, None);
        run(&mut herd, &world, at(21.9, 2.0, 0.5, Vec2::X), 40.0, None);
        // The incantation as it goes: its 0.6 hours are half a minute.
        let mut events = Vec::new();
        let steps = (RITE_HOURS * crate::clock::DAY_SECONDS / 24.0 * 30.0) as usize;
        let mut dimmest: f32 = 1.0;
        for k in 0..steps {
            let u = k as f32 / steps as f32;
            let now = at(RITE_HOUR + RITE_HOURS * u, 2.0, 0.5, Vec2::X);
            herd.set_order(of_the_moon(&now));
            herd.update(1.0 / 30.0, &world, &now, &[], &[], &mut events);
            if u > BOWING {
                dimmest = dimmest.min(herd.glow);
            }
        }
        assert!(
            dimmest > crate::anomaly::SUMMIT,
            "the light of the rite fell to {dimmest} in its course"
        );
    }

    /// What the anomaly asks, the herd does: a weak rite calls fewer hinds to the circle, and
    /// its light stays dim; without its order, no rite at all, full moon or not.
    #[test]
    fn a_weak_rite_gathers_fewer_hinds_and_glows_dimly() {
        let (world, mut herd) = setup();
        let night = at(22.0, 2.0, 0.5, Vec2::X);
        run(&mut herd, &world, at(20.0, 2.0, 0.5, Vec2::X), 60.0, None);
        let n = herd.deer.len();
        herd.set_order(Order {
            strength: 0.4,
            ..of_the_moon(&night)
        });
        for _ in 0..60 * 30 {
            herd.update(1.0 / 30.0, &world, &night, &[], &[], &mut Vec::new());
        }
        let circling = herd
            .deer
            .iter()
            .filter(|d| d.activity == Activity::Circling)
            .count();
        assert_eq!(circling, (0.4 * n as f32).ceil() as usize, "of {n}");
        assert!(
            (herd.glow - (GLOW_FLOOR + 0.4)).abs() < 0.02,
            "glow {} at strength 0.4",
            herd.glow
        );
        // The anomaly silent (dead): the herd grazes on under the full moon.
        herd.set_order(Order::NONE);
        for _ in 0..30 * 30 {
            herd.update(1.0 / 30.0, &world, &night, &[], &[], &mut Vec::new());
        }
        assert!(herd.deer.iter().all(|d| d.activity != Activity::Circling));
        assert!(herd.glow < 0.05 && herd.rite.is_none());
    }

    #[test]
    fn the_nights_before_the_lead_hind_turns_alone() {
        let (world, mut herd) = setup();
        let gibbous = 0.5 - 1.0 / crate::clock::LUNAR_DAYS;
        let night = at(23.0, 1.0, gibbous, Vec2::X);
        assert!(foretelling_time(&night) && !ritual_time(&night));
        run(
            &mut herd,
            &world,
            at(20.0, 1.0, gibbous, Vec2::X),
            90.0,
            None,
        );
        let mut turning = 0;
        for _ in 0..20 {
            run(&mut herd, &world, night, 3.0, None);
            turning += herd.foretelling() as usize;
        }
        assert!(turning > 5, "{turning}");
        assert!(
            herd.deer[1..]
                .iter()
                .all(|d| d.activity != Activity::Circling)
        );
    }

    /// A year of the herd: a stag comes for the rut in autumn, the hinds conceive, calves are
    /// born at the end of spring and none at other times, antlers are shed at the end of
    /// winter.
    #[test]
    fn the_herd_lives_its_year() {
        let (world, mut herd) = setup();
        let mut events = Vec::new();
        let mut births_by_season = [0usize; 4];
        let mut stag_seen_in = Vec::new();
        let day = crate::clock::DAY_SECONDS;
        // Through a year, a step of a game hour (well fed: the grazing is not simulated here).
        let steps = (crate::season::YEAR_DAYS * 24.0) as usize;
        for k in 0..steps {
            let days = k as f64 / 24.0;
            let now = Conditions {
                hour: (k % 24) as f32,
                days,
                moon: 0.25,
                rain: 0.0,
                wind: Vec2::X,
                year: crate::season::year(days),
            };
            for d in &mut herd.deer {
                d.energy = 0.9;
            }
            let before = events.len();
            herd.live(day / 24.0, &world, &now, &mut events);
            herd.rut(day / 24.0, &world, &now, &mut events);
            for e in &events[before..] {
                if *e == HerdEvent::Born {
                    births_by_season[crate::season::season(now.year) as usize] += 1;
                }
            }
            if herd.stag.is_some() {
                stag_seen_in.push(crate::season::season(now.year));
            }
        }
        assert!(
            births_by_season[0] + births_by_season[1] >= 2,
            "{births_by_season:?}"
        );
        assert_eq!(
            births_by_season[2] + births_by_season[3],
            0,
            "{births_by_season:?}"
        );
        assert!(!stag_seen_in.is_empty());
        assert!(
            stag_seen_in
                .iter()
                .all(|&s| s == crate::season::Season::Autumn)
        );
        assert!(events.iter().any(|e| matches!(e, HerdEvent::Bell { .. })));
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, HerdEvent::AntlerShed { .. }))
                .count(),
            1
        );
    }
}
