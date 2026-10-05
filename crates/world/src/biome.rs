//! Biomes: what grows and what the ground is made of, chosen from altitude and climate
//! (Whittaker diagram: temperature × humidity decide the vegetation).

use crate::material::Material;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Biome {
    Ocean,
    Beach,
    Desert,
    Savanna,
    Plains,
    Forest,
    Taiga,
    Mountain,
    SnowyPeak,
}

/// Trees and plants a biome grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plant {
    /// Round deciduous tree.
    Broadleaf,
    /// Conical conifer.
    Pine,
    /// Flat-topped savanna tree.
    Acacia,
    Cactus,
    /// Low round shrub.
    Bush,
    /// Slender white-barked tree with a light, narrow crown.
    Birch,
    /// Weeping tree whose foliage hangs in curtains; grows by the water.
    Willow,
    /// Curved trunk and a crown of drooping fronds; grows on beaches.
    Palm,
    /// Bare grey trunk and a few broken branches.
    DeadTree,
    // Ground cover: small, dense, many per area.
    /// A tuft of tall grass blades.
    Grass,
    /// A flower on its stem.
    Flower,
    /// Fronds radiating from the ground.
    Fern,
    /// One to three mushrooms.
    Mushroom,
    /// A stone or a few pebbles.
    Stone,
    /// A twiggy dry shrub of hot, dry lands.
    DryShrub,
}

impl Plant {
    pub const ALL: [Plant; 15] = [
        Plant::Broadleaf,
        Plant::Pine,
        Plant::Acacia,
        Plant::Cactus,
        Plant::Bush,
        Plant::Birch,
        Plant::Willow,
        Plant::Palm,
        Plant::DeadTree,
        Plant::Grass,
        Plant::Flower,
        Plant::Fern,
        Plant::Mushroom,
        Plant::Stone,
        Plant::DryShrub,
    ];

    /// Small things on the ground, scattered densely, not trees.
    pub const fn is_ground_cover(self) -> bool {
        matches!(
            self,
            Plant::Grass
                | Plant::Flower
                | Plant::Fern
                | Plant::Mushroom
                | Plant::Stone
                | Plant::DryShrub
        )
    }
}

impl Biome {
    pub const ALL: [Biome; 9] = [
        Biome::Ocean,
        Biome::Beach,
        Biome::Desert,
        Biome::Savanna,
        Biome::Plains,
        Biome::Forest,
        Biome::Taiga,
        Biome::Mountain,
        Biome::SnowyPeak,
    ];

    /// Top layer of the ground.
    pub const fn surface(self) -> Material {
        match self {
            Biome::Ocean => Material::Sand,
            Biome::Beach => Material::Sand,
            Biome::Desert => Material::DesertSand,
            Biome::Savanna => Material::DryGrass,
            Biome::Plains => Material::Grass,
            Biome::Forest => Material::ForestFloor,
            Biome::Taiga => Material::ForestFloor,
            Biome::Mountain => Material::Rock,
            Biome::SnowyPeak => Material::Snow,
        }
    }

    /// The few layers under the surface.
    pub const fn subsoil(self) -> Material {
        match self {
            Biome::Ocean | Biome::Beach => Material::Sand,
            Biome::Desert => Material::DesertSand,
            Biome::Savanna => Material::Clay,
            Biome::Plains | Biome::Forest | Biome::Taiga => Material::Dirt,
            Biome::Mountain | Biome::SnowyPeak => Material::Rock,
        }
    }

    /// Bedrock, below the subsoil.
    pub const fn bedrock(self) -> Material {
        match self {
            Biome::Desert | Biome::Savanna => Material::Sandstone,
            _ => Material::Rock,
        }
    }

