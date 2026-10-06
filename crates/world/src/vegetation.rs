//! Where plants grow.
//!
//! Spots are picked on a jittered grid (one candidate per grid cell, moved randomly inside it):
//! plants are spread out without the regularity of a grid nor the clumps of pure chance. The
//! local biome decides what grows there and how likely. Each plant is one of the few variants
//! of its kind (`plants::model`), turned by a quarter turn or not.
//!
//! Plants are drawn from their micro-voxel models. The world grid only keeps a coarse copy of
//! them, for what reasons in whole cells (falling leaves, later: cutting, collisions).

use sim::grid::Dims;

use crate::biome::{Biome, Plant};
use crate::material::{MATERIAL_COUNT, Material};
use crate::noise::{hash, hash_unit};
use crate::plants::{Model, VARIANTS};

/// Size of the grid cells: at most one plant per `SPACING × SPACING` columns.
const SPACING: usize = 4;
/// Steepest ground (height difference with a neighbour) a plant grows on.
const MAX_SLOPE: f32 = 1.5;
/// Room kept above the tallest ground for the tallest tree.
pub const HEADROOM: usize = 20;
/// A world cell counts as part of a plant when at least this share of its micro-cells are.
const COARSE_SHARE: f32 = 1.0 / 8.0;

/// One plant of the world. Variant, orientation and size combine so that no two neighbours
/// look alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlantInstance {
    pub plant: Plant,
    pub variant: u32,
    /// Quarter turns around the vertical axis, 0 to 3.
    pub rotation: u32,
    /// Mirrored (x ↦ −x) before turning: with the 4 turns, 8 orientations.
    pub mirrored: bool,
    /// Size relative to the model.
    pub scale: f32,
    /// The world cell it stands in: the first cell above the ground.
    pub base: [usize; 3],
    /// Horizontal offset from the centre of that cell (ground cover is scattered freely).
    pub offset: [f32; 2],
}

/// Range of sizes, relative to the model: bushes vary more than trees.
fn size_range(plant: Plant) -> (f32, f32) {
    match plant {
        Plant::Bush => (0.7, 1.3),
        p if p.is_ground_cover() => (0.7, 1.3),
        _ => (0.8, 1.2),
    }
}

/// Within this distance of water (in cells), a broadleaf tree may be a willow instead.
const WILLOW_REACH: u32 = 3;
const WILLOW_SHARE: f32 = 0.35;
/// Ground cover candidates per column.
const COVER_DRAWS: i64 = 2;

/// What the vegetation needs to know about each column.
pub struct Ground<'a> {
    pub dims: Dims,
    /// Ground voxels per column: the first air cell is at y = `tops[i]`.
    pub tops: &'a [usize],
    pub height: &'a [f32],
    pub biomes: &'a [Biome],
    /// True where water covers the ground.
    pub wet: &'a [bool],
    /// Distance to the nearest wet column, in cells.
    pub water_distance: &'a [u32],
}

/// Picks the plants of the world.
pub fn place(ground: &Ground, seed: u64) -> Vec<PlantInstance> {
    let Dims { nx, ny, nz } = ground.dims;
    let mut plants = Vec::new();
    for gz in 0..nz / SPACING {
        for gx in 0..nx / SPACING {
            let cell = [gx as i64, gz as i64];
            let x = gx * SPACING + (hash(seed, &[cell[0], cell[1], 1]) % SPACING as u64) as usize;
            let z = gz * SPACING + (hash(seed, &[cell[0], cell[1], 2]) % SPACING as u64) as usize;
            if x < 3 || z < 3 || x + 3 >= nx || z + 3 >= nz {
                continue;
            }
            let i = x + nx * z;
            if ground.wet[i] || ground.tops[i] + HEADROOM > ny || steep(ground, x, z) {
                continue;
            }
            // Pick a plant: the biome's options stacked on [0, 1], the draw falls in one or in
            // the remaining gap (nothing grows).
            let draw = hash_unit(seed, &[cell[0], cell[1], 3]);
            let mut cumulative = 0.0;
            let plant = ground.biomes[i].plants().iter().find_map(|&(plant, p)| {
                cumulative += p;
                (draw < cumulative).then_some(plant)
            });
            // Willows by the water.
            let plant = plant.map(|p| {
                let by_water = ground.water_distance[i] <= WILLOW_REACH;
                let willow = hash_unit(seed, &[cell[0], cell[1], 6]) < WILLOW_SHARE;
                if p == Plant::Broadleaf && by_water && willow {
                    Plant::Willow
                } else {
                    p
                }
            });
            if let Some(plant) = plant {
                let pick = hash(seed, &[x as i64, z as i64, 4]);
                let (small, large) = size_range(plant);
                let size = hash_unit(seed, &[x as i64, z as i64, 5]);
                plants.push(PlantInstance {
                    plant,
                    variant: (pick % VARIANTS as u64) as u32,
                    rotation: ((pick >> 8) % 4) as u32,
                    mirrored: (pick >> 12) & 1 == 1,
                    scale: small + (large - small) * size,
                    base: [x, ground.tops[i], z],
                    offset: [0.0; 2],
                });
            }
        }
    }
    plants
}

