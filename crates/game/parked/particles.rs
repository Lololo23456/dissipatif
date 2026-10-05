//! Visual water particles: foam drifting on fast water, spray thrown up at springs and falls.
//! Purely decorative for now: they read the water simulation, they never change it.
//! Deterministic: all randomness comes from a seeded generator.

use render::ParticleInstance;
use render::palette;
use sim::grid::Field2;
use sim::hydrology::{Hydrology, Spring};
use sim::rng::SplitMix64;

/// Most particles alive at once: beyond, new ones are not spawned.
const MAX_PARTICLES: usize = 3000;
/// Current speed (cells per simulated time unit) above which foam forms, and the number of
/// foam particles per second per column for each unit of speed above it.
/// Same threshold as the foam colour of the water (`water_style` in main.rs).
const FOAM_SPEED: f32 = 3.5;
const FOAM_RATE: f32 = 0.25;
/// The simulation runs much faster than real time; particles drift at the current's speed
/// times this factor, in cells per real second, so the motion stays readable.
const DRIFT_SCALE: f32 = 1.5;
/// A wet column whose neighbour's water surface is this much lower is a fall: spray.
const FALL_HEIGHT: f32 = 0.8;
const FALL_RATE: f32 = 6.0;
/// Spray particles per second at each spring.
const SPRING_RATE: f32 = 30.0;
/// Cells per second squared.
const GRAVITY: f32 = 9.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Floats on the surface and drifts with the current.
    Foam,
    /// Flies, falls back, dies when it lands.
    Spray,
}

#[derive(Clone, Copy, Debug)]
struct Particle {
    kind: Kind,
    position: [f32; 3],
    velocity: [f32; 3],
    age: f32,
    lifetime: f32,
    size: f32,
}

pub struct WaterParticles {
    particles: Vec<Particle>,
    rng: SplitMix64,
    instances: Vec<ParticleInstance>,
    foam_color: [f32; 3],
    spray_color: [f32; 3],
}

impl WaterParticles {
    pub fn new(seed: u64) -> Self {
        let foam = palette::foam();
        Self {
            particles: Vec::with_capacity(MAX_PARTICLES),
            rng: SplitMix64::new(seed),
            instances: Vec::with_capacity(MAX_PARTICLES),
            foam_color: foam[3],
            spray_color: foam[4],
        }
    }

    /// What to draw: one instance per living particle, rebuilt by `update`.
    pub fn instances(&self) -> &[ParticleInstance] {
        &self.instances
    }

