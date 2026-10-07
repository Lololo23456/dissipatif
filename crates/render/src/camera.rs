//! Plunging orbit camera (Minecraft Dungeons style) and its GPU-side uniform.
//!
//! World convention: right-handed, Y up. Grid axis `y` of the simulation is the vertical one.

use glam::camera::rh::{proj::directx, view::look_at_mat4};
use glam::{Mat4, Vec3};

/// Camera orbiting around a target point, always looking down at it.
#[derive(Clone, Copy, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    /// Rotation around the vertical axis, radians.
    pub yaw: f32,
    /// Angle above the horizontal plane, radians. Clamped to stay a plunging view.
    pub pitch: f32,
    /// Distance from the target.
    pub distance: f32,
    /// Vertical field of view, radians. Narrow on purpose: close to an isometric look,
    /// with just enough perspective to read depth.
    pub fov_y: f32,
}

/// The ground is drawn at least this far from the point looked at, and at most this far (the
/// haze is complete there, see `OrbitCamera::reach`).
pub const MIN_REACH: f32 = 64.0;
pub const MAX_REACH: f32 = 160.0;
/// The ground is taken this far below the point looked at when measuring the reach.
const REACH_BELOW: f32 = 10.0;

impl OrbitCamera {
    /// From level with the target (a view at a person's height) to nearly straight down.
    pub const MIN_PITCH: f32 = 2.0_f32.to_radians();
    pub const MAX_PITCH: f32 = 85.0_f32.to_radians();
    pub const MIN_DISTANCE: f32 = 1.0;
    pub const MAX_DISTANCE: f32 = 500.0;

    /// Camera framing a sphere of radius `radius` centred on `target`.
    pub fn framing(target: Vec3, radius: f32) -> Self {
        let fov_y = 30.0_f32.to_radians();
        Self {
            target,
            yaw: 45.0_f32.to_radians(),
            pitch: 40.0_f32.to_radians(),
            // The sphere fits in the vertical field of view when sin(fov/2) = radius / distance.
            distance: (radius / (fov_y / 2.0).sin()).clamp(Self::MIN_DISTANCE, Self::MAX_DISTANCE),
            fov_y,
        }
    }

    /// Rotates the camera by the given angles (radians), keeping the plunging view.
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw = (self.yaw + delta_yaw).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + delta_pitch).clamp(Self::MIN_PITCH, Self::MAX_PITCH);
    }

    /// Multiplies the distance by `factor` (< 1 moves closer).
    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(Self::MIN_DISTANCE, Self::MAX_DISTANCE);
    }

    /// Position of the eye in world space.
    pub fn eye(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let direction = Vec3::new(cos_pitch * sin_yaw, sin_pitch, cos_pitch * cos_yaw);
        self.target + self.distance * direction
    }

    /// World → clip space matrix for a viewport of the given aspect ratio (width / height).
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = look_at_mat4(self.eye(), self.target, Vec3::Y);
        // WebGPU clip space: depth in [0, 1], Y up, which is the "directx" convention in glam.
        // Near and far planes bracket what can be seen; their ratio sets depth precision. Seen
        // from low, the view runs to the horizon: the far plane goes past the farthest ground
        // drawn.
        let near = (self.distance * 0.05).max(0.1);
        let far = self.distance * 4.0 + 2.0 * MAX_REACH;
        let proj = directx::perspective(self.fov_y, aspect, near, far);
        proj * view
    }

    /// How far from the target the ground must be drawn to fill the view, in [`MIN_REACH`,
    /// `MAX_REACH`]: the farthest point where the rays through the top corners of the screen
    /// meet a plane below the target (lower than the ground around, so slopes going down are
    /// covered), with a margin. A ray at or above the horizon sees as far as is drawn.
    pub fn reach(&self, aspect: f32) -> f32 {
        let inverse = self.view_proj(aspect).inverse();
        let eye = self.eye();
        let plane = self.target.y - REACH_BELOW;
        let mut reach: f32 = 0.0;
        for x in [-1.0, 1.0] {
            let far = inverse.project_point3(Vec3::new(x, 1.0, 1.0));
            let ray = (far - eye).normalize();
            if ray.y > -0.02 {
                return MAX_REACH;
            }
            let hit = eye + ray * ((plane - eye.y) / ray.y);
            let offset = glam::Vec2::new(hit.x - self.target.x, hit.z - self.target.z);
            reach = reach.max(offset.length());
        }
        (reach * 1.2 + 16.0).clamp(MIN_REACH, MAX_REACH)
    }
}

