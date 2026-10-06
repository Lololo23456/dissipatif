//! Red deer in the renderer: articulated micro-voxel parts (body, neck and head, four legs),
//! posed from what each deer does. A hind stands about 1.2 cells at the shoulder; calves are
//! drawn smaller. Also draws the naturalist in the shape of a deer (the spell).

use glam::{Mat4, Vec3};
use render::mesh::MeshData;
use render::mesher::mesh_materials;
use render::{PartId, PartInstance, Renderer};
use world::Material;

use crate::deer::{Activity, Deer};
use crate::naturalist::Grid;

/// Size of a voxel of a deer, in world cells.
const VOXEL: f32 = 0.07;
const LOOK_SEED: u64 = 0xdee7;
/// Leg length, in voxels: the body's underside stands this high.
const LEG: f32 = 11.0;

const BODY: usize = 0;
const NECK: usize = 1;
const FRONT_LEFT: usize = 2;
const FRONT_RIGHT: usize = 3;
const BACK_LEFT: usize = 4;
const BACK_RIGHT: usize = 5;
const PARTS: usize = 6;

/// What the drawing needs of a deer.
#[derive(Clone, Copy, Debug)]
pub struct DeerPose {
    pub position: Vec3,
    pub heading: f32,
    /// −1 grazing, 0 level, 1 alert, more raised to the sky.
    pub head: f32,
    pub lying: f32,
    pub stride: f32,
    pub speed: f32,
    pub size: f32,
    /// Seconds left of a foot stamp.
    pub stamp: f32,
}

impl DeerPose {
    pub fn of(d: &Deer) -> Self {
        Self {
            position: d.position,
            heading: d.heading,
            head: d.head,
            lying: d.lying,
            stride: d.stride,
            speed: if d.activity == Activity::Lying {
                0.0
            } else {
                d.speed
            },
            size: d.size,
            stamp: d.stamp,
        }
    }
}

struct Part {
    grid: Grid,
    pivot: [f32; 3],
    /// Where the pivot sits on the deer, in voxels, relative to the feet (facing +z).
    attach: Vec3,
}

pub struct DeerView {
    parts: [Part; PARTS],
    ids: Vec<PartId>,
}

impl DeerView {
    pub fn new() -> Self {
        Self {
            parts: parts(),
            ids: Vec::new(),
        }
    }

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

    /// Poses every deer for this frame.
    pub fn draw(&self, renderer: &mut Renderer, deer: &[DeerPose]) {
        let mut instances: [Vec<PartInstance>; PARTS] = Default::default();
        for pose in deer {
            for (k, transform) in self.transforms(pose).iter().enumerate() {
                instances[k].push(PartInstance::new(*transform));
            }
        }
        for (k, list) in instances.iter().enumerate() {
            if let Some(&id) = self.ids.get(k) {
                renderer.set_part_instances(id, list);
            }
        }
    }

    fn transforms(&self, d: &DeerPose) -> [Mat4; PARTS] {
        let v = VOXEL;
        // Gait: a walk moves the legs in diagonal pairs; a gallop, fore and hind pairs together
        // (a bound), the body rocking.
        let galloping = d.speed > 5.0;
        let amplitude = (d.speed / 2.5).min(1.0) * if galloping { 0.9 } else { 0.5 };
        let phase = d.stride;
        let rock = if galloping { 0.1 * phase.sin() } else { 0.0 };
        // Lying: the body comes down, the legs fold under it.
        let lying = d.lying.clamp(0.0, 1.0);
        let grazing = (-d.head).max(0.0);
        let root = Mat4::from_translation(d.position + Vec3::Y * (-LEG * v * 0.8 * lying * d.size))
            * Mat4::from_rotation_y(d.heading)
            * Mat4::from_scale(Vec3::splat(d.size))
            * Mat4::from_rotation_x(rock + 0.08 * grazing);
        let place = |part: usize, rotation: Mat4| {
            root * Mat4::from_translation(self.parts[part].attach * v) * rotation
        };
        let leg = |part: usize, offset: f32, front: bool| {
            let swing = (phase + offset).sin() * amplitude * (1.0 - lying);
            let fold = if front { 1.5 } else { -1.5 } * lying;
            place(part, Mat4::from_rotation_x(swing + fold))
        };
        let (front_pair, back_pair) = if galloping {
            (
                [0.0, 0.3],
                [std::f32::consts::PI, std::f32::consts::PI + 0.3],
            )
        } else {
            ([0.0, std::f32::consts::PI], [std::f32::consts::PI, 0.0])
        };
        // The stamp: the right foreleg lifted and struck down.
        let stamp = if d.stamp > 0.0 {
            -0.7 * (std::f32::consts::PI * d.stamp / 0.6).sin()
        } else {
            0.0
        };
        // Neck: leaning forward at rest, down to the grass, upright when alert.
        let neck = if d.head < 0.0 {
            0.35 + 1.95 * grazing
        } else {
            0.35 - 0.6 * d.head
        };
        [
            place(BODY, Mat4::IDENTITY),
            place(NECK, Mat4::from_rotation_x(neck)),
            leg(FRONT_LEFT, front_pair[0], true),
            leg(FRONT_RIGHT, front_pair[1], true) * Mat4::from_rotation_x(stamp),
            leg(BACK_LEFT, back_pair[0], false),
            leg(BACK_RIGHT, back_pair[1], false),
        ]
    }
}

