//! Time of day in the game.

/// Real seconds for a whole day: 20 minutes.
const DAY_SECONDS: f32 = 20.0 * 60.0;
/// Speed-up while fast-forwarding (key T): a day in 20 seconds.
const FAST: f32 = 60.0;
/// Hour at the start of a game: late afternoon, the golden hour comes soon.
pub const START_HOUR: f32 = 16.5;

pub struct Clock {
    /// In [0, 24).
    hour: f32,
    pub fast: bool,
}

impl Clock {
    pub fn new(hour: f32) -> Self {
        Self {
            hour: hour.rem_euclid(24.0),
            fast: false,
        }
    }

    pub fn advance(&mut self, dt: f32) {
        let speed = if self.fast { FAST } else { 1.0 };
        self.hour = (self.hour + dt * speed * 24.0 / DAY_SECONDS).rem_euclid(24.0);
    }

    pub fn hour(&self) -> f32 {
        self.hour
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_lasts_twenty_minutes_and_wraps() {
        let mut clock = Clock::new(23.0);
        for _ in 0..(DAY_SECONDS / 24.0 * 60.0) as usize {
            clock.advance(1.0 / 60.0);
        }
        assert!(
            (clock.hour() - 0.0).abs() < 0.01 || (clock.hour() - 24.0).abs() < 0.01,
            "{}",
            clock.hour()
        );
    }
}
