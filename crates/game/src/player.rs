//! The player's body: walking, running, jumping, swimming, colliding with the world, and what
//! the animation reads from it. Movements are smoothed everywhere (exponential easing), so
//! starting, stopping, turning and stepping up never jerk.

use glam::{Vec2, Vec3};
use sim::rng::SplitMix64;
use world::{Biome, Material, World};

use crate::naturalist::Motion;
use crate::obstacles::{Aabb, Obstacles};

/// Horizontal speeds, in cells per second.
const WALK_SPEED: f32 = 3.2;
const RUN_SPEED: f32 = 5.8;
/// How fast the velocity reaches its target (1/s): on the ground, and when airborne.
const GROUND_EASING: f32 = 9.0;
const AIR_EASING: f32 = 2.0;
/// How fast the body turns towards where it goes (1/s).
const TURN_EASING: f32 = 10.0;
const GRAVITY: f32 = 22.0;
/// Initial upward speed of a jump: about 1.2 cells high.
const JUMP_SPEED: f32 = 7.2;
/// Body box: half its width, and its height, in cells.
const HALF_WIDTH: f32 = 0.3;
const HEIGHT: f32 = 1.8;
/// Highest step climbed without jumping.
const STEP_HEIGHT: f32 = 1.05;
/// Walking crouched: slow and low.
const CROUCH_SPEED: f32 = 1.3;
/// In the shape of a deer (the spell): faster, and a deer's leap.
const DEER_WALK: f32 = 4.0;
const DEER_RUN: f32 = 10.5;
const DEER_JUMP: f32 = 10.0;
/// Speed factor in water.
const WADING: f32 = 0.55;
/// The body starts swimming beyond this water depth (at the feet) and stops below the second
/// one: the gap (hysteresis) keeps it from flickering between swimming and sinking.
const SWIM_START: f32 = 1.3;
const SWIM_STOP: f32 = 0.8;
/// Depth of the feet while floating: head and shoulders out of the water.
const FLOAT_DEPTH: f32 = 1.0;
/// Highest bank a swimmer can pull himself onto.
const CLIMB_OUT: f32 = 1.6;
/// Upward speed of a kick while swimming (Space).
const SWIM_KICK: f32 = 4.0;
/// Radians of the walking cycle per cell travelled: one stride (two steps) every 1.6 cells.
const STRIDE_PER_CELL: f32 = std::f32::consts::TAU / 1.6;
/// Longest physics step: larger frames are split, so collisions never let the body through.
const MAX_STEP: f32 = 1.0 / 120.0;

/// What the player asks for this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub run: bool,
    pub jump: bool,
    /// Rubbing a fire drill (held).
    pub rub: bool,
    /// Crouching (held): slow, low and quiet.
    pub crouch: bool,
}

pub struct Player {
    /// Feet (centre of the bottom of the body box), in world cells.
    pub position: Vec3,
    /// Speed factor from the body's state: 1 fine, less when hungry, cold or sick.
    pub pace: f32,
    velocity: Vec3,
    facing: f32,
    on_ground: bool,
    in_water: bool,
    swimming: bool,
    /// Displayed height of the feet: follows `position.y` smoothly, so climbing a step is a
    /// quick lift and not a jump cut.
    shown_y: f32,
    stride_phase: f32,
    stride: f32,
    lean: f32,
    look: [f32; 2],
    look_target: [f32; 2],
    next_glance: f32,
    rng: SplitMix64,
    /// How crouched, 0 (standing) to 1 (eased).
    crouch: f32,
    /// In the shape of a deer (the spell).
    pub deer: bool,
}

