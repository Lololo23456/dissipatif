//! An anomaly: a living carpet on the ground of a clearing, a Gray-Scott reaction-diffusion
//! patch (`sim::gray_scott`, one layer). It is fed from a spring at one edge: the feed rate F
//! falls with the distance to the spring, so several regimes live side by side on the same
//! patch (dense near the spring, a labyrinth further, spots, then nothing): a phase diagram
//! one can walk across. The player acts only on the flow: stones laid on the carpet are walls
//! that nothing crosses.

use glam::{Vec2, Vec3};
use render::ParticleInstance;
use render::palette::srgb_hex;
use sim::gray_scott::{Boundary, GrayScott, GrayScottParams};
use sim::grid::Dims;
use world::{Material, World};

/// Simulation cells per world cell, along each axis.
pub const CELLS_PER_UNIT: usize = 4;
/// Side of the patch, in world cells.
pub const SIDE: usize = 16;
/// Simulation steps per fixed step of the game (60 per second): the carpet lives at a pace
/// one can watch, patterns forming in tens of seconds.
pub const STEPS_PER_TICK: usize = 3;
/// Dissipation k, the same everywhere, and the feed F at the spring and at the far side.
const KILL: f32 = 0.058;
const FEED_AT_SPRING: f32 = 0.046;
const FEED_FAR: f32 = 0.012;
/// Radius of the disc of cells a stone blocks, in simulation cells.
const STONE_RADIUS: f32 = 1.6;

pub struct Anomaly {
    sim: GrayScott,
    /// Simulation steps taken (its clock).
    steps: u64,
    /// World position of the patch's corner (x, z) and the height of its ground.
    origin: Vec3,
    /// The spring, in world coordinates (x, z).
    spring: Vec2,
    /// Stones laid on the carpet, in world coordinates.
    stones: Vec<Vec3>,
}

impl Anomaly {
    /// Grows an anomaly in the flat, open, dry clearing nearest to `near` with water close to
    /// one of its edges. None if the world has no such place.
    /// `ahead`: the direction (x, z) the camera looks at first: the carpet is placed in view.
    pub fn grow(world: &World, near: Vec3, ahead: Vec2, seed: u64) -> Option<Self> {
        let (x0, z0, ground, spring) = find_site(world, near, ahead)?;
        let n = SIDE * CELLS_PER_UNIT;
        let dims = Dims {
            nx: n,
            ny: 1,
            nz: n,
        };
        // The uniform feed is the labyrinth's; the per-cell feed below (falling from the spring)
        // replaces it once `GrayScott::step` reads the feed field (exercise in `sim`).
        let params = GrayScottParams {
            feed_rate: 0.029,
            kill_rate: KILL,
            ..GrayScottParams::reference()
        };
        let mut sim = GrayScott::new(dims, params, Boundary::NoFlux);
        sim.reset_with_seeds(14, 5, seed);
        let origin = Vec3::new(x0 as f32, ground, z0 as f32);
        // Feed falls off with the distance to the spring.
        let reach = (SIDE as f32) * 1.2;
        for z in 0..n {
            for x in 0..n {
                let p = Vec2::new(
                    origin.x + (x as f32 + 0.5) / CELLS_PER_UNIT as f32,
                    origin.z + (z as f32 + 0.5) / CELLS_PER_UNIT as f32,
                );
                let t = (p.distance(spring) / reach).clamp(0.0, 1.0);
                sim.set_feed(x, 0, z, FEED_AT_SPRING + (FEED_FAR - FEED_AT_SPRING) * t);
            }
        }
        Some(Self {
            sim,
            steps: 0,
            origin,
            spring,
            stones: Vec::new(),
        })
    }

    pub fn step(&mut self) {
        for _ in 0..STEPS_PER_TICK {
            self.sim.step();
        }
        self.steps += STEPS_PER_TICK as u64;
    }

    /// Seconds of the anomaly's life (its steps at the game's pace).
    fn sim_time(&self) -> f32 {
        self.steps as f32 / (STEPS_PER_TICK as f32 * 60.0)
    }

