//! How the objects laid in the world look, and what their physics shows: flames as strong as
//! the burning, incandescence whose colour follows the temperature (from the dull red of about
//! 800 K, the Draper point, through cherry and orange to yellow), steam while water boils
//! away, smoke above a fire, embers glowing.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use render::mesh::MeshData;
use render::mesher::mesh_materials;
use render::palette::srgb_hex;
use render::{PartId, PartInstance, ParticleInstance, Renderer};
use sim::grid::Dims;
use sim::thermal::KELVIN;
use world::Material;

use crate::items::{ClaySource, Flower, Matter};
use crate::objects::Objects;

/// Size of a voxel of the objects, cells.
const VOXEL: f32 = 0.0625;

struct Grid {
    dims: Dims,
    voxels: Vec<u8>,
}

impl Grid {
    fn new(nx: usize, ny: usize, nz: usize) -> Self {
        let dims = Dims { nx, ny, nz };
        Self {
            dims,
            voxels: vec![Material::Air.id(); dims.len()],
        }
    }

    fn fill(&mut self, x: [usize; 2], y: [usize; 2], z: [usize; 2], material: Material) {
        let d = self.dims;
        for zi in z[0]..z[1].min(d.nz) {
            for yi in y[0]..y[1].min(d.ny) {
                for xi in x[0]..x[1].min(d.nx) {
                    self.voxels[d.index(xi, yi, zi)] = material.id();
                }
            }
        }
    }
}

/// The little model of a matter, centred on its base.
fn model(matter: Matter) -> Grid {
    let lump = |material, w: usize, h: usize| {
        let mut g = Grid::new(w, h, w);
        g.fill([0, w], [0, h - 1], [1, w - 1], material);
        g.fill([1, w - 1], [0, h], [0, w], material);
        g
    };
    let dish = |material| {
        let mut g = Grid::new(6, 2, 6);
        g.fill([0, 6], [0, 2], [1, 5], material);
        g.fill([1, 5], [0, 2], [0, 6], material);
        g.fill([1, 5], [1, 2], [1, 5], Material::Air);
        g
    };
    let clay = |source| match source {
        ClaySource::Bank => Material::Gravel,
        ClaySource::RedEarth => Material::Clay,
    };
    match matter {
        Matter::Pebble { dark } => lump(
            if dark {
                Material::Rock
            } else {
                Material::Stone
            },
            3,
            2,
        ),
        Matter::DeadTwigs => {
            let mut g = Grid::new(8, 3, 8);
            g.fill([0, 8], [0, 1], [3, 5], Material::DeadWood);
            g.fill([3, 5], [1, 2], [0, 8], Material::DeadWood);
            g.fill([1, 7], [2, 3], [2, 4], Material::Wood);
            g
        }
        Matter::GrassFibre => {
            let mut g = Grid::new(4, 3, 4);
            g.fill([0, 4], [0, 1], [0, 4], Material::DryGrass);
            g.fill([1, 3], [1, 3], [1, 3], Material::DryGrass);
            g
        }
        Matter::Frond => {
            let mut g = Grid::new(7, 1, 3);
            g.fill([0, 7], [0, 1], [1, 2], Material::Fern);
            g.fill([1, 6], [0, 1], [0, 3], Material::Fern);
            g
        }
        Matter::Clay { source } => lump(clay(source), 3, 2),
        Matter::RawDish { source } => dish(clay(source)),
        Matter::FiredDish { source } => dish(match source {
            ClaySource::Bank => Material::DesertSand,
            ClaySource::RedEarth => Material::Sandstone,
        }),
        Matter::Shards => {
            let mut g = Grid::new(6, 1, 6);
            g.fill([0, 2], [0, 1], [1, 2], Material::Sandstone);
            g.fill([3, 5], [0, 1], [0, 2], Material::Sandstone);
            g.fill([2, 4], [0, 1], [4, 6], Material::Sandstone);
            g
        }
        Matter::Sand => lump(Material::Sand, 4, 1),
        Matter::Ash => lump(Material::Snow, 4, 1),
        Matter::Flower(f) => lump(
            match f {
                Flower::Daisy => Material::FlowerWhite,
                Flower::Poppy => Material::FlowerRed,
                Flower::Lavender => Material::FlowerViolet,
                Flower::Buttercup => Material::FlowerYellow,
            },
            2,
            2,
        ),
        Matter::Mushroom { .. } => lump(Material::MushroomCap, 2, 2),
    }
}

