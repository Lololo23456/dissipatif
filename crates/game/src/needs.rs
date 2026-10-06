//! Gentle survival: hunger, thirst, warmth, and the sickness of having eaten the wrong thing.
//! Nobody dies: a hungry, thirsty, cold or sick naturalist slows down and sees the world
//! dimmer, reminding them that they are fragile, then recovers by eating, drinking, warming up.
//!
//! The air temperature is felt from the biome, the altitude, the hour, the rain and the water;
//! the body's warmth drifts towards what that temperature allows.

use glam::Vec3;
use world::{Biome, World};

/// Real seconds for a full stomach to empty, and a full flask (a day lasts 20 minutes).
const FOOD_SECONDS: f32 = 30.0 * 60.0;
const WATER_SECONDS: f32 = 18.0 * 60.0;
/// Seconds for the body to cool from comfortable to cold in freezing air, and to warm back.
const COOLING_SECONDS: f32 = 120.0;
const WARMING_SECONDS: f32 = 60.0;
/// Below this temperature (°C) the body cools; above `HOT`, it sweats (drinks more).
const COLD: f32 = 10.0;
const HOT: f32 = 30.0;
/// Seconds of sickness per unit of toxicity eaten.
const SICKNESS_SECONDS: f32 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Needs {
    /// 1 fed, 0 starving.
    pub food: f32,
    /// 1 quenched, 0 parched.
    pub water: f32,
    /// 1 warm, 0 frozen.
    pub warmth: f32,
    /// Seconds of sickness left.
    pub sick: f32,
}

impl Default for Needs {
    fn default() -> Self {
        Self {
            food: 0.85,
            water: 0.85,
            warmth: 1.0,
            sick: 0.0,
        }
    }
}

/// What the body is going through.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Exposure {
    /// Air temperature felt, °C.
    pub temperature: f32,
    /// Running (more food and water).
    pub running: bool,
    /// In the water (cold).
    pub in_water: bool,
}

/// Air temperature (°C) at `feet`: the biome's climate, colder with altitude (6.5 °C per
/// 1000 m, a cell being a metre, exaggerated ×10 for such small mountains), a daily swing
/// (wide in the desert), cooler under rain.
/// `year`: phase of the year (see `season.rs`); the means above are those of high summer.
pub fn temperature(world: &World, feet: Vec3, hour: f32, rain: f32, year: f32) -> f32 {
    let dims = world.dims();
    let (x, z) = (
        (feet.x.max(0.0) as usize).min(dims.nx - 1),
        (feet.z.max(0.0) as usize).min(dims.nz - 1),
    );
    let (mean, swing) = match world.biome(x, z) {
        Biome::Desert => (27.0, 13.0),
        Biome::Savanna => (25.0, 8.0),
        Biome::Beach | Biome::Ocean => (22.0, 5.0),
        Biome::Plains => (18.0, 7.0),
        Biome::Forest => (16.0, 5.0),
        Biome::Taiga => (9.0, 6.0),
        Biome::Mountain => (7.0, 7.0),
        Biome::SnowyPeak => (-4.0, 6.0),
    };
    let altitude = (feet.y - world.config.sea_level).max(0.0);
    // Warmest at about 15:00, coldest before dawn.
    let daily = (std::f32::consts::TAU * (hour - 9.0) / 24.0).sin();
    mean + swing * daily - altitude * 0.065 - 4.0 * rain + crate::season::cooling(year)
}

impl Needs {
    /// Advances by `dt` seconds.
    pub fn update(&mut self, dt: f32, exposure: &Exposure) {
        let effort = if exposure.running { 1.6 } else { 1.0 };
        let heat = ((exposure.temperature - HOT) / 10.0).clamp(0.0, 1.5);
        let sick = if self.sick > 0.0 { 1.5 } else { 1.0 };
        self.food = (self.food - dt / FOOD_SECONDS * effort).max(0.0);
        self.water = (self.water - dt / WATER_SECONDS * effort * (1.0 + heat) * sick).max(0.0);
        // Warmth: water steals heat fast; cold air slowly; mild air gives it back.
        let felt = exposure.temperature - if exposure.in_water { 8.0 } else { 0.0 };
        if felt < COLD {
            let cold = ((COLD - felt) / 20.0).min(1.5);
            self.warmth = (self.warmth - dt / COOLING_SECONDS * cold).max(0.0);
        } else {
            self.warmth = (self.warmth + dt / WARMING_SECONDS).min(1.0);
        }
        self.sick = (self.sick - dt).max(0.0);
    }

