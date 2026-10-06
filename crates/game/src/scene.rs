//! Turns a generated `World` into what the renderer draws: the ground voxels (one opaque
//! volume coloured by material), the plants (micro-voxel models drawn by instancing) and the
//! water surface (one transparent volume).

use glam::Vec3;
use render::mesh::MeshData;
use render::mesher::{mesh_field, mesh_materials};
use render::palette;
use render::water_mesher::mesh_water;
use render::{LifeStyle, ModelId, ModelInstance, Renderer, VolumeStyle};
use sim::grid::{Dims, Field2, Field3};
use world::noise::hash_unit;
use world::plants::VARIANTS;
use world::{Biome, Material, Plant, World};

use crate::trample::Pliable;

/// The ground's look (material id and a brightness variation per voxel) and mesh. Plants are
/// drawn from their models: the world grid only has a coarse copy of them, left out here.
fn solid(world: &World) -> (Field3, MeshData) {
    let dims = world.dims();
    let seed = world.config.seed ^ VARIATION_SEED;
    let mut occupied = Field3::filled(dims, 0.0);
    let mut look = Field3::filled(dims, 0.0);
    for (i, &id) in world.blocks().iter().enumerate() {
        let plant = Material::from_id(id).is_some_and(Material::is_plant);
        if id == Material::Air.id() || plant {
            continue;
        }
        occupied.data[i] = 1.0;
        // Same variation for a whole voxel: hash of its index.
        let variation = hash_unit(seed, &[i as i64]);
        look.data[i] = id as f32 + 0.999 * variation;
    }
    let mut mesh = MeshData::default();
    mesh_field(&occupied, 0.5, &mut mesh);
    (look, mesh)
}

/// Seed offset of the per-voxel brightness variation.
const VARIATION_SEED: u64 = 0x5eed;

/// The world, ready to upload.
pub struct SceneData {
    dims: Dims,
    /// The ground's volume in the renderer, once installed.
    solid_id: Option<render::VolumeId>,
    /// Material id + brightness variation in [0, 1) per voxel (see `VolumeStyle::materials`).
    solid_look: Field3,
    solid_mesh: MeshData,
    /// Water: depth per column (a 2D grid stored with a height of 1), and its surface mesh.
    water_depth: Field3,
    water_mesh: MeshData,
    /// One mesh per plant model (kind × variant) and where each is drawn.
    plant_models: Vec<(MeshData, Vec<ModelInstance>)>,
    /// Plants that bend when walked through, with where their instance is.
    pliable: Vec<Pliable>,
    /// Where each plant (index in `World::plants`) is drawn: model group, index in it.
    slots: Vec<(usize, usize)>,
    /// Ids of the plant models in the renderer, once installed.
    model_ids: Vec<ModelId>,
    /// Model groups whose instances changed since the last upload.
    dirty: Vec<bool>,
    /// No overlays yet: zero fields with the shapes of the textures above.
    no_overlay_solid: Field3,
    no_overlay_water: Field3,
}

