//! The notebook: found glowing on the ground at the start, nobody knows why. It fills by
//! itself with what the naturalist witnessed: mostly sketches (made from what was really seen,
//! see `sketch.rs`), sometimes a few words. Each page keeps the day, the hour, the moon, the
//! wind and the place: the material for hypotheses. It never explains; the naturalist links
//! the pages.

use glam::Vec2;

use crate::deer::Cause;
use crate::state::Conditions;

/// The spells: magic practised by living things, learnt by understanding them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spell {
    /// The shape of a deer: the herd takes you for one of its own; a deer's speed and leap,
    /// but no hands. Learnt from the hinds' circle under the full moon.
    DeerForm,
}

impl Spell {
    pub fn name(self) -> &'static str {
        match self {
            Spell::DeerForm => "Forme du cerf",
        }
    }
}

/// What a page records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// The notebook itself, where it lay.
    Found,
    /// The herd, first seen.
    Herd,
    /// A hind stamping and barking.
    Alarm,
    /// The herd fled, because of…
    Fled(Cause),
    /// The ring of trodden earth in the meadow.
    Ring,
    /// A hind turning alone at night.
    Turning,
    /// All of them walking the circle under the full moon.
    Rite,
    /// A spell understood.
    Learnt(Spell),
}

impl Entry {
    /// The few words written under the sketch, if any.
    pub fn words(self) -> &'static [&'static str] {
        match self {
            Entry::Found | Entry::Herd | Entry::Rite => &[],
            Entry::Alarm => &["Une biche frappe du pied,", "puis aboie."],
            Entry::Fled(Cause::Scent) => &["Elles ont fui", "sans m'avoir vu."],
            Entry::Fled(Cause::Sight) => &["Elles m'ont vu bouger."],
            Entry::Fled(Cause::Sound) => &["Mes pas."],
            Entry::Fled(Cause::Fire) => &["Le feu."],
            Entry::Ring => &[
                "Un cercle de terre nue.",
                "Des sabots, tous",
                "dans le même sens.",
            ],
            Entry::Turning => &["Seule, en pleine nuit."],
            Entry::Learnt(Spell::DeerForm) => &["Je sais marcher", "comme elles."],
        }
    }

    /// A title over the page, for a spell.
    pub fn title(self) -> Option<&'static str> {
        match self {
            Entry::Learnt(spell) => Some(spell.name()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Page {
    pub entry: Entry,
    pub day: u32,
    pub hour: f32,
    /// Moon phase (0 new, 0.5 full) and the direction the wind blew towards.
    pub moon: f32,
    pub wind: Vec2,
    /// Where it was witnessed.
    pub at: Vec2,
}

pub struct Notebook {
    pub pages: Vec<Page>,
    /// Where it was found: places are told from there.
    pub found_at: Vec2,
}

impl Notebook {
    pub fn new(found_at: Vec2) -> Self {
        Self {
            pages: Vec::new(),
            found_at,
        }
    }

    pub fn has(&self, entry: Entry) -> bool {
        self.pages.iter().any(|p| p.entry == entry)
    }

    /// Writes a page, once per kind of entry. Returns its index if written.
    pub fn write(&mut self, entry: Entry, now: &Conditions, at: Vec2) -> Option<usize> {
        if self.has(entry) {
            return None;
        }
        self.pages.push(Page {
            entry,
            day: now.days as u32 + 1,
            hour: now.hour,
            moon: now.moon,
            wind: now.wind,
            at,
        });
        Some(self.pages.len() - 1)
    }

    /// Where a page was, from where the notebook was found: "54 pas au nord-est".
    pub fn place(&self, page: &Page) -> String {
        let offset = page.at - self.found_at;
        let steps = offset.length().round() as u32;
        if steps < 4 {
            return "là où il était".to_string();
        }
        format!("{steps} pas au {}", compass(offset))
    }
}

/// Compass direction of a horizontal vector, north being −z.
pub fn compass(v: Vec2) -> &'static str {
    // Angle from north, clockwise (east is +x).
    let angle = v.x.atan2(-v.y).rem_euclid(std::f32::consts::TAU);
    let names = [
        "nord",
        "nord-est",
        "est",
        "sud-est",
        "sud",
        "sud-ouest",
        "ouest",
        "nord-ouest",
    ];
    names[((angle / std::f32::consts::FRAC_PI_4).round() as usize) % 8]
}

/// Where the wind comes from, in words: a wind blowing towards `towards` comes from the
/// opposite side ("vent d'ouest" blows towards the east).
pub fn wind_words(towards: Vec2) -> String {
    let from = compass(-towards);
    let article = if from.starts_with(['e', 'o']) {
        "d'"
    } else {
        "du "
    };
    format!("vent {article}{from}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_and_winds_read_like_a_naturalist_would_write_them() {
        assert_eq!(compass(Vec2::new(0.0, -1.0)), "nord");
        assert_eq!(compass(Vec2::new(1.0, 0.0)), "est");
        assert_eq!(compass(Vec2::new(-1.0, 1.0)), "sud-ouest");
        // Blowing towards the east: a west wind.
        assert_eq!(wind_words(Vec2::X), "vent d'ouest");
        assert_eq!(wind_words(Vec2::new(0.0, 1.0)), "vent du nord");
        let mut book = Notebook::new(Vec2::ZERO);
        let now = crate::state::Conditions::noon();
        assert_eq!(
            book.write(Entry::Herd, &now, Vec2::new(30.0, -40.0)),
            Some(0)
        );
        assert_eq!(book.write(Entry::Herd, &now, Vec2::ZERO), None);
        assert_eq!(book.place(&book.pages[0]), "50 pas au nord-est");
    }
}
