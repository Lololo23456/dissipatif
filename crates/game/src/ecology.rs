//! Living plants: herbs, shrubs and trees grow, compete, spread their seeds, burn and die,
//! each species after its own way, in a habitat made of water, light and soil. Nothing is
//! scripted: a meadow, an undergrowth, a clearing slowly closing again (succession), a grass
//! fire running before the wind all come from the same rules, and from what the player does.
//!
//! **Habitat** H ∈ [0, 1] of species S at a place: how well S suits the biome, the moisture
//! (the climate, the distance to water), the light and the soil. Light is not fixed: it is
//! the shade of the living trees' crowns, so a tree that grows darkens the ground under it,
//! and one that dies lets the light back in.
//!
//! **Competition** (Lotka-Volterra for several species): for plant i,
//! ```text
//! ds_i/dt = r_S s_i (1 − (s_i + Σ_j α_ij w_ij s_j) / H_i)
//! ```
//! where w_ij = 1 − d_ij / R_i weighs the neighbours within the competition radius R_i of i's
//! layer (herb, shrub, tree), and α_ij how much j takes of what i needs: 1 within a species,
//! less between species (another niche: other roots, another season), 0 for a tree on the
//! herbs (trees act on them by their shade). Since every species hinders its own kind more
//! than the others (α_ij < 1), several species coexist instead of one taking all.
//!
//! **Life cycle**: below `DEATH_SIZE` a plant dies (crowded, shaded, too dry: self-thinning),
//! and of old age. A grown plant (s > `MATURE`) scatters seeds at its species' rate, landing
//! at a distance drawn from its dispersal kernel; a seed germinates with probability H where
//! its layer has room.
//!
//! **Soil** (`soil.rs`): the slow variables, water and organic matter by patches of 8 × 8
//! cells, follow the plant cover and decide what the plants can reach: H is multiplied by
//! what the soil offers. Past a threshold a patch tips into a bare state and stays there.
//!
//! **Grazing**: grazers (deer) eat the palatable plants around them with a saturating intake
//! (Holling type II), herbs and saplings first: they keep a meadow open, and too many of them
//! strip it bare.
//!
//! **Fire**: a burning plant ignites its neighbours with a probability per second that grows
//! with their flammability and their dryness, and with the wind behind it; rain damps it.
//! A plant that burnt out dies. Fire is updated every frame, life every `STEP_SECONDS`.
//!
//! Deterministic: every chance comes from a seeded generator.

use std::collections::HashMap;

use glam::Vec2;
use sim::rng::SplitMix64;

use crate::soil::Soil;
use world::{Biome, Material, Plant, PlantInstance, World};

/// Real seconds between two life steps, and real seconds in a game day (`clock`).
pub const STEP_SECONDS: f32 = 2.0;
const DAY_SECONDS: f32 = 20.0 * 60.0;
/// Size below which a plant dies, above which it seeds; size of a seedling.
const DEATH_SIZE: f32 = 0.08;
const MATURE: f32 = 0.6;
const SEEDLING: f32 = 0.12;
/// A sapling this big has a trunk and a crown solid in the world grid.
pub const TREE_STANDS: f32 = 0.85;
/// Living plants at most, as a multiple of the initial ones (keeps the cost bounded).
const MAX_GROWTH: f32 = 2.0;
/// Weighted plant matter of a fully covered soil patch (herbs count 1, shrubs 3, trees 10,
/// times their size): about a meadow or a wood of the generated world.
const COVER_FULL: f32 = 50.0;
/// Grazing: most a grazer eats in a game day of grazing (biomass: a grown grass tuft is 1),
/// and the forage within reach at which it eats half of that.
const INTAKE: f32 = 30.0;
const FORAGE_HALF: f32 = 1.0;
/// Reach of a grazer's muzzle as it steps along, in cells.
const GRAZE_REACH: f32 = 1.5;
/// Seeds waiting in the soil for spring, at most.
const SEED_BANK: usize = 4000;
/// Dormant herbs and shrubs (in the cold months) keep this much when grazed: their crown and
/// roots rest underground. In the growing season, grazing them down exhausts them and can
/// kill them; saplings browsed can die of it at any time.
const GRAZE_FLOOR: f32 = 0.12;

/// Storey of a plant: who competes with whom, and over what distance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Herb,
    Shrub,
    Tree,
}

impl Layer {
    /// Distance over which plants of this layer compete (cells).
    fn reach(self) -> f32 {
        match self {
            Layer::Herb => 0.9,
            Layer::Shrub => 1.5,
            Layer::Tree => 3.0,
        }
    }

    /// No seed of this layer germinates closer than this to a plant of the same layer.
    fn room(self) -> f32 {
        match self {
            Layer::Herb => 0.32,
            Layer::Shrub => 0.7,
            Layer::Tree => 1.6,
        }
    }

    /// How far fire jumps from a burning plant of this layer (cells): a tuft stands for a
    /// patch of meadow, so fire reaches the next tuft through the grass between them.
    fn spread(self) -> f32 {
        match self {
            Layer::Herb => 1.7,
            Layer::Shrub => 2.2,
            Layer::Tree => 3.2,
        }
    }
}

/// How a species lives.
#[derive(Clone, Copy, Debug)]
pub struct Species {
    pub layer: Layer,
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
    /// Light: 1 needs full sun (shade hurts), −1 needs shade, 0 indifferent.
    pub sun: f32,
    /// How readily it burns when dry, in [0, 1], and for how long (real seconds).
    pub flammability: f32,
    pub burn_seconds: f32,
    /// Radius of a grown crown (cells): the shade it casts. 0 for what casts none.
    pub crown: f32,
}

