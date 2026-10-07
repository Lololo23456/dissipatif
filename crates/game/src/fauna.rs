//! Small animals around the naturalist: birds hopping on the ground and flying off when
//! approached, butterflies fluttering over the grass, fish in the water, fireflies at night.
//!
//! They live within a radius of the player and are reborn further away when they leave it:
//! the world seems full of life without simulating every animal of the map. Each kind is a few
//! articulated parts (body, wings) drawn by instancing: one mesh per part, one instance per
//! animal. Fireflies are glowing particles. Deterministic: randomness from a seeded generator.

use glam::{Mat4, Vec2, Vec3};
use render::mesh::MeshData;
use render::mesher::mesh_materials;
use render::palette::srgb_hex;
use render::{PartId, PartInstance, ParticleInstance, Renderer};
use sim::grid::Dims;
use sim::rng::SplitMix64;
use world::{Material, World};

/// Animals live within this distance of the player, in cells.
const RADIUS: f32 = 26.0;
const BIRDS: usize = 9;
const BUTTERFLIES: usize = 14;
const FISH: usize = 10;
const FIREFLIES: usize = 70;
/// A bird flies off when the player comes this close.
const FLIGHT_DISTANCE: f32 = 4.5;
const GRAVITY: f32 = 14.0;

#[derive(Clone, Copy, PartialEq)]
enum BirdState {
    Ground,
    Flying,
}

struct Bird {
    position: Vec3,
    velocity: Vec3,
    facing: f32,
    state: BirdState,
    /// Seconds before the next hop (on the ground) or since taking off (flying).
    timer: f32,
    flap: f32,
}

struct Butterfly {
    position: Vec3,
    velocity: Vec3,
    target: Vec3,
    flap: f32,
    colour: usize,
}

struct Fish {
    position: Vec3,
    heading: f32,
    speed: f32,
    wag: f32,
}

struct Firefly {
    position: Vec3,
    velocity: Vec3,
    /// Phase in its blinking cycle (seconds) and the cycle's length.
    blink: f32,
    period: f32,
}

/// Renderer ids of the parts.
struct Meshes {
    bird_body: PartId,
    bird_wing: PartId,
    butterfly_body: PartId,
    butterfly_wings: [PartId; 3],
    fish: PartId,
}

pub struct Fauna {
    rng: SplitMix64,
    birds: Vec<Bird>,
    butterflies: Vec<Butterfly>,
    /// Phase of the year (see `season.rs`): butterflies and fireflies only in the warm
    /// months, fewer birds in winter.
    pub year: f32,
    fish: Vec<Fish>,
    fireflies: Vec<Firefly>,
    meshes: Option<Meshes>,
    /// How dark it is (0 day, 1 night): who is out.
    night: f32,
    particles: Vec<ParticleInstance>,
    firefly_color: [f32; 3],
}

/// Ground height (top of the solid ground, or of the water) at (x, z), and whether it is
/// water. None outside the world.
fn floor_at(world: &World, x: f32, z: f32) -> Option<(f32, bool, Material)> {
    // The world closes on itself.
    let (x, z) = world.wrap(x, z);
    let (xi, zi) = (x as usize, z as usize);
    // `ground_top` is the first empty cell above the ground: the surface block is below it.
    let top = world.ground_top(xi, zi);
    let material = world.block(xi, top.saturating_sub(1), zi);
    match world.water_level(xi, zi) {
        Some(level) => Some((level, true, material)),
        None => Some((top as f32, false, material)),
    }
}

fn grassy(material: Material) -> bool {
    matches!(
        material,
        Material::Grass | Material::ForestFloor | Material::DryGrass
    )
}

