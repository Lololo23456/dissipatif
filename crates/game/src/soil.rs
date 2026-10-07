//! The soil: the slow variables of the ecosystem, by patches of `PATCH` × `PATCH` cells.
//!
//! Plants and animals change fast (days); the soil slowly (weeks). Its state decides what can
//! grow, and what grows decides its state: a loop, with thresholds. Two variables per patch:
//!
//! - **W, water held in the soil**: rain P (the climate, the nearness of water) soaks in
//!   better under plants (roots, litter, shade) than on bare ground, where it runs off;
//!   it evaporates, and plants draw it.
//! - **N, organic matter**: litter and roots build it up under a cover, it washes away from
//!   bare ground; slowly (a month of game).
//!
//! ```text
//! infiltration  f(B) = (B + k·w₀) / (B + k)            w₀: share soaking into bare ground
//! dW/dt = P f(B) − e W − u B W
//! dN/dt = λ (N*(B) − N),   N*(B) = n₀ + (1 − n₀) min(B, 1)
//! ```
//!
//! B is the plant cover of the patch (from the living plants, fast). What the soil offers
//! the plants (their habitat is multiplied by it) is
//!
//! ```text
//! S(W, N) = W⁵ / (W⁵ + W_c⁵) · (0.2 + 0.8 N),   W_c = 0.5 P + 0.04
//! ```
//!
//! the threshold relative to the patch's rain (each climate has its own covered and bare
//! states; the match of species to climate is in the habitat).
//!
//! a steep threshold in water. With the infiltration feedback, a patch has **two stable
//! states** over a range of conditions: covered (water soaks in, plants thrive) and bare
//! (water runs off, nothing takes). Pushed past a threshold (overgrazing, repeated fires,
//! clearing), it tips into the bare state and stays there even when the pressure stops: it
//! must drop much lower before the cover comes back (hysteresis); on dry land, never.
//! Near the threshold, it recovers more and more slowly from a disturbance (critical
//! slowing down). After Rietkerk et al. (2002) and Noy-Meir (1975); see
//! `docs/reactions/ecologie.md`.

use glam::Vec2;

/// Side of a soil patch, in cells.
pub const PATCH: usize = 8;
/// Infiltration: half-saturation in cover, and the share soaking into bare ground.
const K_INFILTRATION: f32 = 0.15;
const BARE_INFILTRATION: f32 = 0.1;
/// Evaporation, and uptake by plants (per game day).
const EVAPORATION: f32 = 0.6;
const UPTAKE: f32 = 0.4;
/// Organic matter: how fast it follows the cover (per game day), and what bare ground keeps.
const HUMUS_RATE: f32 = 0.03;
const BARE_HUMUS: f32 = 0.3;
/// Water at which plants get half of what they could, as a share of the rain, plus a floor.
const WATER_HALF: f32 = 0.5;
const WATER_FLOOR: f32 = 0.04;
/// Water seeping between neighbouring patches (per game day).
const SEEPAGE: f32 = 0.1;

/// Infiltration f(B): the share of the rain that soaks in, from bare ground to full cover.
pub fn infiltration(cover: f32) -> f32 {
    (cover + K_INFILTRATION * BARE_INFILTRATION) / (cover + K_INFILTRATION)
}

/// Organic matter a cover B builds up in the long run.
pub fn humus_target(cover: f32) -> f32 {
    BARE_HUMUS + (1.0 - BARE_HUMUS) * cover.min(1.0)
}

/// Water held in the long run under rain P and cover B: dW/dt = 0.
pub fn water_target(rain: f32, cover: f32) -> f32 {
    rain * infiltration(cover) / (EVAPORATION + UPTAKE * cover)
}

/// What the soil offers the plants, in [0, 1], under rain P.
pub fn offer(water: f32, humus: f32, rain: f32) -> f32 {
    let half = WATER_HALF * rain + WATER_FLOOR;
    let w5 = water.powi(5);
    (w5 / (w5 + half.powi(5)) * (0.2 + 0.8 * humus)).clamp(0.0, 1.0)
}

/// One soil patch's step of `days`: water and organic matter under rain P and cover B.
/// `evaporation`: a factor for the season (1 in summer, less in the cold).
pub fn step(water: &mut f32, humus: &mut f32, rain: f32, cover: f32, days: f32, evaporation: f32) {
    *water += (rain * infiltration(cover)
        - EVAPORATION * evaporation * *water
        - UPTAKE * evaporation * cover * *water)
        * days;
    *water = water.max(0.0);
    *humus += HUMUS_RATE * (humus_target(cover) - *humus) * days;
}

