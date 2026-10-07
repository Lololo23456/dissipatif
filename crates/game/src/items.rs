//! What the naturalist can carry. No item types with recipes: an item is a matter (stone,
//! dead wood, grass fibre, a flower, a mushroom…) and what it can do follows from its
//! properties (hardness, sharpness, nutrition, toxicity…), computed from the matter. The
//! player learns them by trying and observing, not from a menu.

use world::plants::mushroom_is_spotted;
use world::{Plant, PlantInstance};

/// Wild flowers, by model variant (see `world::plants::flower`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flower {
    Daisy,
    Poppy,
    Lavender,
    Buttercup,
}

/// Where a clay comes from: it keeps it, and so do the dishes made of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClaySource {
    /// Grey clay of a river or lake bank: fine sediment left by slow water.
    Bank,
    /// Red earth of the savanna: a clay soil rich in iron oxides (it fires red).
    RedEarth,
}

/// What an item is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Matter {
    /// A pebble; `dark`: a dense dark rock that splits into sharp flakes.
    Pebble {
        dark: bool,
    },
    /// Bare dead twigs.
    DeadTwigs,
    /// A tuft of grass blades.
    GrassFibre,
    /// A fern frond.
    Frond,
    Flower(Flower),
    /// `spotted`: the white spots of a fly agaric (poisonous; the player has to learn it).
    Mushroom {
        spotted: bool,
    },
    /// Wet clay: plastic, hard once fired. Keeps where it comes from.
    Clay {
        source: ClaySource,
    },
    /// Sand from a beach or the desert.
    Sand,
    /// Wood ash, left in the hearth (rich in potash).
    Ash,
    /// A small dish shaped from clay: still raw and wet, it must dry before the fire.
    RawDish {
        source: ClaySource,
    },
    /// The same dish, fired: terracotta, holds water and heat.
    FiredDish {
        source: ClaySource,
    },
    /// What is left of a dish that burst in the fire (steam from its water).
    Shards,
    /// A straight dead stick, pulled from a bundle of twigs.
    Stick,
    /// A sharp flake struck off a fine-grained stone: an edge.
    Flake,
    /// Crumbled bits of a stone that would not flake.
    Chips,
    /// A flake bound to a stick with fibre: it cuts. `uses` left before the binding gives
    /// way (from 0 to `KNIFE_USES`).
    Knife {
        uses: u8,
    },
    /// Green wood cut from a living bush: heavy with sap, it burns badly until dried.
    GreenWood,
    /// An antler a stag shed at the end of winter: bone, hard yet a little springy (a soft
    /// hammer to retouch an edge, a handle, a tool).
    Antler,
    /// An acorn: food, but bitter with tannins unless soaked a long time in water.
    Acorn,
    /// A hazelnut: rich food.
    Hazelnut,
    /// A sheaf of reed stems pulled from the marsh's edge: light, supple, long fibres; dry,
    /// they catch at once, and laid thick they shed the rain (the best thatch).
    Reed,
}

/// Cuts a knife makes before its grass binding gives way.
pub const KNIFE_USES: u8 = 8;

/// What a matter can do, each in [0, 1] unless stated.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Properties {
    /// Mass of one, in kilograms.
    pub mass: f32,
    pub hardness: f32,
    pub sharpness: f32,
    /// Bends without breaking (fibres, green wood).
    pub flexibility: f32,
    /// Breaks easily.
    pub fragility: f32,
    pub flammability: f32,
    /// Share of a day's food in one.
    pub nutrition: f32,
    /// How bad it is to eat (0 harmless).
    pub toxicity: f32,
    /// Can be shaped by hand (wet clay).
    pub plasticity: f32,
    /// Stands a fire without burning, cracking or melting.
    pub heat_resistance: f32,
}

