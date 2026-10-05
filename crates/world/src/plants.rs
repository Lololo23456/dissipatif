//! Plant models in micro-voxels: each world cell is split into `MICRO`³ micro-cells, so trunks
//! can be thin, branches can reach out and foliage can be ragged.
//!
//! A few variants of each plant are generated per world, and every plant of the world is one
//! of them, turned by a quarter turn or not: the renderer draws them by instancing.

use sim::grid::Dims;

use crate::biome::Plant;
use crate::material::Material;
use crate::noise::{hash, hash_unit, value3};

/// Micro-cells per world cell, along each axis.
pub const MICRO: usize = 4;
/// Variants generated for each kind of plant.
pub const VARIANTS: u32 = 16;

/// A plant shape in micro-voxels.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub dims: Dims,
    /// Material id of every micro-voxel, indexed like `Dims::index`.
    pub voxels: Vec<u8>,
    /// Micro-coordinates of the point the plant stands on: the centre of the bottom face of its
    /// base world cell. It lies on a corner between micro-cells.
    pub anchor: [usize; 3],
}

impl Model {
    pub fn get(&self, x: usize, y: usize, z: usize) -> Material {
        Material::from_id(self.voxels[self.dims.index(x, y, z)]).unwrap_or(Material::Air)
    }

    pub fn solid_count(&self) -> usize {
        self.voxels
            .iter()
            .filter(|&&v| v != Material::Air.id())
            .count()
    }
}

/// Builds variant `variant` of `plant`. Deterministic in (plant, variant, seed).
pub fn model(plant: Plant, variant: u32, seed: u64) -> Model {
    let seed = hash(seed, &[plant as i64, variant as i64, 0x91a]);
    let mut b = match plant {
        Plant::Broadleaf => Builder::new(52, 60, seed),
        Plant::Pine => Builder::new(44, 68, seed),
        Plant::Acacia => Builder::new(64, 40, seed),
        Plant::Cactus => Builder::new(24, 32, seed),
        Plant::Bush => Builder::new(24, 14, seed),
        Plant::Birch => Builder::new(36, 60, seed),
        Plant::Willow => Builder::new(64, 44, seed),
        Plant::Palm => Builder::new(72, 64, seed),
        Plant::DeadTree => Builder::new(44, 44, seed),
        Plant::Grass => Builder::new(12, 10, seed),
        Plant::Flower => Builder::new(10, 10, seed),
        Plant::Fern => Builder::new(26, 10, seed),
        Plant::Mushroom => Builder::new(14, 10, seed),
        Plant::Stone => Builder::new(16, 8, seed),
        Plant::DryShrub => Builder::new(24, 14, seed),
    };
    match plant {
        Plant::Broadleaf => broadleaf(&mut b),
        Plant::Pine => pine(&mut b),
        Plant::Acacia => acacia(&mut b),
        Plant::Cactus => cactus(&mut b),
        Plant::Bush => bush(&mut b),
        Plant::Birch => birch(&mut b),
        Plant::Willow => willow(&mut b),
        Plant::Palm => palm(&mut b),
        Plant::DeadTree => dead_tree(&mut b),
        Plant::Grass => grass(&mut b),
        Plant::Flower => flower(&mut b, variant),
        Plant::Fern => fern(&mut b),
        Plant::Mushroom => mushrooms(&mut b),
        Plant::Stone => stones(&mut b, variant),
        Plant::DryShrub => dry_shrub(&mut b),
    }
    b.crop()
}

/// Scratch space where a plant is drawn, centred on its anchor, then cropped to what is filled.
struct Builder {
    dims: Dims,
    voxels: Vec<u8>,
    /// Anchor: centre of the box horizontally, at the bottom.
    a: [f32; 3],
    seed: u64,
    draws: u64,
}

impl Builder {
    fn new(width: usize, height: usize, seed: u64) -> Self {
        let dims = Dims {
            nx: width,
            ny: height,
            nz: width,
        };
        Self {
            dims,
            voxels: vec![Material::Air.id(); dims.len()],
            a: [(width / 2) as f32, 0.0, (width / 2) as f32],
            seed,
            draws: 0,
        }
    }

