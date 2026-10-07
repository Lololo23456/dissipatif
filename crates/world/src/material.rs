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
    // Worn and made things (the naturalist's clothes and gear), never generated in the world.
    Skin = 32,
    Khaki = 33,
    OliveCloth = 34,
    Leather = 35,
    Canvas = 36,
    Felt = 37,
    NotebookRed = 38,
    Hair = 39,
    Eye = 40,
    Brass = 41,
    /// Dark furrows and cracks of bark (generated, part of the trees).
    BarkDark = 42,
    // Animals (drawn, never generated in the grid).
    /// A red deer's summer coat, its pale rump, its hooves and muzzle.
    DeerCoat = 43,
    DeerPale = 44,
    Hoof = 45,
    /// The darker coat along a deer's back and neck.
    DeerDark = 46,
    // Phase 2: the marsh, building, the new animals, fallen leaves.
    /// Dark wet bed of a marsh.
    Mud = 47,
    /// Stems and leaves of reeds (a plant).
    Reed = 48,
    /// The brown plume at the top of a reed (a plant).
    ReedHead = 49,
    /// Raw wood worked by the naturalist: posts and beams.
    Timber = 50,
    /// Green rods woven between posts.
    Wattle = 51,
    /// Wattle plastered with clay and grass.
    Daub = 52,
    /// Bundles of grass, reeds or fronds laid as a roof.
    Thatch = 53,
    /// Split wood laid as a floor.
    Plank = 54,
    /// A tawny owl's mottled brown and its pale face and belly.
    OwlBrown = 55,
    OwlPale = 56,
    /// A red fox's coat, its white throat and tail tip, its dark legs and ears.
    FoxRed = 57,
    FoxPale = 58,
    FoxDark = 59,
    /// A vole's brown fur.
    VoleBrown = 60,
    /// Dead leaves, fallen or gathered.
    Litter = 61,
}

/// Number of materials, `Air` included: the size of the colour table.
pub const MATERIAL_COUNT: usize = 62;

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
        Material::Skin,
        Material::Khaki,
        Material::OliveCloth,
        Material::Leather,
        Material::Canvas,
        Material::Felt,
        Material::NotebookRed,
        Material::Hair,
        Material::Eye,
        Material::Brass,
        Material::BarkDark,
        Material::DeerCoat,
        Material::DeerPale,
        Material::Hoof,
        Material::DeerDark,
        Material::Mud,
        Material::Reed,
        Material::ReedHead,
        Material::Timber,
        Material::Wattle,
        Material::Daub,
        Material::Thatch,
        Material::Plank,
        Material::OwlBrown,
        Material::OwlPale,
        Material::FoxRed,
        Material::FoxPale,
        Material::FoxDark,
        Material::VoleBrown,
        Material::Litter,
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
        matches!(self, Material::BarkDark | Material::Reed | Material::ReedHead)
            || (self as u8) < Material::Skin as u8
                && !matches!(
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

    /// Built by the naturalist (posts, walls, floors, roofs): solid, never dug, never a plant.
    pub const fn is_built(self) -> bool {
        matches!(
            self,
            Material::Timber
                | Material::Wattle
                | Material::Daub
                | Material::Thatch
                | Material::Plank
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
