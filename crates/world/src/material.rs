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
}

/// Number of materials, `Air` included: the size of the colour table.
pub const MATERIAL_COUNT: usize = 16;

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

    /// Part of a plant (trunk, foliage, cactus), not of the ground.
    pub const fn is_plant(self) -> bool {
        matches!(
            self,
            Material::Wood | Material::Leaves | Material::PineNeedles | Material::Cactus
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
