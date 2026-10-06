//! The state of the game that matters to everyone (what would be shared in multiplayer): the
//! players' bodies, bags and needs, and what has been taken from the world.
//!
//! It changes only in two ways, so that it could one day run on a host and be mirrored:
//! - `Command`s: every action of a player (steering, picking up, eating, drinking) is a
//!   command applied by `GameState::apply`; a key press only makes a command;
//! - fixed steps of time (`GameState::step`), the same length on every machine.
//!
//! It tells what happened through `Event`s, for the presentation (sounds, hiding a picked
//! flower, messages). Purely visual things (wind, particles, trampled grass) live outside.

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use world::World;

use crate::anomaly::Anomaly;
use crate::items::{Harvest, Inventory, Matter, Refusal, harvest};
use crate::needs::{self, Exposure, Needs};
use crate::obstacles::Obstacles;
use crate::player::{Controls, Player};

pub type PlayerId = u32;

/// Fixed length of a step of the game, in seconds.
pub const STEP: f32 = 1.0 / 60.0;
/// How far the hands reach, horizontally, from the feet.
const REACH: f32 = 1.4;
/// Pebbles one can gather at the foot of a boulder (enough to build small walls).
const PEBBLES_PER_STONE: u32 = 5;

pub struct PlayerState {
    pub id: PlayerId,
    pub body: Player,
    pub inventory: Inventory,
    pub needs: Needs,
    controls: Controls,
    camera_yaw: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// What the player's keys hold this step, and where their camera looks from.
    Steer {
        player: PlayerId,
        controls: Controls,
        camera_yaw: f32,
    },
    /// Pick up the nearest thing within reach.
    Pick { player: PlayerId },
    /// Eat one of slot `slot` of the bag.
    Eat { player: PlayerId, slot: usize },
    /// Drink from water within reach.
    Drink { player: PlayerId },
    /// Lay one of slot `slot` (a stone) on the ground just in front.
    Lay { player: PlayerId, slot: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    NothingInReach,
    Bag(Refusal),
    NotEdible,
    NoWater,
    /// Only stones can be laid, and only on the anomaly's carpet.
    CannotLay,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// `removed`: the plant (index in `World::plants`) taken away from the world, if any.
    Picked {
        player: PlayerId,
        matter: Matter,
        removed: Option<usize>,
    },
    Ate {
        player: PlayerId,
        matter: Matter,
    },
    Drank {
        player: PlayerId,
    },
    Laid {
        player: PlayerId,
        matter: Matter,
    },
    Failed {
        player: PlayerId,
        failure: Failure,
    },
}

pub struct GameState {
    players: Vec<PlayerState>,
    /// Plants taken away, by index in `World::plants`.
    removed: Vec<bool>,
    pebbles_taken: HashMap<usize, u32>,
    obstacles: Obstacles,
    /// Things that can be picked, by column (x, z).
    pickables: HashMap<(i64, i64), Vec<usize>>,
    next_id: PlayerId,
    anomaly: Option<Anomaly>,
}

impl GameState {
    pub fn new(world: &World) -> Self {
        let mut pickables: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, p) in world.plants().iter().enumerate() {
            if harvest(p).is_some() {
                let (x, z) = position(p);
                pickables
                    .entry((x.floor() as i64, z.floor() as i64))
                    .or_default()
                    .push(i);
            }
        }
        Self {
            players: Vec::new(),
            removed: vec![false; world.plants().len()],
            pebbles_taken: HashMap::new(),
            obstacles: Obstacles::from_world(world),
            pickables,
            next_id: 0,
            anomaly: None,
        }
    }

    /// Grows the anomaly in the clearing nearest to `near`. The plants of the clearing give
    /// way to it: returns them (indices in `World::plants`), to stop drawing them.
    pub fn grow_anomaly(
        &mut self,
        world: &World,
        near: Vec3,
        ahead: Vec2,
        seed: u64,
    ) -> Vec<usize> {
        let Some(anomaly) = Anomaly::grow(world, near, ahead, seed) else {
            return Vec::new();
        };
        let mut covered = Vec::new();
        for (i, p) in world.plants().iter().enumerate() {
            let (x, z) = position(p);
            if anomaly.covers(x, z) && !self.removed[i] {
                self.removed[i] = true;
                self.obstacles.remove(i);
                covered.push(i);
            }
        }
        self.anomaly = Some(anomaly);
        covered
    }

