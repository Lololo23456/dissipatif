//! The marsh: shallow, still water rich in life, within walking distance of a place (the deer's
//! meadow). Reeds stand on its edges and in its shallows; algae and duckweed float on it (the
//! game draws them on the water, see `scene.rs`). It is dug into the generated world, the way
//! the deer's circle is worn into it: right after `World::generate`, before the world is shown
//! or lived in (plant indices shift).
//!
//! Where: some way from the place, never on it, on flat Plains or Forest ground, far from any
//! river, lake or sea, where the ground lies lowest around (a hollow gathers the water).
//! What: a basin of 16 to 26 by 10 to 18 cells, rounded and ragged (its shape is drawn from the
//! seed), its bed a flat layer of mud one cell below the lowest ground around, the water 0.3
//! to 0.7 cells deep over it.
//!
//! Water is drawn where its level stands above the ground voxels, with a wall towards every
//! lower dry neighbour (see `render::water_mesher`). So the bed is one level of whole voxels
//! (`ground == tops`, the water seen over all of it), and every column around is higher than
//! the water: no wall of water stands in the air.

use crate::World;
use crate::biome::{Biome, Plant};
use crate::material::Material;
use crate::noise::{hash, hash_unit, smoothstep, value};
use crate::plants::VARIANTS;
use crate::vegetation::PlantInstance;

/// Seed offset of the marsh's place, shape and reeds.
const MARSH_SEED: u64 = 0x3a75;
/// The marsh's centre lies at least this far from the place it is near, in cells.
const NEAREST: f32 = 20.0;
/// No water this close to the place: the deer's circle (3.5 cells round) and ten cells of
/// meadow around it stay dry.
const CLEARANCE: f32 = 14.0;
/// No water this close to the other places kept dry (where the naturalist starts, where the
/// herd lies up).
const KEEP_CLEAR: f32 = 6.0;
/// Half-lengths of the basin along its two axes, in cells, before its edge is made ragged.
const LONG_AXIS: (f32, f32) = (8.0, 13.0);
const SHORT_AXIS: (f32, f32) = (5.0, 9.0);
/// Depth of the water over the bed, in cells.
const DEPTH: (f32, f32) = (0.3, 0.7);
/// Shapes tried, each smaller than the last and turned another way, until one finds a place.
const SHAPES: u32 = 6;

/// What the place of a marsh must offer, from the most fitting to the least. The near ones are
/// tried together shape by shape, the largest first (a wide marsh in the savanna rather than a
/// puddle in the meadow), then the far one: every world has some hollow where water stays.
struct Demands {
    /// Where the water may lie (its bank may be any land but a beach).
    biomes: &'static [Biome],
    /// Cells the ground under the basin and its bank may rise above its lowest column: the
    /// marsh lies on flat ground (it is dug at most this many cells and one more).
    flat: usize,
    /// Farthest the marsh's centre may be from the place, in cells.
    farthest: f32,
}

const DEMANDS: [Demands; 3] = [
    // A wet meadow or a forest pond, on level ground, a short walk away.
    Demands {
        biomes: &[Biome::Plains, Biome::Forest],
        flat: 1,
        farthest: 60.0,
    },
    // Or a pan in the savanna, a bog in the taiga, on gentler slopes.
    Demands {
        biomes: &[Biome::Plains, Biome::Forest, Biome::Savanna, Biome::Taiga],
        flat: 2,
        farthest: 60.0,
    },
    // Or the same, a longer walk away.
    Demands {
        biomes: &[Biome::Plains, Biome::Forest, Biome::Savanna, Biome::Taiga],
        flat: 2,
        farthest: 90.0,
    },
];
/// No river, lake or sea within this many cells of the basin.
const DRY_MARGIN: i64 = 4;
/// Layers of mud under the water.
const MUD_LAYERS: usize = 2;

/// A marsh dug into the world: where it is, how high its water stands, which columns it covers.
/// Not saved: `World::make_marsh` gives the same one again from the same world and seed.
#[derive(Clone, Debug, PartialEq)]
pub struct Marsh {
    /// Middle of the water (x, z), in the world.
    pub centre: [f32; 2],
    /// Height of the water surface, in cells.
    pub level: f32,
    /// First column (x, z) of the box around the water. The box may cross the edge of the world:
    /// its columns are then those of the other side.
    pub origin: [usize; 2],
    /// Columns of that box along x and z.
    pub size: [usize; 2],
    /// The columns under water (x, z), in the world, sorted (by x, then z).
    pub cells: Vec<[usize; 2]>,
}