/// Every matter that can be laid (one model each).
fn all_matters() -> Vec<Matter> {
    let mut all = vec![
        Matter::Pebble { dark: false },
        Matter::Pebble { dark: true },
        Matter::DeadTwigs,
        Matter::GrassFibre,
        Matter::Frond,
        Matter::Shards,
        Matter::Sand,
        Matter::Ash,
        Matter::Mushroom { spotted: false },
        Matter::Mushroom { spotted: true },
    ];
    for source in [ClaySource::Bank, ClaySource::RedEarth] {
        all.push(Matter::Clay { source });
        all.push(Matter::RawDish { source });
        all.push(Matter::FiredDish { source });
    }
    for f in [
        Flower::Daisy,
        Flower::Poppy,
        Flower::Lavender,
        Flower::Buttercup,
    ] {
        all.push(Matter::Flower(f));
    }
    all
}

/// Colour of a body glowing at `temperature` K, and how strongly (0 below ~800 K): the
/// sequence a smith reads in iron.
fn incandescence(temperature: f32) -> ([f32; 3], f32) {
    let t = temperature;
    if t < 790.0 {
        return ([0.0; 3], 0.0);
    }
    let dull = srgb_hex(0x7a1c0c);
    let cherry = srgb_hex(0xd23a14);
    let orange = srgb_hex(0xff8a2a);
    let yellow = srgb_hex(0xffd47a);
    let mix = |a: [f32; 3], b: [f32; 3], k: f32| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * k);
    let color = if t < 1000.0 {
        mix(dull, cherry, (t - 790.0) / 210.0)
    } else if t < 1200.0 {
        mix(cherry, orange, (t - 1000.0) / 200.0)
    } else {
        mix(orange, yellow, ((t - 1200.0) / 200.0).min(1.0))
    };
    (color, ((t - 790.0) / 400.0).min(1.5))
}

pub struct ObjectsView {
    meshes: HashMap<Matter, PartId>,
    instances: HashMap<Matter, Vec<PartInstance>>,
}

impl ObjectsView {
    pub fn new() -> Self {
        Self {
            meshes: HashMap::new(),
            instances: HashMap::new(),
        }
    }

    pub fn install(&mut self, renderer: &mut Renderer) {
        for (k, matter) in all_matters().into_iter().enumerate() {
            let g = model(matter);
            let pivot = [g.dims.nx as f32 / 2.0, 0.0, g.dims.nz as f32 / 2.0];
            let mut mesh = MeshData::default();
            mesh_materials(&g.voxels, g.dims, pivot, VOXEL, 0xb0 + k as u64, &mut mesh);
            self.meshes.insert(matter, renderer.add_part(&mesh));
        }
    }

    /// Poses of the objects; their flames, glow, steam and smoke appended to `particles`.
    pub fn draw(
        &mut self,
        objects: &Objects,
        time: f32,
        renderer: &mut Renderer,
        particles: &mut Vec<ParticleInstance>,
    ) {
        for list in self.instances.values_mut() {
            list.clear();
        }
        for (i, p) in objects.placed().iter().enumerate() {
            let turn = (i as f32 * 2.399) % std::f32::consts::TAU;
            let pose = Mat4::from_translation(p.base) * Mat4::from_rotation_y(turn);
            self.instances
                .entry(p.matter)
                .or_default()
                .push(PartInstance::new(pose));
            let body = objects.body(i);
            let centre = Vec3::from(body.position);
            glow(
                centre,
                body.radius,
                body.temperature,
                i as f32,
                time,
                particles,
            );
            if body.burning {
                flames(centre, body.radius, body.power, time, i as f32, particles);
            } else if body.fuel.is_some_and(|f| f.mass > 0.0) && body.temperature > KELVIN + 180.0 {
                // Pyrolysis: fuel hot enough gives off smoke before it flames — the sign that
                // it is about to catch.
                let strength = ((body.temperature - KELVIN - 180.0) / 100.0).min(1.0);
                wisps(centre, strength, time, i as f32, particles);
            }
            if body.water > 0.0 && body.temperature > KELVIN + 98.0 {
                steam(centre, time, i as f32, particles);
            }
        }
        for ember in objects.embers() {
            let centre = Vec3::from(ember.position);
            glow(
                centre,
                0.03,
                ember.temperature.max(1000.0),
                0.0,
                time,
                particles,
            );
        }
        for (matter, id) in &self.meshes {
            let list = self.instances.get(matter).map_or(&[][..], |l| l.as_slice());
            renderer.set_part_instances(*id, list);
        }
    }
}

