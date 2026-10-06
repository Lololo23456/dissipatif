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
const VOXEL: f32 = 0.05;
const LOOK_SEED: u64 = 0xdee7;
/// Leg length, in voxels: the body's underside stands this high.
const LEG: f32 = 13.0;

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
        // Neck: as built at rest, down to the grass, upright when alert.
        let neck = if d.head < 0.0 {
            1.7 * grazing
        } else {
            -0.35 * d.head
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

/// A grid filled where `paint` says, voxel centres at `p - offset` (offset in voxels).
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

/// Distance from `p` to the segment [a, b], and how far along it (0 to 1) the nearest point is.
fn to_segment(p: Vec3, a: Vec3, b: Vec3) -> (f32, f32) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    ((a + ab * t).distance(p), t)
}

/// Builds the parts. Units are voxels; the deer faces +z.
fn parts() -> [Part; PARTS] {
    // Body: ellipses along its length, deep at the chest, rounder at the rump; a darker back,
    // a pale belly and the pale rump patch round a short tail. Centred on x, rump at z = 0.
    let body = sculpt([10, 13, 25], Vec3::new(5.0, 0.0, 0.0), |p| {
        let t = (p.z / 25.0).clamp(0.0, 1.0);
        let half_height = 4.0 + 2.0 * (std::f32::consts::PI * (0.1 + 0.85 * t)).sin();
        let half_width = 0.7 * half_height;
        let centre = 6.6 - 0.6 * t;
        let (u, v) = (p.x / half_width, (p.y - centre) / half_height);
        if u * u + v * v > 1.0 {
            // The tail: short, held down over the rump patch.
            let tail = p.z < 1.0 && p.x.abs() < 1.0 && (centre..centre + 3.0).contains(&p.y);
            return tail.then_some(Material::DeerDark);
        }
        Some(if p.z < 1.5 && v > -0.5 && v < 0.7 {
            Material::DeerPale
        } else if v > 0.8 {
            Material::DeerDark
        } else if v < -0.72 {
            Material::DeerPale
        } else {
            Material::DeerCoat
        })
    });
    // Neck and head, built leaning forward; pivot at the base of the neck.
    let neck_top = Vec3::new(0.0, 11.0, 7.0);
    let muzzle = Vec3::new(0.0, 9.0, 15.0);
    let neck = sculpt([9, 18, 20], Vec3::new(4.5, 0.0, 3.0), |p| {
        let (d, t) = to_segment(p, Vec3::ZERO, neck_top);
        if d < 2.3 - 0.7 * t {
            return Some(if p.y > 2.0 && p.z < neck_top.z * p.y / neck_top.y - 1.0 {
                Material::DeerDark
            } else {
                Material::DeerCoat
            });
        }
        let (d, t) = to_segment(p, neck_top + Vec3::new(0.0, 0.5, -1.0), muzzle);
        if d < 2.1 - 1.1 * t {
            if t > 0.92 {
                return Some(Material::Hoof);
            }
            if t > 0.3 && t < 0.42 && p.x.abs() > 1.2 && p.y > 10.0 {
                return Some(Material::Eye);
            }
            return Some(Material::DeerCoat);
        }
        // Large ears, the hind's, set back and spread.
        for side in [-1.0f32, 1.0] {
            let base = Vec3::new(1.6 * side, 12.0, 6.0);
            let tip = Vec3::new(3.6 * side, 15.5, 5.0);
            let (d, t) = to_segment(p, base, tip);
            if d < 1.0 - 0.3 * t {
                return Some(Material::DeerCoat);
            }
        }
        None
    });
    // Legs: a thigh, then a slender shank and a dark hoof. Pivot at the top.
    let leg = || {
        sculpt([4, LEG as usize + 2, 4], Vec3::ZERO, |p| {
            let thigh = p.y > LEG - 3.0;
            let reach = if thigh { 1.6 } else { 1.0 };
            let near = (p.x - 2.0).abs() < reach && (p.z - 2.0).abs() < reach;
            near.then_some(if p.y < 1.0 {
                Material::Hoof
            } else {
                Material::DeerCoat
            })
        })
    };
    let leg_part = |x: f32, z: f32| Part {
        grid: leg(),
        pivot: [2.0, LEG + 2.0, 2.0],
        attach: Vec3::new(x, LEG + 1.0, z),
    };
    [
        Part {
            grid: body,
            pivot: [5.0, 0.0, 12.5],
            attach: Vec3::new(0.0, LEG - 1.0, 0.0),
        },
        Part {
            grid: neck,
            pivot: [4.5, 0.0, 3.0],
            attach: Vec3::new(0.0, LEG + 7.0, 9.0),
        },
        leg_part(-2.2, 8.0),
        leg_part(2.2, 8.0),
        leg_part(-2.2, -8.5),
        leg_part(2.2, -8.5),
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
        let ear = t[NECK] * glam::Vec4::new(3.6 * VOXEL, 15.5 * VOXEL, 5.0 * VOXEL, 1.0);
        let height = ear.y - 5.0;
        assert!((1.5..2.0).contains(&height), "{height}");
        // Grazing, the muzzle comes near the grass.
        let grazing = view.transforms(&DeerPose {
            head: -1.0,
            ..standing()
        });
        let muzzle = grazing[NECK] * glam::Vec4::new(0.0, 9.0 * VOXEL, 15.0 * VOXEL, 1.0);
        assert!(muzzle.y - 5.0 < 0.5, "{muzzle:?}");
    }
}
