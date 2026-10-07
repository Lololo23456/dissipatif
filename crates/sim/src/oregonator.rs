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

    /// Les deux champs (u, v), pour les dessiner ou les sauvegarder.
    pub fn fields(&self) -> (&Field3, &Field3) {
        (&self.u, &self.v)
    }

    /// Écrit u et v dans la cellule (x, 0, z) : pour amorcer une figure (une onde rompue qui
    /// s'enroulera en spirale, une zone réfractaire) ou reprendre un état sauvegardé, cellule
    /// par cellule. Aucun calcul.
    pub fn set_cell(&mut self, x: usize, z: usize, u: f32, v: f32) {
        let i = self.dims().index(x, 0, z);
        self.u.data[i] = u;
        self.v.data[i] = v;
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
    fn set_cell_writes_one_cell_and_fields_read_both() {
        let mut sim = Oregonator::new(
            Dims {
                nx: 8,
                ny: 1,
                nz: 6,
            },
            OregonatorParams::reference(),
        );
        sim.set_cell(5, 2, 0.8, 0.15);
        let (u, v) = sim.fields();
        assert_eq!((u.get(5, 0, 2), v.get(5, 0, 2)), (0.8, 0.15));
        let touched = |f: &Field3| f.data.iter().filter(|&&a| a != 0.0).count();
        assert_eq!((touched(u), touched(v)), (1, 1), "other cells written");
        assert_eq!(u.data, sim.u().data);
        assert_eq!(v.data, sim.v().data);
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

    /// Un milieu excitable : un repos stable, qu'une poussée au-delà du seuil fait flamber
    /// avant qu'il ne revienne au repos. f = 2,6 ; la réaction, plus raide, demande un pas plus
    /// petit (voir `check_stability`). `lab bz-wave` situe le régime d'autres valeurs de f.
    fn excitable() -> OregonatorParams {
        OregonatorParams {
            stoichiometry: 2.6,
            dt: 0.00025,
            ..OregonatorParams::reference()
        }
    }

    /// Le repos homogène de ces paramètres : u* = v*, racine positive de
    /// u² + (f + q − 1) u − q (1 + f) = 0 (là, le terme de réaction s'annule).
    fn rest(p: &OregonatorParams) -> f32 {
        let b = p.stoichiometry + p.q - 1.0;
        (-b + (b * b + 4.0 * p.q * (1.0 + p.stoichiometry)).sqrt()) / 2.0
    }

    /// Une couche nx × nz au repos (u*, v*).
    fn at_rest(nx: usize, nz: usize, params: OregonatorParams) -> Oregonator {
        let mut sim = Oregonator::new(Dims { nx, ny: 1, nz }, params);
        let r = rest(&params);
        for z in 0..nz {
            for x in 0..nx {
                sim.set_cell(x, z, r, r);
            }
        }
        sim
    }

    /// Temps qu'il faut, à partir de maintenant, pour que la cellule `x` d'une bande flambe
    /// (u > 0,5), en faisant avancer `sim` jusque-là, au plus pendant `longest` unités de
    /// temps.
    fn reaches(sim: &mut Oregonator, x: usize, longest: f32) -> Option<f32> {
        let dt = sim.params.dt;
        let mut steps = 0;
        while sim.u().get(x, 0, 0) <= 0.5 {
            if steps as f32 * dt > longest {
                return None;
            }
            sim.step();
            steps += 1;
        }
        Some(steps as f32 * dt)
    }

    /// La diffusion seule (réaction rendue négligeable : ε immense, f = 0) étale une
    /// impulsion de façon symétrique et, entre des parois étanches, n'en perd rien : ni au
    /// milieu, ni contre un bord.
    #[test]
    #[ignore = "exercice : step de l'Oregonator"]
    fn diffusion_alone_spreads_symmetrically_and_is_kept_between_sealed_walls() {
        let params = OregonatorParams {
            epsilon: 1e9,
            stoichiometry: 0.0,
            dt: 0.02,
            ..OregonatorParams::reference()
        };
        let n = 21;
        let mass = |sim: &Oregonator| sim.u().data.iter().map(|&u| u as f64).sum::<f64>();
        // Au milieu : quatre symétries.
        let mut middle = Oregonator::new(Dims { nx: n, ny: 1, nz: n }, params);
        middle.set_cell(10, 10, 1.0, 0.0);
        // Contre la paroi de gauche : une seule, de part et d'autre de z = 10.
        let mut wall = Oregonator::new(Dims { nx: n, ny: 1, nz: n }, params);
        wall.set_cell(1, 10, 1.0, 0.0);
        for _ in 0..400 {
            middle.step();
            wall.step();
        }
        let u = middle.u();
        assert!(u.get(10, 0, 10) < 0.1, "l'impulsion ne s'est pas étalée");
        for z in 0..n {
            for x in 0..n {
                let here = u.get(x, 0, z);
                for (other, what) in [
                    (u.get(z, 0, x), "diagonale"),
                    (u.get(n - 1 - x, 0, z), "gauche-droite"),
                    (u.get(x, 0, n - 1 - z), "haut-bas"),
                ] {
                    assert!(
                        (here - other).abs() < 1e-6,
                        "asymétrie ({what}) en ({x}, {z}) : {here} et {other}"
                    );
                }
                let w = wall.u();
                assert!((w.get(x, 0, z) - w.get(x, 0, n - 1 - z)).abs() < 1e-6);
            }
        }
        for (sim, place) in [(&middle, "au milieu"), (&wall, "contre la paroi")] {
            assert!(
                (mass(sim) - 1.0).abs() < 1e-4,
                "{place} : la masse est passée à {}",
                mass(sim)
            );
        }
        assert!(wall.u().get(0, 0, 10) > 0.0, "rien n'a touché la paroi");
    }

    /// Aux paramètres de référence, trois flambées dans une couche : sur un long calcul, rien
    /// ne diverge, u et v restent dans leurs bornes.
    #[test]
    #[ignore = "exercice : step de l'Oregonator"]
    fn a_long_run_stays_finite_and_bounded() {
        let mut sim = Oregonator::new(
            Dims {
                nx: 32,
                ny: 1,
                nz: 32,
            },
            OregonatorParams::reference(),
        );
        sim.excite(3.0, 3.0, 3.0);
        sim.excite(25.0, 10.0, 2.0);
        sim.excite(12.0, 28.0, 2.5);
        let steps = (30.0 / sim.params.dt) as usize;
        for k in 0..steps {
            sim.step();
            if k % 100 != 0 && k + 1 != steps {
                continue;
            }
            for (&u, &v) in sim.u().data.iter().zip(&sim.v().data) {
                assert!(
                    u.is_finite() && (-1e-3..=1.0 + 1e-3).contains(&u),
                    "pas {k} : u = {u}"
                );
                assert!(v.is_finite() && (-1e-3..=1.0).contains(&v), "pas {k} : v = {v}");
            }
        }
    }

    /// Dans un milieu excitable au repos, l'onde partie d'un bout de la bande avance à vitesse
    /// constante : elle met le même temps à franchir chaque centaine de cellules.
    #[test]
    #[ignore = "exercice : step de l'Oregonator"]
    fn a_wave_travels_at_a_steady_speed() {
        let mut sim = at_rest(400, 1, excitable());
        sim.excite(2.0, 0.5, 4.0);
        // Loin du départ, puis deux tronçons de cent cellules.
        let legs: Vec<f32> = [100, 200, 300]
            .iter()
            .map(|&x| reaches(&mut sim, x, 40.0).unwrap_or_else(|| panic!("l'onde n'atteint pas {x}")))
            .collect();
        let (first, second) = (100.0 / legs[1], 100.0 / legs[2]);
        assert!(first > 0.0 && first.is_finite(), "vitesse {first}");
        assert!(
            (first - second).abs() < 0.02 * first,
            "vitesse {first} puis {second} cellules par unité de temps"
        );
    }

    /// Derrière une onde, le milieu est réfractaire : une flambée juste dans son sillage
    /// s'éteint sans repartir en arrière ; loin derrière, le milieu s'est remis et la même
    /// flambée lance une onde qui revient vers le départ.
    #[test]
    #[ignore = "exercice : step de l'Oregonator"]
    fn a_kick_in_the_wake_of_a_wave_dies_and_far_behind_it_runs_back() {
        for (behind, runs_back) in [(10, false), (100, true)] {
            let mut sim = at_rest(400, 1, excitable());
            sim.excite(2.0, 0.5, 4.0);
            assert!(reaches(&mut sim, 250, 40.0).is_some(), "pas d'onde");
            let kick = 250 - behind;
            sim.excite(kick as f32 + 0.5, 0.5, 3.0);
            let (mut back, mut ahead) = (false, false);
            for _ in 0..(12.0 / sim.params.dt) as usize {
                sim.step();
                back |= (0..kick - 40).any(|x| sim.u().get(x, 0, 0) > 0.5);
                ahead |= sim.u().get(390, 0, 0) > 0.5;
            }
            assert!(ahead, "l'onde ne va pas au bout ({behind} cellules derrière)");
            assert_eq!(
                back, runs_back,
                "flambée {behind} cellules derrière l'onde : repartie en arrière = {back}"
            );
        }
    }

    /// Une onde rompue (son bout est libre, le milieu derrière elle encore réfractaire)
    /// s'enroule autour de son bout : une spirale, qui tourne tant qu'on la laisse. Sans elle,
    /// une onde sort de la boîte en quelques unités de temps et le milieu revient au repos.
    #[test]
    #[ignore = "exercice : step de l'Oregonator"]
    fn a_broken_wave_curls_into_a_lasting_spiral() {
        let params = excitable();
        let n = 80;
        let half = n / 2;
        let mut sim = at_rest(n, n, params);
        let r = rest(&params);
        for z in 0..half {
            for x in 0..n {
                if (half - 2..half + 2).contains(&x) {
                    sim.set_cell(x, z, 1.0, r);
                } else if x < half - 2 {
                    sim.set_cell(x, z, r, 0.2);
                }
            }
        }
        let every = (2.5 / params.dt) as usize;
        for k in 1..=(20.0 / params.dt) as usize {
            sim.step();
            if k % every != 0 || (k as f32) * params.dt < 5.0 {
                continue;
            }
            let active = sim.u().data.iter().filter(|&&u| u > 0.5).count();
            let t = k as f32 * params.dt;
            assert!(active > 0, "plus rien ne tourne à t = {t}");
            assert!(
                active < n * n / 4,
                "à t = {t}, {active} cellules flambent : une oscillation de tout le milieu ?"
            );
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
