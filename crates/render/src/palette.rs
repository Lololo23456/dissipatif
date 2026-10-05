//! Toutes les palettes et fonctions de couleur du jeu.
//! L'apparence de chaque voxel est une fonction de son état local : rien n'est dessiné à la main.
//! La lisibilité ne repose jamais sur la seule teinte (luminance et mouvement portent aussi l'information).

/// Number of colour stops in a palette sent to the GPU.
pub const STOP_COUNT: usize = 5;

/// A sequential palette: colour stops spread evenly over [0, 1], linear RGB.
pub type Palette = [[f32; 3]; STOP_COUNT];

// Art direction: warm and flat, "golden hour" (reference: Firewatch). Warm sunlight, cool
// shadows lit by the sky, distance fading into a peach haze.

/// Default palette for a concentration: plum → brick → burnt orange → amber → pale gold.
///
/// Luminance strictly increases along the stops, so the value stays readable in greyscale
/// and for every kind of colour blindness. But it only spans perceived lightness L* ≈ 36 → 75
/// (out of 0 → 100): the rest of the luminance range is left to lighting, which shows the shape.
pub fn concentration() -> Palette {
    [
        srgb_hex(0x7d4066), // L* 36.0
        srgb_hex(0xa34d4c), // L* 43.7
        srgb_hex(0xc0673d), // L* 53.2
        srgb_hex(0xcf8c48), // L* 63.7
        srgb_hex(0xd9b26e), // L* 74.6
    ]
}

/// Inert ground: umber → brown → ochre → olive → moss (deep earth to grassy surface).
///
/// Darker (L* ≈ 26 → 55) and less saturated than `concentration`: the world stays a calm
/// backdrop and living matter stands out by luminance, not only by hue.
pub fn earth() -> Palette {
    [
        srgb_hex(0x4a3a34), // L* 26.0
        srgb_hex(0x5c4639), // L* 31.7
        srgb_hex(0x6c5840), // L* 38.8
        srgb_hex(0x77704f), // L* 47.1
        srgb_hex(0x82875e), // L* 55.0
    ]
}

/// Light and air of the scene.
#[derive(Clone, Copy, Debug)]
pub struct Atmosphere {
    /// Direction towards the sun (normalised when converted to a uniform).
    pub sun_direction: [f32; 3],
    /// Colour and strength of direct sunlight, linear RGB.
    pub sun_color: [f32; 3],
    /// Ambient light coming from above (the sky): what lights the shadows.
    pub sky_color: [f32; 3],
    /// Ambient light bounced from below (the ground).
    pub ground_color: [f32; 3],
    /// Colour of the haze, also the background.
    pub fog_color: [f32; 3],
    /// Haze starts this far beyond the camera target, in cells (negative = in front of it)…
    pub fog_start: f32,
    /// …and reaches `fog_max` this far beyond it.
    pub fog_end: f32,
    /// Largest share of the haze colour in a pixel, in [0, 1].
    pub fog_max: f32,
}

/// Late-afternoon light: low warm sun, blue-violet sky light in the shadows, peach haze.
pub fn golden_hour() -> Atmosphere {
    Atmosphere {
        sun_direction: [0.7, 0.55, -0.45],
        sun_color: srgb_hex(0xffd2a1),
        sky_color: srgb_hex(0x6f7fa8),
        ground_color: srgb_hex(0x7a5440),
        fog_color: srgb_hex(0xe7a873),
        fog_start: -10.0,
        fog_end: 50.0,
        fog_max: 0.55,
    }
}

/// Light and haze, as read by the shaders.
///
/// WGSL side (`shaders/voxel.wgsl`), every field a `vec4<f32>`, `w` unused unless stated:
/// ```wgsl
/// struct Atmosphere {
///     sun_direction: vec4<f32>,  // offset 0
///     sun_color: vec4<f32>,      // offset 16
///     sky_color: vec4<f32>,      // offset 32
///     ground_color: vec4<f32>,   // offset 48
///     fog_color: vec4<f32>,      // offset 64
///     fog: vec4<f32>,            // offset 80: start, end, max, unused
/// }
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AtmosphereUniform {
    pub sun_direction: [f32; 4],
    pub sun_color: [f32; 4],
    pub sky_color: [f32; 4],
    pub ground_color: [f32; 4],
    pub fog_color: [f32; 4],
    pub fog: [f32; 4],
}

