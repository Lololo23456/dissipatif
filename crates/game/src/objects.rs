//! Things laid in the world by the players, and the physics that decides what becomes of them.
//! There are no recipes and no predefined structures: a fire is twigs that burn, a hearth is
//! stones around it that get hot, a dish fires because it stayed hot long enough.
//!
//! Each object is a body of the thermal network (`sim::thermal`): it has a temperature, water,
//! maybe fuel. Each keeps its history (the hottest it got, how long it stayed above the
//! firing temperature, whether it burst), and its matter changes accordingly:
//! - a raw clay dish that stays above 600 °C for a minute becomes terracotta;
//! - a raw dish still wet when the fire boils its water bursts into shards;
//! - fuel that burnt out leaves its ash.

use glam::Vec3;
use sim::thermal::{Body, Fuel, KELVIN, Thermal};

use crate::items::Matter;

/// Firing: clay turns into terracotta above this temperature (K), after this long (s).
pub const FIRING_TEMPERATURE: f32 = KELVIN + 600.0;
pub const FIRING_SECONDS: f32 = 60.0;
/// A raw dish whose water boils away faster than this (kg/s) bursts.
const BURST_RATE: f32 = 0.003;
/// Hottest an object can be to take it in the hand (K).
pub const HANDLE_TEMPERATURE: f32 = KELVIN + 55.0;
/// Two objects closer than this (horizontally, cells) stack.
const STACK_REACH: f32 = 0.12;

/// The thermal body of a matter laid at `position` (the centre of its base), at air
/// temperature `air` (K), with `water` kg of water.
fn body(matter: Matter, base: Vec3, air: f32) -> Body {
    let wood = |mass: f32, ignition_c: f32, burn_rate: f32, heated_share: f32| Fuel {
        mass,
        ignition: KELVIN + ignition_c,
        heat_value: 16e6,
        burn_rate,
        ash_share: 0.02,
        // Flaming: most of the heat leaves with the gases.
        retained: 0.05,
        heated_share,
    };
    // (radius m, dry mass kg, specific heat J/kg/K, water kg, fuel)
    let (radius, mass, specific_heat, water, fuel) = match matter {
        Matter::Pebble { .. } => (0.05, 0.3, 800.0, 0.0, None),
        // A bundle of twigs burns in about a minute (~0.004 kg/s, some 60 kW). Thin dead
        // twigs catch at ~270 °C when a flame is there to light their gases (piloted
        // ignition; without a flame, wood needs over 300 °C).
        Matter::DeadTwigs => (0.08, 0.01, 1700.0, 0.0, Some(wood(0.24, 270.0, 0.05, 0.2))),
        // Dry grass: fine, light, the best tinder (catches at a lower temperature).
        Matter::GrassFibre => (
            0.04,
            0.002,
            1700.0,
            0.0,
            Some(wood(0.028, 260.0, 0.08, 1.0)),
        ),
        Matter::Frond => (
            0.06,
            0.005,
            1700.0,
            0.02,
            Some(wood(0.045, 280.0, 0.1, 0.6)),
        ),
        Matter::Clay { .. } => (0.05, 0.4, 900.0, 0.1, None),
        Matter::RawDish { .. } => (0.07, 0.32, 900.0, 0.08, None),
        Matter::FiredDish { .. } | Matter::Shards => (0.07, 0.35, 850.0, 0.0, None),
        Matter::Sand => (0.05, 0.5, 830.0, 0.0, None),
        // A stick: a little dry wood, catches like twigs.
        Matter::Stick => (
            0.04,
            0.005,
            1700.0,
            0.0,
            Some(wood(0.055, 280.0, 0.05, 0.5)),
        ),
        Matter::Flake | Matter::Chips => (0.03, 0.06, 800.0, 0.0, None),
        Matter::Knife { .. } => (0.05, 0.06, 900.0, 0.0, Some(wood(0.06, 290.0, 0.05, 0.5))),
        // Green wood: full of sap (water), it must dry before it burns.
        Matter::GreenWood => (0.07, 0.01, 1700.0, 0.12, Some(wood(0.18, 300.0, 0.05, 0.2))),
        Matter::Ash => (0.03, 0.05, 800.0, 0.0, None),
        // Bone: dense, does not burn here.
        Matter::Antler => (0.08, 0.9, 1300.0, 0.0, None),
        Matter::Acorn | Matter::Hazelnut => (0.01, 0.005, 1500.0, 0.001, None),
        Matter::Flower(_) | Matter::Mushroom { .. } => (0.03, 0.03, 3000.0, 0.02, None),
    };
    Body {
        position: [base.x, base.y + radius, base.z],
        radius,
        mass,
        specific_heat,
        emissivity: 0.9,
        temperature: air,
        water,
        fuel,
        ash: 0.0,
        burning: false,
        power: 0.0,
        enclosure: 0.0,
        draft: 0.0,
    }
}