impl Matter {
    pub fn properties(self) -> Properties {
        let base = Properties::default();
        match self {
            Matter::Pebble { dark } => Properties {
                mass: 0.3,
                hardness: if dark { 0.9 } else { 0.75 },
                sharpness: if dark { 0.3 } else { 0.1 },
                fragility: if dark { 0.3 } else { 0.1 },
                heat_resistance: 0.8,
                ..base
            },
            Matter::Stick => Properties {
                mass: 0.06,
                hardness: 0.35,
                flexibility: 0.2,
                fragility: 0.4,
                flammability: 0.8,
                ..base
            },
            Matter::Flake => Properties {
                mass: 0.04,
                hardness: 0.9,
                sharpness: 0.9,
                fragility: 0.5,
                heat_resistance: 0.8,
                ..base
            },
            Matter::Chips => Properties {
                mass: 0.1,
                hardness: 0.7,
                sharpness: 0.2,
                fragility: 0.2,
                heat_resistance: 0.8,
                ..base
            },
            Matter::Knife { .. } => Properties {
                mass: 0.12,
                hardness: 0.6,
                sharpness: 0.85,
                fragility: 0.4,
                flammability: 0.4,
                ..base
            },
            Matter::Antler => Properties {
                mass: 0.9,
                hardness: 0.7,
                flexibility: 0.3,
                fragility: 0.1,
                ..base
            },
            Matter::GreenWood => Properties {
                mass: 0.3,
                hardness: 0.4,
                flexibility: 0.6,
                flammability: 0.2,
                ..base
            },
            Matter::Clay { .. } => Properties {
                mass: 0.5,
                hardness: 0.1,
                plasticity: 0.9,
                heat_resistance: 0.7,
                ..base
            },
            Matter::Sand => Properties {
                mass: 0.5,
                hardness: 0.2,
                heat_resistance: 0.9,
                ..base
            },
            Matter::Ash => Properties {
                mass: 0.05,
                fragility: 1.0,
                heat_resistance: 1.0,
                ..base
            },
            Matter::RawDish { .. } => Properties {
                mass: 0.4,
                hardness: 0.2,
                fragility: 0.8,
                plasticity: 0.4,
                heat_resistance: 0.7,
                ..base
            },
            Matter::FiredDish { .. } => Properties {
                mass: 0.35,
                hardness: 0.6,
                fragility: 0.5,
                heat_resistance: 0.9,
                ..base
            },
            Matter::Shards => Properties {
                mass: 0.3,
                hardness: 0.6,
                sharpness: 0.3,
                fragility: 0.6,
                heat_resistance: 0.9,
                ..base
            },
            Matter::DeadTwigs => Properties {
                mass: 0.25,
                hardness: 0.3,
                flexibility: 0.1,
                fragility: 0.7,
                flammability: 0.9,
                ..base
            },
            Matter::GrassFibre => Properties {
                mass: 0.03,
                flexibility: 0.9,
                fragility: 0.2,
                flammability: 0.6,
                ..base
            },
            Matter::Frond => Properties {
                mass: 0.05,
                flexibility: 0.8,
                fragility: 0.3,
                flammability: 0.2,
                ..base
            },
            Matter::Flower(flower) => Properties {
                mass: 0.02,
                fragility: 0.9,
                flammability: 0.3,
                // Daisies can be eaten; poppies are mildly toxic; buttercups are poisonous
                // (protoanemonin), as in real meadows.
                nutrition: if flower == Flower::Daisy { 0.02 } else { 0.01 },
                toxicity: match flower {
                    Flower::Buttercup => 0.5,
                    Flower::Poppy => 0.2,
                    _ => 0.0,
                },
                ..base
            },
            Matter::Acorn => Properties {
                mass: 0.005,
                hardness: 0.3,
                nutrition: 0.02,
                toxicity: 0.15,
                ..base
            },
            Matter::Hazelnut => Properties {
                mass: 0.003,
                hardness: 0.5,
                nutrition: 0.03,
                ..base
            },
            Matter::Reed => Properties {
                mass: 0.08,
                hardness: 0.15,
                flexibility: 0.7,
                fragility: 0.2,
                flammability: 0.7,
                ..base
            },
            Matter::Mushroom { spotted } => Properties {
                mass: 0.06,
                fragility: 0.8,
                nutrition: 0.08,
                toxicity: if spotted { 0.9 } else { 0.0 },
                ..base
            },
        }
    }

    /// Name shown to the player: what it looks like, never what it does.
    pub fn name(self) -> &'static str {
        match self {
            Matter::Pebble { dark: false } => "Caillou",
            Matter::Pebble { dark: true } => "Caillou sombre",
            Matter::DeadTwigs => "Brindilles mortes",
            Matter::GrassFibre => "Brins d'herbe",
            Matter::Frond => "Fronde de fougère",
            Matter::Flower(Flower::Daisy) => "Marguerite",
            Matter::Flower(Flower::Poppy) => "Coquelicot",
            Matter::Flower(Flower::Lavender) => "Lavande",
            Matter::Flower(Flower::Buttercup) => "Bouton d'or",
            Matter::Mushroom { spotted: false } => "Champignon",
            Matter::Mushroom { spotted: true } => "Champignon tacheté",
            Matter::Clay {
                source: ClaySource::Bank,
            } => "Argile de berge",
            Matter::Clay {
                source: ClaySource::RedEarth,
            } => "Argile rouge",
            Matter::Sand => "Sable",
            Matter::Ash => "Cendres",
            Matter::RawDish { .. } => "Coupelle crue",
            Matter::FiredDish {
                source: ClaySource::Bank,
            } => "Coupelle en terre cuite",
            Matter::FiredDish {
                source: ClaySource::RedEarth,
            } => "Coupelle en terre cuite rouge",
            Matter::Shards => "Tessons",
            Matter::Stick => "Baguette sèche",
            Matter::Flake => "Éclat tranchant",
            Matter::Chips => "Débris de pierre",
            Matter::Knife { .. } => "Couteau emmanché",
            Matter::GreenWood => "Bois vert",
            Matter::Antler => "Bois de cerf",
            Matter::Acorn => "Gland",
            Matter::Hazelnut => "Noisette",
            Matter::Reed => "Roseaux",
        }
    }

    pub fn edible(self) -> bool {
        let p = self.properties();
        p.nutrition > 0.0
    }
}

