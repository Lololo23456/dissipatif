//! The camera that follows the naturalist. One zoom sets how far it stands, how high it looks
//! from and how wide it sees: from a plunging view far above down to a person's height, just
//! behind them. It follows without jolts (a critically damped spring), turns smoothly under
//! the mouse, and never goes into the ground: the ground behind the eye pulls it in.

use glam::Vec3;
use render::OrbitCamera;
use world::{Material, World};

/// Zoom 0: at a person's height; 1: high above. Distance to the point looked at, in cells
/// (spread geometrically in between: each wheel notch moves by the same ratio).
const NEAR_DISTANCE: f32 = 4.5;
const FAR_DISTANCE: f32 = 110.0;
/// Angle above the horizontal, from nearly level to plunging.
const NEAR_PITCH: f32 = 6.0_f32.to_radians();
const FAR_PITCH: f32 = 58.0_f32.to_radians();
/// Vertical field of view: wide near the ground, narrow from above (close to an isometric
/// look, with just enough perspective to read depth).
const NEAR_FOV: f32 = 58.0_f32.to_radians();
const FAR_FOV: f32 = 36.0_f32.to_radians();
/// The point looked at, above the feet: over the head near the ground (the naturalist stands
/// in the lower part of the view, the land ahead shows above them), the chest from above.
const NEAR_LOOK: f32 = 2.1;
const FAR_LOOK: f32 = 1.2;
/// Zoom at the start: a plunging view of ~40°, the naturalist framed as before.
pub const DEFAULT_ZOOM: f32 = 0.42;
/// Zoom change per wheel notch.
const ZOOM_PER_NOTCH: f32 = 0.035;
/// How fast the zoom and the turn catch up with the wheel and the mouse (1/s).
const ZOOM_EASING: f32 = 10.0;
const YAW_EASING: f32 = 16.0;
/// Seconds the follow takes to catch up, sideways and vertically (slower: a jump or a step
/// up does not shake the view).
const FOLLOW_TIME: f32 = 0.12;
const FOLLOW_TIME_VERTICAL: f32 = 0.3;
/// The mouse tilts the view up to this much away from the zoom's pitch.
const MAX_TILT: f32 = 40.0_f32.to_radians();
/// The eye keeps this far from the ground, and comes back out at this rate (1/s) once the
/// ground behind it is gone.
const CLEARANCE: f32 = 0.35;
const RELEASE: f32 = 3.0;
/// Shortest distance the ground can pull the eye in to.
const MIN_DISTANCE: f32 = 0.8;
/// Step of the ray from the point looked at towards the eye.
const RAY_STEP: f32 = 0.1;

pub struct CameraRig {
    pub camera: OrbitCamera,
    zoom: f32,
    zoom_goal: f32,
    yaw_goal: f32,
    /// Added to the zoom's pitch by dragging the mouse up or down.
    tilt: f32,
    /// Velocity of the point looked at (the follow's spring).
    velocity: Vec3,
    /// How much shorter than the zoom's distance the ground keeps the eye.
    pulled_in: f32,
}

impl CameraRig {
    /// Looking at the one standing at `feet`, from the default zoom.
    pub fn new(feet: Vec3) -> Self {
        let mut rig = Self {
            camera: OrbitCamera::framing(feet, 1.0),
            zoom: DEFAULT_ZOOM,
            zoom_goal: DEFAULT_ZOOM,
            yaw_goal: 45.0_f32.to_radians(),
            tilt: 0.0,
            velocity: Vec3::ZERO,
            pulled_in: 0.0,
        };
        rig.camera.yaw = rig.yaw_goal;
        rig.snap(feet);
        rig
    }

    /// Wheel notches: positive comes closer (and lower).
    pub fn zoom_by(&mut self, notches: f32) {
        self.zoom_goal = (self.zoom_goal - notches * ZOOM_PER_NOTCH).clamp(0.0, 1.0);
    }