impl Marsh {
    /// Whether column (x, z), in the world, lies under the marsh's water.
    pub fn contains(&self, x: usize, z: usize) -> bool {
        self.cells.binary_search(&[x, z]).is_ok()
    }
}

/// The outline of a basin, as offsets from the column at its centre.
struct Shape {
    /// Columns under water.
    water: Vec<[i64; 2]>,
    /// For each of them, how many steps from the bank (1 on the edge): reeds stand in the
    /// shallows near the edge, open water lies in the middle.
    edge: Vec<u32>,
    /// Dry columns touching the water (diagonals too): the bank, higher than the water.
    bank: Vec<[i64; 2]>,
    /// Depth of the water over the bed, in cells.
    depth: f32,
    /// The water's bounding box, from `min` to `max` offsets included.
    min: [i64; 2],
    max: [i64; 2],
    /// Radius of the ring around where the ground is compared with the basin's (lowness).
    surround: f32,
}

impl Shape {
    /// Shape `k` of a marsh: an ellipse of random size and heading, its edge waved by a few
    /// slow undulations so that it reads as a pond, not a geometric figure. Each next `k` is
    /// smaller and turned by the golden angle (a smaller place, or one lying another way, may
    /// be found for it).
    fn new(seed: u64, k: u32) -> Self {
        let draw = |i: i64| hash_unit(seed, &[k as i64, i]);
        let scale = 1.0 - 0.1 * k as f32;
        let along = (LONG_AXIS.0 + (LONG_AXIS.1 - LONG_AXIS.0) * draw(0)) * scale;
        let across = (SHORT_AXIS.0 + (SHORT_AXIS.1 - SHORT_AXIS.0) * draw(1)) * scale;
        let heading = std::f32::consts::TAU * hash_unit(seed, &[0, 2]) + 2.4 * k as f32;
        let phases = [3, 4, 5].map(|i| std::f32::consts::TAU * draw(i));
        let depth = DEPTH.0 + (DEPTH.1 - DEPTH.0) * draw(6);
        let (cos, sin) = (heading.cos(), heading.sin());
        let inside = |dx: i64, dz: i64| {
            let (dx, dz) = (dx as f32, dz as f32);
            let u = (dx * cos + dz * sin) / along;
            let v = (-dx * sin + dz * cos) / across;
            let angle = v.atan2(u);
            let wave = 0.12 * (2.0 * angle + phases[0]).sin()
                + 0.08 * (3.0 * angle + phases[1]).sin()
                + 0.05 * (5.0 * angle + phases[2]).sin();
            (u * u + v * v).sqrt() < 1.0 + wave
        };
        let reach = (along * 1.3).ceil() as i64 + 2;
        let mut water = Vec::new();
        let mut bank = Vec::new();
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                if inside(dx, dz) {
                    water.push([dx, dz]);
                } else if (-1..=1).any(|a| (-1..=1).any(|b| inside(dx + a, dz + b))) {
                    bank.push([dx, dz]);
                }
            }
        }
        // Steps from the bank, by rings: the edge first, then inwards.
        let mut edge = vec![0u32; water.len()];
        let index = |c: [i64; 2]| water.binary_search_by_key(&(c[1], c[0]), |w| (w[1], w[0]));
        let mut ring = 1;
        loop {
            let mut changed = false;
            for (i, &[x, z]) in water.iter().enumerate() {
                if edge[i] != 0 {
                    continue;
                }
                let next_to = [[1, 0], [-1, 0], [0, 1], [0, -1]].iter().any(|[a, b]| {
                    match index([x + a, z + b]) {
                        Ok(j) => edge[j] != 0 && edge[j] < ring,
                        Err(_) => true,
                    }
                });
                if next_to {
                    edge[i] = ring;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
            ring += 1;
        }
        let min = [0, 1].map(|a| water.iter().map(|c| c[a]).min().unwrap_or(0));
        let max = [0, 1].map(|a| water.iter().map(|c| c[a]).max().unwrap_or(0));
        Self {
            water,
            edge,
            bank,
            depth,
            min,
            max,
            surround: along + 6.0,
        }
    }
}