/// What picking up a plant of the ground gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Harvest {
    /// The whole thing comes away: the plant disappears from the world.
    Whole(Matter),
    /// A part of it (a pebble at the foot of a boulder): the plant stays.
    Part(Matter),
    /// Only with something that cuts in hand; the plant is cut down.
    NeedsBlade(Matter),
}

/// What can be picked from this plant, if anything. Trees and bushes cannot.
pub fn harvest(plant: &PlantInstance) -> Option<Harvest> {
    let flowers = [
        Flower::Daisy,
        Flower::Poppy,
        Flower::Lavender,
        Flower::Buttercup,
    ];
    match plant.plant {
        // Boulders weigh tens of kilograms: one cannot lift them, but pebbles lie at their foot.
        Plant::Stone => Some(Harvest::Part(Matter::Pebble {
            dark: plant.variant.is_multiple_of(3),
        })),
        Plant::DryShrub => Some(Harvest::Whole(Matter::DeadTwigs)),
        // A bush only gives way to a blade (see `GameState`): green wood.
        Plant::Bush | Plant::Hazel => Some(Harvest::NeedsBlade(Matter::GreenWood)),
        Plant::Grass => Some(Harvest::Whole(Matter::GrassFibre)),
        Plant::Fern => Some(Harvest::Whole(Matter::Frond)),
        Plant::Flower => Some(Harvest::Whole(Matter::Flower(
            flowers[plant.variant as usize % 4],
        ))),
        Plant::Mushroom => Some(Harvest::Whole(Matter::Mushroom {
            spotted: mushroom_is_spotted(plant.variant),
        })),
        // Rooted in soft mud, a clump comes away whole when pulled.
        Plant::Reed => Some(Harvest::Whole(Matter::Reed)),
        _ => None,
    }
}

/// Several of the same matter, carried together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stack {
    pub matter: Matter,
    pub count: u32,
}

/// The naturalist's bag: a few kinds of things, a bounded weight.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inventory {
    stacks: Vec<Stack>,
}

/// Kinds of things the bag holds at once, and the weight it carries, in kilograms.
pub const SLOTS: usize = 8;
pub const MAX_MASS: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    TooHeavy,
    Full,
}

impl Inventory {
    pub fn stacks(&self) -> &[Stack] {
        &self.stacks
    }

    pub fn mass(&self) -> f32 {
        self.stacks
            .iter()
            .map(|s| s.matter.properties().mass * s.count as f32)
            .sum()
    }

    /// Adds one, if the bag has room and can bear the weight.
    pub fn add(&mut self, matter: Matter) -> Result<(), Refusal> {
        if self.mass() + matter.properties().mass > MAX_MASS + 1e-4 {
            return Err(Refusal::TooHeavy);
        }
        if let Some(stack) = self.stacks.iter_mut().find(|s| s.matter == matter) {
            stack.count += 1;
            return Ok(());
        }
        if self.stacks.len() >= SLOTS {
            return Err(Refusal::Full);
        }
        self.stacks.push(Stack { matter, count: 1 });
        Ok(())
    }

    /// Takes one out of slot `slot`.
    pub fn take(&mut self, slot: usize) -> Option<Matter> {
        let stack = self.stacks.get_mut(slot)?;
        let matter = stack.matter;
        stack.count -= 1;
        if stack.count == 0 {
            self.stacks.remove(slot);
        }
        Some(matter)
    }
}

crate::save::persist_enum!(Flower {
    Daisy,
    Poppy,
    Lavender,
    Buttercup
});
crate::save::persist_enum!(ClaySource { Bank, RedEarth });
crate::save::persist_struct!(Stack { matter, count });
crate::save::persist_struct!(Inventory { stacks });