    /// Turns around the vertical axis and tilts the view (radians), as the mouse drags.
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw_goal += delta_yaw;
        self.tilt = (self.tilt + delta_pitch).clamp(-MAX_TILT, MAX_TILT);
    }

    /// At once, without easing (for captures).
    pub fn set_yaw(&mut self, yaw: f32) {
        self.yaw_goal = yaw;
        self.camera.yaw = yaw.rem_euclid(std::f32::consts::TAU);
    }

    /// The distance multiplied by `factor` from the default (< 1: closer), at once.
    pub fn set_zoom_factor(&mut self, factor: f32) {
        let ratio = (FAR_DISTANCE / NEAR_DISTANCE).ln();
        let default = distance(DEFAULT_ZOOM);
        let zoom = ((default * factor / NEAR_DISTANCE).ln() / ratio).clamp(0.0, 1.0);
        self.zoom = zoom;
        self.zoom_goal = zoom;
        self.shape();
    }

    /// This pitch (radians) at the current zoom, at once.
    pub fn set_pitch(&mut self, pitch: f32) {
        self.tilt = pitch - self.pitch();
        self.shape();
    }

    /// Looks at the one standing at `feet` at once, with no follow under way.
    pub fn snap(&mut self, feet: Vec3) {
        self.camera.target = feet + Vec3::Y * look_height(self.zoom);
        self.velocity = Vec3::ZERO;
        self.shape();
    }

    /// Advances by `dt` seconds, following the one whose feet are drawn at `feet`.
    pub fn update(&mut self, dt: f32, feet: Vec3, world: &World) {
        self.zoom += (self.zoom_goal - self.zoom) * (1.0 - (-ZOOM_EASING * dt).exp());
        // The turn eases towards the mouse; the goal is kept within half a turn of it.
        let tau = std::f32::consts::TAU;
        let ahead = (self.yaw_goal - self.camera.yaw + std::f32::consts::PI).rem_euclid(tau)
            - std::f32::consts::PI;
        let turned = ahead * (1.0 - (-YAW_EASING * dt).exp());
        self.camera.yaw = (self.camera.yaw + turned).rem_euclid(tau);
        self.yaw_goal = self.camera.yaw + ahead - turned;

        let goal = feet + Vec3::Y * look_height(self.zoom);
        // The planet closes on itself: when the naturalist crosses an edge, the camera goes
        // with them to the other side instead of sweeping back across the world.
        let target = &mut self.camera.target;
        [target.x, target.z] = world.nearest([goal.x, goal.z], [target.x, target.z]);
        for (axis, time) in [
            (0, FOLLOW_TIME),
            (1, FOLLOW_TIME_VERTICAL),
            (2, FOLLOW_TIME),
        ] {
            (target[axis], self.velocity[axis]) =
                smooth_damp(target[axis], self.velocity[axis], goal[axis], time, dt);
        }
        self.shape();
        // The ground behind the eye pulls it in at once; it goes back out slowly.
        let wanted = self.camera.distance;
        let free = clear_distance(world, self.camera.target, self.direction(), wanted);
        self.pulled_in = (self.pulled_in * (-RELEASE * dt).exp()).max(wanted - free);
        self.camera.distance = (wanted - self.pulled_in).max(MIN_DISTANCE.min(wanted));
    }

    /// Pitch, field of view and distance from the zoom and the tilt.
    fn shape(&mut self) {
        self.camera.pitch =
            (self.pitch() + self.tilt).clamp(OrbitCamera::MIN_PITCH, OrbitCamera::MAX_PITCH);
        self.camera.fov_y = NEAR_FOV + (FAR_FOV - NEAR_FOV) * self.zoom;
        self.camera.distance = distance(self.zoom);
    }

    /// The zoom's own pitch: it rises fast from the ground, then slowly.
    fn pitch(&self) -> f32 {
        NEAR_PITCH + (FAR_PITCH - NEAR_PITCH) * self.zoom.sqrt()
    }

    /// From the point looked at towards the eye.
    fn direction(&self) -> Vec3 {
        (self.camera.eye() - self.camera.target) / self.camera.distance
    }
}

fn distance(zoom: f32) -> f32 {
    NEAR_DISTANCE * (FAR_DISTANCE / NEAR_DISTANCE).powf(zoom)
}

fn look_height(zoom: f32) -> f32 {
    NEAR_LOOK + (FAR_LOOK - NEAR_LOOK) * zoom
}

/// A critically damped spring towards `goal`, reached in about `time` seconds: it starts and
/// stops without a jolt, never overshoots, and does not depend on the frame rate (the closed
/// form from "Game Programming Gems 4", 1.10). Returns the new value and velocity.
fn smooth_damp(current: f32, velocity: f32, goal: f32, time: f32, dt: f32) -> (f32, f32) {
    let omega = 2.0 / time;
    let x = omega * dt;
    let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let change = current - goal;
    let temp = (velocity + omega * change) * dt;
    (
        goal + (change + temp) * decay,
        (velocity - omega * temp) * decay,
    )
}

/// How far from `from` along `direction` the eye can stand, up to `wanted`, keeping
/// `CLEARANCE` from the ground on every side and below. Plants do not count: what hides the
/// view is cut away by the renderer.
fn clear_distance(world: &World, from: Vec3, direction: Vec3, wanted: f32) -> f32 {
    let steps = (wanted / RAY_STEP).ceil() as usize;
    for k in 1..=steps {
        let along = (k as f32 * RAY_STEP).min(wanted);
        let p = from + direction * along;
        let near_ground = [
            Vec3::ZERO,
            Vec3::new(0.0, -CLEARANCE, 0.0),
            Vec3::new(CLEARANCE, 0.0, 0.0),
            Vec3::new(-CLEARANCE, 0.0, 0.0),
            Vec3::new(0.0, 0.0, CLEARANCE),
            Vec3::new(0.0, 0.0, -CLEARANCE),
        ]
        .iter()
        .any(|&o| ground_at(world, p + o));
        if near_ground {
            return (along - RAY_STEP).max(0.0);
        }
    }
    wanted
}

