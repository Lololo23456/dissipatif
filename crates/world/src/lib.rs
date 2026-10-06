//! Génération procédurale du monde : relief, climat, biomes, eau, végétation.
//! Pure et déterministe : même graine, même monde. Aucune dépendance GPU ni fenêtre.
//!
//! Étapes de `World::generate` :
//! 1. relief et climat (`land`) : mer, plaines, collines, massifs, dunes ; température, humidité ;
//! 2. eau (`drainage`) : lacs dans les cuvettes, rivières là où le débit accumulé est fort ;
//! 3. biomes (`biome`) : diagramme de Whittaker, plages le long de la côte ;
//! 4. voxels : surface, sous-sol et roche selon le biome, roche sur les pentes raides ;
//! 5. végétation (`vegetation`) : arbres, cactus, buissons.

pub mod biome;
pub mod drainage;
pub mod land;
pub mod material;
pub mod noise;
pub mod plants;
pub mod vegetation;

use sim::grid::{Dims, Field2};

pub use biome::{Biome, Plant};
pub use material::{MATERIAL_COUNT, Material};
pub use plants::Model;
pub use vegetation::PlantInstance;

/// Columns within this distance of the open sea and at most `BEACH_RISE` above it are beach.
const BEACH_WIDTH: u32 = 5;
const BEACH_RISE: f32 = 1.8;
/// Height difference with a neighbour above which the ground shows bare rock (cliffs).
const CLIFF: f32 = 2.2;
/// Layers of subsoil under the surface, before bedrock.
const SUBSOIL_DEPTH: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldConfig {
    pub dims: Dims,
    /// Height of the sea surface, in cells.
    pub sea_level: f32,
    pub seed: u64,
}

impl WorldConfig {
    /// The default world: 256 × 64 × 256 cells, sea at 20.
    pub fn standard(seed: u64) -> Self {
        Self {
            dims: Dims {
                nx: 256,
                ny: 64,
                nz: 256,
            },
            sea_level: 20.0,
            seed,
        }
    }
}

pub struct World {
    pub config: WorldConfig,
    /// Height of the ground of each column (continuous; voxels are rounded from it).
    pub ground: Field2,
    /// Level of the water surface of each column, `f32::NEG_INFINITY` where it is dry.
    pub water: Field2,
    biomes: Vec<Biome>,
    /// Ground voxels per column (plants excluded): the first cell above ground is at this y.
    tops: Vec<usize>,
    /// Material of every voxel, as `Material` ids, indexed like `Dims::index`.
    blocks: Vec<u8>,
    plants: Vec<PlantInstance>,
    /// `plants::VARIANTS` models per kind of plant, in `Plant::ALL` order.
    models: Vec<Model>,
}

impl World {
    pub fn generate(config: WorldConfig) -> Self {
        let WorldConfig {
            dims,
            sea_level,
            seed,
        } = config;
        let (nx, nz) = (dims.nx, dims.nz);
        let n = nx * nz;
        assert!(dims.ny >= 40, "world too low: {dims:?}");

        let mut land = land::generate(nx, nz, sea_level, seed);
        // Biomes from the relief before rivers are carved: a river does not change the climate.
        let biomes: Vec<Biome> = (0..n)
            .map(|i| {
                let altitude = land.height.data[i] - sea_level;
                if land.ocean[i] {
                    Biome::Ocean
                } else if land.coast_distance[i] <= BEACH_WIDTH && altitude <= BEACH_RISE {
                    Biome::Beach
                } else {
                    biome::land_biome(
                        altitude,
                        land.mountain.data[i],
                        land.temperature.data[i],
                        land.moisture.data[i],
                    )
                }
            })
            .collect();
        let drainage = drainage::drain(&mut land.height, &land.ocean, &land.moisture, sea_level);
        let ground = land.height;
        let water = drainage.water_level;

        // Plants only grow where there is room above (see `vegetation::HEADROOM`); the ground
        // itself may rise almost to the top of the world.
        let max_top = dims.ny - 2;
        let tops: Vec<usize> = ground
            .data
            .iter()
            .map(|&h| (h.round().max(1.0) as usize).min(max_top))
            .collect();
        let wet: Vec<bool> = (0..n)
            .map(|i| water.data[i] > ground.data[i] + 0.05)
            .collect();

        let mut blocks = vec![Material::Air.id(); dims.len()];
        for z in 0..nz {
            for x in 0..nx {
                let i = x + nx * z;
                let column = column_materials(&ground, x, z, biomes[i], wet[i]);
                let top = tops[i];
                for y in 0..top {
                    let depth = top - 1 - y;
                    let material = if depth == 0 {
                        column.0
                    } else if depth <= SUBSOIL_DEPTH {
                        column.1
                    } else {
                        column.2
                    };
                    blocks[dims.index(x, y, z)] = material.id();
                }
            }
        }

        let water_distance = land::distance_to(&wet, nx, nz);
        let vegetation_ground = vegetation::Ground {
            dims,
            tops: &tops,
            height: &ground.data,
            biomes: &biomes,
            wet: &wet,
            water_distance: &water_distance,
        };
        let mut plants = vegetation::place(&vegetation_ground, seed);
        let models: Vec<Model> = Plant::ALL
            .iter()
            .flat_map(|&plant| (0..plants::VARIANTS).map(move |v| plants::model(plant, v, seed)))
            .collect();
        // Only trees and shrubs get a coarse copy in the world grid; ground cover is too small.
        for plant in &plants {
            let model = &models[model_index(plant.plant, plant.variant)];
            vegetation::stamp(&mut blocks, dims, plant, model);
        }
        plants.extend(vegetation::place_ground_cover(&vegetation_ground, seed));

        Self {
            config,
            ground,
            water,
            biomes,
            tops,
            blocks,
            plants,
            models,
        }
    }