pub struct Soil {
    nx: usize,
    nz: usize,
    /// Rain reaching each patch (the climate and the nearness of water), in [0, 1].
    rain: Vec<f32>,
    water: Vec<f32>,
    humus: Vec<f32>,
    /// Plant cover, recomputed from the plants at each life step.
    cover: Vec<f32>,
    /// What grazers find to eat there.
    forage: Vec<f32>,
    /// What the soil offered at the start: the generated world is at its balance, and what
    /// the plants get is measured against it.
    reference: Vec<f32>,
    /// The plant cover at the start: a patch is bare when it has lost most of it.
    cover_start: Vec<f32>,
}

impl Soil {
    /// Patches over a world of `nx` × `nz` columns; `moisture(x, z)` the climate's moisture of
    /// a column. Water and organic matter start at their balance with the `cover` there.
    pub fn new(nx: usize, nz: usize, moisture: impl Fn(usize, usize) -> f32) -> Self {
        let (px, pz) = (nx.div_ceil(PATCH), nz.div_ceil(PATCH));
        let mut rain = vec![0.0; px * pz];
        for (k, r) in rain.iter_mut().enumerate() {
            let (a, b) = (k % px, k / px);
            let (mut sum, mut count) = (0.0, 0.0);
            for z in b * PATCH..((b + 1) * PATCH).min(nz) {
                for x in a * PATCH..((a + 1) * PATCH).min(nx) {
                    sum += moisture(x, z);
                    count += 1.0;
                }
            }
            *r = sum / count;
        }
        Self {
            nx: px,
            nz: pz,
            water: vec![0.0; px * pz],
            humus: vec![1.0; px * pz],
            cover: vec![0.0; px * pz],
            forage: vec![0.0; px * pz],
            reference: vec![1.0; px * pz],
            cover_start: vec![0.0; px * pz],
            rain,
        }
    }

    /// Saves what the soil remembers (water, organic matter, its starting offer).
    pub fn save(&self, w: &mut crate::save::Writer) {
        w.put(&self.water);
        w.put(&self.humus);
        w.put(&self.reference);
        w.put(&self.cover_start);
    }

    /// Reads back what `save` wrote, over a soil built for the same world.
    pub fn load(&mut self, r: &mut crate::save::Reader) -> crate::save::Result<()> {
        let (water, humus, reference): (Vec<f32>, Vec<f32>, Vec<f32>) =
            (r.get()?, r.get()?, r.get()?);
        let n = self.rain.len();
        if water.len() != n || humus.len() != n || reference.len() != n {
            return Err("sol de taille différente".into());
        }
        (self.water, self.humus, self.reference) = (water, humus, reference);
        let cover_start: Vec<f32> = r.get()?;
        if cover_start.len() != n {
            return Err("sol de taille différente".into());
        }
        self.cover_start = cover_start;
        Ok(())
    }

    pub fn patch(&self, at: Vec2) -> Option<usize> {
        let (x, z) = (at.x / PATCH as f32, at.y / PATCH as f32);
        (x >= 0.0 && z >= 0.0 && (x as usize) < self.nx && (z as usize) < self.nz)
            .then(|| x as usize + self.nx * z as usize)
    }

    /// Centre of patch `k`, in cells.
    pub fn centre(&self, k: usize) -> Vec2 {
        Vec2::new(
            ((k % self.nx) as f32 + 0.5) * PATCH as f32,
            ((k / self.nx) as f32 + 0.5) * PATCH as f32,
        )
    }

    pub fn patches(&self) -> usize {
        self.rain.len()
    }

    /// Sets the cover and forage of each patch (from the plants), each in `[0, ∞)`.
    pub fn set_cover(&mut self, cover: &[f32], forage: &[f32]) {
        self.cover.copy_from_slice(cover);
        self.forage.copy_from_slice(forage);
    }

    /// Water and organic matter at their balance with the present cover (at the start).
    pub fn settle(&mut self) {
        for k in 0..self.rain.len() {
            self.water[k] = water_target(self.rain[k], self.cover[k]);
            self.humus[k] = humus_target(self.cover[k]);
            self.reference[k] = offer(self.water[k], self.humus[k], self.rain[k]).max(0.05);
            self.cover_start[k] = self.cover[k];
        }
    }

    /// `evaporation`: the season's factor (see `season::evaporation`).
    pub fn step(&mut self, days: f32, evaporation: f32) {
        for k in 0..self.rain.len() {
            step(
                &mut self.water[k],
                &mut self.humus[k],
                self.rain[k],
                self.cover[k],
                days,
                evaporation,
            );
        }
        // Water seeps sideways between patches (a 5-point Laplacian, double-buffered): a bare
        // spot amid a cover is fed by its neighbours and closes again from its edges; a wide
        // bare land is not.
        let (nx, nz) = (self.nx, self.nz);
        let before = self.water.clone();
        let rate = (SEEPAGE * days).min(0.2);
        for z in 0..nz {
            for x in 0..nx {
                let k = x + nx * z;
                let mut sum = 0.0;
                let mut count = 0.0;
                // Across the edges too: the world closes on itself.
                for (a, b) in [
                    ((x + nx - 1) % nx, z),
                    ((x + 1) % nx, z),
                    (x, (z + nz - 1) % nz),
                    (x, (z + 1) % nz),
                ] {
                    sum += before[a + nx * b];
                    count += 1.0;
                }
                self.water[k] += rate * (sum - count * before[k]);
            }
        }
    }