/// Scatters the ground cover: grass, flowers, ferns, mushrooms, stones, dry shrubs. A few
/// draws per column, each placed freely inside the cell, so no grid shows.
pub fn place_ground_cover(ground: &Ground, seed: u64) -> Vec<PlantInstance> {
    let Dims { nx, ny, nz } = ground.dims;
    let mut cover = Vec::new();
    for z in 1..nz - 1 {
        for x in 1..nx - 1 {
            let i = x + nx * z;
            if ground.wet[i] || ground.tops[i] + 4 > ny {
                continue;
            }
            let options = ground.biomes[i].ground_cover();
            for draw_index in 0..COVER_DRAWS {
                let at = |k: i64| hash_unit(seed, &[x as i64, z as i64, 10 + 8 * draw_index + k]);
                let draw = at(0);
                let mut cumulative = 0.0;
                let Some(plant) = options.iter().find_map(|&(plant, p)| {
                    cumulative += p;
                    (draw < cumulative).then_some(plant)
                }) else {
                    continue;
                };
                let pick = hash(seed, &[x as i64, z as i64, 11 + 8 * draw_index]);
                let (small, large) = size_range(plant);
                cover.push(PlantInstance {
                    plant,
                    variant: (pick % VARIANTS as u64) as u32,
                    rotation: ((pick >> 8) % 4) as u32,
                    mirrored: (pick >> 12) & 1 == 1,
                    scale: small + (large - small) * at(1),
                    base: [x, ground.tops[i], z],
                    offset: [at(2) - 0.5, at(3) - 0.5].map(|o| 0.8 * o),
                });
            }
        }
    }
    cover
}