    /// Whether (x, z) is within `margin` cells of the carpet.
    pub fn near(&self, x: f32, z: f32, margin: f32) -> bool {
        let (lx, lz) = (x - self.origin.x, z - self.origin.z);
        let side = SIDE as f32;
        (-margin..side + margin).contains(&lx) && (-margin..side + margin).contains(&lz)
    }

    /// World corner and ground height of the patch.
    pub fn origin(&self) -> Vec3 {
        self.origin
    }

    #[cfg(test)]
    pub fn sim(&self) -> &GrayScott {
        &self.sim
    }

    /// Whether (x, z) lies on the carpet.
    pub fn covers(&self, x: f32, z: f32) -> bool {
        let (lx, lz) = (x - self.origin.x, z - self.origin.z);
        (0.0..SIDE as f32).contains(&lx) && (0.0..SIDE as f32).contains(&lz)
    }

    /// Lays a stone at (x, z): the cells under it become walls. False if not on the carpet.
    pub fn lay_stone(&mut self, x: f32, z: f32) -> bool {
        if !self.covers(x, z) {
            return false;
        }
        let k = CELLS_PER_UNIT as f32;
        let (cx, cz) = ((x - self.origin.x) * k, (z - self.origin.z) * k);
        let n = self.sim.dims().nx as i64;
        let r = STONE_RADIUS.ceil() as i64;
        for dz in -r..=r {
            for dx in -r..=r {
                let (sx, sz) = (cx.floor() as i64 + dx, cz.floor() as i64 + dz);
                if sx < 0 || sz < 0 || sx >= n || sz >= n {
                    continue;
                }
                let d = Vec2::new(sx as f32 + 0.5 - cx, sz as f32 + 0.5 - cz).length();
                if d <= STONE_RADIUS {
                    self.sim.set_solid(sx as usize, 0, sz as usize, true);
                }
            }
        }
        self.stones.push(Vec3::new(x, self.origin.y, z));
        true
    }

    /// Draws the carpet: one micro-cube per cell where V is present, rising out of the ground
    /// and paling as V grows, shining faintly by itself (much more at night); stones in grey.
    /// `night` in [0, 1].
    pub fn draw(&self, world: &World, night: f32, look: Look, out: &mut Vec<ParticleInstance>) {
        match look {
            Look::Carpet => self.draw_carpet(world, night, out),
            Look::Stalks => self.draw_stalks(world, night, out),
            Look::Lichen => self.draw_lichen(world, night, out),
            Look::Motes => self.draw_motes(world, night, out),
        }
        self.draw_spring_and_stones(world, out);
    }

    /// Ground height under (wx, wz).
    fn ground(world: &World, wx: f32, wz: f32) -> f32 {
        let d = world.dims();
        world.ground_top(
            (wx.max(0.0) as usize).min(d.nx - 1),
            (wz.max(0.0) as usize).min(d.nz - 1),
        ) as f32
    }