/// Camera data read by the shaders.
///
/// WGSL side (`shaders/voxel.wgsl`):
/// ```wgsl
/// struct Camera {
///     view_proj: mat4x4<f32>,  // offset 0, size 64
///     eye: vec4<f32>,          // offset 64: eye position, w = distance to the target
///     time: vec4<f32>,         // offset 80: x = seconds, y = mist, z = wetness, w = mist floor
///     light_view_proj: mat4x4<f32>,  // offset 96, size 64: world → sun's shadow map
///     world: vec4<f32>,        // offset 160: size of the world in x, z (0: it has edges);
///                              // zw: the point looked at (x, z)
///     inverse_view_proj: mat4x4<f32>,  // offset 176, size 64: clip space → world
///     focus: vec4<f32>,        // offset 240: feet of the one followed (xyz), w = reach
/// }
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    /// Column-major, as WGSL expects.
    pub view_proj: [[f32; 4]; 4],
    /// Eye position in xyz, distance to the target in w (used by the haze).
    pub eye: [f32; 4],
    /// x: seconds since start, for animations (wind, ripples). y: mist (0 to 1), z: how wet
    /// the ground is (0 to 1), w: height around which mist lies (cells). See `Weather` in
    /// the renderer.
    pub time: [f32; 4],
    /// World → clip space of the sun, for shadows (see `light_view_proj`).
    pub light_view_proj: [[f32; 4]; 4],
    /// Size of the world in x and z when it closes on itself (a planet): what lies across its
    /// edges is drawn next to the eye. 0 when it has edges.
    pub world: [f32; 4],
    /// Clip space → world: the sky turns each pixel back into the direction it looks along.
    pub inverse_view_proj: [[f32; 4]; 4],
    /// Feet of the one the camera follows, in xyz: what stands between them and the eye is
    /// cut away. w: how far from the point looked at the ground is drawn (the haze is
    /// complete there).
    pub focus: [f32; 4],
}

impl CameraUniform {
    /// `weather`: mist, wetness, mist floor height (see `time`). `focus`: feet of the one
    /// followed, `reach`: see `OrbitCamera::reach`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        camera: &OrbitCamera,
        aspect: f32,
        time: f32,
        weather: [f32; 3],
        light_view_proj: Mat4,
        world: [f32; 2],
        focus: Vec3,
        reach: f32,
    ) -> Self {
        let view_proj = camera.view_proj(aspect);
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            eye: camera.eye().extend(camera.distance).to_array(),
            time: [time, weather[0], weather[1], weather[2]],
            light_view_proj: light_view_proj.to_cols_array_2d(),
            world: [world[0], world[1], camera.target.x, camera.target.z],
            inverse_view_proj: view_proj.inverse().to_cols_array_2d(),
            focus: focus.extend(reach).to_array(),
        }
    }
}

