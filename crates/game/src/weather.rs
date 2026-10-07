//! Weather: morning mist lying in the hollows, and showers of rain that wet the ground.
//!
//! Rain comes in episodes: while dry, a shower may start at any moment (a constant chance per
//! second, so the waits between showers are random, exponentially distributed); it lasts a few
//! minutes, swelling and dying away. The ground gets wet quickly under the rain and dries
//! slowly after. Mist comes at dawn, thicker some days than others, and after rain.
//!
//! Below freezing, rain falls as snow: flakes drifting down slowly, a cover that builds up on
//! the ground and melts in the thaw. Water freezes after hours of frost and thaws after hours
//! above it.

use glam::{Mat4, Vec3};
use render::mesh::MeshData;
use render::mesher::mesh_materials;
use render::palette::srgb_hex;
use render::sky::Sky;
use render::{PartId, PartInstance, ParticleInstance, Renderer};
use sim::grid::Dims;
use sim::rng::SplitMix64;
use world::{Material, World};

/// Chance per real second that a shower starts while dry: one every ~8 minutes on average.
const SHOWER_RATE: f32 = 1.0 / 480.0;
/// How long a shower lasts, in real seconds: between these.
const SHOWER_SHORTEST: f32 = 90.0;
const SHOWER_LONGEST: f32 = 240.0;
/// Seconds to get soaked and to dry.
const WETTING: f32 = 20.0;
const DRYING: f32 = 150.0;
/// Rain drops falling around the player at full rain, within this radius, at this speed.
const DROPS: usize = 450;
const DROP_RADIUS: f32 = 13.0;
const FALL_SPEED: f32 = 14.0;
const DROP_VOXEL: f32 = 0.022;
/// Snow falls slower, and wobbles.
const SNOW_FALL_SPEED: f32 = 1.3;
/// Game hours of snowfall for a full white cover; of thaw (per °C above freezing) to melt it.
const SNOW_BUILD_HOURS: f32 = 3.0;
const SNOW_MELT_HOURS: f32 = 12.0;
/// Game hours of frost (below −1 °C) to freeze the water over, and of thaw to free it.
const ICE_BUILD_HOURS: f32 = 10.0;
const ICE_MELT_HOURS: f32 = 8.0;

struct RainDrop {
    position: Vec3,
}

struct Splash {
    position: Vec3,
    age: f32,
}

pub struct Weather {
    /// Direction the wind blows towards (x, z), unit: set each frame (see `wind.rs`).
    pub wind: [f32; 2],
    rng: SplitMix64,
    /// Rain now, in [0, 1], and seconds left in the current shower (0 when dry).
    rain: f32,
    shower_left: f32,
    /// Forced by the player (key R), for testing.
    pub forced: bool,
    wet: f32,
    /// How misty mornings are today, in [0, 1], drawn anew each day.
    mistiness: f32,
    last_hour: f32,
    drops: Vec<RainDrop>,
    splashes: Vec<Splash>,
    particles: Vec<ParticleInstance>,
    drop_mesh: Option<PartId>,
    splash_color: [f32; 3],
    /// Air temperature where the player is (°C): set each frame.
    pub temperature: f32,
    /// Snow lying on the ground, and ice on the water, in [0, 1].
    snow: f32,
    ice: f32,
}

impl Weather {
    pub fn new(seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let mistiness = rng.next_f32();
        Self {
            wind: [0.89, 0.45],
            rng,
            rain: 0.0,
            shower_left: 0.0,
            forced: false,
            wet: 0.0,
            mistiness,
            last_hour: 0.0,
            drops: Vec::with_capacity(DROPS),
            splashes: Vec::new(),
            particles: Vec::new(),
            drop_mesh: None,
            splash_color: srgb_hex(0xdfe8ee),
            temperature: 15.0,
            snow: 0.0,
            ice: 0.0,
        }
    }

    /// Starts raining at once, as if a shower had come (`--weather rain`).
    pub fn start_shower(&mut self) {
        self.shower_left = SHOWER_LONGEST;
        self.rain = 1.0;
        self.wet = 1.0;
    }

    pub fn rain(&self) -> f32 {
        self.rain
    }

    /// Whether it snows rather than rains.
    pub fn snowing(&self) -> bool {
        self.rain > 0.05 && self.temperature < 1.0
    }

    #[cfg(test)]
    pub fn snow(&self) -> f32 {
        self.snow
    }

    #[cfg(test)]
    pub fn ice(&self) -> f32 {
        self.ice
    }

    /// Lays snow and ice at once (captures).
    pub fn set_cover(&mut self, snow: f32, ice: f32) {
        self.snow = snow.clamp(0.0, 1.0);
        self.ice = ice.clamp(0.0, 1.0);
    }