impl Player {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            pace: 1.0,
            velocity: Vec3::ZERO,
            facing: 0.0,
            on_ground: false,
            in_water: false,
            swimming: false,
            shown_y: position.y,
            stride_phase: 0.0,
            stride: 0.0,
            lean: 0.0,
            look: [0.0; 2],
            look_target: [0.0; 2],
            next_glance: 2.0,
            rng: SplitMix64::new(0x10c),
            crouch: 0.0,
            deer: false,
        }
    }

    /// Direction the body faces, radians around the vertical (0 = +z).
    pub fn facing(&self) -> f32 {
        self.facing
    }

    pub fn in_water(&self) -> bool {
        self.in_water
    }

    pub fn on_ground(&self) -> bool {
        self.on_ground
    }

    /// Phase of the walking cycle (radians) and how strongly the legs swing (0 to 1).
    pub fn stride(&self) -> (f32, f32) {
        (self.stride_phase, self.stride)
    }

    /// How crouched, 0 to 1.
    pub fn crouched(&self) -> f32 {
        self.crouch
    }

    /// Current velocity, in cells per second.
    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }

    /// Where the feet are drawn.
    pub fn shown_position(&self) -> Vec3 {
        Vec3::new(self.position.x, self.shown_y, self.position.z)
    }

    /// Advances by `dt` seconds. `camera_yaw` orients the controls: "forward" goes away from the
    /// camera.
    pub fn update(
        &mut self,
        controls: &Controls,
        camera_yaw: f32,
        dt: f32,
        world: &World,
        obstacles: &Obstacles,
    ) {
        let mut remaining = dt;
        let mut jump = controls.jump;
        while remaining > 0.0 {
            let step = remaining.min(MAX_STEP);
            self.physics(controls, jump, camera_yaw, step, world, obstacles);
            jump = false;
            remaining -= step;
        }
        self.animate(dt);
        let crouch = if controls.crouch && !self.swimming && !self.deer {
            1.0
        } else {
            0.0
        };
        self.crouch += (crouch - self.crouch) * (1.0 - (-8.0 * dt).exp());
    }

    fn physics(
        &mut self,
        c: &Controls,
        jump: bool,
        camera_yaw: f32,
        dt: f32,
        world: &World,
        obstacles: &Obstacles,
    ) {
        // Controls relative to the camera: it looks from (sin yaw, ·, cos yaw) towards its
        // target, so "forward" is the opposite horizontal direction.
        let forward = Vec2::new(-camera_yaw.sin(), -camera_yaw.cos());
        let right = Vec2::new(-forward.y, forward.x);
        let axis = |plus: bool, minus: bool| plus as i32 as f32 - minus as i32 as f32;
        let wish = forward * axis(c.forward, c.back) + right * axis(c.right, c.left);
        let wish = wish.normalize_or_zero();

        let water_level = water_surface(world, self.position);
        let depth = water_level.map_or(0.0, |w| w - self.position.y);
        self.in_water = depth > 0.1;
        if depth > SWIM_START {
            self.swimming = true;
        } else if depth < SWIM_STOP || (self.on_ground && depth < SWIM_START) {
            self.swimming = false;
        }
        let swimming = self.swimming;
        let mut speed = match (self.deer, c.run, c.crouch) {
            (true, true, _) => DEER_RUN,
            (true, false, _) => DEER_WALK,
            (false, true, false) => RUN_SPEED,
            (false, _, true) => CROUCH_SPEED,
            (false, false, false) => WALK_SPEED,
        } * self.pace;
        if self.in_water {
            speed *= WADING;
        }

        // Ease the horizontal velocity towards the wished one.
        let target = wish * speed;
        let easing = if self.on_ground || swimming {
            GROUND_EASING
        } else {
            AIR_EASING
        };
        let k = 1.0 - (-easing * dt).exp();
        self.velocity.x += (target.x - self.velocity.x) * k;
        self.velocity.z += (target.y - self.velocity.z) * k;

        // Vertical: gravity, jumping, floating.
        if swimming {
            // Buoyancy pulls the body towards floating height, with damping: a spring.
            let float = water_level.unwrap_or(self.position.y) - FLOAT_DEPTH;
            self.velocity.y += ((float - self.position.y) * 12.0 - self.velocity.y * 4.0) * dt;
            if jump {
                self.velocity.y = SWIM_KICK;
            }
        } else {
            self.velocity.y -= GRAVITY * dt;
            if jump && self.on_ground {
                self.velocity.y = if self.deer { DEER_JUMP } else { JUMP_SPEED };
            }
        }

        // Turn smoothly towards the direction of travel.
        let horizontal = Vec2::new(self.velocity.x, self.velocity.z);
        if horizontal.length() > 0.3 {
            let target_facing = horizontal.x.atan2(horizontal.y);
            let turn = 1.0 - (-TURN_EASING * dt).exp();
            self.facing += wrap_angle(target_facing - self.facing) * turn;
        }

        self.move_and_collide(dt, world, obstacles);
        let ease_y = 1.0 - (-14.0 * dt).exp();
        self.shown_y += (self.position.y - self.shown_y) * ease_y;
        // Falling far (a cliff) is not smoothed: the body would lag behind.
        if (self.position.y - self.shown_y).abs() > 2.0 {
            self.shown_y = self.position.y;
        }
    }

    /// Moves axis by axis; a blocked horizontal move first tries to climb one step.
    fn move_and_collide(&mut self, dt: f32, world: &World, obstacles: &Obstacles) {
        let delta = self.velocity * dt;
        for axis in [0, 2] {
            let mut moved = self.position;
            moved[axis] += delta[axis];
            if !blocked(world, obstacles, moved) {
                self.position = moved;
            } else if self.on_ground || self.swimming {
                // Step up (or, swimming, pull out onto the bank): the same move at the lowest
                // free cell level within reach.
                let reach = if self.swimming {
                    CLIMB_OUT
                } else {
                    STEP_HEIGHT
                };
                let lowest = self.position.y.ceil() as i32;
                let highest = (self.position.y + reach).floor() as i32;
                let free = (lowest..=highest).map(|level| {
                    let mut raised = moved;
                    raised.y = level as f32 + 0.001;
                    raised
                });
                if let Some(raised) = free.into_iter().find(|&r| !blocked(world, obstacles, r)) {
                    self.position = raised;
                    self.swimming = false;
                } else {
                    self.velocity[axis] = 0.0;
                }
            } else {
                self.velocity[axis] = 0.0;
            }
        }
        let mut moved = self.position;
        moved.y += delta.y;
        if blocked(world, obstacles, moved) {
            if delta.y < 0.0 {
                // Landed: rest exactly on top of the cell below.
                self.position.y = moved.y.floor() + 1.0;
                if blocked(world, obstacles, self.position) {
                    self.position.y = (self.position.y + 0.001).ceil();
                }
                self.on_ground = true;
            }
            self.velocity.y = 0.0;
        } else {
            self.position = moved;
            // Still on the ground if something is right below.
            let below = self.position - Vec3::Y * 0.05;
            self.on_ground = blocked(world, obstacles, below);
        }
        // The world closes on itself: past an edge, the other side.
        let (x, z) = world.wrap(self.position.x, self.position.z);
        self.position.x = x;
        self.position.z = z;
    }

    /// Walking cycle, lean, and where the curious naturalist looks.
    fn animate(&mut self, dt: f32) {
        let horizontal = Vec2::new(self.velocity.x, self.velocity.z).length();
        // The cycle advances with the distance walked, so feet do not slide.
        self.stride_phase =
            (self.stride_phase + horizontal * STRIDE_PER_CELL * dt) % std::f32::consts::TAU;
        let target_stride = if self.on_ground || self.in_water {
            (horizontal / RUN_SPEED).min(1.0)
        } else {
            0.2
        };
        let ease = |current: f32, target: f32, rate: f32| {
            current + (target - current) * (1.0 - (-rate * dt).exp())
        };
        self.stride = ease(self.stride, target_stride, 8.0);
        self.lean = ease(self.lean, 0.18 * (horizontal / RUN_SPEED).min(1.0), 6.0);

        // Curiosity: when walking slowly or standing, glance around from time to time.
        self.next_glance -= dt;
        if self.next_glance <= 0.0 {
            let calm = horizontal < WALK_SPEED * 0.6;
            self.look_target = if calm {
                [
                    (self.rng.next_f32() - 0.5) * 1.6,
                    (self.rng.next_f32() - 0.6) * 0.5,
                ]
            } else {
                [0.0, 0.0]
            };
            self.next_glance = 1.5 + 3.0 * self.rng.next_f32();
        }
        self.look = [
            ease(self.look[0], self.look_target[0], 3.0),
            ease(self.look[1], self.look_target[1], 3.0),
        ];
    }

    /// What the animation of the body needs this frame.
    pub fn motion(&self, time: f32) -> Motion {
        Motion {
            position: self.shown_position(),
            facing: self.facing,
            stride_phase: self.stride_phase,
            stride: self.stride,
            lean: self.lean,
            look: self.look,
            airborne: !self.on_ground && !self.in_water,
            time,
            gesture: None,
            crouch: self.crouch,
        }
    }
}