    /// Next random number in [0, 1).
    fn rand(&mut self) -> f32 {
        self.draws += 1;
        hash_unit(self.seed, &[self.draws as i64])
    }

    /// Fills the micro-cell containing the point (x, y, z), given relative to the anchor, if it
    /// is empty and inside the box. Wood is drawn first, so foliage never hides a branch.
    fn set(&mut self, x: f32, y: f32, z: f32, material: Material) {
        let p = [x + self.a[0], y + self.a[1], z + self.a[2]].map(f32::floor);
        let d = self.dims;
        if p[0] < 0.0 || p[1] < 0.0 || p[2] < 0.0 {
            return;
        }
        let (px, py, pz) = (p[0] as usize, p[1] as usize, p[2] as usize);
        if px >= d.nx || py >= d.ny || pz >= d.nz {
            return;
        }
        let i = d.index(px, py, pz);
        if self.voxels[i] == Material::Air.id() {
            self.voxels[i] = material.id();
        }
    }

    /// A vertical post of `width × width` micro-cells centred on (x, z), from y0 to y1.
    fn post(&mut self, x: f32, z: f32, y0: f32, y1: f32, width: f32, material: Material) {
        let half = width / 2.0;
        let mut y = y0;
        while y < y1 {
            let mut dz = -half;
            while dz < half {
                let mut dx = -half;
                while dx < half {
                    self.set(x + dx + 0.5, y + 0.5, z + dz + 0.5, material);
                    dx += 1.0;
                }
                dz += 1.0;
            }
            y += 1.0;
        }
    }

    /// A line of micro-cells from `from` to `to`, `width` cells thick.
    fn branch(&mut self, from: [f32; 3], to: [f32; 3], width: f32, material: Material) {
        let length = (0..3)
            .map(|i| (to[i] - from[i]).powi(2))
            .sum::<f32>()
            .sqrt();
        let steps = (length * 2.0).ceil().max(1.0) as usize;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let p: [f32; 3] = std::array::from_fn(|i| from[i] + t * (to[i] - from[i]));
            self.post(p[0], p[2], p[1], p[1] + 1.0, width, material);
        }
    }

    /// A ragged ellipsoid of foliage: radius `r` across, `ry` up, its surface eaten by 3D noise
    /// so it reads as clumps of leaves, not as a ball.
    fn blob(&mut self, centre: [f32; 3], r: f32, ry: f32, material: Material) {
        let reach = r.ceil() as i32;
        let reach_y = ry.ceil() as i32;
        let noise_seed = self.seed ^ 0xb10b;
        for dy in -reach_y..=reach_y {
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let (x, y, z) = (
                        centre[0] + dx as f32,
                        centre[1] + dy as f32,
                        centre[2] + dz as f32,
                    );
                    let d = ((dx * dx + dz * dz) as f32 / (r * r) + (dy * dy) as f32 / (ry * ry))
                        .sqrt();
                    let n = value3(x, y, z, 1.8, noise_seed);
                    if d < 1.0 - 0.45 * n {
                        self.set(x, y, z, material);
                    }
                }
            }
        }
    }

    /// Keeps only the box around filled micro-cells.
    fn crop(self) -> Model {
        let d = self.dims;
        let (mut lo, mut hi) = ([usize::MAX; 3], [0usize; 3]);
        for z in 0..d.nz {
            for y in 0..d.ny {
                for x in 0..d.nx {
                    if self.voxels[d.index(x, y, z)] != Material::Air.id() {
                        for (k, c) in [x, y, z].into_iter().enumerate() {
                            lo[k] = lo[k].min(c);
                            hi[k] = hi[k].max(c);
                        }
                    }
                }
            }
        }
        // The anchor must stay inside the box (the plant stands on it).
        let anchor = self.a.map(|v| v as usize);
        for k in 0..3 {
            lo[k] = lo[k].min(anchor[k]);
            hi[k] = hi[k].max(anchor[k]);
        }
        let dims = Dims {
            nx: hi[0] - lo[0] + 1,
            ny: hi[1] - lo[1] + 1,
            nz: hi[2] - lo[2] + 1,
        };
        let mut voxels = vec![Material::Air.id(); dims.len()];
        for z in 0..dims.nz {
            for y in 0..dims.ny {
                for x in 0..dims.nx {
                    voxels[dims.index(x, y, z)] =
                        self.voxels[d.index(x + lo[0], y + lo[1], z + lo[2])];
                }
            }
        }
        Model {
            dims,
            voxels,
            anchor: [anchor[0] - lo[0], anchor[1] - lo[1], anchor[2] - lo[2]],
        }
    }
}

