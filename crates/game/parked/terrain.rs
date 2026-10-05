//! Terrain as a heightmap, and its conversion to voxels for display.
//!
//! The ground is a continuous height per column (`Field2`), so water and later erosion can
//! change it by small amounts; voxels are only how it is drawn.
//! Deterministic: same seed, same terrain.

use sim::grid::{Dims, Field2, Field3};

/// Ground level of the plain, in cells, and how much it still rises towards the mountain.
const PLAIN_HEIGHT: f32 = 3.0;
const PLAIN_TILT: f32 = 3.0;
/// Gentle hills on the plain: amplitude and size, in cells.
const HILL_HEIGHT: f32 = 2.5;
const HILL_SCALE: f32 = 14.0;
/// Small bumps, so the rivers do not run in straight lines.
const BUMP_HEIGHT: f32 = 1.0;
const BUMP_SCALE: f32 = 5.0;
/// Size of the ridges and gullies carved into the mountain's flanks, in cells.
const RIDGE_SCALE: f32 = 11.0;
/// Share of the mountain's height made of ridges: the rest is its smooth bulk.
const RIDGE_SHARE: f32 = 0.4;
/// Height above which the ground is bare rock (no grass).
pub const ROCK_LINE: f32 = 15.0;

/// The massif the rivers come down from.
#[derive(Clone, Copy, Debug)]
pub struct Mountain {
    /// Horizontal position of the summit (x, z), in cells.
    pub centre: [f32; 2],
    /// Distance from the summit at which the mountain meets the plain.
    pub radius: f32,
    /// Height of the summit above the plain.
    pub peak: f32,
}

impl Mountain {
    /// The world's mountain: at the back (small z), centred in x, facing the plain.
    pub fn for_world(nx: usize, nz: usize) -> Self {
        Self {
            centre: [nx as f32 * 0.5, nz as f32 * 0.22],
            radius: nx.min(nz) as f32 * 0.4,
            peak: 19.0,
        }
    }

