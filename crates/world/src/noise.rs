//! Deterministic noise: everything procedural in the world comes from here.
//!
//! No global state: every value is a hash of (seed, position), so the result does not depend
//! on the order in which positions are visited, and the same seed always gives the same world.

/// SplitMix64 mixing step: every output bit depends on every input bit.
#[inline]
fn mix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Hash of an integer position, as a 64-bit value.
#[inline]
pub fn hash(seed: u64, coords: &[i64]) -> u64 {
    coords.iter().fold(mix(seed), |h, &c| mix(h ^ c as u64))
}

/// Hash of an integer position, as a float in [0, 1).
#[inline]
pub fn hash_unit(seed: u64, coords: &[i64]) -> f32 {
    // Keep the 24 high bits: exactly representable in an f32.
    (hash(seed, coords) >> 40) as f32 / (1u64 << 24) as f32
}

/// Smooth 2D noise in [0, 1]: random values on a lattice of spacing `scale` cells, blended
/// between lattice points with smoothstep weights (no visible creases along lattice lines).
pub fn value(x: f32, z: f32, scale: f32, seed: u64) -> f32 {
    let (qx, qz) = (x / scale, z / scale);
    let (cx, cz) = (qx.floor(), qz.floor());
    let smooth = |f: f32| f * f * (3.0 - 2.0 * f);
    let (tx, tz) = (smooth(qx - cx), smooth(qz - cz));
    let (ix, iz) = (cx as i64, cz as i64);
    let corner = |dx: i64, dz: i64| hash_unit(seed, &[ix + dx, iz + dz]);
    let top = corner(0, 0) * (1.0 - tx) + corner(1, 0) * tx;
    let bottom = corner(0, 1) * (1.0 - tx) + corner(1, 1) * tx;
    top * (1.0 - tz) + bottom * tz
}

/// Fractal noise in [0, 1]: `octaves` layers of `value`, each twice finer and half as strong.
/// Large shapes from the first octave, details from the next ones.
pub fn fbm(x: f32, z: f32, scale: f32, octaves: u32, seed: u64) -> f32 {
    let (mut sum, mut weight, mut total, mut s) = (0.0, 1.0, 0.0, scale);
    for octave in 0..octaves {
        sum += weight * value(x, z, s, seed.wrapping_add(octave as u64 * 1013));
        total += weight;
        weight *= 0.5;
        s *= 0.5;
    }
    sum / total
}

/// Ridged fractal noise in [0, 1]: each octave folded around its middle value
/// (1 − |2n − 1|), so the lines where it crossed the middle become sharp crests and the rest
/// valleys between them. The classic shape of mountain ranges.
pub fn ridged(x: f32, z: f32, scale: f32, octaves: u32, seed: u64) -> f32 {
    let (mut sum, mut weight, mut total, mut s) = (0.0, 1.0, 0.0, scale);
    for octave in 0..octaves {
        let n = value(x, z, s, seed.wrapping_add(octave as u64 * 7919));
        let crest = 1.0 - (2.0 * n - 1.0).abs();
        sum += weight * crest * crest;
        total += weight;
        weight *= 0.5;
        s *= 0.5;
    }
    sum / total
}

/// Smoothstep from `edge0` to `edge1`: 0 below, 1 above, a smooth S in between.
#[inline]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noises_stay_in_unit_interval() {
        for i in 0..2000 {
            let (x, z) = (i as f32 * 0.37, i as f32 * 1.13);
            for v in [
                value(x, z, 8.0, 1),
                fbm(x, z, 32.0, 5, 2),
                ridged(x, z, 32.0, 4, 3),
            ] {
                assert!((0.0..=1.0).contains(&v), "{v}");
            }
        }
    }

    #[test]
    fn noise_is_deterministic_and_seed_dependent() {
        assert_eq!(fbm(10.5, 3.25, 16.0, 4, 9), fbm(10.5, 3.25, 16.0, 4, 9));
        assert_ne!(fbm(10.5, 3.25, 16.0, 4, 9), fbm(10.5, 3.25, 16.0, 4, 10));
    }

    #[test]
    fn noise_is_continuous() {
        // Two points a hundredth of a cell apart have almost the same value.
        let (a, b) = (value(5.0, 5.0, 8.0, 4), value(5.01, 5.0, 8.0, 4));
        assert!((a - b).abs() < 0.01);
    }
}