    pub fn anomaly(&self) -> Option<&Anomaly> {
        self.anomaly.as_ref()
    }

    /// Where player `id` would lay something: on the ground just in front of the feet.
    pub fn lay_point(&self, id: PlayerId) -> Option<Vec2> {
        let body = &self.player(id)?.body;
        let facing = body.facing();
        Some(
            Vec2::new(body.position.x, body.position.z)
                + Vec2::new(facing.sin(), facing.cos()) * 0.9,
        )
    }

    /// Whether player `id` could lay what is in slot `slot` now.
    pub fn can_lay(&self, id: PlayerId, slot: usize) -> bool {
        let (Some(p), Some(anomaly), Some(at)) =
            (self.player(id), &self.anomaly, self.lay_point(id))
        else {
            return false;
        };
        matches!(
            p.inventory.stacks().get(slot).map(|s| s.matter),
            Some(Matter::Pebble { .. })
        ) && anomaly.covers(at.x, at.y)
    }

    /// A new player standing at `feet`.
    pub fn join(&mut self, feet: Vec3) -> PlayerId {
        let id = self.next_id;
        self.next_id += 1;
        self.players.push(PlayerState {
            id,
            body: Player::new(feet),
            inventory: Inventory::default(),
            needs: Needs::default(),
            controls: Controls::default(),
            camera_yaw: 0.0,
        });
        id
    }

    /// The body of player `id`. Ids are indices in `players`, and players are never removed:
    /// any id returned by `join` is valid.
    pub fn body(&self, id: PlayerId) -> &Player {
        &self.players[id as usize].body
    }

    pub fn player(&self, id: PlayerId) -> Option<&PlayerState> {
        self.players.iter().find(|p| p.id == id)
    }

    fn player_mut(&mut self, id: PlayerId) -> Option<&mut PlayerState> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    /// Moves a player's body to `feet` (tools, captures).
    pub fn place(&mut self, id: PlayerId, feet: Vec3) {
        if let Some(p) = self.player_mut(id) {
            p.body = Player::new(feet);
        }
    }

    #[cfg(test)]
    pub fn is_removed(&self, plant: usize) -> bool {
        self.removed.get(plant).copied().unwrap_or(false)
    }

    /// The thing player `id` would pick up now: the nearest within reach, those in front
    /// first. Its index in `World::plants` and what it would give.
    pub fn target(&self, world: &World, id: PlayerId) -> Option<(usize, Matter)> {
        let p = self.player(id)?;
        let feet = p.body.position;
        let facing = p.body.facing();
        let ahead = Vec2::new(facing.sin(), facing.cos());
        let (cx, cz) = (feet.x.floor() as i64, feet.z.floor() as i64);
        let mut best: Option<(f32, usize, Matter)> = None;
        for z in cz - 2..=cz + 2 {
            for x in cx - 2..=cx + 2 {
                let Some(list) = self.pickables.get(&(x, z)) else {
                    continue;
                };
                for &i in list {
                    if self.removed[i] {
                        continue;
                    }
                    let plant = &world.plants()[i];
                    let matter = match harvest(plant) {
                        Some(Harvest::Whole(m)) => m,
                        Some(Harvest::Part(m))
                            if self.pebbles_taken.get(&i).copied().unwrap_or(0)
                                < PEBBLES_PER_STONE =>
                        {
                            m
                        }
                        _ => continue,
                    };
                    let (px, pz) = position(plant);
                    let offset = Vec2::new(px - feet.x, pz - feet.z);
                    let distance = offset.length();
                    let height = plant.base[1] as f32 - feet.y;
                    if distance > REACH || !(-1.5..=1.0).contains(&height) {
                        continue;
                    }
                    // Prefer what lies in front of the naturalist.
                    let front = offset.normalize_or_zero().dot(ahead);
                    let score = distance - 0.5 * front;
                    if best.is_none_or(|b| score < b.0) {
                        best = Some((score, i, matter));
                    }
                }
            }
        }
        best.map(|(_, i, m)| (i, m))
    }

