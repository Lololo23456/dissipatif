//! What the naturalist hears: the levels of the ambient sounds around them, from the world,
//! the wind and the hour; and their footsteps.

use glam::Vec3;
use world::{Material, World};

use crate::sound::{Scene, Surface};

/// Water is heard up to this many cells away.
const WATER_REACH: i32 = 12;
/// Trees and grass are sampled within this radius, every `SAMPLE_STEP` cells.
const AROUND: i32 = 8;
const SAMPLE_STEP: i32 = 2;

/// Strength of the wind at `time` around (x, z), in [0, 1]: the same gusts as the plants and
/// the particles (see `ambient.rs`).
pub fn gust(time: f32, x: f32, z: f32) -> f32 {
    let along = 0.89 * x + 0.45 * z;
    0.35 + 0.65 * (0.5 + 0.5 * (time * 0.7 - along * 0.09).sin())
}

/// Levels of the ambient sounds at `feet`. `hour` in [0, 24), `night` in [0, 1].
/// `rain` in [0, 1] silences the birds.
pub fn hear(world: &World, feet: Vec3, time: f32, hour: f32, night: f32, rain: f32) -> Scene {
    let dims = world.dims();
    let (px, pz) = (feet.x.floor() as i32, feet.z.floor() as i32);
    let inside =
        |x: i32, z: i32| x >= 0 && z >= 0 && (x as usize) < dims.nx && (z as usize) < dims.nz;

    // Water: the closest water column, heard louder as it nears.
    let mut nearest = f32::INFINITY;
    for dz in -WATER_REACH..=WATER_REACH {
        for dx in -WATER_REACH..=WATER_REACH {
            let (x, z) = (px + dx, pz + dz);
            if inside(x, z) && world.water_level(x as usize, z as usize).is_some() {
                nearest = nearest.min(((dx * dx + dz * dz) as f32).sqrt());
            }
        }
    }
    let water = (1.0 - nearest / WATER_REACH as f32).clamp(0.0, 1.0).powi(2);

    // Trees (leaves above the ground) and grass, sampled around.
    let (mut samples, mut leafy, mut grassy) = (0, 0, 0);
    for dz in (-AROUND..=AROUND).step_by(SAMPLE_STEP as usize) {
        for dx in (-AROUND..=AROUND).step_by(SAMPLE_STEP as usize) {
            let (x, z) = (px + dx, pz + dz);
            if !inside(x, z) {
                continue;
            }
            let (x, z) = (x as usize, z as usize);
            samples += 1;
            let top = world.ground_top(x, z);
            // `ground_top` is the first empty cell: the surface block is just below.
            if matches!(
                world.block(x, top.saturating_sub(1), z),
                Material::Grass | Material::ForestFloor | Material::DryGrass
            ) {
                grassy += 1;
            }
            let crown = (top + 3..(top + 16).min(dims.ny)).any(|y| {
                let m = world.block(x, y, z);
                m.is_canopy() || m == Material::PineNeedles
            });
            if crown {
                leafy += 1;
            }
        }
    }
    let share = |n: i32| {
        if samples == 0 {
            0.0
        } else {
            n as f32 / samples as f32
        }
    };
    let (trees, grass) = (share(leafy), share(grassy));

    // Birds sing by day near trees, most of all at dawn (the dawn chorus); crickets at night
    // in the grass.
    let dawn = (-((hour - 6.5) / 1.2).powi(2)).exp();
    let day = 1.0 - night;
    let birds = day * (0.15 + 0.85 * trees) * (0.5 + 1.5 * dawn) * 0.6 * (1.0 - rain);
    let crickets = night * (0.2 + 0.8 * grass);

    Scene {
        wind: gust(time, feet.x, feet.z),
        foliage: trees,
        water,
        birds: birds.min(1.0),
        crickets: (crickets * (1.0 - rain)).min(1.0),
        rain,
    }
}

/// The ground under the feet, as it sounds.
pub fn surface(world: &World, feet: Vec3, in_water: bool) -> Surface {
    if in_water {
        return Surface::Water;
    }
    let dims = world.dims();
    let (x, y, z) = (feet.x.floor(), feet.y.floor() - 1.0, feet.z.floor());
    if x < 0.0 || y < 0.0 || z < 0.0 {
        return Surface::Grass;
    }
    let (x, y, z) = (x as usize, y as usize, z as usize);
    if x >= dims.nx || y >= dims.ny || z >= dims.nz {
        return Surface::Grass;
    }
    match world.block(x, y, z) {
        Material::Sand | Material::DesertSand | Material::Gravel | Material::Snow => Surface::Sand,
        Material::Rock | Material::Sandstone | Material::Stone | Material::Clay => Surface::Stone,
        _ => Surface::Grass,
    }
}

/// Surface of the water at the feet' column, if any.
pub fn water_level_at(world: &World, feet: Vec3) -> Option<f32> {
    let dims = world.dims();
    if feet.x < 0.0 || feet.z < 0.0 {
        return None;
    }
    let (x, z) = (feet.x as usize, feet.z as usize);
    if x >= dims.nx || z >= dims.nz {
        return None;
    }
    world.water_level(x, z)
}

/// Ground that keeps footprints: sand and snow.
pub fn soft_ground(world: &World, feet: Vec3) -> bool {
    let dims = world.dims();
    let (x, y, z) = (feet.x.floor(), feet.y.floor() - 1.0, feet.z.floor());
    if x < 0.0 || y < 0.0 || z < 0.0 {
        return false;
    }
    let (x, y, z) = (x as usize, y as usize, z as usize);
    x < dims.nx
        && y < dims.ny
        && z < dims.nz
        && matches!(
            world.block(x, y, z),
            Material::Sand | Material::DesertSand | Material::Snow
        )
}