impl World {
    /// Digs a marsh within walking distance of `near` (the deer's circle), and returns where
    /// it is. `None` if no place in reach suits one. Meant right after generation, before the
    /// world is shown or lived in: the plants of the basin are taken away and reeds added at
    /// the end of the list, so plant indices shift. Deterministic in (world, `near`, `seed`).
    pub fn make_marsh(&mut self, near: [f32; 2], seed: u64) -> Option<Marsh> {
        self.make_marsh_away_from(near, &[], seed)
    }

    /// `make_marsh`, keeping the water away from a few more places (where the naturalist
    /// starts, where the herd lies up).
    pub fn make_marsh_away_from(
        &mut self,
        near: [f32; 2],
        away: &[[f32; 2]],
        seed: u64,
    ) -> Option<Marsh> {
        let seed = seed ^ MARSH_SEED;
        let (near_demands, far_demands) = DEMANDS.split_at(DEMANDS.len() - 1);
        let tries = (0..SHAPES)
            .flat_map(|k| near_demands.iter().map(move |d| (k, d)))
            .chain((0..SHAPES).flat_map(|k| far_demands.iter().map(move |d| (k, d))));
        let (shape, centre) = tries.into_iter().find_map(|(k, demands)| {
            let shape = Shape::new(seed, k);
            self.marsh_site(&shape, demands, near, away)
                .map(|centre| (shape, centre))
        })?;
        Some(self.dig_marsh(&shape, centre, seed))
    }

    /// The best centre column for a basin of this shape: the lowest ground among the places
    /// that suit it, then the flattest, then the nearest to 35 cells away. Searched every
    /// other column, in a fixed order (ties keep the first).
    fn marsh_site(
        &self,
        shape: &Shape,
        demands: &Demands,
        near: [f32; 2],
        away: &[[f32; 2]],
    ) -> Option<[usize; 2]> {
        let reach = demands.farthest.ceil() as i64;
        let (x0, z0) = (near[0].floor() as i64, near[1].floor() as i64);
        let mut best: Option<(f32, [usize; 2])> = None;
        for dz in (-reach..=reach).step_by(2) {
            for dx in (-reach..=reach).step_by(2) {
                let (cx, cz) = self.column(x0 + dx, z0 + dz);
                let centre = [cx as f32 + 0.5, cz as f32 + 0.5];
                let distance = self.torus_distance(near, centre);
                if !(NEAREST..=demands.farthest).contains(&distance) {
                    continue;
                }
                let Some(score) = self.marsh_score(shape, demands, [cx, cz], near, away) else {
                    continue;
                };
                let score = score - 0.01 * (distance - 35.0).abs();
                if best.is_none_or(|(s, _)| score > s) {
                    best = Some((score, [cx, cz]));
                }
            }
        }
        best.map(|(_, centre)| centre)
    }

