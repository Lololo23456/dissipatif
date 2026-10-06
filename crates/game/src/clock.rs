//! Time in the game: the hour, the day, the moon.
//!
//! A real lunar month lasts 29.5 days; with 20-minute days that would be ten hours of play
//! between two full moons. The cycle is stylised to `LUNAR_DAYS` game days.

/// Real seconds for a whole day: 20 minutes.
pub const DAY_SECONDS: f32 = 20.0 * 60.0;
/// Speed-up while fast-forwarding (key T): a day in 20 seconds.
const FAST: f32 = 60.0;
/// Hour at the start of a game: late afternoon, the golden hour comes soon.
pub const START_HOUR: f32 = 16.5;
/// Game days from one new moon to the next.
pub const LUNAR_DAYS: f32 = 8.0;
/// Moon phase on the first day (0 new, 0.5 full): full moon on the third night.
const START_PHASE: f32 = 0.5 - 2.75 / LUNAR_DAYS;

pub struct Clock {
    /// Days since the start, the hour as a fraction of the day.
    days: f64,
    pub fast: bool,
}

impl Clock {
    pub fn new(hour: f32) -> Self {
        Self {
            days: (hour.rem_euclid(24.0) / 24.0) as f64,
            fast: false,
        }
    }

    /// At `days` since the start (a saved game).
    pub fn at_days(days: f64) -> Self {
        Self {
            days: days.max(0.0),
            fast: false,
        }
    }

    /// At `hour` on day `day` (from 1).
    pub fn on_day(day: u32, hour: f32) -> Self {
        let mut clock = Self::new(hour);
        clock.days += day.saturating_sub(1) as f64;
        clock
    }

    pub fn advance(&mut self, dt: f32) {
        let speed = if self.fast { FAST } else { 1.0 };
        self.days += (dt * speed / DAY_SECONDS) as f64;
    }

    /// Hour of the day, in [0, 24).
    pub fn hour(&self) -> f32 {
        (self.days.fract() * 24.0) as f32
    }

    /// Days since the start, fractional.
    pub fn days(&self) -> f64 {
        self.days
    }

    /// Day number, from 1.
    pub fn day(&self) -> u32 {
        self.days as u32 + 1
    }

    /// Moon phase in [0, 1): 0 new moon, 0.5 full moon.
    pub fn moon_phase(&self) -> f32 {
        moon_phase(self.days)
    }

    /// The moment, for the game state, with the rain of the hour.
    pub fn conditions(&self, rain: f32) -> crate::state::Conditions {
        crate::state::Conditions {
            hour: self.hour(),
            days: self.days,
            moon: self.moon_phase(),
            rain,
            wind: crate::wind::direction(self.days),
        }
    }

    /// Lit share of the moon's disc, in [0, 1].
    pub fn moon_light(&self) -> f32 {
        moon_light(self.moon_phase())
    }
}

/// Moon phase at `days` since the start.
pub fn moon_phase(days: f64) -> f32 {
    ((days as f32 / LUNAR_DAYS) + START_PHASE).rem_euclid(1.0)
}

/// Lit share of the moon's disc at `phase`: (1 − cos 2πφ) / 2.
pub fn moon_light(phase: f32) -> f32 {
    0.5 * (1.0 - (std::f32::consts::TAU * phase).cos())
}

/// Name of a phase, for the notebook.
pub fn moon_name(phase: f32) -> &'static str {
    match (phase * 8.0).round() as u32 % 8 {
        0 => "nouvelle lune",
        1 => "premier croissant",
        2 => "premier quartier",
        3 => "lune gibbeuse croissante",
        4 => "pleine lune",
        5 => "lune gibbeuse décroissante",
        6 => "dernier quartier",
        _ => "dernier croissant",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_moon_is_full_on_the_third_night_and_new_four_days_later() {
        let night_three = 2.0 + 21.0 / 24.0;
        assert!(moon_light(moon_phase(night_three)) > 0.97);
        assert!(moon_light(moon_phase(night_three + LUNAR_DAYS as f64 / 2.0)) < 0.03);
        assert_eq!(moon_name(moon_phase(night_three)), "pleine lune");
    }

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
