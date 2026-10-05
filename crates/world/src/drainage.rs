//! Static hydrography: lakes and rivers, as geomorphology computes them.
//!
//! 1. **Priority flood** (Barnes, Lehman, Mulla 2014). Starting from the sea, we "flood" the
//!    land in order of increasing height, like water rising from the coast. Each column is
//!    reached from a lower (or equal) neighbour: that neighbour is where its water drains.
//!    A depression is reached only once the flood overflows its rim: its columns get the rim's
//!    level, and that filled part is a **lake**.
//! 2. **Flow accumulation.** Each column receives rain (more where it is wet) and passes all
//!    its water to the column it drains to. Visiting columns from the last flooded (highest) to
//!    the first, each total is complete before it is passed on. A column's total is the rain of
//!    its whole drainage basin.
//! 3. **Rivers** are where that total is large. Their bed is carved into the ground, wider and
//!    deeper as the flow grows, and filled with water a little below the banks.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use sim::grid::Field2;

use crate::land::neighbours4;

/// Each flooded column is at least this much higher than the one it drains to, so that water
/// always has a direction, even across a filled lake.
const EPSILON: f32 = 1e-3;
/// A filled depression deeper than this is a lake.
const LAKE_DEPTH: f32 = 0.5;
/// Rain received by the driest column, plus moisture (in [0, 1]).
const BASE_RAIN: f32 = 0.15;
/// Accumulated rain above which a column carries a river.
pub const RIVER_FLOW: f32 = 300.0;
/// River banks: the water surface sits this far below the bank level.
const FREEBOARD: f32 = 0.3;

/// Water of the world, before voxels.
pub struct Drainage {
    /// Level of the water surface of each column, `f32::NEG_INFINITY` where there is none.
    pub water_level: Field2,
    /// Accumulated rain flowing through each column (0 at sea).
    pub flow: Vec<f32>,
    /// Column each column drains to (itself at the sea).
    pub receiver: Vec<usize>,
}

/// Fills lakes, finds and carves rivers. `height` is lowered along river beds.
pub fn drain(height: &mut Field2, ocean: &[bool], moisture: &Field2, sea_level: f32) -> Drainage {
    let (nx, nz) = (height.nx, height.nz);
    let n = nx * nz;
    let (filled, receiver, order) = priority_flood(height, ocean);

    // Flow accumulation, from the highest flooded columns down to the sea.
    let mut flow: Vec<f32> = (0..n)
        .map(|i| {
            if ocean[i] {
                0.0
            } else {
                BASE_RAIN + moisture.data[i]
            }
        })
        .collect();
    for &i in order.iter().rev() {
        let r = receiver[i];
        if r != i {
            flow[r] += flow[i];
        }
    }

    let mut water_level = Field2::filled(nx, nz, f32::NEG_INFINITY);
    for i in 0..n {
        if ocean[i] {
            water_level.data[i] = sea_level;
        } else if filled[i] - height.data[i] > LAKE_DEPTH {
            water_level.data[i] = filled[i];
        }
    }
    carve_rivers(height, &mut water_level, &filled, &flow, ocean, &order);
    Drainage {
        water_level,
        flow,
        receiver,
    }
}

/// Returns the filled heights, the column each column drains to, and the flooding order.
fn priority_flood(height: &Field2, ocean: &[bool]) -> (Vec<f32>, Vec<usize>, Vec<usize>) {
    let (nx, nz) = (height.nx, height.nz);
    let n = nx * nz;
    let mut filled = height.data.clone();
    let mut receiver: Vec<usize> = (0..n).collect();
    let mut done = vec![false; n];
    let mut order = Vec::with_capacity(n);
    // Min-heap on (level, insertion count): the count breaks ties deterministically.
    // Heights are ≥ 0, so the bits of an f32 sort like the f32 itself.
    let mut heap = BinaryHeap::new();
    let mut count = 0u64;
    for i in 0..n {
        let (x, z) = (i % nx, i / nx);
        let on_edge = x == 0 || z == 0 || x == nx - 1 || z == nz - 1;
        if ocean[i] || on_edge {
            done[i] = true;
            heap.push(Reverse((filled[i].max(0.0).to_bits(), count, i)));
            count += 1;
        }
    }
    while let Some(Reverse((_, _, i))) = heap.pop() {
        order.push(i);
        for j in neighbours4(i, nx, nz) {
            if done[j] {
                continue;
            }
            done[j] = true;
            filled[j] = filled[j].max(filled[i] + EPSILON);
            receiver[j] = i;
            heap.push(Reverse((filled[j].max(0.0).to_bits(), count, j)));
            count += 1;
        }
    }
    (filled, receiver, order)
}