impl SceneData {
    pub fn build(world: &World) -> Self {
        let dims = world.dims();
        let seed = world.config.seed ^ VARIATION_SEED;
        let (solid_look, solid_mesh) = solid(world);

        let (nx, nz) = (dims.nx, dims.nz);
        let columns = Dims { nx, ny: 1, nz };
        let mut floor = Field2::filled(nx, nz, 0.0);
        let mut surface = Field2::filled(nx, nz, 0.0);
        let mut water_depth = Field3::filled(columns, 0.0);
        for z in 0..nz {
            for x in 0..nx {
                let top = world.ground_top(x, z) as f32;
                floor.set(x, z, top);
                let level = world.water_level(x, z).unwrap_or(f32::NEG_INFINITY);
                // The water surface is drawn where it is above the ground voxels.
                surface.set(x, z, level.max(top));
                water_depth.set(x, 0, z, (level - world.ground.get(x, z)).max(0.0));
            }
        }
        let mut water_mesh = MeshData::default();
        mesh_water(&floor, &surface, &mut water_mesh);

        // Plants: one mesh per model, a quarter of a cell per micro-voxel, its origin where the
        // plant stands (centre of the bottom of its base cell).
        // Instances grouped by model in one pass: index = kind × VARIANTS + variant.
        let model_index = |plant: Plant, variant: u32| {
            let kind = Plant::ALL.iter().position(|&p| p == plant).unwrap_or(0);
            kind * VARIANTS as usize + variant as usize
        };
        let mut grouped = vec![Vec::new(); Plant::ALL.len() * VARIANTS as usize];
        let mut pliable = Vec::new();
        let mut slots = Vec::with_capacity(world.plants().len());
        for p in world.plants() {
            let [x, y, z] = p.base;
            let biome = world.biome(x, z);
            let group = model_index(p.plant, p.variant);
            let position = [
                x as f32 + 0.5 + p.offset[0],
                y as f32,
                z as f32 + 0.5 + p.offset[1],
            ];
            let yielding = yielding(p.plant);
            if yielding > 0.0 {
                pliable.push(Pliable {
                    base: Vec3::from(position),
                    yielding,
                    group,
                    index: grouped[group].len(),
                });
            }
            slots.push((group, grouped[group].len()));
            grouped[group].push(ModelInstance::new(
                position,
                p.rotation,
                p.mirrored,
                p.scale,
                foliage_tint(p.plant, biome, p.base, p.offset, seed),
                flexibility(p.plant),
            ));
        }
        let mut plant_models = Vec::new();
        for plant in Plant::ALL {
            for variant in 0..VARIANTS {
                let model = world.model(plant, variant);
                let mut mesh = MeshData::default();
                mesh_materials(
                    &model.voxels,
                    model.dims,
                    model.anchor.map(|a| a as f32),
                    1.0 / model.resolution as f32,
                    seed ^ (plant as u64 * 31 + variant as u64),
                    &mut mesh,
                );
                let instances = std::mem::take(&mut grouped[model_index(plant, variant)]);
                plant_models.push((mesh, instances));
            }
        }

        let groups = plant_models.len();
        Self {
            dims,
            solid_id: None,
            plant_models,
            pliable,
            slots,
            model_ids: Vec::new(),
            dirty: vec![false; groups],
            no_overlay_solid: Field3::filled(dims, 0.0),
            no_overlay_water: Field3::filled(columns, 0.0),
            solid_look,
            solid_mesh,
            water_depth,
            water_mesh,
        }
    }

    /// The ground changed (dug): rebuilds its mesh and, if installed, sends it to the
    /// renderer. The whole ground is remeshed (some tens of milliseconds); per-chunk
    /// remeshing will come with the micro-bricks.
    pub fn remesh_ground(&mut self, world: &World, renderer: Option<&mut Renderer>) {
        let (look, mesh) = solid(world);
        self.solid_look = look;
        self.solid_mesh = mesh;
        if let (Some(renderer), Some(id)) = (renderer, self.solid_id) {
            renderer.upload_base(id, &self.solid_look);
            renderer.upload_mesh(id, &self.solid_mesh);
        }
    }

    pub fn solid_faces(&self) -> usize {
        self.solid_mesh.face_count()
    }

    pub fn water_faces(&self) -> usize {
        self.water_mesh.face_count()
    }

    /// Faces of all plant models (each drawn many times).
    pub fn plant_model_faces(&self) -> usize {
        self.plant_models.iter().map(|(m, _)| m.face_count()).sum()
    }

    /// Adds the world's volumes to a renderer and uploads everything (the world is static,
    /// nothing needs updating afterwards).
    pub fn install(&mut self, renderer: &mut Renderer) {
        // The whole world casts and receives shadows.
        let Dims { nx, ny, nz } = self.dims;
        renderer.set_scene_bounds(Vec3::ZERO, Vec3::new(nx as f32, ny as f32, nz as f32));
        let solid = renderer.add_volume(&VolumeStyle {
            origin: [0.0; 3],
            palette: palette::earth(),
            value_range: (0.0, 1.0),
            materials: true,
            life: no_overlay(),
            transparent: false,
        });
        let water = renderer.add_volume(&VolumeStyle {
            origin: [0.0; 3],
            palette: palette::water(),
            // Depth: turquoise lagoon at 0, deep blue from 2.5 cells.
            value_range: (0.0, 2.5),
            materials: false,
            life: no_overlay(),
            transparent: true,
        });
        self.solid_id = Some(solid);
        renderer.upload_base(solid, &self.solid_look);
        renderer.upload_life(solid, &self.no_overlay_solid);
        renderer.upload_mesh(solid, &self.solid_mesh);
        renderer.upload_base(water, &self.water_depth);
        renderer.upload_life(water, &self.no_overlay_water);
        renderer.upload_mesh(water, &self.water_mesh);
        self.model_ids = self
            .plant_models
            .iter()
            .map(|(mesh, instances)| {
                let model = renderer.add_model(mesh);
                renderer.set_model_instances(model, instances);
                model
            })
            .collect();
    }

    /// The plants that bend when walked through (for `trample::Trample`).
    pub fn pliable(&self) -> Vec<Pliable> {
        self.pliable.clone()
    }