/// Builds the parts. Units are voxels; the deer faces +z.
fn parts() -> [Part; PARTS] {
    // Body: a summer-red coat, a pale belly, the pale rump patch round a short tail.
    let mut body = Grid::new(8, 8, 23);
    body.fill([0, 8], [0, 8], [1, 23], Material::DeerCoat);
    // Rounded: the long edges cut.
    for z in 1..23 {
        for (x, y) in [(0, 0), (7, 0), (0, 7), (7, 7)] {
            body.set(x, y, z, Material::Air);
        }
    }
    body.fill([2, 6], [0, 1], [4, 20], Material::DeerPale);
    body.fill([2, 6], [2, 7], [1, 2], Material::DeerPale);
    body.fill([3, 5], [5, 8], [0, 1], Material::DeerCoat);
    // Neck and head, pivot at the base of the neck. The head points forward (+z).
    let mut neck = Grid::new(6, 12, 11);
    neck.fill([1, 5], [0, 7], [0, 4], Material::DeerCoat);
    neck.fill([2, 4], [0, 6], [3, 4], Material::DeerPale);
    neck.fill([1, 5], [6, 10], [1, 9], Material::DeerCoat);
    neck.fill([2, 4], [6, 9], [9, 11], Material::DeerCoat);
    neck.fill([2, 4], [6, 8], [10, 11], Material::Hoof);
    neck.set(0, 8, 6, Material::Eye);
    neck.set(5, 8, 6, Material::Eye);
    // Large ears, the hind's.
    neck.fill([0, 1], [9, 12], [2, 4], Material::DeerCoat);
    neck.fill([5, 6], [9, 12], [2, 4], Material::DeerCoat);
    // Legs: slender, darker hooves. Pivot at the top.
    let leg = || {
        let mut g = Grid::new(2, LEG as usize, 2);
        g.fill([0, 2], [0, LEG as usize], [0, 2], Material::DeerCoat);
        g.fill([0, 2], [0, 1], [0, 2], Material::Hoof);
        g
    };
    let leg_part = |x: f32, z: f32| Part {
        grid: leg(),
        pivot: [1.0, LEG, 1.0],
        attach: Vec3::new(x, LEG, z),
    };
    [
        Part {
            grid: body,
            pivot: [4.0, 0.0, 11.5],
            attach: Vec3::new(0.0, LEG, 0.0),
        },
        Part {
            grid: neck,
            pivot: [3.0, 0.0, 2.0],
            attach: Vec3::new(0.0, LEG + 4.0, 9.5),
        },
        leg_part(-2.3, 8.5),
        leg_part(2.3, 8.5),
        leg_part(-2.3, -8.0),
        leg_part(2.3, -8.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing() -> DeerPose {
        DeerPose {
            position: Vec3::new(10.0, 5.0, 10.0),
            heading: 0.0,
            head: 1.0,
            lying: 0.0,
            stride: 0.0,
            speed: 0.0,
            size: 1.0,
            stamp: 0.0,
        }
    }

    #[test]
    fn a_hind_stands_on_its_hooves_and_lifts_its_head_above_its_back() {
        let view = DeerView::new();
        let t = view.transforms(&standing());
        let hoof = t[FRONT_LEFT] * glam::Vec4::new(0.0, -LEG * VOXEL, 0.0, 1.0);
        assert!((hoof.y - 5.0).abs() < 0.08, "{hoof:?}");
        let ear = t[NECK] * glam::Vec4::new(0.0, 12.0 * VOXEL, 3.0 * VOXEL, 1.0);
        let height = ear.y - 5.0;
        assert!((1.6..2.1).contains(&height), "{height}");
        // Grazing, the muzzle comes near the grass.
        let grazing = view.transforms(&DeerPose {
            head: -1.0,
            ..standing()
        });
        let muzzle = grazing[NECK] * glam::Vec4::new(0.0, 7.0 * VOXEL, 10.0 * VOXEL, 1.0);
        assert!(muzzle.y - 5.0 < 0.5, "{muzzle:?}");
    }
}