/// Round deciduous tree: thin trunk flared at the roots, a few branches, a crown of clumps.
fn broadleaf(b: &mut Builder) {
    let height = 16.0 + (b.rand() * 8.0).floor();
    b.post(0.0, 0.0, 2.0, height, 2.0, Material::Wood);
    b.post(0.0, 0.0, 0.0, 2.0, 4.0, Material::Wood);
    let mut ends = vec![[0.0, height + 3.0, 0.0]];
    let branches = 2 + (b.rand() * 2.0) as usize;
    for k in 0..branches {
        let angle = std::f32::consts::TAU * (k as f32 + b.rand() * 0.6) / branches as f32;
        let start = height * (0.55 + 0.3 * b.rand());
        let length = 6.0 + 4.0 * b.rand();
        let end = [
            angle.cos() * length,
            start + 0.7 * length,
            angle.sin() * length,
        ];
        b.branch([0.0, start, 0.0], end, 1.0, Material::Wood);
        ends.push(end);
    }
    // Crown: many small clumps scattered around the top and the branch ends, rather than a
    // few big balls, so that the outline is irregular and light passes between clumps.
    let crown = ends[0];
    let clumps = 6 + (b.rand() * 4.0) as usize;
    for _ in 0..clumps {
        let angle = std::f32::consts::TAU * b.rand();
        let out = 6.0 * b.rand().sqrt();
        let centre = [
            crown[0] + angle.cos() * out,
            crown[1] - 2.0 + 5.0 * b.rand(),
            crown[2] + angle.sin() * out,
        ];
        let r = 4.0 + 2.5 * b.rand();
        b.blob(centre, r, r * 0.7, Material::Leaves);
    }
    for &end in &ends[1..] {
        let r = 3.5 + 2.0 * b.rand();
        b.blob([end[0], end[1] + 1.0, end[2]], r, r * 0.7, Material::Leaves);
    }
}

/// Conifer: a straight thin trunk and stacked cones of needles narrowing to a spike.
fn pine(b: &mut Builder) {
    let height = 34.0 + (b.rand() * 10.0).floor();
    b.post(0.0, 0.0, 0.0, height, 2.0, Material::Wood);
    let first = 6.0 + (b.rand() * 4.0).floor();
    let tiers = 5;
    let span = (height - first) / tiers as f32;
    let noise_seed = b.seed ^ 0x9e1;
    for tier in 0..tiers {
        let bottom = first + tier as f32 * span;
        let radius = 12.0 - 1.9 * tier as f32 + 1.5 * b.rand();
        let tier_height = span + 3.0;
        let mut y = 0.0;
        while y < tier_height {
            // Each tier is a cone: wide at its bottom, narrow at its top.
            let r = radius * (1.0 - y / tier_height).powf(0.9) + 1.0;
            let reach = r.ceil() as i32;
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let d = ((dx * dx + dz * dz) as f32).sqrt();
                    let n = value3(dx as f32, bottom + y, dz as f32, 2.0, noise_seed);
                    if d < r - 1.2 * n {
                        b.set(
                            dx as f32 + 0.5,
                            bottom + y + 0.5,
                            dz as f32 + 0.5,
                            Material::PineNeedles,
                        );
                    }
                }
            }
            y += 1.0;
        }
    }
    // Spike above the trunk.
    b.post(0.0, 0.0, height, height + 5.0, 2.0, Material::PineNeedles);
}

