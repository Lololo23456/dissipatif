//! Anomalies: the other form of life of this world. Not a plant nor an animal, but a pattern
//! that living things keep together far from equilibrium (a dissipative structure): the hinds
//! walking their circle under the full moon, later the owls' gaze and the marsh's spiral. Each
//! lives on a place, and only while nature is well there.
//!
//! An anomaly is described by:
//! - its **bearers** and its **place**: the herd and its meadow for the rite; a site (the
//!   circle) and a radius (the meadow around it);
//! - its **health**, long-term: how well nature is doing in its place. A raw measure in [0, 1]
//!   (`Kind::measure`) is taken every `MEASURE_EVERY` game seconds and smoothed over days (an
//!   exponential, time constant `HEALTH_DAYS`). An anomaly is born when its health has stayed at
//!   or above `BIRTH` for its ripening days, dies below `DEATH`, and is born again by the same
//!   rule. Between the two thresholds nothing changes (hysteresis): a place must really recover,
//!   not just flicker above a line, for its anomaly to come back;
//! - its **strength**, short-term, 0 to 1: it drops when it is disturbed (`Harm`: its bearers
//!   frightened in its vigil, fire or digging in its place) and comes back by itself, the faster
//!   the healthier its place. A weak anomaly shows itself less (fewer deer on the circle, a
//!   dimmer light, rarer omens);
//! - its **moment**: its nights (`Kind::due`: the rite on the full moon), its window of hours,
//!   its omens on the nights around. Tolerance: if the window passes without the anomaly rising
//!   to its height (`SUMMIT`), or broken off, it comes back once, the next night, at the same
//!   hours, weaker (`ENCORE_STRENGTH`): a missed rite is not lost for a whole moon;
//! - its **trial**: what a player does to understand it (the rite: watch its height unseen),
//!   counted per player and per kind, and the spell it teaches. A spell answers only while the
//!   anomaly that taught it lives: lost when it dies, back when it is born again (the words the
//!   notebook lost meanwhile never come back).
//!
//! Its life: dormant (never yet alive) → alive → dead → alive again… At the start the world is
//! at its balance: the anomalies there are created alive, their health measured at once.
//!
//! The anomaly decides, its bearers carry out: the herd no longer keeps its rite on its own, it
//! reads the `Order` the anomaly gives it each step, before it moves.
//!
//! **Adding a kind** (the owls' gaze, the marsh's spiral): a variant at the end of `Kind` (and
//! of its `persist_enum!` and `Kind::ALL`), then an arm in each `match self` of `impl Kind`:
//! - `measure`: the raw health of its place, from what `Around` lets it read (add to `Around`
//!   what it needs: the owls, the voles, the marsh…);
//! - `due`, `in_window`, `omen_night`, `vigil`: its moment;
//! - `height`, `broken`, `startled`: how it is going now (for the encore and its strength);
//! - `harm`: what each disturbance costs it;
//! - `attends`, `understood`, `trial_seconds`: its trial; `spell`: what it teaches, if anything;
//! - `ripening`, `name`.
//!
//! Then register it where its place is chosen (`Anomalies::register`), and give its bearers its
//! `Order` before they move, as `GameState::step` does for the herd.
//!
//! Deterministic: the omens are drawn by hashing the seed (`SEED`), the night and the kind;
//! everything else follows the world.

use glam::Vec2;
use world::World;

use crate::deer::{self, Herd, Observer};
use crate::notebook::Spell;
use crate::soil::Soil;
use crate::state::Conditions;