    pub fn dims(&self) -> Dims {
        self.config.dims
    }

    pub fn block(&self, x: usize, y: usize, z: usize) -> Material {
        Material::from_id(self.blocks[self.dims().index(x, y, z)]).unwrap_or(Material::Air)
    }

    /// Every voxel's material id, indexed like `Dims::index` (x fastest, then y, then z).
    pub fn blocks(&self) -> &[u8] {
        &self.blocks
    }

    pub fn biome(&self, x: usize, z: usize) -> Biome {
        self.biomes[x + self.dims().nx * z]
    }

    /// Ground voxels of column (x, z), plants excluded.
    pub fn ground_top(&self, x: usize, z: usize) -> usize {
        self.tops[x + self.dims().nx * z]
    }

    /// Digs out the top ground voxel of column (x, z): it becomes air and the column is one
    /// lower. Returns what was dug, or None if too little ground is left (bedrock).
    pub fn remove_top(&mut self, x: usize, z: usize) -> Option<Material> {
        let dims = self.dims();
        let column = x + dims.nx * z;
        let top = self.tops[column];
        if top <= 2 {
            return None;
        }
        let i = dims.index(x, top - 1, z);
        let dug = Material::from_id(self.blocks[i])?;
        self.blocks[i] = Material::Air.id();
        self.tops[column] = top - 1;
        let height = &mut self.ground.data[column];
        *height = height.min((top - 1) as f32);
        Some(dug)
    }

    /// Water surface of column (x, z), if water covers its ground.
    pub fn water_level(&self, x: usize, z: usize) -> Option<f32> {
        let i = x + self.dims().nx * z;
        (self.water.data[i] > self.ground.data[i]).then_some(self.water.data[i])
    }

    /// Number of trees, cacti and bushes grown (ground cover excluded).
    pub fn plant_count(&self) -> usize {
        self.plants
            .iter()
            .filter(|p| !p.plant.is_ground_cover())
            .count()
    }

    /// Number of grass tufts, flowers, ferns, mushrooms, stones and dry shrubs.
    pub fn ground_cover_count(&self) -> usize {
        self.plants.len() - self.plant_count()
    }

    /// Every plant of the world, ground cover included.
    pub fn plants(&self) -> &[PlantInstance] {
        &self.plants
    }

    /// The micro-voxel model of a variant of a plant.
    pub fn model(&self, plant: Plant, variant: u32) -> &Model {
        &self.models[model_index(plant, variant)]
    }
}

fn model_index(plant: Plant, variant: u32) -> usize {
    let kind = Plant::ALL.iter().position(|&p| p == plant).unwrap_or(0);
    kind * plants::VARIANTS as usize + variant as usize
}

