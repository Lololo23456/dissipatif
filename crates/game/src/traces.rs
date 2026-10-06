//! What the naturalist leaves behind: ripples spreading in the water, footprints in sand and
//! snow that fade, a few droplets when striding through water, dust kicked up when running on
//! dry ground. Purely visual. Ripples and footprints are drawn by the shaders
//! (`render::marks`); droplets and dust are particles. Driven by the footsteps (see
//! `listen::surface`).

use glam::Vec3;
use render::ParticleInstance;
use render::marks::{MAX_PRINTS, MAX_RIPPLES, Print, Ripple};
use render::palette::srgb_hex;
use sim::rng::SplitMix64;

use crate::sound::Surface;

const MAX_DROPS: usize = 60;
const MAX_PUFFS: usize = 80;
/// Seconds a footprint stays (as `PRINT_LIFE` in voxel.wgsl), and a ripple (`RIPPLE_LIFE`).
const PRINT_LIFE: f32 = 40.0;
const RIPPLE_LIFE: f32 = 3.0;
const GRAVITY: f32 = 12.0;

#[derive(Clone, Copy)]
struct Drop {
    position: Vec3,
    velocity: Vec3,
    age: f32,
}

#[derive(Clone, Copy)]
struct Puff {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    life: f32,
}

pub struct Traces {
    rng: SplitMix64,
    drops: Vec<Drop>,
    puffs: Vec<Puff>,
    ripples: Vec<Ripple>,
    prints: Vec<Print>,
    /// Seconds since start, the renderer's clock: marks are timed with it.
    now: f32,
    /// Alternates left and right foot.
    left_foot: bool,
    instances: Vec<ParticleInstance>,
    water: [f32; 3],
    dust: [f32; 3],
}

