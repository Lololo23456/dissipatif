//! Living plants: the ground cover and the bushes grow, spread their seeds and die, each
//! species after its own way, in a habitat made of water, light and soil. Nothing is
//! scripted: a meadow, an undergrowth, a bare patch slowly recolonised (succession) all come
//! from the same few rules, and from what the player does (picking, digging, fire).
//!
//! Model, per plant i of species S (a time step dt in game days):
//! - **habitat** H ∈ [0, 1] at its place: how well S suits the biome, the moisture (near
//!   water, the climate), the light (the shade of tree crowns) and the soil;
//! - **crowding** c = Σ over neighbours j within `CROWD_RADIUS` of size_j × (1 − d_ij / R);
//! - **carrying size** K = H / (1 + α c): what the place allows it, minus what the
//!   neighbours take (competition for water and light);
//! - **growth**, logistic: ds/dt = r s (1 − s / K). Where K drops below s (crowded, shaded,
//!   dry), the plant shrinks; below `DEATH_SIZE` it dies. It also dies of old age;
//! - **seeds**: a grown plant (s > `MATURE`) scatters seeds at a species rate, each landing
//!   at a distance drawn from its dispersal kernel; a seed germinates with probability H
//!   where nothing grows yet.
//!
//! Deterministic: every chance comes from a seeded generator.

use std::collections::HashMap;

use glam::Vec2;
use sim::rng::SplitMix64;
use world::{Biome, Material, Plant, PlantInstance, World};

/// Real seconds between two ecology steps, and real seconds in a game day (`clock`).
pub const STEP_SECONDS: f32 = 2.0;
const DAY_SECONDS: f32 = 20.0 * 60.0;
/// Neighbours closer than this compete (cells), and how strongly.
const CROWD_RADIUS: f32 = 0.9;
const CROWDING: f32 = 1.2;
/// Size below which a plant dies, above which it seeds; size of a seedling.
const DEATH_SIZE: f32 = 0.08;
const MATURE: f32 = 0.6;
const SEEDLING: f32 = 0.12;
/// A seed does not germinate closer than this to another plant (cells).
const ROOM: f32 = 0.32;
/// Living plants at most, as a multiple of the initial ones (keeps the cost bounded).
const MAX_GROWTH: f32 = 2.0;

/// How a species lives.
#[derive(Clone, Copy, Debug)]
pub struct Species {
    /// Logistic growth rate (per game day).
    pub growth: f32,
    /// Typical lifespan (game days).
    pub lifespan: f32,
    /// Seeds scattered per game day by a grown plant.
    pub seeds: f32,
    /// How far seeds go (cells, the edge of the kernel).
    pub dispersal: f32,
    /// Best moisture, and how far from it the species still copes.
    pub moisture: f32,
    pub tolerance: f32,
    /// Light: 1 loves full sun (shade hurts), −1 needs shade, 0 indifferent.
    pub sun: f32,
}

/// The species that live and spread (stones do not; trees are left out for now).
pub fn species(plant: Plant) -> Option<Species> {
    let s = |growth, lifespan, seeds, dispersal, moisture, tolerance, sun| Species {
        growth,
        lifespan,
        seeds,
        dispersal,
        moisture,
        tolerance,
        sun,
    };
    Some(match plant {
        // Grass: fast, many seeds near by, sun-loving, copes with most moisture: the pioneer.
        Plant::Grass => s(3.0, 20.0, 1.6, 1.6, 0.5, 0.45, 0.8),
        // Wild flowers: shorter lives, seeds carried farther.
        Plant::Flower => s(2.0, 6.0, 1.0, 3.5, 0.5, 0.3, 0.9),
        // Ferns: slow, spores carried far, moist shade.
        Plant::Fern => s(1.0, 30.0, 0.4, 6.0, 0.75, 0.25, -0.6),
        // Mushrooms: fruiting bodies of a hidden mycelium, brief, in damp shade only.
        Plant::Mushroom => s(6.0, 2.0, 1.5, 2.0, 0.8, 0.2, -1.0),
        // Dry shrubs: slow and hardy, the drylands.
        Plant::DryShrub => s(0.6, 25.0, 0.2, 2.5, 0.15, 0.25, 0.6),
        // Bushes: slow, few seeds, long-lived.
        Plant::Bush => s(0.4, 60.0, 0.08, 3.0, 0.6, 0.3, 0.3),
        _ => return None,
    })
}