/// (surface, subsoil, bedrock) materials of a column.
fn column_materials(
    ground: &Field2,
    x: usize,
    z: usize,
    biome: Biome,
    wet: bool,
) -> (Material, Material, Material) {
    let h = ground.get(x, z);
    let steepest = [
        (x.wrapping_sub(1), z),
        (x + 1, z),
        (x, z.wrapping_sub(1)),
        (x, z + 1),
    ]
    .iter()
    .filter(|&&(a, b)| a < ground.nx && b < ground.nz)
    .map(|&(a, b)| (ground.get(a, b) - h).abs())
    .fold(0.0, f32::max);
    if wet {
        // Under water: sand in the sea and lakes, gravel in the steeper river beds.
        let bed = if steepest > 0.8 {
            Material::Gravel
        } else {
            Material::Sand
        };
        return (bed, biome.subsoil(), biome.bedrock());
    }
    let sandy = matches!(biome, Biome::Desert | Biome::Beach);
    if steepest > CLIFF && !sandy {
        // Cliffs: bare rock (sandstone in dry lands).
        let rock = biome.bedrock();
        return (rock, rock, rock);
    }
    (biome.surface(), biome.subsoil(), biome.bedrock())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small(seed: u64) -> World {
        World::generate(WorldConfig {
            dims: Dims {
                nx: 128,
                ny: 48,
                nz: 128,
            },
            sea_level: 16.0,
            seed,
        })
    }

    #[test]
    fn same_seed_same_world() {
        assert_eq!(small(3).blocks(), small(3).blocks());
        assert_ne!(small(3).blocks(), small(4).blocks());
    }

    #[test]
    fn world_ends_in_the_sea() {
        let world = small(5);
        for i in 0..128 {
            for (x, z) in [(i, 0), (i, 127), (0, i), (127, i)] {
                assert_eq!(world.biome(x, z), Biome::Ocean, "({x}, {z})");
                assert!(world.water_level(x, z).is_some(), "dry edge at ({x}, {z})");
            }
        }
    }

    #[test]
    fn varied_biomes_and_plants() {
        // Over a few seeds, most biomes appear somewhere.
        let mut seen = std::collections::HashSet::new();
        let mut plants = 0;
        for seed in 1..=4 {
            let world = small(seed);
            plants += world.plant_count();
            for z in 0..128 {
                for x in 0..128 {
                    seen.insert(world.biome(x, z));
                }
            }
        }
        assert!(seen.len() >= 6, "only {seen:?}");
        assert!(plants > 100, "{plants} plants");
    }

    #[test]
    #[ignore]
    fn biome_census() {
        // `cargo test -p world --release -- --ignored --nocapture`
        for seed in 1..=6 {
            let world = World::generate(WorldConfig::standard(seed));
            let (nx, nz) = (world.dims().nx, world.dims().nz);
            let mut counts = std::collections::BTreeMap::new();
            for z in 0..nz {
                for x in 0..nx {
                    *counts
                        .entry(format!("{:?}", world.biome(x, z)))
                        .or_insert(0usize) += 1;
                }
            }
            let line: Vec<String> = counts
                .iter()
                .map(|(b, c)| format!("{b} {:.0}%", 100.0 * *c as f32 / (nx * nz) as f32))
                .collect();
            eprintln!("graine {seed} : {}", line.join(", "));
            let config = WorldConfig::standard(seed);
            let land = land::generate(nx, nz, config.sea_level, seed);
            let pct = |field: &Field2| {
                let mut v: Vec<f32> = (0..nx * nz)
                    .filter(|&i| !land.ocean[i])
                    .map(|i| field.data[i])
                    .collect();
                v.sort_by(f32::total_cmp);
                let q = |p: f32| v[((v.len() - 1) as f32 * p) as usize];
                format!("p10 {:.2} p50 {:.2} p90 {:.2}", q(0.1), q(0.5), q(0.9))
            };
            eprintln!(
                "    température {}, humidité {}",
                pct(&land.temperature),
                pct(&land.moisture)
            );
        }
    }

    #[test]
    fn columns_are_solid_up_to_their_top() {
        let world = small(6);
        for z in (0..128).step_by(7) {
            for x in (0..128).step_by(7) {
                let top = world.ground_top(x, z);
                assert!((0..top).all(|y| world.block(x, y, z).is_solid()));
                let above = world.block(x, top, z);
                assert!(
                    above == Material::Air || above.is_plant(),
                    "{above:?} above ground"
                );
            }
        }
    }

    #[test]
    fn sea_surface_is_flat() {
        let world = small(7);
        let config = world.config;
        for z in 0..128 {
            for x in 0..128 {
                if world.biome(x, z) == Biome::Ocean {
                    assert_eq!(world.water_level(x, z), Some(config.sea_level));
                }
            }
        }
    }
}