impl Traces {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix64::new(seed),
            drops: Vec::with_capacity(MAX_DROPS),
            puffs: Vec::with_capacity(MAX_PUFFS),
            ripples: Vec::with_capacity(MAX_RIPPLES),
            prints: Vec::with_capacity(MAX_PRINTS),
            now: 0.0,
            left_foot: false,
            instances: Vec::new(),
            water: srgb_hex(0xd8ecf2),
            dust: srgb_hex(0xd8c4a0),
        }
    }

    fn unit(&mut self) -> f32 {
        self.rng.next_f32()
    }

    fn ripple(&mut self, at: Vec3, strength: f32) {
        if self.ripples.len() >= MAX_RIPPLES {
            self.ripples.remove(0);
        }
        self.ripples.push(Ripple {
            x: at.x,
            z: at.z,
            start: self.now,
            strength,
        });
    }

    /// A footstep at `feet` on `surface`. `facing` (radians, 0 = +z) places the foot left or
    /// right of the body's line and orients the print; `strength` 0.3 walking to 1 running;
    /// `water_level` is the surface height when in water; `soft`: sand or snow, prints stay.
    pub fn step(
        &mut self,
        feet: Vec3,
        facing: f32,
        surface: Surface,
        strength: f32,
        water_level: Option<f32>,
        soft: bool,
    ) {
        self.left_foot = !self.left_foot;
        let side = if self.left_foot { 0.12 } else { -0.12 };
        let across = Vec3::new(facing.cos(), 0.0, -facing.sin());
        let foot = feet + across * side;
        match (surface, water_level) {
            (Surface::Water, Some(level)) => {
                let at = Vec3::new(foot.x, level, foot.z);
                self.ripple(at, 0.6 + 0.4 * strength);
                // A few droplets, only when striding hard.
                let count = (8.0 * (strength - 0.4)).max(0.0) as usize;
                for _ in 0..count {
                    if self.drops.len() >= MAX_DROPS {
                        break;
                    }
                    let angle = std::f32::consts::TAU * self.unit();
                    let out = 0.4 + 0.8 * self.unit();
                    let up = 1.2 + 1.8 * self.unit() * strength;
                    self.drops.push(Drop {
                        position: at + Vec3::Y * 0.05,
                        velocity: Vec3::new(angle.cos() * out, up, angle.sin() * out),
                        age: 0.0,
                    });
                }
            }
            _ => {
                if soft {
                    if self.prints.len() >= MAX_PRINTS {
                        self.prints.remove(0);
                    }
                    self.prints.push(Print {
                        x: foot.x,
                        z: foot.z,
                        facing,
                        start: self.now,
                    });
                }
                // Running on dry ground kicks up a little dust.
                if strength > 0.75 && matches!(surface, Surface::Sand | Surface::Grass) {
                    let count = if surface == Surface::Sand { 6 } else { 2 };
                    for _ in 0..count {
                        if self.puffs.len() >= MAX_PUFFS {
                            break;
                        }
                        let angle = std::f32::consts::TAU * self.unit();
                        let life = 0.8 + 0.8 * self.unit();
                        let rise = 0.3 + 0.4 * self.unit();
                        self.puffs.push(Puff {
                            position: foot + Vec3::Y * 0.1,
                            velocity: Vec3::new(angle.cos() * 0.4, rise, angle.sin() * 0.4),
                            age: 0.0,
                            life,
                        });
                    }
                }
            }
        }
    }

    /// While swimming, the body pushes a small ripple around itself now and then. `at` on the
    /// water surface.
    pub fn swim(&mut self, at: Vec3, dt: f32) {
        if self.unit() < dt * 2.0 {
            self.ripple(at, 0.5);
        }
    }

    /// Ripples and footprints, for `Renderer::set_marks`.
    pub fn marks(&self) -> (&[Ripple], &[Print]) {
        (&self.ripples, &self.prints)
    }

    /// Advances by `dt`; `now` is the renderer's clock (seconds since start).
    pub fn update(&mut self, dt: f32, now: f32) {
        self.now = now;
        for d in &mut self.drops {
            d.age += dt;
            d.velocity.y -= GRAVITY * dt;
            d.position += d.velocity * dt;
        }
        self.drops.retain(|d| d.age < 0.9 && d.velocity.y > -6.0);
        self.ripples.retain(|r| now - r.start < RIPPLE_LIFE);
        self.prints.retain(|p| now - p.start < PRINT_LIFE);
        for p in &mut self.puffs {
            p.age += dt;
            p.position += p.velocity * dt;
            p.velocity *= 1.0 - 1.5 * dt;
        }
        self.puffs.retain(|p| p.age < p.life);

        self.instances.clear();
        let [wr, wg, wb] = self.water;
        for d in &self.drops {
            self.instances.push(ParticleInstance {
                centre_size: [d.position.x, d.position.y, d.position.z, 0.035],
                color: [wr, wg, wb, 0.6],
            });
        }
        let [dr, dg, db] = self.dust;
        for p in &self.puffs {
            let t = p.age / p.life;
            self.instances.push(ParticleInstance {
                centre_size: [p.position.x, p.position.y, p.position.z, 0.09 * (1.0 - t)],
                color: [dr, dg, db, 0.0],
            });
        }
    }

    pub fn instances(&self) -> &[ParticleInstance] {
        &self.instances
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_in_water_ripples_then_settles() {
        let mut t = Traces::new(1);
        t.update(0.0, 10.0);
        t.step(
            Vec3::new(5.0, 3.0, 5.0),
            0.0,
            Surface::Water,
            1.0,
            Some(3.4),
            false,
        );
        t.update(1.0 / 60.0, 10.0 + 1.0 / 60.0);
        assert_eq!(t.marks().0.len(), 1);
        assert!(!t.drops.is_empty());
        // Longer than a ripple lives.
        for k in 0..60 * 4 {
            t.update(1.0 / 60.0, 10.0 + k as f32 / 60.0);
        }
        assert!(t.drops.is_empty() && t.marks().0.is_empty());
    }

    #[test]
    fn prints_stay_in_sand_alternate_feet_and_fade_away() {
        let mut t = Traces::new(2);
        for k in 0..4 {
            t.step(
                Vec3::new(0.0, 2.0, k as f32),
                0.0,
                Surface::Sand,
                0.5,
                None,
                true,
            );
        }
        t.update(1.0, 1.0);
        let prints = t.marks().1;
        assert_eq!(prints.len(), 4);
        assert!((prints[0].x - prints[1].x).abs() > 0.2, "left, right");
        t.update(1.0, 50.0);
        assert!(t.marks().1.is_empty());
    }
}
