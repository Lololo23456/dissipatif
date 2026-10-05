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

impl OrbitCamera {
    const MIN_PITCH: f32 = 15.0_f32.to_radians();
    const MAX_PITCH: f32 = 85.0_f32.to_radians();
    const MIN_DISTANCE: f32 = 10.0;
    const MAX_DISTANCE: f32 = 500.0;

    /// Camera framing a sphere of radius `radius` centred on `target`.
    pub fn framing(target: Vec3, radius: f32) -> Self {
        let fov_y = 30.0_f32.to_radians();
        Self {
            target,
            yaw: 45.0_f32.to_radians(),
            pitch: 50.0_f32.to_radians(),
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
        // Near and far planes bracket the target generously; their ratio sets depth precision.
        let near = (self.distance * 0.1).max(0.1);
        let far = self.distance * 4.0;
        let proj = directx::perspective(self.fov_y, aspect, near, far);
        proj * view
    }
}

/// Camera data read by the shaders.
///
/// WGSL side (`shaders/voxel.wgsl`):
/// ```wgsl
/// struct Camera {
///     view_proj: mat4x4<f32>,  // offset 0, size 64
///     eye: vec4<f32>,          // offset 64: eye position, w = distance to the target
///     time: vec4<f32>,         // offset 80: x = seconds since start, yzw unused
/// }
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    /// Column-major, as WGSL expects.
    pub view_proj: [[f32; 4]; 4],
    /// Eye position in xyz, distance to the target in w (used by the haze).
    pub eye: [f32; 4],
    /// x: seconds since start, for animations (water ripples). yzw unused.
    pub time: [f32; 4],
}

impl CameraUniform {
    pub fn new(camera: &OrbitCamera, aspect: f32, time: f32) -> Self {
        Self {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            eye: camera.eye().extend(camera.distance).to_array(),
            time: [time, 0.0, 0.0, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec4;

    #[test]
    fn uniform_layout_matches_wgsl() {
        // Uniform structs must be a multiple of 16 bytes.
        assert_eq!(std::mem::size_of::<CameraUniform>(), 96);
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
    fn pitch_stays_plunging() {
        let mut camera = OrbitCamera::framing(Vec3::ZERO, 10.0);
        camera.orbit(0.0, -10.0);
        assert!(camera.eye().y > 0.0);
        camera.orbit(0.0, 10.0);
        assert!(camera.pitch < std::f32::consts::FRAC_PI_2);
    }
}
