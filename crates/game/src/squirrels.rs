//! Red squirrels (Sciurus vulgaris): the first animals that plant.
//!
//! Real behaviour: diurnal and arboreal, each squirrel lives around a few trees. In autumn it
//! gathers acorns and hazelnuts and buries them **one by one**, scattered some way off (so a
//! thief never finds them all: "scatter-hoarding"). In winter it finds them again by its
//! memory of the places and by smell, and steals others' caches when it smells them. Some are
//! never found: they germinate in spring. Oaks and hazels spread this way, far from their
//! parents. Oaks bear a great many acorns some years and few others ("mast years").
//!
//! In the game: each cache is a real point in the soil. What remains of the caches in spring
//! goes to the seed bank of the soil (see `ecology.rs`), and comes up if the ground allows.
//! Deterministic: randomness from a seeded generator.

use glam::{Vec2, Vec3};
use sim::rng::SplitMix64;
use world::{Plant, PlantInstance, World};

use crate::state::Conditions;

/// Squirrels at most, each with a home tree.
const SQUIRRELS: usize = 8;
/// Speeds (cells per second): hopping along the ground, dashing away.
const HOP_SPEED: f32 = 2.2;
const DASH_SPEED: f32 = 6.0;
/// A cache is buried this far from where the nut was found, at most and at least.
const CACHE_NEAR: f32 = 4.0;
const CACHE_FAR: f32 = 18.0;
/// Real seconds to bury a nut, to dig one up and eat it.
const BURYING: f32 = 3.0;
const DIGGING: f32 = 4.0;
/// Chance a squirrel remembers one of its own caches; how far it smells a nut in the ground.
const MEMORY: f32 = 0.85;
const SMELL: f32 = 1.5;
/// Nuts a squirrel eats in a game day in the lean months (from its caches).
const NUTS_PER_DAY: f32 = 3.0;
/// It flees when the naturalist comes this close.
const FLIGHT_DISTANCE: f32 = 5.0;
/// Caches at most (the oldest rot first).
const MAX_CACHES: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nut {
    Acorn,
    Hazelnut,
}

impl Nut {
    pub fn plant(self) -> Plant {
        match self {
            Nut::Acorn => Plant::Oak,
            Nut::Hazelnut => Plant::Hazel,
        }
    }
}

/// A nut buried in the soil.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cache {
    pub at: Vec2,
    pub nut: Nut,
    /// The squirrel that buried it.
    pub owner: usize,
    /// Whether the owner remembers where it is.
    pub remembered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Doing {
    /// In its tree, out of sight (at night, by the cold, or after a fright).
    InTree,
    /// Hopping about on the ground near its tree.
    Roaming,
    /// Carrying a nut to where it will bury it.
    Carrying(Nut, Vec2),
    /// Burying a nut (seconds left).
    Burying(Nut, f32),
    /// Going to dig up a cache (its index).
    Seeking(usize),
    /// Digging up a cache and eating the nut (seconds left).
    Digging(usize, f32),
    /// Running back to its tree.
    Fleeing,
}

pub struct Squirrel {
    pub position: Vec3,
    pub heading: f32,
    pub doing: Doing,
    /// Its tree, and the nut trees near it (oaks, hazels).
    pub home: Vec2,
    sources: Vec<(Vec2, Nut)>,
    /// Hop cycle (radians).
    pub hop: f32,
    pub speed: f32,
    /// Seconds until it decides something new.
    timer: f32,
    /// Nuts it owes its stomach (it eats from its caches in the lean months).
    hunger: f32,
}

/// What happened, for the presentation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SquirrelEvent {
    Buried { at: Vec2 },
    DugUp { at: Vec2 },
}

pub struct Squirrels {
    pub squirrels: Vec<Squirrel>,
    pub caches: Vec<Cache>,
    rng: SplitMix64,
    /// This year's crop of nuts, 0.3 (a lean year) to 1.6 (a mast year), and the year it was
    /// drawn for.
    crop: f32,
    crop_year: i64,
    /// Whether the caches left this spring went to the soil already.
    sown_year: i64,
}

