//! The seasons: a year of four seasons of two lunar months each (16 game days, about five
//! hours of play), and what follows from them: warmth, the colour and the fall of leaves, the
//! drying of grass. Snow and ice depend on the weather as well (see `weather.rs`).
//!
//! The year is told by its phase in [0, 1): 0 the start of spring, 0.25 summer, 0.5 autumn,
//! 0.75 winter. A game begins at the end of summer.

use crate::clock::LUNAR_DAYS;

/// Game days in a season, and in a year.
pub const SEASON_DAYS: f32 = 2.0 * LUNAR_DAYS;
pub const YEAR_DAYS: f32 = 4.0 * SEASON_DAYS;
/// Phase of the year on the first day: late summer.
const START: f32 = 0.42;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub fn name(self) -> &'static str {
        match self {
            Season::Spring => "printemps",
            Season::Summer => "été",
            Season::Autumn => "automne",
            Season::Winter => "hiver",
        }
    }
}

/// Phase of the year at `days` since the start.
pub fn year(days: f64) -> f32 {
    ((days as f32) / YEAR_DAYS + START).rem_euclid(1.0)
}

pub fn season(year: f32) -> Season {
    match (year.rem_euclid(1.0) * 4.0) as u32 {
        0 => Season::Spring,
        1 => Season::Summer,
        2 => Season::Autumn,
        _ => Season::Winter,
    }
}

/// How much colder than in high summer it is (°C, ≤ 0): 9 degrees in spring and autumn, 18 in
/// the heart of winter.
pub fn cooling(year: f32) -> f32 {
    9.0 * ((std::f32::consts::TAU * (year - 0.375)).cos() - 1.0)
}

/// Smooth step from 0 at `a` to 1 at `b`.
fn ramp(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How the season looks, each in [0, 1].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    /// Deciduous leaves turned yellow, orange and red.
    pub autumn: f32,
    /// Deciduous leaves fallen.
    pub bare: f32,
    /// Grass dried (late summer) or dull and laid down (winter).
    pub dry: f32,
}

pub fn look(year: f32) -> Look {
    let y = year.rem_euclid(1.0);
    // Leaves turn through the autumn and fall at its end; buds open early in spring.
    let autumn = ramp(0.5, 0.62, y) * (1.0 - ramp(0.7, 0.76, y));
    let bare = if y < 0.15 {
        1.0 - ramp(0.02, 0.12, y)
    } else {
        ramp(0.62, 0.74, y)
    };
    // Grass: green in spring, drying through summer, dull in winter.
    let dry = 0.6 * ramp(0.3, 0.48, y) * (1.0 - ramp(0.55, 0.65, y)) + 0.45 * ramp(0.68, 0.78, y)
        - 0.45 * ramp(0.02, 0.12, y) * (if y < 0.5 { 1.0 } else { 0.0 });
    Look {
        autumn,
        bare,
        dry: dry.clamp(0.0, 1.0),
    }
}

/// How fast plants grow at this time of the year (a factor on their growth rate): fast in
/// spring, slower in summer heat, little in autumn, almost not at all in winter.
pub fn growth(year: f32) -> f32 {
    match season(year) {
        Season::Spring => 1.6,
        Season::Summer => 1.0,
        Season::Autumn => 0.4,
        Season::Winter => 0.05,
    }
}

/// Evaporation and the plants' draw of water, against high summer: little in the cold.
pub fn evaporation(year: f32) -> f32 {
    // Follows the warmth: 1 at the height of summer, 0.3 in the heart of winter.
    1.0 + 0.7 * cooling(year) / 18.0
}

/// How readily a seed germinates now: mostly in spring.
pub fn germination(year: f32) -> f32 {
    match season(year) {
        Season::Spring => 1.0,
        Season::Summer => 0.4,
        Season::Autumn => 0.15,
        Season::Winter => 0.0,
    }
}

/// What share of their summer size herbs keep above ground: they die back in winter.
pub fn herb_cover(year: f32) -> f32 {
    let y = year.rem_euclid(1.0);
    // Winter: from late autumn until the growth of early spring.
    let winter = if y >= 0.5 {
        ramp(0.62, 0.78, y)
    } else {
        1.0 - ramp(0.0, 0.12, y)
    };
    1.0 - 0.4 * winter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_year_turns_through_its_seasons() {
        // Late summer at the start; autumn a few days later; spring again after a year.
        assert_eq!(season(year(0.0)), Season::Summer);
        let autumn = (0.5 - START) * YEAR_DAYS;
        assert_eq!(season(year(autumn as f64 + 1.0)), Season::Autumn);
        assert!((year(YEAR_DAYS as f64) - year(0.0)).abs() < 1e-5);
        // Warm in summer, cold in winter.
        assert!(cooling(0.375).abs() < 1e-4);
        assert!(cooling(0.875) < -17.0);
        // Leaves: green in summer, turning in autumn, gone in winter, back in late spring.
        assert_eq!(look(0.35).bare, 0.0);
        assert!(look(0.6).autumn > 0.7);
        assert!(look(0.85).bare > 0.99);
        assert!(look(0.2).bare < 0.01);
        // Herbs die back in winter, not in summer.
        assert!(herb_cover(0.85) < 0.7 && herb_cover(0.35) > 0.99);
    }
}
