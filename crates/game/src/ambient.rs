//! Life in the air: pollen and dust drifting in the light, leaves falling from the trees.
//! Purely visual. They live around the place the camera looks at, where they can be seen.
//! Deterministic: all randomness comes from a seeded generator.

use render::ParticleInstance;
use render::palette::srgb_hex;
use sim::rng::SplitMix64;
use world::{Material, World};

/// Particles live within this horizontal distance of the camera target, in cells.
const RADIUS: f32 = 40.0;
/// Leaves are born within `RADIUS` but the wind may carry them this far before we let go.
const LEAF_REACH: f32 = 1.5 * RADIUS;
/// How many motes of pollen and dust float at once.
const MOTES: usize = 260;
/// Glow of a mote between glints (see `ParticleInstance::color`).
const MOTE_GLOW_BASE: f32 = 0.25;
/// Leaves torn off per second around the target, at full gust (fewer when the wind drops).
const LEAF_RATE: f32 = 7.0;
const MAX_LEAVES: usize = 200;
/// Speed of the wind carrying leaves and motes, at full gust, in cells per second.
const WIND_SPEED: f32 = 2.2;

#[derive(Clone, Copy, Debug)]
struct Particle {
    position: [f32; 3],
    /// Phase of the wobble, so that particles do not all move in step.
    phase: f32,
    age: f32,
    lifetime: f32,
    size: f32,
    color: [f32; 3],
}

pub struct Ambient {
    /// Direction the wind blows towards (x, z), unit: set each frame (see `wind.rs`).
    pub wind: [f32; 2],
    rng: SplitMix64,
    motes: Vec<Particle>,
    leaves: Vec<Particle>,
    /// Where leaves can start to fall: the underside of every leaf block.
    canopy: Vec<[f32; 3]>,
    leaf_colors: [[f32; 3]; 3],
    mote_color: [f32; 3],
    leaf_debt: f32,
    instances: Vec<ParticleInstance>,
}

impl Ambient {
    pub fn new(world: &World, seed: u64) -> Self {
        let dims = world.dims();
        let mut canopy = Vec::new();
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                for y in 1..dims.ny {
                    if world.block(x, y, z).is_canopy() && world.block(x, y - 1, z) == Material::Air
                    {
                        canopy.push([x as f32 + 0.5, y as f32, z as f32 + 0.5]);
                    }
                }
            }
        }
        Self {
            wind: [0.89, 0.45],
            rng: SplitMix64::new(seed),
            motes: Vec::with_capacity(MOTES),
            leaves: Vec::with_capacity(MAX_LEAVES),
            canopy,
            // Mostly green, some turning yellow and orange: late summer.
            leaf_colors: [srgb_hex(0x7f9e3e), srgb_hex(0xc9a43a), srgb_hex(0xc8692e)],
            mote_color: srgb_hex(0xfff0c8),
            leaf_debt: 0.0,
            instances: Vec::with_capacity(MOTES + MAX_LEAVES),
        }
    }

    /// What to draw, rebuilt by `update`.
    pub fn instances(&self) -> &[ParticleInstance] {
        &self.instances
    }

    /// Advances by `dt` seconds (`time` since start drives the gusts), keeping particles
    /// around `target` (x, z).
    pub fn update(&mut self, dt: f32, time: f32, target: [f32; 2], world: &World) {
        // Same gust rhythm as the plants in the shader (`gust` in voxel.wgsl), at the target.
        let gust = crate::listen::gust(time, target[0], target[1]);
        self.update_motes(dt, time, gust, target, world);
        self.update_leaves(dt, time, gust, target, world);

        self.instances.clear();
        let motes = self.motes.iter().map(|p| (p, true));
        for (p, mote) in motes.chain(self.leaves.iter().map(|p| (p, false))) {
            // Fade in and out by size, so particles never pop.
            let t = p.age / p.lifetime;
            let fade = (t * 6.0).min((1.0 - t) * 6.0).clamp(0.0, 1.0);
            let [r, g, b] = p.color;
            // Motes glint as they turn in the light: a sharp peak now and then, a soft glow
            // the rest of the time. Leaves do not glow.
            let glow = if mote {
                let turn = 0.5 + 0.5 * (time * 2.3 + p.phase * 7.0).sin();
                MOTE_GLOW_BASE + turn.powi(6)
            } else {
                0.0
            };
            self.instances.push(ParticleInstance {
                centre_size: [p.position[0], p.position[1], p.position[2], p.size * fade],
                color: [r, g, b, glow],
            });
        }
    }

    fn update_motes(&mut self, dt: f32, time: f32, gust: f32, target: [f32; 2], world: &World) {
        let wind = self.wind;
        while self.motes.len() < MOTES {
            let (x, z) = self.random_around(target);
            let Some(floor) = surface(world, x, z) else {
                break;
            };
            let mote = Particle {
                position: [x, floor + 0.5 + 5.0 * self.rng.next_f32(), z],
                phase: std::f32::consts::TAU * self.rng.next_f32(),
                age: 0.0,
                lifetime: 6.0 + 6.0 * self.rng.next_f32(),
                size: 0.08 + 0.07 * self.rng.next_f32(),
                color: self.mote_color,
            };
            self.motes.push(mote);
        }
        for p in &mut self.motes {
            p.age += dt;
            // Carried by the wind, with a slow lazy wobble up and down.
            let push = 0.4 * WIND_SPEED * gust;
            p.position[0] += (wind[0] * push + 0.2 * (time * 0.7 + p.phase).sin()) * dt;
            p.position[2] += (wind[1] * push + 0.2 * (time * 0.6 + p.phase).cos()) * dt;
            p.position[1] += 0.15 * (time * 0.9 + p.phase).sin() * dt;
        }
        self.motes
            .retain(|p| p.age < p.lifetime && near(p.position, target));
    }

    fn update_leaves(&mut self, dt: f32, time: f32, gust: f32, target: [f32; 2], world: &World) {
        let wind = self.wind;
        // Gusts tear leaves off: more of them when the wind is strong.
        self.leaf_debt += LEAF_RATE * gust * gust * dt;
        while self.leaf_debt >= 1.0 {
            self.leaf_debt -= 1.0;
            if self.leaves.len() >= MAX_LEAVES || self.canopy.is_empty() {
                continue;
            }
            // A few tries to find a leafy spot near the target.
            for _ in 0..24 {
                let start = self.canopy[self.rng.next_below(self.canopy.len())];
                if near(start, target) {
                    let color = self.leaf_colors[self.rng.next_below(3)];
                    let leaf = Particle {
                        position: start,
                        phase: std::f32::consts::TAU * self.rng.next_f32(),
                        age: 0.0,
                        lifetime: 20.0,
                        size: 0.18 + 0.08 * self.rng.next_f32(),
                        color,
                    };
                    self.leaves.push(leaf);
                    break;
                }
            }
        }
        for p in &mut self.leaves {
            p.age += dt;
            // Swept along by the wind, falling slowly, sometimes lifted by a gust, fluttering.
            let flutter = (time * 3.1 + p.phase).sin();
            let push = WIND_SPEED * gust;
            p.position[0] += (wind[0] * push + 0.3 * flutter) * dt;
            p.position[2] += (wind[1] * push + 0.3 * (time * 2.3 + p.phase).cos()) * dt;
            let lift = 0.5 * (gust - 0.6).max(0.0) * (time * 1.7 + p.phase).sin().max(0.0);
            p.position[1] += (lift - 0.45 - 0.2 * flutter.abs()) * dt;
        }
        // A leaf ends when it reaches the ground or the water.
        self.leaves.retain(|p| {
            p.age < p.lifetime
                && within(p.position, target, LEAF_REACH)
                && surface(world, p.position[0], p.position[2]).is_some_and(|s| p.position[1] > s)
        });
    }

    fn random_around(&mut self, target: [f32; 2]) -> (f32, f32) {
        // Uniform in the disc: radius ∝ √u.
        let r = RADIUS * self.rng.next_f32().sqrt();
        let a = std::f32::consts::TAU * self.rng.next_f32();
        (target[0] + r * a.cos(), target[1] + r * a.sin())
    }
}

