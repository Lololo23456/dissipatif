//! The wind: where it blows from changes over the hours.
//!
//! A prevailing direction that wanders slowly over the days (weather systems passing), plus a
//! daily swing (land and valley breezes turn with the sun). Deterministic: a function of time
//! only, so every system (plants bending, fire, the scent animals catch) sees the same wind.

use glam::Vec2;
use std::f32::consts::TAU;

/// Direction the wind blows towards (unit, on the ground plane) at `days` since the start.
pub fn direction(days: f64) -> Vec2 {
    let d = days as f32;
    let angle = 0.47 // the old fixed wind, towards +x and a bit +z
        + 1.1 * (TAU * d / 2.7).sin()
        + 0.6 * (TAU * d / 1.3 + 1.7).sin()
        + 0.35 * (TAU * d + 0.8).sin();
    Vec2::new(angle.cos(), angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wind_turns_slowly() {
        // Over a game hour it barely moves; over two days it has gone round a good deal.
        let hour = 1.0 / 24.0;
        let mut most_in_an_hour: f32 = 0.0;
        let mut widest: f32 = 1.0;
        let start = direction(0.0);
        for k in 0..48 {
            let t = k as f64 * hour;
            most_in_an_hour = most_in_an_hour.max(direction(t).angle_to(direction(t + hour)).abs());
            widest = widest.min(start.dot(direction(t)));
        }
        assert!(most_in_an_hour < 0.35, "{most_in_an_hour}");
        assert!(widest < 0.3, "{widest}");
    }
}