    /// Cells with their world position and strength t in [0, 1] (V above `floor`).
    fn cells(&self, floor: f32) -> impl Iterator<Item = (usize, usize, f32, f32, f32)> + '_ {
        let k = CELLS_PER_UNIT as f32;
        let dims = self.sim.dims();
        (0..dims.nz)
            .flat_map(move |z| (0..dims.nx).map(move |x| (x, z)))
            .filter_map(move |(x, z)| {
                let value = self.sim.v().get(x, 0, z);
                if value < floor || self.sim.is_solid(x, 0, z) {
                    return None;
                }
                let t = ((value - floor) / 0.3).clamp(0.0, 1.0);
                let wx = self.origin.x + (x as f32 + 0.5) / k;
                let wz = self.origin.z + (z as f32 + 0.5) / k;
                Some((x, z, wx, wz, t))
            })
    }

    /// Stalks: thin pale columns rising where the reaction is strong, like coral or a
    /// colony of fungi, their tips glowing. Sparse (one cell in two), so light passes.
    fn draw_stalks(&self, world: &World, night: f32, out: &mut Vec<ParticleInstance>) {
        let stem = srgb_hex(0xd9d2c4);
        let tip = srgb_hex(0xf0a8e0);
        let size = 0.11;
        for (x, z, wx, wz, t) in self.cells(0.12) {
            if (x + z) % 2 == 1 || t < 0.15 {
                continue;
            }
            let ground = Self::ground(world, wx, wz);
            let height = 0.15 + 0.85 * t * t;
            let count = (height / size).ceil() as usize;
            for k in 0..count {
                let y = ground + size * (k as f32 + 0.5);
                let top = k + 1 == count;
                let (color, glow) = if top {
                    (tip, -(0.3 + 1.8 * night) * t)
                } else {
                    (stem, 0.0)
                };
                // Thinner towards the top.
                let thin = size * (1.0 - 0.35 * k as f32 / count as f32);
                out.push(ParticleInstance {
                    centre_size: [wx, y, wz, thin],
                    color: [color[0], color[1], color[2], glow],
                });
            }
        }
    }

    /// Lichen: a thin crust on the ground, barely darker than the soil by day, its lines
    /// lighting up at night. One has to notice it.
    fn draw_lichen(&self, world: &World, night: f32, out: &mut Vec<ParticleInstance>) {
        let crust = srgb_hex(0x4a5240);
        let light = srgb_hex(0xb8f0c0);
        let k = CELLS_PER_UNIT as f32;
        let size = 1.0 / k;
        for (_, _, wx, wz, t) in self.cells(0.1) {
            let ground = Self::ground(world, wx, wz);
            // Sunk so that only a sliver shows above the ground.
            let y = ground - size * 0.5 + 0.015 + 0.02 * t;
            let color = [0, 1, 2].map(|i| crust[i] + (light[i] - crust[i]) * night * t);
            out.push(ParticleInstance {
                centre_size: [wx, y, wz, size],
                color: [color[0], color[1], color[2], -(2.2 * night * t * t)],
            });
        }
    }

    /// Motes: points of light hovering over the ground where the reaction is strong, drifting
    /// and breathing; faint by day, a living constellation at night.
    fn draw_motes(&self, world: &World, night: f32, out: &mut Vec<ParticleInstance>) {
        let warm = srgb_hex(0xffd9a0);
        let cold = srgb_hex(0xc8a8ff);
        let time = self.sim_time();
        for (x, z, wx, wz, t) in self.cells(0.15) {
            // One cell in three: a scattered swarm rather than a sheet.
            if (x * 7 + z * 13) % 3 != 0 {
                continue;
            }
            let ground = Self::ground(world, wx, wz);
            let phase = (x * 31 + z * 17) as f32 * 0.37;
            let y = ground + 0.35 + 0.5 * t + 0.08 * (time * 0.7 + phase).sin();
            let color = [0, 1, 2].map(|i| cold[i] + (warm[i] - cold[i]) * t);
            out.push(ParticleInstance {
                centre_size: [
                    wx + 0.05 * (time * 0.5 + phase).cos(),
                    y,
                    wz + 0.05 * (time * 0.4 + phase).sin(),
                    0.035 + 0.03 * t,
                ],
                color: [color[0], color[1], color[2], -(0.8 + 3.0 * night) * t],
            });
        }
    }

    /// The first look: a mosaic of tiles rising out of the ground.
    fn draw_carpet(&self, world: &World, night: f32, out: &mut Vec<ParticleInstance>) {
        let deep = srgb_hex(0x45285e);
        let bright = srgb_hex(0xf2b8e6);
        let k = CELLS_PER_UNIT as f32;
        let size = 1.0 / k * 0.92;
        let dims = self.sim.dims();
        let v = self.sim.v();
        let world_dims = world.dims();
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                let value = v.get(x, 0, z);
                if value < 0.08 || self.sim.is_solid(x, 0, z) {
                    continue;
                }
                let t = ((value - 0.08) / 0.3).clamp(0.0, 1.0);
                let wx = self.origin.x + (x as f32 + 0.5) / k;
                let wz = self.origin.z + (z as f32 + 0.5) / k;
                let column = (
                    (wx as usize).min(world_dims.nx - 1),
                    (wz as usize).min(world_dims.nz - 1),
                );
                let ground = world.ground_top(column.0, column.1) as f32;
                // Buried when faint, standing up to half its size out of the ground when strong.
                let y = ground - size * 0.5 + size * (0.15 + 0.5 * t);
                let color = [0, 1, 2].map(|i| deep[i] + (bright[i] - deep[i]) * t);
                let glow = -(0.12 + 1.6 * night) * t;
                out.push(ParticleInstance {
                    centre_size: [wx, y, wz, size],
                    color: [color[0], color[1], color[2], glow],
                });
            }
        }
    }

    fn draw_spring_and_stones(&self, world: &World, out: &mut Vec<ParticleInstance>) {
        let world_dims = world.dims();
        // The spring: water welling up, a few bubbles rising and falling back.
        let water = srgb_hex(0x9fd4e8);
        let spring_ground = world.ground_top(
            (self.spring.x as usize).min(world_dims.nx - 1),
            (self.spring.y as usize).min(world_dims.nz - 1),
        ) as f32;
        let level = world
            .water_level(
                (self.spring.x as usize).min(world_dims.nx - 1),
                (self.spring.y as usize).min(world_dims.nz - 1),
            )
            .unwrap_or(spring_ground);
        for b in 0..5 {
            let phase = (self.sim_time() * 0.9 + b as f32 * 0.37).fract();
            let angle = b as f32 * 2.4;
            out.push(ParticleInstance {
                centre_size: [
                    self.spring.x + 0.25 * angle.cos(),
                    level + 0.05 + 0.25 * (phase * std::f32::consts::PI).sin(),
                    self.spring.y + 0.25 * angle.sin(),
                    0.07 * (1.0 - phase),
                ],
                color: [water[0], water[1], water[2], 0.6],
            });
        }
        let stone = srgb_hex(0x9c958c);
        for s in &self.stones {
            let ground = world.ground_top(
                (s.x as usize).min(world_dims.nx - 1),
                (s.z as usize).min(world_dims.nz - 1),
            ) as f32;
            out.push(ParticleInstance {
                centre_size: [s.x, ground + 0.08, s.z, 0.32],
                color: [stone[0], stone[1], stone[2], 0.0],
            });
        }
    }

    /// Share of the carpet alive (v above a threshold): the measure the notebook records.
    pub fn activity(&self) -> f32 {
        let v = &self.sim.v().data;
        v.iter().filter(|&&v| v > 0.18).count() as f32 / v.len() as f32
    }
}

