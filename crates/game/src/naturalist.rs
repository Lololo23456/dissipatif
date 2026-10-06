//! The naturalist: a character made of articulated micro-voxel parts, and its procedural
//! animation (walking, breathing, looking around). No drawn animation: every pose is computed
//! from the motion.

use glam::{Mat4, Vec3};
use render::mesh::MeshData;
use render::mesher::mesh_materials;
use render::{PartId, PartInstance, Renderer};
use sim::grid::Dims;
use world::Material;

/// Size of a voxel of the character, in world cells.
const VOXEL: f32 = 0.066;
/// Seed of the brightness variation of the character's voxels.
const LOOK_SEED: u64 = 0x5a7;

/// A part drawn with its own transform: voxels, pivot (in voxels), where the pivot sits on the
/// body (in world cells, relative to the feet, the character facing +z).
struct Part {
    grid: Grid,
    pivot: [f32; 3],
    attach: Vec3,
}

/// Indices of the parts, in the order they are built and posed.
const LEFT_LEG: usize = 0;
const RIGHT_LEG: usize = 1;
const TORSO: usize = 2;
const HEAD: usize = 3;
const LEFT_ARM: usize = 4;
const RIGHT_ARM: usize = 5;
const PART_COUNT: usize = 6;

/// A small box of voxels being drawn.
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

    /// Fills the box [x0, x1) × [y0, y1) × [z0, z1) (clipped to the grid).
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

    fn set(&mut self, x: usize, y: usize, z: usize, material: Material) {
        self.fill([x, x + 1], [y, y + 1], [z, z + 1], material);
    }
}

/// Builds the parts. Units are voxels; the character faces +z.
fn parts() -> [Part; PART_COUNT] {
    // Legs: olive trousers, leather boots. Pivot at the hip (top centre).
    let leg = || {
        let mut g = Grid::new(3, 8, 3);
        g.fill([0, 3], [0, 3], [0, 3], Material::Leather);
        g.fill([0, 3], [0, 1], [3, 3], Material::Leather);
        g.fill([0, 3], [3, 8], [0, 3], Material::OliveCloth);
        g
    };
    // Torso: khaki shirt, leather belt with a brass buckle, red notebook at the belt,
    // canvas backpack with a rolled blanket on top. Pivot at the waist (bottom centre).
    let mut torso = Grid::new(9, 9, 8);
    // Body occupies x 1..8, z 2..6 (front is +z).
    torso.fill([1, 8], [0, 8], [2, 6], Material::Khaki);
    torso.fill([1, 8], [0, 1], [2, 6], Material::Leather);
    torso.set(4, 0, 6, Material::Brass);
    torso.fill([7, 8], [0, 2], [5, 7], Material::NotebookRed);
    // Straps over the shoulders.
    torso.fill([2, 3], [1, 8], [6, 7], Material::Leather);
    torso.fill([6, 7], [1, 8], [6, 7], Material::Leather);
    // Backpack behind (z 0..2) and its blanket roll.
    torso.fill([2, 7], [1, 7], [0, 2], Material::Canvas);
    torso.fill([3, 6], [3, 4], [0, 1], Material::Leather);
    torso.fill([1, 8], [7, 9], [0, 2], Material::OliveCloth);
    // Collar.
    torso.fill([3, 6], [8, 9], [3, 5], Material::Khaki);

    // Head: skin, hair at the back, eyes, felt hat with a wide brim and a leather band.
    // Pivot at the neck (bottom centre).
    let mut head = Grid::new(10, 11, 10);
    head.fill([2, 8], [0, 6], [2, 8], Material::Skin);
    head.fill([2, 8], [3, 6], [2, 3], Material::Hair);
    head.fill([2, 3], [3, 6], [2, 6], Material::Hair);
    head.fill([7, 8], [3, 6], [2, 6], Material::Hair);
    head.set(3, 3, 7, Material::Eye);
    head.set(6, 3, 7, Material::Eye);
    head.fill([0, 10], [6, 7], [0, 10], Material::Felt);
    head.fill([2, 8], [7, 8], [2, 8], Material::Leather);
    head.fill([2, 8], [8, 10], [2, 8], Material::Felt);
    head.fill([3, 7], [10, 11], [3, 7], Material::Felt);

    // Arms: rolled-up sleeves, bare forearms. Pivot at the shoulder (top centre).
    let arm = || {
        let mut g = Grid::new(2, 8, 2);
        g.fill([0, 2], [0, 4], [0, 2], Material::Skin);
        g.fill([0, 2], [4, 8], [0, 2], Material::Khaki);
        g
    };

    let v = VOXEL;
    [
        Part {
            grid: leg(),
            pivot: [1.5, 8.0, 1.5],
            attach: Vec3::new(-1.9 * v, 8.0 * v, 0.0),
        },
        Part {
            grid: leg(),
            pivot: [1.5, 8.0, 1.5],
            attach: Vec3::new(1.9 * v, 8.0 * v, 0.0),
        },
        Part {
            grid: torso,
            pivot: [4.5, 0.0, 4.0],
            attach: Vec3::new(0.0, 8.0 * v, 0.0),
        },
        Part {
            grid: head,
            pivot: [5.0, 0.0, 5.0],
            attach: Vec3::new(0.0, 17.0 * v, 0.0),
        },
        Part {
            grid: arm(),
            pivot: [1.0, 8.0, 1.0],
            attach: Vec3::new(-4.6 * v, 15.5 * v, 0.0),
        },
        Part {
            grid: arm(),
            pivot: [1.0, 8.0, 1.0],
            attach: Vec3::new(4.6 * v, 15.5 * v, 0.0),
        },
    ]
}