/// Seed offset of the anomalies' chance (their omens), see `docs/phase-2-architecture.md`.
const SEED: u64 = 0xa40a;
/// Game seconds between two measures of a place's health.
const MEASURE_EVERY: f32 = 30.0;
/// The same, in game days.
const MEASURE_DAYS: f32 = MEASURE_EVERY / crate::clock::DAY_SECONDS;
/// Days over which health is smoothed (the time constant of the exponential): a place's health
/// is what it has been through these last days, not the last hour.
const HEALTH_DAYS: f32 = 1.5;
/// Health thresholds: born at or above `BIRTH` (held for the kind's ripening days), dead below
/// `DEATH`; between the two, an anomaly stays as it is.
pub const BIRTH: f32 = 0.65;
pub const DEATH: f32 = 0.35;
/// Strength regained per game day, in a place in full health.
const RECOVERY: f32 = 0.5;
/// How high an anomaly must rise in its window to have shown itself (the rite: its light).
pub const SUMMIT: f32 = 0.6;
/// The night after a missed one, an anomaly shows at this share of its strength.
pub const ENCORE_STRENGTH: f32 = 0.6;
/// Share of the omen nights a spent anomaly still shows its omens on (all of them at full
/// strength).
const OMENS_WEAK: f32 = 0.2;
/// How far one sees an anomaly from, cells: its trial, and a night waited for in vain.
pub const WATCH_RANGE: f32 = 32.0;
/// The rite's place: the meadow within this many cells of its circle.
pub const RITE_MEADOW: f32 = 16.0;
/// Game seconds of watching the rite at its height, unnoticed, to understand it.
const RITE_UNDERSTOOD: f32 = 10.0;

/// The kinds of anomalies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The hinds' circle under the full moon, in their meadow: teaches the shape of a deer.
    DeerRite,
}

/// Where an anomaly is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Life {
    /// Never yet alive: its place is not (yet) well enough.
    Dormant,
    Alive,
    /// It lived, and its place failed it; it is born again if the place recovers.
    Dead,
}

impl Life {
    /// In words, for the debug lines.
    fn name(self) -> &'static str {
        match self {
            Life::Dormant => "dormante",
            Life::Alive => "vivante",
            Life::Dead => "morte",
        }
    }
}

/// What disturbs an anomaly, each costing it some strength (`Kind::harm`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harm {
    /// Its bearers took fright during its vigil (the herd stamped, barked or fled): once per
    /// fright.
    Fright,
    /// Fire burning in its place: per game minute.
    Fire,
    /// A handful of its ground dug.
    Dig,
}

/// What the anomalies may read of the world each step, gathered by `GameState`; each kind takes
/// what concerns it.
pub struct Around<'a> {
    pub world: &'a World,
    pub now: &'a Conditions,
    pub soil: &'a Soil,
    pub herd: Option<&'a Herd>,
    /// Where something burns: fires laid by the players, plants on fire.
    pub fires: &'a [Vec2],
}

/// What an anomaly asks of its bearers now (the herd reads it each step).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Order {
    /// It keeps its moment tonight: its own night, or the one after a missed night.
    pub tonight: bool,
    /// Its omens show tonight (the rite: the lead hind turning alone around midnight).
    pub omens: bool,
    /// How strongly it shows, 0 to 1 (weaker the night after a missed one).
    pub strength: f32,
}

impl Order {
    /// Nothing tonight: a dead or dormant anomaly asks nothing.
    pub const NONE: Order = Order {
        tonight: false,
        omens: false,
        strength: 0.0,
    };
}

/// Where a player stands with a spell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellState {
    /// Never learnt.
    Unknown,
    /// Learnt, and the anomaly that taught it lives: it answers.
    Usable,
    /// Learnt, but its anomaly is dead: it no longer answers, until the anomaly is born again.
    Lost,
}

/// A trial under way: which anomaly a player is understanding now, how far (0 to 1), and where
/// it is (its light comes from there).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Understanding {
    pub kind: Kind,
    pub progress: f32,
    pub site: Vec2,
}

impl Kind {
    /// Every kind, in the order of the list.
    pub const ALL: [Kind; 1] = [Kind::DeerRite];