impl Squirrels {
    /// Squirrels at the oaks and hazels nearest to `near` (where the naturalist starts).
    pub fn new(plants: &[PlantInstance], world: &World, near: Vec2, seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let at = |p: &PlantInstance| {
            Vec2::new(
                p.base[0] as f32 + 0.5 + p.offset[0],
                p.base[2] as f32 + 0.5 + p.offset[1],
            )
        };
        let nut_trees: Vec<(Vec2, Nut)> = plants
            .iter()
            .filter_map(|p| match p.plant {
                Plant::Oak => Some((at(p), Nut::Acorn)),
                Plant::Hazel => Some((at(p), Nut::Hazelnut)),
                _ => None,
            })
            .collect();
        let mut oaks: Vec<Vec2> = nut_trees
            .iter()
            .filter(|(_, n)| *n == Nut::Acorn)
            .map(|&(p, _)| p)
            .collect();
        oaks.sort_by(|a, b| a.distance(near).total_cmp(&b.distance(near)));
        // Homes a little apart from each other (squirrels keep their own ranges).
        let mut homes: Vec<Vec2> = Vec::new();
        for oak in oaks {
            if homes.len() >= SQUIRRELS {
                break;
            }
            if homes.iter().all(|h| h.distance(oak) > 12.0) {
                homes.push(oak);
            }
        }
        let squirrels = homes
            .into_iter()
            .map(|home| {
                let sources = nut_trees
                    .iter()
                    .filter(|(p, _)| p.distance(home) < 20.0)
                    .copied()
                    .collect();
                Squirrel {
                    position: Vec3::new(home.x, world.surface_height(home.x, home.y), home.y),
                    heading: rng.next_f32() * std::f32::consts::TAU,
                    doing: Doing::InTree,
                    home,
                    sources,
                    hop: 0.0,
                    speed: 0.0,
                    timer: rng.next_f32() * 5.0,
                    hunger: 0.0,
                }
            })
            .collect();
        Self {
            squirrels,
            caches: Vec::new(),
            rng,
            crop: 1.0,
            crop_year: -1,
            sown_year: -1,
        }
    }

