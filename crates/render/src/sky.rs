//! The sky over a day: where the sun (or the moon) is, the colour of light, shadows and haze,
//! the grading of the final image, and how many stars show, for any hour.
//!
//! A few key moments are written by hand (night, dawn, morning, noon, golden hour, sunset,
//! dusk); between two of them every value is blended linearly. The light's direction is
//! computed from the hour: the sun rises at 5:30, peaks at about 12:45 and sets at 20:00; the
//! moon crosses the sky at night. Light fades to nothing as either nears the horizon, so the
//! switch between sun and moon (and their shadows) happens unseen, in the twilight.

use crate::palette::{Atmosphere, srgb_hex};

pub const SUNRISE: f32 = 5.5;
pub const SUNSET: f32 = 20.0;

/// Grading of the final image (see `post.wgsl`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grade {
    /// Multiplies the darks and the lights (split toning).
    pub shadow_tint: [f32; 3],
    pub light_tint: [f32; 3],
    /// Overall brightness factor, before grading.
    pub exposure: f32,
    /// 1 unchanged, < 1 greyer (at night, as our eyes lose colours in the dark).
    pub saturation: f32,
}

/// Everything that changes with the hour.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    pub atmosphere: Atmosphere,
    pub grade: Grade,
    /// Stars visible, in [0, 1] (reflected in the water).
    pub stars: f32,
    /// Darkness, in [0, 1]: 0 by day, 1 in the deep night (fireflies, crickets…).
    pub night: f32,
    /// Whether the direct light is the moon's (1) or the sun's (0).
    pub moonlit: f32,
    /// Lit share of the moon's disc, in [0, 1] (see `with_moon`).
    pub moon: f32,
    /// Direction the wind blows towards, (x, z), unit.
    pub wind: [f32; 2],
    /// The season: leaves turned, leaves fallen, snow on the ground, ice on the water (each
    /// 0 to 1), and grass dried.
    pub season: [f32; 4],
    pub dry: f32,
}

impl Sky {
    /// The moon's phase: its light at night is as strong as the lit share of its disc
    /// (`moon`, 0 new to 1 full), a new-moon night lit by the stars alone.
    pub fn with_moon(mut self, moon: f32) -> Self {
        let factor = 1.0 + self.moonlit * (0.08 + 0.92 * moon - 1.0);
        self.atmosphere.sun_color = self.atmosphere.sun_color.map(|c| c * factor);
        self.moon = moon;
        self
    }
}

/// One hand-written moment of the day.
struct Key {
    hour: f32,
    /// Colour of the direct light (sun, or moon at night), sRGB hex, and its strength.
    light: u32,
    strength: f32,
    sky: u32,
    ground: u32,
    fog: u32,
    fog_max: f32,
    shadow_tint: [f32; 3],
    light_tint: [f32; 3],
    exposure: f32,
    saturation: f32,
    stars: f32,
    night: f32,
}

const NIGHT: Key = Key {
    hour: 0.0,
    light: 0x7d93c8,
    strength: 0.32,
    sky: 0x223052,
    ground: 0x15171f,
    fog: 0x172036,
    fog_max: 0.5,
    shadow_tint: [0.88, 0.95, 1.3],
    light_tint: [0.92, 0.97, 1.12],
    exposure: 1.35,
    saturation: 0.7,
    stars: 1.0,
    night: 1.0,
};