    /// In words, for the debug lines.
    pub fn name(self) -> &'static str {
        match self {
            Kind::DeerRite => "rite des cerfs",
        }
    }

    /// The spell its trial teaches, if any.
    pub fn spell(self) -> Option<Spell> {
        match self {
            Kind::DeerRite => Some(Spell::DeerForm),
        }
    }

    /// The kind that teaches `spell`: the spell lives while it does.
    pub fn teaching(spell: Spell) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.spell() == Some(spell))
    }

    /// Game days its place must stay healthy before it is born (or born again).
    fn ripening(self) -> f32 {
        match self {
            // A herd living for a couple of days on a healthy meadow.
            Kind::DeerRite => 2.0,
        }
    }

    /// The raw health of its place now, 0 to 1 (before smoothing).
    pub fn measure(self, site: Vec2, radius: f32, around: &Around) -> f32 {
        match self {
            // The scarcer of the two (Liebig's minimum): the meadow, and its herd.
            Kind::DeerRite => {
                meadow(site, radius, around).min(around.herd.map_or(0.0, herd_health))
            }
        }
    }

    /// Its own night comes tonight (before any encore).
    pub fn due(self, now: &Conditions) -> bool {
        match self {
            Kind::DeerRite => deer::rite_night(now),
        }
    }

    /// Within the hours of its window, on its night.
    pub fn in_window(self, hour: f32) -> bool {
        match self {
            Kind::DeerRite => deer::rite_hours(hour),
        }
    }

    /// A night its omens may show on.
    fn omen_night(self, now: &Conditions) -> bool {
        match self {
            Kind::DeerRite => deer::omen_night(now),
        }
    }

    /// The hours of its nights (and of its omens' nights) that are watched over: a fright then
    /// wounds it.
    fn vigil(self, hour: f32) -> bool {
        match self {
            Kind::DeerRite => deer::vigil_hours(hour),
        }
    }

    /// How high it rises now, 0 to 1 (the rite: its light).
    fn height(self, around: &Around) -> f32 {
        match self {
            Kind::DeerRite => around.herd.map_or(0.0, |h| h.glow),
        }
    }

    /// Whether it is broken off now (the rite: the herd afraid, fleeing or keeping away).
    fn broken(self, around: &Around) -> bool {
        match self {
            Kind::DeerRite => around.herd.is_some_and(|h| h.frightened(around.now)),
        }
    }

    /// Whether its bearers are frightened now (the rite: a hind stamping, barking, fleeing).
    fn startled(self, around: &Around) -> bool {
        match self {
            Kind::DeerRite => around.herd.is_some_and(Herd::startled),
        }
    }

    /// What a disturbance costs it (strength; for fire, per game minute).
    pub fn harm(self, harm: Harm) -> f32 {
        match (self, harm) {
            (Kind::DeerRite, Harm::Fright) => 0.35,
            (Kind::DeerRite, Harm::Fire) => 0.25,
            (Kind::DeerRite, Harm::Dig) => 0.03,
        }
    }

    /// Whether `who` attends it now as its trial asks (the rite: its height watched from
    /// nearby, the herd unafraid).
    pub fn attends(self, anomaly: &Anomaly, around: &Around, who: &Observer) -> bool {
        match self {
            Kind::DeerRite => around.herd.is_some_and(|h| {
                h.glow > SUMMIT
                    && !h.frightened(around.now)
                    && anomaly.within(around.world, Vec2::new(who.at.x, who.at.z), WATCH_RANGE)
            }),
        }
    }

    /// Game seconds of its trial to understand it.
    pub fn trial_seconds(self) -> f32 {
        match self {
            Kind::DeerRite => RITE_UNDERSTOOD,
        }
    }

    /// Whether one who attended it `seconds` understands it now (the rite: at its height, heads
    /// raised to the moon).
    pub fn understood(self, around: &Around, seconds: f32) -> bool {
        match self {
            Kind::DeerRite => {
                seconds >= RITE_UNDERSTOOD
                    && around
                        .herd
                        .and_then(|h| h.rite)
                        .is_some_and(|u| u >= deer::TO_THE_MOON)
            }
        }
    }
}

/// The meadow around the circle, 0 (burnt, stripped) to 1 (as at the start): over the soil
/// patches whose centre lies within `radius` of `site`, the mean of their cover against the
/// start, less where it has grown bare. Patches where nothing grew at the start are not meadow.
fn meadow(site: Vec2, radius: f32, around: &Around) -> f32 {
    let soil = around.soil;
    let (mut sum, mut count) = (0.0, 0.0);
    for k in 0..soil.patches() {
        let centre = Vec2::from(
            around
                .world
                .nearest(site.to_array(), soil.centre(k).to_array()),
        );
        let start = soil.cover_start(k);
        if centre.distance(site) >= radius || start < 0.1 {
            continue;
        }
        sum += (soil.cover(k) / start).min(1.0) * (1.0 - soil.bareness(k));
        count += 1.0;
    }
    if count > 0.0 { sum / count } else { 0.0 }
}

