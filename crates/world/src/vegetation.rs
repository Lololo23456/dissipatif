//! Plants: trees, cacti and bushes, built from voxels.
//!
//! Spots are picked on a jittered grid (one candidate per grid cell, moved randomly inside it):
//! plants are spread out without the regularity of a grid nor the clumps of pure chance. The
//! local biome decides what grows there and how likely. Every shape comes from a hash of its
//! position: the same seed always grows the same forest.

use sim::grid::Dims;

use crate::biome::{Biome, Plant};
use crate::material::Material;
use crate::noise::{hash, hash_unit};

/// Size of the grid cells: at most one plant per `SPACING × SPACING` columns.
const SPACING: usize = 4;
/// Steepest ground (height difference with a neighbour) a plant grows on.
const MAX_SLOPE: f32 = 1.5;
/// Room kept above the tallest ground for the tallest tree.
pub const HEADROOM: usize = 14;

/// What the vegetation needs to know about each column.
pub struct Ground<'a> {
    pub dims: Dims,
    /// Ground voxels per column: the first air cell is at y = `tops[i]`.
    pub tops: &'a [usize],
    pub height: &'a [f32],
    pub biomes: &'a [Biome],
    /// True where water covers the ground.
    pub wet: &'a [bool],
}

/// Grows plants into `blocks` (only into air cells). Returns how many plants grew.
pub fn grow(blocks: &mut [u8], ground: &Ground, seed: u64) -> usize {
    let Dims { nx, ny, nz } = ground.dims;
    let mut grown = 0;
    for gz in 0..nz / SPACING {
        for gx in 0..nx / SPACING {
            let cell = [gx as i64, gz as i64];
            let x = gx * SPACING + (hash(seed, &[cell[0], cell[1], 1]) % SPACING as u64) as usize;
            let z = gz * SPACING + (hash(seed, &[cell[0], cell[1], 2]) % SPACING as u64) as usize;
            if x < 2 || z < 2 || x + 2 >= nx || z + 2 >= nz {
                continue;
            }
            let i = x + nx * z;
            if ground.wet[i] || ground.tops[i] + HEADROOM > ny || steep(ground, x, z) {
                continue;
            }
            // Pick a plant: the biome's options stacked on [0, 1], the draw falls in one or
            // in the remaining gap (nothing grows).
            let draw = hash_unit(seed, &[cell[0], cell[1], 3]);
            let mut cumulative = 0.0;
            let plant = ground.biomes[i].plants().iter().find_map(|&(plant, p)| {
                cumulative += p;
                (draw < cumulative).then_some(plant)
            });
            if let Some(plant) = plant {
                let shape_seed = hash(seed, &[x as i64, z as i64, 4]);
                build(
                    blocks,
                    ground.dims,
                    [x, ground.tops[i], z],
                    plant,
                    shape_seed,
                );
                grown += 1;
            }
        }
    }
    grown
}

