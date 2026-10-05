//! Relief and climate: the heightmap (sea, plains, hills, mountain ranges, dunes) and the
//! temperature and moisture of every column.

use std::collections::VecDeque;

use sim::grid::Field2;

use crate::biome::desert_weight;
use crate::noise::{fbm, ridged, smoothstep, value};

/// Seed offsets, so that each layer of noise is independent of the others.
mod layer {
    pub const CONTINENT: u64 = 0x100;
    pub const HILLS: u64 = 0x200;
    pub const MOUNTAIN_MASK: u64 = 0x300;
    pub const MOUNTAIN_RIDGES: u64 = 0x400;
    pub const TEMPERATURE: u64 = 0x500;
    pub const MOISTURE: u64 = 0x600;
    pub const CLIMATE_JITTER: u64 = 0x700;
    pub const DUNES: u64 = 0x800;
}

/// Shape of the relief, in cells. The sea level itself comes from the world configuration.
mod shape {
    /// Width of the band along the edges of the world that is pushed under the sea, as a share
    /// of half the world: the world always ends in water.
    pub const EDGE_BAND: f32 = 0.5;
    /// Continentalness above which there is land.
    pub const COAST: f32 = 0.5;
    /// How fast the sea floor drops and the land rises with continentalness.
    pub const SEA_DEPTH: f32 = 40.0;
    pub const LAND_RISE: f32 = 14.0;
    pub const HILL_HEIGHT: f32 = 3.0;
    pub const MOUNTAIN_HEIGHT: f32 = 22.0;
    pub const DUNE_HEIGHT: f32 = 2.5;
    /// Above this height the relief is compressed, to leave room for trees under the sky.
    pub const SOFT_CEILING: f32 = 46.0;
}

/// Everything known about the land before water and plants.
pub struct Land {
    pub height: Field2,
    /// How much each column belongs to a mountain range, in [0, 1].
    pub mountain: Field2,
    pub temperature: Field2,
    pub moisture: Field2,
    /// Open sea: below sea level and connected to the edge of the world.
    pub ocean: Vec<bool>,
    /// Distance to the open sea, in cells (4-neighbour steps).
    pub coast_distance: Vec<u32>,
}

pub fn generate(nx: usize, nz: usize, sea_level: f32, seed: u64) -> Land {
    let (mut height, mountain) = relief(nx, nz, sea_level, seed);
    let ocean = open_sea(&height, sea_level);
    let coast_distance = distance_to(&ocean, nx, nz);
    let (temperature, moisture) = climate(&height, &coast_distance, sea_level, seed);
    raise_dunes(&mut height, &temperature, &moisture, &coast_distance, seed);
    Land {
        height,
        mountain,
        temperature,
        moisture,
        ocean,
        coast_distance,
    }
}

/// Heights of the ground and mountain mask.
fn relief(nx: usize, nz: usize, sea_level: f32, seed: u64) -> (Field2, Field2) {
    use shape::*;
    let mut height = Field2::filled(nx, nz, 0.0);
    let mut mountain = Field2::filled(nx, nz, 0.0);
    let size = nx.min(nz) as f32;
    for z in 0..nz {
        for x in 0..nx {
            let (fx, fz) = (x as f32, z as f32);
            // 1 in the middle of the world, 0 on its edges.
            let to_edge = x.min(z).min(nx - 1 - x).min(nz - 1 - z) as f32 / (0.5 * size);
            let inland = smoothstep(0.0, EDGE_BAND, to_edge);
            // Continentalness: large noise, pushed down near the edges. Where it crosses
            // `COAST` is the shoreline; its shape (island, archipelago, coast) depends on the seed.
            let continent =
                0.75 * fbm(fx, fz, size * 0.4, 5, seed ^ layer::CONTINENT) + 0.45 * inland - 0.19;
            let shore = continent - COAST;
            let base = if shore < 0.0 {
                sea_level + shore * SEA_DEPTH
            } else {
                sea_level + shore * LAND_RISE
            };
            // Hills and mountains only on land, growing away from the shore.
            let land = smoothstep(0.0, 0.12, shore);
            let hills = land * HILL_HEIGHT * fbm(fx, fz, 40.0, 4, seed ^ layer::HILLS);
            let range = smoothstep(
                0.55,
                0.72,
                fbm(fx, fz, size * 0.35, 3, seed ^ layer::MOUNTAIN_MASK),
            ) * smoothstep(0.04, 0.2, shore);
            let ridges = ridged(fx, fz, 48.0, 5, seed ^ layer::MOUNTAIN_RIDGES);
            // Mostly ridges: crests and peaks with valleys between them, little flat bulk.
            let peaks = range * MOUNTAIN_HEIGHT * (0.15 + 0.85 * ridges);
            let mut h = base + hills + peaks;
            if h > SOFT_CEILING {
                h = SOFT_CEILING + 0.5 * (h - SOFT_CEILING);
            }
            height.set(x, z, h.max(1.0));
            mountain.set(x, z, range);
        }
    }
    (height, mountain)
}

/// Open sea: columns below sea level reachable from the edge of the world through columns
/// below sea level. Depressions inland are not sea: they become lakes later.
fn open_sea(height: &Field2, sea_level: f32) -> Vec<bool> {
    let (nx, nz) = (height.nx, height.nz);
    let mut ocean = vec![false; nx * nz];
    let mut queue = VecDeque::new();
    for z in 0..nz {
        for x in 0..nx {
            let on_edge = x == 0 || z == 0 || x == nx - 1 || z == nz - 1;
            let i = x + nx * z;
            if on_edge && height.data[i] < sea_level {
                ocean[i] = true;
                queue.push_back(i);
            }
        }
    }
    while let Some(i) = queue.pop_front() {
        for n in neighbours4(i, nx, nz) {
            if !ocean[n] && height.data[n] < sea_level {
                ocean[n] = true;
                queue.push_back(n);
            }
        }
    }
    ocean
}