/// The herd, 0 to 1: four deer or more, fed (mean reserves of 0.4 or more); softly less below.
fn herd_health(herd: &Herd) -> f32 {
    let n = herd.deer.len() as f32;
    if n == 0.0 {
        return 0.0;
    }
    let reserves = herd.deer.iter().map(|d| d.energy).sum::<f32>() / n;
    ramp(2.0, 4.0, n).min(ramp(0.2, 0.4, reserves))
}

/// 0 below `a`, 1 above `b`, smooth between.
fn ramp(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The night of a moment, `days` since the start: night N runs from noon of day N to noon of
/// day N + 1, so that an evening and the small hours after it are the same night.
pub fn night_of(days: f64) -> u32 {
    ((days - 0.5).floor() + 1.0).max(0.0) as u32
}

/// An anomaly's window tonight, while it is open.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Watch {
    /// The night (see `night_of`).
    night: u32,
    /// It is the encore of a missed night (it does not come back a second time).
    encore: bool,
    /// The highest it rose.
    peak: f32,
    /// Whether it was broken off.
    broken: bool,
}

pub struct Anomaly {
    pub kind: Kind,
    /// Its place: the centre (the rite: its circle), and how far it reaches.
    pub site: Vec2,
    pub radius: f32,
    pub life: Life,
    /// The health of its place, smoothed over days, 0 to 1.
    pub health: f32,
    /// How strongly it shows, 0 to 1.
    pub strength: f32,
    /// When its life last changed (game days; 0 for one there from the start).
    pub since: f64,
    /// Game days its place's health has stayed at or above `BIRTH`.
    pub healthy_for: f32,
    /// The night it comes back, after a missed one.
    pub encore: Option<u32>,
    /// When it last rose to its height (game days).
    pub last_shown: Option<f64>,
    /// Its window tonight, while it is open.
    watch: Option<Watch>,
    /// Its bearers are frightened now (a fright wounds it once, when it starts).
    troubled: bool,
    /// Whether its place has been measured yet (the first measure is taken as it is).
    measured: bool,
}

impl Anomaly {
    /// An anomaly of `kind` on its place, at full strength. Created `Alive` where the world is
    /// at its balance (its health is measured at the first step); `Dormant` where it waits for
    /// its place to be well.
    pub fn new(kind: Kind, site: Vec2, radius: f32, life: Life) -> Self {
        Self {
            kind,
            site,
            radius,
            life,
            health: if life == Life::Alive { 1.0 } else { 0.0 },
            strength: 1.0,
            since: 0.0,
            healthy_for: 0.0,
            encore: None,
            last_shown: None,
            watch: None,
            troubled: false,
            measured: false,
        }
    }

    /// Whether `at` lies within `range` of its site (across the edges of the world if need be).
    pub fn within(&self, world: &World, at: Vec2, range: f32) -> bool {
        Vec2::from(world.nearest(self.site.to_array(), at.to_array())).distance(self.site) < range
    }

    /// Whether `at` lies in its place.
    pub fn holds(&self, world: &World, at: Vec2) -> bool {
        self.within(world, at, self.radius)
    }

    /// Disturbed: weaker by `amount`.
    fn weaken(&mut self, amount: f32) {
        self.strength = (self.strength - amount).max(0.0);
    }

    /// `days` go by: strength comes back by itself, the faster the healthier its place.
    fn recover(&mut self, days: f32) {
        self.strength = (self.strength + RECOVERY * self.health * days).min(1.0);
    }

    /// What it asks of its bearers now. Nothing unless alive; its moment on its own nights and
    /// on the encore of a missed one (then weaker); its omens on the nights around, the fewer
    /// the weaker it is (drawn once a night, from the seed `seed`).
    pub fn order(&self, now: &Conditions, seed: u64) -> Order {
        if self.life != Life::Alive {
            return Order::NONE;
        }
        let night = night_of(now.days);
        let due = self.kind.due(now);
        let encore = !due && self.encore == Some(night);
        let chance = OMENS_WEAK + (1.0 - OMENS_WEAK) * self.strength;
        let omens = self.kind.omen_night(now)
            && world::noise::hash_unit(seed ^ SEED, &[self.kind as i64, night as i64]) < chance;
        Order {
            tonight: due || encore,
            omens,
            strength: self.strength * if encore { ENCORE_STRENGTH } else { 1.0 },
        }
    }