fn steep(ground: &Ground, x: usize, z: usize) -> bool {
    let nx = ground.dims.nx;
    let h = ground.height[x + nx * z];
    [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
        .iter()
        .any(|&(a, b)| (ground.height[a + nx * b] - h).abs() > MAX_SLOPE)
}

/// Builds one plant whose base (first cell above the ground) is `base`.
fn build(blocks: &mut [u8], dims: Dims, base: [usize; 3], plant: Plant, seed: u64) {
    let mut set = |dx: i64, dy: i64, dz: i64, material: Material| {
        let (x, y, z) = (
            base[0] as i64 + dx,
            base[1] as i64 + dy,
            base[2] as i64 + dz,
        );
        if x < 0
            || y < 0
            || z < 0
            || x >= dims.nx as i64
            || y >= dims.ny as i64
            || z >= dims.nz as i64
        {
            return;
        }
        let i = dims.index(x as usize, y as usize, z as usize);
        if blocks[i] == Material::Air.id() {
            blocks[i] = material.id();
        }
    };
    // Random in [0, 1) per (cell of the shape, purpose): ragged foliage edges.
    let rand = |dx: i64, dy: i64, dz: i64| hash_unit(seed, &[dx, dy, dz]);
    let pick = |range: u64, salt: u64| (hash(seed, &[salt as i64]) % range) as i64;

    match plant {
        Plant::Broadleaf => {
            let trunk = 4 + pick(3, 1);
            let radius = 2 + pick(2, 2);
            for y in 0..trunk {
                set(0, y, 0, Material::Wood);
            }
            // Ellipsoid crown centred at the top of the trunk, flatter than tall, with its
            // outer cells randomly missing so it does not look like a ball.
            let r = radius as f32;
            for dy in -radius + 1..=radius {
                for dz in -radius..=radius {
                    for dx in -radius..=radius {
                        let d = ((dx * dx + dz * dz) as f32 / (r * r)
                            + (dy * dy) as f32 / (0.8 * r).powi(2))
                        .sqrt();
                        if d <= 1.0 && (d < 0.75 || rand(dx, dy, dz) > 0.35) {
                            set(dx, trunk + dy, dz, Material::Leaves);
                        }
                    }
                }
            }
        }
        Plant::Pine => {
            let trunk = 7 + pick(4, 1);
            for y in 0..trunk {
                set(0, y, 0, Material::Wood);
            }
            // Cone of needles from a third of the trunk up to just above its top.
            let start = trunk / 3;
            let top = trunk + 1;
            for y in start..=top {
                let t = (y - start) as f32 / (top - start) as f32;
                let radius = 3.0 * (1.0 - t) + 0.4;
                let reach = radius.floor() as i64;
                for dz in -reach..=reach {
                    for dx in -reach..=reach {
                        let d = ((dx * dx + dz * dz) as f32).sqrt();
                        // Tiers: every other layer is a bit narrower.
                        let tier = if (y - start) % 2 == 1 { 0.8 } else { 0.0 };
                        if d <= radius - tier && (d < radius - 1.0 || rand(dx, y, dz) > 0.3) {
                            set(dx, y, dz, Material::PineNeedles);
                        }
                    }
                }
            }
            // The tip always covers the top of the trunk (narrow tiers can leave it bare).
            set(0, trunk, 0, Material::PineNeedles);
            set(0, top, 0, Material::PineNeedles);
        }
        Plant::Acacia => {
            let trunk = 4 + pick(2, 1);
            // Slightly leaning trunk: shifted by one cell for its top half.
            let lean = [(1, 0), (-1, 0), (0, 1), (0, -1)][pick(4, 2) as usize];
            for y in 0..trunk {
                let (dx, dz) = if y > trunk / 2 { lean } else { (0, 0) };
                set(dx, y, dz, Material::Wood);
            }
            // Wide flat crown, two layers.
            for (dy, radius) in [(0, 3.2f32), (1, 2.2)] {
                let reach = radius.floor() as i64;
                for dz in -reach..=reach {
                    for dx in -reach..=reach {
                        let d = ((dx * dx + dz * dz) as f32).sqrt();
                        if d <= radius && (d < radius - 1.0 || rand(dx, dy, dz) > 0.3) {
                            set(dx + lean.0, trunk + dy, dz + lean.1, Material::Leaves);
                        }
                    }
                }
            }
        }
        Plant::Cactus => {
            let height = 2 + pick(3, 1);
            for y in 0..height {
                set(0, y, 0, Material::Cactus);
            }
            // Arms: out by one cell at mid height, then up.
            for (k, (dx, dz)) in [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().enumerate() {
                if height >= 3 && rand(dx, k as i64, dz) < 0.35 {
                    let at = 1 + pick(height as u64 - 2, 3 + k as u64);
                    set(dx, at, dz, Material::Cactus);
                    set(dx, at + 1, dz, Material::Cactus);
                }
            }
        }
        Plant::Bush => {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let corner = dx != 0 && dz != 0;
                    if !corner || rand(dx, 0, dz) > 0.5 {
                        set(dx, 0, dz, Material::Leaves);
                    }
                }
            }
            set(0, 1, 0, Material::Leaves);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_ground(biome: Biome) -> (Dims, Vec<usize>, Vec<f32>, Vec<Biome>, Vec<bool>) {
        let dims = Dims {
            nx: 32,
            ny: 32,
            nz: 32,
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

    fn grow_on(biome: Biome) -> (Dims, Vec<u8>, usize) {
        let (dims, tops, height, biomes, wet) = flat_ground(biome);
        let mut blocks = vec![0u8; dims.len()];
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                for y in 0..5 {
                    blocks[dims.index(x, y, z)] = Material::Grass.id();
                }
            }
        }
        let ground = Ground {
            dims,
            tops: &tops,
            height: &height,
            biomes: &biomes,
            wet: &wet,
        };
        let grown = grow(&mut blocks, &ground, 42);
        (dims, blocks, grown)
    }

    #[test]
    fn forests_are_dense_and_deserts_sparse() {
        let (_, _, forest) = grow_on(Biome::Forest);
        let (_, _, desert) = grow_on(Biome::Desert);
        assert!(forest > 3 * desert, "forest {forest}, desert {desert}");
        let (_, _, beach) = grow_on(Biome::Beach);
        assert_eq!(beach, 0);
    }

    #[test]
    fn plants_stand_on_the_ground() {
        // Every trunk or cactus cell either touches the ground or sits on its own kind.
        let (dims, blocks, _) = grow_on(Biome::Taiga);
        let at =
            |x: usize, y: usize, z: usize| Material::from_id(blocks[dims.index(x, y, z)]).unwrap();
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                if at(x, 5, z) == Material::Wood {
                    assert_eq!(at(x, 4, z), Material::Grass, "trunk not on the ground");
                }
            }
        }
    }

    #[test]
    fn plants_never_replace_the_ground() {
        let (dims, blocks, _) = grow_on(Biome::Forest);
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                for y in 0..5 {
                    assert_eq!(blocks[dims.index(x, y, z)], Material::Grass.id());
                }
            }
        }
    }

    #[test]
    fn no_bare_trunk_tips() {
        // Above every wood cell there is wood or foliage, never air: no trunk sticks out.
        let (dims, blocks, _) = grow_on(Biome::Taiga);
        let at =
            |x: usize, y: usize, z: usize| Material::from_id(blocks[dims.index(x, y, z)]).unwrap();
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                for y in 5..dims.ny - 1 {
                    if at(x, y, z) == Material::Wood {
                        assert_ne!(
                            at(x, y + 1, z),
                            Material::Air,
                            "bare trunk at ({x}, {y}, {z})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn same_seed_same_forest() {
        assert_eq!(grow_on(Biome::Forest).1, grow_on(Biome::Forest).1);
    }
}