/// Savanna tree: a leaning trunk forking into branches, each holding a flat umbrella of leaves.
fn acacia(b: &mut Builder) {
    let height = 16.0 + (b.rand() * 5.0).floor();
    let lean_angle = std::f32::consts::TAU * b.rand();
    let lean = [lean_angle.cos(), lean_angle.sin()];
    // Trunk bending further as it rises.
    let mut y = 0.0;
    while y < height {
        let bend = 6.0 * (y / height).powi(2);
        b.post(
            lean[0] * bend,
            lean[1] * bend,
            y,
            y + 1.0,
            2.0,
            Material::Wood,
        );
        y += 1.0;
    }
    let top = [lean[0] * 6.0, height, lean[1] * 6.0];
    let forks = 2 + (b.rand() * 2.0) as usize;
    for k in 0..forks {
        let angle = std::f32::consts::TAU * (k as f32 + b.rand() * 0.5) / forks as f32;
        let length = 6.0 + 4.0 * b.rand();
        let end = [
            top[0] + angle.cos() * length,
            height + 4.0 + 3.0 * b.rand(),
            top[2] + angle.sin() * length,
        ];
        b.branch(top, end, 1.0, Material::Wood);
        // Flat crown: a thin disc, thinner towards its edge.
        let r = 9.0 + 3.0 * b.rand();
        let reach = r.ceil() as i32;
        let noise_seed = b.seed ^ (0xac + k as u64);
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let d = ((dx * dx + dz * dz) as f32).sqrt() / r;
                let n = value3(
                    end[0] + dx as f32,
                    end[1],
                    end[2] + dz as f32,
                    3.0,
                    noise_seed,
                );
                if d < 1.0 - 0.3 * n {
                    let thickness = (3.0 * (1.0 - d * d)).ceil().max(1.0);
                    let mut t = 0.0;
                    while t < thickness {
                        b.set(
                            end[0] + dx as f32 + 0.5,
                            end[1] + t,
                            end[2] + dz as f32 + 0.5,
                            Material::Leaves,
                        );
                        t += 1.0;
                    }
                }
            }
        }
    }
}

/// Cactus: a rounded column, with arms going out then up.
fn cactus(b: &mut Builder) {
    let height = 10.0 + (b.rand() * 8.0).floor();
    // Rounded 4×4 section: the four corner columns are left out.
    let column = |b: &mut Builder, x: f32, z: f32, y0: f32, y1: f32| {
        b.post(x, z, y0, y1, 2.0, Material::Cactus);
        for (dx, dz) in [(-1.5, -0.5), (-1.5, 0.5), (1.5, -0.5), (1.5, 0.5)] {
            b.post(x + dx, z + dz, y0, y1, 1.0, Material::Cactus);
            b.post(x + dz, z + dx, y0, y1, 1.0, Material::Cactus);
        }
    };
    column(b, 0.0, 0.0, 0.0, height);
    let arms = (b.rand() * 3.0) as usize;
    for k in 0..arms {
        let side = [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]]
            [(k * 2 + (b.rand() * 2.0) as usize) % 4];
        let at = (height * (0.35 + 0.25 * b.rand())).floor();
        let reach = 4.0;
        // Out…
        let mut s = 1.0;
        while s <= reach {
            b.post(
                side[0] * (1.5 + s),
                side[1] * (1.5 + s),
                at,
                at + 2.0,
                2.0,
                Material::Cactus,
            );
            s += 1.0;
        }
        // …then up.
        let up = 4.0 + (b.rand() * 5.0).floor();
        b.post(
            side[0] * (1.5 + reach),
            side[1] * (1.5 + reach),
            at,
            at + up,
            2.0,
            Material::Cactus,
        );
    }
}

