//! Oregonator (réduction de Tyson-Fife) : modèle de la réaction de Belooussov-Jabotinski.
//! Fiche : `docs/reactions/oregonator.md`.
//!
//! ```text
//! ∂u/∂t = (1/ε) [ u (1 − u) − f v (u − q) / (u + q) ] + Du ∇²u
//! ∂v/∂t = u − v                                        + Dv ∇²v
//! ```
//!
//! - Intégration : Euler explicite, double tampon.
//! - Laplacien : 7 points avec un pas d'espace `dx` (∇²u ≈ (Σ voisines − 6 u) / dx²). Une
//!   boîte de Petri est une couche (ny = 1) : les voisines en y sont la cellule elle-même.
//! - Bords : étanches (paroi de la boîte, Neumann).
//!
//! EXERCICE : `step` est à écrire. Les tests marqués `#[ignore]` décrivent ce qu'il doit faire.

use crate::grid::{Dims, Field3};

/// Paramètres de l'Oregonator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OregonatorParams {
    /// ε : rapport des temps propres de u (rapide) et v (lent). Petit = raide.
    pub epsilon: f32,
    /// q : constante de vitesse réduite (fixe le seuil d'excitation, très près de 0).
    pub q: f32,
    /// f : facteur stœchiométrique (bromure libéré par catalyseur réduit).
    pub stoichiometry: f32,
    /// Du : diffusion de l'activateur.
    pub diffusion_u: f32,
    /// Dv : diffusion de l'inhibiteur (0 si le catalyseur est fixé dans un gel).
    pub diffusion_v: f32,
    /// Pas d'espace.
    pub dx: f32,
    /// Pas de temps.
    pub dt: f32,
}

impl OregonatorParams {
    /// Régime excitable de référence (cibles et spirales), d'après la littérature
    /// (q = 0,002, f = 1,4) ; ε = 0,05 pour un système modérément raide.
    pub const fn reference() -> Self {
        Self {
            epsilon: 0.05,
            q: 0.002,
            stoichiometry: 1.4,
            diffusion_u: 1.0,
            diffusion_v: 0.0,
            dx: 0.5,
            dt: 0.0004,
        }
    }

    /// Vérifie les deux conditions de stabilité d'Euler explicite (voir la fiche) :
    /// - diffusion : D·dt/dx² ≤ 1/6 (couche ou volume, on garde la plus stricte) ;
    /// - réaction : dt plus petit que le temps le plus court du terme de réaction, estimé à
    ///   ε · 2q / (f · v_max) avec v_max ≈ 0,3 (raideur près de u ≈ q).
    pub fn check_stability(&self) -> Result<(), String> {
        for (name, d) in [("Du", self.diffusion_u), ("Dv", self.diffusion_v)] {
            let number = d * self.dt / (self.dx * self.dx);
            if number > 1.0 / 6.0 {
                return Err(format!(
                    "{name}·dt/dx² = {number} > 1/6 : diffusion instable"
                ));
            }
        }
        let reaction_time = self.epsilon * 2.0 * self.q / (self.stoichiometry * 0.3);
        if self.dt > reaction_time {
            return Err(format!(
                "dt = {} > {reaction_time:.2e} : réaction trop raide pour Euler explicite",
                self.dt
            ));
        }
        Ok(())
    }
}

/// État d'une couche de réaction BZ : activateur u, inhibiteur v (le catalyseur oxydé, celui
/// qu'on voit), plus les tampons du pas suivant.
pub struct Oregonator {
    pub params: OregonatorParams,
    u: Field3,
    v: Field3,
    // Lus par `step` une fois l'exercice écrit ; retirer alors cet attribut.
    #[allow(dead_code)]
    u_next: Field3,
    #[allow(dead_code)]
    v_next: Field3,
}

impl Oregonator {
    /// Couche au repos (u, v) = (0, 0). Panique si les paramètres sont instables.
    pub fn new(dims: Dims, params: OregonatorParams) -> Self {
        if let Err(message) = params.check_stability() {
            panic!("{message}");
        }
        Self {
            params,
            u: Field3::filled(dims, 0.0),
            v: Field3::filled(dims, 0.0),
            u_next: Field3::filled(dims, 0.0),
            v_next: Field3::filled(dims, 0.0),
        }
    }

    pub fn dims(&self) -> Dims {
        self.u.dims
    }

    /// Activateur (HBrO₂).
    pub fn u(&self) -> &Field3 {
        &self.u
    }

