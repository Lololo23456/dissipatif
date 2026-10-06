//! Red deer in the renderer: articulated micro-voxel parts (body, neck, head, tail, legs in two
//! segments), posed from what each deer does, with the real gaits of deer: a four-beat walk
//! (hind, fore, other hind, other fore), a diagonal trot, and bounds at the gallop, the back
//! flexing. Knees and hocks fold as a leg swings forward. A hind stands about 1.1 cells at the
//! shoulder; calves are drawn smaller. The naturalist in the shape of a deer is a stag, with
//! antlers.

use glam::{Mat4, Vec3};
use render::mesh::MeshData;
use render::mesher::mesh_materials;
use render::{PartId, PartInstance, Renderer};
use std::f32::consts::{PI, TAU};
use world::Material;

use crate::deer::{Activity, Deer};
use crate::naturalist::Grid;

/// Size of a voxel of a deer, in world cells.
const VOXEL: f32 = 0.05;
const LOOK_SEED: u64 = 0xdee7;
/// Leg segments, in voxels: upper (forearm, or thigh and gaskin) and lower (cannon, hoof).
const UPPER: f32 = 7.0;
const LOWER: f32 = 7.0;
const LEG: f32 = UPPER + LOWER;

const BODY: usize = 0;
const NECK: usize = 1;
const HEAD: usize = 2;
const TAIL: usize = 3;
/// Upper legs: fore left, fore right, hind left, hind right; lower legs follow in the same
/// order.
const UPPER_LEGS: usize = 4;
const LOWER_LEGS: usize = 8;
const ANTLERS: usize = 12;
const PARTS: usize = 13;

/// What the drawing needs of a deer.
#[derive(Clone, Copy, Debug)]
pub struct DeerPose {
    pub position: Vec3,
    pub heading: f32,
    /// −1 grazing, 0 level, 1 alert, more raised to the sky.
    pub head: f32,
    pub lying: f32,
    /// Gait cycle (radians: a full turn per stride) and ground speed.
    pub stride: f32,
    pub speed: f32,
    pub size: f32,
    /// Seconds left of a foot stamp.
    pub stamp: f32,
    /// Alarmed or fleeing: the tail goes up, showing the pale rump.
    pub alarmed: bool,
    /// In the air (a leap).
    pub airborne: bool,
    /// A stag, with antlers.
    pub stag: bool,
    /// Seconds, and a phase of its own (breathing, tail).
    pub time: f32,
    pub seed: f32,
}

impl DeerPose {
    pub fn of(d: &Deer, index: usize, time: f32) -> Self {
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
            alarmed: matches!(d.activity, Activity::Alarmed | Activity::Fleeing),
            airborne: false,
            stag: false,
            time,
            seed: index as f32 * 1.7,
        }
    }
}

struct Part {
    grid: Grid,
    pivot: [f32; 3],
}

pub struct DeerView {
    parts: Vec<Part>,
    ids: Vec<PartId>,
}

/// A gait: the share of the cycle each foot is on the ground, how far legs swing, and when
/// each leg starts its stance (fore left, fore right, hind left, hind right).
struct Gait {
    duty: f32,
    swing: f32,
    offsets: [f32; 4],
    /// Body rise and pitch over the cycle.
    bounce: f32,
    pitch: f32,
}

fn gait(speed: f32) -> Gait {
    if speed > 5.5 {
        // Bounding: fore pair, then hind pair, the back flexing.
        Gait {
            duty: 0.4,
            swing: 0.8,
            offsets: [0.0, 0.08, 0.5, 0.58],
            bounce: 0.12,
            pitch: 0.12,
        }
    } else if speed > 2.2 {
        // Trot: diagonal pairs together.
        Gait {
            duty: 0.5,
            swing: 0.55,
            offsets: [0.0, 0.5, 0.5, 0.0],
            bounce: 0.03,
            pitch: 0.0,
        }
    } else {
        // Walk: four beats, hind left, fore left, hind right, fore right.
        Gait {
            duty: 0.65,
            swing: 0.38 * (speed / 0.9).min(1.0),
            offsets: [0.25, 0.75, 0.0, 0.5],
            bounce: 0.012,
            pitch: 0.0,
        }
    }
}