    /// Plants of the biome, each with the probability that a candidate spot grows one.
    pub const fn plants(self) -> &'static [(Plant, f32)] {
        match self {
            Biome::Forest => &[
                (Plant::Broadleaf, 0.55),
                (Plant::Birch, 0.15),
                (Plant::Bush, 0.12),
                (Plant::DeadTree, 0.02),
            ],
            Biome::Taiga => &[
                (Plant::Pine, 0.72),
                (Plant::Birch, 0.06),
                (Plant::DeadTree, 0.03),
            ],
            Biome::Plains => &[
                (Plant::Broadleaf, 0.04),
                (Plant::Birch, 0.02),
                (Plant::Bush, 0.12),
            ],
            Biome::Savanna => &[
                (Plant::Acacia, 0.12),
                (Plant::Bush, 0.08),
                (Plant::DeadTree, 0.02),
            ],
            Biome::Desert => &[(Plant::Cactus, 0.12), (Plant::DeadTree, 0.01)],
            Biome::Mountain => &[(Plant::Pine, 0.08), (Plant::DeadTree, 0.02)],
            Biome::Beach => &[(Plant::Palm, 0.1)],
            Biome::Ocean | Biome::SnowyPeak => &[],
        }
    }

    /// Ground cover of the biome: for each column, the chance of each kind.
    pub const fn ground_cover(self) -> &'static [(Plant, f32)] {
        match self {
            Biome::Plains => &[
                (Plant::Grass, 0.45),
                (Plant::Flower, 0.045),
                (Plant::Stone, 0.01),
            ],
            Biome::Forest => &[
                (Plant::Fern, 0.12),
                (Plant::Grass, 0.12),
                (Plant::Mushroom, 0.025),
                (Plant::Flower, 0.02),
                (Plant::Stone, 0.01),
            ],
            Biome::Taiga => &[
                (Plant::Fern, 0.08),
                (Plant::Mushroom, 0.03),
                (Plant::Stone, 0.02),
                (Plant::Grass, 0.04),
            ],
            Biome::Savanna => &[
                (Plant::Grass, 0.35),
                (Plant::DryShrub, 0.008),
                (Plant::Stone, 0.01),
            ],
            Biome::Desert => &[(Plant::DryShrub, 0.008), (Plant::Stone, 0.012)],
            Biome::Beach => &[(Plant::Stone, 0.01)],
            Biome::Mountain => &[(Plant::Stone, 0.05), (Plant::Grass, 0.04)],
            Biome::SnowyPeak => &[(Plant::Stone, 0.015)],
            Biome::Ocean => &[],
        }
    }
}

/// Climate thresholds, all values in [0, 1].
mod limits {
    pub const DESERT_HEAT: f32 = 0.6;
    pub const DESERT_DRYNESS: f32 = 0.4;
    pub const SAVANNA_HEAT: f32 = 0.55;
    pub const SAVANNA_DRYNESS: f32 = 0.52;
    pub const TAIGA_COLD: f32 = 0.38;
    pub const FOREST_MOISTURE: f32 = 0.55;
    /// Altitude of the snow line where it is coldest, and how much higher where warmest.
    pub const SNOW_LINE: f32 = 15.0;
    pub const SNOW_LINE_WARMTH: f32 = 16.0;
}

/// Land biome (not ocean, not beach: decided by the coast) from the local conditions.
///
/// - `altitude`: height above sea level, in cells; `mountain`: how much the place is part of a
///   mountain range, in [0, 1];
/// - `temperature` and `moisture`: in [0, 1], cold to hot, dry to wet.
pub fn land_biome(altitude: f32, mountain: f32, temperature: f32, moisture: f32) -> Biome {
    use limits::*;
    // Snow line: lower where it is cold. Only on mountains.
    if mountain > 0.3 && altitude > SNOW_LINE + SNOW_LINE_WARMTH * temperature {
        Biome::SnowyPeak
    } else if mountain > 0.45 && altitude > 9.0 {
        Biome::Mountain
    } else if temperature > DESERT_HEAT && moisture < DESERT_DRYNESS {
        Biome::Desert
    } else if temperature > SAVANNA_HEAT && moisture < SAVANNA_DRYNESS {
        Biome::Savanna
    } else if temperature < TAIGA_COLD && moisture > 0.3 {
        Biome::Taiga
    } else if moisture > FOREST_MOISTURE {
        Biome::Forest
    } else {
        Biome::Plains
    }
}

/// How much a place is desert, in [0, 1], smoothly: used to raise dunes without a hard edge.
pub fn desert_weight(temperature: f32, moisture: f32) -> f32 {
    use crate::noise::smoothstep;
    smoothstep(
        limits::DESERT_HEAT - 0.04,
        limits::DESERT_HEAT + 0.06,
        temperature,
    ) * (1.0
        - smoothstep(
            limits::DESERT_DRYNESS - 0.06,
            limits::DESERT_DRYNESS + 0.02,
            moisture,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whittaker_corners() {
        assert_eq!(land_biome(2.0, 0.0, 0.9, 0.1), Biome::Desert);
        assert_eq!(land_biome(2.0, 0.0, 0.58, 0.45), Biome::Savanna);
        assert_eq!(land_biome(2.0, 0.0, 0.2, 0.6), Biome::Taiga);
        assert_eq!(land_biome(2.0, 0.0, 0.5, 0.8), Biome::Forest);
        assert_eq!(land_biome(2.0, 0.0, 0.5, 0.45), Biome::Plains);
        assert_eq!(land_biome(15.0, 0.9, 0.5, 0.5), Biome::Mountain);
        assert_eq!(land_biome(25.0, 0.9, 0.1, 0.5), Biome::SnowyPeak);
        // Same altitude, but warm: no snow.
        assert_eq!(land_biome(25.0, 0.9, 0.9, 0.5), Biome::Mountain);
    }

    #[test]
    fn desert_weight_is_one_in_deserts_and_zero_in_forests() {
        assert!(desert_weight(0.9, 0.1) > 0.99);
        assert!(desert_weight(0.5, 0.8) < 0.01);
    }
}