    /// Snow and ice over `hours` of game time (sped up with the day).
    pub fn seasons(&mut self, hours: f32) {
        let t = self.temperature;
        if self.snowing() {
            self.snow += hours * self.rain / SNOW_BUILD_HOURS;
        } else if t > 1.0 {
            self.snow -= hours * (t - 1.0) / SNOW_MELT_HOURS;
        }
        self.snow = self.snow.clamp(0.0, 1.0);
        if t < -1.0 {
            self.ice += hours / ICE_BUILD_HOURS;
        } else if t > 1.0 {
            self.ice -= hours / ICE_MELT_HOURS;
        }
        self.ice = self.ice.clamp(0.0, 1.0);
    }

    pub fn wet(&self) -> f32 {
        self.wet
    }

    /// Mist at `hour`: at dawn, as thick as today's mood, more after rain.
    pub fn mist(&self, hour: f32) -> f32 {
        let dawn = (-((hour - 6.3) / 1.4).powi(2)).exp();
        (dawn * (0.25 + 0.75 * self.mistiness) + 0.2 * self.wet + 0.1 * self.rain).min(1.0)
    }

    pub fn update(&mut self, dt: f32, hour: f32, player: Vec3, world: &World) {
        // A new day: a new mood for the mist.
        if hour < self.last_hour {
            self.mistiness = self.rng.next_f32();
        }
        self.last_hour = hour;

        if self.shower_left > 0.0 {
            self.shower_left -= dt;
        } else if self.rng.next_f32() < SHOWER_RATE * dt {
            self.shower_left =
                SHOWER_SHORTEST + (SHOWER_LONGEST - SHOWER_SHORTEST) * self.rng.next_f32();
        }
        let raining = self.forced || self.shower_left > 0.0;
        // Swell and die away over ~15 seconds.
        let target = if raining { 1.0 } else { 0.0 };
        self.rain += (target - self.rain) * (1.0 - (-dt / 15.0).exp());
        // Snow does not wet the ground (it lies on it).
        if self.rain > 0.2 && !self.snowing() {
            self.wet = (self.wet + dt / WETTING * self.rain).min(1.0);
        } else {
            self.wet = (self.wet - dt / DRYING).max(0.0);
        }
        self.update_drops(dt, player, world);
    }

    fn update_drops(&mut self, dt: f32, player: Vec3, world: &World) {
        let wanted = (DROPS as f32 * self.rain) as usize;
        let dims = world.dims();
        while self.drops.len() < wanted {
            let angle = std::f32::consts::TAU * self.rng.next_f32();
            let r = DROP_RADIUS * self.rng.next_f32().sqrt();
            let height = 4.0 + 10.0 * self.rng.next_f32();
            self.drops.push(RainDrop {
                position: player + Vec3::new(angle.cos() * r, height, angle.sin() * r),
            });
        }
        self.drops.truncate(wanted);
        let wind = self.wind;
        let snowing = self.snowing();
        let (speed, drift) = if snowing {
            (SNOW_FALL_SPEED, 0.8)
        } else {
            (FALL_SPEED, 2.5)
        };
        let fall = Vec3::new(wind[0] * drift, -speed, wind[1] * drift) * dt;
        for i in 0..self.drops.len() {
            self.drops[i].position += fall;
            if snowing {
                // Flakes wobble as they fall.
                let p = self.drops[i].position;
                let phase = p.y * 1.7 + i as f32;
                self.drops[i].position.x += 0.4 * phase.sin() * dt;
                self.drops[i].position.z += 0.4 * (phase * 1.3).cos() * dt;
            }
            let p = self.drops[i].position;
            let inside = p.x >= 0.0 && p.z >= 0.0 && p.x < dims.nx as f32 && p.z < dims.nz as f32;
            let floor = if inside {
                let (x, z) = (p.x as usize, p.z as usize);
                world
                    .water_level(x, z)
                    .unwrap_or(world.ground_top(x, z) as f32)
            } else {
                f32::NEG_INFINITY
            };
            let gone = p.y <= floor || p.y < player.y - 6.0;
            if gone {
                if inside && !snowing && self.splashes.len() < 200 {
                    self.splashes.push(Splash {
                        position: Vec3::new(p.x, floor + 0.03, p.z),
                        age: 0.0,
                    });
                }
                // Back up into the sky somewhere around the player.
                let angle = std::f32::consts::TAU * self.rng.next_f32();
                let r = DROP_RADIUS * self.rng.next_f32().sqrt();
                self.drops[i].position = player
                    + Vec3::new(
                        angle.cos() * r,
                        10.0 + 4.0 * self.rng.next_f32(),
                        angle.sin() * r,
                    );
            }
        }
        for s in &mut self.splashes {
            s.age += dt;
        }
        self.splashes.retain(|s| s.age < 0.15);
        self.particles.clear();
        if snowing {
            for d in &self.drops {
                self.particles.push(ParticleInstance {
                    centre_size: [d.position.x, d.position.y, d.position.z, 0.04],
                    color: [0.95, 0.96, 0.98, 0.4],
                });
            }
        }
        let [r, g, b] = self.splash_color;
        for s in &self.splashes {
            let size = 0.07 * (1.0 - s.age / 0.15);
            self.particles.push(ParticleInstance {
                centre_size: [s.position.x, s.position.y, s.position.z, size],
                color: [r, g, b, 0.0],
            });
        }
    }