/// The species that live (stones and dead trees do not).
pub fn species(plant: Plant) -> Option<Species> {
    use Layer::*;
    // (layer, growth, lifespan, seeds, dispersal, moisture, tolerance, sun, flammability,
    // burn seconds, crown)
    let s = |v: (Layer, f32, f32, f32, f32, f32, f32, f32, f32, f32, f32)| Species {
        layer: v.0,
        growth: v.1,
        lifespan: v.2,
        seeds: v.3,
        dispersal: v.4,
        moisture: v.5,
        tolerance: v.6,
        sun: v.7,
        flammability: v.8,
        burn_seconds: v.9,
        crown: v.10,
    };
    Some(s(match plant {
        // Grass: fast, many seeds near by, sun-loving: the pioneer, perennial (a tuft lives a
        // couple of years). Burns fast when dry.
        Plant::Grass => (Herb, 3.0, 120.0, 1.6, 1.6, 0.5, 0.45, 0.8, 0.9, 5.0, 0.0),
        // Wild flowers: shorter lives (about a year), seeds carried farther; the seed bank
        // brings them back each spring.
        Plant::Flower => (Herb, 2.0, 50.0, 1.4, 3.5, 0.5, 0.35, 0.9, 0.5, 4.0, 0.0),
        // Ferns: slow, long-lived, spores carried far, moist shade.
        Plant::Fern => (Herb, 1.0, 200.0, 0.4, 6.0, 0.75, 0.25, -0.6, 0.3, 6.0, 0.0),
        // Mushrooms: fruiting bodies of a hidden mycelium, brief, in damp shade only.
        Plant::Mushroom => (Herb, 6.0, 2.0, 1.5, 2.0, 0.8, 0.2, -1.0, 0.0, 2.0, 0.0),
        // Dry shrubs: slow and hardy, the drylands; tinder.
        Plant::DryShrub => (Shrub, 0.6, 150.0, 0.2, 2.5, 0.15, 0.25, 0.6, 1.0, 10.0, 0.0),
        // Bushes: slow, few seeds, long-lived.
        Plant::Bush => (Shrub, 0.4, 300.0, 0.08, 3.0, 0.6, 0.3, 0.3, 0.6, 18.0, 0.8),
        // Trees: slow growth, long lives, a few seeds carried more or less far. Pioneers
        // (birch, pine) need light; the broadleaf grows up in shade.
        Plant::Broadleaf => (
            Tree, 0.05, 300.0, 0.05, 5.0, 0.65, 0.3, -0.1, 0.4, 60.0, 2.6,
        ),
        Plant::Birch => (Tree, 0.09, 120.0, 0.08, 9.0, 0.6, 0.3, 0.9, 0.5, 45.0, 1.6),
        Plant::Pine => (Tree, 0.06, 250.0, 0.06, 7.0, 0.5, 0.3, 0.6, 0.8, 60.0, 2.4),
        Plant::Willow => (Tree, 0.08, 150.0, 0.05, 4.0, 0.9, 0.2, 0.5, 0.3, 50.0, 2.8),
        Plant::Acacia => (Tree, 0.05, 200.0, 0.04, 6.0, 0.25, 0.2, 0.9, 0.6, 50.0, 3.0),
        Plant::Palm => (
            Tree, 0.06, 150.0, 0.04, 4.0, 0.45, 0.25, 0.9, 0.5, 40.0, 2.2,
        ),
        Plant::Cactus => (
            Shrub, 0.08, 200.0, 0.03, 3.0, 0.05, 0.15, 1.0, 0.0, 1.0, 0.0,
        ),
        _ => return None,
    }))
}

/// When a layer scatters its seeds (a factor on its yearly rate, averaging about 1): herbs
/// through summer, shrubs and trees in autumn.
fn seeding(layer: Layer, year: f32) -> f32 {
    use crate::season::Season::*;
    match (layer, crate::season::season(year)) {
        (Layer::Herb, Summer) => 2.5,
        (Layer::Herb, Spring | Autumn) => 0.75,
        (Layer::Shrub | Layer::Tree, Autumn) => 3.0,
        (Layer::Shrub | Layer::Tree, Summer) => 1.0,
        _ => 0.0,
    }
}