    /// One step of `seconds` game seconds: strength comes back or is lost, the night's window is
    /// followed, and, when `measure`, the place's health is measured. Returns its new life, if
    /// it changed.
    fn step(&mut self, around: &Around, seconds: f32, measure: bool) -> Option<Life> {
        let now = around.now;
        self.recover(seconds / crate::clock::DAY_SECONDS);
        if around.fires.iter().any(|&f| self.holds(around.world, f)) {
            self.weaken(self.kind.harm(Harm::Fire) * seconds / 60.0);
        }
        let order = self.order(now, around.world.config.seed);
        // A fright in its vigil wounds it, once, when it starts.
        let startled = self.kind.startled(around);
        if startled && !self.troubled && (order.tonight || order.omens) && self.kind.vigil(now.hour)
        {
            self.weaken(self.kind.harm(Harm::Fright));
        }
        self.troubled = startled;
        let open = order.tonight && self.kind.in_window(now.hour);
        let encore = !self.kind.due(now);
        self.follow_night(
            open,
            night_of(now.days),
            encore,
            self.kind.height(around),
            self.kind.broken(around),
            now.days,
        );
        if !measure {
            return None;
        }
        let raw = self.kind.measure(self.site, self.radius, around);
        let next = self.observe(raw, MEASURE_DAYS)?;
        self.set_life(next, now.days);
        Some(next)
    }

    /// Follows its window on night `night` (`open` while it is), as high as `height` now, and
    /// `broken` off or not; `encore` when the night is an encore. When the window has passed
    /// without its height, or broken off, it comes back once, the next night.
    fn follow_night(
        &mut self,
        open: bool,
        night: u32,
        encore: bool,
        height: f32,
        broken: bool,
        days: f64,
    ) {
        if open {
            let watch = self.watch.get_or_insert(Watch {
                night,
                encore,
                peak: 0.0,
                broken: false,
            });
            watch.peak = watch.peak.max(height);
            watch.broken |= broken;
            if height > SUMMIT {
                self.last_shown = Some(days);
            }
        } else if let Some(watch) = self.watch.take() {
            // The window has passed: it showed itself, or it comes back the next night (once).
            let missed = watch.peak <= SUMMIT || watch.broken;
            self.encore = (missed && !watch.encore).then_some(watch.night + 1);
        } else if self.encore.is_some_and(|n| n < night) {
            // An encore whose night went by without its window (it died meanwhile).
            self.encore = None;
        }
    }

    /// A measure `raw` of its place's health, `days` after the last: health follows it slowly
    /// (the first measure is taken as it is). Returns the life this calls for, if it changes:
    /// death below `DEATH`, birth after the ripening days at or above `BIRTH`.
    fn observe(&mut self, raw: f32, days: f32) -> Option<Life> {
        let raw = raw.clamp(0.0, 1.0);
        if self.measured {
            self.health += (raw - self.health) * (1.0 - (-days / HEALTH_DAYS).exp());
        } else {
            self.health = raw;
            self.measured = true;
        }
        if self.health >= BIRTH {
            self.healthy_for += days;
        } else {
            self.healthy_for = 0.0;
        }
        let next = match self.life {
            Life::Alive if self.health < DEATH => Life::Dead,
            Life::Dormant | Life::Dead if self.healthy_for >= self.kind.ripening() => Life::Alive,
            life => life,
        };
        (next != self.life).then_some(next)
    }

    /// Its life becomes `life`, at `days`. Not alive, its moment is over (no window, no encore)
    /// and its place must be healthy again for the ripening days before it is born again.
    fn set_life(&mut self, life: Life, days: f64) {
        if life == self.life {
            return;
        }
        self.life = life;
        self.since = days;
        if life != Life::Alive {
            self.watch = None;
            self.encore = None;
            self.healthy_for = 0.0;
        }
    }

