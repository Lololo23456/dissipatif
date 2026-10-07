//! The notebook: found glowing on the ground at the start, nobody knows why. It fills by
//! itself with what the naturalist witnessed: mostly sketches (made from what was really seen,
//! see `sketch.rs`), sometimes a few words. Each page keeps the day, the hour, the moon, the
//! wind and the place: the material for hypotheses. It never explains; the naturalist links
//! the pages.

use glam::Vec2;

use crate::anomaly::Kind;
use crate::deer::Cause;
use crate::state::Conditions;

/// The spells: magic practised by living things, learnt by understanding them. Each answers only
/// while the anomaly that taught it lives (see `anomaly.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spell {
    /// The shape of a deer: the herd takes you for one of its own; a deer's speed and leap,
    /// but no hands. Learnt from the hinds' circle under the full moon.
    DeerForm,
    /// The eye of an owl: a spell of perception, to see in the dark and hear what moves. Learnt
    /// from the owls' gaze on the dark nights.
    OwlEye,
}

impl Spell {
    pub fn name(self) -> &'static str {
        match self {
            Spell::DeerForm => "Forme du cerf",
            Spell::OwlEye => "Œil de chouette",
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
    /// A new calf in the herd.
    Calf,
    /// A deer dead of hunger.
    Starved,
    /// The stag belling in the rut.
    Bell,
    /// A squirrel burying a nut.
    Cache,
    /// A squirrel digging a nut up again.
    Recovery,
    /// All of them walking the circle under the full moon.
    Rite,
    /// A spell understood.
    Learnt(Spell),
    /// The night of an anomaly, watched for in vain: nothing came.
    Unkept(Kind),
    /// A spell that no longer answers: the anomaly that taught it died.
    Lost(Spell),
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
            Entry::Calf => &["Un faon de plus."],
            Entry::Bell => &["Le brame, au loin."],
            Entry::Cache => &["Un écureuil enterre", "un gland. Un seul."],
            Entry::Recovery => &["Il creuse juste", "au bon endroit."],
            Entry::Starved => &["Une biche morte.", "Les côtes saillantes."],
            Entry::Learnt(Spell::DeerForm) => &["Je sais marcher", "comme elles."],
            Entry::Learnt(Spell::OwlEye) => &["Je vois dans le noir", "ce qu'elles voient."],
            Entry::Unkept(Kind::DeerRite) => &["La pleine lune,", "et personne au cercle."],
            Entry::Lost(Spell::DeerForm) => &["Je ne sais plus", "marcher comme elles."],
            Entry::Lost(Spell::OwlEye) => &["Le noir est revenu."],
        }
    }

    /// A title over the page, for a spell (learnt, or lost).
    pub fn title(self) -> Option<&'static str> {
        match self {
            Entry::Learnt(spell) | Entry::Lost(spell) => Some(spell.name()),
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

crate::save::persist_enum!(Spell { DeerForm, OwlEye });

impl crate::save::Persist for Entry {
    fn write(&self, w: &mut crate::save::Writer) {
        let (tag, cause, spell) = match *self {
            Entry::Found => (0u8, None, None),
            Entry::Herd => (1, None, None),
            Entry::Alarm => (2, None, None),
            Entry::Fled(c) => (3, Some(c), None),
            Entry::Ring => (4, None, None),
            Entry::Turning => (5, None, None),
            Entry::Calf => (6, None, None),
            Entry::Starved => (7, None, None),
            Entry::Rite => (8, None, None),
            Entry::Learnt(s) => (9, None, Some(s)),
            Entry::Bell => (10, None, None),
            Entry::Cache => (11, None, None),
            Entry::Recovery => (12, None, None),
            Entry::Unkept(_) => (13, None, None),
            Entry::Lost(s) => (14, None, Some(s)),
        };
        w.put(&tag);
        w.put(&cause);
        w.put(&spell);
        // The anomaly waited for, after the common fields.
        if let Entry::Unkept(kind) = *self {
            w.put(&kind);
        }
    }
    fn read(r: &mut crate::save::Reader) -> crate::save::Result<Self> {
        let tag: u8 = r.get()?;
        let cause: Option<Cause> = r.get()?;
        let spell: Option<Spell> = r.get()?;
        Ok(match tag {
            0 => Entry::Found,
            1 => Entry::Herd,
            2 => Entry::Alarm,
            3 => Entry::Fled(cause.ok_or("cause manquante")?),
            4 => Entry::Ring,
            5 => Entry::Turning,
            6 => Entry::Calf,
            7 => Entry::Starved,
            8 => Entry::Rite,
            9 => Entry::Learnt(spell.ok_or("sort manquant")?),
            10 => Entry::Bell,
            11 => Entry::Cache,
            12 => Entry::Recovery,
            13 => Entry::Unkept(r.get()?),
            14 => Entry::Lost(spell.ok_or("sort manquant")?),
            _ => return Err("page inconnue".into()),
        })
    }
}

crate::save::persist_struct!(Page {
    entry,
    day,
    hour,
    moon,
    wind,
    at
});
crate::save::persist_struct!(Notebook { pages, found_at });

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

    #[test]
    fn lost_spells_and_empty_nights_are_written_and_kept() {
        let entries = [
            Entry::Fled(Cause::Scent),
            Entry::Learnt(Spell::OwlEye),
            Entry::Unkept(Kind::DeerRite),
            Entry::Lost(Spell::DeerForm),
            Entry::Lost(Spell::OwlEye),
            Entry::Recovery,
        ];
        let mut book = Notebook::new(Vec2::ZERO);
        let now = crate::state::Conditions::noon();
        for entry in entries {
            assert!(book.write(entry, &now, Vec2::ONE).is_some());
            if !matches!(entry, Entry::Recovery | Entry::Fled(_)) {
                assert!(!entry.words().is_empty(), "{entry:?} has no words");
            }
        }
        assert_eq!(Entry::Lost(Spell::OwlEye).title(), Some("Œil de chouette"));
        let mut w = crate::save::Writer::default();
        w.put(&book);
        let back: Notebook = crate::save::Reader::new(&w.bytes)
            .get()
            .expect("a notebook");
        let kinds = |b: &Notebook| b.pages.iter().map(|p| p.entry).collect::<Vec<_>>();
        assert_eq!(kinds(&back), entries.to_vec());
    }
}
