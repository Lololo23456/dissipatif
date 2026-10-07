//! Effects the shaders draw over the world when the game asks for them: the marsh's living
//! medium seen on its water, a morning mist lying over the marsh, a slow wave of light passing
//! across the land, the owl's sight. They share one uniform (`EffectsUniform`, group 0 binding
//! 6) and two fields read texel by texel: the litter on the ground, one value per soil patch
//! (binding 7, see `Renderer::upload_litter`), and the marsh's medium (binding 8, see
//! `Renderer::upload_marsh`). A zero uniform and empty fields draw nothing.

use bytemuck::{Pod, Zeroable};

/// Side of the marsh's field texture, in texels. The medium's grid (at most this many texels
/// each way) is written in its corner, from texel (0, 0).
pub const MARSH_FIELD_SIZE: usize = 128;

/// WGSL side (`shaders/voxel.wgsl`, group 0 binding 6), every field a `vec4<f32>`:
/// ```wgsl
/// struct Effects {
///     marsh: vec4<f32>,             // offset 0: origin x, z (world cells), texels per cell,
///                                   //   strength (0 = not drawn)
///     marsh_size: vec4<f32>,        // offset 16: texels used along x, z; zw free
///     mist: vec4<f32>,              // offset 32: centre x, z, radius (cells), amount (0 = none)
///     wave: vec4<f32>,              // offset 48: origin x, z, distance travelled, width (cells)
///     wave_style: vec4<f32>,        // offset 64: colour (linear rgb), strength (0 = none)
///     vision: vec4<f32>,            // offset 80: night sight, dazzle (0 to 1); zw free
///     spare: array<vec4<f32>, 2>,   // offset 96: free for later effects
/// }                                 // size 128
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct EffectsUniform {
    /// The marsh's grid: world position (x, z) of the corner of texel (0, 0), texels per world
    /// cell, and how strongly the medium shows (0: not at all).
    pub marsh: [f32; 4],
    /// Texels of the grid used along x and z (the rest of the texture is unused); zw free.
    pub marsh_size: [f32; 4],
    /// A mist lying over a place: its centre (x, z), radius in cells, amount (0 to 1).
    pub mist: [f32; 4],
    /// A wave of light crossing the land from an origin (x, z): how far its front has gone and
    /// how wide it is, in cells.
    pub wave: [f32; 4],
    /// The wave's light: colour (linear RGB) and strength (0: no wave).
    pub wave_style: [f32; 4],
    /// The owl's sight: how much one sees in the dark, how dazzled one is (0 to 1); zw free.
    pub vision: [f32; 4],
    pub spare: [[f32; 4]; 2],
}

impl EffectsUniform {
    /// The marsh's medium shown with `strength`, its grid of `size` texels (at most
    /// `MARSH_FIELD_SIZE` each way) laid from world point `origin` (x, z) at `texels_per_cell`.
    pub fn with_marsh(
        mut self,
        origin: [f32; 2],
        texels_per_cell: f32,
        size: [usize; 2],
        strength: f32,
    ) -> Self {
        self.marsh = [origin[0], origin[1], texels_per_cell, strength];
        self.marsh_size = [size[0] as f32, size[1] as f32, 0.0, 0.0];
        self
    }

    /// Where world point `p` (x, z) falls in the marsh's grid, in texels (texel (i, j) spans
    /// [i, i + 1) × [j, j + 1)), on a world of `world_size` cells that closes on itself (0:
    /// with edges): the copy of `p` ahead of the grid's origin. Mirrors `marsh_texel` in
    /// voxel.wgsl.
    pub fn marsh_texel(&self, world_size: [f32; 2], p: [f32; 2]) -> [f32; 2] {
        let mut d = [p[0] - self.marsh[0], p[1] - self.marsh[1]];
        if world_size[0] > 0.0 {
            for a in 0..2 {
                d[a] -= world_size[a] * (d[a] / world_size[a]).floor();
            }
        }
        d.map(|v| v * self.marsh[2])
    }
}

/// The soil patch (column, row of the litter field, `patches` of them each way, covering the
/// world evenly) that world point `p` (x, z) lies in, on a world of `world_size` cells that
/// closes on itself. Mirrors `litter_at` in voxel.wgsl.
pub fn litter_patch(world_size: [f32; 2], patches: [usize; 2], p: [f32; 2]) -> [usize; 2] {
    [0, 1].map(|a| {
        let n = world_size[a];
        let wrapped = p[a] - n * (p[a] / n).floor();
        ((wrapped / n * patches[a] as f32) as usize).min(patches[a] - 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<EffectsUniform>(), 128);
        assert_eq!(std::mem::offset_of!(EffectsUniform, marsh_size), 16);
        assert_eq!(std::mem::offset_of!(EffectsUniform, mist), 32);
        assert_eq!(std::mem::offset_of!(EffectsUniform, wave), 48);
        assert_eq!(std::mem::offset_of!(EffectsUniform, wave_style), 64);
        assert_eq!(std::mem::offset_of!(EffectsUniform, vision), 80);
        assert_eq!(std::mem::offset_of!(EffectsUniform, spare), 96);
    }

    #[test]
    fn nothing_is_drawn_by_default() {
        let effects = EffectsUniform::default();
        assert_eq!(effects.marsh[3], 0.0, "marsh shown");
        assert_eq!(effects.mist[3], 0.0, "mist shown");
        assert_eq!(effects.wave_style[3], 0.0, "wave shown");
        assert_eq!(effects.vision, [0.0; 4], "owl's sight on");
    }

    #[test]
    fn the_marsh_grid_is_found_across_the_edge_of_the_world() {
        let world = [512.0, 512.0];
        let effects = EffectsUniform::default().with_marsh([500.0, 20.0], 2.0, [64, 40], 1.0);
        assert_eq!(effects.marsh_size, [64.0, 40.0, 0.0, 0.0]);
        // Its origin is texel (0, 0); a cell on, two texels on.
        assert_eq!(effects.marsh_texel(world, [500.0, 20.0]), [0.0, 0.0]);
        assert_eq!(effects.marsh_texel(world, [501.5, 21.0]), [3.0, 2.0]);
        // Past the edge of the world, the grid goes on from the other side.
        assert_eq!(effects.marsh_texel(world, [4.0, 30.0]), [32.0, 20.0]);
        // Just before its origin: far ahead, outside the grid.
        let before = effects.marsh_texel(world, [499.0, 20.0]);
        assert!(before[0] > 64.0, "{before:?}");
    }

    #[test]
    fn litter_patches_cover_the_world_and_close_on_themselves() {
        let world = [256.0, 256.0];
        let patches = [32, 32];
        assert_eq!(litter_patch(world, patches, [0.0, 0.0]), [0, 0]);
        assert_eq!(litter_patch(world, patches, [7.9, 8.0]), [0, 1]);
        assert_eq!(litter_patch(world, patches, [255.9, 100.0]), [31, 12]);
        assert_eq!(litter_patch(world, patches, [-1.0, 260.0]), [31, 0]);
    }
}