/// The ember a fire drill makes: a pinch of charred wood dust, smouldering.
fn ember(base: Vec3) -> Body {
    Body {
        position: [base.x, base.y + 0.01, base.z],
        radius: 0.01,
        mass: 0.0005,
        specific_heat: 1700.0,
        emissivity: 0.9,
        temperature: KELVIN + 700.0,
        water: 0.0,
        fuel: Some(Fuel {
            mass: 0.003,
            ignition: KELVIN + 250.0,
            heat_value: 16e6,
            burn_rate: 0.1,
            ash_share: 0.0,
            // Smouldering, without flame: the heat stays in the glowing char.
            retained: 0.6,
            heated_share: 1.0,
        }),
        ash: 0.0,
        burning: false,
        power: 0.0,
        enclosure: 0.0,
        draft: 0.0,
    }
}

/// What an object went through.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct History {
    /// Hottest it got (K).
    pub hottest: f32,
    /// Seconds spent above the firing temperature.
    pub fired_seconds: f32,
}

/// An object lying in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub matter: Matter,
    /// Centre of its base.
    pub base: Vec3,
    pub history: History,
    /// Water at the last step, to see how fast it boils away.
    last_water: f32,
}

/// The objects laid in the world, and their physics. Embers are bodies of the network with no
/// object (they live a few seconds).
#[derive(Default)]
pub struct Objects {
    placed: Vec<Placed>,
    /// Body of each placed object, then the embers: `bodies[i]` belongs to `placed[i]` for
    /// i < placed.len().
    thermal: Thermal,
    embers: usize,
}

impl Objects {
    pub fn placed(&self) -> &[Placed] {
        &self.placed
    }

    /// The thermal body of object `i`.
    pub fn body(&self, i: usize) -> &Body {
        &self.thermal.bodies[i]
    }

    /// Embers smouldering now (positions and how strongly).
    pub fn embers(&self) -> impl Iterator<Item = &Body> {
        self.thermal.bodies[self.placed.len()..].iter()
    }

    /// Where something laid at (x, z) on ground `ground` would rest: on top of what is
    /// already there.
    pub fn resting_point(&self, x: f32, z: f32, ground: f32) -> Vec3 {
        let mut top = ground;
        for (i, p) in self.placed.iter().enumerate() {
            let b = &self.thermal.bodies[i];
            let d = ((p.base.x - x).powi(2) + (p.base.z - z).powi(2)).sqrt();
            if d < STACK_REACH + b.radius * 0.5 {
                // Settles a little into what bears it (twigs on a nest of grass).
                top = top.max(b.position[1] + b.radius * 0.5);
            }
        }
        Vec3::new(x, top, z)
    }

    /// Lays `matter` at `base`, at air temperature `air` (K).
    pub fn lay(&mut self, matter: Matter, base: Vec3, air: f32) {
        let body = body(matter, base, air);
        let water = body.water;
        // Placed objects' bodies come first: insert before the embers.
        let index = self.placed.len();
        self.thermal.bodies.insert(index, body);
        self.rebuild_buffers();
        self.placed.push(Placed {
            matter,
            base,
            history: History {
                hottest: air,
                fired_seconds: 0.0,
            },
            last_water: water,
        });
        self.thermal.relink();
    }

    /// Takes object `i` away; returns what it has become.
    pub fn take(&mut self, i: usize) -> Matter {
        let matter = self.placed.remove(i).matter;
        self.thermal.bodies.remove(i);
        self.rebuild_buffers();
        self.thermal.relink();
        matter
    }

