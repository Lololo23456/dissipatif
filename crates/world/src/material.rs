//! What a voxel is made of. Colours are not here: they live in `render::palette::materials`,
//! in the same order as these ids.

/// Material of a voxel, stored as one byte. `Air` is empty space.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Material {
    Air = 0,
    Grass = 1,
    ForestFloor = 2,
    Dirt = 3,
    Sand = 4,
    DesertSand = 5,
    Sandstone = 6,
    Rock = 7,
    Snow = 8,
    Gravel = 9,
    Wood = 10,
    Leaves = 11,
    PineNeedles = 12,
    Cactus = 13,
    DryGrass = 14,
    Clay = 15,
    TallGrass = 16,
    FlowerRed = 17,
    FlowerYellow = 18,
    FlowerWhite = 19,
    FlowerViolet = 20,
    Fern = 21,
    MushroomCap = 22,
    MushroomStem = 23,
    BirchBark = 24,
    BirchLeaves = 25,
    PalmTrunk = 26,
    PalmLeaves = 27,
    DeadWood = 28,
    Stone = 29,
    WillowLeaves = 30,
    DryShrub = 31,
}

/// Number of materials, `Air` included: the size of the colour table.
pub const MATERIAL_COUNT: usize = 32;

impl Material {
    /// Every material, in id order.
    pub const ALL: [Material; MATERIAL_COUNT] = [
        Material::Air,
        Material::Grass,
        Material::ForestFloor,
        Material::Dirt,
        Material::Sand,
        Material::DesertSand,
        Material::Sandstone,
        Material::Rock,
        Material::Snow,
        Material::Gravel,
        Material::Wood,
        Material::Leaves,
        Material::PineNeedles,
        Material::Cactus,
        Material::DryGrass,
        Material::Clay,
        Material::TallGrass,
        Material::FlowerRed,
        Material::FlowerYellow,
        Material::FlowerWhite,
        Material::FlowerViolet,
        Material::Fern,
        Material::MushroomCap,
        Material::MushroomStem,
        Material::BirchBark,
        Material::BirchLeaves,
        Material::PalmTrunk,
        Material::PalmLeaves,
        Material::DeadWood,
        Material::Stone,
        Material::WillowLeaves,
        Material::DryShrub,
    ];

    pub const fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Material> {
        Self::ALL.get(id as usize).copied()
    }

    pub const fn is_solid(self) -> bool {
        !matches!(self, Material::Air)
    }

    /// Part of a plant or of what lies on the ground (stones), not of the ground itself.
    pub const fn is_plant(self) -> bool {
        !matches!(
            self,
            Material::Air
                | Material::Grass
                | Material::ForestFloor
                | Material::Dirt
                | Material::Sand
                | Material::DesertSand
                | Material::Sandstone
                | Material::Rock
                | Material::Snow
                | Material::Gravel
                | Material::DryGrass
                | Material::Clay
        )
    }

    /// Leaves of a tree crown: where falling leaves come from.
    pub const fn is_canopy(self) -> bool {
        matches!(
            self,
            Material::Leaves
                | Material::BirchLeaves
                | Material::WillowLeaves
                | Material::PalmLeaves
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_follow_the_table_order() {
        for (i, material) in Material::ALL.iter().enumerate() {
            assert_eq!(material.id() as usize, i);
            assert_eq!(Material::from_id(i as u8), Some(*material));
        }
        assert_eq!(Material::from_id(MATERIAL_COUNT as u8), None);
    }
}