/// Birch: a slender white trunk speckled with dark marks, a light crown of tall clumps.
fn birch(b: &mut Builder) {
    let height = 26.0 + (b.rand() * 8.0).floor();
    let mut y = 0.0;
    while y < height {
        // Bark: white, with the dark marks birches are known for.
        let material = if b.rand() < 0.18 {
            Material::DeadWood
        } else {
            Material::BirchBark
        };
        b.post(0.0, 0.0, y, y + 1.0, 2.0, material);
        y += 1.0;
    }
    let clumps = 4 + (b.rand() * 3.0) as usize;
    for _ in 0..clumps {
        let angle = std::f32::consts::TAU * b.rand();
        let out = 3.0 * b.rand();
        let at = height * (0.6 + 0.45 * b.rand());
        let centre = [angle.cos() * out, at, angle.sin() * out];
        b.branch([0.0, at - 3.0, 0.0], centre, 1.0, Material::BirchBark);
        let r = 3.5 + 1.5 * b.rand();
        b.blob(centre, r, r * 1.3, Material::BirchLeaves);
    }
}

/// Willow: a stout trunk, a dome of foliage and curtains of leaves hanging down to the ground.
fn willow(b: &mut Builder) {
    let height = 14.0 + (b.rand() * 4.0).floor();
    b.post(0.0, 0.0, 0.0, height, 3.0, Material::Wood);
    let r = 10.0 + 2.0 * b.rand();
    b.blob(
        [0.0, height + 2.0, 0.0],
        r,
        r * 0.45,
        Material::WillowLeaves,
    );
    // Hanging strands around the rim of the dome.
    let strands = 60 + (b.rand() * 30.0) as usize;
    for _ in 0..strands {
        let angle = std::f32::consts::TAU * b.rand();
        let out = r * (0.55 + 0.45 * b.rand());
        let (x, z) = (angle.cos() * out, angle.sin() * out);
        let top = height + 1.0;
        let length = 6.0 + (height - 3.0) * b.rand();
        let mut y = top;
        while y > top - length {
            b.set(x, y, z, Material::WillowLeaves);
            y -= 1.0;
        }
    }
}

/// Palm: a curved ringed trunk and a crown of long fronds arching down, a few coconuts.
fn palm(b: &mut Builder) {
    let height = 30.0 + (b.rand() * 10.0).floor();
    let lean_angle = std::f32::consts::TAU * b.rand();
    let lean = [lean_angle.cos(), lean_angle.sin()];
    let mut y = 0.0;
    while y < height {
        let bend = 8.0 * (y / height).powi(2);
        // Rings of the trunk: a darker band every few cells.
        let material = if (y as i32) % 4 == 0 {
            Material::Wood
        } else {
            Material::PalmTrunk
        };
        b.post(lean[0] * bend, lean[1] * bend, y, y + 1.0, 2.0, material);
        y += 1.0;
    }
    let top = [lean[0] * 8.0, height, lean[1] * 8.0];
    let fronds = 6 + (b.rand() * 3.0) as usize;
    for k in 0..fronds {
        let angle = std::f32::consts::TAU * (k as f32 + 0.4 * b.rand()) / fronds as f32;
        let (dx, dz) = (angle.cos(), angle.sin());
        let length = 11.0 + 4.0 * b.rand();
        let mut s = 0.0;
        while s < length {
            // Arching up, then drooping: a parabola.
            let y = top[1] + 1.0 + 0.9 * s - 0.09 * s * s;
            let (x, z) = (top[0] + dx * s, top[2] + dz * s);
            b.set(x, y, z, Material::PalmLeaves);
            // Leaflets on both sides, shorter towards the tip.
            let leaflet = ((1.0 - s / length) * 3.0).round();
            let mut w = 1.0;
            while w <= leaflet {
                b.set(x - dz * w, y - 0.3 * w, z + dx * w, Material::PalmLeaves);
                b.set(x + dz * w, y - 0.3 * w, z - dx * w, Material::PalmLeaves);
                w += 1.0;
            }
            s += 0.5;
        }
    }
    for _ in 0..(2 + (b.rand() * 3.0) as usize) {
        let angle = std::f32::consts::TAU * b.rand();
        b.post(
            top[0] + angle.cos() * 1.5,
            top[2] + angle.sin() * 1.5,
            top[1] - 2.0,
            top[1],
            2.0,
            Material::Wood,
        );
    }
}