    /// A fire drill's ember dropped on object `i` (it must touch something to light it).
    pub fn drop_ember(&mut self, i: usize) {
        let base = self.placed[i].base;
        self.thermal.add(ember(base));
        self.embers += 1;
        self.thermal.relink();
    }

    fn rebuild_buffers(&mut self) {
        // `Thermal` keeps one flow slot per body; rebuild after inserting or removing.
        let bodies = std::mem::take(&mut self.thermal.bodies);
        self.thermal = Thermal::new();
        for b in bodies {
            self.thermal.add(b);
        }
    }

    /// Someone blowing at `at` (x, z): fresh air on the embers and fuel within a hand's
    /// breadth. Lasts one step (call again while blowing).
    pub fn blow(&mut self, x: f32, z: f32) {
        for b in &mut self.thermal.bodies {
            let d = ((b.position[0] - x).powi(2) + (b.position[2] - z).powi(2)).sqrt();
            if d < 0.35 {
                b.draft = 1.0;
            }
        }
    }

    /// Whether something within a hand's breadth of (x, z) glows: an ember, or fuel burning
    /// or smouldering (worth blowing on).
    pub fn glowing_near(&self, x: f32, z: f32) -> bool {
        self.thermal.bodies.iter().any(|b| {
            let d = ((b.position[0] - x).powi(2) + (b.position[2] - z).powi(2)).sqrt();
            d < 0.35
                && b.fuel.is_some_and(|f| f.mass > 0.0)
                && (b.burning || b.temperature > KELVIN + 200.0)
        })
    }

    /// Whether object `i` can be taken in the hand now.
    pub fn handleable(&self, i: usize) -> bool {
        let b = &self.thermal.bodies[i];
        b.temperature < HANDLE_TEMPERATURE && !b.burning
    }

    /// Advances by `dt` seconds with the air at `air` K. Matters change with their history:
    /// each change (where, from, to) is pushed onto `changes`.
    pub fn step(&mut self, dt: f32, air: f32, changes: &mut Vec<(Vec3, Matter, Matter)>) {
        if self.thermal.bodies.is_empty() {
            return;
        }
        self.thermal.step(dt, air);
        // Blowing lasts while someone blows.
        for b in &mut self.thermal.bodies {
            b.draft = 0.0;
        }
        let mut relink = false;
        for (i, p) in self.placed.iter_mut().enumerate() {
            let b = &mut self.thermal.bodies[i];
            p.history.hottest = p.history.hottest.max(b.temperature);
            if b.temperature > FIRING_TEMPERATURE {
                p.history.fired_seconds += dt;
            }
            let boiled = (p.last_water - b.water) / dt;
            p.last_water = b.water;
            let before = p.matter;
            match p.matter {
                Matter::RawDish { .. } if boiled > BURST_RATE => {
                    // Steam inside the wet clay: the dish bursts.
                    p.matter = Matter::Shards;
                    b.water = 0.0;
                }
                Matter::RawDish { source } if p.history.fired_seconds >= FIRING_SECONDS => {
                    p.matter = Matter::FiredDish { source };
                }
                Matter::DeadTwigs | Matter::GrassFibre | Matter::Frond
                    if b.fuel.is_some_and(|f| f.mass <= 1e-4) && !b.burning =>
                {
                    // Burnt out: what is left is its ash.
                    p.matter = Matter::Ash;
                    b.fuel = None;
                    b.mass = b.ash.max(0.005);
                    b.radius = 0.03;
                    relink = true;
                }
                _ => {}
            }
            if p.matter != before {
                changes.push((p.base, before, p.matter));
            }
        }
        // Spent embers go.
        let first_ember = self.placed.len();
        let before = self.thermal.bodies.len();
        let mut k = first_ember;
        while k < self.thermal.bodies.len() {
            let spent = self.thermal.bodies[k].fuel.is_none_or(|f| f.mass <= 1e-6)
                && !self.thermal.bodies[k].burning;
            if spent {
                self.thermal.bodies.remove(k);
            } else {
                k += 1;
            }
        }
        if self.thermal.bodies.len() != before {
            self.embers = self.thermal.bodies.len() - first_ember;
            self.rebuild_buffers();
            relink = true;
        }
        if relink {
            self.thermal.relink();
        }
    }