/// The day, in order of hours; the first and last wrap around midnight.
const KEYS: [Key; 9] = [
    NIGHT,
    Key { hour: 4.6, ..NIGHT },
    // Dawn: pink light, mauve shadows, a rosy haze.
    Key {
        hour: 5.8,
        light: 0xffb09a,
        strength: 0.6,
        sky: 0x7d82ae,
        ground: 0x5a4a52,
        fog: 0xe6b6b4,
        fog_max: 0.55,
        shadow_tint: [0.9, 0.86, 1.16],
        light_tint: [1.08, 0.97, 0.96],
        exposure: 1.08,
        saturation: 0.9,
        stars: 0.15,
        night: 0.2,
    },
    // Morning: soft light, pale misty haze.
    Key {
        hour: 7.5,
        light: 0xffe4c4,
        strength: 0.9,
        sky: 0x9cb2d2,
        ground: 0x7a6c5a,
        fog: 0xe4dfd6,
        fog_max: 0.45,
        shadow_tint: [0.88, 0.9, 1.12],
        light_tint: [1.04, 1.0, 0.93],
        exposure: 1.0,
        saturation: 0.97,
        stars: 0.0,
        night: 0.0,
    },
    // Noon: white light, clear air.
    Key {
        hour: 12.5,
        light: 0xfff4e6,
        strength: 1.0,
        sky: 0x9ab8de,
        ground: 0x7c6a55,
        fog: 0xdde5ee,
        fog_max: 0.3,
        shadow_tint: [0.9, 0.92, 1.1],
        light_tint: [1.02, 1.0, 0.96],
        exposure: 0.97,
        saturation: 0.97,
        stars: 0.0,
        night: 0.0,
    },
    // Golden hour: the look the game was tuned in.
    Key {
        hour: 17.0,
        light: 0xffe6c4,
        strength: 1.0,
        sky: 0x8ea6c8,
        ground: 0x7c6a55,
        fog: 0xefd6b8,
        fog_max: 0.35,
        shadow_tint: [0.84, 0.86, 1.2],
        light_tint: [1.07, 1.0, 0.88],
        exposure: 1.0,
        saturation: 0.97,
        stars: 0.0,
        night: 0.0,
    },
    // Sunset: orange light, violet shadows, glowing haze.
    Key {
        hour: 19.2,
        light: 0xff9858,
        strength: 0.9,
        sky: 0x8a7eaa,
        ground: 0x6a4a40,
        fog: 0xf0a676,
        fog_max: 0.45,
        shadow_tint: [0.86, 0.8, 1.22],
        light_tint: [1.12, 0.96, 0.8],
        exposure: 1.04,
        saturation: 1.0,
        stars: 0.0,
        night: 0.0,
    },
    // Dusk: the blue hour.
    Key {
        hour: 20.6,
        light: 0x8a92cc,
        strength: 0.3,
        sky: 0x3c4674,
        ground: 0x2a2632,
        fog: 0x3c3e66,
        fog_max: 0.5,
        shadow_tint: [0.86, 0.9, 1.3],
        light_tint: [0.96, 0.95, 1.1],
        exposure: 1.22,
        saturation: 0.82,
        stars: 0.55,
        night: 0.6,
    },
    Key {
        hour: 22.0,
        ..NIGHT
    },
];

/// Direction towards the sun at `hour`, not normalised: y > 0 by day, < 0 at night. The sun
/// travels from −x (east) to +x (west), leaning south (−z); the angle θ goes from 0 to π over
/// the day and from π to 2π over the night, so the path is continuous.
fn sun_path(hour: f32) -> [f32; 3] {
    let h = hour.rem_euclid(24.0);
    let day = SUNSET - SUNRISE;
    let theta = if (SUNRISE..SUNSET).contains(&h) {
        std::f32::consts::PI * (h - SUNRISE) / day
    } else {
        let since = (h - SUNSET).rem_euclid(24.0);
        std::f32::consts::PI * (1.0 + since / (24.0 - day))
    };
    [-0.9 * theta.cos(), 0.85 * theta.sin(), -0.45]
}

pub(crate) fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Lowest elevation of the light used for shadows: a lower one would stretch them across the
/// whole map.
const MIN_SHADOW_ELEVATION: f32 = 0.2;