impl Fauna {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix64::new(seed),
            birds: Vec::with_capacity(BIRDS),
            butterflies: Vec::with_capacity(BUTTERFLIES),
            year: 0.375,
            fish: Vec::with_capacity(FISH),
            fireflies: Vec::with_capacity(FIREFLIES),
            meshes: None,
            night: 0.0,
            particles: Vec::with_capacity(FIREFLIES),
            firefly_color: srgb_hex(0xd6f26a),
        }
    }

    /// A random point between `near` and `far` cells from `centre`.
    fn around(&mut self, centre: Vec3, near: f32, far: f32) -> (f32, f32) {
        let angle = std::f32::consts::TAU * self.rng.next_f32();
        let r = near + (far - near) * self.rng.next_f32().sqrt();
        (centre.x + angle.cos() * r, centre.z + angle.sin() * r)
    }

    /// `night` and `rain` in [0, 1]: in the dark or the rain, birds and butterflies shelter.
    pub fn update(
        &mut self,
        dt: f32,
        time: f32,
        player: Vec3,
        night: f32,
        rain: f32,
        world: &World,
    ) {
        self.night = night;
        let day = night < 0.6 && rain < 0.4;
        // Birds and butterflies by day, fireflies at night, fish always.
        if day {
            self.update_birds(dt, player, world);
            self.update_butterflies(dt, time, player, world);
        } else {
            self.birds.clear();
            self.butterflies.clear();
        }
        self.update_fish(dt, time, player, world);
        if night > 0.4 && rain < 0.3 {
            self.update_fireflies(dt, time, player, world);
        } else {
            self.fireflies.clear();
        }
        // The season: what is about, and how many (the rest are reborn when it comes).
        use crate::season::Season::*;
        let (birds, butterflies, fireflies) = match crate::season::season(self.year) {
            Spring => (1.0, 0.5, 0.0),
            Summer => (1.0, 1.0, 1.0),
            Autumn => (0.8, 0.2, 0.0),
            Winter => (0.5, 0.0, 0.0),
        };
        self.birds.truncate((BIRDS as f32 * birds) as usize);
        self.butterflies
            .truncate((BUTTERFLIES as f32 * butterflies) as usize);
        self.fireflies
            .truncate((FIREFLIES as f32 * fireflies) as usize);
        self.particles.clear();
        let strength = ((night - 0.4) / 0.4).clamp(0.0, 1.0);
        let [r, g, b] = self.firefly_color;
        for f in &self.fireflies {
            // Blink: a soft flash at the start of each cycle, dark the rest of the time.
            let t = f.blink / 0.6;
            let flash = if t < 1.0 {
                (t * std::f32::consts::PI).sin()
            } else {
                0.0
            };
            if flash <= 0.01 {
                continue;
            }
            self.particles.push(ParticleInstance {
                centre_size: [f.position.x, f.position.y, f.position.z, 0.06],
                // Negative glow: shines by itself, whatever the light (see voxel.wgsl).
                color: [r, g, b, -3.0 * flash * strength],
            });
        }
    }

    fn update_birds(&mut self, dt: f32, player: Vec3, world: &World) {
        while self.birds.len() < BIRDS {
            let (x, z) = self.around(player, 6.0, RADIUS * 0.7);
            let Some((y, water, material)) = floor_at(world, x, z) else {
                break;
            };
            if water || !grassy(material) {
                // Try again next frame elsewhere: birds land on grass.
                break;
            }
            let facing = std::f32::consts::TAU * self.rng.next_f32();
            let timer = 0.5 + 2.0 * self.rng.next_f32();
            self.birds.push(Bird {
                position: Vec3::new(x, y, z),
                velocity: Vec3::ZERO,
                facing,
                state: BirdState::Ground,
                timer,
                flap: 0.0,
            });
        }
        for i in 0..self.birds.len() {
            let hop_direction = std::f32::consts::TAU * self.rng.next_f32();
            let hop_timer = 0.6 + 2.5 * self.rng.next_f32();
            let b = &mut self.birds[i];
            let away = Vec2::new(b.position.x - player.x, b.position.z - player.z);
            match b.state {
                BirdState::Ground => {
                    if away.length() < FLIGHT_DISTANCE {
                        // Startled: off and away, climbing.
                        let out = away.normalize_or(Vec2::X);
                        b.velocity = Vec3::new(out.x * 5.0, 4.0, out.y * 5.0);
                        b.state = BirdState::Flying;
                        b.timer = 0.0;
                    } else {
                        b.timer -= dt;
                        if b.timer <= 0.0 {
                            // A little hop, turning.
                            b.facing = hop_direction;
                            b.velocity = Vec3::new(
                                hop_direction.sin() * 0.8,
                                2.2,
                                hop_direction.cos() * 0.8,
                            );
                            b.timer = hop_timer;
                        }
                    }
                    b.velocity.y -= GRAVITY * dt;
                    b.position += b.velocity * dt;
                    if let Some((y, _, _)) = floor_at(world, b.position.x, b.position.z)
                        && b.position.y <= y
                    {
                        b.position.y = y;
                        b.velocity = Vec3::ZERO;
                    }
                }
                BirdState::Flying => {
                    b.timer += dt;
                    b.flap += dt * 16.0;
                    // Climb to a cruising height, then fly level.
                    let lift = if b.timer < 1.5 { 3.0 } else { 0.3 };
                    b.velocity.y += (lift - b.velocity.y) * dt * 2.0;
                    b.position += b.velocity * dt;
                    let horizontal = Vec2::new(b.velocity.x, b.velocity.z);
                    if horizontal.length() > 0.1 {
                        b.facing = horizontal.x.atan2(horizontal.y);
                    }
                }
            }
        }
        // Gone: flown long enough, or left the area. Reborn elsewhere next frame.
        self.birds.retain(|b| {
            let distance = Vec2::new(b.position.x - player.x, b.position.z - player.z).length();
            distance < RADIUS * 1.3 && !(b.state == BirdState::Flying && b.timer > 7.0)
        });
    }

    fn update_butterflies(&mut self, dt: f32, time: f32, player: Vec3, world: &World) {
        while self.butterflies.len() < BUTTERFLIES {
            let (x, z) = self.around(player, 2.0, RADIUS * 0.6);
            let Some((y, water, material)) = floor_at(world, x, z) else {
                break;
            };
            if water || !grassy(material) {
                break;
            }
            let colour = self.rng.next_below(3);
            let at = Vec3::new(x, y + 0.6, z);
            self.butterflies.push(Butterfly {
                position: at,
                velocity: Vec3::ZERO,
                target: at,
                flap: std::f32::consts::TAU * self.rng.next_f32(),
                colour,
            });
        }
        for i in 0..self.butterflies.len() {
            let (tx, tz) = {
                let p = self.butterflies[i].position;
                self.around(p, 0.5, 3.0)
            };
            let height = 0.3 + 1.2 * self.rng.next_f32();
            let jitter = Vec3::new(
                self.rng.next_f32() - 0.5,
                self.rng.next_f32() - 0.5,
                self.rng.next_f32() - 0.5,
            );
            let b = &mut self.butterflies[i];
            if b.position.distance(b.target) < 0.3
                && let Some((y, water, _)) = floor_at(world, tx, tz)
                && !water
            {
                b.target = Vec3::new(tx, y + height, tz);
            }
            // Erratic flight: drawn to the next flower, shaken by every wing beat.
            let wish = (b.target - b.position).normalize_or_zero() * 1.1;
            b.velocity += (wish - b.velocity) * dt * 2.5 + jitter * dt * 14.0;
            b.velocity.y += (time * 9.0 + b.flap).sin() * dt * 2.0;
            b.position += b.velocity * dt;
            if let Some((y, _, _)) = floor_at(world, b.position.x, b.position.z)
                && b.position.y < y + 0.15
            {
                b.position.y = y + 0.15;
                b.velocity.y = b.velocity.y.abs();
            }
            b.flap += dt * 22.0;
        }
        self.butterflies.retain(|b| {
            Vec2::new(b.position.x - player.x, b.position.z - player.z).length() < RADIUS * 1.2
        });
    }

    fn update_fish(&mut self, dt: f32, time: f32, player: Vec3, world: &World) {
        // A few tries per frame to place missing fish in water deep enough.
        for _ in 0..4 {
            if self.fish.len() >= FISH {
                break;
            }
            let (x, z) = self.around(player, 2.0, RADIUS);
            let Some((level, true, _)) = floor_at(world, x, z) else {
                continue;
            };
            let bottom = world.ground_top(x as usize, z as usize) as f32;
            if level - bottom < 0.8 {
                continue;
            }
            let heading = std::f32::consts::TAU * self.rng.next_f32();
            let speed = 0.6 + 0.8 * self.rng.next_f32();
            self.fish.push(Fish {
                position: Vec3::new(x, (bottom + level) * 0.5, z),
                heading,
                speed,
                wag: std::f32::consts::TAU * self.rng.next_f32(),
            });
        }
        let player_in_water = floor_at(world, player.x, player.z).is_some_and(|f| f.1);
        for i in 0..self.fish.len() {
            let turn = (self.rng.next_f32() - 0.5) * 2.0;
            let f = &mut self.fish[i];
            let away = Vec2::new(f.position.x - player.x, f.position.z - player.z);
            let fleeing = player_in_water && away.length() < 3.5;
            if fleeing {
                f.heading = away.x.atan2(away.y);
            } else {
                f.heading += turn * dt * 1.5 + (time * 0.3 + f.wag).sin() * dt * 0.3;
            }
            let speed = if fleeing { 3.0 } else { f.speed };
            let step = Vec3::new(f.heading.sin(), 0.0, f.heading.cos()) * speed * dt;
            let next = f.position + step;
            // Stay in the water: turn round at the shore.
            match floor_at(world, next.x, next.z) {
                Some((level, true, _))
                    if level - (world.ground_top(next.x as usize, next.z as usize) as f32)
                        > 0.5 =>
                {
                    f.position = next;
                    f.position.y = f.position.y.min(level - 0.25);
                }
                _ => f.heading += std::f32::consts::PI * 0.75,
            }
            f.wag += dt * speed * 9.0;
        }
        self.fish.retain(|f| {
            Vec2::new(f.position.x - player.x, f.position.z - player.z).length() < RADIUS * 1.2
        });
    }

    fn update_fireflies(&mut self, dt: f32, time: f32, player: Vec3, world: &World) {
        while self.fireflies.len() < FIREFLIES {
            let (x, z) = self.around(player, 1.0, RADIUS * 0.8);
            let Some((y, water, material)) = floor_at(world, x, z) else {
                break;
            };
            if !water && !grassy(material) {
                break;
            }
            let period = 2.0 + 3.0 * self.rng.next_f32();
            let blink = period * self.rng.next_f32();
            let height = 0.4 + 1.8 * self.rng.next_f32();
            self.fireflies.push(Firefly {
                position: Vec3::new(x, y + height, z),
                velocity: Vec3::ZERO,
                blink,
                period,
            });
        }
        for (k, f) in self.fireflies.iter_mut().enumerate() {
            let seed = k as f32 * 1.7;
            // Slow, wandering drift.
            let wish = Vec3::new(
                (time * 0.31 + seed).sin(),
                (time * 0.47 + seed * 2.0).sin() * 0.4,
                (time * 0.27 + seed * 3.0).cos(),
            ) * 0.35;
            f.velocity += (wish - f.velocity) * dt;
            f.position += f.velocity * dt;
            f.blink = (f.blink + dt) % f.period;
        }
        self.fireflies.retain(|f| {
            Vec2::new(f.position.x - player.x, f.position.z - player.z).length() < RADIUS
        });
    }

    /// Glowing particles (fireflies), to draw with the ambient ones.
    pub fn particles(&self) -> &[ParticleInstance] {
        &self.particles
    }

    pub fn install(&mut self, renderer: &mut Renderer) {
        let mut add = |grid: &Grid, pivot: [f32; 3], voxel: f32, seed: u64| {
            let mut mesh = MeshData::default();
            mesh_materials(&grid.voxels, grid.dims, pivot, voxel, seed, &mut mesh);
            renderer.add_part(&mesh)
        };
        // Bird: a brown body with a red breast and a dark beak; wings extend along +x from
        // their root.
        let mut body = Grid::new(4, 4, 7);
        body.fill([0, 4], [0, 3], [0, 6], Material::Felt);
        body.fill([1, 3], [3, 4], [3, 6], Material::Felt); // head
        body.fill([1, 3], [0, 2], [4, 6], Material::FlowerRed); // breast
        body.fill([1, 3], [3, 4], [6, 7], Material::Eye); // beak
        body.fill([1, 3], [1, 2], [0, 1], Material::Hair); // tail
        let mut wing = Grid::new(5, 1, 3);
        wing.fill([0, 5], [0, 1], [0, 3], Material::Leather);
        let bird_body = add(&body, [2.0, 0.0, 3.0], BIRD_VOXEL, 0xb1);
        let bird_wing = add(&wing, [0.0, 0.5, 1.5], BIRD_VOXEL, 0xb2);
        // Butterfly: a thin dark body, wings in three colours.
        let mut body = Grid::new(1, 1, 4);
        body.fill([0, 1], [0, 1], [0, 4], Material::Hair);
        let butterfly_body = add(&body, [0.5, 0.5, 2.0], BUTTERFLY_VOXEL, 0xb3);
        let butterfly_wings = [
            Material::FlowerYellow,
            Material::FlowerWhite,
            Material::FlowerViolet,
        ]
        .map(|colour| {
            let mut wing = Grid::new(4, 1, 5);
            wing.fill([0, 4], [0, 1], [0, 5], colour);
            wing.fill([3, 4], [0, 1], [0, 1], Material::Air);
            wing.fill([3, 4], [0, 1], [4, 5], Material::Air);
            wing.set(2, 0, 2, Material::Hair);
            add(&wing, [0.0, 0.5, 2.5], BUTTERFLY_VOXEL, 0xb4)
        });
        // Fish: a silvery spindle with a pale belly and a tail.
        let mut fish = Grid::new(3, 3, 9);
        fish.fill([0, 3], [0, 3], [2, 7], Material::Stone);
        fish.fill([1, 2], [1, 2], [7, 9], Material::Stone);
        fish.fill([0, 3], [0, 1], [3, 7], Material::FlowerWhite);
        fish.fill([1, 2], [0, 3], [0, 2], Material::Gravel);
        let fish = add(&fish, [1.5, 1.5, 4.5], FISH_VOXEL, 0xb5);
        self.meshes = Some(Meshes {
            bird_body,
            bird_wing,
            butterfly_body,
            butterfly_wings,
            fish,
        });
    }

    /// Sends this frame's poses to the renderer.
    pub fn draw(&self, renderer: &mut Renderer) {
        let Some(m) = &self.meshes else {
            return;
        };
        let mut bodies = Vec::with_capacity(self.birds.len());
        let mut wings = Vec::with_capacity(self.birds.len() * 2);
        for b in &self.birds {
            let base = Mat4::from_translation(b.position + Vec3::Y * 0.05)
                * Mat4::from_rotation_y(b.facing);
            bodies.push(PartInstance::new(base));
            // Folded on the ground, beating in flight.
            let angle = match b.state {
                BirdState::Ground => -1.3,
                BirdState::Flying => 0.15 + 0.9 * b.flap.sin(),
            };
            let root = Vec3::new(2.0, 2.5, 0.0) * BIRD_VOXEL;
            wings.push(PartInstance::new(
                base * Mat4::from_translation(root) * Mat4::from_rotation_z(angle),
            ));
            wings.push(PartInstance::new(
                base * Mat4::from_translation(root * Vec3::new(-1.0, 1.0, 1.0))
                    * Mat4::from_rotation_z(-angle)
                    * Mat4::from_rotation_y(std::f32::consts::PI),
            ));
        }
        renderer.set_part_instances(m.bird_body, &bodies);
        renderer.set_part_instances(m.bird_wing, &wings);

        let mut bodies = Vec::with_capacity(self.butterflies.len());
        let mut coloured: [Vec<PartInstance>; 3] = Default::default();
        for b in &self.butterflies {
            let horizontal = Vec2::new(b.velocity.x, b.velocity.z);
            let facing = horizontal.x.atan2(horizontal.y);
            let base = Mat4::from_translation(b.position) * Mat4::from_rotation_y(facing);
            bodies.push(PartInstance::new(base));
            let angle = 0.1 + 1.2 * (0.5 + 0.5 * b.flap.sin());
            coloured[b.colour].push(PartInstance::new(base * Mat4::from_rotation_z(angle)));
            coloured[b.colour].push(PartInstance::new(
                base * Mat4::from_rotation_z(-angle) * Mat4::from_rotation_y(std::f32::consts::PI),
            ));
        }
        renderer.set_part_instances(m.butterfly_body, &bodies);
        for (id, instances) in m.butterfly_wings.iter().zip(&coloured) {
            renderer.set_part_instances(*id, instances);
        }

        let fish: Vec<PartInstance> = self
            .fish
            .iter()
            .map(|f| {
                let wag = f.wag.sin() * 0.25;
                PartInstance::new(
                    Mat4::from_translation(f.position) * Mat4::from_rotation_y(f.heading + wag),
                )
            })
            .collect();
        renderer.set_part_instances(m.fish, &fish);
    }
}