/// What a column offers: moisture and shade, both in [0, 1].
pub struct Habitat {
    nx: usize,
    nz: usize,
    moisture: Vec<f32>,
    shade: Vec<f32>,
}

impl Habitat {
    pub fn new(world: &World) -> Self {
        let d = world.dims();
        let (nx, nz) = (d.nx, d.nz);
        // Distance to water (breadth-first over columns, capped).
        let mut distance = vec![u32::MAX; nx * nz];
        let mut queue = std::collections::VecDeque::new();
        for z in 0..nz {
            for x in 0..nx {
                if world.water_level(x, z).is_some() {
                    distance[x + nx * z] = 0;
                    queue.push_back((x, z));
                }
            }
        }
        while let Some((x, z)) = queue.pop_front() {
            let next = distance[x + nx * z] + 1;
            if next > 10 {
                continue;
            }
            for (a, b) in [
                (x + 1, z),
                (x.wrapping_sub(1), z),
                (x, z + 1),
                (x, z.wrapping_sub(1)),
            ] {
                if a < nx && b < nz && distance[a + nx * b] > next {
                    distance[a + nx * b] = next;
                    queue.push_back((a, b));
                }
            }
        }
        let mut moisture = vec![0.0; nx * nz];
        let mut shade = vec![0.0; nx * nz];
        for z in 0..nz {
            for x in 0..nx {
                let i = x + nx * z;
                let climate = match world.biome(x, z) {
                    Biome::Desert => 0.08,
                    Biome::Savanna => 0.25,
                    Biome::Beach => 0.35,
                    Biome::Plains => 0.5,
                    Biome::Forest => 0.7,
                    Biome::Taiga => 0.6,
                    Biome::Mountain => 0.4,
                    Biome::SnowyPeak => 0.3,
                    Biome::Ocean => 1.0,
                };
                let near = (1.0 - distance[i].min(10) as f32 / 8.0).max(0.0);
                moisture[i] = (climate + 0.45 * near).min(1.0);
                let top = world.ground_top(x, z);
                let crown = (top + 3..(top + 18).min(d.ny)).any(|y| {
                    let m = world.block(x, y, z);
                    m.is_canopy() || m == Material::PineNeedles
                });
                shade[i] = if crown { 0.75 } else { 0.0 };
            }
        }
        Self {
            nx,
            nz,
            moisture,
            shade,
        }
    }

    /// How well `plant` suits the place (x, z), in [0, 1].
    pub fn suitability(&self, world: &World, plant: Plant, x: f32, z: f32) -> f32 {
        let Some(sp) = species(plant) else {
            return 0.0;
        };
        if x < 0.0 || z < 0.0 || x >= self.nx as f32 || z >= self.nz as f32 {
            return 0.0;
        }
        let (cx, cz) = (x as usize, z as usize);
        if world.water_level(cx, cz).is_some() {
            return 0.0;
        }
        let i = cx + self.nx * cz;
        let top = world.ground_top(cx, cz);
        if top == 0 {
            return 0.0;
        }
        // Soil: plants root in soil; sand only suits the hardy ones.
        let soil = match world.block(cx, top - 1, cz) {
            Material::Grass | Material::ForestFloor | Material::Dirt => 1.0,
            Material::DryGrass | Material::Clay => 0.8,
            Material::Sand | Material::DesertSand => match plant {
                Plant::DryShrub => 1.0,
                Plant::Grass => 0.3,
                _ => 0.05,
            },
            _ => 0.0,
        };
        let biome = world.biome(cx, cz);
        let native = biome
            .ground_cover()
            .iter()
            .chain(biome.plants())
            .any(|&(p, _)| p == plant);
        let affinity = if native { 1.0 } else { 0.3 };
        let off = (self.moisture[i] - sp.moisture).abs() / sp.tolerance;
        let wet = (-off * off).exp();
        let shade = self.shade[i];
        let light = if sp.sun >= 0.0 {
            1.0 - sp.sun * 0.8 * shade
        } else {
            (1.0 + sp.sun) + (-sp.sun) * shade
        };
        (soil * affinity * wet * light).clamp(0.0, 1.0)
    }
}