    /// How well a basin of this shape fits at `centre`: `None` where it cannot be dug (too
    /// close to the places kept dry, not on flat dry Plains or Forest ground, water near),
    /// else how much lower its ground lies than the land around, less a little for each cell
    /// of rise.
    fn marsh_score(
        &self,
        shape: &Shape,
        demands: &Demands,
        centre: [usize; 2],
        near: [f32; 2],
        away: &[[f32; 2]],
    ) -> Option<f32> {
        let at = |o: [i64; 2]| self.column(centre[0] as i64 + o[0], centre[1] as i64 + o[1]);
        // The cheap tests first: the middle of the basin.
        let suits = |x: usize, z: usize| {
            demands.biomes.contains(&self.biome(x, z)) && self.water_level(x, z).is_none()
        };
        let (cx, cz) = at([0, 0]);
        if !suits(cx, cz) {
            return None;
        }
        let bank_suits = |x: usize, z: usize| {
            !matches!(self.biome(x, z), Biome::Ocean | Biome::Beach)
                && self.water_level(x, z).is_none()
        };
        for &o in &shape.water {
            let (x, z) = at(o);
            let cell = [x as f32 + 0.5, z as f32 + 0.5];
            if self.torus_distance(near, cell) < CLEARANCE
                || away
                    .iter()
                    .any(|&a| self.torus_distance(a, cell) < KEEP_CLEAR)
            {
                return None;
            }
        }
        let (mut low, mut high) = (usize::MAX, 0);
        let mut sum = 0.0;
        for (k, &o) in shape.water.iter().chain(&shape.bank).enumerate() {
            let (x, z) = at(o);
            if !(if k < shape.water.len() {
                suits(x, z)
            } else {
                bank_suits(x, z)
            }) {
                return None;
            }
            let top = self.ground_top(x, z);
            low = low.min(top);
            high = high.max(top);
            sum += self.ground.get(x, z);
        }
        // Room for the mud and the ground under it, and for reeds above.
        if high - low > demands.flat || low < 2 + MUD_LAYERS + 2 || high + 4 > self.dims().ny {
            return None;
        }
        // No river, lake or sea near: the marsh is still water, fed by rain and the ground.
        for dz in shape.min[1] - DRY_MARGIN..=shape.max[1] + DRY_MARGIN {
            for dx in shape.min[0] - DRY_MARGIN..=shape.max[0] + DRY_MARGIN {
                let (x, z) = at([dx, dz]);
                if self.water_level(x, z).is_some() {
                    return None;
                }
            }
        }
        // Lowness: the ground on a ring around, against the basin's.
        let basin = sum / (shape.water.len() + shape.bank.len()) as f32;
        let around = (0..16)
            .map(|k| {
                let angle = std::f32::consts::TAU * k as f32 / 16.0;
                let o = [angle.cos(), angle.sin()].map(|c| (c * shape.surround).round() as i64);
                let (x, z) = at(o);
                self.ground.get(x, z)
            })
            .sum::<f32>()
            / 16.0;
        Some(around - basin - 0.3 * (high - low) as f32)
    }

    /// Distance between two points, around the world if that is shorter.
    fn torus_distance(&self, from: [f32; 2], to: [f32; 2]) -> f32 {
        let [x, z] = self.nearest(from, to);
        ((x - from[0]).powi(2) + (z - from[1]).powi(2)).sqrt()
    }