/// How readily grazers eat a plant of `size`: grass and flowers, and tree saplings (browsed:
/// this is what keeps a meadow open); ferns are bitter, grown trees out of reach.
/// In winter, with little grass, deer browse: brambles, bushes, twigs and bark.
fn palatability(plant: Plant, size: f32, year: f32) -> f32 {
    let winter = crate::season::season(year) == crate::season::Season::Winter;
    match plant {
        Plant::Grass => 1.0,
        Plant::Flower => 0.8,
        Plant::Mushroom => 0.3,
        Plant::Fern => 0.05,
        Plant::DryShrub if winter => 0.6,
        Plant::DryShrub => 0.2,
        Plant::Bush if winter => 1.2,
        Plant::Bush => 0.5,
        Plant::Cactus => 0.0,
        p if is_tree(p) => {
            if size < 0.5 {
                1.3
            } else if winter {
                // Twigs and bark of grown trees, in want of better.
                0.08
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

/// How much species `a` is held back by a plant of species `b` growing near it.
fn competition(a: Plant, b: Plant) -> f32 {
    if a == b {
        return 1.0;
    }
    let (Some(sa), Some(sb)) = (species(a), species(b)) else {
        return 0.0;
    };
    match (sa.layer, sb.layer) {
        (Layer::Herb, Layer::Herb) => 0.4,
        (Layer::Herb, Layer::Shrub) | (Layer::Shrub, Layer::Herb) => 0.25,
        (Layer::Shrub, Layer::Shrub) => 0.5,
        (Layer::Tree, Layer::Tree) => 0.7,
        // Trees act on what grows under them by their shade (see `Habitat`).
        (Layer::Herb | Layer::Shrub, Layer::Tree) => 0.0,
        // Saplings struggle in the grass.
        (Layer::Tree, Layer::Herb | Layer::Shrub) => 0.15,
    }
}

/// What a column offers: moisture (fixed) and shade (from the living trees), in [0, 1].
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
            }
        }
        Self {
            nx,
            nz,
            moisture,
            shade: vec![0.0; nx * nz],
        }
    }

    fn column(&self, x: f32, z: f32) -> Option<usize> {
        (x >= 0.0 && z >= 0.0 && x < self.nx as f32 && z < self.nz as f32)
            .then(|| x as usize + self.nx * z as usize)
    }

    pub fn moisture(&self, x: f32, z: f32) -> f32 {
        self.column(x, z).map_or(0.0, |i| self.moisture[i])
    }

    #[cfg(test)]
    pub fn shade(&self, x: f32, z: f32) -> f32 {
        self.column(x, z).map_or(0.0, |i| self.shade[i])
    }

    /// Recomputes the shade from the crowns: a crown of radius R and relative size s darkens
    /// the columns under it by 0.8 s (1 − d / R), added up, at most 0.9.
    fn cast_shade(&mut self, crowns: &[(Vec2, f32, f32)]) {
        self.shade.iter_mut().for_each(|s| *s = 0.0);
        for &(at, radius, size) in crowns {
            let r = radius.ceil() as i64;
            let (cx, cz) = (at.x.floor() as i64, at.y.floor() as i64);
            for z in cz - r..=cz + r {
                for x in cx - r..=cx + r {
                    if x < 0 || z < 0 || x as usize >= self.nx || z as usize >= self.nz {
                        continue;
                    }
                    let d = Vec2::new(x as f32 + 0.5, z as f32 + 0.5).distance(at);
                    if d < radius {
                        let i = x as usize + self.nx * z as usize;
                        self.shade[i] = (self.shade[i] + 0.8 * size * (1.0 - d / radius)).min(0.9);
                    }
                }
            }
        }
    }

    /// How well `plant` suits the place (x, z), in [0, 1]. `own_shade`: the shade the plant
    /// itself casts there (a tree is not shaded by its own crown).
    pub fn suitability(&self, world: &World, plant: Plant, x: f32, z: f32, own_shade: f32) -> f32 {
        let Some(sp) = species(plant) else {
            return 0.0;
        };
        let Some(i) = self.column(x, z) else {
            return 0.0;
        };
        let (cx, cz) = (x as usize, z as usize);
        if world.water_level(cx, cz).is_some() {
            return 0.0;
        }
        let top = world.ground_top(cx, cz);
        if top == 0 {
            return 0.0;
        }
        // Soil: plants root in soil; sand only suits the hardy ones.
        let soil = match world.block(cx, top - 1, cz) {
            Material::Grass | Material::ForestFloor | Material::Dirt => 1.0,
            Material::DryGrass | Material::Clay => 0.8,
            Material::Sand | Material::DesertSand => match plant {
                Plant::DryShrub | Plant::Cactus | Plant::Palm => 1.0,
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
        let shade = (self.shade[i] - own_shade).max(0.0);
        let light = if sp.sun >= 0.0 {
            1.0 - sp.sun * 0.9 * shade
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
    /// Real seconds of burning left (0: not burning).
    pub burning: f32,
}

/// What happened to the plants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    /// A new plant (index in the plant list).
    Sprouted(usize),
    /// A plant's size changed noticeably.
    Resized(usize),
    /// A sapling grew into a tree: its trunk and crown now stand in the world grid.
    Stood(usize),
    /// A plant caught fire.
    Ignited(usize),
    /// A plant died (of age, of want, or burnt out). If it `stood` in the world grid, it
    /// must be erased from it.
    Died { plant: usize, stood: bool },
}

pub struct Ecology {
    habitat: Habitat,
    /// Life of each plant (index as in the plant list); `None` for what does not live
    /// (stones, dead trees) or is gone.
    life: Vec<Option<Life>>,
    /// Size last reported for each plant (a `Resized` is sent when it moves enough).
    shown: Vec<f32>,
    /// Whether each plant stands in the world grid (the initial trees, and saplings grown).
    stood: Vec<bool>,
    /// Living plants by column.
    grid: HashMap<(i64, i64), Vec<usize>>,
    /// Plants burning now.
    burning: Vec<usize>,
    rng: SplitMix64,
    /// Real seconds until the next life step.
    timer: f32,
    max_plants: usize,
    soil: Soil,
    /// Where grazers are grazing now (set before each update), and what each has eaten since
    /// it was last read.
    grazers: Vec<Vec2>,
    eaten: Vec<f32>,
    /// Phase of the year (see `season.rs`): growth, seeds and herbs follow the seasons.
    year: f32,
    /// The seed bank: seeds fallen out of season, lying in the soil until spring.
    bank: Vec<(Plant, Vec2)>,
}

fn column(p: Vec2) -> (i64, i64) {
    (p.x.floor() as i64, p.y.floor() as i64)
}

pub fn place(p: &PlantInstance) -> Vec2 {
    Vec2::new(
        p.base[0] as f32 + 0.5 + p.offset[0],
        p.base[2] as f32 + 0.5 + p.offset[1],
    )
}

pub fn is_tree(plant: Plant) -> bool {
    species(plant).is_some_and(|s| s.layer == Layer::Tree)
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
                    size: if sp.layer == Layer::Tree {
                        0.8 + 0.2 * rng.next_f32()
                    } else {
                        0.6 + 0.4 * rng.next_f32()
                    },
                    age: lifespan * rng.next_f32() * 0.8,
                    lifespan,
                    burning: 0.0,
                }
            });
            if entry.is_some() {
                grid.entry(column(place(p))).or_default().push(i);
            }
            life.push(entry);
        }
        let living = life.iter().filter(|l| l.is_some()).count();
        let shown = life.iter().map(|l| l.map_or(1.0, |l| l.size)).collect();
        let stood = plants.iter().map(|p| !p.plant.is_ground_cover()).collect();
        let habitat = Habitat::new(world);
        let d = world.dims();
        let soil = Soil::new(d.nx, d.nz, |x, z| {
            habitat.moisture(x as f32 + 0.5, z as f32 + 0.5)
        });
        let mut ecology = Self {
            habitat,
            soil,
            grazers: Vec::new(),
            eaten: Vec::new(),
            year: 0.375,
            bank: Vec::new(),
            life,
            shown,
            stood,
            grid,
            burning: Vec::new(),
            rng,
            timer: STEP_SECONDS,
            max_plants: (living as f32 * MAX_GROWTH) as usize,
        };
        ecology.cast_shade(plants);
        ecology.measure_cover(plants);
        ecology.soil.settle();
        ecology
    }

    /// Saves the plants' lives, the fires, the soil.
    pub fn save(&self, w: &mut crate::save::Writer) {
        w.put(&self.life);
        w.put(&self.shown);
        w.put(&self.stood);
        w.put(&self.burning);
        w.put(&self.rng);
        w.put(&self.timer);
        w.put(&self.max_plants);
        self.soil.save(w);
        w.put(&self.bank);
    }

    /// Reads back what `save` wrote, for `plants` in `world` (both restored first).
    pub fn load(
        r: &mut crate::save::Reader,
        world: &World,
        plants: &[PlantInstance],
    ) -> crate::save::Result<Self> {
        let life: Vec<Option<Life>> = r.get()?;
        if life.len() != plants.len() {
            return Err("plantes incohérentes".into());
        }
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, l) in life.iter().enumerate() {
            if l.is_some() {
                grid.entry(column(place(&plants[i]))).or_default().push(i);
            }
        }
        let habitat = Habitat::new(world);
        let d = world.dims();
        let soil = Soil::new(d.nx, d.nz, |x, z| {
            habitat.moisture(x as f32 + 0.5, z as f32 + 0.5)
        });
        let mut ecology = Self {
            habitat,
            life,
            shown: r.get()?,
            stood: r.get()?,
            grid,
            burning: r.get()?,
            rng: r.get()?,
            timer: r.get()?,
            max_plants: r.get()?,
            soil,
            grazers: Vec::new(),
            eaten: Vec::new(),
            year: 0.375,
            bank: Vec::new(),
        };
        ecology.soil.load(r)?;
        ecology.bank = r.get()?;
        ecology.cast_shade(plants);
        ecology.measure_cover(plants);
        Ok(ecology)
    }

    /// The soil, the slow variables.
    pub fn soil(&self) -> &Soil {
        &self.soil
    }

    /// The time of the year (phase in [0, 1), see `season.rs`).
    pub fn set_year(&mut self, year: f32) {
        self.year = year;
    }

    /// Where grazers graze now: they eat at the next life steps.
    pub fn set_grazers(&mut self, at: &[Vec2]) {
        self.grazers.clear();
        self.grazers.extend_from_slice(at);
        self.eaten.resize(at.len(), 0.0);
    }

    /// What each grazer ate since the last call (biomass), in the order they were set.
    pub fn take_eaten(&mut self) -> Vec<f32> {
        std::mem::replace(&mut self.eaten, vec![0.0; self.grazers.len()])
    }

    /// Plant cover and forage of each soil patch, from the living plants.
    fn measure_cover(&mut self, plants: &[PlantInstance]) {
        let n = self.soil.patches();
        let (mut cover, mut forage) = (vec![0.0; n], vec![0.0; n]);
        for (i, l) in self.life.iter().enumerate() {
            let (Some(l), Some(sp)) = (l, species(plants[i].plant)) else {
                continue;
            };
            let Some(k) = self.soil.patch(place(&plants[i])) else {
                continue;
            };
            let weight = match sp.layer {
                Layer::Herb => 1.0,
                Layer::Shrub => 3.0,
                Layer::Tree => 10.0,
            };
            // A herb dormant in winter still holds the soil (roots, litter): the cover counts
            // it whole. Grazers only find what is above ground.
            let held = if sp.layer == Layer::Herb {
                l.size / crate::season::herb_cover(self.year)
            } else {
                l.size
            };
            cover[k] += weight * held.min(1.0) / COVER_FULL;
            forage[k] += palatability(plants[i].plant, l.size, self.year) * l.size;
        }
        self.soil.set_cover(&cover, &forage);
    }

    /// Grazer `g` at `at` eats for `days` of grazing: as much as it finds, up to its fill,
    /// from the palatable plants within reach, in proportion to what each offers. A plant
    /// grazed below the living size dies.
    fn graze(
        &mut self,
        g: usize,
        at: Vec2,
        days: f32,
        plants: &[PlantInstance],
        changes: &mut Vec<Change>,
    ) {
        let dormant = crate::season::herb_cover(self.year) < 0.95;
        let mut offers = Vec::new();
        self.for_near(at, GRAZE_REACH, plants, |j| {
            let size = self.size(j);
            let edible = if dormant && !is_tree(plants[j].plant) {
                (size - GRAZE_FLOOR).max(0.0)
            } else {
                size
            };
            let offer = palatability(plants[j].plant, size, self.year) * edible;
            if offer > 0.0 {
                offers.push((j, offer));
            }
        });
        let available: f32 = offers.iter().map(|&(_, o)| o).sum();
        if available <= 0.0 {
            return;
        }
        // Holling type II: the intake saturates when forage abounds.
        let want = INTAKE * available / (available + FORAGE_HALF) * days;
        let mut eaten = 0.0;
        for (j, offer) in offers {
            let Some(mut l) = self.life[j] else {
                continue;
            };
            let bite = (want * offer / available).min(l.size);
            let bite = if dormant && !is_tree(plants[j].plant) {
                bite.min((l.size - GRAZE_FLOOR).max(0.0))
            } else {
                bite
            };
            l.size -= bite;
            eaten += bite;
            if l.size < DEATH_SIZE {
                let stood = self.remove(j, plants);
                changes.push(Change::Died { plant: j, stood });
            } else {
                self.life[j] = Some(l);
            }
        }
        if let Some(e) = self.eaten.get_mut(g) {
            *e += eaten;
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

    /// Plants burning now, with how fiercely (0 to 1).
    pub fn burning<'a>(
        &'a self,
        plants: &'a [PlantInstance],
    ) -> impl Iterator<Item = (usize, f32)> + 'a {
        self.burning.iter().map(move |&i| {
            let strength = match species(plants[i].plant).map(|s| s.layer) {
                Some(Layer::Tree) => 1.0,
                Some(Layer::Shrub) => 0.6,
                _ => 0.35,
            };
            (i, strength * self.size(i).max(0.3))
        })
    }

    /// A plant is gone (taken, dug up): it stops living. Returns whether it stood in the
    /// world grid (to erase it from there).
    pub fn remove(&mut self, i: usize, plants: &[PlantInstance]) -> bool {
        if let Some(slot) = self.life.get_mut(i)
            && slot.take().is_some()
            && let Some(list) = self.grid.get_mut(&column(place(&plants[i])))
        {
            list.retain(|&j| j != i);
        }
        self.burning.retain(|&j| j != i);
        self.stood.get_mut(i).is_some_and(std::mem::take)
    }

    /// Living plants within `radius` of `at`.
    pub fn near(&self, at: Vec2, radius: f32, plants: &[PlantInstance]) -> Vec<usize> {
        let mut found = Vec::new();
        self.for_near(at, radius, plants, |j| found.push(j));
        found
    }

    fn for_near(&self, at: Vec2, radius: f32, plants: &[PlantInstance], mut f: impl FnMut(usize)) {
        let (cx, cz) = column(at);
        let r = radius.ceil() as i64;
        for z in cz - r..=cz + r {
            for x in cx - r..=cx + r {
                for &j in self.grid.get(&(x, z)).into_iter().flatten() {
                    if place(&plants[j]).distance(at) <= radius {
                        f(j);
                    }
                }
            }
        }
    }

    /// The crowns of the living trees and bushes cast their shade.
    fn cast_shade(&mut self, plants: &[PlantInstance]) {
        let crowns: Vec<(Vec2, f32, f32)> = self
            .life
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let l = (*l)?;
                let sp = species(plants[i].plant)?;
                (sp.crown > 0.0).then(|| {
                    let radius = sp.crown * plants[i].scale * l.size.sqrt();
                    (place(&plants[i]), radius.max(0.5), l.size)
                })
            })
            .collect();
        self.habitat.cast_shade(&crowns);
    }

    /// Fire reaches the plants within `radius` of `at` (something burning there): each catches
    /// with a chance that grows with its flammability and its dryness.
    pub fn ignite(
        &mut self,
        at: Vec2,
        radius: f32,
        plants: &[PlantInstance],
        changes: &mut Vec<Change>,
    ) {
        for i in self.near(at, radius, plants) {
            self.try_ignite(i, plants, 1.0, changes);
        }
    }

    fn try_ignite(
        &mut self,
        i: usize,
        plants: &[PlantInstance],
        chance: f32,
        changes: &mut Vec<Change>,
    ) {
        let Some(mut l) = self.life[i] else {
            return;
        };
        let Some(sp) = species(plants[i].plant) else {
            return;
        };
        if l.burning > 0.0 || sp.flammability <= 0.0 {
            return;
        }
        let at = place(&plants[i]);
        let dryness = (1.15 - self.habitat.moisture(at.x, at.y)).clamp(0.0, 1.0);
        if self.rng.next_f32() < chance * sp.flammability * dryness {
            l.burning = sp.burn_seconds * (0.6 + 0.4 * l.size);
            self.life[i] = Some(l);
            self.burning.push(i);
            changes.push(Change::Ignited(i));
        }
    }

    /// Fire runs through the plants for `dt` real seconds; `rain` in [0, 1] damps it.
    pub fn update_fire(
        &mut self,
        dt: f32,
        rain: f32,
        wind: Vec2,
        plants: &[PlantInstance],
        changes: &mut Vec<Change>,
    ) {
        if self.burning.is_empty() {
            return;
        }
        let burning = self.burning.clone();
        for i in burning {
            let Some(mut l) = self.life[i] else {
                continue;
            };
            let p = plants[i];
            let layer = species(p.plant).map_or(Layer::Herb, |s| s.layer);
            let at = place(&p);
            // Spread: each neighbour in reach may catch this second.
            let reach = layer.spread();
            for j in self.near(at, reach, plants) {
                if j == i {
                    continue;
                }
                let offset = place(&plants[j]) - at;
                let towards = offset.normalize_or_zero();
                let wind = 1.0 + 0.8 * towards.dot(wind);
                let near = 1.0 - offset.length() / reach;
                let rate = 1.5 * near * wind * (1.0 - 0.9 * rain);
                self.try_ignite(j, plants, 1.0 - (-rate * dt).exp(), changes);
            }
            l.burning -= dt * (1.0 + 3.0 * rain);
            if l.burning <= 0.0 {
                let stood = self.remove(i, plants);
                changes.push(Change::Died { plant: i, stood });
                // Ashes: organic matter back to the soil at once.
                self.soil.ash(at, 0.02);
            } else {
                self.life[i] = Some(l);
            }
        }
    }

    /// Advances life by `dt` real seconds (already sped up when the day is fast-forwarded).
    /// New plants are pushed onto `plants`; `free` says whether a seed may land at a point
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

    /// One life step of `days` game days.
    pub fn step(
        &mut self,
        days: f32,
        world: &World,
        plants: &mut Vec<PlantInstance>,
        free: &impl Fn(Vec2) -> bool,
        changes: &mut Vec<Change>,
    ) {
        self.cast_shade(plants);
        // Grazing, then the soil follows the cover (slowly).
        for g in 0..self.grazers.len() {
            let at = self.grazers[g];
            self.graze(g, at, days, plants, changes);
        }
        self.measure_cover(plants);
        self.soil.step(days, crate::season::evaporation(self.year));
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
            let own_shade = if sp.crown > 0.0 { 0.8 * l.size } else { 0.0 };
            // Herbs die back above ground in winter and come again in spring.
            let seasonal = if sp.layer == Layer::Herb {
                crate::season::herb_cover(self.year)
            } else {
                1.0
            };
            let habitat = self
                .habitat
                .suitability(world, p.plant, at.x, at.y, own_shade)
                * self.soil.offer_at(at)
                * seasonal;
            // Lotka-Volterra: what the neighbours take, weighed by distance and niche.
            let reach = sp.layer.reach();
            let mut crowd = 0.0;
            self.for_near(at, reach, plants, |j| {
                if j != i {
                    let w = 1.0 - place(&plants[j]).distance(at) / reach;
                    crowd += competition(p.plant, plants[j].plant) * w * self.size(j);
                }
            });
            let room = 1.0 - (l.size + crowd) / habitat.max(1e-3);
            // Growth follows the seasons; withering (room < 0) goes on all year.
            let pace = if room > 0.0 {
                crate::season::growth(self.year)
            } else {
                1.0
            };
            l.size = (l.size + sp.growth * pace * l.size * room * days).min(1.0);
            l.age += days;
            if l.size < DEATH_SIZE || l.age > l.lifespan {
                let stood = self.remove(i, plants);
                changes.push(Change::Died { plant: i, stood });
                continue;
            }
            self.life[i] = Some(l);
            if (l.size - self.shown[i]).abs() > 0.06 {
                self.shown[i] = l.size;
                changes.push(Change::Resized(i));
            }
            if sp.layer == Layer::Tree && !self.stood[i] && l.size >= TREE_STANDS {
                self.stood[i] = true;
                changes.push(Change::Stood(i));
            }
            // Seeds of a grown plant: a Poisson trial per step.
            if l.size > MATURE
                && self.rng.next_f32() < sp.seeds * seeding(sp.layer, self.year) * l.size * days
            {
                let angle = std::f32::consts::TAU * self.rng.next_f32();
                let distance = sp.dispersal * self.rng.next_f32().sqrt();
                seeds.push((p.plant, at + Vec2::new(angle.cos(), angle.sin()) * distance));
            }
        }
        // The seed bank: seeds that fell out of season wait in the soil; in spring they come
        // up, a share at each step over a few days.
        let germination = crate::season::germination(self.year);
        if germination < 0.3 {
            for seed in seeds.drain(..) {
                if self.bank.len() < SEED_BANK {
                    self.bank.push(seed);
                }
            }
        } else if !self.bank.is_empty() {
            let share =
                ((self.bank.len() as f32 * days / 3.0).ceil() as usize).min(self.bank.len());
            seeds.extend(self.bank.drain(..share));
        }
        let mut living = self.living();
        for (plant, target) in seeds {
            // The cap on plants (for the cost) holds where the ground is covered; bare ground
            // can always be recolonised, up to a hard limit a little above.
            let bare = self
                .soil
                .patch(target)
                .is_some_and(|k| self.soil.cover(k) < 0.5);
            if living >= self.max_plants && (!bare || living >= self.max_plants * 5 / 4) {
                continue;
            }
            let Some(sp) = species(plant) else {
                continue;
            };
            let h = self
                .habitat
                .suitability(world, plant, target.x, target.y, 0.0)
                * self.soil.offer_at(target);
            if self.rng.next_f32() >= h * crate::season::germination(self.year) || !free(target) {
                continue;
            }
            // Room in its own layer (and never on a trunk).
            let mut crowded = false;
            self.for_near(target, sp.layer.room().max(0.6), plants, |j| {
                let other = species(plants[j].plant).map(|s| s.layer);
                let d = place(&plants[j]).distance(target);
                if (other == Some(sp.layer) && d < sp.layer.room())
                    || (other == Some(Layer::Tree) && d < 0.6)
                {
                    crowded = true;
                }
            });
            if crowded {
                continue;
            }
            let (x, z) = (target.x.floor() as usize, target.y.floor() as usize);
            let variants = world::plants::VARIANTS;
            let new = PlantInstance {
                plant,
                variant: ((self.rng.next_f32() * variants as f32) as u32).min(variants - 1),
                rotation: (self.rng.next_f32() * 4.0) as u32 % 4,
                mirrored: self.rng.next_f32() < 0.5,
                scale: 0.85 + 0.3 * self.rng.next_f32(),
                base: [x, world.ground_top(x, z), z],
                offset: [target.x - x as f32 - 0.5, target.y - z as f32 - 0.5],
            };
            let lifespan = sp.lifespan * (0.7 + 0.6 * self.rng.next_f32());
            let index = plants.len();
            plants.push(new);
            self.life.push(Some(Life {
                size: SEEDLING,
                age: 0.0,
                lifespan,
                burning: 0.0,
            }));
            self.shown.push(SEEDLING);
            self.stood.push(false);
            self.grid.entry(column(target)).or_default().push(index);
            changes.push(Change::Sprouted(index));
            living += 1;
        }
    }
}