    /// One step of `dt` seconds. `players`: where the naturalists are (they frighten them).
    /// Caches left at the start of spring are handed to `sow` (plant, place) for the soil.
    pub fn update(
        &mut self,
        dt: f32,
        world: &World,
        now: &Conditions,
        players: &[Vec3],
        events: &mut Vec<SquirrelEvent>,
        sow: &mut Vec<(Plant, Vec2)>,
    ) {
        let year = now.year;
        let year_count = (now.days / crate::season::YEAR_DAYS as f64).floor() as i64;
        // The crop of the year: drawn at the start of each autumn.
        if year >= 0.5 && self.crop_year < year_count {
            self.crop_year = year_count;
            let draw = self.rng.next_f32();
            self.crop = if draw < 0.2 { 1.6 } else { 0.3 + 0.9 * draw };
        }
        // Spring: what was not found goes to the soil, to come up if it can.
        if year < 0.1 && self.sown_year < year_count {
            self.sown_year = year_count;
            sow.extend(self.caches.drain(..).map(|c| (c.nut.plant(), c.at)));
        }
        let season = crate::season::season(year);
        let autumn = season == crate::season::Season::Autumn;
        let lean = matches!(
            season,
            crate::season::Season::Winter | crate::season::Season::Spring
        ) && year > 0.6
            || year < 0.15;
        let daytime = (7.5..18.0).contains(&now.hour);
        let days = dt / crate::clock::DAY_SECONDS;
        for i in 0..self.squirrels.len() {
            let here = {
                let p = self.squirrels[i].position;
                Vec2::new(p.x, p.z)
            };
            if lean {
                self.squirrels[i].hunger += NUTS_PER_DAY * days;
            }
            // Frightened: back to its tree.
            let threatened = players
                .iter()
                .any(|p| Vec2::new(p.x, p.z).distance(here) < FLIGHT_DISTANCE);
            let s = &mut self.squirrels[i];
            if threatened && !matches!(s.doing, Doing::InTree | Doing::Fleeing) {
                s.doing = Doing::Fleeing;
            }
            s.timer -= dt;
            let decide = s.timer <= 0.0;
            let doing = s.doing;
            let (goal, speed) = match doing {
                Doing::Fleeing => {
                    if here.distance(s.home) < 0.5 {
                        s.doing = Doing::InTree;
                        s.timer = 20.0 + 20.0 * self.rng.next_f32();
                    }
                    (s.home, DASH_SPEED)
                }
                Doing::InTree => {
                    if decide && daytime && !threatened {
                        s.doing = Doing::Roaming;
                        s.timer = 2.0;
                    }
                    (s.home, 0.0)
                }
                _ if !daytime => {
                    s.doing = Doing::Fleeing;
                    (s.home, HOP_SPEED)
                }
                Doing::Roaming => {
                    if decide {
                        s.timer = 3.0 + 5.0 * self.rng.next_f32();
                        if autumn && !s.sources.is_empty() && self.rng.next_f32() < 0.35 * self.crop
                        {
                            // A nut from one of its trees, carried off to be buried.
                            let k = self.rng.next_below(s.sources.len());
                            let (tree, nut) = s.sources[k];
                            let angle = self.rng.next_f32() * std::f32::consts::TAU;
                            let reach = CACHE_NEAR + (CACHE_FAR - CACHE_NEAR) * self.rng.next_f32();
                            let spot = tree + Vec2::new(angle.cos(), angle.sin()) * reach;
                            if world_walkable(world, spot) {
                                s.position =
                                    Vec3::new(tree.x, world.surface_height(tree.x, tree.y), tree.y);
                                s.doing = Doing::Carrying(nut, spot);
                            }
                        } else if s.hunger >= 1.0 {
                            // Hungry: to a cache it remembers, or one it smells.
                            let own = (0..self.caches.len())
                                .filter(|&c| self.caches[c].owner == i && self.caches[c].remembered)
                                .min_by(|&a, &b| {
                                    self.caches[a]
                                        .at
                                        .distance(here)
                                        .total_cmp(&self.caches[b].at.distance(here))
                                });
                            let smelt = (0..self.caches.len())
                                .find(|&c| self.caches[c].at.distance(here) < SMELL);
                            if let Some(c) = smelt.or(own) {
                                s.doing = Doing::Seeking(c);
                            }
                        }
                    }
                    let angle = s.heading + (self.rng.next_f32() - 0.5) * 2.0;
                    let wander = here + Vec2::new(angle.sin(), angle.cos()) * 1.5;
                    let goal = if wander.distance(s.home) > 10.0 {
                        s.home
                    } else {
                        wander
                    };
                    (goal, if decide { HOP_SPEED * 0.6 } else { 0.0 })
                }
                Doing::Carrying(nut, spot) => {
                    if here.distance(spot) < 0.4 {
                        s.doing = Doing::Burying(nut, BURYING);
                    }
                    (spot, HOP_SPEED)
                }
                Doing::Burying(nut, left) => {
                    let left = left - dt;
                    if left <= 0.0 {
                        if self.caches.len() >= MAX_CACHES {
                            self.caches.remove(0);
                        }
                        let remembered = self.rng.next_f32() < MEMORY;
                        self.caches.push(Cache {
                            at: here,
                            nut,
                            owner: i,
                            remembered,
                        });
                        events.push(SquirrelEvent::Buried { at: here });
                        let s = &mut self.squirrels[i];
                        s.doing = Doing::Roaming;
                        s.timer = 1.0;
                        continue;
                    }
                    s.doing = Doing::Burying(nut, left);
                    (here, 0.0)
                }
                Doing::Seeking(c) => match self.caches.get(c) {
                    Some(cache) if cache.at.distance(here) < 0.4 => {
                        s.doing = Doing::Digging(c, DIGGING);
                        (here, 0.0)
                    }
                    Some(cache) => (cache.at, HOP_SPEED),
                    None => {
                        s.doing = Doing::Roaming;
                        (here, 0.0)
                    }
                },
                Doing::Digging(c, left) => {
                    let left = left - dt;
                    if left <= 0.0 {
                        if c < self.caches.len() {
                            let cache = self.caches.swap_remove(c);
                            events.push(SquirrelEvent::DugUp { at: cache.at });
                            // Indices moved: whoever sought the moved cache looks again.
                            for other in &mut self.squirrels {
                                if matches!(other.doing, Doing::Seeking(k) if k == c || k == self.caches.len())
                                {
                                    other.doing = Doing::Roaming;
                                }
                            }
                        }
                        let s = &mut self.squirrels[i];
                        s.hunger = (s.hunger - 1.0).max(0.0);
                        s.doing = Doing::Roaming;
                        s.timer = 1.0;
                        continue;
                    }
                    s.doing = Doing::Digging(c, left);
                    (here, 0.0)
                }
            };
            let s = &mut self.squirrels[i];
            step(s, world, goal, speed, dt);
        }
    }

    /// Takes the cache within `reach` of `at`, if there is one (the naturalist digging).
    pub fn dig_up(&mut self, at: Vec2, reach: f32) -> Option<Nut> {
        let k = (0..self.caches.len()).find(|&k| self.caches[k].at.distance(at) < reach)?;
        let cache = self.caches.swap_remove(k);
        for s in &mut self.squirrels {
            if matches!(s.doing, Doing::Seeking(c) | Doing::Digging(c, _) if c == k || c == self.caches.len())
            {
                s.doing = Doing::Roaming;
            }
        }
        Some(cache.nut)
    }