    /// Bends a plant; uploaded with the next `upload_changes`.
    pub fn set_bend(&mut self, plant: &Pliable, bend: glam::Vec2) {
        if let Some(instance) = self.plant_models[plant.group].1.get_mut(plant.index) {
            instance.set_bend(bend.to_array());
            self.dirty[plant.group] = true;
        }
    }

    /// Stops drawing plant `plant` (picked up): its instance shrinks to nothing.
    pub fn hide(&mut self, plant: usize) {
        if let Some(&(group, index)) = self.slots.get(plant)
            && let Some(instance) = self.plant_models[group].1.get_mut(index)
        {
            instance.scale_mirror[0] = 0.0;
            self.dirty[group] = true;
        }
    }

    /// Sends the model groups that changed to the renderer.
    pub fn upload_changes(&mut self, renderer: &mut Renderer) {
        for (group, dirty) in self.dirty.iter_mut().enumerate() {
            if *dirty && let Some(&id) = self.model_ids.get(group) {
                renderer.set_model_instances(id, &self.plant_models[group].1);
                *dirty = false;
            }
        }
    }
}

/// How much a plant bends in the wind: 1 for a broadleaf tree, less for stiffer plants, 0 for
/// a cactus.
///
/// The bending grows with the square of the height (see `wind` in voxel.wgsl), so small plants
/// need a large flexibility to move at all: a grass blade is a hundred times shorter than a tree.
fn flexibility(plant: Plant) -> f32 {
    match plant {
        Plant::Broadleaf | Plant::Birch => 1.0,
        Plant::Willow => 0.9,
        Plant::Acacia => 0.8,
        Plant::Palm => 0.7,
        Plant::Pine => 0.6,
        Plant::Bush => 0.5,
        Plant::DeadTree => 0.2,
        Plant::Grass => 4.0,
        Plant::Flower => 3.5,
        Plant::Fern => 2.0,
        Plant::DryShrub => 0.8,
        Plant::Cactus | Plant::Mushroom | Plant::Stone => 0.0,
    }
}

/// How much a plant bends away from the player walking through it: grass and flowers fully,
/// bushes partly; trees, cacti, stones and mushrooms not at all.
fn yielding(plant: Plant) -> f32 {
    match plant {
        Plant::Grass | Plant::Flower => 1.0,
        Plant::Fern => 0.8,
        Plant::Bush | Plant::DryShrub => 0.3,
        _ => 0.0,
    }
}

/// Seed offset of the foliage shades.
const TINT_SEED: u64 = 0x7147;
/// Share of broadleaf trees already turning orange.
const AUTUMN_SHARE: f32 = 0.07;

/// Colour multiplier of a plant's foliage: each plant its own shade. Brightness and warmth
/// (from blue-green to yellow-green) vary; a few broadleaf trees have turned orange. Conifers
/// stay in cooler, darker greens.
fn foliage_tint(
    plant: Plant,
    biome: Biome,
    base: [usize; 3],
    offset: [f32; 2],
    seed: u64,
) -> [f32; 3] {
    // Ground cover shares columns: the offset tells two tufts of the same column apart.
    let sub = (offset[0] * 997.0) as i64 ^ ((offset[1] * 991.0) as i64) << 16;
    let draw = |k: i64| hash_unit(seed ^ TINT_SEED, &[base[0] as i64, base[2] as i64, sub, k]);
    if plant == Plant::Grass && biome == Biome::Savanna {
        // Dry season grass: straw yellow.
        let b = 0.9 + 0.2 * draw(4);
        return [1.25 * b, 1.05 * b, 0.55 * b];
    }
    if plant == Plant::Broadleaf && draw(0) < AUTUMN_SHARE {
        // Late-summer turn: from golden to burnt orange.
        let t = draw(1);
        return [1.45 + 0.15 * t, 1.0 - 0.15 * t, 0.45];
    }
    let (brightness, warmth) = match plant {
        Plant::Pine => (0.8 + 0.25 * draw(2), -draw(3)),
        _ => (0.85 + 0.27 * draw(2), 2.0 * draw(3) - 1.0),
    };
    [
        brightness * (1.0 + 0.1 * warmth),
        brightness * (1.0 + 0.04 * warmth),
        brightness * (1.0 - 0.15 * warmth),
    ]
}

/// An overlay that never shows: its field stays at zero, below the fade.
fn no_overlay() -> LifeStyle {
    LifeStyle {
        palette: palette::foam(),
        fade: (1.0, 2.0),
        value_range: (1.0, 2.0),
    }
}
