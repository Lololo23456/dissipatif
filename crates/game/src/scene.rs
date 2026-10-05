//! Turns a generated `World` into what the renderer draws: the ground and plant voxels (one
//! opaque volume coloured by material) and the water surface (one transparent volume).

use render::mesh::MeshData;
use render::mesher::mesh_field;
use render::palette;
use render::water_mesher::mesh_water;
use render::{LifeStyle, Renderer, VolumeStyle};
use sim::grid::{Dims, Field2, Field3};
use world::noise::hash_unit;
use world::{Material, World};

/// Seed offset of the per-voxel brightness variation.
const VARIATION_SEED: u64 = 0x5eed;

/// The world, ready to upload.
pub struct SceneData {
    /// Material id + brightness variation in [0, 1) per voxel (see `VolumeStyle::materials`).
    solid_look: Field3,
    solid_mesh: MeshData,
    /// Water: depth per column (a 2D grid stored with a height of 1), and its surface mesh.
    water_depth: Field3,
    water_mesh: MeshData,
    /// No overlays yet: zero fields with the shapes of the textures above.
    no_overlay_solid: Field3,
    no_overlay_water: Field3,
}

impl SceneData {
    pub fn build(world: &World) -> Self {
        let dims = world.dims();
        let seed = world.config.seed ^ VARIATION_SEED;
        let mut occupied = Field3::filled(dims, 0.0);
        let mut solid_look = Field3::filled(dims, 0.0);
        for (i, &id) in world.blocks().iter().enumerate() {
            if id == Material::Air.id() {
                continue;
            }
            occupied.data[i] = 1.0;
            // Same variation for a whole voxel: hash of its index.
            let variation = hash_unit(seed, &[i as i64]);
            solid_look.data[i] = id as f32 + 0.999 * variation;
        }
        let mut solid_mesh = MeshData::default();
        mesh_field(&occupied, 0.5, &mut solid_mesh);

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

        Self {
            no_overlay_solid: Field3::filled(dims, 0.0),
            no_overlay_water: Field3::filled(columns, 0.0),
            solid_look,
            solid_mesh,
            water_depth,
            water_mesh,
        }
    }

    pub fn solid_faces(&self) -> usize {
        self.solid_mesh.face_count()
    }

    pub fn water_faces(&self) -> usize {
        self.water_mesh.face_count()
    }

    /// Adds the world's volumes to a renderer and uploads everything (the world is static,
    /// nothing needs updating afterwards).
    pub fn install(&self, renderer: &mut Renderer) {
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
        renderer.upload_base(solid, &self.solid_look);
        renderer.upload_life(solid, &self.no_overlay_solid);
        renderer.upload_mesh(solid, &self.solid_mesh);
        renderer.upload_base(water, &self.water_depth);
        renderer.upload_life(water, &self.no_overlay_water);
        renderer.upload_mesh(water, &self.water_mesh);
    }
}

/// An overlay that never shows: its field stays at zero, below the fade.
fn no_overlay() -> LifeStyle {
    LifeStyle {
        palette: palette::foam(),
        fade: (1.0, 2.0),
        value_range: (1.0, 2.0),
    }
}