/// Upper and lower angles of a leg at `phase` of its cycle (radians; positive swings the foot
/// back). On the ground it sweeps from front to back; in the air it comes forward, the joint
/// folding.
fn leg_angles(phase: f32, g: &Gait) -> (f32, f32) {
    let a = g.swing;
    if phase < g.duty {
        (-a + 2.0 * a * phase / g.duty, 0.0)
    } else {
        let t = (phase - g.duty) / (1.0 - g.duty);
        let s = t * t * (3.0 - 2.0 * t);
        let fold = (1.4 * a / 0.38).min(1.6) * (PI * t).sin();
        (a - 2.0 * a * s, fold)
    }
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
        let mut instances: Vec<Vec<PartInstance>> = vec![Vec::new(); PARTS];
        for pose in deer {
            for (k, transform) in transforms(pose) {
                instances[k].push(PartInstance::new(transform));
            }
        }
        for (k, list) in instances.iter().enumerate() {
            if let Some(&id) = self.ids.get(k) {
                renderer.set_part_instances(id, list);
            }
        }
    }
}

/// Model → world matrix of each part drawn for this deer.
fn transforms(d: &DeerPose) -> Vec<(usize, Mat4)> {
    let v = VOXEL;
    let g = gait(d.speed);
    let cycle = (d.stride / TAU).rem_euclid(1.0);
    let moving = d.speed > 0.05 && !d.airborne;
    let lying = d.lying.clamp(0.0, 1.0);
    let grazing = (-d.head).max(0.0);
    // The body rises and pitches with the gait, breathes when still.
    let (rise, pitch) = if moving {
        let wave = (TAU * cycle).sin();
        let rise = if g.pitch > 0.0 {
            g.bounce * wave.max(0.0)
        } else {
            g.bounce * (2.0 * TAU * cycle).cos()
        };
        (rise, g.pitch * (TAU * cycle).cos())
    } else {
        (0.004 * (d.time * 1.4 + d.seed).sin(), 0.0)
    };
    let root =
        Mat4::from_translation(d.position + Vec3::Y * ((rise - LEG * v * 0.82 * lying) * d.size))
            * Mat4::from_rotation_y(d.heading)
            * Mat4::from_scale(Vec3::splat(d.size))
            * Mat4::from_rotation_x(pitch + 0.06 * grazing + if d.airborne { -0.15 } else { 0.0 });
    let at = |x: f32, y: f32, z: f32| Mat4::from_translation(Vec3::new(x, y, z) * v);
    let body = root * at(0.0, LEG - 1.0, 0.0);
    let mut out = vec![(BODY, body)];

    // Neck and head: down to the grass, upright when alert, muzzle to the sky in the rite;
    // nodding at the walk.
    let nod = if moving && g.pitch == 0.0 {
        0.05 * (2.0 * TAU * cycle).sin()
    } else {
        0.0
    };
    let (neck_angle, head_angle) = if d.head < 0.0 {
        (2.1 * grazing, -0.8 * grazing)
    } else {
        let up = d.head.min(1.0);
        let sky = (d.head - 1.0).max(0.0);
        (-0.3 * up - 0.2 * sky, -0.1 * up - 0.55 * sky)
    };
    let neck = root * at(0.0, LEG + 7.0, 9.0) * Mat4::from_rotation_x(neck_angle + nod);
    let head = neck * at(0.0, 10.0, 6.0) * Mat4::from_rotation_x(head_angle - nod);
    out.push((NECK, neck));
    out.push((HEAD, head));
    if d.stag {
        out.push((ANTLERS, head * at(0.0, 1.5, -0.5)));
    }
    // Tail: hanging, swishing a little; raised when alarmed or running.
    let raised = ((d.speed - 4.0) / 3.0)
        .clamp(0.0, 1.0)
        .max(if d.alarmed { 1.0 } else { 0.0 });
    let swish = 0.15 * (d.time * 2.3 + d.seed).sin() * (1.0 - raised);
    out.push((
        TAIL,
        root * at(0.0, LEG + 8.5, -12.0)
            * Mat4::from_rotation_x(-1.6 * raised)
            * Mat4::from_rotation_z(swish),
    ));

    // Legs.
    let places = [(-2.2, 8.0), (2.2, 8.0), (-2.2, -8.5), (2.2, -8.5)];
    for (k, &(x, z)) in places.iter().enumerate() {
        let fore = k < 2;
        let (mut upper, mut lower) = if d.airborne {
            // A leap: forelegs reaching forward and folded, hind legs stretched back.
            if fore { (-0.9, 1.3) } else { (0.8, 0.2) }
        } else if moving {
            leg_angles((cycle + g.offsets[k]).rem_euclid(1.0), &g)
        } else {
            (0.0, 0.0)
        };
        // Standing hind legs angle at the hock.
        if !fore {
            upper -= 0.12;
            lower += 0.25;
        }
        // Lying: legs folded under the body.
        if fore {
            upper += 1.4 * lying;
            lower -= 2.5 * lying;
        } else {
            upper -= 1.3 * lying;
            lower += 2.4 * lying;
        }
        // The stamp: the right foreleg raised, then struck down.
        if k == 1 && d.stamp > 0.0 {
            let s = (PI * d.stamp / 0.6).sin();
            upper -= 0.6 * s;
            lower += 1.1 * s;
        }
        let top = root * at(x, LEG, z) * Mat4::from_rotation_x(upper);
        let knee = top * at(0.0, -UPPER, 0.0) * Mat4::from_rotation_x(lower);
        out.push((UPPER_LEGS + k, top));
        out.push((LOWER_LEGS + k, knee));
    }
    out
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

/// A part whose pivot is the voxel point `offset` of its grid.
fn part(size: [usize; 3], offset: Vec3, paint: impl Fn(Vec3) -> Option<Material>) -> Part {
    Part {
        grid: sculpt(size, offset, paint),
        pivot: offset.to_array(),
    }
}

/// Builds the parts, in the order of the part indices. Units are voxels; the deer faces +z.
fn parts() -> Vec<Part> {
    // Body: ellipses along its length, deep at the chest, rounder at the rump; a darker back,
    // a pale belly and the pale rump patch. Centred on x, rump at z = −12.5.
    let body = part([10, 13, 25], Vec3::new(5.0, 0.0, 12.5), |p| {
        let t = ((p.z + 12.5) / 25.0).clamp(0.0, 1.0);
        let half_height = 4.0 + 2.0 * (PI * (0.1 + 0.85 * t)).sin();
        let half_width = 0.7 * half_height;
        let centre = 6.6 - 0.6 * t;
        let (u, v) = (p.x / half_width, (p.y - centre) / half_height);
        if u * u + v * v > 1.0 {
            return None;
        }
        Some(if t < 0.06 && v > -0.6 && v < 0.75 {
            Material::DeerPale
        } else if v > 0.8 {
            Material::DeerDark
        } else if v < -0.72 {
            Material::DeerPale
        } else {
            Material::DeerCoat
        })
    });
    // Neck, leaning forward, a darker mane along its top; pivot at its base.
    let neck_top = Vec3::new(0.0, 10.0, 6.0);
    let neck = part([9, 13, 11], Vec3::new(4.5, 0.0, 3.0), |p| {
        let (d, t) = to_segment(p, Vec3::ZERO, neck_top);
        (d < 2.4 - 0.8 * t).then(|| {
            // Behind the neck's axis (towards the back), the mane.
            if p.cross(neck_top).x < -1.2 * neck_top.length() {
                Material::DeerDark
            } else {
                Material::DeerCoat
            }
        })
    });
    // Head: long and tapering to a dark muzzle, eyes on the sides, the hind's large ears;
    // pivot at the poll (top of the neck).
    let muzzle = Vec3::new(0.0, -2.5, 8.0);
    let head = part([10, 9, 13], Vec3::new(5.0, 3.5, 2.5), |p| {
        let (d, t) = to_segment(p, Vec3::new(0.0, 0.3, -0.8), muzzle);
        if d < 2.1 - 1.1 * t {
            if t > 0.9 {
                return Some(Material::Hoof);
            }
            if (0.28..0.42).contains(&t) && p.x.abs() > 1.3 && p.y > -0.5 {
                return Some(Material::Eye);
            }
            return Some(if t < 0.3 && p.y > 1.2 {
                Material::DeerDark
            } else {
                Material::DeerCoat
            });
        }
        for side in [-1.0f32, 1.0] {
            let (d, t) = to_segment(
                p,
                Vec3::new(1.5 * side, 1.0, -0.6),
                Vec3::new(4.2 * side, 4.6, -1.6),
            );
            if d < 1.15 - 0.45 * t {
                return Some(if t > 0.25 && p.z > -0.6 {
                    Material::DeerPale
                } else {
                    Material::DeerCoat
                });
            }
        }
        None
    });
    // Tail: short, dark above, pale beneath; pivot at its root, hanging.
    let tail = part([3, 5, 3], Vec3::new(1.5, 5.0, 1.5), |p| {
        (p.x.abs() < 1.2 && p.z.abs() < 1.2).then_some(if p.z < 0.0 {
            Material::DeerDark
        } else {
            Material::DeerPale
        })
    });
    // Legs. Upper: a forearm, or a heavier thigh; pivot at the top (overlapping the body).
    let upper = |hind: bool| {
        part(
            [5, UPPER as usize + 2, 5],
            Vec3::new(2.5, UPPER, 2.5),
            move |p| {
                // 0 at the knee, 1 at the top.
                let t = (p.y / UPPER + 1.0).clamp(0.0, 1.0);
                let reach = if hind { 1.0 + 1.3 * t } else { 0.9 + 0.8 * t };
                (p.x * p.x + p.z * p.z < reach * reach).then_some(Material::DeerCoat)
            },
        )
    };
    // Lower: a slender cannon and a dark, slightly wider hoof; pivot at the knee or hock.
    let lower = || {
        part(
            [4, LOWER as usize + 1, 4],
            Vec3::new(2.0, LOWER, 2.0),
            |p| {
                let hoof = p.y < -LOWER + 1.2;
                let reach = if hoof { 1.05 } else { 0.8 };
                (p.x.abs() < reach && p.z.abs() < reach).then_some(if hoof {
                    Material::Hoof
                } else {
                    Material::DeerCoat
                })
            },
        )
    };
    // Antlers (the stag): two beams sweeping up and back, each with a brow tine forward,
    // a tine at mid-height and a small crown.
    let antlers = part([16, 16, 12], Vec3::new(8.0, 0.0, 7.0), |p| {
        for side in [-1.0f32, 1.0] {
            let s = Vec3::new(side, 1.0, 1.0);
            let base = Vec3::new(1.2, 0.0, 0.0) * s;
            let mid = Vec3::new(3.6, 6.0, -2.5) * s;
            let top = Vec3::new(5.0, 12.5, -3.0) * s;
            let segments = [
                (base, mid, 0.75),
                (mid, top, 0.6),
                (
                    Vec3::new(1.6, 1.5, -0.3) * s,
                    Vec3::new(2.4, 3.0, 4.0) * s,
                    0.5,
                ),
                (
                    Vec3::new(3.4, 5.5, -2.2) * s,
                    Vec3::new(4.2, 7.5, 2.0) * s,
                    0.45,
                ),
                (top, Vec3::new(6.8, 14.0, -1.5) * s, 0.45),
                (top, Vec3::new(4.0, 14.5, -4.5) * s, 0.45),
            ];
            for (a, b, r) in segments {
                if to_segment(p, a, b).0 < r {
                    return Some(Material::DeadWood);
                }
            }
        }
        None
    });
    let mut list = vec![body, neck, head, tail];
    list.extend([upper(false), upper(false), upper(true), upper(true)]);
    list.extend([lower(), lower(), lower(), lower()]);
    list.push(antlers);
    debug_assert_eq!(list.len(), PARTS);
    list
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
            alarmed: false,
            airborne: false,
            stag: false,
            time: 0.0,
            seed: 0.0,
        }
    }

    fn find(pose: &DeerPose, part: usize) -> Mat4 {
        transforms(pose)
            .into_iter()
            .find(|&(k, _)| k == part)
            .map(|(_, m)| m)
            .expect("part drawn")
    }

    #[test]
    fn a_hind_stands_on_its_hooves_and_lifts_its_head_above_its_back() {
        let pose = standing();
        let hoof = find(&pose, LOWER_LEGS) * glam::Vec4::new(0.0, -LOWER * VOXEL, 0.0, 1.0);
        assert!((hoof.y - 5.0).abs() < 0.06, "{hoof:?}");
        let ear = find(&pose, HEAD) * glam::Vec4::new(4.2 * VOXEL, 4.6 * VOXEL, -1.6 * VOXEL, 1.0);
        let height = ear.y - 5.0;
        assert!((1.4..1.9).contains(&height), "{height}");
        // Grazing, the muzzle comes near the grass.
        let grazing = DeerPose {
            head: -1.0,
            ..standing()
        };
        let muzzle = find(&grazing, HEAD) * glam::Vec4::new(0.0, -2.5 * VOXEL, 8.0 * VOXEL, 1.0);
        assert!(muzzle.y - 5.0 < 0.35, "{muzzle:?}");
        // Only a stag carries antlers.
        assert!(transforms(&pose).iter().all(|&(k, _)| k != ANTLERS));
    }

    #[test]
    fn walking_feet_stay_near_the_ground_and_take_turns() {
        // Over a walking cycle, every hoof stays within a hand of the ground, and at any time
        // at least two are down.
        for step in 0..16 {
            let pose = DeerPose {
                speed: 1.3,
                stride: TAU * step as f32 / 16.0,
                ..standing()
            };
            let mut down = 0;
            for k in 0..4 {
                let hoof =
                    find(&pose, LOWER_LEGS + k) * glam::Vec4::new(0.0, -LOWER * VOXEL, 0.0, 1.0);
                let height = hoof.y - 5.0;
                assert!(
                    (-0.08..0.35).contains(&height),
                    "step {step} leg {k}: {height}"
                );
                // A straight leg sweeping under the hip lifts its hoof a little at either end.
                if height < 0.08 {
                    down += 1;
                }
            }
            assert!(down >= 2, "step {step}: {down} hooves down");
        }
    }
}
