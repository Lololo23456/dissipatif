//! Marks left on the world, drawn by the shaders rather than as particles: ripples spreading
//! on the water (the surface itself tilts) and footprints pressed into sand and snow.

use bytemuck::{Pod, Zeroable};

pub const MAX_RIPPLES: usize = 16;
pub const MAX_PRINTS: usize = 48;

/// A ripple: where it started on the water and when.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ripple {
    pub x: f32,
    pub z: f32,
    /// Seconds since start (same clock as the renderer's `time`).
    pub start: f32,
    /// 0 to 1: a step makes a strong ring, swimming small ones.
    pub strength: f32,
}

/// A footprint: centre on the ground, direction the foot points to (radians, 0 = +z), when.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Print {
    pub x: f32,
    pub z: f32,
    pub facing: f32,
    pub start: f32,
}

/// WGSL side (`shaders/voxel.wgsl`, group 0 binding 5):
/// ```wgsl
/// struct Marks {
///     ripples: array<vec4<f32>, 16>,  // offset 0: x, z, start time, strength (0 = none)
///     prints: array<vec4<f32>, 48>,   // offset 256: x, z, facing, start time (< 0 = none)
/// }
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct MarksUniform {
    pub ripples: [[f32; 4]; MAX_RIPPLES],
    pub prints: [[f32; 4]; MAX_PRINTS],
}

impl MarksUniform {
    /// The most recent marks (the oldest are dropped if there are too many).
    pub fn new(ripples: &[Ripple], prints: &[Print]) -> Self {
        let mut uniform = Self {
            ripples: [[0.0; 4]; MAX_RIPPLES],
            prints: [[0.0, 0.0, 0.0, -1.0]; MAX_PRINTS],
        };
        let skip = ripples.len().saturating_sub(MAX_RIPPLES);
        for (slot, r) in uniform.ripples.iter_mut().zip(&ripples[skip..]) {
            *slot = [r.x, r.z, r.start, r.strength];
        }
        let skip = prints.len().saturating_sub(MAX_PRINTS);
        for (slot, p) in uniform.prints.iter_mut().zip(&prints[skip..]) {
            *slot = [p.x, p.z, p.facing, p.start];
        }
        uniform
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<MarksUniform>(), 1024);
        assert_eq!(std::mem::offset_of!(MarksUniform, prints), 256);
    }

    #[test]
    fn keeps_the_most_recent_marks() {
        let prints: Vec<Print> = (0..60)
            .map(|k| Print {
                start: k as f32,
                ..Print::default()
            })
            .collect();
        let u = MarksUniform::new(&[], &prints);
        assert_eq!(u.prints[0][3], 12.0);
        assert_eq!(u.prints[MAX_PRINTS - 1][3], 59.0);
        assert_eq!(u.ripples[0][3], 0.0);
    }
}
