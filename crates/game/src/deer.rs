//! Red deer (Cervus elaphus): a herd of hinds led by an old hind, living as real deer do, and
//! the anomaly they keep: on full-moon nights they walk in a circle in their meadow.
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
/// running round the circle), the procession, heads raised to the moon, the end.
const BOWING: f32 = 0.12;
const PROCESSION: f32 = 0.45;
const TO_THE_MOON: f32 = 0.8;
const ENDING: f32 = 0.97;

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
    Alarm { cause: Cause },
    /// The herd fled.
    Fled { cause: Cause },
    /// A calf was born (the herd is well fed).
    Born,
    /// A deer starved.
    Starved,
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
}

/// Reserves a deer spends in a game day (a grown hind; a calf less), and what a unit of
/// forage eaten gives back: about five grown grass tufts a day keep a hind.
const METABOLISM: f32 = 1.0;
const ENERGY_PER_BITE: f32 = 0.2;
/// Births: chance per game day for each grown, well-fed hind (above `FED`), fewer as the herd
/// nears `CROWDED` (fecundity falls with density, as in real deer), at most `MAX_HERD`.
/// Calves grow to full size in about thirty days.
const BIRTHS: f32 = 0.08;
const CROWDED: f32 = 9.0;
const FED: f32 = 0.75;
const MAX_HERD: usize = 12;
const CALF: f32 = 0.55;
const GROWTH_PER_DAY: f32 = 0.015;

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
    /// Which deer were grazing when the grazers were last asked for (see `grazers`).
    grazing: Vec<usize>,
}

/// Lit share of the moon at a phase (0 new, 0.5 full).
fn moonlight(phase: f32) -> f32 {
    crate::clock::moon_light(phase)
}

/// Daylight, 0 at night to 1 by day: as in the sky (the sun sets at 20 h).
pub fn daylight(hour: f32) -> f32 {
    1.0 - render::sky::sky(hour).night
}

/// The full moon's night, from the gathering to the end of the incantation.
pub fn ritual_time(now: &Conditions) -> bool {
    moonlight(now.moon) >= 0.93 && (GATHER_HOUR..RITE_HOUR + RITE_HOURS).contains(&now.hour)
}