    /// Bulk of the mountain at (x, z), from 1 at the summit to 0 at its foot, with smooth
    /// shoulders (smoothstep), so the flanks steepen in the middle and flatten at the foot.
    fn mass(&self, x: f32, z: f32) -> f32 {
        let r = ((x - self.centre[0]).powi(2) + (z - self.centre[1]).powi(2)).sqrt() / self.radius;
        let t = (1.0 - r).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

/// A mountain with ridges and gullies at the back, a plain sloping gently away from it, hills
/// and bumps.
pub fn generate_heights(nx: usize, nz: usize, seed: u64) -> Field2 {
    let mountain = Mountain::for_world(nx, nz);
    let mut ground = Field2::filled(nx, nz, 0.0);
    for z in 0..nz {
        for x in 0..nx {
            let p = [x as f32 + 0.5, 0.0, z as f32 + 0.5];
            let mass = mountain.mass(p[0], p[2]);
            // Ridged noise: 1 along crest lines, lower in between. Two octaves.
            let ridged = 0.65 * ridged_noise(p, RIDGE_SCALE, seed.wrapping_add(4))
                + 0.35 * ridged_noise(p, RIDGE_SCALE / 2.0, seed.wrapping_add(5));
            let rock = mountain.peak * mass * ((1.0 - RIDGE_SHARE) + RIDGE_SHARE * ridged);
            let tilt = PLAIN_TILT * (1.0 - z as f32 / (nz - 1) as f32);
            // Hills fade out on the mountain, where the ridges take over.
            let hills = (1.0 - mass) * HILL_HEIGHT * value_noise(p, HILL_SCALE, seed);
            let bumps = BUMP_HEIGHT * value_noise(p, BUMP_SCALE, seed.wrapping_add(1));
            ground.set(x, z, PLAIN_HEIGHT + tilt + hills + bumps + rock);
        }
    }
    ground
}

/// Springs where rivers are born: on a ring around the summit, at `ring` × radius, one per
/// sector of the side facing the plain, each at the lowest point of its sector, i.e. in a gully,
/// where water would gather.
pub fn find_springs(
    ground: &Field2,
    mountain: &Mountain,
    count: usize,
    rate: f32,
) -> Vec<sim::hydrology::Spring> {
    const RING: f32 = 0.45;
    const SAMPLES_PER_SECTOR: usize = 24;
    // The side facing the plain: angles from 25° to 155°, z growing (sin > 0).
    let (first, last) = (25.0_f32.to_radians(), 155.0_f32.to_radians());
    let sector = (last - first) / count as f32;
    (0..count)
        .filter_map(|s| {
            (0..SAMPLES_PER_SECTOR)
                .filter_map(|k| {
                    let angle =
                        first + sector * (s as f32 + (k as f32 + 0.5) / SAMPLES_PER_SECTOR as f32);
                    let x = mountain.centre[0] + RING * mountain.radius * angle.cos();
                    let z = mountain.centre[1] + RING * mountain.radius * angle.sin();
                    let (x, z) = (x.floor(), z.floor());
                    (x >= 0.0 && z >= 0.0 && (x as usize) < ground.nx && (z as usize) < ground.nz)
                        .then_some((x as usize, z as usize))
                })
                .min_by(|a, b| ground.get(a.0, a.1).total_cmp(&ground.get(b.0, b.1)))
                .map(|(x, z)| sim::hydrology::Spring { x, z, rate })
        })
        .collect()
}

/// Orographic rain: air pushed up a mountain cools and its vapour condenses, so it rains more
/// higher up. `base` on the plain, up to `base × (1 + boost)` at the summit.
pub fn rain_map(ground: &Field2, mountain: &Mountain, base: f32, boost: f32, rain: &mut Field2) {
    for (r, &h) in rain.data.iter_mut().zip(&ground.data) {
        let altitude = ((h - PLAIN_HEIGHT) / mountain.peak).clamp(0.0, 1.0);
        *r = base * (1.0 + boost * altitude);
    }
}

/// Number of solid cells of a column of continuous height `height`: a cell is ground when its
/// centre is below the surface. At least 1 (no holes through the world), at most `ny − 1`.
pub fn column_cells(height: f32, ny: usize) -> usize {
    (height.round().max(1.0) as usize).min(ny - 1)
}

/// Colour values of the ground, in [0, 1] (palette `earth`: low = deep earth, mid = sand,
/// high = grass).
const SAND: f32 = 0.5;
/// How much the ground must have moved (in cells) to lose its grass: dug (bare earth) or
/// covered by deposits (sand).
const DISTURBED: f32 = 0.3;

/// The inert look of the ground, computed once: strata for every cell, grass grain per column.
pub struct GroundLook {
    strata: Field3,
    grass: Field2,
}

impl GroundLook {
    pub fn new(dims: Dims, seed: u64) -> Self {
        let mut strata = Field3::filled(dims, 0.0);
        let mut grass = Field2::filled(dims.nx, dims.nz, 0.0);
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                let column = [x as f32 + 0.5, 0.0, z as f32 + 0.5];
                grass.set(x, z, value_noise(column, 3.0, seed.wrapping_add(3)));
                for y in 0..dims.ny {
                    let p = [column[0], y as f32 + 0.5, column[2]];
                    let grain = value_noise(p, 3.0, seed.wrapping_add(2));
                    // Earth in strata, one band every 2 cells: visible on slopes and banks.
                    let band = if (y / 2) % 2 == 0 { 0.0 } else { 0.15 };
                    strata.set(x, y, z, 0.05 + 0.35 * grain + band);
                }
            }
        }
        Self { strata, grass }
    }
}

/// Voxels of the ground: `solid` (1 = ground) and `color` (inert colour value). The top cell
/// of a column is grass where the ground is as generated, bare earth where water dug it, sand
/// where it deposited, bare rock above `ROCK_LINE`. Both fields are overwritten.
pub fn voxelize_ground(
    ground: &Field2,
    initial: &Field2,
    look: &GroundLook,
    solid: &mut Field3,
    color: &mut Field3,
) {
    let dims = solid.dims;
    assert_eq!((ground.nx, ground.nz), (dims.nx, dims.nz));
    solid.data.fill(0.0);
    color.data.copy_from_slice(&look.strata.data);
    for z in 0..dims.nz {
        for x in 0..dims.nx {
            let top = column_cells(ground.get(x, z), dims.ny);
            for y in 0..top {
                solid.set(x, y, z, 1.0);
            }
            let moved = ground.get(x, z) - initial.get(x, z);
            let grain = look.grass.get(x, z);
            if moved > DISTURBED {
                color.set(x, top - 1, z, SAND + 0.05 * grain);
            } else if moved > -DISTURBED && ground.get(x, z) < ROCK_LINE {
                color.set(x, top - 1, z, 0.75 + 0.25 * grain);
            }
            // Otherwise the strata show: dug banks, and bare rock above the rock line.
        }
    }
}