crate::save::persist_struct!(Life {
    size,
    age,
    lifespan,
    burning
});

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

    fn run(
        eco: &mut Ecology,
        world: &World,
        plants: &mut Vec<PlantInstance>,
        days: usize,
    ) -> Vec<Change> {
        // 60 steps a game day: r dt ≤ 0.1 for every species, stable for Euler.
        let mut changes = Vec::new();
        for _ in 0..days * 60 {
            eco.step(1.0 / 60.0, world, plants, &|_| true, &mut changes);
        }
        changes
    }

    /// A meadow patch grazed hard by many grazers, then left alone; and one grazed lightly.
    /// Returns its cover before, right after grazing, and after resting `rest` days.
    fn graze_then_rest(grazers: usize, spacing: f32, rest: usize) -> (f32, f32, f32) {
        let (world, mut plants, mut eco) = setup();
        let soil = eco.soil();
        let k = (0..soil.patches())
            .filter(|&k| {
                let c = soil.centre(k);
                world.biome(c.x as usize, c.y as usize) == Biome::Plains
            })
            .max_by(|&a, &b| soil.forage(a).total_cmp(&soil.forage(b)))
            .expect("a meadow");
        let centre = soil.centre(k);
        let before = soil.cover(k);
        let side = (grazers as f32).sqrt().ceil() as usize;
        let at: Vec<Vec2> = (0..grazers)
            .map(|g| {
                let (a, b) = ((g % side) as f32, (g / side) as f32);
                let half = (side - 1) as f32 / 2.0;
                centre + Vec2::new(a - half, b - half) * spacing
            })
            .collect();
        eco.set_grazers(&at);
        // Long enough for the soil to lose its water: a short pressure does not tip it.
        run(&mut eco, &world, &mut plants, 14);
        let grazed = eco.soil().cover(k);
        eco.set_grazers(&[]);
        for day in 0..rest {
            run(&mut eco, &world, &mut plants, 1);
            if std::env::var_os("DISSIPATIF_TRACE").is_some() {
                let (w, n) = eco.soil().state(k);
                println!(
                    "  repos j{day}: couverture {:.3}, eau {w:.3}, humus {n:.3}, offre {:.3}",
                    eco.soil().cover(k),
                    eco.soil().offer_at(centre)
                );
            }
        }
        (before, grazed, eco.soil().cover(k))
    }

    /// The tipping point, with the real plants and soil: light grazing does no harm; a small
    /// spot grazed bare closes again from the meadow around it; a wide land grazed bare stays
    /// bare once the grazers are gone (the soil has lost its water and its humus).
    #[test]
    fn grazed_bare_a_wide_meadow_does_not_come_back_a_small_spot_does() {
        let (before, _, light) = graze_then_rest(3, 2.0, 12);
        assert!(light > 0.8 * before, "light grazing: {before} → {light}");
        let (_, bare, spot) = graze_then_rest(16, 2.0, 12);
        assert!(bare < 0.05 && spot > 0.2, "small spot: {bare} → {spot}");
        let (_, bare, wide) = graze_then_rest(196, 2.5, 12);
        assert!(bare < 0.1 && wide < 0.1, "wide land: {bare} → {wide}");
    }

    #[test]
    fn grass_suits_a_meadow_mushrooms_need_damp_shade() {
        let (world, _, eco) = setup();
        let h = eco.habitat();
        let d = world.dims();
        let mut meadow = None;
        let mut shaded = None;
        for z in (2..d.nz - 2).step_by(3) {
            for x in (2..d.nx - 2).step_by(3) {
                let (fx, fz) = (x as f32 + 0.5, z as f32 + 0.5);
                let soil = world.block(x, world.ground_top(x, z).saturating_sub(1), z);
                if world.biome(x, z) == Biome::Plains && h.shade(fx, fz) == 0.0 {
                    meadow.get_or_insert((fx, fz));
                }
                if h.shade(fx, fz) > 0.5
                    && h.moisture(fx, fz) > 0.6
                    && soil == Material::ForestFloor
                    && world.water_level(x, z).is_none()
                {
                    shaded.get_or_insert((fx, fz));
                }
            }
        }
        let (mx, mz) = meadow.expect("no meadow");
        assert!(h.suitability(&world, Plant::Grass, mx, mz, 0.0) > 0.5);
        assert!(h.suitability(&world, Plant::Mushroom, mx, mz, 0.0) < 0.1);
        let (sx, sz) = shaded.expect("no damp shade");
        assert!(
            h.suitability(&world, Plant::Mushroom, sx, sz, 0.0)
                > h.suitability(&world, Plant::Mushroom, mx, mz, 0.0)
        );
        assert!(
            h.suitability(&world, Plant::Grass, sx, sz, 0.0)
                < h.suitability(&world, Plant::Grass, mx, mz, 0.0)
        );
    }

    /// Plants grow, spread and die; the population stays bounded; the run is the same every
    /// time; and the species coexist (flowers are not wiped out by grass).
    #[test]
    fn a_landscape_lives_its_species_coexist_and_it_is_deterministic() {
        let census = || {
            let (world, mut plants, mut eco) = setup();
            let count = |eco: &Ecology, plants: &[PlantInstance], kind: Plant| {
                (0..plants.len())
                    .filter(|&i| plants[i].plant == kind && eco.life[i].is_some())
                    .count()
            };
            let flowers0 = count(&eco, &plants, Plant::Flower);
            let start = eco.living();
            let changes = run(&mut eco, &world, &mut plants, 6);
            let sprouted = changes
                .iter()
                .filter(|c| matches!(c, Change::Sprouted(_)))
                .count();
            let died = changes
                .iter()
                .filter(|c| matches!(c, Change::Died { .. }))
                .count();
            (
                start,
                eco.living(),
                sprouted,
                died,
                flowers0,
                count(&eco, &plants, Plant::Flower),
            )
        };
        let (start, end, sprouted, died, flowers0, flowers) = census();
        assert!(
            sprouted > 100 && died > 100,
            "sprouted {sprouted}, died {died}"
        );
        assert!(
            end > start / 2 && end <= (start as f32 * MAX_GROWTH) as usize + 1,
            "{start} → {end}"
        );
        assert!(flowers * 2 > flowers0, "flowers {flowers0} → {flowers}");
        assert_eq!(census(), (start, end, sprouted, died, flowers0, flowers));
    }

    /// A patch cleared of every plant is recolonised, grass first.
    #[test]
    fn a_cleared_patch_is_recolonised_by_pioneer_herbs_first() {
        let (world, mut plants, mut eco) = setup();
        let centre = (0..plants.len())
            .filter(|&i| plants[i].plant == Plant::Grass)
            .map(|i| place(&plants[i]))
            .max_by_key(|&p| eco.near(p, 3.0, &plants).len())
            .unwrap();
        for i in eco.near(centre, 4.0, &plants) {
            eco.remove(i, &plants);
        }
        // A clearing loses some of its soil water (less cover, less infiltration): it takes
        // longer than before the soil was simulated.
        run(&mut eco, &world, &mut plants, 3);
        let back = eco.near(centre, 4.0, &plants);
        assert!(back.len() > 5, "only {} plants came back", back.len());
        // Pioneers come back first: grass and flowers (wind carries the flowers' seeds
        // farther), not shrubs or trees.
        let herbs = back
            .iter()
            .filter(|&&i| matches!(plants[i].plant, Plant::Grass | Plant::Flower))
            .count();
        assert!(
            herbs * 4 > back.len() * 3,
            "{herbs} herbs of {}",
            back.len()
        );
    }

    /// A tree that dies lets the light back in: the shade under it goes.
    #[test]
    fn a_dead_tree_lets_the_light_in() {
        let (_, plants, mut eco) = setup();
        let tree = (0..plants.len())
            .find(|&i| plants[i].plant == Plant::Broadleaf)
            .unwrap();
        let at = place(&plants[tree]);
        let before = eco.habitat.shade(at.x, at.y);
        assert!(before > 0.3);
        let stood = eco.remove(tree, &plants);
        assert!(stood, "an initial tree stands in the world grid");
        eco.cast_shade(&plants);
        assert!(eco.habitat.shade(at.x, at.y) < before - 0.3);
    }

    /// Trees scatter seeds: saplings appear.
    #[test]
    fn trees_seed_saplings() {
        let (world, mut plants, mut eco) = setup();
        let changes = run(&mut eco, &world, &mut plants, 20);
        let saplings = changes
            .iter()
            .filter(|c| matches!(c, Change::Sprouted(i) if is_tree(plants[*i].plant)))
            .count();
        assert!(saplings > 0, "no tree seedling in 20 days");
    }

    /// Fire runs through dry grass, and rain damps it.
    #[test]
    fn fire_spreads_in_dry_grass_and_rain_damps_it() {
        let burnt = |rain: f32| {
            let (_, plants, mut eco) = setup();
            // The driest place with plenty of grass.
            let start = (0..plants.len())
                .filter(|&i| plants[i].plant == Plant::Grass)
                .map(|i| place(&plants[i]))
                .filter(|&p| eco.near(p, 2.0, &plants).len() > 6)
                .min_by(|a, b| {
                    eco.habitat
                        .moisture(a.x, a.y)
                        .total_cmp(&eco.habitat.moisture(b.x, b.y))
                })
                .unwrap();
            let mut changes = Vec::new();
            for _ in 0..5 {
                eco.ignite(start, 0.6, &plants, &mut changes);
            }
            for _ in 0..60 * 60 {
                eco.update_fire(
                    1.0 / 60.0,
                    rain,
                    Vec2::new(0.89, 0.45),
                    &plants,
                    &mut changes,
                );
            }
            changes
                .iter()
                .filter(|c| matches!(c, Change::Died { .. }))
                .count()
        };
        let (dry, wet) = (burnt(0.0), burnt(1.0));
        assert!(dry > 10, "only {dry} plants burnt");
        assert!(wet < dry / 2, "rain: {wet} vs {dry}");
    }
}
#[cfg(test)]
mod timing {
    use super::*;
    use world::WorldConfig;
    #[test]
    #[ignore = "mesure : cargo test --release -p game timing -- --ignored --nocapture"]
    fn step_cost() {
        let world = World::generate(WorldConfig::standard(1));
        let mut plants = world.plants().to_vec();
        let mut eco = Ecology::new(&world, &plants, 1);
        let mut changes = Vec::new();
        let start = std::time::Instant::now();
        for _ in 0..100 {
            eco.step(1.0 / 600.0, &world, &mut plants, &|_| true, &mut changes);
        }
        println!(
            "{} plantes vivantes, un pas : {:?}",
            eco.living(),
            start.elapsed() / 100
        );
    }
}