    /// One line for `DISSIPATIF_DEBUG_ANOMALIES`, in French.
    pub fn describe(&self) -> String {
        let encore = self
            .encore
            .map_or("non".to_owned(), |night| format!("nuit {night}"));
        let shown = self.last_shown.map_or("jamais".to_owned(), |days| {
            let hour = days.fract() * 24.0;
            format!(
                "jour {}, {:02} h {:02}",
                days as u32 + 1,
                hour as u32,
                (hour.fract() * 60.0) as u32
            )
        });
        format!(
            "anomalie : {}, {} depuis le jour {}, santé {:.2} (saine depuis {:.1} j), force {:.2}, \
             lieu ({:.1}, {:.1}) sur {:.0} cases, encore : {encore}, à son sommet : {shown}",
            self.kind.name(),
            self.life.name(),
            self.since as u32 + 1,
            self.health,
            self.healthy_for,
            self.strength,
            self.site.x,
            self.site.y,
            self.radius,
        )
    }
}

/// The anomalies of the world.
#[derive(Default)]
pub struct Anomalies {
    list: Vec<Anomaly>,
    /// Game seconds until their places are measured again.
    measure_in: f32,
}

impl Anomalies {
    /// An anomaly comes to live in the world (one of each kind: the first registered is the one
    /// `get` finds, and the one its spell follows).
    pub fn register(&mut self, anomaly: Anomaly) {
        self.list.push(anomaly);
    }

    /// The anomaly of `kind`, if the world has one.
    pub fn get(&self, kind: Kind) -> Option<&Anomaly> {
        self.list.iter().find(|a| a.kind == kind)
    }

    fn get_mut(&mut self, kind: Kind) -> Option<&mut Anomaly> {
        self.list.iter_mut().find(|a| a.kind == kind)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Anomaly> {
        self.list.iter()
    }

    /// The anomaly of `kind` is disturbed: weaker by `amount`.
    pub fn disturb(&mut self, kind: Kind, amount: f32) {
        if let Some(a) = self.get_mut(kind) {
            a.weaken(amount);
        }
    }

    /// Sets the life of the anomaly of `kind` at once, at `days` (tests and captures). Returns
    /// whether it changed.
    pub fn force_life(&mut self, kind: Kind, life: Life, days: f64) -> bool {
        match self.get_mut(kind) {
            Some(a) if a.life != life => {
                a.set_life(life, days);
                true
            }
            _ => false,
        }
    }

    /// One step of `seconds` game seconds for every anomaly; those whose life changed go onto
    /// `changes`, with their new life.
    pub fn step(&mut self, around: &Around, seconds: f32, changes: &mut Vec<(Kind, Life)>) {
        self.measure_in -= seconds;
        let measure = self.measure_in <= 0.0;
        if measure {
            self.measure_in = MEASURE_EVERY;
        }
        for a in &mut self.list {
            if let Some(life) = a.step(around, seconds, measure) {
                changes.push((a.kind, life));
            }
        }
    }
}

crate::save::persist_enum!(Kind { DeerRite });
crate::save::persist_enum!(Life {
    Dormant,
    Alive,
    Dead
});
crate::save::persist_struct!(Watch {
    night,
    encore,
    peak,
    broken
});
crate::save::persist_struct!(Anomaly {
    kind,
    site,
    radius,
    life,
    health,
    strength,
    since,
    healthy_for,
    encore,
    last_shown,
    watch,
    troubled,
    measured,
});
crate::save::persist_struct!(Anomalies { list, measure_in });

#[cfg(test)]
mod tests {
    use super::*;

    /// The moment `days` since the start, under a moon of phase `moon`.
    fn at(days: f64, moon: f32) -> Conditions {
        Conditions {
            hour: (days.fract() * 24.0) as f32,
            days,
            moon,
            rain: 0.0,
            wind: Vec2::X,
            year: 0.45,
        }
    }

    fn rite(life: Life) -> Anomaly {
        Anomaly::new(Kind::DeerRite, Vec2::new(40.0, 40.0), RITE_MEADOW, life)
    }

    /// Measures `raw` health for `days`, as the game does (every `MEASURE_DAYS`); the lives it
    /// went through, in order.
    fn live(a: &mut Anomaly, raw: f32, days: f32) -> Vec<Life> {
        let mut lives = Vec::new();
        for _ in 0..(days / MEASURE_DAYS).round() as usize {
            if let Some(life) = a.observe(raw, MEASURE_DAYS) {
                a.set_life(life, 0.0);
                lives.push(life);
            }
        }
        lives
    }