    /// Number of living particles (used by tests and measurements).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.particles.len()
    }

    /// Advances the particles by `dt` real seconds, spawns new ones, rebuilds the instances.
    /// `floor` and `surface` are the ground voxel tops and the drawn water surface.
    pub fn update(
        &mut self,
        dt: f32,
        hydrology: &Hydrology,
        floor: &Field2,
        surface: &Field2,
        springs: &[Spring],
    ) {
        self.spawn(dt, hydrology, floor, surface, springs);
        self.advance(dt, hydrology, floor, surface);
        let (foam_color, spray_color) = (self.foam_color, self.spray_color);
        self.instances.clear();
        self.instances.extend(self.particles.iter().map(|p| {
            // Shrinks as it ages, so it fades out instead of popping.
            let size = p.size * (1.0 - p.age / p.lifetime).max(0.0).sqrt();
            let [r, g, b] = if p.kind == Kind::Foam {
                foam_color
            } else {
                spray_color
            };
            ParticleInstance {
                centre_size: [p.position[0], p.position[1], p.position[2], size],
                color: [r, g, b, 1.0],
            }
        }));
    }

    fn spawn(
        &mut self,
        dt: f32,
        hydrology: &Hydrology,
        floor: &Field2,
        surface: &Field2,
        springs: &[Spring],
    ) {
        let (nx, nz) = (floor.nx, floor.nz);
        let wet = |x: usize, z: usize| surface.get(x, z) > floor.get(x, z) + 1e-3;
        for z in 0..nz {
            for x in 0..nx {
                if !wet(x, z) {
                    continue;
                }
                let [vx, vz] = hydrology.velocity(x, z);
                let speed = (vx * vx + vz * vz).sqrt();
                let top = surface.get(x, z);
                // Foam on fast water.
                if speed > FOAM_SPEED && self.chance(FOAM_RATE * (speed - FOAM_SPEED) * dt) {
                    let position = [
                        x as f32 + self.rng.next_f32(),
                        top,
                        z as f32 + self.rng.next_f32(),
                    ];
                    let lifetime = 1.0 + 2.0 * self.rng.next_f32();
                    let size = 0.12 + 0.1 * self.rng.next_f32();
                    self.push(Kind::Foam, position, [0.0; 3], lifetime, size);
                }
                // Spray where the water drops into a lower neighbour, thrown along the flow.
                let downstream = (
                    x as isize + vx.signum() as isize * (vx.abs() > vz.abs()) as isize,
                    z as isize + vz.signum() as isize * (vz.abs() >= vx.abs()) as isize,
                );
                if let (Ok(xn), Ok(zn)) =
                    (usize::try_from(downstream.0), usize::try_from(downstream.1))
                    && xn < nx
                    && zn < nz
                    && speed > 0.1
                    && top - surface.get(xn, zn) > FALL_HEIGHT
                    && self.chance(FALL_RATE * dt)
                {
                    let edge = [
                        x as f32 + 0.5 + 0.5 * (xn as f32 - x as f32),
                        top,
                        z as f32 + 0.5 + 0.5 * (zn as f32 - z as f32),
                    ];
                    let throw = [vx * 0.4, 1.5 + 1.5 * self.rng.next_f32(), vz * 0.4];
                    let size = 0.08 + 0.08 * self.rng.next_f32();
                    self.push(Kind::Spray, edge, throw, 1.5, size);
                }
            }
        }
        // Spray jumping out of each spring.
        for spring in springs {
            if !self.chance(SPRING_RATE * dt) {
                continue;
            }
            let top = surface
                .get(spring.x, spring.z)
                .max(floor.get(spring.x, spring.z));
            let angle = std::f32::consts::TAU * self.rng.next_f32();
            let position = [spring.x as f32 + 0.5, top, spring.z as f32 + 0.5];
            let throw = [
                angle.cos() * 0.8,
                3.0 + 2.0 * self.rng.next_f32(),
                angle.sin() * 0.8,
            ];
            let size = 0.08 + 0.1 * self.rng.next_f32();
            self.push(Kind::Spray, position, throw, 2.0, size);
        }
    }

    /// True with probability `p` (clamped to [0, 1]).
    fn chance(&mut self, p: f32) -> bool {
        self.rng.next_f32() < p.min(1.0)
    }

    fn push(
        &mut self,
        kind: Kind,
        position: [f32; 3],
        velocity: [f32; 3],
        lifetime: f32,
        size: f32,
    ) {
        if self.particles.len() < MAX_PARTICLES {
            self.particles.push(Particle {
                kind,
                position,
                velocity,
                age: 0.0,
                lifetime,
                size,
            });
        }
    }

    fn advance(&mut self, dt: f32, hydrology: &Hydrology, floor: &Field2, surface: &Field2) {
        let (nx, nz) = (floor.nx, floor.nz);
        let column = |p: [f32; 3]| -> Option<(usize, usize)> {
            let (x, z) = (p[0].floor(), p[2].floor());
            (x >= 0.0 && z >= 0.0 && (x as usize) < nx && (z as usize) < nz)
                .then_some((x as usize, z as usize))
        };
        let mut i = 0;
        while i < self.particles.len() {
            let p = &mut self.particles[i];
            p.age += dt;
            let alive = match p.kind {
                Kind::Foam => match column(p.position) {
                    Some((x, z)) if surface.get(x, z) > floor.get(x, z) + 1e-3 => {
                        let [vx, vz] = hydrology.velocity(x, z);
                        p.position[0] += vx * DRIFT_SCALE * dt;
                        p.position[2] += vz * DRIFT_SCALE * dt;
                        // Floats: half of it above the surface.
                        p.position[1] = surface.get(x, z);
                        true
                    }
                    _ => false, // drifted onto dry ground or out of the world
                },
                Kind::Spray => {
                    p.velocity[1] -= GRAVITY * dt;
                    for (c, v) in p.position.iter_mut().zip(p.velocity) {
                        *c += v * dt;
                    }
                    // Dies when it falls back below the water or the ground.
                    column(p.position).is_some_and(|(x, z)| {
                        let level = surface.get(x, z).max(floor.get(x, z));
                        p.velocity[1] > 0.0 || p.position[1] > level
                    })
                }
            };
            if alive && p.age < p.lifetime {
                i += 1;
            } else {
                // O(1) removal, order does not matter.
                self.particles.swap_remove(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::hydrology::{Edge, HydrologyParams};

    /// A tilted channel with a spring, run until the water flows.
    fn flowing() -> (Hydrology, Field2, Field2, Vec<Spring>) {
        let (nx, nz) = (12, 24);
        let mut ground = Field2::filled(nx, nz, 0.0);
        for z in 0..nz {
            for x in 0..nx {
                ground.set(x, z, 0.4 * (nz - z) as f32);
            }
        }
        let springs = vec![Spring {
            x: 6,
            z: 2,
            rate: 4.0,
        }];
        let params = HydrologyParams::reference().without_erosion();
        let mut hydrology = Hydrology::new(ground, params, Edge::Open, &springs);
        for _ in 0..2000 {
            hydrology.step();
        }
        let mut floor = Field2::filled(nx, nz, 0.0);
        let mut surface = Field2::filled(nx, nz, 0.0);
        crate::terrain::water_surfaces(
            hydrology.ground(),
            hydrology.water(),
            32,
            &mut floor,
            &mut surface,
        );
        (hydrology, floor, surface, springs)
    }

    #[test]
    fn particles_appear_and_stay_bounded() {
        let (hydrology, floor, surface, springs) = flowing();
        let mut particles = WaterParticles::new(1);
        for _ in 0..600 {
            particles.update(1.0 / 60.0, &hydrology, &floor, &surface, &springs);
            assert!(particles.len() <= MAX_PARTICLES);
        }
        assert!(particles.len() > 0, "no particle at all");
        assert_eq!(particles.instances().len(), particles.len());
        assert!(
            particles
                .instances()
                .iter()
                .all(|p| p.centre_size.iter().all(|c| c.is_finite()))
        );
    }

    #[test]
    fn same_seed_same_particles() {
        let (hydrology, floor, surface, springs) = flowing();
        let run = || {
            let mut particles = WaterParticles::new(5);
            for _ in 0..120 {
                particles.update(1.0 / 60.0, &hydrology, &floor, &surface, &springs);
            }
            particles.instances().to_vec()
        };
        assert_eq!(run(), run());
    }
}