fn steep(ground: &Ground, x: usize, z: usize) -> bool {
    let nx = ground.dims.nx;
    let h = ground.height[x + nx * z];
    [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
        .iter()
        .any(|&(a, b)| (ground.height[a + nx * b] - h).abs() > MAX_SLOPE)
}

/// Orients horizontal coordinates relative to the anchor: mirrored first if asked, then turned.
/// The shader does the same (`orient` in voxel.wgsl).
pub fn orient(dx: f32, dz: f32, rotation: u32, mirrored: bool) -> (f32, f32) {
    let dx = if mirrored { -dx } else { dx };
    rotate(dx, dz, rotation)
}

/// Turns micro-coordinates relative to the anchor (micro-cell centres, so ±0.5, ±1.5…) by
/// `rotation` quarter turns around the vertical axis.
pub fn rotate(dx: f32, dz: f32, rotation: u32) -> (f32, f32) {
    match rotation % 4 {
        0 => (dx, dz),
        1 => (-dz, dx),
        2 => (-dx, -dz),
        _ => (dz, -dx),
    }
}

/// Writes the coarse copy of a plant into the world grid (only into air cells): each world cell
/// takes the plant material most present among its micro-cells, if there are enough of them.
/// The world cells a plant fills in the coarse grid, and with what: a cell counts when enough
/// of its micro-cells are of the plant (`COARSE_SHARE`), with the material most of them have.
pub fn footprint(dims: Dims, plant: &PlantInstance, model: &Model) -> Vec<([usize; 3], Material)> {
    let m = model.resolution as f32;
    let needed = (COARSE_SHARE * m * m * m) as usize;
    // Micro-cells per world cell and material.
    let mut counts: std::collections::HashMap<(i64, i64, i64), [usize; MATERIAL_COUNT]> =
        Default::default();
    let [ax, ay, az] = model.anchor.map(|v| v as f32);
    for z in 0..model.dims.nz {
        for y in 0..model.dims.ny {
            for x in 0..model.dims.nx {
                let material = model.get(x, y, z);
                if !material.is_plant() {
                    continue;
                }
                // Micro-cell centre relative to the anchor, oriented, sized, then in world cells.
                let (dx, dz) = orient(
                    x as f32 + 0.5 - ax,
                    z as f32 + 0.5 - az,
                    plant.rotation,
                    plant.mirrored,
                );
                let k = plant.scale / m;
                let wx = plant.base[0] as f32 + 0.5 + plant.offset[0] + dx * k;
                let wy = plant.base[1] as f32 + (y as f32 + 0.5 - ay) * k;
                let wz = plant.base[2] as f32 + 0.5 + plant.offset[1] + dz * k;
                let key = (wx.floor() as i64, wy.floor() as i64, wz.floor() as i64);
                counts.entry(key).or_insert([0; MATERIAL_COUNT])[material.id() as usize] += 1;
            }
        }
    }
    let mut cells: Vec<([usize; 3], Material)> = counts
        .into_iter()
        .filter_map(|((x, y, z), count)| {
            let in_world = x >= 0
                && y >= 0
                && z >= 0
                && (x as usize) < dims.nx
                && (y as usize) < dims.ny
                && (z as usize) < dims.nz;
            let (best, &n) = count.iter().enumerate().max_by_key(|(_, n)| **n)?;
            (in_world && n >= needed).then(|| {
                (
                    [x as usize, y as usize, z as usize],
                    Material::from_id(best as u8).unwrap_or(Material::Air),
                )
            })
        })
        .collect();
    // Same order on every run (the map's order is not).
    cells.sort_by_key(|(c, _)| (c[2], c[1], c[0]));
    cells
}

/// Writes a plant's coarse copy into the grid (empty cells only).
pub fn stamp(blocks: &mut [u8], dims: Dims, plant: &PlantInstance, model: &Model) {
    for ([x, y, z], material) in footprint(dims, plant, model) {
        let i = dims.index(x, y, z);
        if blocks[i] == Material::Air.id() {
            blocks[i] = material.id();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plants::model;

    fn flat(biome: Biome) -> (Dims, Vec<usize>, Vec<f32>, Vec<Biome>, Vec<bool>) {
        let dims = Dims {
            nx: 48,
            ny: 40,
            nz: 48,
        };
        let n = dims.nx * dims.nz;
        (
            dims,
            vec![5; n],
            vec![5.0; n],
            vec![biome; n],
            vec![false; n],
        )
    }

    fn far_from_water(dims: Dims) -> Vec<u32> {
        vec![100; dims.nx * dims.nz]
    }

    fn place_on(biome: Biome) -> Vec<PlantInstance> {
        let (dims, tops, height, biomes, wet) = flat(biome);
        let water_distance = far_from_water(dims);
        place(
            &Ground {
                dims,
                tops: &tops,
                height: &height,
                biomes: &biomes,
                wet: &wet,
                water_distance: &water_distance,
            },
            42,
        )
    }

    fn cover_on(biome: Biome) -> Vec<PlantInstance> {
        let (dims, tops, height, biomes, wet) = flat(biome);
        let water_distance = far_from_water(dims);
        place_ground_cover(
            &Ground {
                dims,
                tops: &tops,
                height: &height,
                biomes: &biomes,
                wet: &wet,
                water_distance: &water_distance,
            },
            42,
        )
    }

    #[test]
    fn plains_are_grassy_and_ground_cover_is_scattered() {
        let cover = cover_on(Biome::Plains);
        let grass = cover.iter().filter(|p| p.plant == Plant::Grass).count();
        assert!(grass > 48 * 48 / 2, "only {grass} tufts");
        assert!(cover.iter().any(|p| p.plant == Plant::Flower));
        assert!(
            cover
                .iter()
                .all(|p| p.offset.iter().all(|o| o.abs() <= 0.4))
        );
        assert!(cover_on(Biome::Ocean).is_empty());
    }

    #[test]
    fn willows_grow_by_the_water() {
        let (dims, tops, height, biomes, wet) = flat(Biome::Forest);
        let near = vec![1; dims.nx * dims.nz];
        let ground = Ground {
            dims,
            tops: &tops,
            height: &height,
            biomes: &biomes,
            wet: &wet,
            water_distance: &near,
        };
        let plants = place(&ground, 42);
        assert!(plants.iter().any(|p| p.plant == Plant::Willow));
        assert!(
            place_on(Biome::Forest)
                .iter()
                .all(|p| p.plant != Plant::Willow)
        );
    }

    #[test]
    fn forests_are_dense_and_deserts_sparse() {
        let (forest, desert) = (place_on(Biome::Forest).len(), place_on(Biome::Desert).len());
        assert!(forest > 3 * desert, "forest {forest}, desert {desert}");
        // Beaches grow palms only, the open sea nothing.
        let beach = place_on(Biome::Beach);
        assert!(!beach.is_empty() && beach.iter().all(|p| p.plant == Plant::Palm));
        assert!(place_on(Biome::Ocean).is_empty());
    }

    #[test]
    fn plants_stand_on_the_ground_and_are_varied() {
        let plants = place_on(Biome::Forest);
        assert!(plants.iter().all(|p| p.base[1] == 5));
        let variants: std::collections::HashSet<_> = plants.iter().map(|p| p.variant).collect();
        assert!(variants.len() > 2);
    }

    #[test]
    fn sizes_and_orientations_vary() {
        let plants = place_on(Biome::Forest);
        for p in &plants {
            let (small, large) = size_range(p.plant);
            assert!((small..=large).contains(&p.scale), "{p:?}");
        }
        let sizes: Vec<f32> = plants.iter().map(|p| p.scale).collect();
        let spread = sizes.iter().cloned().fold(f32::MIN, f32::max)
            - sizes.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread > 0.2, "sizes barely vary: {spread}");
        assert!(plants.iter().any(|p| p.mirrored) && plants.iter().any(|p| !p.mirrored));
    }

    #[test]
    fn mirror_then_turn() {
        assert_eq!(orient(1.0, 2.0, 0, true), (-1.0, 2.0));
        assert_eq!(orient(1.0, 2.0, 1, true), rotate(-1.0, 2.0, 1));
    }

    #[test]
    fn quarter_turns_compose() {
        let (x, z) = (2.5, -0.5);
        let (a, b) = rotate(x, z, 1);
        assert_eq!(rotate(a, b, 3), (x, z));
        assert_eq!(rotate(x, z, 4), (x, z));
    }

    #[test]
    fn coarse_copy_has_a_trunk_and_foliage_and_respects_the_ground() {
        let dims = Dims {
            nx: 32,
            ny: 40,
            nz: 32,
        };
        let mut blocks = vec![Material::Air.id(); dims.len()];
        for z in 0..32 {
            for x in 0..32 {
                for y in 0..5 {
                    blocks[dims.index(x, y, z)] = Material::Grass.id();
                }
            }
        }
        let tree = PlantInstance {
            plant: Plant::Broadleaf,
            variant: 0,
            rotation: 1,
            mirrored: true,
            scale: 1.0,
            base: [16, 5, 16],
            offset: [0.0; 2],
        };
        stamp(&mut blocks, dims, &tree, &model(Plant::Broadleaf, 0, 9));
        let count = |m: Material| blocks.iter().filter(|&&b| b == m.id()).count();
        assert!(count(Material::Wood) >= 3, "no trunk");
        assert!(count(Material::Leaves) >= 10, "no crown");
        assert_eq!(count(Material::Grass), 32 * 32 * 5, "ground overwritten");
        assert_eq!(
            blocks[dims.index(16, 5, 16)],
            Material::Wood.id(),
            "trunk not at the base"
        );
    }
}