    #[test]
    fn a_night_runs_from_noon_to_noon() {
        // The evening of the third day and the small hours after it are the third night.
        assert_eq!(night_of(2.0 + 21.0 / 24.0), 3);
        assert_eq!(night_of(3.0 + 1.0 / 24.0), 3);
        assert_eq!(night_of(3.0 + 13.0 / 24.0), 4);
        assert_eq!(night_of(0.69), 1, "the first evening of the game");
    }

    #[test]
    fn a_healthy_place_gives_birth_once_it_has_ripened() {
        let mut a = rite(Life::Dormant);
        a.observe(0.9, MEASURE_DAYS);
        assert!(
            live(&mut a, 0.9, 1.5).is_empty(),
            "born after a day and a half only (its ripening is {} days)",
            Kind::DeerRite.ripening()
        );
        assert_eq!(live(&mut a, 0.9, 0.6), vec![Life::Alive]);
    }

    #[test]
    fn a_failing_place_lets_its_anomaly_die_but_not_at_once() {
        let mut a = rite(Life::Alive);
        a.observe(0.95, MEASURE_DAYS);
        // A burnt meadow: health follows over days, it does not drop in an hour.
        assert!(
            live(&mut a, 0.05, 0.5).is_empty(),
            "dead after half a day: health {}",
            a.health
        );
        assert_eq!(live(&mut a, 0.05, 2.5), vec![Life::Dead]);
        assert!(a.health < DEATH);
    }

    #[test]
    fn between_the_thresholds_nothing_changes() {
        // Alive, a place gone middling keeps its anomaly…
        let mut alive = rite(Life::Alive);
        alive.observe(0.9, MEASURE_DAYS);
        assert!(live(&mut alive, 0.5, 20.0).is_empty());
        assert_eq!(alive.life, Life::Alive);
        // …dead, the same middling place does not bring it back: it must truly recover.
        let mut dead = rite(Life::Dead);
        dead.observe(0.2, MEASURE_DAYS);
        assert!(live(&mut dead, 0.5, 20.0).is_empty());
        assert_eq!(dead.life, Life::Dead);
        assert_eq!(live(&mut dead, 0.9, 6.0), vec![Life::Alive]);
    }

    #[test]
    fn a_flickering_place_does_not_bring_it_back() {
        // Healthy a day, ailing a day, again and again: never two days well in a row.
        let mut a = rite(Life::Dead);
        a.observe(0.2, MEASURE_DAYS);
        for _ in 0..10 {
            assert!(live(&mut a, 1.0, 1.0).is_empty(), "born of a good day");
            live(&mut a, 0.2, 1.0);
        }
        assert_eq!(a.life, Life::Dead);
    }

    #[test]
    fn disturbed_it_weakens_and_comes_back_as_its_place_allows() {
        let mut a = rite(Life::Alive);
        a.observe(1.0, MEASURE_DAYS);
        a.weaken(Kind::DeerRite.harm(Harm::Fright));
        a.weaken(Kind::DeerRite.harm(Harm::Fright));
        assert!((a.strength - 0.3).abs() < 1e-5, "{}", a.strength);
        for _ in 0..20 {
            a.weaken(Kind::DeerRite.harm(Harm::Dig));
        }
        assert_eq!(a.strength, 0.0, "never below nothing");
        // Half a day in full health: a quarter back.
        a.recover(0.5);
        assert!((a.strength - 0.25).abs() < 1e-5, "{}", a.strength);
        // An ailing place mends it slower.
        let mut ailing = rite(Life::Alive);
        ailing.observe(0.4, MEASURE_DAYS);
        ailing.strength = 0.0;
        ailing.recover(0.5);
        assert!(ailing.strength < 0.11, "{}", ailing.strength);
        a.recover(10.0);
        assert_eq!(a.strength, 1.0, "never above full strength");
    }