fn near(p: [f32; 3], target: [f32; 2]) -> bool {
    within(p, target, RADIUS)
}

fn within(p: [f32; 3], target: [f32; 2], radius: f32) -> bool {
    let (dx, dz) = (p[0] - target[0], p[2] - target[1]);
    dx * dx + dz * dz < radius * radius
}

/// Height of what a falling thing lands on at (x, z): the water surface or the ground.
/// `None` outside the world.
fn surface(world: &World, x: f32, z: f32) -> Option<f32> {
    let dims = world.dims();
    if x < 0.0 || z < 0.0 || x >= dims.nx as f32 || z >= dims.nz as f32 {
        return None;
    }
    let (cx, cz) = (x as usize, z as usize);
    let ground = world.ground_top(cx, cz) as f32;
    Some(world.water_level(cx, cz).map_or(ground, |w| w.max(ground)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::grid::Dims;
    use world::WorldConfig;

    fn small_world() -> World {
        World::generate(WorldConfig {
            dims: Dims {
                nx: 96,
                ny: 48,
                nz: 96,
            },
            sea_level: 16.0,
            seed: 2,
        })
    }

    #[test]
    fn particles_fill_up_and_stay_near_the_target() {
        let world = small_world();
        let mut ambient = Ambient::new(&world, 1);
        let target = [48.0, 48.0];
        for frame in 0..600 {
            ambient.update(1.0 / 60.0, frame as f32 / 60.0, target, &world);
        }
        assert!(ambient.instances().len() >= MOTES / 2);
        for p in ambient.instances() {
            let [x, _, z, size] = p.centre_size;
            assert!(
                within([x, 0.0, z], target, LEAF_REACH),
                "particle far from the target"
            );
            assert!(size.is_finite() && size >= 0.0);
        }
    }

    #[test]
    fn same_seed_same_air() {
        let world = small_world();
        let run = || {
            let mut ambient = Ambient::new(&world, 3);
            for frame in 0..120 {
                ambient.update(1.0 / 60.0, frame as f32 / 60.0, [40.0, 50.0], &world);
            }
            ambient.instances().to_vec()
        };
        assert_eq!(run(), run());
    }
}