    /// Inhibiteur : le catalyseur oxydé (Ce⁴⁺ jaune, Mn³⁺ rouge). C'est la couleur visible.
    pub fn v(&self) -> &Field3 {
        &self.v
    }

    /// Déclenche une flambée : u = 1 dans le disque de centre (cx, cz) et de rayon `radius`
    /// cellules (couche y = 0). Comme une pointe d'aiguille plongée dans la boîte.
    pub fn excite(&mut self, cx: f32, cz: f32, radius: f32) {
        let dims = self.dims();
        for z in 0..dims.nz {
            for x in 0..dims.nx {
                let (dx, dz) = (x as f32 + 0.5 - cx, z as f32 + 0.5 - cz);
                if dx * dx + dz * dz <= radius * radius {
                    let i = dims.index(x, 0, z);
                    self.u.data[i] = 1.0;
                }
            }
        }
    }

    /// Avance d'un pas de temps dt. Aucune allocation.
    ///
    /// EXERCICE : à écrire. Lire u, v (état n), écrire u_next, v_next (état n + 1), puis
    /// échanger les tampons. Bords étanches. Voir la fiche pour les équations et la raideur.
    pub fn step(&mut self) {
        todo!("exercice : écrire le pas de l'Oregonator (docs/reactions/oregonator.md)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(n: usize) -> Oregonator {
        Oregonator::new(
            Dims {
                nx: n,
                ny: 1,
                nz: 1,
            },
            OregonatorParams::reference(),
        )
    }

    #[test]
    fn reference_parameters_are_stable_and_too_large_a_step_is_refused() {
        assert!(OregonatorParams::reference().check_stability().is_ok());
        let p = OregonatorParams {
            dt: 0.01,
            ..OregonatorParams::reference()
        };
        assert!(p.check_stability().is_err());
    }

    #[test]
    fn excite_sets_the_activator_in_a_disc() {
        let mut sim = Oregonator::new(
            Dims {
                nx: 20,
                ny: 1,
                nz: 20,
            },
            OregonatorParams::reference(),
        );
        sim.excite(10.0, 10.0, 2.0);
        assert_eq!(sim.u().get(10, 0, 10), 1.0);
        assert_eq!(sim.u().get(15, 0, 10), 0.0);
    }

    // ---------- Exercices : `cargo test -p sim -- --ignored` ----------

    /// Le repos (0, 0) reste au repos : chaque terme s'annule.
    #[test]
    #[ignore = "exercice : step de l'Oregonator"]
    fn rest_stays_at_rest() {
        let mut sim = strip(50);
        for _ in 0..1000 {
            sim.step();
        }
        assert!(sim.u().data.iter().all(|&u| u == 0.0));
        assert!(sim.v().data.iter().all(|&v| v == 0.0));
    }

    /// Une flambée au bout d'une bande déclenche une onde qui la parcourt, puis la zone de
    /// départ revient au repos (elle a été réfractaire, puis s'est remise).
    #[test]
    #[ignore = "exercice : step de l'Oregonator"]
    fn a_kick_starts_a_wave_that_travels_then_rest_returns() {
        let n = 240;
        let mut sim = strip(n);
        sim.excite(2.0, 0.5, 4.0);
        let far = 160;
        let mut reached = false;
        let steps = (20.0 / sim.params.dt) as usize;
        for _ in 0..steps {
            sim.step();
            reached |= sim.u().get(far, 0, 0) > 0.5;
        }
        assert!(reached, "l'onde n'a pas atteint la cellule {far}");
        for x in 0..10 {
            assert!(sim.u().get(x, 0, 0) < 0.1, "le départ ne s'est pas remis");
        }
        for (&u, &v) in sim.u().data.iter().zip(&sim.v().data) {
            assert!(
                u.is_finite() && (-1e-3..=1.0 + 1e-3).contains(&u),
                "u = {u}"
            );
            assert!(v.is_finite() && (-1e-3..=1.0).contains(&v), "v = {v}");
        }
    }

    /// Même départ, même résultat, bit à bit.
    #[test]
    #[ignore = "exercice : step de l'Oregonator"]
    fn deterministic() {
        let run = || {
            let mut sim = strip(60);
            sim.excite(3.0, 0.5, 3.0);
            for _ in 0..4000 {
                sim.step();
            }
            (sim.u().data.clone(), sim.v().data.clone())
        };
        assert_eq!(run(), run());
    }
}