    /// Clouds over the sun: dimmer, greyer light, thicker haze; and the season's look, with
    /// the snow and the ice.
    pub fn apply(&self, sky: &mut Sky, hour: f32, look: crate::season::Look) {
        sky.season = [look.autumn, look.bare, self.snow, self.ice];
        sky.dry = look.dry * (1.0 - self.snow);
        let rain = self.rain;
        let a = &mut sky.atmosphere;
        a.sun_color = a.sun_color.map(|c| c * (1.0 - 0.7 * rain));
        let grey = |c: [f32; 3]| {
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            c.map(|v| v + (l * 1.05 - v) * 0.6 * rain)
        };
        a.sky_color = grey(a.sky_color);
        a.fog_color = grey(a.fog_color);
        a.fog_max = (a.fog_max + 0.12 * rain + 0.08 * self.mist(hour)).min(0.7);
        sky.grade.saturation *= 1.0 - 0.2 * rain;
        // No sunbeam dust under the clouds (the post pass dims it with `night`).
        sky.night = sky.night.max(rain);
    }

    /// Splashes, to draw with the other particles.
    pub fn particles(&self) -> &[ParticleInstance] {
        &self.particles
    }

    pub fn install(&mut self, renderer: &mut Renderer) {
        // A drop: a thin vertical streak (motion blur of a falling drop).
        let dims = Dims {
            nx: 1,
            ny: 9,
            nz: 1,
        };
        let voxels = vec![Material::FlowerWhite.id(); dims.len()];
        let mut mesh = MeshData::default();
        mesh_materials(&voxels, dims, [0.5, 4.5, 0.5], DROP_VOXEL, 0xd0, &mut mesh);
        self.drop_mesh = Some(renderer.add_part(&mesh));
    }

    pub fn draw(&self, renderer: &mut Renderer) {
        let wind = self.wind;
        let Some(id) = self.drop_mesh else {
            return;
        };
        // Leaning with the wind: the streak follows the drop's path.
        let lean = Mat4::from_rotation_z(-wind[0] * 0.17) * Mat4::from_rotation_x(wind[1] * 0.17);
        // Snowflakes are particles; rain, streaks.
        let instances: Vec<PartInstance> = if self.snowing() {
            Vec::new()
        } else {
            self.drops
                .iter()
                .map(|d| PartInstance::new(Mat4::from_translation(d.position) * lean))
                .collect()
        };
        renderer.set_part_instances(id, &instances);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    #[test]
    fn a_shower_wets_the_ground_which_then_dries() {
        let world = World::generate(WorldConfig::small(6));
        let player = crate::player::spawn_point(&world);
        let mut w = Weather::new(1);
        w.forced = true;
        for _ in 0..60 * 60 {
            w.update(1.0 / 60.0, 12.0, player, &world);
        }
        assert!(w.rain() > 0.9 && w.wet() > 0.9);
        assert!(w.drops.len() > DROPS / 2);
        w.forced = false;
        w.shower_left = 0.0;
        // No new shower for this test: watch it dry.
        for _ in 0..60 * 400 {
            w.update(1.0 / 60.0, 12.0, player, &world);
            w.shower_left = 0.0;
        }
        assert!(
            w.rain() < 0.05 && w.wet() < 0.05,
            "rain {} wet {}",
            w.rain(),
            w.wet()
        );
        assert!(w.drops.is_empty());
    }

    #[test]
    fn below_freezing_it_snows_the_snow_lies_then_melts_and_water_freezes() {
        let mut w = Weather::new(3);
        w.forced = true;
        w.rain = 1.0;
        w.temperature = -4.0;
        assert!(w.snowing());
        w.seasons(12.0);
        assert!(w.snow() > 0.99 && w.ice() > 0.99);
        w.forced = false;
        w.rain = 0.0;
        w.temperature = 6.0;
        w.seasons(6.0);
        assert!(w.snow() < 0.01 && w.ice() < 0.3, "{} {}", w.snow(), w.ice());
    }

    #[test]
    fn mist_comes_at_dawn_not_at_noon() {
        let w = Weather::new(2);
        assert!(w.mist(6.3) > w.mist(13.0) + 0.2);
    }
}