/// Wraps an angle to (−π, π].
fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut a = a % tau;
    if a > std::f32::consts::PI {
        a -= tau;
    } else if a <= -std::f32::consts::PI {
        a += tau;
    }
    a
}

/// What the body cannot walk through: the ground and trunks. Foliage, grass and flowers are
/// soft and let it pass.
fn solid(material: Material) -> bool {
    if !material.is_solid() {
        return false;
    }
    !material.is_plant()
        || matches!(
            material,
            Material::Wood
                | Material::BarkDark
                | Material::BirchBark
                | Material::PalmTrunk
                | Material::DeadWood
                | Material::Cactus
        )
}

/// Whether the body box with its feet at `feet` overlaps anything solid (the world grid or an
/// obstacle such as a stone). Outside the world
/// counts as solid below the ground and empty above.
fn blocked(world: &World, obstacles: &Obstacles, feet: Vec3) -> bool {
    let dims = world.dims();
    let min = feet - Vec3::new(HALF_WIDTH, 0.0, HALF_WIDTH);
    let max = feet + Vec3::new(HALF_WIDTH, HEIGHT, HALF_WIDTH);
    if obstacles.hits(&Aabb { min, max }) {
        return true;
    }
    let range = |lo: f32, hi: f32| (lo.floor() as i64)..=((hi - 1e-4).floor() as i64);
    for z in range(min.z, max.z) {
        for y in range(min.y, max.y) {
            for x in range(min.x, max.x) {
                if y < 0 {
                    return true;
                }
                if (y as usize) >= dims.ny {
                    continue;
                }
                // The world closes on itself: past an edge is the other side.
                let (cx, cz) = world.column(x, z);
                let cy = y as usize;
                // A dug cell: only its remaining micro-voxels are solid.
                if let Some(micro) = world.micro(cx, cy, cz) {
                    // The cell where the box is, not its copy across the edge.
                    if micro_hit(micro, [x, y, z], min, max) {
                        return true;
                    }
                    continue;
                }
                if solid(world.block(cx, cy, cz)) {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether the box [min, max] overlaps a micro-voxel of the brick filling `cell`.
fn micro_hit(micro: &[u8; world::MICRO_CELLS], cell: [i64; 3], min: Vec3, max: Vec3) -> bool {
    let m = world::MICRO as f32;
    let local = |v: f32, c: i64| ((v - c as f32) * m).clamp(0.0, m);
    let range = |lo: f32, hi: f32, c: i64| {
        let (a, b) = (local(lo, c), local(hi, c));
        (a.floor() as usize)..(b.ceil() as usize).min(world::MICRO)
    };
    for mz in range(min.z, max.z, cell[2]) {
        for my in range(min.y, max.y, cell[1]) {
            for mx in range(min.x, max.x, cell[0]) {
                if micro[world::micro_index(mx, my, mz)] != 0 {
                    return true;
                }
            }
        }
    }
    false
}

/// Water surface at the column of `feet`, if there is water there.
fn water_surface(world: &World, feet: Vec3) -> Option<f32> {
    let (x, z) = world.column(feet.x.floor() as i64, feet.z.floor() as i64);
    world.water_level(x, z)
}

/// A good place to start: dry, open ground near the middle of the world, in a welcoming biome
/// (meadow, forest, savanna, beach) if there is one, searched in growing squares from the
/// centre.
pub fn spawn_point(world: &World) -> Vec3 {
    // A meadow or a wood first; else a savanna or a beach; else any land.
    search_spawn(world, |b| matches!(b, Biome::Plains | Biome::Forest))
        .or_else(|| search_spawn(world, |b| matches!(b, Biome::Savanna | Biome::Beach)))
        .or_else(|| search_spawn(world, |b| b != Biome::Ocean))
        .unwrap_or_else(|| {
            let dims = world.dims();
            Vec3::new(dims.nx as f32 / 2.0, dims.ny as f32, dims.nz as f32 / 2.0)
        })
}

fn search_spawn(world: &World, accept: impl Fn(Biome) -> bool) -> Option<Vec3> {
    let dims = world.dims();
    let (cx, cz) = (dims.nx as i64 / 2, dims.nz as i64 / 2);
    for radius in 0..(dims.nx.min(dims.nz) as i64 / 2 - 1) {
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs() != radius && dz.abs() != radius {
                    continue;
                }
                let (x, z) = ((cx + dx) as usize, (cz + dz) as usize);
                if world.water_level(x, z).is_some() || !accept(world.biome(x, z)) {
                    continue;
                }
                let top = world.ground_top(x, z);
                // Soft ground underfoot, not the bare rock of a steep slope.
                let soil = world.block(x, top.saturating_sub(1), z);
                let soft = matches!(
                    soil,
                    Material::Grass | Material::ForestFloor | Material::DryGrass | Material::Sand
                );
                let feet = Vec3::new(x as f32 + 0.5, top as f32 + 0.001, z as f32 + 0.5);
                if soft && !blocked(world, &Obstacles::default(), feet) {
                    return Some(feet);
                }
            }
        }
    }
    None
}

/// A body is saved by where it stands, where it faces and its shape; the rest (velocity,
/// gait, glances) settles again within a step.
impl crate::save::Persist for Player {
    fn write(&self, w: &mut crate::save::Writer) {
        w.put(&self.position);
        w.put(&self.facing);
        w.put(&self.deer);
    }
    fn read(r: &mut crate::save::Reader) -> crate::save::Result<Self> {
        let mut player = Player::new(r.get()?);
        player.facing = r.get()?;
        player.deer = r.get()?;
        Ok(player)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::grid::Dims;
    use world::WorldConfig;

    fn world() -> World {
        World::generate(WorldConfig {
            dims: Dims {
                nx: 96,
                ny: 48,
                nz: 96,
            },
            sea_level: 16.0,
            seed: 3,
        })
    }

    fn settle(player: &mut Player, world: &World, controls: Controls, seconds: f32) {
        let frames = (seconds * 60.0) as usize;
        for _ in 0..frames {
            player.update(&controls, 0.0, 1.0 / 60.0, world, &Obstacles::default());
        }
    }

    #[test]
    fn spawns_on_dry_ground_and_stands_still() {
        let world = world();
        let start = spawn_point(&world);
        let mut player = Player::new(start);
        settle(&mut player, &world, Controls::default(), 2.0);
        assert!(player.on_ground, "not on the ground");
        assert!(
            !blocked(&world, &Obstacles::default(), player.position),
            "inside the ground"
        );
        assert!(
            (player.position - start).length() < 0.1,
            "drifted to {:?}",
            player.position
        );
    }

    #[test]
    fn walking_moves_away_from_the_camera_and_eases_in() {
        let world = world();
        let mut player = Player::new(spawn_point(&world));
        settle(&mut player, &world, Controls::default(), 1.0);
        let start = player.position;
        let forward = Controls {
            forward: true,
            ..Default::default()
        };
        // Camera yaw 0: the camera is on the +z side, so forward is −z.
        player.update(&forward, 0.0, 1.0 / 60.0, &world, &Obstacles::default());
        let first = Vec2::new(player.velocity.x, player.velocity.z).length();
        assert!(first < WALK_SPEED * 0.5, "no easing: {first}");
        settle(&mut player, &world, forward, 1.0);
        assert!(player.position.z < start.z - 1.0, "did not walk forward");
    }

    #[test]
    fn never_ends_inside_the_ground_while_walking_around() {
        let world = world();
        let mut player = Player::new(spawn_point(&world));
        let directions = [
            Controls {
                forward: true,
                run: true,
                ..Default::default()
            },
            Controls {
                right: true,
                jump: true,
                ..Default::default()
            },
            Controls {
                back: true,
                left: true,
                ..Default::default()
            },
        ];
        for (k, controls) in directions.iter().cycle().take(12).enumerate() {
            for _ in 0..90 {
                player.update(
                    controls,
                    k as f32 * 0.7,
                    1.0 / 60.0,
                    &world,
                    &Obstacles::default(),
                );
                assert!(
                    !blocked(&world, &Obstacles::default(), player.position),
                    "stuck in the ground at {:?}",
                    player.position
                );
            }
        }
    }

    /// A small closed world: a pool 3 cells deep in flat ground.
    fn pool_world() -> (World, Vec3) {
        // Find a deep water column in a generated world, away from its edges.
        let world = world();
        let dims = world.dims();
        for z in 10..dims.nz - 10 {
            for x in 10..dims.nx - 10 {
                if let Some(level) = world.water_level(x, z)
                    && level - world.ground_top(x, z) as f32 > 3.0
                {
                    return (
                        world,
                        Vec3::new(x as f32 + 0.5, level + 2.0, z as f32 + 0.5),
                    );
                }
            }
        }
        panic!("no deep water in the test world");
    }

    #[test]
    fn swimmer_floats_with_the_head_out_and_does_not_sink() {
        let (world, above) = pool_world();
        let level = water_surface(&world, above).unwrap();
        let mut player = Player::new(above);
        // Falls in, then floats.
        settle(&mut player, &world, Controls::default(), 6.0);
        assert!(player.swimming, "not swimming");
        let depth = level - player.position.y;
        assert!(
            (depth - FLOAT_DEPTH).abs() < 0.3,
            "feet {depth} below the surface"
        );
        // Still floating a while later, while paddling around.
        let paddle = Controls {
            forward: true,
            ..Default::default()
        };
        for _ in 0..240 {
            player.update(&paddle, 0.0, 1.0 / 60.0, &world, &Obstacles::default());
            let surface = water_surface(&world, player.position);
            if let Some(surface) = surface {
                assert!(surface - player.position.y < SWIM_START + 0.3, "sank");
            }
        }
    }

    #[test]
    fn swimmer_climbs_out_onto_the_nearest_bank() {
        let (world, above) = pool_world();
        let mut player = Player::new(above);
        settle(&mut player, &world, Controls::default(), 4.0);
        // Nearest dry column.
        let dims = world.dims();
        let (px, pz) = (player.position.x, player.position.z);
        let mut best: Option<(f32, f32, f32)> = None;
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                if world.water_level(x, z).is_none() {
                    let (dx, dz) = (x as f32 + 0.5 - px, z as f32 + 0.5 - pz);
                    let d = dx * dx + dz * dz;
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, dx, dz));
                    }
                }
            }
        }
        let (_, dx, dz) = best.expect("no dry land");
        // Forward is (−sin yaw, −cos yaw): aim it at the bank.
        let yaw = (-dx).atan2(-dz);
        let towards = Controls {
            forward: true,
            ..Default::default()
        };
        for _ in 0..60 * 30 {
            player.update(&towards, yaw, 1.0 / 60.0, &world, &Obstacles::default());
            if !player.swimming
                && player.on_ground
                && water_surface(&world, player.position).is_none()
            {
                return;
            }
        }
        panic!("still in the water at {:?}", player.position);
    }

    #[test]
    fn angles_wrap() {
        assert!((wrap_angle(3.0 * std::f32::consts::PI) - std::f32::consts::PI).abs() < 1e-5);
        assert!((wrap_angle(-0.5) + 0.5).abs() < 1e-6);
    }

    /// The world closes on itself: walking west past its edge, one comes out on its east side,
    /// on the ground, and walks on.
    #[test]
    fn walking_off_an_edge_comes_back_from_the_other() {
        let world = world();
        let (nx, nz) = (world.dims().nx, world.dims().nz);
        // A row where the ground is dry and gentle across the edge.
        let columns: Vec<usize> = (nx - 10..nx).chain(0..4).collect();
        let z = (0..nz)
            .find(|&z| {
                columns.iter().all(|&x| world.water_level(x, z).is_none())
                    && columns.windows(2).all(|w| {
                        (world.ground_top(w[0], z) as i64 - world.ground_top(w[1], z) as i64).abs()
                            <= 1
                    })
            })
            .expect("a dry row across the edge");
        let top = world.ground_top(1, z) as f32;
        let mut player = Player::new(Vec3::new(1.5, top + 0.001, z as f32 + 0.5));
        let west = Controls {
            left: true,
            ..Controls::default()
        };
        settle(&mut player, &world, west, 2.0);
        let x = player.position.x;
        assert!(x > nx as f32 - 8.0 && x < nx as f32, "x = {x}");
        assert!((player.position.y - world.ground_top(x as usize, z) as f32).abs() < 1.2);
    }
}
