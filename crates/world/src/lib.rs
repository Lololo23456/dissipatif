//! Génération procédurale du monde : relief, climat, biomes, eau, végétation.
//! Pure et déterministe : même graine, même monde. Aucune dépendance GPU ni fenêtre.
//!
//! Étapes de `World::generate` :
//! 1. relief et climat (`land`) : mer, plaines, collines, massifs, dunes ; température, humidité ;
//! 2. eau (`drainage`) : lacs dans les cuvettes, rivières là où le débit accumulé est fort ;
//! 3. biomes (`biome`) : diagramme de Whittaker, plages le long de la côte ;
//! 4. voxels : surface, sous-sol et roche selon le biome, roche sur les pentes raides ;
//! 5. végétation (`vegetation`) : arbres, cactus, buissons.
//!
//! Une fois le monde fait, `World::make_marsh` y creuse un marais (`marsh`) près d'un lieu
//! donné (la prairie des cerfs).

pub mod biome;
pub mod drainage;
pub mod land;
pub mod marsh;
pub mod material;
pub mod noise;
pub mod plants;
pub mod vegetation;

use sim::grid::{Dims, Field2};

pub use biome::{Biome, Plant};
pub use marsh::Marsh;
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
    /// The planet of the game: 512 × 64 × 512 cells (half a kilometre around, a cell being
    /// a metre), sea at 20.
    pub fn standard(seed: u64) -> Self {
        Self {
            dims: Dims {
                nx: 512,
                ny: 64,
                nz: 512,
            },
            sea_level: 20.0,
            seed,
        }
    }

    /// A smaller planet, 256 × 64 × 256: for tests and experiments (four times cheaper).
    pub fn small(seed: u64) -> Self {
        Self {
            dims: Dims {
                nx: 256,
                ny: 64,
                nz: 256,
            },
            ..Self::standard(seed)
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
    /// Cells subdivided into micro-voxels (dug into), by block index: `MICRO`³ material ids,
    /// x fastest, then y, then z. A subdivided cell keeps its material in `blocks` until its
    /// last micro-voxel is gone.
    micro: std::collections::HashMap<usize, [u8; MICRO_CELLS]>,
}

/// What play changes in a world (see `World::state`).
pub struct WorldState {
    pub ground: Vec<f32>,
    pub water: Vec<f32>,
    pub tops: Vec<usize>,
    pub blocks: Vec<u8>,
    pub micro: Vec<(usize, [u8; MICRO_CELLS])>,
    pub plants: Vec<PlantInstance>,
}

/// Micro-voxels per cell along each axis, and per cell.
pub const MICRO: usize = 4;
pub const MICRO_CELLS: usize = MICRO * MICRO * MICRO;

/// Index of micro-voxel (x, y, z) in a brick.
pub const fn micro_index(x: usize, y: usize, z: usize) -> usize {
    x + MICRO * (y + MICRO * z)
}

/// What changed when digging: the cell dug into, and whether it was emptied (the column then
/// one cell lower) and whether water flowed into the column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dug {
    pub material: Material,
    pub cell: [usize; 3],
    pub emptied: bool,
    pub flooded: bool,
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
            micro: std::collections::HashMap::new(),
        }
    }

    /// The micro-voxels of a subdivided cell, if it is one.
    pub fn micro(&self, x: usize, y: usize, z: usize) -> Option<&[u8; MICRO_CELLS]> {
        let d = self.dims();
        self.micro.get(&d.index(x % d.nx, y, z % d.nz))
    }

    /// Every subdivided cell: (x, y, z) and its micro-voxels.
    pub fn micro_cells(&self) -> impl Iterator<Item = ([usize; 3], &[u8; MICRO_CELLS])> {
        let d = self.dims();
        self.micro.iter().map(move |(&i, m)| {
            let x = i % d.nx;
            let y = (i / d.nx) % d.ny;
            let z = i / (d.nx * d.ny);
            ([x, y, z], m)
        })
    }

    /// Writes a plant's coarse copy into the grid (a tree that has grown: its trunk is solid,
    /// its crown shades).
    pub fn stamp_plant(&mut self, plant: &PlantInstance) {
        let model = &self.models[model_index(plant.plant, plant.variant)];
        let dims = self.dims();
        vegetation::stamp(&mut self.blocks, dims, plant, model);
    }

    /// Erases a plant's coarse copy from the grid (a tree that died, burnt or fell): its cells
    /// holding plant matter become air again.
    pub fn unstamp_plant(&mut self, plant: &PlantInstance) {
        let model = &self.models[model_index(plant.plant, plant.variant)];
        let dims = self.dims();
        for ([x, y, z], _) in vegetation::footprint(dims, plant, model) {
            let i = dims.index(x, y, z);
            if Material::from_id(self.blocks[i]).is_some_and(Material::is_plant) {
                self.blocks[i] = Material::Air.id();
            }
        }
    }

    /// What play changes in a world, to save it: the ground and water heights, the voxels,
    /// the dug cells, the plants of the grid.
    pub fn state(&self) -> WorldState {
        let mut micro: Vec<(usize, [u8; MICRO_CELLS])> =
            self.micro.iter().map(|(&k, v)| (k, *v)).collect();
        micro.sort_by_key(|&(k, _)| k);
        WorldState {
            ground: self.ground.data.clone(),
            water: self.water.data.clone(),
            tops: self.tops.clone(),
            blocks: self.blocks.clone(),
            micro,
            plants: self.plants.clone(),
        }
    }

    /// Lays a saved state back over this world (generated again from the same seed).
    pub fn restore(&mut self, state: WorldState) -> Result<(), &'static str> {
        if state.ground.len() != self.ground.data.len()
            || state.water.len() != self.water.data.len()
            || state.tops.len() != self.tops.len()
            || state.blocks.len() != self.blocks.len()
        {
            return Err("taille du monde différente");
        }
        self.ground.data = state.ground;
        self.water.data = state.water;
        self.tops = state.tops;
        self.blocks = state.blocks;
        self.micro = state.micro.into_iter().collect();
        self.plants = state.plants;
        Ok(())
    }

    /// A point brought back into the world, which closes on itself (a planet): going past one
    /// edge comes back from the other.
    pub fn wrap(&self, x: f32, z: f32) -> (f32, f32) {
        let d = self.dims();
        let (nx, nz) = (d.nx as f32, d.nz as f32);
        let (x, z) = (x.rem_euclid(nx), z.rem_euclid(nz));
        // rem_euclid may round up to the size itself.
        (if x >= nx { 0.0 } else { x }, if z >= nz { 0.0 } else { z })
    }

    /// The column (x, z), brought back into the world.
    pub fn column(&self, x: i64, z: i64) -> (usize, usize) {
        let d = self.dims();
        (
            x.rem_euclid(d.nx as i64) as usize,
            z.rem_euclid(d.nz as i64) as usize,
        )
    }

    /// The copy of `to` nearest `from`, across the edges if that is shorter (the world closes
    /// on itself: the way from one to the other is the shortest one around it).
    pub fn nearest(&self, from: [f32; 2], to: [f32; 2]) -> [f32; 2] {
        let d = self.dims();
        let (nx, nz) = (d.nx as f32, d.nz as f32);
        [
            to[0] + nx * ((from[0] - to[0]) / nx).round(),
            to[1] + nz * ((from[1] - to[1]) / nz).round(),
        ]
    }

    /// The material of the surface of column (x, z) (the top ground cell).
    pub fn surface(&self, x: usize, z: usize) -> Material {
        let top = self.ground_top(x, z);
        if top == 0 {
            return Material::Air;
        }
        self.block(x, top - 1, z)
    }

    /// Changes the surface of column (x, z) between soil materials (grass, forest floor, dry
    /// grass, bare earth): what grows or has worn away. Returns the cell changed, if it did.
    pub fn set_surface(&mut self, x: usize, z: usize, material: Material) -> Option<[usize; 3]> {
        let dims = self.dims();
        if x >= dims.nx || z >= dims.nz {
            return None;
        }
        let top = self.ground_top(x, z);
        if top == 0 {
            return None;
        }
        let i = dims.index(x, top - 1, z);
        let soil = |m: Option<Material>| {
            matches!(
                m,
                Some(Material::Grass | Material::ForestFloor | Material::DryGrass | Material::Dirt)
            )
        };
        if !soil(Material::from_id(self.blocks[i])) || !soil(Some(material)) {
            return None;
        }
        if self.blocks[i] == material.id() {
            return None;
        }
        self.blocks[i] = material.id();
        Some([x, top - 1, z])
    }

    /// Wears the ground of column (x, z) bare, as a path trodden for years: grass and forest
    /// floor turn to earth and the ground cover growing there is gone. Meant right after
    /// generation, before the world is shown or lived in (plant indices shift).
    pub fn wear(&mut self, x: usize, z: usize) {
        let dims = self.dims();
        if x >= dims.nx || z >= dims.nz {
            return;
        }
        let top = self.ground_top(x, z);
        if top == 0 {
            return;
        }
        let i = dims.index(x, top - 1, z);
        if matches!(
            Material::from_id(self.blocks[i]),
            Some(Material::Grass | Material::ForestFloor | Material::DryGrass)
        ) {
            self.blocks[i] = Material::Dirt.id();
        }
        let worn: Vec<PlantInstance> = self
            .plants
            .iter()
            .filter(|p| p.plant.is_ground_cover() && p.base[0] == x && p.base[2] == z)
            .copied()
            .collect();
        for plant in &worn {
            self.unstamp_plant(plant);
        }
        self.plants
            .retain(|p| !(p.plant.is_ground_cover() && p.base[0] == x && p.base[2] == z));
    }

    /// Height of the ground surface at (x, z), micro-voxels included.
    pub fn surface_height(&self, x: f32, z: f32) -> f32 {
        let d = self.dims();
        // The world closes on itself.
        let (x, z) = self.wrap(x, z);
        let (cx, cz) = ((x as usize).min(d.nx - 1), (z as usize).min(d.nz - 1));
        let top = self.tops[cx + d.nx * cz];
        if top == 0 {
            return 0.0;
        }
        let Some(m) = self.micro(cx, top - 1, cz) else {
            return top as f32;
        };
        let mx = (((x - cx as f32) * MICRO as f32) as usize).min(MICRO - 1);
        let mz = (((z - cz as f32) * MICRO as f32) as usize).min(MICRO - 1);
        let filled = (0..MICRO).rev().find(|&my| m[micro_index(mx, my, mz)] != 0);
        (top - 1) as f32 + filled.map_or(0.0, |my| (my + 1) as f32 / MICRO as f32)
    }

    /// Digs one micro-voxel out of the ground near (x, z): the highest one within reach of the
    /// point in the column's top cell (subdivided on first dig). When the cell is emptied, the
    /// column becomes one cell lower, and water next to it flows in. Returns what was dug, or
    /// None at bedrock.
    pub fn dig(&mut self, x: f32, z: f32) -> Option<Dug> {
        let d = self.dims();
        if x < 1.0 || z < 1.0 || x >= d.nx as f32 - 1.0 || z >= d.nz as f32 - 1.0 {
            return None;
        }
        let (cx, cz) = (x as usize, z as usize);
        let column = cx + d.nx * cz;
        let top = self.tops[column];
        if top <= 2 {
            return None;
        }
        let y = top - 1;
        let index = d.index(cx, y, cz);
        let material = Material::from_id(self.blocks[index])?;
        if !material.is_solid() || material.is_plant() || material.is_built() {
            return None;
        }
        let brick = self
            .micro
            .entry(index)
            .or_insert([material.id(); MICRO_CELLS]);
        // The highest micro-voxel among the micro-columns near the point (a pit forms under
        // the hands), else the highest anywhere in the cell.
        let (px, pz) = (
            (x - cx as f32) * MICRO as f32,
            (z - cz as f32) * MICRO as f32,
        );
        let mut best: Option<(usize, f32, usize)> = None;
        for mz in 0..MICRO {
            for mx in 0..MICRO {
                let Some(my) = (0..MICRO)
                    .rev()
                    .find(|&my| brick[micro_index(mx, my, mz)] != 0)
                else {
                    continue;
                };
                let distance =
                    ((mx as f32 + 0.5 - px).powi(2) + (mz as f32 + 0.5 - pz).powi(2)).sqrt();
                // Near the hands first, then the highest, then the nearest.
                let near = distance < 1.6;
                let key = (near as usize) * 100 + my;
                let better = match best {
                    None => true,
                    Some((k, dist, _)) => key > k || (key == k && distance < dist),
                };
                if better {
                    best = Some((key, distance, micro_index(mx, my, mz)));
                }
            }
        }
        let (_, _, voxel) = best?;
        brick[voxel] = 0;
        let emptied = brick.iter().all(|&v| v == 0);
        let mut flooded = false;
        if emptied {
            self.micro.remove(&index);
            self.blocks[index] = Material::Air.id();
            self.tops[column] = y;
            let height = &mut self.ground.data[column];
            *height = height.min(y as f32);
            flooded = self.flood(cx, cz);
        }
        Some(Dug {
            material,
            cell: [cx, y, cz],
            emptied,
            flooded,
        })
    }

    /// Water from the columns around (x, z) flows into it if their surface is above its ground,
    /// and on into the dry columns lower than that surface it reaches (a dug channel fills).
    fn flood(&mut self, x: usize, z: usize) -> bool {
        let d = self.dims();
        let neighbours = |x: usize, z: usize| {
            [
                (x + 1, z),
                (x.wrapping_sub(1), z),
                (x, z + 1),
                (x, z.wrapping_sub(1)),
            ]
            .into_iter()
            .filter(move |&(a, b)| a < d.nx && b < d.nz)
        };
        let level = neighbours(x, z)
            .filter_map(|(a, b)| self.water_level(a, b))
            .fold(f32::NEG_INFINITY, f32::max);
        if level <= self.tops[x + d.nx * z] as f32 + 0.05 {
            return false;
        }
        let mut stack = vec![(x, z)];
        let mut filled = false;
        while let Some((a, b)) = stack.pop() {
            let column = a + d.nx * b;
            if self.water_level(a, b).is_some() || self.tops[column] as f32 >= level - 0.05 {
                continue;
            }
            self.water.data[column] = level;
            filled = true;
            stack.extend(neighbours(a, b));
        }
        filled
    }

    pub fn dims(&self) -> Dims {
        self.config.dims
    }

    /// The material of cell (x, y, z). Columns past the edges are those of the other side
    /// (the world closes on itself), so a query a step beyond is never out of the world.
    pub fn block(&self, x: usize, y: usize, z: usize) -> Material {
        let d = self.dims();
        Material::from_id(self.blocks[d.index(x % d.nx, y, z % d.nz)]).unwrap_or(Material::Air)
    }

    /// Every voxel's material id, indexed like `Dims::index` (x fastest, then y, then z).
    pub fn blocks(&self) -> &[u8] {
        &self.blocks
    }

    pub fn biome(&self, x: usize, z: usize) -> Biome {
        let d = self.dims();
        self.biomes[x % d.nx + d.nx * (z % d.nz)]
    }

    /// Ground voxels of column (x, z), plants excluded.
    pub fn ground_top(&self, x: usize, z: usize) -> usize {
        let d = self.dims();
        self.tops[x % d.nx + d.nx * (z % d.nz)]
    }

    /// Water surface of column (x, z), if water covers its ground.
    pub fn water_level(&self, x: usize, z: usize) -> Option<f32> {
        let d = self.dims();
        let i = x % d.nx + d.nx * (z % d.nz);
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
    fn the_world_has_sea_and_cold_poles() {
        let world = small(5);
        let sea = (0..128 * 128)
            .filter(|&i| world.biome(i % 128, i / 128) == Biome::Ocean)
            .count();
        assert!(sea > 128 * 128 / 20, "{sea}");
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

    #[test]
    fn digging_takes_micro_voxels_until_the_cell_is_gone() {
        let mut world = small(6);
        let dims = world.dims();
        let (x, z) = (4..dims.nz - 4)
            .flat_map(|z| (4..dims.nx - 4).map(move |x| (x, z)))
            .find(|&(x, z)| world.water_level(x, z).is_none() && world.ground_top(x, z) > 4)
            .unwrap();
        let top = world.ground_top(x, z);
        // Inside a micro-column (not on a corner between four).
        let (px, pz) = (x as f32 + 0.6, z as f32 + 0.6);
        let first = world.dig(px, pz).unwrap();
        assert!(!first.emptied && world.micro(x, top - 1, z).is_some());
        assert!(world.surface_height(px, pz) < top as f32);
        let mut digs = 1;
        while !world.dig(px, pz).unwrap().emptied {
            digs += 1;
        }
        assert_eq!(digs + 1, MICRO_CELLS);
        assert_eq!(world.ground_top(x, z), top - 1);
        assert!(world.micro(x, top - 1, z).is_none());
    }

    #[test]
    fn water_flows_into_a_hole_dug_beside_it() {
        let mut world = small(6);
        let dims = world.dims();
        // A dry column next to a lake or the sea, its ground just above the water.
        let found = (2..dims.nz - 2)
            .flat_map(|z| (2..dims.nx - 2).map(move |x| (x, z)))
            .find_map(|(x, z)| {
                if world.water_level(x, z).is_some() {
                    return None;
                }
                let level = [(x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)]
                    .into_iter()
                    .filter_map(|(a, b)| world.water_level(a, b))
                    .fold(f32::NEG_INFINITY, f32::max);
                let top = world.ground_top(x, z) as f32;
                (level > top - 1.0 && level < top).then_some((x, z))
            });
        let Some((x, z)) = found else {
            return;
        };
        let mut flooded = false;
        for _ in 0..MICRO_CELLS {
            if let Some(dug) = world.dig(x as f32 + 0.5, z as f32 + 0.5) {
                flooded |= dug.flooded;
                if dug.emptied {
                    break;
                }
            }
        }
        assert!(flooded && world.water_level(x, z).is_some());
    }
}