    /// Whether player `id` can drink here: water at their feet or within reach, not far
    /// below them.
    pub fn can_drink(&self, world: &World, id: PlayerId) -> bool {
        let Some(p) = self.player(id) else {
            return false;
        };
        let feet = p.body.position;
        let dims = world.dims();
        for dz in [-1.0f32, 0.0, 1.0] {
            for dx in [-1.0f32, 0.0, 1.0] {
                let (x, z) = (feet.x + dx, feet.z + dz);
                if x < 0.0 || z < 0.0 || x >= dims.nx as f32 || z >= dims.nz as f32 {
                    continue;
                }
                if let Some(level) = world.water_level(x as usize, z as usize)
                    && level > feet.y - 1.2
                {
                    return true;
                }
            }
        }
        false
    }

    /// Applies a command. Returns what happened, if anything worth telling.
    pub fn apply(&mut self, world: &World, command: Command) -> Option<Event> {
        match command {
            Command::Steer {
                player,
                controls,
                camera_yaw,
            } => {
                let p = self.player_mut(player)?;
                // A jump is kept until a step uses it.
                let jump = p.controls.jump || controls.jump;
                p.controls = Controls { jump, ..controls };
                p.camera_yaw = camera_yaw;
                None
            }
            Command::Pick { player } => {
                let Some((plant, matter)) = self.target(world, player) else {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::NothingInReach,
                    });
                };
                let p = self.player_mut(player)?;
                if let Err(refusal) = p.inventory.add(matter) {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::Bag(refusal),
                    });
                }
                let removed = match harvest(&world.plants()[plant]) {
                    Some(Harvest::Whole(_)) => {
                        self.removed[plant] = true;
                        self.obstacles.remove(plant);
                        Some(plant)
                    }
                    _ => {
                        *self.pebbles_taken.entry(plant).or_default() += 1;
                        None
                    }
                };
                Some(Event::Picked {
                    player,
                    matter,
                    removed,
                })
            }
            Command::Eat { player, slot } => {
                let p = self.player_mut(player)?;
                let matter = p.inventory.stacks().get(slot)?.matter;
                if !matter.edible() {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::NotEdible,
                    });
                }
                p.inventory.take(slot);
                let properties = matter.properties();
                p.needs.eat(properties.nutrition, properties.toxicity);
                Some(Event::Ate { player, matter })
            }
            Command::Lay { player, slot } => {
                if !self.can_lay(player, slot) {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::CannotLay,
                    });
                }
                let at = self.lay_point(player)?;
                let matter = self.player_mut(player)?.inventory.take(slot)?;
                self.anomaly.as_mut()?.lay_stone(at.x, at.y);
                Some(Event::Laid { player, matter })
            }
            Command::Drink { player } => {
                if !self.can_drink(world, player) {
                    return Some(Event::Failed {
                        player,
                        failure: Failure::NoWater,
                    });
                }
                self.player_mut(player)?.needs.drink();
                Some(Event::Drank { player })
            }
        }
    }

    /// One fixed step of time: bodies move as steered, needs follow the conditions.
    /// `hour` in [0, 24), `rain` in [0, 1].
    pub fn step(&mut self, world: &World, hour: f32, rain: f32) {
        if let Some(anomaly) = &mut self.anomaly {
            anomaly.step();
        }
        for p in &mut self.players {
            p.body.pace = p.needs.pace();
            p.body
                .update(&p.controls, p.camera_yaw, STEP, world, &self.obstacles);
            p.controls.jump = false;
            let moving = p.body.velocity().length() > 0.5;
            let exposure = Exposure {
                temperature: needs::temperature(world, p.body.position, hour, rain),
                running: p.controls.run && moving,
                in_water: p.body.in_water(),
            };
            p.needs.update(STEP, &exposure);
        }
    }
}