/// The sun's view of the scene, for its shadow map: an orthographic projection (sun rays are
/// parallel, so no perspective) looking along `-towards_sun`, fitted tightly around the box
/// [`min`, `max`] so that the whole scene fits in the map and no texel is wasted.
pub fn light_view_proj(towards_sun: Vec3, min: Vec3, max: Vec3) -> Mat4 {
    let direction = towards_sun.normalize();
    let centre = (min + max) * 0.5;
    let radius = (max - min).length() * 0.5;
    // Any up vector works as long as it is not parallel to the light.
    let up = if direction.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let view = look_at_mat4(centre + direction * (2.0 * radius), centre, up);
    // Bounds of the box seen from the sun.
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for corner in 0..8 {
        let p = Vec3::new(
            if corner & 1 == 0 { min.x } else { max.x },
            if corner & 2 == 0 { min.y } else { max.y },
            if corner & 4 == 0 { min.z } else { max.z },
        );
        let v = view.transform_point3(p);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    // Right-handed view space looks down −z: the nearest points have the largest z.
    let proj = directx::orthographic(lo.x, hi.x, lo.y, hi.y, -hi.z, -lo.z);
    proj * view
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec4;

    #[test]
    fn light_projection_contains_the_whole_scene() {
        let (min, max) = (Vec3::ZERO, Vec3::new(256.0, 64.0, 256.0));
        let light = light_view_proj(Vec3::new(0.45, 0.8, -0.4), min, max);
        for corner in 0..8 {
            let p = Vec3::new(
                if corner & 1 == 0 { min.x } else { max.x },
                if corner & 2 == 0 { min.y } else { max.y },
                if corner & 4 == 0 { min.z } else { max.z },
            );
            let clip = light * p.extend(1.0);
            let ndc = clip / clip.w;
            assert!(
                ndc.x.abs() <= 1.0 + 1e-4 && ndc.y.abs() <= 1.0 + 1e-4,
                "{ndc:?}"
            );
            assert!(ndc.z >= -1e-4 && ndc.z <= 1.0 + 1e-4, "{ndc:?}");
        }
    }

    #[test]
    fn closer_to_the_sun_means_smaller_depth() {
        let light = light_view_proj(Vec3::Y, Vec3::ZERO, Vec3::splat(10.0));
        let depth = |y: f32| {
            let clip = light * Vec4::new(5.0, y, 5.0, 1.0);
            clip.z / clip.w
        };
        assert!(depth(9.0) < depth(1.0));
    }

    #[test]
    fn uniform_layout_matches_wgsl() {
        // Uniform structs must be a multiple of 16 bytes.
        assert_eq!(std::mem::size_of::<CameraUniform>(), 256);
        assert_eq!(std::mem::offset_of!(CameraUniform, world), 160);
        assert_eq!(std::mem::offset_of!(CameraUniform, inverse_view_proj), 176);
        assert_eq!(std::mem::offset_of!(CameraUniform, focus), 240);
        assert_eq!(std::mem::offset_of!(CameraUniform, light_view_proj), 96);
        assert_eq!(std::mem::offset_of!(CameraUniform, eye), 64);
        assert_eq!(std::mem::offset_of!(CameraUniform, time), 80);
    }

    #[test]
    fn target_projects_to_screen_centre_inside_depth_range() {
        let camera = OrbitCamera::framing(Vec3::new(24.0, 24.0, 24.0), 40.0);
        let clip = camera.view_proj(16.0 / 9.0) * camera.target.extend(1.0);
        let ndc = clip / clip.w;
        assert!(ndc.x.abs() < 1e-4 && ndc.y.abs() < 1e-4, "{ndc:?}");
        assert!(ndc.z > 0.0 && ndc.z < 1.0, "{ndc:?}");
    }

    #[test]
    fn point_above_target_appears_higher_on_screen() {
        let camera = OrbitCamera::framing(Vec3::ZERO, 10.0);
        let vp = camera.view_proj(1.0);
        let above = vp * Vec4::new(0.0, 5.0, 0.0, 1.0);
        assert!(above.y / above.w > 0.0);
    }

    #[test]
    fn reach_grows_as_the_view_lowers() {
        let mut camera = OrbitCamera::framing(Vec3::new(0.0, 20.0, 0.0), 7.0);
        camera.pitch = 60.0_f32.to_radians();
        let high = camera.reach(1.6);
        camera.pitch = 20.0_f32.to_radians();
        let low = camera.reach(1.6);
        assert!(high < low, "{high} {low}");
        // Level with the target: the view runs to the horizon.
        camera.pitch = OrbitCamera::MIN_PITCH;
        assert_eq!(camera.reach(1.6), MAX_REACH);
    }

    #[test]
    fn the_far_plane_holds_the_farthest_ground_drawn() {
        let mut camera = OrbitCamera::framing(Vec3::ZERO, 7.0);
        camera.distance = 3.0;
        camera.pitch = OrbitCamera::MIN_PITCH;
        // A point at the edge of what is drawn, straight ahead.
        let ahead = -Vec3::new(camera.yaw.sin(), 0.0, camera.yaw.cos()) * MAX_REACH;
        let clip = camera.view_proj(1.6) * ahead.extend(1.0);
        let depth = clip.z / clip.w;
        assert!(depth > 0.0 && depth < 1.0, "{depth}");
    }

    #[test]
    fn pitch_stays_plunging() {
        let mut camera = OrbitCamera::framing(Vec3::ZERO, 10.0);
        camera.orbit(0.0, -10.0);
        assert!(camera.eye().y > 0.0);
        camera.orbit(0.0, 10.0);
        assert!(camera.pitch < std::f32::consts::FRAC_PI_2);
    }
}
