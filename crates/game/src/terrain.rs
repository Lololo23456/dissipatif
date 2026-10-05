//! Terrain: a heightmap of voxel columns. Its shape never changes; what lives on its surface is
//! simulated separately and shown as the colour of the top cells.
//! Deterministic: same seed, same terrain.

use sim::grid::{Dims, Field3};

pub struct Terrain {
    /// 1 where there is ground, 0 elsewhere: meshed with a 0.5 threshold.
    pub solid: Field3,
    /// Inert colour of the ground, in [0, 1]: low = deep earth, high = grassy surface.
    pub color: Field3,
    /// Height of each column (number of solid cells), indexed `x + nx * z`.
    /// The surface cell of column (x, z) is at y = height − 1.
    pub heights: Vec<usize>,
}

impl Terrain {
    /// The cell at the top of column (x, z), where surface life shows.
    pub fn surface(&self, x: usize, z: usize) -> [usize; 3] {
        let height = self.heights[x + self.solid.dims.nx * z];
        [x, height - 1, z]
    }
}

/// Ground level under the hills, in cells.
const BASE_HEIGHT: f32 = 6.0;
/// Largest height of the hills above the ground level, in cells.
const HILL_HEIGHT: f32 = 5.0;
/// Size of the hills, in cells.
const HILL_SCALE: f32 = 16.0;

pub fn generate(dims: Dims, seed: u64) -> Terrain {
    assert!(
        ((BASE_HEIGHT + HILL_HEIGHT) as usize) < dims.ny,
        "world {dims:?} too low for the hills"
    );
    let mut solid = Field3::filled(dims, 0.0);
    let mut color = Field3::filled(dims, 0.0);
    let mut heights = vec![0; dims.nx * dims.nz];
    for z in 0..dims.nz {
        for x in 0..dims.nx {
            let p = [x as f32 + 0.5, 0.0, z as f32 + 0.5];
            let height = (BASE_HEIGHT + HILL_HEIGHT * value_noise(p, HILL_SCALE, seed)).round();
            let height = height as usize;
            heights[x + dims.nx * z] = height;
            for y in 0..height {
                solid.set(x, y, z, 1.0);
                let q = [p[0], y as f32 + 0.5, p[2]];
                let grain = value_noise(q, 3.0, seed.wrapping_add(1));
                let value = if y + 1 == height {
                    // Grassy top: olive to moss.
                    0.75 + 0.25 * grain
                } else {
                    // Earth in strata, one band every 2 cells: visible on the slopes.
                    let band = if (y / 2) % 2 == 0 { 0.0 } else { 0.15 };
                    0.05 + 0.35 * grain + band
                };
                color.set(x, y, z, value);
            }
        }
    }
    Terrain {
        solid,
        color,
        heights,
    }
}

/// Smooth noise in [0, 1]: random values on a lattice of spacing `scale`, interpolated between
/// lattice points with smoothstep weights (no visible creases along lattice planes).
fn value_noise(p: [f32; 3], scale: f32, seed: u64) -> f32 {
    let q = p.map(|c| c / scale);
    let cell = q.map(|c| c.floor() as i64);
    let t = std::array::from_fn::<f32, 3, _>(|i| {
        let f = q[i] - cell[i] as f32;
        f * f * (3.0 - 2.0 * f)
    });
    let mut sum = 0.0;
    for corner in 0..8 {
        let offset = [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1];
        let weight: f32 = (0..3)
            .map(|i| if offset[i] == 1 { t[i] } else { 1.0 - t[i] })
            .product();
        let lattice = std::array::from_fn(|i| cell[i] + offset[i] as i64);
        sum += weight * lattice_value(lattice, seed);
    }
    sum
}

/// Pseudo-random value in [0, 1) attached to a lattice point: a hash of (seed, point), so it
/// does not depend on the order in which points are visited.
fn lattice_value(point: [i64; 3], seed: u64) -> f32 {
    let mut h = seed;
    for c in point {
        h = splitmix64(h ^ c as u64);
    }
    // Keep the 24 high bits: exactly representable in an f32.
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// SplitMix64 mixing step: every output bit depends on every input bit.
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIMS: Dims = Dims {
        nx: 32,
        ny: 16,
        nz: 32,
    };

    #[test]
    fn same_seed_same_terrain() {
        let a = generate(DIMS, 3);
        let b = generate(DIMS, 3);
        assert_eq!(a.solid.data, b.solid.data);
        assert_eq!(a.color.data, b.color.data);
    }

    #[test]
    fn surface_is_the_top_of_each_column() {
        let terrain = generate(DIMS, 3);
        for z in 0..DIMS.nz {
            for x in 0..DIMS.nx {
                let [_, top, _] = terrain.surface(x, z);
                assert_eq!(terrain.solid.get(x, top, z), 1.0, "surface not solid");
                assert_eq!(terrain.solid.get(x, top + 1, z), 0.0, "solid above surface");
                // Columns are full down to the bottom: no floating ground.
                assert!((0..top).all(|y| terrain.solid.get(x, y, z) == 1.0));
            }
        }
    }

    #[test]
    fn colours_in_unit_interval() {
        let terrain = generate(DIMS, 3);
        assert!(terrain.color.data.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