/// Carves a bed under every river column: a bowl of radius and depth growing with the flow,
/// filled with water up to a little below the banks.
fn carve_rivers(
    height: &mut Field2,
    water_level: &mut Field2,
    filled: &[f32],
    flow: &[f32],
    ocean: &[bool],
    order: &[usize],
) {
    let (nx, nz) = (height.nx, height.nz);
    let original = height.data.clone();
    for &i in order {
        if ocean[i] || flow[i] < RIVER_FLOW {
            continue;
        }
        let strength = flow[i] / RIVER_FLOW;
        let depth = (0.8 + 0.4 * strength.ln()).clamp(0.8, 2.5);
        let radius = (0.6 + 0.35 * strength.sqrt()).clamp(0.6, 3.0);
        // The river runs at the level of its column (a lake's level inside a lake).
        let bank = filled[i];
        let surface = bank - FREEBOARD;
        let (x, z) = ((i % nx) as isize, (i / nx) as isize);
        let reach = radius.ceil() as isize;
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let (cx, cz) = (x + dx, z + dz);
                if cx < 0 || cz < 0 || cx >= nx as isize || cz >= nz as isize {
                    continue;
                }
                let d = ((dx * dx + dz * dz) as f32).sqrt();
                if d > radius {
                    continue;
                }
                let c = cx as usize + nx * cz as usize;
                // Bowl-shaped bed: deepest on the axis, rising to the banks.
                let bed = bank - depth * (1.0 - (d / radius).powi(2));
                if bed < height.data[c] && bed < original[c] {
                    height.data[c] = bed;
                    if surface > bed {
                        water_level.data[c] = water_level.data[c].max(surface.min(original[c]));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A slope down to the sea (z = 0), with a pit in the middle.
    fn slope_with_pit() -> (Field2, Vec<bool>, Field2) {
        let (nx, nz) = (12, 20);
        let mut height = Field2::filled(nx, nz, 0.0);
        let mut ocean = vec![false; nx * nz];
        for z in 0..nz {
            for x in 0..nx {
                height.set(x, z, 0.5 * z as f32);
                ocean[x + nx * z] = z == 0;
            }
        }
        // A 3×3 pit, 3 cells deep, around (6, 10).
        for z in 9..12 {
            for x in 5..8 {
                height.set(x, z, height.get(x, z) - 3.0);
            }
        }
        (height, ocean, Field2::filled(nx, nz, 0.5))
    }

    #[test]
    fn every_column_drains_to_the_sea_downhill() {
        let (mut height, ocean, moisture) = slope_with_pit();
        let drainage = drain(&mut height, &ocean, &moisture, 0.0);
        for start in 0..height.data.len() {
            let (mut i, mut steps) = (start, 0);
            while drainage.receiver[i] != i {
                i = drainage.receiver[i];
                steps += 1;
                assert!(steps < 1000, "cycle from {start}");
            }
            let edge = |i: usize| i.is_multiple_of(12) || i % 12 == 11 || i / 12 == 19;
            assert!(ocean[i] || edge(i), "{start} ends at {i}, not at the sea");
        }
    }

    #[test]
    fn pit_becomes_a_flat_lake() {
        let (mut height, ocean, moisture) = slope_with_pit();
        let drainage = drain(&mut height, &ocean, &moisture, 0.0);
        let level = drainage.water_level.get(6, 10);
        assert!(level > height.get(6, 10) + 1.0, "no lake in the pit");
        assert!(
            (drainage.water_level.get(5, 9) - level).abs() < 0.05,
            "lake not flat"
        );
    }

    #[test]
    fn flow_grows_downstream() {
        let (mut height, ocean, moisture) = slope_with_pit();
        let drainage = drain(&mut height, &ocean, &moisture, 0.0);
        for i in 0..height.data.len() {
            let r = drainage.receiver[i];
            if r != i {
                assert!(drainage.flow[r] >= drainage.flow[i]);
            }
        }
    }

    #[test]
    fn big_rivers_are_carved_and_filled() {
        // A long, wide valley draining a lot of rain to the sea at z = 0.
        let (nx, nz) = (41, 80);
        let mut height = Field2::filled(nx, nz, 0.0);
        let mut ocean = vec![false; nx * nz];
        for z in 0..nz {
            for x in 0..nx {
                let valley = (x as f32 - 20.0).abs() * 0.3;
                height.set(x, z, 1.0 + 0.2 * z as f32 + valley);
                ocean[x + nx * z] = z == 0;
            }
        }
        let before = height.clone();
        let moisture = Field2::filled(nx, nz, 1.0);
        let drainage = drain(&mut height, &ocean, &moisture, 0.5);
        let carved = (0..nx * nz)
            .filter(|&i| height.data[i] < before.data[i] - 0.5)
            .count();
        assert!(carved > 20, "only {carved} columns carved");
        // Most carved columns hold water (the others are banks cut into the valley sides,
        // above the water), and water never rises above the original ground.
        let river: Vec<usize> = (0..nx * nz)
            .filter(|&i| !ocean[i] && height.data[i] < before.data[i] - 0.5)
            .collect();
        let wet = river
            .iter()
            .filter(|&&i| drainage.water_level.data[i] > height.data[i])
            .count();
        assert!(
            wet * 10 >= river.len() * 6,
            "{wet} wet of {} carved",
            river.len()
        );
        for &i in &river {
            assert!(
                drainage.water_level.data[i] <= before.data[i],
                "water above the banks at {i}"
            );
        }
    }
}