    /// Eating something: its food, and maybe sickness.
    pub fn eat(&mut self, nutrition: f32, toxicity: f32) {
        self.food = (self.food + nutrition).min(1.0);
        self.sick += toxicity * SICKNESS_SECONDS;
    }

    /// A few sips of water.
    pub fn drink(&mut self) {
        self.water = (self.water + 0.25).min(1.0);
    }

    /// How much the body is held back, in [0, 1]: 0 fine, 1 exhausted. Gentle: never stops.
    pub fn weariness(&self) -> f32 {
        let low = |v: f32| ((0.2 - v) / 0.2).clamp(0.0, 1.0);
        let sickness = if self.sick > 0.0 { 0.6 } else { 0.0 };
        low(self.food)
            .max(low(self.water))
            .max(low(self.warmth))
            .max(sickness)
    }

    /// Speed factor of the body: 1 fine, down to 0.6 when exhausted.
    pub fn pace(&self) -> f32 {
        1.0 - 0.4 * self.weariness()
    }
}

crate::save::persist_struct!(Needs {
    food,
    water,
    warmth,
    sick
});

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    fn mild() -> Exposure {
        Exposure {
            temperature: 18.0,
            ..Exposure::default()
        }
    }

    #[test]
    fn hunger_and_thirst_come_over_tens_of_minutes() {
        let mut n = Needs::default();
        // Ten minutes of walking in mild weather: hungry and thirsty, not exhausted.
        for _ in 0..600 {
            n.update(1.0, &mild());
        }
        assert!(n.food > 0.4 && n.food < 0.85);
        assert!(n.water > 0.2 && n.water < n.food);
        assert_eq!(n.weariness(), 0.0);
        // Half an hour more: slowed down, never stopped.
        for _ in 0..1800 {
            n.update(1.0, &mild());
        }
        assert!(n.weariness() > 0.9 && n.pace() >= 0.6);
    }

    #[test]
    fn cold_water_chills_and_mild_air_warms_back() {
        let mut n = Needs::default();
        let swim = Exposure {
            temperature: 12.0,
            in_water: true,
            ..Exposure::default()
        };
        for _ in 0..120 {
            n.update(1.0, &swim);
        }
        assert!(n.warmth < 0.85);
        for _ in 0..60 {
            n.update(1.0, &mild());
        }
        assert_eq!(n.warmth, 1.0);
    }

    #[test]
    fn poisonous_food_makes_sick_for_a_while() {
        let mut n = Needs::default();
        n.eat(0.08, 0.9);
        assert!(n.weariness() > 0.5);
        for _ in 0..120 {
            n.update(1.0, &mild());
        }
        assert_eq!(n.sick, 0.0);
    }

    #[test]
    fn deserts_are_hot_by_day_and_cold_at_night_peaks_are_cold() {
        let world = World::generate(WorldConfig::standard(6));
        let dims = world.dims();
        let mut desert = None;
        let mut peak = None;
        for z in (0..dims.nz).step_by(4) {
            for x in (0..dims.nx).step_by(4) {
                let feet = Vec3::new(x as f32, world.ground_top(x, z) as f32, z as f32);
                match world.biome(x, z) {
                    Biome::Desert => desert = Some(feet),
                    Biome::SnowyPeak => peak = Some(feet),
                    _ => {}
                }
            }
        }
        if let Some(d) = desert {
            assert!(temperature(&world, d, 15.0, 0.0, 0.375) > 30.0);
            assert!(temperature(&world, d, 3.0, 0.0, 0.375) < 20.0);
        }
        if let Some(p) = peak {
            assert!(temperature(&world, p, 15.0, 0.0, 0.375) < 5.0);
        }
    }
}