    #[test]
    fn a_missed_night_comes_back_once_the_next_night_weaker() {
        let mut a = rite(Life::Alive);
        let full = 0.5;
        // Night 3: the window opens, the light never rises to its height, the window passes.
        let night = 2.0 + 22.0 / 24.0;
        assert!(a.order(&at(night, full), 1).tonight);
        a.follow_night(true, 3, false, 0.4, false, night);
        a.follow_night(false, 3, false, 0.0, false, night + 0.05);
        assert_eq!(a.encore, Some(4));
        // Night 4: the moon is no longer full, the rite comes back all the same, weaker.
        let encore = at(3.0 + 22.0 / 24.0, full + 1.0 / crate::clock::LUNAR_DAYS);
        let order = a.order(&encore, 1);
        assert!(order.tonight, "no encore the next night");
        assert!((order.strength - ENCORE_STRENGTH).abs() < 1e-5);
        // Missed again: no third chance.
        a.follow_night(true, 4, true, 0.4, false, encore.days);
        a.follow_night(false, 4, true, 0.0, false, encore.days + 0.05);
        assert_eq!(a.encore, None);
        let after = at(4.0 + 22.0 / 24.0, full + 2.0 / crate::clock::LUNAR_DAYS);
        assert!(!a.order(&after, 1).tonight);
    }

    #[test]
    fn a_night_kept_or_broken_off() {
        // At its height: shown, no encore.
        let mut kept = rite(Life::Alive);
        kept.follow_night(true, 3, false, 0.95, false, 2.92);
        kept.follow_night(false, 3, false, 0.0, false, 2.95);
        assert_eq!(kept.encore, None);
        assert_eq!(kept.last_shown, Some(2.92));
        // At its height, but the herd fled: broken off, it comes back the next night.
        let mut broken = rite(Life::Alive);
        broken.follow_night(true, 3, false, 0.95, false, 2.92);
        broken.follow_night(true, 3, false, 0.5, true, 2.93);
        broken.follow_night(false, 3, false, 0.0, false, 2.95);
        assert_eq!(broken.encore, Some(4));
    }

    #[test]
    fn a_dead_anomaly_asks_nothing_of_its_bearers() {
        let full_moon = at(2.0 + 22.0 / 24.0, 0.5);
        assert!(rite(Life::Alive).order(&full_moon, 1).tonight);
        assert_eq!(rite(Life::Dead).order(&full_moon, 1), Order::NONE);
        assert_eq!(rite(Life::Dormant).order(&full_moon, 1), Order::NONE);
    }

    #[test]
    fn a_weak_anomaly_shows_fewer_omens() {
        // A gibbous moon: the nights around the full moon.
        let omens = |strength: f32| {
            let mut a = rite(Life::Alive);
            a.strength = strength;
            (0..400)
                .filter(|&night| a.order(&at(night as f64 + 0.95, 0.35), 7).omens)
                .count()
        };
        assert_eq!(omens(1.0), 400, "at full strength, every omen night");
        let weak = omens(0.0);
        assert!(
            (40..130).contains(&weak),
            "{weak} omen nights of 400 when spent (about {OMENS_WEAK} expected)"
        );
        assert!(omens(0.5) > weak);
    }

    #[test]
    fn anomalies_come_back_from_a_save_as_they_were() {
        let mut anomalies = Anomalies::default();
        let mut a = rite(Life::Alive);
        a.observe(0.8, MEASURE_DAYS);
        a.strength = 0.42;
        a.healthy_for = 3.5;
        a.encore = Some(4);
        a.last_shown = Some(2.93);
        a.troubled = true;
        a.follow_night(true, 4, true, 0.3, false, 3.92);
        anomalies.register(a);
        anomalies.measure_in = 12.5;
        let mut w = crate::save::Writer::default();
        w.put(&anomalies);
        let mut r = crate::save::Reader::new(&w.bytes);
        let back: Anomalies = r.get().expect("anomalies");
        let (a, b) = (&anomalies.list[0], &back.list[0]);
        assert_eq!(
            (a.kind, a.site, a.radius, a.life, a.health, a.strength),
            (b.kind, b.site, b.radius, b.life, b.health, b.strength)
        );
        assert_eq!(
            (a.since, a.healthy_for, a.encore, a.last_shown),
            (b.since, b.healthy_for, b.encore, b.last_shown)
        );
        assert_eq!(
            (a.watch, a.troubled, a.measured),
            (b.watch, b.troubled, b.measured)
        );
        assert_eq!(back.measure_in, 12.5);
    }
}
