//! Saving a game: a small binary format written by hand, no dependency.
//!
//! The world is generated again from its seed, then what changed is laid back over it: the
//! voxels, the water, the plants and their lives, the soil, the herd, the objects, the
//! players, the notebook and its sketches. Each type says how it writes and reads itself
//! (`Persist`); plain structs list their fields with `persist_struct!`.
//!
//! Format: the magic `DSPF`, a version, the seed, then the fields in order, little-endian.
//! A save from another version or another seed is not read (the game starts afresh).

use std::collections::HashMap;
use std::path::PathBuf;

use glam::{Vec2, Vec3};

/// Bumped whenever the layout changes: older saves are then ignored.
pub const VERSION: u32 = 5;
const MAGIC: &[u8; 4] = b"DSPF";

#[derive(Default)]
pub struct Writer {
    pub bytes: Vec<u8>,
}

pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

pub type Result<T> = std::result::Result<T, String>;

impl Writer {
    pub fn put<T: Persist>(&mut self, value: &T) {
        value.write(self);
    }

    fn raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// Bytes compressed by runs (a voxel grid is mostly long runs of air or stone).
    pub fn runs(&mut self, data: &[u8]) {
        let mut runs: Vec<(u32, u8)> = Vec::new();
        for &b in data {
            match runs.last_mut() {
                Some((n, v)) if *v == b && *n < u32::MAX => *n += 1,
                _ => runs.push((1, b)),
            }
        }
        self.put(&(runs.len() as u64));
        for (n, v) in runs {
            self.put(&n);
            self.put(&v);
        }
    }
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub fn get<T: Persist>(&mut self) -> Result<T> {
        T::read(self)
    }

    fn raw(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or("sauvegarde tronquée")?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    pub fn runs(&mut self) -> Result<Vec<u8>> {
        let count: u64 = self.get()?;
        let mut data = Vec::new();
        for _ in 0..count {
            let n: u32 = self.get()?;
            let v: u8 = self.get()?;
            if data.len() + n as usize > 1 << 28 {
                return Err("sauvegarde invalide".into());
            }
            data.extend(std::iter::repeat_n(v, n as usize));
        }
        Ok(data)
    }
}

/// A value that can be written to a save and read back.
pub trait Persist: Sized {
    fn write(&self, w: &mut Writer);
    fn read(r: &mut Reader) -> Result<Self>;
}

macro_rules! persist_number {
    ($($t:ty),*) => {$(
        impl Persist for $t {
            fn write(&self, w: &mut Writer) {
                w.raw(&self.to_le_bytes());
            }
            fn read(r: &mut Reader) -> Result<Self> {
                let bytes = r.raw(std::mem::size_of::<$t>())?;
                Ok(<$t>::from_le_bytes(bytes.try_into().map_err(|_| "nombre")?))
            }
        }
    )*};
}
persist_number!(u8, u16, u32, u64, i32, i64, f32, f64);

impl Persist for usize {
    fn write(&self, w: &mut Writer) {
        w.put(&(*self as u64));
    }
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(r.get::<u64>()? as usize)
    }
}

impl Persist for bool {
    fn write(&self, w: &mut Writer) {
        w.put(&(*self as u8));
    }
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(r.get::<u8>()? != 0)
    }
}

impl Persist for Vec2 {
    fn write(&self, w: &mut Writer) {
        w.put(&self.x);
        w.put(&self.y);
    }
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(Vec2::new(r.get()?, r.get()?))
    }
}

impl Persist for Vec3 {
    fn write(&self, w: &mut Writer) {
        w.put(&self.x);
        w.put(&self.y);
        w.put(&self.z);
    }
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(Vec3::new(r.get()?, r.get()?, r.get()?))
    }
}

impl<T: Persist> Persist for Vec<T> {
    fn write(&self, w: &mut Writer) {
        w.put(&self.len());
        for v in self {
            w.put(v);
        }
    }
    fn read(r: &mut Reader) -> Result<Self> {
        let n: usize = r.get()?;
        if n > 1 << 26 {
            return Err("sauvegarde invalide".into());
        }
        (0..n).map(|_| r.get()).collect()
    }
}

impl<T: Persist> Persist for Option<T> {
    fn write(&self, w: &mut Writer) {
        match self {
            None => w.put(&0u8),
            Some(v) => {
                w.put(&1u8);
                w.put(v);
            }
        }
    }
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(match r.get::<u8>()? {
            0 => None,
            _ => Some(r.get()?),
        })
    }
}

impl<T: Persist + Copy + Default, const N: usize> Persist for [T; N] {
    fn write(&self, w: &mut Writer) {
        for v in self {
            w.put(v);
        }
    }
    fn read(r: &mut Reader) -> Result<Self> {
        let mut out = [T::default(); N];
        for v in &mut out {
            *v = r.get()?;
        }
        Ok(out)
    }
}

impl<A: Persist, B: Persist> Persist for (A, B) {
    fn write(&self, w: &mut Writer) {
        w.put(&self.0);
        w.put(&self.1);
    }
    fn read(r: &mut Reader) -> Result<Self> {
        Ok((r.get()?, r.get()?))
    }
}

impl<A: Persist, B: Persist, C: Persist> Persist for (A, B, C) {
    fn write(&self, w: &mut Writer) {
        w.put(&self.0);
        w.put(&self.1);
        w.put(&self.2);
    }
    fn read(r: &mut Reader) -> Result<Self> {
        Ok((r.get()?, r.get()?, r.get()?))
    }
}