/// Height of the top of the ground voxels of each column, written into `tops`.
/// Returns whether any column changed since the previous call (the ground must be remeshed).
pub fn update_tops(ground: &Field2, ny: usize, tops: &mut [usize]) -> bool {
    let mut changed = false;
    for (top, &height) in tops.iter_mut().zip(&ground.data) {
        let cells = column_cells(height, ny);
        changed |= cells != *top;
        *top = cells;
    }
    changed
}

/// Water thinner than this is not drawn as water: it only darkens the ground (`wet_ground`).
const SHOWN_DEPTH: f32 = 0.02;
/// Thinnest water layer drawn above the voxel ground, so shallow streams stay visible.
const MIN_LAYER: f32 = 0.1;

/// Where the water surface is drawn. `floor` gets the top of the ground voxels, `surface` the
/// water surface: the true level (ground + water) where it is above the voxels, which keeps
/// lakes flat, but always at least a thin layer above the voxels, so a stream on ground whose
/// voxels are rounded up stays visible. Dry columns get `surface = floor`.
pub fn water_surfaces(
    ground: &Field2,
    water: &Field2,
    ny: usize,
    floor: &mut Field2,
    surface: &mut Field2,
) {
    for i in 0..ground.data.len() {
        let top = column_cells(ground.data[i], ny) as f32;
        let depth = water.data[i];
        floor.data[i] = top;
        surface.data[i] = if depth > SHOWN_DEPTH {
            (ground.data[i] + depth).max(top + depth.min(MIN_LAYER))
        } else {
            top
        };
    }
}

