//! Générateur pseudo-aléatoire à graine explicite.
//! Toute aléa de la simulation passe par lui : même graine, même suite de nombres, sur toute machine.

/// SplitMix64 : un état de 64 bits, avancé d'une constante à chaque tirage puis mélangé.
/// Rapide, de bonne qualité statistique pour nos besoins (germes, bruit), et trivial à reproduire.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// L'état interne : `SplitMix64::new(g.state())` reprend la suite exactement où `g` en
    /// est (sauvegarde d'une partie).
    pub fn state(&self) -> u64 {
        self.state
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Flottant uniforme dans [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        // Les 24 bits de poids fort : exactement représentables en f32.
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Entier uniforme dans [0, n). Le léger biais du modulo est négligeable pour n petit.
    pub fn next_below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn floats_in_unit_interval() {
        let mut rng = SplitMix64::new(1);
        for _ in 0..10_000 {
            let x = rng.next_f32();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