/// Whether `p` lies in the ground (not in a plant).
fn ground_at(world: &World, p: Vec3) -> bool {
    let y = p.y.floor();
    if y < 0.0 {
        return true;
    }
    if y as usize >= world.dims().ny {
        return false;
    }
    let (x, z) = world.column(p.x.floor() as i64, p.z.floor() as i64);
    let material: Material = world.block(x, y as usize, z);
    material.is_solid() && !material.is_plant()
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::WorldConfig;

    #[test]
    fn zooming_in_lowers_the_view_and_widens_it() {
        let mut rig = CameraRig::new(Vec3::ZERO);
        let (far_pitch, far_fov) = (rig.camera.pitch, rig.camera.fov_y);
        rig.set_zoom_factor(0.01);
        assert!(
            rig.camera.pitch < 10.0_f32.to_radians(),
            "{}",
            rig.camera.pitch
        );
        assert!(rig.camera.pitch < far_pitch && rig.camera.fov_y > far_fov);
        assert!((rig.camera.distance - NEAR_DISTANCE).abs() < 1e-3);
    }

    #[test]
    fn the_default_view_plunges_at_about_forty_degrees() {
        let rig = CameraRig::new(Vec3::ZERO);
        let degrees = rig.camera.pitch.to_degrees();
        assert!((36.0..44.0).contains(&degrees), "{degrees}");
    }

    #[test]
    fn the_follow_catches_up_without_overshooting() {
        let world = World::generate(WorldConfig::small(1));
        let mut rig = CameraRig::new(Vec3::new(100.0, 40.0, 100.0));
        let feet = Vec3::new(104.0, 40.0, 100.0);
        let mut last = rig.camera.target.x;
        for _ in 0..120 {
            rig.update(1.0 / 60.0, feet, &world);
            let x = rig.camera.target.x;
            assert!(x >= last - 1e-4 && x <= feet.x + 1e-3, "{x}");
            last = x;
        }
        assert!((last - feet.x).abs() < 0.05, "{last}");
    }

    #[test]
    fn the_eye_stays_out_of_the_ground() {
        let world = World::generate(WorldConfig::small(1));
        let dims = world.dims();
        // Close and low, in turn all around the naturalist: the eye never ends in the ground.
        let (x, z) = (dims.nx / 2, dims.nz / 2);
        let feet = Vec3::new(
            x as f32 + 0.5,
            world.ground_top(x, z) as f32,
            z as f32 + 0.5,
        );
        let mut rig = CameraRig::new(feet);
        rig.set_zoom_factor(0.25);
        rig.set_pitch(OrbitCamera::MIN_PITCH);
        for turn in 0..16 {
            rig.set_yaw(turn as f32 * std::f32::consts::TAU / 16.0);
            for _ in 0..30 {
                rig.update(1.0 / 60.0, feet, &world);
            }
            assert!(!ground_at(&world, rig.camera.eye()), "turn {turn}");
        }
    }

    #[test]
    fn a_wall_behind_pulls_the_eye_in() {
        let world = World::generate(WorldConfig::small(1));
        let dims = world.dims();
        let (x, z) = (0..dims.nx * dims.nz)
            .map(|i| (i % dims.nx, i / dims.nx))
            .find(|&(x, z)| world.ground_top(x, z) >= 12)
            .expect("no high ground");
        let top = world.ground_top(x, z) as f32;
        let (cx, cz) = (x as f32 + 0.5, z as f32 + 0.5);
        // From deep in the ground, up towards the surface: in the ground all the way.
        let deep = Vec3::new(cx, top - 10.0, cz);
        assert_eq!(clear_distance(&world, deep, Vec3::Y, 5.0), 0.0);
        // From above the ground, down: stopped short of it, keeping clear of it.
        let above = Vec3::new(cx, top + 6.0, cz);
        let free = clear_distance(&world, above, -Vec3::Y, 20.0);
        assert!(free > 5.0 && free < 6.0 - CLEARANCE + 1e-3, "{free}");
        // High above everything, nothing is in the way.
        let sky = Vec3::new(cx, dims.ny as f32 + 1.0, cz);
        assert_eq!(clear_distance(&world, sky, Vec3::Y, 10.0), 10.0);
    }
}