pub fn sky(hour: f32) -> Sky {
    let h = hour.rem_euclid(24.0);
    // The two keys around `h` (wrapping past midnight) and how far between them.
    let next = KEYS.iter().position(|k| k.hour > h).unwrap_or(KEYS.len());
    let (a, b) = (&KEYS[next - 1], &KEYS[next % KEYS.len()]);
    let span = (b.hour - a.hour).rem_euclid(24.0).max(1e-3);
    let t = smoothstep(0.0, 1.0, (h - a.hour).rem_euclid(24.0) / span);
    let mix = |x: f32, y: f32| x + (y - x) * t;
    let color = |x: u32, y: u32| mix3(srgb_hex(x), srgb_hex(y), t);

    // Sun by day, moon by night (opposite in the sky), fading out near the horizon.
    let sun = sun_path(h);
    let moonlit = if sun[1] >= 0.0 { 0.0 } else { 1.0 };
    let (raw, up) = if sun[1] >= 0.0 {
        (sun, sun[1])
    } else {
        ([-sun[0], -sun[1], 0.35], -sun[1])
    };
    let presence = smoothstep(0.0, 0.15, up);
    let length = (raw[0] * raw[0] + raw[2] * raw[2]).sqrt();
    let elevation = up.max(MIN_SHADOW_ELEVATION);
    let direction = [raw[0] / length, elevation, raw[2] / length];

    let strength = mix(a.strength, b.strength) * presence;
    let light = color(a.light, b.light).map(|c| c * strength);
    Sky {
        atmosphere: Atmosphere {
            sun_direction: direction,
            sun_color: light,
            sky_color: color(a.sky, b.sky),
            ground_color: color(a.ground, b.ground),
            fog_color: color(a.fog, b.fog),
            fog_start: 60.0,
            fog_end: 400.0,
            fog_max: mix(a.fog_max, b.fog_max),
        },
        grade: Grade {
            shadow_tint: mix3(a.shadow_tint, b.shadow_tint, t),
            light_tint: mix3(a.light_tint, b.light_tint, t),
            exposure: mix(a.exposure, b.exposure),
            saturation: mix(a.saturation, b.saturation),
        },
        stars: mix(a.stars, b.stars),
        night: mix(a.night, b.night),
        moonlit,
        moon: 1.0,
        wind: [0.89, 0.45],
        season: [0.0; 4],
        dry: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance([r, g, b]: [f32; 3]) -> f32 {
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    #[test]
    fn keys_are_in_order() {
        assert!(KEYS.windows(2).all(|w| w[0].hour < w[1].hour));
    }

    #[test]
    fn noon_is_bright_and_midnight_is_dark_and_starry() {
        let noon = sky(12.5);
        let midnight = sky(0.0);
        assert!(
            luminance(noon.atmosphere.sun_color) > 4.0 * luminance(midnight.atmosphere.sun_color)
        );
        assert_eq!(noon.stars, 0.0);
        assert!(midnight.stars > 0.9 && midnight.night > 0.9);
    }

    #[test]
    fn the_light_always_comes_from_above() {
        for k in 0..240 {
            let s = sky(k as f32 * 0.1);
            assert!(s.atmosphere.sun_direction[1] >= MIN_SHADOW_ELEVATION - 1e-6);
        }
    }

    /// No sudden change anywhere in the day: the light's colour and direction (weighted by
    /// its strength: an invisible light may turn round) move little in 1 minute.
    #[test]
    fn the_day_is_continuous() {
        let minute = 1.0 / 60.0;
        let mut previous = sky(0.0);
        for k in 1..=24 * 60 {
            let s = sky(k as f32 * minute);
            let (a, b) = (&previous.atmosphere, &s.atmosphere);
            let lit = |x: &Atmosphere| {
                let l = luminance(x.sun_color);
                x.sun_direction.map(|d| d * l)
            };
            let (la, lb) = (lit(a), lit(b));
            let jump = (0..3).map(|i| (la[i] - lb[i]).abs()).fold(0.0, f32::max);
            assert!(
                jump < 0.02,
                "light jumps by {jump} at {:.2} h",
                k as f32 * minute
            );
            let fog = (0..3)
                .map(|i| (a.fog_color[i] - b.fog_color[i]).abs())
                .fold(0.0, f32::max);
            assert!(
                fog < 0.02,
                "haze jumps by {fog} at {:.2} h",
                k as f32 * minute
            );
            previous = s;
        }
    }

    #[test]
    fn golden_hour_matches_the_tuned_look() {
        let s = sky(17.0);
        assert_eq!(s.atmosphere.sky_color, srgb_hex(0x8ea6c8));
        assert!(s.atmosphere.sun_direction[0] > 0.0, "sun in the west");
    }
}