/// Water depth written on the top ground cell of each column, 0 elsewhere: the shader darkens
/// and tints the ground with it (wet earth, rain film, the bed under a stream).
pub fn wet_ground(ground: &Field2, water: &Field2, wet: &mut Field3) {
    let dims = wet.dims;
    wet.data.fill(0.0);
    for z in 0..dims.nz {
        for x in 0..dims.nx {
            let top = column_cells(ground.get(x, z), dims.ny);
            wet.set(x, top - 1, z, water.get(x, z));
        }
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

/// Ridged noise in [0, 1]: smooth noise folded around its middle value, so that crest lines
/// (where the noise crosses 0.5) become sharp ridges and the rest becomes valleys between them.
fn ridged_noise(p: [f32; 3], scale: f32, seed: u64) -> f32 {
    let n = value_noise(p, scale, seed);
    1.0 - (2.0 * n - 1.0).abs()
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
        nx: 24,
        ny: 32,
        nz: 24,
    };

    #[test]
    fn same_seed_same_heights() {
        assert_eq!(generate_heights(24, 24, 3), generate_heights(24, 24, 3));
    }

    #[test]
    fn the_summit_is_on_the_mountain() {
        let (nx, nz) = (64, 64);
        let ground = generate_heights(nx, nz, 3);
        let mountain = Mountain::for_world(nx, nz);
        let highest = (0..nx * nz)
            .max_by(|&a, &b| ground.data[a].total_cmp(&ground.data[b]))
            .unwrap();
        let (x, z) = ((highest % nx) as f32, (highest / nx) as f32);
        let distance = ((x - mountain.centre[0]).powi(2) + (z - mountain.centre[1]).powi(2)).sqrt();
        assert!(
            distance < 0.5 * mountain.radius,
            "summit {distance} cells from the centre"
        );
        // Far higher than the plain at the front.
        let front = (0..nx).map(|x| ground.get(x, nz - 1)).fold(0.0, f32::max);
        assert!(ground.data[highest] > front + 10.0);
    }

    #[test]
    fn springs_sit_in_gullies_on_the_mountain() {
        let (nx, nz) = (96, 96);
        let ground = generate_heights(nx, nz, 7);
        let mountain = Mountain::for_world(nx, nz);
        let springs = find_springs(&ground, &mountain, 2, 1.0);
        assert_eq!(springs.len(), 2);
        for spring in &springs {
            let h = ground.get(spring.x, spring.z);
            assert!(h > PLAIN_HEIGHT + 5.0, "spring at {h}: not on the mountain");
            // Lower than the average of its ring neighbourhood: a gully, not a ridge.
            let around: Vec<f32> = [(-2isize, 0isize), (2, 0)]
                .iter()
                .map(|&(dx, dz)| {
                    let x = (spring.x as isize + dx).clamp(0, nx as isize - 1) as usize;
                    let z = (spring.z as isize + dz).clamp(0, nz as isize - 1) as usize;
                    ground.get(x, z)
                })
                .collect();
            assert!(around.iter().any(|&a| a > h), "spring on a crest");
        }
        assert_ne!((springs[0].x, springs[0].z), (springs[1].x, springs[1].z));
    }

    #[test]
    fn it_rains_more_on_the_mountain() {
        let (nx, nz) = (48, 48);
        let ground = generate_heights(nx, nz, 3);
        let mountain = Mountain::for_world(nx, nz);
        let mut rain = Field2::filled(nx, nz, 0.0);
        rain_map(&ground, &mountain, 1.0, 3.0, &mut rain);
        let summit = (0..nx * nz)
            .max_by(|&a, &b| ground.data[a].total_cmp(&ground.data[b]))
            .unwrap();
        assert!(rain.data[summit] > 2.5);
        assert!(rain.data.iter().all(|&r| (1.0..=4.0).contains(&r)));
    }

    #[test]
    fn ground_columns_are_full_and_coloured() {
        let ground = generate_heights(DIMS.nx, DIMS.nz, 3);
        let look = GroundLook::new(DIMS, 3);
        let mut solid = Field3::filled(DIMS, 0.0);
        let mut color = Field3::filled(DIMS, 0.0);
        voxelize_ground(&ground, &ground, &look, &mut solid, &mut color);
        for z in 0..DIMS.nz {
            for x in 0..DIMS.nx {
                let top = column_cells(ground.get(x, z), DIMS.ny);
                assert!((0..top).all(|y| solid.get(x, y, z) == 1.0));
                assert!((top..DIMS.ny).all(|y| solid.get(x, y, z) == 0.0));
                // Untouched ground: grassy top below the rock line, bare rock above.
                let grassy = color.get(x, top - 1, z) >= 0.75;
                assert_eq!(grassy, ground.get(x, z) < ROCK_LINE, "({x}, {z})");
            }
        }
        assert!(color.data.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn dug_ground_is_bare_and_deposits_are_sand() {
        let dims = Dims {
            nx: 2,
            ny: 8,
            nz: 1,
        };
        let initial = Field2::filled(2, 1, 4.0);
        let mut ground = initial.clone();
        ground.set(0, 0, 3.0); // dug by one cell
        ground.set(1, 0, 5.0); // one cell of deposits
        let look = GroundLook::new(dims, 1);
        let mut solid = Field3::filled(dims, 0.0);
        let mut color = Field3::filled(dims, 0.0);
        voxelize_ground(&ground, &initial, &look, &mut solid, &mut color);
        assert!(color.get(0, 2, 0) < 0.75, "dug top still grassy");
        assert!((color.get(1, 4, 0) - SAND).abs() < 0.06, "deposit not sand");
    }

    #[test]
    fn tops_report_changes_only_when_a_voxel_appears_or_vanishes() {
        let mut ground = Field2::filled(2, 1, 3.2);
        let mut tops = vec![0; 2];
        assert!(update_tops(&ground, 8, &mut tops));
        ground.set(0, 0, 3.4); // still 3 voxels
        assert!(!update_tops(&ground, 8, &mut tops));
        ground.set(0, 0, 3.6); // now 4
        assert!(update_tops(&ground, 8, &mut tops));
    }

    #[test]
    fn water_surface_keeps_lakes_flat_and_streams_visible() {
        let mut ground = Field2::filled(3, 1, 2.0);
        ground.set(1, 0, 2.4); // voxels round down to 2
        ground.set(2, 0, 2.6); // voxels round up to 3
        let mut water = Field2::filled(3, 1, 1.0);
        water.set(1, 0, 0.6); // lake: level 3.0 everywhere
        water.set(2, 0, 0.05); // thin stream on ground rounded up
        let (mut floor, mut surface) = (Field2::filled(3, 1, 0.0), Field2::filled(3, 1, 0.0));
        water_surfaces(&ground, &water, 8, &mut floor, &mut surface);
        assert_eq!(surface.get(0, 0), 3.0);
        assert_eq!(surface.get(1, 0), 3.0);
        assert!(
            surface.get(2, 0) > floor.get(2, 0),
            "stream hidden under its voxels"
        );
    }

    #[test]
    fn wet_ground_marks_only_the_top_cell() {
        let ground = Field2::filled(1, 1, 3.4); // 3 ground cells: top at y = 2
        let water = Field2::filled(1, 1, 0.3);
        let dims = Dims {
            nx: 1,
            ny: 6,
            nz: 1,
        };
        let mut wet = Field3::filled(dims, 1.0);
        wet_ground(&ground, &water, &mut wet);
        let column: Vec<f32> = (0..6).map(|y| wet.get(0, y, 0)).collect();
        assert_eq!(column, [0.0, 0.0, 0.3, 0.0, 0.0, 0.0]);
    }
}