/// Dead tree: a bare grey trunk, slightly leaning, with a few branches, some of them broken.
fn dead_tree(b: &mut Builder) {
    let height = 18.0 + (b.rand() * 10.0).floor();
    let lean_angle = std::f32::consts::TAU * b.rand();
    let lean = [lean_angle.cos(), lean_angle.sin()];
    let mut y = 0.0;
    while y < height {
        let off = 2.0 * y / height;
        b.post(
            lean[0] * off,
            lean[1] * off,
            y,
            y + 1.0,
            2.0,
            Material::DeadWood,
        );
        y += 1.0;
    }
    let branches = 2 + (b.rand() * 3.0) as usize;
    for _ in 0..branches {
        let angle = std::f32::consts::TAU * b.rand();
        let start = height * (0.4 + 0.5 * b.rand());
        let length = 3.0 + 7.0 * b.rand();
        let origin = [
            lean[0] * 2.0 * start / height,
            start,
            lean[1] * 2.0 * start / height,
        ];
        let end = [
            origin[0] + angle.cos() * length,
            start + 0.8 * length,
            origin[2] + angle.sin() * length,
        ];
        b.branch(origin, end, 1.0, Material::DeadWood);
    }
}

/// A tuft of grass: thin blades of various heights, the tall ones bending over.
fn grass(b: &mut Builder) {
    let blades = 5 + (b.rand() * 5.0) as usize;
    for _ in 0..blades {
        let angle = std::f32::consts::TAU * b.rand();
        let out = 2.5 * b.rand().sqrt();
        let (x, z) = (angle.cos() * out, angle.sin() * out);
        let height = 2.0 + (b.rand() * 4.0).floor();
        let lean = std::f32::consts::TAU * b.rand();
        let mut y = 0.0;
        while y < height {
            // The top of a tall blade leans over by one micro-cell.
            let bend = if height > 3.0 && y >= height - 1.0 {
                1.0
            } else {
                0.0
            };
            b.set(
                x + lean.cos() * bend,
                y,
                z + lean.sin() * bend,
                Material::TallGrass,
            );
            y += 1.0;
        }
    }
}

/// A flower: a stem with a leaf, petals of the variant's colour around a heart.
fn flower(b: &mut Builder, variant: u32) {
    let petals = [
        Material::FlowerRed,
        Material::FlowerYellow,
        Material::FlowerWhite,
        Material::FlowerViolet,
    ][variant as usize % 4];
    let heart = if petals == Material::FlowerYellow {
        Material::FlowerWhite
    } else {
        Material::FlowerYellow
    };
    let height = 3.0 + (b.rand() * 3.0).floor();
    let mut y = 0.0;
    while y < height {
        b.set(0.0, y, 0.0, Material::TallGrass);
        y += 1.0;
    }
    b.set(1.0, 1.0, 0.0, Material::TallGrass);
    b.set(0.0, height, 0.0, heart);
    for (dx, dz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        b.set(dx, height, dz, petals);
    }
}

/// A fern: fronds arching out of the ground, with leaflets along them.
fn fern(b: &mut Builder) {
    let fronds = 5 + (b.rand() * 4.0) as usize;
    for k in 0..fronds {
        let angle = std::f32::consts::TAU * (k as f32 + 0.5 * b.rand()) / fronds as f32;
        let (dx, dz) = (angle.cos(), angle.sin());
        let length = 6.0 + 4.0 * b.rand();
        let mut s = 0.0;
        while s < length {
            let y = 0.9 * s - 0.1 * s * s;
            let (x, z) = (dx * s, dz * s);
            b.set(x, y.max(0.0), z, Material::Fern);
            if s >= 2.0 && (s as i32) % 2 == 0 {
                b.set(x - dz, y.max(0.0), z + dx, Material::Fern);
                b.set(x + dz, y.max(0.0), z - dx, Material::Fern);
            }
            s += 0.5;
        }
    }
}