// Larger than life (a robin would be 0.05): seen from above, small animals must stay readable.
const BIRD_VOXEL: f32 = 0.085;
const BUTTERFLY_VOXEL: f32 = 0.055;
const FISH_VOXEL: f32 = 0.05;

/// A small box of voxels being drawn.
struct Grid {
    dims: Dims,
    voxels: Vec<u8>,
}

impl Grid {
    fn new(nx: usize, ny: usize, nz: usize) -> Self {
        let dims = Dims { nx, ny, nz };
        Self {
            dims,
            voxels: vec![Material::Air.id(); dims.len()],
        }
    }

    fn fill(&mut self, x: [usize; 2], y: [usize; 2], z: [usize; 2], material: Material) {
        let d = self.dims;
        for zi in z[0]..z[1].min(d.nz) {
            for yi in y[0]..y[1].min(d.ny) {
                for xi in x[0]..x[1].min(d.nx) {
                    self.voxels[d.index(xi, yi, zi)] = material.id();
                }
            }
        }
    }

    fn set(&mut self, x: usize, y: usize, z: usize, material: Material) {
        self.fill([x, x + 1], [y, y + 1], [z, z + 1], material);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    fn world() -> World {
        World::generate(WorldConfig::small(6))
    }

    #[test]
    fn by_day_birds_and_butterflies_live_around_the_player_and_birds_flee() {
        let world = world();
        let player = crate::player::spawn_point(&world);
        let mut fauna = Fauna::new(3);
        for k in 0..600 {
            fauna.update(1.0 / 60.0, k as f32 / 60.0, player, 0.0, 0.0, &world);
        }
        assert!(!fauna.birds.is_empty() && !fauna.butterflies.is_empty());
        assert!(fauna.fireflies.is_empty());
        // Walk onto a grounded bird: it takes off.
        let Some(bird) = fauna.birds.iter().find(|b| b.state == BirdState::Ground) else {
            panic!("no bird on the ground");
        };
        let next_to = bird.position + Vec3::new(1.0, 0.0, 0.0);
        fauna.update(1.0 / 60.0, 10.0, next_to, 0.0, 0.0, &world);
        assert!(
            fauna
                .birds
                .iter()
                .any(|b| b.state == BirdState::Flying && b.position.distance(next_to) < 2.0)
        );
    }

    #[test]
    fn at_night_fireflies_glow_and_birds_sleep() {
        let world = world();
        let player = crate::player::spawn_point(&world);
        let mut fauna = Fauna::new(4);
        for k in 0..600 {
            fauna.update(1.0 / 60.0, k as f32 / 60.0, player, 1.0, 0.0, &world);
        }
        assert!(fauna.birds.is_empty());
        assert!(fauna.fireflies.len() > FIREFLIES / 2);
        assert!(!fauna.particles().is_empty());
        assert!(fauna.particles().iter().all(|p| p.color[3] < 0.0));
    }

    #[test]
    fn fish_stay_in_the_water() {
        let world = world();
        let player = crate::player::spawn_point(&world);
        let mut fauna = Fauna::new(5);
        for k in 0..1200 {
            fauna.update(1.0 / 60.0, k as f32 / 60.0, player, 0.0, 0.0, &world);
            for f in &fauna.fish {
                let (level, water, _) = floor_at(&world, f.position.x, f.position.z).unwrap();
                assert!(water && f.position.y <= level, "fish out of the water");
            }
        }
    }
}