/// How far into the incantation (0 to 1), while it lasts.
pub fn rite_progress(now: &Conditions) -> Option<f32> {
    let u = (now.hour - RITE_HOUR) / RITE_HOURS;
    (moonlight(now.moon) >= 0.93 && (0.0..1.0).contains(&u)).then_some(u)
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

/// The nights before and after: a gibbous moon, around midnight.
pub fn foretelling_time(now: &Conditions) -> bool {
    let light = moonlight(now.moon);
    (0.55..0.93).contains(&light) && (now.hour >= 22.0 || now.hour < 1.0)
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
            grazing: Vec::new(),
        }
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
        if ritual_time(now) {
            return Mode::Ritual;
        }
        if foretelling_time(now) {
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

    /// Reserves spent, calves growing, births and deaths, over `dt` seconds.
    fn live(&mut self, dt: f32, world: &World, events: &mut Vec<HerdEvent>) {
        let days = dt / crate::clock::DAY_SECONDS;
        let mut births = Vec::new();
        let room = self.deer.len() < MAX_HERD;
        let fecundity = (1.0 - self.deer.len() as f32 / CROWDED).max(0.0);
        for d in &mut self.deer {
            d.energy -= METABOLISM * d.size * days;
            if d.size < 0.95 {
                d.size += GROWTH_PER_DAY * days;
            }
            if room
                && d.size > 0.9
                && d.energy > FED
                && self.rng.next_f32() < BIRTHS * fecundity * days
            {
                births.push(d.position);
            }
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
        self.live(dt, world, events);
        if self.deer.is_empty() {
            self.glow = 0.0;
            self.rite = None;
            return;
        }
        self.sense(dt, now, observers, fires, events);
        let mode = self.mode(now);
        let rite = rite_progress(now).filter(|_| mode == Mode::Ritual);
        self.rite = rite;
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
        let n = self.deer.len();
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
            let slot = self.ring_slot(i, n);
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
                            circle(here, slot)
                        } else {
                            graze(d, rng, self.ring, dt)
                        }
                    }
                    Mode::Ritual => {
                        if here.distance(slot) < 0.8 {
                            in_place += 1;
                        }
                        // A stamp runs round the circle while they bow, one hind after the
                        // other, two beats a second.
                        let beat = seconds * 2.0;
                        if rite.is_some_and(|u| (BOWING..PROCESSION).contains(&u))
                            && beat.fract() < dt * 2.0
                            && (beat as usize) % n == i
                        {
                            d.stamp = 0.6;
                        }
                        circle(here, slot)
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
        // The rite gathers while (nearly) all walk their places.
        let gathering = mode == Mode::Ritual && self.flight.is_none() && in_place + 1 >= n;
        self.glow = if gathering {
            (self.glow + dt / GATHERING).min(1.0)
        } else {
            (self.glow - dt / 5.0).max(0.0)
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
                let at = Vec2::new(o.at.x, o.at.z);
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

/// Walking the circle: to its place on the ring, at a slow, even pace.
fn circle(here: Vec2, slot: Vec2) -> (Activity, Vec2, f32, bool) {
    let gap = here.distance(slot);
    // Slowing down to stop on its place.
    let speed = if gap > 3.0 {
        WALK_SPEED
    } else {
        (0.45 + gap * 0.4).min(gap * 1.5)
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
    // Try straight on, then veering more and more.
    for veer in [0.0, 0.5, -0.5, 1.1, -1.1, 1.8, -1.8] {
        let angle = d.heading + veer;
        let next =
            here + Vec2::new(angle.sin(), angle.cos()) * step.min(to.length().max(step * 0.2));
        if walkable(world, d.position.y, next) {
            d.heading = if veer == 0.0 {
                d.heading
            } else {
                d.heading + veer * 0.2
            };
            d.position = ground(world, next).with_y(d.position.y);
            let height = world.surface_height(next.x, next.y);
            d.position.y += (height - d.position.y) * (1.0 - (-dt * 10.0).exp());
            d.stride += step * std::f32::consts::TAU / stride_length(d.speed, d.size);
            return;
        }
    }
    d.speed = 0.0;
}

fn inside(world: &World, at: Vec2) -> bool {
    let dims = world.dims();
    at.x >= 1.0 && at.y >= 1.0 && at.x < dims.nx as f32 - 1.0 && at.y < dims.nz as f32 - 1.0
}

/// Dry ground no higher than a deer steps up (a cell and a bit), within the world, and no
/// trunk standing there.
fn walkable(world: &World, from_height: f32, at: Vec2) -> bool {
    if !inside(world, at) {
        return false;
    }
    let (x, z) = (at.x as usize, at.y as usize);
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
    let ring = [8, 6, 4]
        .into_iter()
        .find_map(|clear| meadow(world, start, |x, z| open(x, z, clear)))?;
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
fn meadow(world: &World, start: Vec2, open: impl Fn(usize, usize) -> bool) -> Option<Vec2> {
    let dims = world.dims();
    let height = |x: usize, z: usize| world.ground_top(x, z) as i64;
    let mut best: Option<(f32, Vec2)> = None;
    for z in (8..dims.nz - 8).step_by(2) {
        for x in (8..dims.nx - 8).step_by(2) {
            let at = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            let distance = at.distance(start);
            if !(30.0..90.0).contains(&distance)
                || !matches!(world.biome(x, z), Biome::Plains | Biome::Savanna)
            {
                continue;
            }
            // Flat and dry over the circle and around it.
            let h = height(x, z);
            let mut flat = true;
            'around: for dz in -6i64..=6 {
                for dx in -6i64..=6 {
                    let (cx, cz) = ((x as i64 + dx) as usize, (z as i64 + dz) as usize);
                    if (height(cx, cz) - h).abs() > 1 || world.water_level(cx, cz).is_some() {
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
    let mut rng = SplitMix64::new(seed ^ 0x51ce);
    let r = RING_RADIUS.ceil() as i64 + 1;
    for dz in -r..=r {
        for dx in -r..=r {
            let at = Vec2::new(
                (ring.x.floor() as i64 + dx) as f32 + 0.5,
                (ring.y.floor() as i64 + dz) as f32 + 0.5,
            );
            let off = (at.distance(ring) - RING_RADIUS).abs();
            if off < 0.6 && rng.next_f32() < 0.8 {
                world.wear(at.x as usize, at.y as usize);
            }
        }
    }
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
    }
    fn read(r: &mut crate::save::Reader) -> crate::save::Result<Self> {
        Ok(Self {
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
            grazing: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    fn setup() -> (World, Herd) {
        let mut world = World::generate(WorldConfig::standard(1));
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
}
