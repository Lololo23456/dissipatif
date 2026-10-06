//! Plants pushed aside by the player walking through them, with memory: each touched plant
//! has a bend and a bend velocity, driven by a damped spring. While the body touches it, the
//! plant is pushed along the walk and away from the body; once free, it springs back upright
//! with a small sway, like an elastic stem, instead of following the player around.

use std::collections::HashMap;

use glam::{Vec2, Vec3};

/// Reach of the body among plants, from the feet, in cells: fully pushed, and just touched.
const NEAR: f32 = 0.2;
const REACH: f32 = 0.7;
/// Largest bend: sideways offset of the top per unit of height (≈ 25°).
const MAX_BEND: f32 = 0.45;
/// Spring stiffness (1/s²) and damping (1/s): a quick push, a recovery in about half a second
/// with one small overshoot.
const STIFFNESS: f32 = 90.0;
const DAMPING: f32 = 9.0;
/// Below this bend and bend speed, a free plant is upright again and no longer simulated.
const REST: f32 = 1e-3;
/// Longest step of the spring (stable integration whatever the frame rate).
const MAX_STEP: f32 = 1.0 / 120.0;

/// A plant that can be pushed: where it is, how much it yields (1 grass, less for bushes), and
/// where its instance is in the scene (model group, index in the group).
#[derive(Clone, Copy, Debug)]
pub struct Pliable {
    pub base: Vec3,
    pub yielding: f32,
    pub group: usize,
    pub index: usize,
}

struct State {
    bend: Vec2,
    velocity: Vec2,
}

pub struct Trample {
    plants: Vec<Pliable>,
    /// Plants by column (x, z).
    by_column: HashMap<(i64, i64), Vec<usize>>,
    /// Plants currently bent or moving, by index in `plants`.
    moving: HashMap<usize, State>,
}

impl Trample {
    pub fn new(plants: Vec<Pliable>) -> Self {
        let mut by_column: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, p) in plants.iter().enumerate() {
            let key = (p.base.x.floor() as i64, p.base.z.floor() as i64);
            by_column.entry(key).or_default().push(i);
        }
        Self {
            plants,
            by_column,
            moving: HashMap::new(),
        }
    }

    /// A plant that sprouted: it bends too.
    pub fn add(&mut self, plant: Pliable) {
        let key = (plant.base.x.floor() as i64, plant.base.z.floor() as i64);
        self.by_column
            .entry(key)
            .or_default()
            .push(self.plants.len());
        self.plants.push(plant);
    }

    /// Advances by `dt` seconds with the player's feet at `feet` moving at `velocity`.
    /// Calls `apply(plant, bend)` for every plant whose bend changed.
    pub fn update(
        &mut self,
        dt: f32,
        feet: Vec3,
        velocity: Vec3,
        mut apply: impl FnMut(&Pliable, Vec2),
    ) {
        // Targets of the plants the body touches now.
        let mut targets: HashMap<usize, Vec2> = HashMap::new();
        let walk = Vec2::new(velocity.x, velocity.z);
        let walk_direction = walk.normalize_or_zero();
        let walking = (walk.length() / 3.0).min(1.0);
        let (cx, cz) = (feet.x.floor() as i64, feet.z.floor() as i64);
        for z in cz - 1..=cz + 1 {
            for x in cx - 1..=cx + 1 {
                let Some(list) = self.by_column.get(&(x, z)) else {
                    continue;
                };
                for &i in list {
                    let p = &self.plants[i];
                    if (p.base.y - feet.y).abs() > 1.5 {
                        continue;
                    }
                    let away = Vec2::new(p.base.x - feet.x, p.base.z - feet.z);
                    let distance = away.length();
                    if distance > REACH {
                        continue;
                    }
                    // Pushed along the walk and out of the body's way.
                    let out = away.normalize_or_zero();
                    let direction = (out + walk_direction * walking).normalize_or(out);
                    let touch = 1.0 - smoothstep(NEAR, REACH, distance);
                    targets.insert(i, direction * (MAX_BEND * p.yielding * touch));
                    self.moving.entry(i).or_insert(State {
                        bend: Vec2::ZERO,
                        velocity: Vec2::ZERO,
                    });
                }
            }
        }

        // Springs: every moving plant tends to its target (upright when free).
        let mut settled = Vec::new();
        for (&i, state) in self.moving.iter_mut() {
            let target = targets.get(&i).copied().unwrap_or(Vec2::ZERO);
            let mut remaining = dt;
            while remaining > 0.0 {
                let step = remaining.min(MAX_STEP);
                let acceleration = (target - state.bend) * STIFFNESS - state.velocity * DAMPING;
                state.velocity += acceleration * step;
                state.bend += state.velocity * step;
                remaining -= step;
            }
            let at_rest = target == Vec2::ZERO
                && state.bend.length() < REST
                && state.velocity.length() < REST;
            if at_rest {
                state.bend = Vec2::ZERO;
                settled.push(i);
            }
            apply(&self.plants[i], state.bend);
        }
        for i in settled {
            self.moving.remove(&i);
        }
    }

    /// Number of plants bent or moving right now.
    #[cfg(test)]
    pub fn moving(&self) -> usize {
        self.moving.len()
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_plant() -> Trample {
        Trample::new(vec![Pliable {
            base: Vec3::new(5.5, 2.0, 5.5),
            yielding: 1.0,
            group: 0,
            index: 0,
        }])
    }

    /// Walks the feet along x through the plant and records its bend at every frame.
    fn walk_through(trample: &mut Trample, z: f32) -> Vec<Vec2> {
        let mut bends = Vec::new();
        let speed = 3.2;
        let dt = 1.0 / 60.0;
        for frame in 0..180 {
            let feet = Vec3::new(4.0 + speed * dt * frame as f32, 2.0, z);
            let mut bend = Vec2::ZERO;
            trample.update(dt, feet, Vec3::new(speed, 0.0, 0.0), |_, b| bend = b);
            bends.push(bend);
        }
        bends
    }

    #[test]
    fn walking_over_a_plant_bends_it_smoothly_along_the_walk() {
        let mut trample = one_plant();
        let bends = walk_through(&mut trample, 5.5);
        let largest = bends.iter().map(|b| b.length()).fold(0.0, f32::max);
        assert!(largest > 0.2 && largest < 0.6, "largest bend {largest}");
        // Smooth: no jump between frames.
        let jump = bends
            .windows(2)
            .map(|w| (w[1] - w[0]).length())
            .fold(0.0, f32::max);
        assert!(jump < 0.06, "jump of {jump} in one frame");
        // Pushed forward (+x), the way the body goes, never swinging round to the back.
        let most = bends
            .iter()
            .max_by(|a, b| a.length().total_cmp(&b.length()))
            .unwrap();
        assert!(most.x > 0.0, "{most:?}");
    }

    #[test]
    fn freed_plant_springs_back_upright_and_stops_being_simulated() {
        let mut trample = one_plant();
        walk_through(&mut trample, 5.5);
        // The body is now far away: the plant settles.
        for _ in 0..240 {
            trample.update(
                1.0 / 60.0,
                Vec3::new(20.0, 2.0, 20.0),
                Vec3::ZERO,
                |_, _| {},
            );
        }
        assert_eq!(trample.moving(), 0);
    }

    #[test]
    fn plants_far_from_the_path_do_not_move() {
        let mut trample = one_plant();
        let bends = walk_through(&mut trample, 7.0);
        assert!(bends.iter().all(|b| *b == Vec2::ZERO));
    }
}