/// How the anomaly is drawn (the simulation is the same).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Carpet,
    Stalks,
    Lichen,
    Motes,
}

impl Look {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "tapis" => Some(Look::Carpet),
            "tiges" => Some(Look::Stalks),
            "lichen" => Some(Look::Lichen),
            "lueurs" => Some(Look::Motes),
            _ => None,
        }
    }
}

/// Wood standing up: a tree is there.
fn is_trunk(material: Material) -> bool {
    matches!(
        material,
        Material::Wood
            | Material::BarkDark
            | Material::BirchBark
            | Material::PalmTrunk
            | Material::DeadWood
            | Material::Cactus
    )
}

/// The best place for the carpet near `near`: a square of `SIDE` cells, dry and without
/// trees (bushes and ground cover give way), scored on how flat and grassy it is, whether
/// water is close to one edge (the spring), and its distance from `near` (close enough to be
/// seen from there, not on top of it). Returns (corner x, corner z, ground height, spring).
/// With no water nearby, the spring wells up at the edge of the square facing `near`.
fn find_site(world: &World, near: Vec3, ahead: Vec2) -> Option<(usize, usize, f32, Vec2)> {
    let dims = world.dims();
    let side = SIDE as f32;
    let mut best: Option<(f32, (usize, usize, f32, Vec2))> = None;
    let start = Vec2::new(near.x, near.z);
    for z0 in (2..dims.nz.saturating_sub(SIDE + 2)).step_by(2) {
        for x0 in (2..dims.nx.saturating_sub(SIDE + 2)).step_by(2) {
            let centre = Vec2::new(x0 as f32 + side / 2.0, z0 as f32 + side / 2.0);
            let distance = centre.distance(start);
            // Seen from the start: its near edge a few cells away, not too far.
            let ideal = side / 2.0 + 6.0;
            let away = (distance - ideal).abs();
            if away > 40.0 {
                continue;
            }
            let (mut lowest, mut highest, mut bare) = (usize::MAX, 0, 0);
            let mut usable = true;
            'cells: for z in z0..z0 + SIDE {
                for x in x0..x0 + SIDE {
                    let top = world.ground_top(x, z);
                    if top == 0
                        || world.water_level(x, z).is_some()
                        || (top..(top + 3).min(dims.ny)).any(|y| is_trunk(world.block(x, y, z)))
                    {
                        usable = false;
                        break 'cells;
                    }
                    lowest = lowest.min(top);
                    highest = highest.max(top);
                    if !matches!(
                        world.block(x, top - 1, z),
                        Material::Grass | Material::ForestFloor | Material::DryGrass
                    ) {
                        bare += 1;
                    }
                }
            }
            // The carpet drapes over the ground (each cell drawn at its own height): gentle
            // slopes are fine, cliffs are not.
            if !usable || highest - lowest > 8 {
                continue;
            }
            // The spring: the water closest to the square.
            let mut spring: Option<(f32, Vec2)> = None;
            let reach = 10;
            for z in z0.saturating_sub(reach)..(z0 + SIDE + reach).min(dims.nz) {
                for x in x0.saturating_sub(reach)..(x0 + SIDE + reach).min(dims.nx) {
                    if world.water_level(x, z).is_some() {
                        let p = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
                        let d = p.distance(centre);
                        if spring.is_none_or(|s| d < s.0) {
                            spring = Some((d, p));
                        }
                    }
                }
            }
            // In front of the camera: seen at once.
            let facing = (centre - start)
                .normalize_or_zero()
                .dot(ahead.normalize_or_zero());
            let score = away * 0.5
                + (1.0 - facing) * 12.0
                + (highest - lowest) as f32 * 3.0
                + bare as f32 / (side * side) * 20.0
                + if spring.is_some() { 0.0 } else { 12.0 };
            if best.as_ref().is_some_and(|b| score >= b.0) {
                continue;
            }
            let spring = spring.map(|s| s.1).unwrap_or_else(|| {
                // Wells up at the edge facing the start.
                let towards = (start - centre).normalize_or(Vec2::X);
                centre + towards * (side / 2.0)
            });
            best = Some((score, (x0, z0, lowest as f32, spring)));
        }
    }
    best.map(|(_, site)| site)
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    #[test]
    fn grows_in_sight_of_the_start_in_every_world() {
        for seed in 1..=8 {
            let world = World::generate(WorldConfig::standard(seed));
            let start = crate::player::spawn_point(&world);
            let anomaly = Anomaly::grow(&world, start, Vec2::new(-1.0, -1.0), 1)
                .unwrap_or_else(|| panic!("no anomaly in world {seed}"));
            let centre = anomaly.origin() + Vec3::new(SIDE as f32 / 2.0, 0.0, SIDE as f32 / 2.0);
            let distance = Vec2::new(centre.x - start.x, centre.z - start.z).length();
            assert!(distance < 45.0, "world {seed}: {distance} cells away");
        }
    }

    #[test]
    fn grows_near_the_start_and_lives() {
        let world = World::generate(WorldConfig::standard(6));
        let start = crate::player::spawn_point(&world);
        let mut anomaly =
            Anomaly::grow(&world, start, Vec2::new(-1.0, -1.0), 1).expect("no site for an anomaly");
        assert!(anomaly.origin().distance(start) < 80.0);
        for _ in 0..1000 {
            anomaly.step();
        }
        let activity = anomaly.activity();
        assert!(activity > 0.05 && activity < 0.95, "activity {activity}");
    }

    #[test]
    fn stones_block_cells_only_on_the_carpet() {
        let world = World::generate(WorldConfig::standard(6));
        let start = crate::player::spawn_point(&world);
        let mut anomaly = Anomaly::grow(&world, start, Vec2::new(-1.0, -1.0), 1).expect("no site");
        let o = anomaly.origin();
        assert!(!anomaly.lay_stone(o.x - 3.0, o.z));
        assert!(anomaly.lay_stone(o.x + 5.0, o.z + 5.0));
        let k = CELLS_PER_UNIT;
        assert!(anomaly.sim().is_solid(5 * k, 0, 5 * k));
        assert!(!anomaly.sim().is_solid(5 * k + 4, 0, 5 * k));
    }
}
