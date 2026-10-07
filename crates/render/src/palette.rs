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

/// Water by depth: lagoon turquoise → deep sea blue.
///
/// Luminance strictly *decreases* along the stops (L* ≈ 73 → 29): depth reads as darkness,
/// whatever the hue, as in real water. Saturated enough to read as water and not as stone
/// even in shallow, almost transparent places.
pub fn water() -> Palette {
    [
        srgb_hex(0x5cc4c0), // L* 73.3
        srgb_hex(0x35a3b3), // L* 61.8
        srgb_hex(0x24829f), // L* 50.5
        srgb_hex(0x1c6488), // L* 39.8
        srgb_hex(0x16496b), // L* 29.4
    ]
}

/// Foam on fast water: pale sea green → off-white. Brighter than any water stop, so foam
/// reads by luminance.
pub fn foam() -> Palette {
    [
        srgb_hex(0xa9d2c4),
        srgb_hex(0xc0ddd0),
        srgb_hex(0xd5e7dc),
        srgb_hex(0xe6efe6),
        srgb_hex(0xf4f4ec),
    ]
}

/// Number of entries of the material colour table.
pub const MATERIAL_SLOTS: usize = 64;

/// Colour of each material of the world, indexed by material id (`world::Material`, same
/// order). Golden-hour tones: yellow-green grass, warm sand, warm grey rock.
///
/// Distinct materials differ in lightness, not only in hue (snow L* 95, sand 84, grass 66,
/// rock 54, pine 41, wood 35), so they stay distinguishable in greyscale.
pub fn materials() -> [[f32; 3]; MATERIAL_SLOTS] {
    [
        srgb_hex(0x000000), // 0 air (never drawn)
        srgb_hex(0x98a74f), // 1 grass
        srgb_hex(0x6f8a3e), // 2 forest floor
        srgb_hex(0x8a6440), // 3 dirt
        srgb_hex(0xe8cf98), // 4 sand
        srgb_hex(0xe0aa6a), // 5 desert sand
        srgb_hex(0xc58252), // 6 sandstone
        srgb_hex(0x8c8079), // 7 rock
        srgb_hex(0xf2f1ee), // 8 snow
        srgb_hex(0x9c948a), // 9 gravel
        srgb_hex(0x6b4a33), // 10 wood
        srgb_hex(0x7f9e3e), // 11 leaves
        srgb_hex(0x3f6b4a), // 12 pine needles
        srgb_hex(0x6f9a55), // 13 cactus
        srgb_hex(0xc2ad62), // 14 dry grass
        srgb_hex(0xb5683f), // 15 clay
        srgb_hex(0xa9b955), // 16 tall grass (L* 72)
        srgb_hex(0xd0503c), // 17 red flower (51)
        srgb_hex(0xeac54a), // 18 yellow flower (81)
        srgb_hex(0xf2eee2), // 19 white flower (94)
        srgb_hex(0x9468bd), // 20 violet flower (52)
        srgb_hex(0x5c9040), // 21 fern (55)
        srgb_hex(0x9a5038), // 22 mushroom cap (42)
        srgb_hex(0xeae0c8), // 23 mushroom stem (89)
        srgb_hex(0xe4dfd2), // 24 birch bark (89)
        srgb_hex(0xaabd4c), // 25 birch leaves (73)
        srgb_hex(0x9c7c54), // 26 palm trunk (54)
        srgb_hex(0x6c9e3a), // 27 palm leaves (60)
        srgb_hex(0x8c7f72), // 28 dead wood (54)
        srgb_hex(0xaaa49c), // 29 stone (68)
        srgb_hex(0x8cad52), // 30 willow leaves (67)
        srgb_hex(0xa38c55), // 31 dry shrub (59)
        // The naturalist's clothes and gear.
        srgb_hex(0xd9a77c), // 32 skin (72)
        srgb_hex(0xb39a5e), // 33 khaki shirt (65)
        srgb_hex(0x666a3c), // 34 olive cloth (43)
        srgb_hex(0x6a4529), // 35 leather (33)
        srgb_hex(0xc8b386), // 36 canvas (74)
        srgb_hex(0x7d5534), // 37 felt hat (40)
        srgb_hex(0xa63b2d), // 38 red notebook (40)
        srgb_hex(0x4a3122), // 39 hair (23)
        srgb_hex(0x241c18), // 40 eyes (11)
        srgb_hex(0xc9a24a), // 41 brass (69)
        srgb_hex(0x45301f), // 42 dark bark (22)
        srgb_hex(0x9a5c36), // 43 deer coat, summer red (45)
        srgb_hex(0xd8c7a2), // 44 deer rump and belly (81)
        srgb_hex(0x2b221c), // 45 hooves, muzzle (14)
        srgb_hex(0x6e4128), // 46 deer back, darker (32)
        // Phase 2: the marsh, building, the new animals, fallen leaves.
        srgb_hex(0x4a4033), // 47 marsh mud (28)
        srgb_hex(0x8f9e55), // 48 reed stems (63)
        srgb_hex(0x6b4c33), // 49 reed plume (34)
        srgb_hex(0xa58560), // 50 timber, raw wood (57)
        srgb_hex(0x7a6a44), // 51 wattle, woven rods (45)
        srgb_hex(0xbfa27a), // 52 daub, clay and straw (68)
        srgb_hex(0xd2b46e), // 53 thatch (74)
        srgb_hex(0x8e6b48), // 54 planks (48)
        srgb_hex(0x7b5b3e), // 55 owl, mottled brown (41)
        srgb_hex(0xcdb894), // 56 owl face and belly (75)
        srgb_hex(0xb5552b), // 57 fox coat (47)
        srgb_hex(0xece2d0), // 58 fox throat, tail tip (90)
        srgb_hex(0x2f2621), // 59 fox legs and ears (16)
        srgb_hex(0x6a5543), // 60 vole fur (38)
        srgb_hex(0x9a6532), // 61 dead leaves (47)
        // Free slots.
        srgb_hex(0xff00ff),
        srgb_hex(0xff00ff),
    ]
}