/// Life of one plant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Life {
    /// Size relative to a grown plant, in (0, 1].
    pub size: f32,
    /// Age, game days.
    pub age: f32,
    /// Its own lifespan (the species' one, varied).
    pub lifespan: f32,
}

/// What happened to the plants in a step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    /// A new plant (index in the plant list).
    Sprouted(usize),
    /// A plant's size changed noticeably.
    Resized(usize),
    Died(usize),
}

pub struct Ecology {
    habitat: Habitat,
    /// Life of each plant (index as in the plant list); `None` for what does not live
    /// (stones) or is gone.
    life: Vec<Option<Life>>,
    /// Size last reported for each plant (a `Resized` is sent when it moves enough).
    shown: Vec<f32>,
    /// Living plants by column.
    grid: HashMap<(i64, i64), Vec<usize>>,
    rng: SplitMix64,
    /// Real seconds until the next step.
    timer: f32,
    max_plants: usize,
}

fn column(p: Vec2) -> (i64, i64) {
    (p.x.floor() as i64, p.y.floor() as i64)
}

fn place(p: &PlantInstance) -> Vec2 {
    Vec2::new(
        p.base[0] as f32 + 0.5 + p.offset[0],
        p.base[2] as f32 + 0.5 + p.offset[1],
    )
}