/// One to three mushrooms: a pale stem under a round red-brown cap.
fn mushrooms(b: &mut Builder) {
    let count = 1 + (b.rand() * 3.0) as usize;
    for k in 0..count {
        let (x, z) = if k == 0 {
            (0.0, 0.0)
        } else {
            let angle = std::f32::consts::TAU * b.rand();
            (angle.cos() * 3.0, angle.sin() * 3.0)
        };
        let height = 2.0 + (b.rand() * 3.0).floor();
        let mut y = 0.0;
        while y < height {
            b.set(x, y, z, Material::MushroomStem);
            y += 1.0;
        }
        let r = 1.0 + 0.8 * b.rand();
        let reach = r.ceil() as i32;
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let d = ((dx * dx + dz * dz) as f32).sqrt();
                if d <= r {
                    b.set(x + dx as f32, height, z + dz as f32, Material::MushroomCap);
                }
                if d <= r - 1.0 {
                    b.set(
                        x + dx as f32,
                        height + 1.0,
                        z + dz as f32,
                        Material::MushroomCap,
                    );
                }
            }
        }
    }
}

/// A stone, or a few pebbles, half sunk in the ground.
fn stones(b: &mut Builder, variant: u32) {
    let material = if variant.is_multiple_of(3) {
        Material::Rock
    } else {
        Material::Stone
    };
    let count = 1 + (b.rand() * 3.0) as usize;
    for k in 0..count {
        let r = if k == 0 {
            2.0 + 2.0 * b.rand()
        } else {
            1.0 + 1.0 * b.rand()
        };
        let angle = std::f32::consts::TAU * b.rand();
        let out = if k == 0 { 0.0 } else { 3.0 + 2.0 * b.rand() };
        b.blob(
            [angle.cos() * out, r * 0.3, angle.sin() * out],
            r,
            r * 0.6,
            material,
        );
    }
}

/// A dry shrub: bare twigs fanning out of the ground, a few dry leaves at their tips.
fn dry_shrub(b: &mut Builder) {
    let twigs = 4 + (b.rand() * 3.0) as usize;
    for _ in 0..twigs {
        let angle = std::f32::consts::TAU * b.rand();
        let length = 2.5 + 2.5 * b.rand();
        let end = [
            angle.cos() * length,
            2.0 + length * 0.9,
            angle.sin() * length,
        ];
        b.branch([0.0, 0.0, 0.0], end, 1.0, Material::DeadWood);
        if b.rand() < 0.7 {
            b.blob(end, 1.5, 1.2, Material::DryShrub);
        }
    }
}

/// Low shrub: a flattened ragged blob sitting on the ground.
fn bush(b: &mut Builder) {
    let r = 5.0 + 2.0 * b.rand();
    b.blob([0.0, r * 0.5, 0.0], r, r * 0.6, Material::Leaves);
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [Plant; 15] = Plant::ALL;

    #[test]
    fn every_model_has_matter_and_stands_on_its_anchor() {
        for plant in KINDS {
            for variant in 0..VARIANTS {
                let m = model(plant, variant, 7);
                assert!(m.solid_count() > 5, "{plant:?} {variant} almost empty");
                // Something touches the ground right around the anchor (no floating plant).
                let [ax, ay, az] = m.anchor;
                let grounded = (ax.saturating_sub(4)..(ax + 4).min(m.dims.nx))
                    .flat_map(|x| {
                        (az.saturating_sub(4)..(az + 4).min(m.dims.nz)).map(move |z| (x, z))
                    })
                    .any(|(x, z)| m.get(x, ay, z) != Material::Air);
                assert!(grounded, "{plant:?} {variant} floats");
            }
        }
    }

    #[test]
    fn variants_differ_and_are_deterministic() {
        assert_eq!(model(Plant::Broadleaf, 0, 3), model(Plant::Broadleaf, 0, 3));
        assert_ne!(model(Plant::Broadleaf, 0, 3), model(Plant::Broadleaf, 1, 3));
    }

    #[test]
    fn trees_are_taller_than_bushes() {
        let tall = model(Plant::Pine, 0, 1).dims.ny;
        let low = model(Plant::Bush, 0, 1).dims.ny;
        assert!(tall > 3 * low, "pine {tall}, bush {low}");
    }
}