/// Material colours as read by the shaders.
///
/// WGSL side (`shaders/voxel.wgsl`):
/// ```wgsl
/// struct Materials {
///     colors: array<vec4<f32>, 64>,  // offset 0, size 1024 (rgb + unused w)
/// }
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialsUniform {
    pub colors: [[f32; 4]; MATERIAL_SLOTS],
}

impl MaterialsUniform {
    pub fn new(colors: &[[f32; 3]; MATERIAL_SLOTS]) -> Self {
        Self {
            colors: colors.map(|[r, g, b]| [r, g, b, 1.0]),
        }
    }
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

/// Late-afternoon light: warm sun, blue sky light in the shadows, a light cream haze that
/// only veils what is far beyond the camera target.
pub fn golden_hour() -> Atmosphere {
    Atmosphere {
        sun_direction: [0.45, 0.8, -0.4],
        sun_color: srgb_hex(0xffe6c4),
        sky_color: srgb_hex(0x8ea6c8),
        ground_color: srgb_hex(0x7c6a55),
        fog_color: srgb_hex(0xefd6b8),
        fog_start: 60.0,
        fog_end: 400.0,
        fog_max: 0.35,
    }
}

/// Light and haze, as read by the shaders.
///
/// WGSL side (`shaders/voxel.wgsl`), every field a `vec4<f32>`, `w` unused unless stated:
/// ```wgsl
/// struct Atmosphere {
///     sun_direction: vec4<f32>,  // offset 0, w = moon's lit share
///     sun_color: vec4<f32>,      // offset 16
///     sky_color: vec4<f32>,      // offset 32, w = wind towards x
///     ground_color: vec4<f32>,   // offset 48, w = wind towards z
///     fog_color: vec4<f32>,      // offset 64, w = grass dried by the season (0 to 1)
///     fog: vec4<f32>,            // offset 80: start, end, max, stars (0 to 1)
///     season: vec4<f32>,         // offset 96: leaves turned, leaves fallen, snow, ice (0 to 1)
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
    pub season: [f32; 4],
}

impl AtmosphereUniform {
    /// The season: leaves turned and fallen, snow cover, ice on the water, grass dried (each
    /// 0 to 1).
    pub fn with_season(mut self, season: [f32; 4], dry: f32) -> Self {
        self.season = season;
        self.fog_color[3] = dry;
        self
    }

    /// Direction the wind blows towards, on the ground plane (x, z), unit.
    pub fn with_wind(mut self, wind: [f32; 2]) -> Self {
        self.sky_color[3] = wind[0];
        self.ground_color[3] = wind[1];
        self
    }

    /// Lit share of the moon's disc, in [0, 1] (its reflection in the water).
    pub fn with_moon(mut self, moon: f32) -> Self {
        self.sun_direction[3] = moon;
        self
    }

    /// `stars`: how many stars show, in [0, 1] (night sky reflected in the water). The moon's
    /// lit share defaults to full; see `with_moon`.
    pub fn new(atmosphere: &Atmosphere, stars: f32) -> Self {
        let [x, y, z] = atmosphere.sun_direction;
        let length = (x * x + y * y + z * z).sqrt();
        let rgb = |[r, g, b]: [f32; 3]| [r, g, b, 1.0];
        let wind = [0.89, 0.45];
        Self {
            sun_direction: [x / length, y / length, z / length, 1.0],
            sun_color: rgb(atmosphere.sun_color),
            sky_color: [
                atmosphere.sky_color[0],
                atmosphere.sky_color[1],
                atmosphere.sky_color[2],
                wind[0],
            ],
            ground_color: [
                atmosphere.ground_color[0],
                atmosphere.ground_color[1],
                atmosphere.ground_color[2],
                wind[1],
            ],
            fog_color: rgb(atmosphere.fog_color),
            fog: [
                atmosphere.fog_start,
                atmosphere.fog_end,
                atmosphere.fog_max,
                stars,
            ],
            season: [0.0; 4],
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
    fn materials_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<MaterialsUniform>(), 1024);
    }

    #[test]
    fn atmosphere_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<AtmosphereUniform>(), 112);
        assert_eq!(std::mem::offset_of!(AtmosphereUniform, season), 96);
        assert_eq!(std::mem::offset_of!(AtmosphereUniform, fog), 80);
    }

    #[test]
    fn sun_direction_is_normalised() {
        let [x, y, z, _] = AtmosphereUniform::new(&golden_hour(), 0.0).sun_direction;
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
    fn foam_is_brighter_than_water() {
        let foam = foam().map(lightness);
        assert!(foam.windows(2).all(|w| w[0] < w[1]), "{foam:?}");
        let brightest_water = water().map(lightness)[0];
        assert!(foam[0] > brightest_water, "{foam:?}");
    }

    #[test]
    fn deeper_water_is_darker() {
        let l = water().map(lightness);
        assert!(l.windows(2).all(|w| w[0] > w[1]), "{l:?}");
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