impl Ecology {
    pub fn new(world: &World, plants: &[PlantInstance], seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let mut life = Vec::with_capacity(plants.len());
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, p) in plants.iter().enumerate() {
            let entry = species(p.plant).map(|sp| {
                let lifespan = sp.lifespan * (0.7 + 0.6 * rng.next_f32());
                Life {
                    size: 0.6 + 0.4 * rng.next_f32(),
                    age: lifespan * rng.next_f32() * 0.8,
                    lifespan,
                }
            });
            if entry.is_some() {
                grid.entry(column(place(p))).or_default().push(i);
            }
            life.push(entry);
        }
        let living = life.iter().filter(|l| l.is_some()).count();
        let shown = life.iter().map(|l| l.map_or(1.0, |l| l.size)).collect();
        Self {
            habitat: Habitat::new(world),
            life,
            shown,
            grid,
            rng,
            timer: STEP_SECONDS,
            max_plants: (living as f32 * MAX_GROWTH) as usize,
        }
    }

    #[cfg(test)]
    pub fn habitat(&self) -> &Habitat {
        &self.habitat
    }

    /// Size of plant `i` (1 for what does not grow).
    pub fn size(&self, i: usize) -> f32 {
        self.life.get(i).copied().flatten().map_or(1.0, |l| l.size)
    }

    pub fn living(&self) -> usize {
        self.life.iter().filter(|l| l.is_some()).count()
    }

    /// A plant is gone (taken, burnt, dug up): it stops living.
    pub fn remove(&mut self, i: usize, plants: &[PlantInstance]) {
        if let Some(slot) = self.life.get_mut(i)
            && slot.take().is_some()
            && let Some(list) = self.grid.get_mut(&column(place(&plants[i])))
        {
            list.retain(|&j| j != i);
        }
    }

    /// Living plants within `radius` of `at`.
    pub fn near(&self, at: Vec2, radius: f32, plants: &[PlantInstance]) -> Vec<usize> {
        let (cx, cz) = column(at);
        let r = radius.ceil() as i64;
        let mut found = Vec::new();
        for z in cz - r..=cz + r {
            for x in cx - r..=cx + r {
                for &j in self.grid.get(&(x, z)).into_iter().flatten() {
                    if place(&plants[j]).distance(at) <= radius {
                        found.push(j);
                    }
                }
            }
        }
        found
    }

    /// Advances by `dt` real seconds (`speed` × faster when the day is fast-forwarded). New
    /// plants are pushed onto `plants`; `free` says whether a seed may land at a point
    /// (nothing laid there). Changes go onto `changes`.
    pub fn update(
        &mut self,
        dt: f32,
        world: &World,
        plants: &mut Vec<PlantInstance>,
        free: impl Fn(Vec2) -> bool,
        changes: &mut Vec<Change>,
    ) {
        self.timer -= dt;
        while self.timer <= 0.0 {
            self.timer += STEP_SECONDS;
            self.step(STEP_SECONDS / DAY_SECONDS, world, plants, &free, changes);
        }
    }

    /// One step of `days` game days.
    pub fn step(
        &mut self,
        days: f32,
        world: &World,
        plants: &mut Vec<PlantInstance>,
        free: &impl Fn(Vec2) -> bool,
        changes: &mut Vec<Change>,
    ) {
        let count = self.life.len();
        let mut seeds: Vec<(Plant, Vec2)> = Vec::new();
        for i in 0..count {
            let Some(mut l) = self.life[i] else {
                continue;
            };
            let p = plants[i];
            let Some(sp) = species(p.plant) else {
                continue;
            };
            let at = place(&p);
            let h = self.habitat.suitability(world, p.plant, at.x, at.y);
            let mut crowd = 0.0;
            for j in self.near(at, CROWD_RADIUS, plants) {
                if j != i {
                    let d = place(&plants[j]).distance(at);
                    crowd += self.size(j) * (1.0 - d / CROWD_RADIUS);
                }
            }
            let carrying = h / (1.0 + CROWDING * crowd);
            l.size += sp.growth * l.size * (1.0 - l.size / carrying.max(1e-3)) * days;
            l.size = l.size.min(1.0);
            l.age += days;
            if l.size < DEATH_SIZE || l.age > l.lifespan {
                self.remove(i, plants);
                changes.push(Change::Died(i));
                continue;
            }
            self.life[i] = Some(l);
            if (l.size - self.shown[i]).abs() > 0.08 {
                self.shown[i] = l.size;
                changes.push(Change::Resized(i));
            }
            // Seeds of a grown plant: a Poisson trial per step.
            if l.size > MATURE && self.rng.next_f32() < sp.seeds * l.size * days {
                let angle = std::f32::consts::TAU * self.rng.next_f32();
                let distance = sp.dispersal * self.rng.next_f32().sqrt();
                let target = at + Vec2::new(angle.cos(), angle.sin()) * distance;
                seeds.push((p.plant, target));
            }
        }
        for (plant, target) in seeds {
            if self.living() >= self.max_plants {
                break;
            }
            let h = self.habitat.suitability(world, plant, target.x, target.y);
            if self.rng.next_f32() >= h || !free(target) {
                continue;
            }
            if !self.near(target, ROOM, plants).is_empty() {
                continue;
            }
            let (x, z) = (target.x.floor() as usize, target.y.floor() as usize);
            let variant = (self.rng.next_f32() * world::plants::VARIANTS as f32) as u32;
            let new = PlantInstance {
                plant,
                variant: variant.min(world::plants::VARIANTS - 1),
                rotation: (self.rng.next_f32() * 4.0) as u32 % 4,
                mirrored: self.rng.next_f32() < 0.5,
                scale: 0.85 + 0.3 * self.rng.next_f32(),
                base: [x, world.ground_top(x, z), z],
                offset: [target.x - x as f32 - 0.5, target.y - z as f32 - 0.5],
            };
            let lifespan =
                species(plant).map_or(10.0, |sp| sp.lifespan) * (0.7 + 0.6 * self.rng.next_f32());
            let index = plants.len();
            plants.push(new);
            self.life.push(Some(Life {
                size: SEEDLING,
                age: 0.0,
                lifespan,
            }));
            self.shown.push(SEEDLING);
            self.grid.entry(column(target)).or_default().push(index);
            changes.push(Change::Sprouted(index));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    fn setup() -> (World, Vec<PlantInstance>, Ecology) {
        let world = World::generate(WorldConfig::standard(6));
        let plants = world.plants().to_vec();
        let ecology = Ecology::new(&world, &plants, 1);
        (world, plants, ecology)
    }

    #[test]
    fn grass_suits_a_meadow_mushrooms_need_damp_shade_nothing_grows_in_water() {
        let (world, _, eco) = setup();
        let h = eco.habitat();
        let d = world.dims();
        let mut meadow = None;
        let mut shaded = None;
        for z in (2..d.nz - 2).step_by(3) {
            for x in (2..d.nx - 2).step_by(3) {
                let (fx, fz) = (x as f32 + 0.5, z as f32 + 0.5);
                if world.biome(x, z) == Biome::Plains && h.shade[x + h.nx * z] == 0.0 {
                    meadow.get_or_insert((fx, fz));
                }
                let soil = world.block(x, world.ground_top(x, z).saturating_sub(1), z);
                if h.shade[x + h.nx * z] > 0.5
                    && h.moisture[x + h.nx * z] > 0.6
                    && soil == Material::ForestFloor
                    && world.water_level(x, z).is_none()
                {
                    shaded.get_or_insert((fx, fz));
                }
            }
        }
        let (mx, mz) = meadow.expect("no meadow");
        assert!(h.suitability(&world, Plant::Grass, mx, mz) > 0.5);
        assert!(h.suitability(&world, Plant::Mushroom, mx, mz) < 0.1);
        if let Some((sx, sz)) = shaded {
            assert!(
                h.suitability(&world, Plant::Mushroom, sx, sz)
                    > h.suitability(&world, Plant::Mushroom, mx, mz)
            );
        }
    }

    /// Plants grow, spread and die; the population stays bounded and the run is the same
    /// every time.
    #[test]
    fn a_meadow_lives_stays_bounded_and_is_deterministic() {
        let run = || {
            let (world, mut plants, mut eco) = setup();
            let start = eco.living();
            let mut changes = Vec::new();
            for _ in 0..600 * 3 {
                eco.step(1.0 / 600.0, &world, &mut plants, &|_| true, &mut changes);
            }
            let sprouted = changes
                .iter()
                .filter(|c| matches!(c, Change::Sprouted(_)))
                .count();
            let died = changes
                .iter()
                .filter(|c| matches!(c, Change::Died(_)))
                .count();
            (start, eco.living(), sprouted, died)
        };
        let (start, end, sprouted, died) = run();
        assert!(
            sprouted > 100 && died > 100,
            "sprouted {sprouted}, died {died}"
        );
        assert!(
            end > start / 2 && end <= (start as f32 * MAX_GROWTH) as usize + 1,
            "{start} → {end}"
        );
        assert_eq!(run(), (start, end, sprouted, died));
    }

    /// A patch cleared of every plant is recolonised, grass first.
    #[test]
    fn a_cleared_patch_is_recolonised_by_grass_first() {
        let (world, mut plants, mut eco) = setup();
        // The densest grassy place: clear a 4-cell radius around it.
        let centre = (0..plants.len())
            .filter(|&i| plants[i].plant == Plant::Grass)
            .map(|i| place(&plants[i]))
            .max_by_key(|&p| eco.near(p, 3.0, &plants).len())
            .unwrap();
        for i in eco.near(centre, 4.0, &plants) {
            eco.remove(i, &plants);
        }
        assert!(eco.near(centre, 4.0, &plants).is_empty());
        let mut changes = Vec::new();
        for _ in 0..600 * 2 {
            eco.step(1.0 / 600.0, &world, &mut plants, &|_| true, &mut changes);
        }
        let back = eco.near(centre, 4.0, &plants);
        assert!(back.len() > 5, "only {} plants came back", back.len());
        let grass = back
            .iter()
            .filter(|&&i| plants[i].plant == Plant::Grass)
            .count();
        assert!(
            grass * 2 > back.len(),
            "{grass} grass out of {}",
            back.len()
        );
    }
}