    /// Digs the basin at `centre`: its plants gone, the ground taken down to a flat bed of mud
    /// one cell below the lowest column of the basin and its bank, the water over it, reeds on
    /// its edges and shallows.
    fn dig_marsh(&mut self, shape: &Shape, centre: [usize; 2], seed: u64) -> Marsh {
        let dims = self.dims();
        let at = |world: &World, o: [i64; 2]| {
            let (x, z) = world.column(centre[0] as i64 + o[0], centre[1] as i64 + o[1]);
            [x, z]
        };
        let lowest = shape
            .water
            .iter()
            .chain(&shape.bank)
            .map(|&o| {
                let [x, z] = at(self, o);
                self.ground_top(x, z)
            })
            .min()
            .unwrap_or(0);
        let bed = lowest - 1;
        let level = bed as f32 + shape.depth;
        let mut cells: Vec<[usize; 2]> = shape.water.iter().map(|&o| at(self, o)).collect();
        cells.sort_unstable();

        // What grew where the water now stands is gone (a tree's coarse copy with it).
        let flooded = |p: &PlantInstance| cells.binary_search(&[p.base[0], p.base[2]]).is_ok();
        let gone: Vec<PlantInstance> = self.plants.iter().filter(|p| flooded(p)).copied().collect();
        for plant in gone.iter().filter(|p| !p.plant.is_ground_cover()) {
            self.unstamp_plant(plant);
        }
        self.plants.retain(|p| !flooded(p));

        // The ground dug down to the bed, the bed of mud, the water over it.
        for &[x, z] in &cells {
            let i = x + dims.nx * z;
            for y in bed..self.tops[i] {
                self.blocks[dims.index(x, y, z)] = Material::Air.id();
            }
            for y in bed - MUD_LAYERS..bed {
                self.blocks[dims.index(x, y, z)] = Material::Mud.id();
            }
            self.tops[i] = bed;
            self.ground.data[i] = bed as f32;
            self.water.data[i] = level;
        }

        // Reeds: thick on the edge, thinning towards open water, in stands along the shore
        // with open stretches between them (where animals come down to drink); a few on the
        // wet bank. Positions relative to the centre, so nothing changes across the world's
        // edge.
        let stands = |o: [i64; 2]| {
            smoothstep(
                0.3,
                0.55,
                value(o[0] as f32, o[1] as f32, 6.0, seed ^ 0x57a4d),
            )
        };
        let edges = shape.water.iter().zip(shape.edge.iter().copied());
        let banks = shape.bank.iter().map(|o| (o, 0));
        for (&o, edge) in edges.chain(banks) {
            let chance = match edge {
                0 => 0.3,
                1 => 0.85,
                2 => 0.5,
                3 => 0.15,
                _ => 0.02,
            } * stands(o);
            let [x, z] = at(self, o);
            for k in 0..2 {
                let draw = |j: i64| hash_unit(seed, &[o[0], o[1], 10 * k + j]);
                if draw(0) >= chance {
                    continue;
                }
                let pick = hash(seed, &[o[0], o[1], 10 * k + 1]);
                self.plants.push(PlantInstance {
                    plant: Plant::Reed,
                    variant: (pick % VARIANTS as u64) as u32,
                    rotation: ((pick >> 8) % 4) as u32,
                    mirrored: (pick >> 12) & 1 == 1,
                    scale: 0.8 + 0.4 * draw(2),
                    base: [x, self.ground_top(x, z), z],
                    offset: [draw(3) - 0.5, draw(4) - 0.5].map(|d| 0.8 * d),
                });
            }
        }

        let origin = at(self, shape.min);
        Marsh {
            centre: [centre[0] as f32 + 0.5, centre[1] as f32 + 0.5],
            level,
            origin,
            size: [0, 1].map(|a| (shape.max[a] - shape.min[a] + 1) as usize),
            cells,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldConfig;
    use sim::grid::Dims;

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

    /// A dry spot of open Plains nearest the middle of the world, where a herd could live.
    fn meadow(world: &World) -> [f32; 2] {
        let d = world.dims();
        (0..d.nz as i64 / 2)
            .flat_map(|r| {
                (-r..=r).flat_map(move |a| [(a, -r), (a, r), (-r, a), (r, a)].into_iter())
            })
            .map(|(dx, dz)| world.column(d.nx as i64 / 2 + dx, d.nz as i64 / 2 + dz))
            .find(|&(x, z)| world.biome(x, z) == Biome::Plains && world.water_level(x, z).is_none())
            .map(|(x, z)| [x as f32 + 0.5, z as f32 + 0.5])
            .expect("no plains in this world")
    }

    fn with_marsh(seed: u64) -> (World, [f32; 2], Marsh) {
        let mut world = small(seed);
        let near = meadow(&world);
        let marsh = world
            .make_marsh(near, seed)
            .unwrap_or_else(|| panic!("no marsh near {near:?} in world {seed}"));
        (world, near, marsh)
    }

    #[test]
    fn every_world_has_a_marsh_within_walking_distance_of_the_meadow() {
        for seed in 1..=6 {
            let (world, near, marsh) = with_marsh(seed);
            let distance = world.torus_distance(near, marsh.centre);
            assert!(
                (NEAREST..=DEMANDS[DEMANDS.len() - 1].farthest).contains(&distance),
                "world {seed}: marsh {distance} cells from the meadow"
            );
            for &[x, z] in &marsh.cells {
                let cell = [x as f32 + 0.5, z as f32 + 0.5];
                assert!(
                    world.torus_distance(near, cell) >= CLEARANCE,
                    "world {seed}: water at {cell:?}, too close to the meadow {near:?}"
                );
            }
            assert!(
                (50..=600).contains(&marsh.cells.len()),
                "world {seed}: {} columns of water",
                marsh.cells.len()
            );
            let [x, z] = [0, 1].map(|a| marsh.centre[a] as usize);
            assert!(marsh.contains(x, z), "world {seed}: dry in its middle");
        }
    }

    #[test]
    fn the_water_lies_flat_over_a_bed_of_mud_below_its_bank() {
        for seed in 1..=6 {
            let (world, _, marsh) = with_marsh(seed);
            let bed = world.ground_top(marsh.cells[0][0], marsh.cells[0][1]);
            let depth = marsh.level - bed as f32;
            assert!(
                (DEPTH.0..=DEPTH.1).contains(&depth),
                "world {seed}: water {depth} deep"
            );
            for &[x, z] in &marsh.cells {
                assert_eq!(world.ground_top(x, z), bed, "world {seed}: uneven bed");
                // Drawn: the water is above the voxels, which are the ground's height.
                assert_eq!(world.ground.get(x, z), bed as f32);
                assert_eq!(world.water_level(x, z), Some(marsh.level));
                assert_eq!(world.surface(x, z), Material::Mud, "world {seed}: no mud");
                // The bank: every neighbour is water at the same level, or dry ground higher
                // than the water (no wall of water standing in the air).
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (a, b) = world.column(x as i64 + dx, z as i64 + dz);
                    if marsh.contains(a, b) {
                        continue;
                    }
                    assert!(
                        world.water_level(a, b).is_none()
                            && world.ground_top(a, b) as f32 > marsh.level,
                        "world {seed}: the bank at ({a}, {b}) does not hold the water"
                    );
                }
            }
        }
    }

    #[test]
    fn columns_stay_solid_up_to_their_top_around_the_marsh() {
        let (world, _, marsh) = with_marsh(2);
        for dz in -2..marsh.size[1] as i64 + 2 {
            for dx in -2..marsh.size[0] as i64 + 2 {
                let (x, z) = world.column(marsh.origin[0] as i64 + dx, marsh.origin[1] as i64 + dz);
                let top = world.ground_top(x, z);
                assert!(
                    (0..top).all(|y| world.block(x, y, z).is_solid()),
                    "hole under ({x}, {z})"
                );
                let above = world.block(x, top, z);
                assert!(
                    above == Material::Air || above.is_plant(),
                    "{above:?} above the ground at ({x}, {z})"
                );
            }
        }
    }

    #[test]
    fn reeds_grow_on_the_edges_and_nothing_else_in_the_water() {
        for seed in 1..=6 {
            let (world, _, marsh) = with_marsh(seed);
            let reeds: Vec<&PlantInstance> = world
                .plants()
                .iter()
                .filter(|p| p.plant == Plant::Reed)
                .collect();
            assert!(reeds.len() >= 20, "world {seed}: {} reeds", reeds.len());
            let in_water = reeds
                .iter()
                .filter(|p| marsh.contains(p.base[0], p.base[2]))
                .count();
            assert!(
                in_water * 2 > reeds.len(),
                "world {seed}: reeds mostly ashore"
            );
            for p in world.plants() {
                if marsh.contains(p.base[0], p.base[2]) {
                    assert_eq!(p.plant, Plant::Reed, "world {seed}: {p:?} in the water");
                    assert_eq!(p.base[1], world.ground_top(p.base[0], p.base[2]));
                }
            }
        }
    }

    #[test]
    fn same_world_same_marsh() {
        let (a, _, marsh_a) = with_marsh(4);
        let (b, _, marsh_b) = with_marsh(4);
        assert_eq!(marsh_a, marsh_b);
        assert_eq!(a.blocks(), b.blocks());
        assert_eq!(a.plants(), b.plants());
    }

    #[test]
    fn the_places_kept_clear_stay_dry() {
        // Where the marsh would be, kept clear: it goes elsewhere.
        let (_, near, first) = with_marsh(3);
        let keep = first.centre;
        let mut world = small(3);
        let marsh = world
            .make_marsh_away_from(near, &[keep], 3)
            .expect("another place for the marsh");
        assert_ne!(marsh.centre, first.centre);
        for &[x, z] in &marsh.cells {
            let cell = [x as f32 + 0.5, z as f32 + 0.5];
            assert!(
                world.torus_distance(keep, cell) >= KEEP_CLEAR,
                "water at {cell:?}, {keep:?} was to stay dry"
            );
        }
    }

    /// Where the marsh is in the worlds of the game, for a look: `cargo test -p world
    /// --release marsh_census -- --ignored --nocapture`.
    #[test]
    #[ignore = "relevé : cargo test -p world --release marsh_census -- --ignored --nocapture"]
    fn marsh_census() {
        for seed in 1..=6 {
            let mut world = World::generate(WorldConfig::standard(seed));
            let near = meadow(&world);
            let start = std::time::Instant::now();
            match world.make_marsh(near, seed) {
                Some(m) => eprintln!(
                    "graine {seed} : marais en ({:.0}, {:.0}), à {:.0} cases de ({:.0}, {:.0}), \
                     niveau {:.2}, {} cases d'eau, boîte {:?}, {:.0} ms",
                    m.centre[0],
                    m.centre[1],
                    world.torus_distance(near, m.centre),
                    near[0],
                    near[1],
                    m.level,
                    m.cells.len(),
                    m.size,
                    start.elapsed().as_secs_f64() * 1000.0
                ),
                None => eprintln!("graine {seed} : pas de marais près de {near:?}"),
            }
        }
    }
}

#[cfg(test)]
mod diagnose {
    use super::*;
    use crate::WorldConfig;
    use sim::grid::Dims;