/// Where a plant stands, horizontally (cell centre plus its offset).
fn position(p: &world::PlantInstance) -> (f32, f32) {
    (
        p.base[0] as f32 + 0.5 + p.offset[0],
        p.base[2] as f32 + 0.5 + p.offset[1],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    fn setup() -> (World, GameState, PlayerId) {
        let world = World::generate(WorldConfig::standard(6));
        let mut state = GameState::new(&world);
        let id = state.join(crate::player::spawn_point(&world));
        (world, state, id)
    }

    /// Puts the player right next to the first pickable thing giving `want`.
    fn next_to(
        world: &World,
        state: &mut GameState,
        id: PlayerId,
        want: fn(Matter) -> bool,
    ) -> usize {
        let (i, p) = world
            .plants()
            .iter()
            .enumerate()
            .find(|(_, p)| matches!(harvest(p), Some(Harvest::Whole(m) | Harvest::Part(m)) if want(m)))
            .expect("no such plant in the world");
        let (x, z) = position(p);
        state.place(id, Vec3::new(x + 0.3, p.base[1] as f32 + 0.001, z));
        i
    }

    #[test]
    fn picking_a_flower_takes_it_from_the_world_into_the_bag() {
        let (world, mut state, id) = setup();
        let plant = next_to(&world, &mut state, id, |m| matches!(m, Matter::Flower(_)));
        let event = state.apply(&world, Command::Pick { player: id });
        let Some(Event::Picked {
            removed, matter, ..
        }) = event
        else {
            panic!("{event:?}");
        };
        assert!(matches!(matter, Matter::Flower(_)));
        assert_eq!(removed, Some(plant));
        assert!(state.is_removed(plant));
        assert_eq!(state.player(id).unwrap().inventory.stacks().len(), 1);
    }

    #[test]
    fn a_boulder_stays_but_gives_a_few_pebbles() {
        let (world, mut state, id) = setup();
        let stone = next_to(&world, &mut state, id, |m| {
            matches!(m, Matter::Pebble { .. })
        });
        let mut pebbles = 0;
        for _ in 0..10 {
            if let Some(Event::Picked { removed, .. }) =
                state.apply(&world, Command::Pick { player: id })
            {
                assert_eq!(removed, None);
                pebbles += 1;
            }
        }
        assert!(!state.is_removed(stone));
        // Other things nearby may have been picked too; at most 3 pebbles from this stone.
        assert!(pebbles >= PEBBLES_PER_STONE);
        assert_eq!(state.pebbles_taken[&stone], PEBBLES_PER_STONE);
    }

    #[test]
    fn eating_a_mushroom_feeds_and_a_spotted_one_makes_sick() {
        let (world, mut state, id) = setup();
        next_to(&world, &mut state, id, |m| {
            m == Matter::Mushroom { spotted: true }
        });
        state.apply(&world, Command::Pick { player: id });
        let slot = state
            .player(id)
            .unwrap()
            .inventory
            .stacks()
            .iter()
            .position(|s| s.matter == Matter::Mushroom { spotted: true })
            .unwrap();
        let before = state.player(id).unwrap().needs.food;
        let event = state.apply(&world, Command::Eat { player: id, slot });
        assert!(matches!(event, Some(Event::Ate { .. })));
        let needs = state.player(id).unwrap().needs;
        assert!(needs.food > before && needs.sick > 0.0);
    }

    #[test]
    fn one_drinks_only_near_water() {
        let (world, mut state, id) = setup();
        // Far from water at first? Find a dry spot away from any water, then a shore.
        let dims = world.dims();
        let mut dry = None;
        let mut shore = None;
        for z in 2..dims.nz - 2 {
            for x in 2..dims.nx - 2 {
                let wet = world.water_level(x, z).is_some();
                let near =
                    (0..9).any(|k| world.water_level(x + k % 3 - 1, z + k / 3 - 1).is_some());
                let feet = Vec3::new(
                    x as f32 + 0.5,
                    world.ground_top(x, z) as f32,
                    z as f32 + 0.5,
                );
                if !near && dry.is_none() {
                    dry = Some(feet);
                }
                if !wet && near && shore.is_none() {
                    shore = Some(feet);
                }
            }
        }
        state.place(id, dry.unwrap());
        assert!(matches!(
            state.apply(&world, Command::Drink { player: id }),
            Some(Event::Failed {
                failure: Failure::NoWater,
                ..
            })
        ));
        state.place(id, shore.unwrap());
        assert!(matches!(
            state.apply(&world, Command::Drink { player: id }),
            Some(Event::Drank { .. })
        ));
    }

    #[test]
    fn steering_moves_the_body_by_fixed_steps() {
        let (world, mut state, id) = setup();
        let start = state.player(id).unwrap().body.position;
        let controls = Controls {
            forward: true,
            ..Controls::default()
        };
        state.apply(
            &world,
            Command::Steer {
                player: id,
                controls,
                camera_yaw: 0.0,
            },
        );
        for _ in 0..60 {
            state.step(&world, 12.0, 0.0);
        }
        assert!(state.player(id).unwrap().body.position.distance(start) > 1.0);
    }
}
