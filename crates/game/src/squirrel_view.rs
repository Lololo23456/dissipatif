//! Squirrels in the renderer: a small body with its tufted ears and a great bushy tail curled
//! over the back, hopping (the body arcs, the tail streams behind); bent over the ground while
//! burying or digging. Out of sight while in its tree.

use glam::{Mat4, Vec3};
use render::mesh::MeshData;
use render::mesher::mesh_materials;
use render::{PartId, PartInstance, Renderer};
use world::Material;

use crate::naturalist::Grid;
use crate::squirrels::{Doing, Squirrel};

/// Size of a voxel of a squirrel, in world cells: larger than life (a real red
/// squirrel is a hand and a half long), so it is seen from the high camera.
const VOXEL: f32 = 0.045;
const LOOK_SEED: u64 = 0x5c12;

/// Fills where `paint` says, voxel centres at `p - offset`.
fn sculpt(size: [usize; 3], offset: Vec3, paint: impl Fn(Vec3) -> Option<Material>) -> Grid {
    let mut g = Grid::new(size[0], size[1], size[2]);
    for z in 0..size[2] {
        for y in 0..size[1] {
            for x in 0..size[0] {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5) - offset;
                if let Some(m) = paint(p) {
                    g.set(x, y, z, m);
                }
            }
        }
    }
    g
}

pub struct SquirrelView {
    /// Body (with head and legs) and tail, and their pivots (voxels).
    parts: [(Grid, [f32; 3]); 2],
    ids: Vec<PartId>,
}

impl SquirrelView {
    pub fn new() -> Self {
        // Body: an egg along z, the head at the front, pale belly, tufted ears, dark eye.
        let body = sculpt([7, 9, 14], Vec3::new(3.5, 0.0, 6.0), |p| {
            let body = (p.x / 2.6).powi(2) + ((p.y - 3.2) / 2.6).powi(2) + (p.z / 4.5).powi(2);
            let head =
                (p.x / 2.2).powi(2) + ((p.y - 4.8) / 2.0).powi(2) + ((p.z - 5.2) / 2.2).powi(2);
            let ear = |side: f32| {
                (p.x - 1.1 * side).abs() < 0.6 && (p.y - 7.5).abs() < 1.1 && (p.z - 4.6).abs() < 0.6
            };
            let legs = p.y < 1.5 && (p.z.abs() - 3.0).abs() < 1.2 && p.x.abs() < 2.0;
            if ear(-1.0) || ear(1.0) {
                return Some(Material::DeerDark);
            }
            if head <= 1.0 {
                if p.x.abs() > 1.4 && (p.y - 5.3).abs() < 0.6 && (p.z - 6.0).abs() < 0.6 {
                    return Some(Material::Eye);
                }
                return Some(if p.y < 4.0 && p.z > 6.0 {
                    Material::DeerPale
                } else {
                    Material::DeerCoat
                });
            }
            if body <= 1.0 || legs {
                return Some(if p.y < 2.2 && p.x.abs() < 1.6 {
                    Material::DeerPale
                } else {
                    Material::DeerCoat
                });
            }
            None
        });
        // Tail: rising from the rump and curling forward over the back; bushy.
        let tail = sculpt([8, 16, 9], Vec3::new(4.0, 0.0, 6.0), |p| {
            // Centre line of the curl: up and forward.
            let t = (p.y / 15.0).clamp(0.0, 1.0);
            let centre_z = -3.0 + 7.0 * t * t;
            let r = 1.6 + 1.8 * (std::f32::consts::PI * t).sin();
            let d = (p.x * p.x + (p.z - centre_z).powi(2)).sqrt();
            (d < r && p.y < 15.0).then_some(if d > r - 0.8 && t > 0.3 {
                Material::DeerDark
            } else {
                Material::DeerCoat
            })
        });
        Self {
            parts: [(body, [3.5, 0.0, 6.0]), (tail, [4.0, 0.0, 6.0])],
            ids: Vec::new(),
        }
    }

    pub fn install(&mut self, renderer: &mut Renderer) {
        self.ids = self
            .parts
            .iter()
            .enumerate()
            .map(|(k, (grid, pivot))| {
                let mut mesh = MeshData::default();
                mesh_materials(
                    &grid.voxels,
                    grid.dims,
                    *pivot,
                    VOXEL,
                    LOOK_SEED + k as u64,
                    &mut mesh,
                );
                renderer.add_part(&mesh)
            })
            .collect();
    }

    pub fn draw(&self, renderer: &mut Renderer, squirrels: &[Squirrel], time: f32) {
        let mut bodies = Vec::new();
        let mut tails = Vec::new();
        for (k, s) in squirrels.iter().enumerate() {
            if s.doing == Doing::InTree {
                if !s.perched {
                    // In its nest for the night.
                    continue;
                }
                // Clinging to the trunk, head down, facing it, the tail flat against it.
                let root = Mat4::from_translation(
                    s.position + Vec3::new(0.0, crate::squirrels::PERCH_HEIGHT, 0.0),
                ) * Mat4::from_rotation_y(s.heading)
                    * Mat4::from_translation(Vec3::new(0.0, 0.0, -0.5))
                    // Head down, belly against the bark (the trunk is ahead, +z).
                    * Mat4::from_rotation_z(std::f32::consts::PI)
                    * Mat4::from_rotation_x(
                        -std::f32::consts::FRAC_PI_2 + 0.1 * (time * 1.3 + k as f32).sin(),
                    );
                bodies.push(PartInstance::new(root));
                tails.push(PartInstance::new(
                    root * Mat4::from_translation(Vec3::new(0.0, 2.5, -4.5) * VOXEL)
                        * Mat4::from_rotation_x(-1.2),
                ));
                continue;
            }
            let hopping = s.speed > 0.1;
            let arc = if hopping {
                (s.hop.sin()).abs() * 0.12
            } else {
                0.0
            };
            let busy = matches!(s.doing, Doing::Burying(..) | Doing::Digging(..));
            // Burying or digging: nose down, forepaws working.
            let pitch = if busy {
                0.5 + 0.1 * (time * 14.0 + k as f32).sin()
            } else if hopping {
                0.3 * s.hop.cos()
            } else {
                -0.15
            };
            let root = Mat4::from_translation(s.position + Vec3::Y * arc)
                * Mat4::from_rotation_y(s.heading)
                * Mat4::from_rotation_x(pitch);
            bodies.push(PartInstance::new(root));
            // The tail streams back when hopping, stands curled when still.
            let sway = 0.15 * (time * 2.0 + k as f32).sin();
            let stream = if hopping { 0.9 } else { 0.0 };
            tails.push(PartInstance::new(
                root * Mat4::from_translation(Vec3::new(0.0, 2.5, -4.5) * VOXEL)
                    * Mat4::from_rotation_x(-stream)
                    * Mat4::from_rotation_z(sway),
            ));
        }
        if let [body, tail] = self.ids[..] {
            renderer.set_part_instances(body, &bodies);
            renderer.set_part_instances(tail, &tails);
        }
    }
}