    /// The air warming near the objects: the hottest burning thing within `reach` of (x, z)
    /// adds this many kelvins to the air felt there.
    pub fn warmth(&self, x: f32, z: f32, reach: f32) -> f32 {
        self.thermal
            .bodies
            .iter()
            .filter(|b| b.burning)
            .map(|b| {
                let d = ((b.position[0] - x).powi(2) + (b.position[2] - z).powi(2)).sqrt();
                let near = (1.0 - d / reach).max(0.0);
                (b.power / 20_000.0).min(1.0) * 14.0 * near
            })
            .fold(0.0, f32::max)
    }
}

crate::save::persist_struct!(History {
    hottest,
    fired_seconds
});
crate::save::persist_struct!(Placed {
    matter,
    base,
    history,
    last_water
});

impl crate::save::Persist for Objects {
    fn write(&self, w: &mut crate::save::Writer) {
        w.put(&self.placed);
        w.put(&self.thermal.bodies);
        w.put(&self.embers);
    }
    fn read(r: &mut crate::save::Reader) -> crate::save::Result<Self> {
        let placed: Vec<Placed> = r.get()?;
        let bodies: Vec<Body> = r.get()?;
        let embers: usize = r.get()?;
        if bodies.len() != placed.len() + embers {
            return Err("objets incohérents".into());
        }
        let mut thermal = Thermal::new();
        for body in bodies {
            thermal.add(body);
        }
        thermal.relink();
        Ok(Self {
            placed,
            thermal,
            embers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ClaySource;

    const AIR: f32 = KELVIN + 15.0;

    fn run(objects: &mut Objects, seconds: f32) {
        let mut changes = Vec::new();
        for _ in 0..(seconds * 4.0) as usize {
            objects.step(0.25, AIR, &mut changes);
        }
    }

    fn burning(objects: &Objects) -> bool {
        (0..objects.placed().len()).any(|i| objects.body(i).burning)
    }

    /// The way a fire is made: a nest of dry grass, the ember in it; once the grass flames,
    /// twigs laid on it catch.
    #[test]
    fn an_ember_lights_a_nest_of_grass_which_lights_twigs_laid_on_it() {
        let mut o = Objects::default();
        o.lay(Matter::GrassFibre, Vec3::ZERO, AIR);
        let top = o.resting_point(0.0, 0.0, 0.0);
        o.lay(Matter::GrassFibre, top, AIR);
        o.drop_ember(0);
        let mut changes = Vec::new();
        let mut flaming = false;
        for _ in 0..4 * 120 {
            o.step(0.25, AIR, &mut changes);
            if o.body(0).burning || o.body(1).burning {
                flaming = true;
                break;
            }
        }
        assert!(flaming, "the grass never caught");
        for k in 0..3 {
            let a = std::f32::consts::TAU * k as f32 / 3.0;
            let at = o.resting_point(0.12 * a.cos(), 0.12 * a.sin(), 0.0);
            o.lay(Matter::DeadTwigs, at, AIR);
        }
        let mut twigs_caught = false;
        for _ in 0..4 * 90 {
            o.step(0.25, AIR, &mut changes);
            twigs_caught |= (2..5).any(|i| o.body(i).burning);
        }
        assert!(twigs_caught, "the twigs never caught");
    }

    /// Twigs piled on a single tuft at once take its heat and its air: it never flames.
    #[test]
    fn twigs_piled_on_a_tuft_at_once_smother_it() {
        let mut o = Objects::default();
        o.lay(Matter::GrassFibre, Vec3::ZERO, AIR);
        o.lay(Matter::DeadTwigs, Vec3::new(0.1, 0.0, 0.0), AIR);
        o.drop_ember(0);
        run(&mut o, 120.0);
        assert!(!burning(&o));
    }

    #[test]
    fn an_ember_alone_does_not_light_twigs_it_takes_tinder() {
        let mut o = Objects::default();
        o.lay(Matter::DeadTwigs, Vec3::ZERO, AIR);
        o.drop_ember(0);
        run(&mut o, 60.0);
        assert!(!burning(&o), "twigs lit from an ember without tinder");
    }

    #[test]
    fn a_dry_dish_in_a_fire_fires_and_a_wet_one_bursts() {
        let fire = |dry: bool| {
            let mut o = Objects::default();
            o.lay(
                Matter::RawDish {
                    source: ClaySource::RedEarth,
                },
                Vec3::ZERO,
                AIR,
            );
            if dry {
                o.thermal.bodies[0].water = 0.0;
                o.placed[0].last_water = 0.0;
            }
            // Twigs all around and on top, lit.
            for k in 0..6 {
                let a = std::f32::consts::TAU * k as f32 / 6.0;
                o.lay(
                    Matter::DeadTwigs,
                    Vec3::new(0.15 * a.cos(), 0.0, 0.15 * a.sin()),
                    AIR,
                );
            }
            for i in 1..7 {
                o.thermal.bodies[i].temperature = KELVIN + 600.0;
            }
            run(&mut o, 120.0);
            o.placed()[0].matter
        };
        assert_eq!(
            fire(true),
            Matter::FiredDish {
                source: ClaySource::RedEarth
            }
        );
        assert_eq!(fire(false), Matter::Shards);
    }

    #[test]
    fn burnt_twigs_become_ash_and_hot_things_cannot_be_handled() {
        let mut o = Objects::default();
        o.lay(Matter::DeadTwigs, Vec3::ZERO, AIR);
        o.thermal.bodies[0].temperature = KELVIN + 600.0;
        o.step(0.25, AIR, &mut Vec::new());
        assert!(!o.handleable(0));
        run(&mut o, 300.0);
        assert_eq!(o.placed()[0].matter, Matter::Ash);
        run(&mut o, 900.0);
        assert!(o.handleable(0));
    }

    #[test]
    fn things_stack() {
        let mut o = Objects::default();
        o.lay(Matter::Pebble { dark: false }, Vec3::ZERO, AIR);
        let top = o.resting_point(0.02, 0.0, 0.0);
        assert!(top.y > 0.05);
    }

    /// Blowing on the ember lights even a single tuft under twigs piled on it at once: what a
    /// player who does not know the technique will do.
    #[test]
    fn blowing_on_the_ember_lights_even_a_smothered_tuft() {
        let mut o = Objects::default();
        o.lay(Matter::GrassFibre, Vec3::ZERO, AIR);
        for k in 0..3 {
            let a = std::f32::consts::TAU * k as f32 / 3.0;
            let at = o.resting_point(0.12 * a.cos(), 0.12 * a.sin(), 0.0);
            o.lay(Matter::DeadTwigs, at, AIR);
        }
        o.drop_ember(0);
        let mut changes = Vec::new();
        let mut twigs = 0.0;
        for step in 0..4 * 120 {
            // Blowing for the first half minute.
            if step < 4 * 30 {
                o.blow(0.0, 0.0);
            }
            o.step(0.25, AIR, &mut changes);
            if (1..4).any(|i| o.body(i).burning) {
                twigs = step as f32 / 4.0;
                break;
            }
        }
        assert!(twigs > 0.0, "the twigs never caught");
        assert!(twigs < 90.0, "caught only after {twigs} s");
    }

    /// A fire spreads to fuel laid nearby, sooner the closer, and never to fuel far away.
    #[test]
    fn fire_spreads_gradually_with_distance() {
        let caught = |gap: f32| {
            let mut o = Objects::default();
            o.lay(Matter::DeadTwigs, Vec3::ZERO, AIR);
            o.lay(Matter::DeadTwigs, Vec3::new(gap, 0.0, 0.0), AIR);
            o.thermal.bodies[0].temperature = KELVIN + 700.0;
            let mut changes = Vec::new();
            for k in 0..4 * 60 {
                o.step(0.25, AIR, &mut changes);
                if o.body(1).burning {
                    return Some(k as f32 / 4.0);
                }
            }
            None
        };
        let (near, mid, far) = (caught(0.2), caught(0.4), caught(1.2));
        assert!(
            near.is_some() && mid.is_some() && far.is_none(),
            "{near:?} {mid:?} {far:?}"
        );
        assert!(near < mid);
    }
}