impl<K: Persist + std::hash::Hash + Eq + Ord + Copy, V: Persist + Clone> Persist for HashMap<K, V> {
    fn write(&self, w: &mut Writer) {
        // In key order: the same state always writes the same bytes.
        let mut keys: Vec<&K> = self.keys().collect();
        keys.sort();
        w.put(&keys.len());
        for k in keys {
            w.put(k);
            w.put(&self[k]);
        }
    }
    fn read(r: &mut Reader) -> Result<Self> {
        let pairs: Vec<(K, V)> = r.get()?;
        Ok(pairs.into_iter().collect())
    }
}

/// `Persist` for a struct, field by field (all fields, in this order).
macro_rules! persist_struct {
    ($t:ty { $($field:ident),* $(,)? }) => {
        impl $crate::save::Persist for $t {
            fn write(&self, w: &mut $crate::save::Writer) {
                $(w.put(&self.$field);)*
            }
            fn read(r: &mut $crate::save::Reader) -> $crate::save::Result<Self> {
                Ok(Self { $($field: r.get()?,)* })
            }
        }
    };
}
pub(crate) use persist_struct;

/// `Persist` for a fieldless enum, by its index in the list.
macro_rules! persist_enum {
    ($t:ty { $($variant:ident),* $(,)? }) => {
        impl $crate::save::Persist for $t {
            fn write(&self, w: &mut $crate::save::Writer) {
                let all = [$(<$t>::$variant),*];
                let k = all.iter().position(|v| v == self).unwrap_or(0) as u8;
                w.put(&k);
            }
            fn read(r: &mut $crate::save::Reader) -> $crate::save::Result<Self> {
                let all = [$(<$t>::$variant),*];
                let k: u8 = r.get()?;
                all.get(k as usize).copied().ok_or_else(|| "variante inconnue".into())
            }
        }
    };
}
pub(crate) use persist_enum;

/// Starts a save: magic, version, seed.
pub fn header(seed: u64) -> Writer {
    let mut w = Writer::default();
    w.raw(MAGIC);
    w.put(&VERSION);
    w.put(&seed);
    w
}

/// Checks the header of a save for `seed`, and returns a reader past it.
pub fn open(bytes: &[u8], seed: u64) -> Result<Reader<'_>> {
    let mut r = Reader::new(bytes);
    if r.raw(4)? != MAGIC {
        return Err("ce n'est pas une sauvegarde".into());
    }
    let version: u32 = r.get()?;
    if version != VERSION {
        return Err(format!("sauvegarde d'une autre version ({version})"));
    }
    if r.get::<u64>()? != seed {
        return Err("sauvegarde d'un autre monde".into());
    }
    Ok(r)
}

/// Where saves go: the user's application data folder, one file per world seed.
pub fn path(seed: u64) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/Dissipatif"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Dissipatif"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or(home.map(|h| h.join(".local/share")))
            .map(|d| d.join("dissipatif"))
    };
    base.unwrap_or_else(|| PathBuf::from("saves"))
        .join(format!("monde-{seed}.sav"))
}

/// Writes `bytes` to `path` safely: to a temporary file first, then renamed over the old save
/// (a crash while writing never leaves a half-written save).
pub fn write_file(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

// ---- Types of other crates ----

impl Persist for sim::rng::SplitMix64 {
    fn write(&self, w: &mut Writer) {
        w.put(&self.state());
    }
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(sim::rng::SplitMix64::new(r.get()?))
    }
}

impl Persist for world::Plant {
    fn write(&self, w: &mut Writer) {
        let k = world::Plant::ALL
            .iter()
            .position(|p| p == self)
            .unwrap_or(0) as u8;
        w.put(&k);
    }
    fn read(r: &mut Reader) -> Result<Self> {
        let k: u8 = r.get()?;
        world::Plant::ALL
            .get(k as usize)
            .copied()
            .ok_or_else(|| "plante inconnue".into())
    }
}

persist_struct!(world::PlantInstance {
    plant,
    variant,
    rotation,
    mirrored,
    scale,
    base,
    offset,
});

impl Persist for world::WorldState {
    fn write(&self, w: &mut Writer) {
        w.put(&self.ground);
        w.put(&self.water);
        w.put(&self.tops);
        w.runs(&self.blocks);
        w.put(&self.micro);
        w.put(&self.plants);
    }
    fn read(r: &mut Reader) -> Result<Self> {
        Ok(world::WorldState {
            ground: r.get()?,
            water: r.get()?,
            tops: r.get()?,
            blocks: r.runs()?,
            micro: r.get()?,
            plants: r.get()?,
        })
    }
}

persist_struct!(sim::thermal::Fuel {
    mass,
    ignition,
    heat_value,
    burn_rate,
    ash_share,
    retained,
    heated_share,
});

persist_struct!(sim::thermal::Body {
    position,
    radius,
    mass,
    specific_heat,
    emissivity,
    temperature,
    water,
    fuel,
    ash,
    burning,
    power,
    enclosure,
    draft,
});

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Sample {
        a: u32,
        b: f32,
        c: Vec<Option<Vec2>>,
        d: [u8; 3],
    }
    persist_struct!(Sample { a, b, c, d });

    #[test]
    fn values_come_back_as_they_went() {
        let sample = Sample {
            a: 7,
            b: -1.5,
            c: vec![None, Some(Vec2::new(1.0, 2.0))],
            d: [1, 2, 3],
        };
        let mut w = header(42);
        w.put(&sample);
        let grid = vec![0u8; 1000]
            .into_iter()
            .chain([5, 5, 7])
            .collect::<Vec<_>>();
        w.runs(&grid);
        let bytes = w.bytes;
        let mut r = open(&bytes, 42).expect("our own save");
        assert_eq!(r.get::<Sample>().expect("sample"), sample);
        assert_eq!(r.runs().expect("grid"), grid);
        assert!(open(&bytes, 43).is_err());
        assert!(Reader::new(&bytes[..10]).get::<Sample>().is_err());
    }
}