    #[test]
    #[ignore]
    fn which_demands_fit() {
        for n in [128usize, 256, 512] {
            for seed in 1..=6u64 {
                let world = World::generate(WorldConfig {
                    dims: Dims { nx: n, ny: if n == 128 { 48 } else { 64 }, nz: n },
                    sea_level: if n == 128 { 16.0 } else { 20.0 },
                    seed,
                });
                let d = world.dims();
                let near = (0..d.nz as i64 / 2)
                    .flat_map(|r| (-r..=r).flat_map(move |a| [(a, -r), (a, r), (-r, a), (r, a)].into_iter()))
                    .map(|(dx, dz)| world.column(d.nx as i64 / 2 + dx, d.nz as i64 / 2 + dz))
                    .find(|&(x, z)| matches!(world.biome(x, z), Biome::Plains | Biome::Savanna) && world.water_level(x, z).is_none())
                    .map(|(x, z)| [x as f32 + 0.5, z as f32 + 0.5]).unwrap();
                let mut found = None;
                'outer: for k in 0..SHAPES {
                    for (t, demands) in DEMANDS.iter().enumerate().take(2) {
                        let shape = Shape::new(seed ^ MARSH_SEED, k);
                        if let Some(c) = world.marsh_site(&shape, demands, near, &[]) {
                            found = Some((t, k, c, shape.water.len(), world.biome(c[0], c[1]), world.torus_distance(near, [c[0] as f32 + 0.5, c[1] as f32 + 0.5])));
                            break 'outer;
                        }
                    }
                }
                eprintln!("n {n} seed {seed} near {near:?} {:?}: {found:?}", world.biome(near[0] as usize, near[1] as usize));
                if found.is_none() {
                    let demands = &DEMANDS[2];
                    let shape = Shape::new(seed ^ MARSH_SEED, 3);
                    let mut counts = [0usize; 8];
                    let mut biomes = std::collections::BTreeMap::new();
                    let reach = demands.farthest.ceil() as i64;
                    for dz in (-reach..=reach).step_by(2) {
                        for dx in (-reach..=reach).step_by(2) {
                            let (cx, cz) = world.column(near[0] as i64 + dx, near[1] as i64 + dz);
                            let distance = world.torus_distance(near, [cx as f32 + 0.5, cz as f32 + 0.5]);
                            if !(NEAREST..=demands.farthest).contains(&distance) { continue; }
                            counts[0] += 1;
                            *biomes.entry(format!("{:?}", world.biome(cx, cz))).or_insert(0) += 1;
                            let at = |o: [i64; 2]| world.column(cx as i64 + o[0], cz as i64 + o[1]);
                            if !demands.biomes.contains(&world.biome(cx, cz)) || world.water_level(cx, cz).is_some() { counts[1] += 1; continue; }
                            let mut bad_biome = false; let mut wet = false; let (mut lo, mut hi) = (usize::MAX, 0);
                            for &o in shape.water.iter().chain(&shape.bank) { let (x, z) = at(o);
                                bad_biome |= !demands.biomes.contains(&world.biome(x, z));
                                wet |= world.water_level(x, z).is_some();
                                let t = world.ground_top(x, z); lo = lo.min(t); hi = hi.max(t); }
                            if bad_biome { counts[3] += 1; continue; }
                            if wet { counts[4] += 1; continue; }
                            if hi - lo > demands.flat { counts[5] += 1; continue; }
                            let mut near_water = false;
                            for dz in shape.min[1] - DRY_MARGIN..=shape.max[1] + DRY_MARGIN { for dx in shape.min[0] - DRY_MARGIN..=shape.max[0] + DRY_MARGIN { let (x, z) = at([dx, dz]); near_water |= world.water_level(x, z).is_some(); } }
                            if near_water { counts[6] += 1; continue; }
                            counts[7] += 1;
                        }
                    }
                    eprintln!("    in range {}, centre bad {}, biome {}, wet {}, not flat {}, water near {}, ok {}; centres {biomes:?}", counts[0], counts[1], counts[3], counts[4], counts[5], counts[6], counts[7]);
                }
            }
        }
    }
}