impl AtmosphereUniform {
    pub fn new(atmosphere: &Atmosphere) -> Self {
        let [x, y, z] = atmosphere.sun_direction;
        let length = (x * x + y * y + z * z).sqrt();
        let rgb = |[r, g, b]: [f32; 3]| [r, g, b, 1.0];
        Self {
            sun_direction: [x / length, y / length, z / length, 0.0],
            sun_color: rgb(atmosphere.sun_color),
            sky_color: rgb(atmosphere.sky_color),
            ground_color: rgb(atmosphere.ground_color),
            fog_color: rgb(atmosphere.fog_color),
            fog: [
                atmosphere.fog_start,
                atmosphere.fog_end,
                atmosphere.fog_max,
                0.0,
            ],
        }
    }
}

/// Converts a colour written as `0xRRGGBB` (sRGB, as in colour pickers) to linear RGB,
/// the space where shaders compute light.
pub fn srgb_hex(rgb: u32) -> [f32; 3] {
    [16, 8, 0].map(|shift| srgb_to_linear(((rgb >> shift) & 0xff) as f32 / 255.0))
}

/// sRGB transfer function, inverted: screen value in [0, 1] → linear light in [0, 1].
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Relative luminance (perceived brightness) of a linear RGB colour, Rec. 709 weights.
pub fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atmosphere_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<AtmosphereUniform>(), 96);
        assert_eq!(std::mem::offset_of!(AtmosphereUniform, fog), 80);
    }

    #[test]
    fn sun_direction_is_normalised() {
        let [x, y, z, _] = AtmosphereUniform::new(&golden_hour()).sun_direction;
        assert!((x * x + y * y + z * z - 1.0).abs() < 1e-6);
        assert!(y > 0.0, "the sun must be above the horizon");
    }

    #[test]
    fn shadows_are_cooler_than_sunlight() {
        // Firewatch look: warm light, cool shadows. Compare blue / red ratios.
        let a = golden_hour();
        assert!(a.sky_color[2] / a.sky_color[0] > a.sun_color[2] / a.sun_color[0]);
    }

    #[test]
    fn concentration_luminance_strictly_increases() {
        let lum = concentration().map(luminance);
        assert!(lum.windows(2).all(|w| w[0] < w[1]), "{lum:?}");
    }

    /// CIE L*: perceived lightness, 0 = black, 100 = white.
    fn lightness(rgb: [f32; 3]) -> f32 {
        let y = luminance(rgb);
        if y > 0.008856 {
            116.0 * y.cbrt() - 16.0
        } else {
            903.3 * y
        }
    }

    #[test]
    fn concentration_leaves_room_for_lighting() {
        let l = concentration().map(lightness);
        assert!(l[0] > 30.0 && l[STOP_COUNT - 1] < 75.0, "{l:?}");
    }

    #[test]
    fn earth_is_monotonic_and_darker_than_living_matter() {
        let earth = earth().map(lightness);
        assert!(earth.windows(2).all(|w| w[0] < w[1]), "{earth:?}");
        let living = concentration().map(lightness);
        let mean = |l: [f32; STOP_COUNT]| l.iter().sum::<f32>() / STOP_COUNT as f32;
        assert!(mean(earth) + 10.0 < mean(living));
    }

    #[test]
    fn srgb_conversion_endpoints() {
        assert_eq!(srgb_hex(0x000000), [0.0; 3]);
        let white = srgb_hex(0xffffff);
        assert!(white.iter().all(|c| (c - 1.0).abs() < 1e-6));
        // Mid-grey on screen is only ~21 % of the light: the reason we light in linear space.
        assert!((srgb_to_linear(0.5) - 0.214).abs() < 1e-3);
    }
}