/// What the animation needs to know about the body this frame.
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    /// Feet, in world cells (smoothed for display).
    pub position: Vec3,
    /// Direction the body faces, radians around the vertical (0 = +z).
    pub facing: f32,
    /// Phase of the walking cycle, radians.
    pub stride_phase: f32,
    /// How strongly the limbs swing, 0 (still) to 1 (running).
    pub stride: f32,
    /// Forward lean of the torso, radians (when accelerating or running).
    pub lean: f32,
    /// Where the head looks, relative to the body: yaw and pitch, radians.
    pub look: [f32; 2],
    /// In the air: legs gather.
    pub airborne: bool,
    /// Seconds since start, for breathing.
    pub time: f32,
}

/// The naturalist in the renderer.
pub struct Naturalist {
    parts: [Part; PART_COUNT],
    ids: Vec<PartId>,
}

impl Naturalist {
    pub fn new() -> Self {
        Self {
            parts: parts(),
            ids: Vec::new(),
        }
    }

    /// Uploads the parts' meshes.
    pub fn install(&mut self, renderer: &mut Renderer) {
        self.ids = self
            .parts
            .iter()
            .enumerate()
            .map(|(k, part)| {
                let mut mesh = MeshData::default();
                mesh_materials(
                    &part.grid.voxels,
                    part.grid.dims,
                    part.pivot,
                    VOXEL,
                    LOOK_SEED + k as u64,
                    &mut mesh,
                );
                renderer.add_part(&mesh)
            })
            .collect();
    }

    /// Poses the parts for this frame.
    pub fn pose(&self, renderer: &mut Renderer, motion: &Motion) {
        for (k, transform) in self.transforms(motion).iter().enumerate() {
            if let Some(&id) = self.ids.get(k) {
                renderer.set_part_instances(id, &[PartInstance::new(*transform)]);
            }
        }
    }