impl crate::save::Persist for Matter {
    fn write(&self, w: &mut crate::save::Writer) {
        let (tag, a, b): (u8, u8, Option<ClaySource>) = match *self {
            Matter::Pebble { dark } => (0, dark as u8, None),
            Matter::DeadTwigs => (1, 0, None),
            Matter::GrassFibre => (2, 0, None),
            Matter::Frond => (3, 0, None),
            Matter::Flower(f) => (
                4,
                [
                    Flower::Daisy,
                    Flower::Poppy,
                    Flower::Lavender,
                    Flower::Buttercup,
                ]
                .iter()
                .position(|&x| x == f)
                .unwrap_or(0) as u8,
                None,
            ),
            Matter::Mushroom { spotted } => (5, spotted as u8, None),
            Matter::Clay { source } => (6, 0, Some(source)),
            Matter::Sand => (7, 0, None),
            Matter::Ash => (8, 0, None),
            Matter::RawDish { source } => (9, 0, Some(source)),
            Matter::FiredDish { source } => (10, 0, Some(source)),
            Matter::Shards => (11, 0, None),
            Matter::Stick => (12, 0, None),
            Matter::Flake => (13, 0, None),
            Matter::Chips => (14, 0, None),
            Matter::Knife { uses } => (15, uses, None),
            Matter::GreenWood => (16, 0, None),
            Matter::Antler => (17, 0, None),
            Matter::Acorn => (18, 0, None),
            Matter::Hazelnut => (19, 0, None),
            // 20 is the fallen leaves' (see docs/phase-2-architecture.md).
            Matter::Reed => (21, 0, None),
        };
        w.put(&tag);
        w.put(&a);
        w.put(&b);
    }
    fn read(r: &mut crate::save::Reader) -> crate::save::Result<Self> {
        let tag: u8 = r.get()?;
        let a: u8 = r.get()?;
        let b: Option<ClaySource> = r.get()?;
        let source = || b.ok_or_else(|| "argile sans origine".to_string());
        Ok(match tag {
            0 => Matter::Pebble { dark: a != 0 },
            1 => Matter::DeadTwigs,
            2 => Matter::GrassFibre,
            3 => Matter::Frond,
            4 => Matter::Flower(
                *[
                    Flower::Daisy,
                    Flower::Poppy,
                    Flower::Lavender,
                    Flower::Buttercup,
                ]
                .get(a as usize)
                .ok_or("fleur inconnue")?,
            ),
            5 => Matter::Mushroom { spotted: a != 0 },
            6 => Matter::Clay { source: source()? },
            7 => Matter::Sand,
            8 => Matter::Ash,
            9 => Matter::RawDish { source: source()? },
            10 => Matter::FiredDish { source: source()? },
            11 => Matter::Shards,
            12 => Matter::Stick,
            13 => Matter::Flake,
            14 => Matter::Chips,
            15 => Matter::Knife { uses: a },
            16 => Matter::GreenWood,
            17 => Matter::Antler,
            18 => Matter::Acorn,
            19 => Matter::Hazelnut,
            21 => Matter::Reed,
            _ => return Err("matière inconnue".into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_things_stack_and_the_bag_has_limits() {
        let mut bag = Inventory::default();
        for _ in 0..3 {
            bag.add(Matter::GrassFibre).unwrap();
        }
        assert_eq!(bag.stacks().len(), 1);
        assert_eq!(bag.stacks()[0].count, 3);
        // Pebbles until too heavy.
        let mut pebbles = 0;
        while bag.add(Matter::Pebble { dark: false }).is_ok() {
            pebbles += 1;
        }
        assert_eq!(pebbles, 19);
        assert_eq!(
            bag.add(Matter::Pebble { dark: false }),
            Err(Refusal::TooHeavy)
        );
        // Taking empties the stack.
        for _ in 0..3 {
            assert_eq!(bag.take(0), Some(Matter::GrassFibre));
        }
        assert_eq!(bag.stacks().len(), 1);
    }

    #[test]
    fn reeds_are_pulled_whole_and_make_light_supple_tinder() {
        let reed = PlantInstance {
            plant: Plant::Reed,
            variant: 3,
            rotation: 0,
            mirrored: false,
            scale: 1.0,
            base: [10, 20, 10],
            offset: [0.0; 2],
        };
        assert_eq!(harvest(&reed), Some(Harvest::Whole(Matter::Reed)));
        let p = Matter::Reed.properties();
        assert!(p.flexibility > Matter::Stick.properties().flexibility);
        assert!(p.flammability > 0.5 && p.mass < 0.1, "{p:?}");
        assert!(!Matter::Reed.edible());
        // Saved under its own tag, and read back.
        let mut w = crate::save::Writer::default();
        w.put(&Matter::Reed);
        assert_eq!(w.bytes[0], 21);
        let mut r = crate::save::Reader::new(&w.bytes);
        assert_eq!(r.get::<Matter>(), Ok(Matter::Reed));
    }

    #[test]
    fn names_never_tell_what_is_poisonous() {
        let poisonous = Matter::Mushroom { spotted: true };
        assert!(poisonous.properties().toxicity > 0.5);
        assert!(!poisonous.name().to_lowercase().contains("poison"));
        assert!(!poisonous.name().to_lowercase().contains("toxique"));
    }
}