    /// Whether a cache lies within `reach` of `at`.
    pub fn cache_near(&self, at: Vec2, reach: f32) -> bool {
        self.caches.iter().any(|c| c.at.distance(at) < reach)
    }
}

fn world_walkable(world: &World, at: Vec2) -> bool {
    let d = world.dims();
    at.x > 1.0
        && at.y > 1.0
        && at.x < d.nx as f32 - 1.0
        && at.y < d.nz as f32 - 1.0
        && world.water_level(at.x as usize, at.y as usize).is_none()
}

/// Hops towards `goal` at `speed`.
fn step(s: &mut Squirrel, world: &World, goal: Vec2, speed: f32, dt: f32) {
    let here = Vec2::new(s.position.x, s.position.z);
    let to = goal - here;
    s.speed += (speed - s.speed) * (1.0 - (-dt * 8.0).exp());
    if to.length() < 0.05 || s.speed < 0.01 {
        return;
    }
    s.heading = to.x.atan2(to.y);
    let next = here + to.normalize() * (s.speed * dt).min(to.length());
    if !world_walkable(world, next) {
        return;
    }
    s.position = Vec3::new(next.x, world.surface_height(next.x, next.y), next.y);
    s.hop += s.speed * dt * std::f32::consts::TAU / 0.6;
}

crate::save::persist_enum!(Nut { Acorn, Hazelnut });
crate::save::persist_struct!(Cache {
    at,
    nut,
    owner,
    remembered
});

impl crate::save::Persist for Squirrel {
    fn write(&self, w: &mut crate::save::Writer) {
        w.put(&self.position);
        w.put(&self.heading);
        w.put(&self.home);
        w.put(
            &self
                .sources
                .iter()
                .map(|&(p, n)| (p, n))
                .collect::<Vec<_>>(),
        );
        w.put(&self.hunger);
    }
    fn read(r: &mut crate::save::Reader) -> crate::save::Result<Self> {
        Ok(Self {
            position: r.get()?,
            heading: r.get()?,
            home: r.get()?,
            sources: r.get()?,
            hunger: r.get()?,
            doing: Doing::InTree,
            hop: 0.0,
            speed: 0.0,
            timer: 1.0,
        })
    }
}

impl crate::save::Persist for Squirrels {
    fn write(&self, w: &mut crate::save::Writer) {
        w.put(&self.squirrels);
        w.put(&self.caches);
        w.put(&self.rng);
        w.put(&self.crop);
        w.put(&self.crop_year);
        w.put(&self.sown_year);
    }
    fn read(r: &mut crate::save::Reader) -> crate::save::Result<Self> {
        Ok(Self {
            squirrels: r.get()?,
            caches: r.get()?,
            rng: r.get()?,
            crop: r.get()?,
            crop_year: r.get()?,
            sown_year: r.get()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    /// Through a year: caches are made in autumn, eaten from in winter, and what is left goes
    /// to the soil in spring.
    #[test]
    fn squirrels_hoard_in_autumn_eat_in_winter_and_leave_some_to_sprout() {
        let world = World::generate(WorldConfig::standard(1));
        let start = crate::player::spawn_point(&world);
        let mut squirrels = Squirrels::new(world.plants(), &world, Vec2::new(start.x, start.z), 3);
        assert!(!squirrels.squirrels.is_empty(), "no oak in this world?");
        let mut events = Vec::new();
        let mut sown = Vec::new();
        let mut most_caches = 0;
        let mut dug = 0;
        // A year, a step of a game minute (a fifth of the time, speeded: enough to see it).
        let dt = crate::clock::DAY_SECONDS / 24.0 / 60.0;
        let steps = (crate::season::YEAR_DAYS * 24.0 * 60.0) as usize;
        for k in 0..steps {
            let days = k as f64 / (24.0 * 60.0);
            let now = Conditions {
                hour: ((k / 60) % 24) as f32,
                days,
                moon: 0.25,
                rain: 0.0,
                wind: Vec2::X,
                year: crate::season::year(days),
            };
            events.clear();
            squirrels.update(dt, &world, &now, &[], &mut events, &mut sown);
            dug += events
                .iter()
                .filter(|e| matches!(e, SquirrelEvent::DugUp { .. }))
                .count();
            most_caches = most_caches.max(squirrels.caches.len());
        }
        assert!(most_caches > 50, "{most_caches} caches");
        assert!(dug > 10, "{dug} dug up");
        assert!(
            !sown.is_empty() && sown.len() < most_caches,
            "{} sown",
            sown.len()
        );
    }
}