/// A hot object shining by itself: a glowing core the colour of its temperature.
fn glow(
    at: Vec3,
    radius: f32,
    temperature: f32,
    seed: f32,
    time: f32,
    out: &mut Vec<ParticleInstance>,
) {
    let (color, strength) = incandescence(temperature);
    if strength <= 0.0 {
        return;
    }
    let flicker = 0.85 + 0.15 * (time * 7.0 + seed * 3.1).sin();
    out.push(ParticleInstance {
        centre_size: [at.x, at.y, at.z, radius * 1.3],
        color: [color[0], color[1], color[2], -2.0 * strength * flicker],
    });
}

/// Flame tongues rising from a burning object: as many and as tall as it burns strongly.
fn flames(
    at: Vec3,
    radius: f32,
    power: f32,
    time: f32,
    seed: f32,
    out: &mut Vec<ParticleInstance>,
) {
    let strength = (power / 40_000.0).min(1.0);
    let yellow = srgb_hex(0xffd36b);
    let orange = srgb_hex(0xf2782e);
    let smoke = srgb_hex(0x8a8580);
    for i in 0..(4.0 + 12.0 * strength) as usize {
        let phase = ((time * 1.8 + i as f32 * 0.137 + seed * 0.31) % 1.0 + 1.0) % 1.0;
        let angle = i as f32 * 2.399 + seed;
        let spread = radius * 1.2 * (1.0 - phase);
        let color = [0, 1, 2].map(|c| yellow[c] + (orange[c] - yellow[c]) * phase);
        out.push(ParticleInstance {
            centre_size: [
                at.x + angle.cos() * spread,
                at.y + 0.05 + (0.15 + 0.5 * strength) * phase,
                at.z + angle.sin() * spread,
                (0.05 + 0.08 * strength) * (1.0 - phase),
            ],
            color: [color[0], color[1], color[2], -2.5],
        });
    }
    // Smoke: light puffs, thinning as they rise and drift with the wind.
    for i in 0..(2.0 + 4.0 * strength) as usize {
        let phase = ((time * 0.25 + i as f32 * 0.21 + seed * 0.11) % 1.0 + 1.0) % 1.0;
        out.push(ParticleInstance {
            centre_size: [
                at.x + 0.8 * phase,
                at.y + 0.5 + 2.2 * phase,
                at.z + 0.4 * phase,
                0.06 + 0.1 * phase * (1.0 - phase),
            ],
            color: [smoke[0], smoke[1], smoke[2], 0.0],
        });
    }
}

/// Thin grey wisps rising from smouldering fuel.
fn wisps(at: Vec3, strength: f32, time: f32, seed: f32, out: &mut Vec<ParticleInstance>) {
    let grey = srgb_hex(0xb8b4ae);
    for i in 0..(1.0 + 3.0 * strength) as usize {
        let phase = ((time * 0.5 + i as f32 * 0.3 + seed * 0.13) % 1.0 + 1.0) % 1.0;
        out.push(ParticleInstance {
            centre_size: [
                at.x + 0.08 * (time * 0.7 + seed + i as f32).sin(),
                at.y + 0.05 + 0.9 * phase,
                at.z + 0.08 * (time * 0.6 + seed + i as f32).cos(),
                (0.02 + 0.05 * phase) * (0.5 + strength),
            ],
            color: [grey[0], grey[1], grey[2], 0.0],
        });
    }
}

/// White steam rising while water boils away.
fn steam(at: Vec3, time: f32, seed: f32, out: &mut Vec<ParticleInstance>) {
    let white = srgb_hex(0xf0f0ee);
    for i in 0..4 {
        let phase = ((time * 0.6 + i as f32 * 0.25 + seed * 0.17) % 1.0 + 1.0) % 1.0;
        out.push(ParticleInstance {
            centre_size: [
                at.x + 0.1 * (seed + i as f32).sin(),
                at.y + 0.1 + 0.8 * phase,
                at.z + 0.1 * (seed + i as f32).cos(),
                0.05 + 0.12 * phase,
            ],
            color: [white[0], white[1], white[2], 0.0],
        });
    }
}