    /// What the soil offers the plants at `at`, against what it offered at the start (1: as
    /// then; less once it has dried and thinned; a little more if it has been enriched).
    pub fn offer_at(&self, at: Vec2) -> f32 {
        self.patch(at).map_or(0.0, |k| {
            (offer(self.water[k], self.humus[k], self.rain[k]) / self.reference[k]).min(1.2)
        })
    }

    /// Ashes: a fire returns organic matter to the soil at once (a flush, before the bare
    /// ground loses it).
    pub fn ash(&mut self, at: Vec2, amount: f32) {
        if let Some(k) = self.patch(at) {
            self.humus[k] = (self.humus[k] + amount).min(1.2);
        }
    }

    /// Rain reaching patch `k`, in [0, 1].
    #[cfg(test)]
    pub fn rain(&self, k: usize) -> f32 {
        self.rain[k]
    }

    /// How bare patch `k` has grown, 0 (as at the start) to 1 (stripped): it shows from
    /// below 60 % of its starting cover.
    pub fn bareness(&self, k: usize) -> f32 {
        let start = self.cover_start[k];
        if start < 0.1 {
            return 0.0;
        }
        (1.0 - self.cover[k] / start / 0.6).clamp(0.0, 1.0)
    }

    pub fn cover(&self, k: usize) -> f32 {
        self.cover[k]
    }

    pub fn forage(&self, k: usize) -> f32 {
        self.forage[k]
    }

    #[cfg(test)]
    pub fn state(&self, k: usize) -> (f32, f32) {
        (self.water[k], self.humus[k])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mean field of one patch: cover B growing logistically up to what the soil offers,
    /// grazed at pressure G with a saturating intake (Holling type II).
    fn settle(rain: f32, grazing: f32, state: (f32, f32, f32)) -> (f32, f32, f32) {
        let (mut b, mut w, mut n) = state;
        let dt = 0.05;
        for _ in 0..(300.0 / dt) as usize {
            let k = offer(w, n, rain).max(1e-3);
            b += (2.0 * b * (1.0 - b / k) - grazing * b / (b + 0.35)) * dt;
            b = b.max(1e-3);
            step(&mut w, &mut n, rain, b, dt, 1.0);
        }
        (b, w, n)
    }

    #[test]
    fn a_grazed_meadow_tips_over_and_alone_does_not_come_back() {
        let rain = 0.5;
        let mut state = (0.9, water_target(rain, 0.9), humus_target(0.9));
        let pressures = [0.0, 0.2, 0.4, 0.6, 0.8, 0.6, 0.4, 0.2, 0.0];
        let covers: Vec<f32> = pressures
            .iter()
            .map(|&g| {
                state = settle(rain, g, state);
                state.0
            })
            .collect();
        // Up: the cover thins, then collapses past the threshold.
        assert!(covers[3] > 0.3, "{covers:?}");
        assert!(covers[4] < 0.02, "{covers:?}");
        // Down: at the same pressures as on the way up, still bare (two stable states), and
        // even when grazing stops: alone, the bare patch keeps its state.
        assert!(covers[5..].iter().all(|&c| c < 0.02), "{covers:?}");
    }

    #[test]
    fn a_bare_spot_amid_a_meadow_gets_water_from_around_a_bare_land_does_not() {
        let soil_with = |covered_around: bool| {
            let mut soil = Soil::new(5 * PATCH, 5 * PATCH, |_, _| 0.5);
            let mut cover = vec![if covered_around { 0.9 } else { 0.0 }; 25];
            cover[12] = 0.0;
            soil.set_cover(&cover, &[0.0; 25]);
            soil.settle();
            for _ in 0..100 {
                soil.step(0.2, 1.0);
            }
            soil.state(12).0
        };
        let (amid, alone) = (soil_with(true), soil_with(false));
        // Enough water amid the meadow for plants to take again (slowly), not in the bare
        // land.
        assert!(offer(amid, 0.6, 0.5) > 0.1, "{amid}");
        assert!(offer(alone, 0.6, 0.5) < 0.02, "{alone}");
    }

    #[test]
    fn dry_land_once_bare_stays_bare() {
        let rain = 0.35;
        let state = settle(rain, 0.8, (0.6, water_target(rain, 0.6), humus_target(0.6)));
        assert!(state.0 < 0.02);
        let after = settle(rain, 0.0, state);
        assert!(after.0 < 0.02, "{after:?}");
    }
}