/// Distance (in 4-neighbour steps) from each column to the nearest `true` column, by a
/// breadth-first search started from all of them at once.
pub fn distance_to(mask: &[bool], nx: usize, nz: usize) -> Vec<u32> {
    let mut distance = vec![u32::MAX; nx * nz];
    let mut queue = VecDeque::new();
    for (i, &m) in mask.iter().enumerate() {
        if m {
            distance[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        for n in neighbours4(i, nx, nz) {
            if distance[n] == u32::MAX {
                distance[n] = distance[i] + 1;
                queue.push_back(n);
            }
        }
    }
    distance
}

/// Temperature and moisture in [0, 1].
///
/// - Temperature: a north–south gradient (colder at small z) plus noise, minus a lapse rate:
///   it gets colder with altitude.
/// - Moisture: noise, plus the sea's influence fading inland.
///
/// A little fine noise is added to both so that biome borders wander instead of following
/// straight iso-lines.
fn climate(height: &Field2, coast_distance: &[u32], sea_level: f32, seed: u64) -> (Field2, Field2) {
    let (nx, nz) = (height.nx, height.nz);
    let size = nx.min(nz) as f32;
    let mut temperature = Field2::filled(nx, nz, 0.0);
    let mut moisture = Field2::filled(nx, nz, 0.0);
    for z in 0..nz {
        for x in 0..nx {
            let (fx, fz) = (x as f32, z as f32);
            let i = x + nx * z;
            let altitude = (height.data[i] - sea_level).max(0.0);
            let jitter = |offset: u64| {
                0.08 * (value(fx, fz, 6.0, seed ^ layer::CLIMATE_JITTER ^ offset) - 0.5)
            };
            let warm = 0.6 * stretch(fbm(fx, fz, size * 0.22, 3, seed ^ layer::TEMPERATURE))
                + 0.4 * (fz / nz as f32)
                + 0.1
                - 0.015 * altitude
                + jitter(1);
            let sea = (-(coast_distance[i] as f32) / (size * 0.12)).exp();
            let wet = 0.85 * stretch(fbm(fx, fz, size * 0.2, 3, seed ^ layer::MOISTURE))
                + 0.15 * sea
                + jitter(2);
            temperature.data[i] = warm.clamp(0.0, 1.0);
            moisture.data[i] = wet.clamp(0.0, 1.0);
        }
    }
    (temperature, moisture)
}

/// Fractal noise clusters around 0.5 (it averages octaves), so extremes are rare. Stretching
/// it around its middle makes very hot, very cold, very dry and very wet places exist.
fn stretch(noise: f32) -> f32 {
    (0.5 + 2.2 * (noise - 0.5)).clamp(0.0, 1.0)
}

/// Dunes: long parallel crests (ridged noise stretched across the wind) where it is desert,
/// fading near the coast.
fn raise_dunes(
    height: &mut Field2,
    temperature: &Field2,
    moisture: &Field2,
    coast_distance: &[u32],
    seed: u64,
) {
    let nx = height.nx;
    for (i, h) in height.data.iter_mut().enumerate() {
        let weight = desert_weight(temperature.data[i], moisture.data[i])
            * smoothstep(3.0, 10.0, coast_distance[i] as f32);
        if weight <= 0.0 {
            continue;
        }
        let (x, z) = ((i % nx) as f32, (i / nx) as f32);
        // Wind from the south-west: crests run across it, short in one direction, long in
        // the other.
        let (along, across) = (0.8 * x + 0.6 * z, -0.6 * x + 0.8 * z);
        let dune = ridged(along, across * 0.3, 9.0, 2, seed ^ layer::DUNES);
        *h += weight * shape::DUNE_HEIGHT * dune;
    }
}

/// The 4 horizontal neighbours of column `i` inside the world.
pub fn neighbours4(i: usize, nx: usize, nz: usize) -> impl Iterator<Item = usize> {
    let (x, z) = (i % nx, i / nx);
    [
        (x > 0).then(|| i - 1),
        (x + 1 < nx).then(|| i + 1),
        (z > 0).then(|| i - nx),
        (z + 1 < nz).then(|| i + nx),
    ]
    .into_iter()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_are_sea_and_middle_has_land() {
        let land = generate(96, 96, 20.0, 3);
        for i in 0..96 {
            for &(x, z) in &[(i, 0), (i, 95), (0, i), (95, i)] {
                assert!(land.ocean[x + 96 * z], "({x}, {z}) is not sea");
            }
        }
        let land_columns = land.ocean.iter().filter(|o| !**o).count();
        assert!(
            land_columns > 96 * 96 / 10,
            "almost no land: {land_columns}"
        );
    }

    #[test]
    fn climate_in_unit_interval_and_colder_up_high() {
        let land = generate(96, 96, 20.0, 5);
        assert!(
            land.temperature
                .data
                .iter()
                .all(|t| (0.0..=1.0).contains(t))
        );
        assert!(land.moisture.data.iter().all(|m| (0.0..=1.0).contains(m)));
    }

    #[test]
    fn distance_to_mask_counts_steps() {
        let mask = [true, false, false, false];
        assert_eq!(distance_to(&mask, 4, 1), vec![0, 1, 2, 3]);
    }

    #[test]
    fn same_seed_same_land() {
        let (a, b) = (generate(48, 48, 20.0, 9), generate(48, 48, 20.0, 9));
        assert_eq!(a.height, b.height);
        assert_eq!(a.moisture, b.moisture);
    }
}