    /// Model → world matrix of each part.
    fn transforms(&self, m: &Motion) -> [Mat4; PART_COUNT] {
        // Walking: legs and arms swing in opposition; the body rises twice per stride, at
        // each step, and breathes when still.
        let swing = m.stride_phase.sin() * 0.75 * m.stride;
        let bob = m.stride_phase.sin().abs() * 0.05 * m.stride;
        let breath = (m.time * 1.7).sin() * 0.008 * (1.0 - m.stride);
        let tuck = if m.airborne { 0.5 } else { 0.0 };
        let root = Mat4::from_translation(m.position + Vec3::Y * (bob + breath))
            * Mat4::from_rotation_y(m.facing);
        let place = |part: usize, rotation: Mat4| {
            root * Mat4::from_translation(self.parts[part].attach) * rotation
        };
        // A negative angle around x swings a hanging limb forward (towards +z).
        let torso = Mat4::from_rotation_x(m.lean);
        let torso_top = |part: usize, rotation: Mat4| {
            // Arms and head follow the torso's lean: pivot on the waist.
            let waist = self.parts[TORSO].attach;
            root * Mat4::from_translation(waist)
                * torso
                * Mat4::from_translation(self.parts[part].attach - waist)
                * rotation
        };
        [
            place(LEFT_LEG, Mat4::from_rotation_x(-swing - tuck)),
            place(RIGHT_LEG, Mat4::from_rotation_x(swing - tuck)),
            place(TORSO, torso),
            torso_top(
                HEAD,
                Mat4::from_rotation_y(m.look[0]) * Mat4::from_rotation_x(m.look[1] - m.lean),
            ),
            torso_top(
                LEFT_ARM,
                Mat4::from_rotation_x(swing * 0.8) * Mat4::from_rotation_z(-0.06),
            ),
            torso_top(
                RIGHT_ARM,
                Mat4::from_rotation_x(-swing * 0.8) * Mat4::from_rotation_z(0.06),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn still() -> Motion {
        Motion {
            position: Vec3::new(10.0, 5.0, 10.0),
            facing: 0.0,
            stride_phase: 0.0,
            stride: 0.0,
            lean: 0.0,
            look: [0.0; 2],
            airborne: false,
            time: 0.0,
        }
    }

    #[test]
    fn standing_naturalist_is_about_two_cells_tall_and_stands_on_its_feet() {
        let n = Naturalist::new();
        let transforms = n.transforms(&still());
        // Bottom of the boots at the feet position, top of the hat near 1.85 cells.
        let boot_sole = transforms[LEFT_LEG] * glam::Vec4::new(0.0, -8.0 * VOXEL, 0.0, 1.0);
        assert!((boot_sole.y - 5.0).abs() < 1e-4, "{boot_sole:?}");
        let hat_top = transforms[HEAD] * glam::Vec4::new(0.0, 11.0 * VOXEL, 0.0, 1.0);
        let height = hat_top.y - 5.0;
        assert!((1.7..2.0).contains(&height), "height {height}");
    }

    #[test]
    fn legs_swing_in_opposition_when_walking() {
        let n = Naturalist::new();
        let walking = Motion {
            stride_phase: std::f32::consts::FRAC_PI_2,
            stride: 1.0,
            ..still()
        };
        let t = n.transforms(&walking);
        let foot = glam::Vec4::new(0.0, -8.0 * VOXEL, 0.0, 1.0);
        let (left, right) = (t[LEFT_LEG] * foot, t[RIGHT_LEG] * foot);
        // One foot ahead (+z), the other behind.
        // Relative to the body, at z = 10.
        assert!(
            (left.z - 10.0) * (right.z - 10.0) < 0.0,
            "{left:?} {right:?}"
        );
    }

    #[test]
    fn facing_turns_the_whole_body() {
        let n = Naturalist::new();
        let turned = Motion {
            facing: std::f32::consts::FRAC_PI_2,
            ..still()
        };
        // The eyes look along +z when facing 0, along +x when facing a quarter turn.
        let eye = glam::Vec4::new(0.0, 3.0 * VOXEL, 3.0 * VOXEL, 1.0);
        let looking = n.transforms(&turned)[HEAD] * eye;
        assert!(
            looking.x > 10.0 && (looking.z - 10.0).abs() < 1e-3,
            "{looking:?}"
        );
    }
}
