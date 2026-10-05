//! Règle de réaction-diffusion de Gray-Scott. Fiche : `docs/reactions/gray-scott.md`.
//!
//! ```text
//! ∂u/∂t = Du ∇²u − u v² + F (1 − u)
//! ∂v/∂t = Dv ∇²v + u v² − (F + k) v
//! ```
//!
//! - Intégration : Euler explicite, double tampon (on lit l'état n, on écrit l'état n+1, on échange).
//! - Laplacien : 7 points (la cellule et ses 6 voisines), dx = 1.
//! - Bords : au choix (`Boundary`), **périodiques** (la grille se referme comme un tore) ou
//!   **étanches** (parois de Neumann : rien ne traverse, comme dans un bassin).

use crate::grid::{Dims, Field3};
use crate::rng::SplitMix64;

/// Paramètres physiques de Gray-Scott.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrayScottParams {
    /// F : taux d'alimentation en U depuis le réservoir (et de lessivage de tout). Le « flux entrant ».
    pub feed_rate: f32,
    /// k : taux de dégradation de V en produit inerte. La « dissipation ».
    pub kill_rate: f32,
    /// Du : coefficient de diffusion de U. Doit être plus grand que Dv pour que des motifs naissent.
    pub diffusion_u: f32,
    /// Dv : coefficient de diffusion de V.
    pub diffusion_v: f32,
    /// Pas de temps.
    pub dt: f32,
}

impl GrayScottParams {
    /// Régime de référence de la fiche : cellules qui se divisent puis se stabilisent.
    pub const fn reference() -> Self {
        Self {
            feed_rate: 0.0367,
            kill_rate: 0.0649,
            diffusion_u: 0.16,
            diffusion_v: 0.08,
            dt: 1.0,
        }
    }

    /// Vérifie la condition de stabilité de la diffusion explicite en 3D : D·dt ≤ 1/6.
    ///
    /// Pourquoi 1/6 : en un pas, une cellule échange avec ses 6 voisines une fraction D·dt de
    /// la différence de concentration. Au-delà de 1/6, elle donne plus qu'elle n'a d'écart : le
    /// mode en damier (valeurs alternées + − + −) est amplifié au lieu d'être lissé, et la
    /// simulation explose en silence.
    pub fn check_stability(&self) -> Result<(), String> {
        for (name, d) in [("Du", self.diffusion_u), ("Dv", self.diffusion_v)] {
            if d * self.dt > 1.0 / 6.0 {
                return Err(format!(
                    "{name}·dt = {} > 1/6 : diffusion explicite instable",
                    d * self.dt
                ));
            }
        }
        Ok(())
    }
}

/// Ce qui se passe aux bords de la grille.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    /// La grille se referme sur elle-même comme un tore : la voisine de droite de la dernière
    /// colonne est la première. Aucun bord, donc aucun effet de bord. Pratique pour étudier la
    /// règle seule.
    Periodic,
    /// Parois étanches (condition de Neumann homogène, flux nul : ∂u/∂n = 0). La voisine d'une
    /// cellule de bord, côté paroi, est la cellule elle-même : la différence u − u = 0, donc la
    /// diffusion ne transporte rien à travers la paroi. Pour un bassin.
    NoFlux,
}

/// État d'une parcelle Gray-Scott : les deux concentrations, plus les tampons du pas suivant.
pub struct GrayScott {
    pub params: GrayScottParams,
    u: Field3,
    v: Field3,
    u_next: Field3,
    v_next: Field3,
    /// Voisins précalculés par axe selon `Boundary` : `x_minus[x]` est la voisine de gauche de
    /// la colonne x (repliée ou bloquée au bord). Évite tout test de bord dans la boucle interne.
    x_minus: Vec<usize>,
    x_plus: Vec<usize>,
    y_minus: Vec<usize>,
    y_plus: Vec<usize>,
    z_minus: Vec<usize>,
    z_plus: Vec<usize>,
}

impl GrayScott {
    /// Parcelle à l'état trivial (u, v) = (1, 0) : que de la nourriture, aucun consommateur.
    /// Panique si les paramètres sont instables.
    pub fn new(dims: Dims, params: GrayScottParams, boundary: Boundary) -> Self {
        if let Err(message) = params.check_stability() {
            panic!("{message}");
        }
        let neighbours = |n: usize| -> (Vec<usize>, Vec<usize>) {
            match boundary {
                Boundary::Periodic => (
                    (0..n).map(|i| (i + n - 1) % n).collect(),
                    (0..n).map(|i| (i + 1) % n).collect(),
                ),
                // Au bord, la voisine côté paroi est la cellule elle-même.
                Boundary::NoFlux => (
                    (0..n).map(|i| i.saturating_sub(1)).collect(),
                    (0..n).map(|i| (i + 1).min(n - 1)).collect(),
                ),
            }
        };
        let (x_minus, x_plus) = neighbours(dims.nx);
        let (y_minus, y_plus) = neighbours(dims.ny);
        let (z_minus, z_plus) = neighbours(dims.nz);
        Self {
            params,
            u: Field3::filled(dims, 1.0),
            v: Field3::filled(dims, 0.0),
            u_next: Field3::filled(dims, 1.0),
            v_next: Field3::filled(dims, 0.0),
            x_minus,
            x_plus,
            y_minus,
            y_plus,
            z_minus,
            z_plus,
        }
    }

    pub fn dims(&self) -> Dims {
        self.u.dims
    }

    /// Concentration du substrat U.
    pub fn u(&self) -> &Field3 {
        &self.u
    }

    /// Concentration de l'activateur V.
    pub fn v(&self) -> &Field3 {
        &self.v
    }

    /// Remet la parcelle à (1, 0) puis place `count` germes cubiques de côté `size` (réduit à
    /// la taille de la grille si besoin), entièrement à l'intérieur, à des positions tirées avec
    /// `seed`. Dans un germe, (u, v) = (0,5, 0,25) plus un petit bruit qui brise la symétrie
    /// parfaite du cube.
    pub fn reset_with_seeds(&mut self, count: usize, size: usize, seed: u64) {
        self.u.data.fill(1.0);
        self.v.data.fill(0.0);
        let dims = self.dims();
        let extent = [dims.nx, dims.ny, dims.nz];
        let side = extent.map(|n| size.min(n));
        let mut rng = SplitMix64::new(seed);
        for _ in 0..count {
            let origin: [usize; 3] =
                std::array::from_fn(|a| rng.next_below(extent[a] - side[a] + 1));
            for dz in 0..side[2] {
                for dy in 0..side[1] {
                    for dx in 0..side[0] {
                        let i = dims.index(origin[0] + dx, origin[1] + dy, origin[2] + dz);
                        self.u.data[i] = 0.5 + 0.02 * (rng.next_f32() - 0.5);
                        self.v.data[i] = 0.25 + 0.02 * (rng.next_f32() - 0.5);
                    }
                }
            }
        }
    }

    /// Avance d'un pas de temps dt. Aucune allocation.
    pub fn step(&mut self) {
        let GrayScottParams {
            feed_rate,
            kill_rate,
            diffusion_u,
            diffusion_v,
            dt,
        } = self.params;
        let dims = self.dims();
        let (u, v) = (&self.u.data, &self.v.data);
        let (u_next, v_next) = (&mut self.u_next.data, &mut self.v_next.data);

        // z puis y puis x : on parcourt la mémoire dans l'ordre où elle est rangée.
        for z in 0..dims.nz {
            let (zm, zp) = (self.z_minus[z], self.z_plus[z]);
            for y in 0..dims.ny {
                let (ym, yp) = (self.y_minus[y], self.y_plus[y]);
                for x in 0..dims.nx {
                    let (xm, xp) = (self.x_minus[x], self.x_plus[x]);
                    let i = dims.index(x, y, z);
                    // Indices des 6 voisines.
                    let n = [
                        dims.index(xm, y, z),
                        dims.index(xp, y, z),
                        dims.index(x, ym, z),
                        dims.index(x, yp, z),
                        dims.index(x, y, zm),
                        dims.index(x, y, zp),
                    ];
                    let (ui, vi) = (u[i], v[i]);

                    // Laplacien 7 points : somme des voisines − 6 × la cellule.
                    // Positif si les voisines sont plus riches : la matière afflue ici.
                    let laplacian_u = n.iter().map(|&j| u[j]).sum::<f32>() - 6.0 * ui;
                    let laplacian_v = n.iter().map(|&j| v[j]).sum::<f32>() - 6.0 * vi;

                    // Réaction U + 2V → 3V, vitesse ∝ u·v·v (loi d'action de masse).
                    let reaction = ui * vi * vi;

                    // Euler explicite : valeur suivante = valeur + dt × dérivée.
                    u_next[i] =
                        ui + dt * (diffusion_u * laplacian_u - reaction + feed_rate * (1.0 - ui));
                    v_next[i] = vi
                        + dt * (diffusion_v * laplacian_v + reaction
                            - (feed_rate + kill_rate) * vi);
                }
            }
        }

        // Échange des tampons : l'état n+1 devient l'état courant. Ce sont des échanges de
        // pointeurs, aucune donnée n'est copiée.
        std::mem::swap(&mut self.u, &mut self.u_next);
        std::mem::swap(&mut self.v, &mut self.v_next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> GrayScott {
        GrayScott::new(
            Dims::cube(16),
            GrayScottParams::reference(),
            Boundary::Periodic,
        )
    }

    #[test]
    fn trivial_state_stays_stationary() {
        // (1, 0) annule exactement chaque terme : pas de diffusion (uniforme), pas de
        // réaction (v = 0), alimentation nulle (u = 1), dissipation nulle (v = 0).
        let mut sim = small();
        for _ in 0..100 {
            sim.step();
        }
        assert!(sim.u().data.iter().all(|&u| u == 1.0));
        assert!(sim.v().data.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn nontrivial_homogeneous_state_stays_stationary() {
        // Sans motif, un état homogène qui annule la réaction reste homogène.
        // Équilibre non trivial : u v = F + k et u v² = F (1 − u). Il n'existe que si
        // (F + k)² < F / 4 : ce n'est PAS le cas des paramètres de référence (les cellules qui
        // se divisent vivent au-delà de la ligne selle-nœud). On prend donc F, k à l'intérieur.
        let p = GrayScottParams {
            feed_rate: 0.04,
            kill_rate: 0.05,
            ..GrayScottParams::reference()
        };
        let (f, k) = (p.feed_rate as f64, p.kill_rate as f64);
        assert!((f + k).powi(2) < f / 4.0, "pas d'équilibre non trivial");
        let disc = (1.0 - 4.0 * (f + k).powi(2) / f).sqrt();
        let v_star = (1.0 + disc) / 2.0 * f / (f + k);
        let u_star = (f + k) / v_star;
        let mut sim = GrayScott::new(Dims::cube(16), p, Boundary::Periodic);
        sim.u.data.fill(u_star as f32);
        sim.v.data.fill(v_star as f32);
        for _ in 0..10 {
            sim.step();
        }
        let first = (sim.u().data[0], sim.v().data[0]);
        assert!(sim.u().data.iter().all(|&u| u == first.0), "plus homogène");
        assert!((first.0 - u_star as f32).abs() < 1e-4, "{first:?}");
        assert!((first.1 - v_star as f32).abs() < 1e-4, "{first:?}");
    }

    #[test]
    fn same_seed_is_bitwise_identical() {
        let run = || {
            let mut sim = small();
            sim.reset_with_seeds(4, 3, 7);
            for _ in 0..200 {
                sim.step();
            }
            (sim.u().data.clone(), sim.v().data.clone())
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn long_run_stays_finite_and_bounded() {
        let mut sim = GrayScott::new(
            Dims::cube(24),
            GrayScottParams::reference(),
            Boundary::Periodic,
        );
        // Germes de 5³ : en dessous (3³, 4³), ils meurent. Il y a une taille critique de
        // nucléation, comme pour une gouttelette.
        sim.reset_with_seeds(4, 5, 1);
        for _ in 0..3000 {
            sim.step();
        }
        for (&u, &v) in sim.u().data.iter().zip(&sim.v().data) {
            assert!(
                u.is_finite() && (-1e-3..=1.0 + 1e-3).contains(&u),
                "u = {u}"
            );
            assert!(
                v.is_finite() && (-1e-3..=1.0 + 1e-3).contains(&v),
                "v = {v}"
            );
        }
        // Et le régime de référence ne s'est pas éteint.
        assert!(sim.v().data.iter().any(|&v| v > 0.18));
    }

    #[test]
    fn pure_diffusion_spreads_symmetrically_and_conserves_mass() {
        // F = k = 0 et u = 0 : plus de réaction ni d'alimentation, V ne fait que diffuser.
        let params = GrayScottParams {
            feed_rate: 0.0,
            kill_rate: 0.0,
            ..GrayScottParams::reference()
        };
        let dims = Dims::cube(15);
        let mut sim = GrayScott::new(dims, params, Boundary::Periodic);
        sim.u.data.fill(0.0);
        let c = 7;
        sim.v.set(c, c, c, 1.0);
        for _ in 0..20 {
            sim.step();
        }
        let v = sim.v();
        // Symétrie : même valeur à même distance dans chaque direction.
        for d in 1..=3 {
            let reference = v.get(c + d, c, c);
            for value in [
                v.get(c - d, c, c),
                v.get(c, c + d, c),
                v.get(c, c - d, c),
                v.get(c, c, c + d),
                v.get(c, c, c - d),
            ] {
                assert!((value - reference).abs() < 1e-7, "{value} vs {reference}");
            }
        }
        // Conservation : avec des bords périodiques, la diffusion ne crée ni ne détruit rien.
        let mass: f32 = v.data.iter().sum();
        assert!((mass - 1.0).abs() < 1e-5, "masse = {mass}");
    }

    #[test]
    fn no_flux_walls_conserve_mass_and_reflect() {
        // Impulsion dans un coin : avec des parois étanches, rien ne sort, et rien ne doit
        // réapparaître de l'autre côté (ce que ferait un bord périodique).
        let params = GrayScottParams {
            feed_rate: 0.0,
            kill_rate: 0.0,
            ..GrayScottParams::reference()
        };
        let dims = Dims {
            nx: 12,
            ny: 5,
            nz: 12,
        };
        let mut sim = GrayScott::new(dims, params, Boundary::NoFlux);
        sim.u.data.fill(0.0);
        sim.v.set(0, 0, 0, 1.0);
        for _ in 0..20 {
            sim.step();
        }
        let v = sim.v();
        let mass: f32 = v.data.iter().sum();
        assert!((mass - 1.0).abs() < 1e-5, "masse = {mass}");
        // Symétrie par rapport à la diagonale du coin.
        assert!((v.get(3, 0, 0) - v.get(0, 0, 3)).abs() < 1e-7);
        // Le coin opposé en x n'a reçu que ce qui a traversé toute la grille, pas un raccourci.
        assert!(v.get(11, 0, 0) < v.get(1, 0, 0) * 1e-3);
    }

    #[test]
    fn no_flux_trivial_state_stays_stationary() {
        let mut sim = GrayScott::new(
            Dims {
                nx: 8,
                ny: 3,
                nz: 8,
            },
            GrayScottParams::reference(),
            Boundary::NoFlux,
        );
        for _ in 0..50 {
            sim.step();
        }
        assert!(sim.u().data.iter().all(|&u| u == 1.0));
        assert!(sim.v().data.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn patterns_survive_in_a_shallow_basin() {
        // Le bassin du jeu : 40×6×40, parois étanches, 8 germes de 5³. Mesuré : ~23 % de
        // cellules avec v > 0,18 après 6000 pas (voir la fiche).
        let dims = Dims {
            nx: 40,
            ny: 6,
            nz: 40,
        };
        let mut sim = GrayScott::new(dims, GrayScottParams::reference(), Boundary::NoFlux);
        sim.reset_with_seeds(8, 5, 1);
        for _ in 0..3000 {
            sim.step();
        }
        let active = sim.v().data.iter().filter(|&&v| v > 0.18).count();
        let fraction = active as f32 / dims.len() as f32;
        assert!(
            (0.1..0.4).contains(&fraction),
            "fraction active = {fraction}"
        );
    }

    #[test]
    fn unstable_parameters_are_rejected() {
        let params = GrayScottParams {
            dt: 2.0,
            ..GrayScottParams::reference()
        };
        assert!(params.check_stability().is_err());
        assert!(GrayScottParams::reference().check_stability().is_ok());
    }

    /// Mesure : `cargo test -p sim --release -- --ignored --nocapture`.
    /// Repère de `.claude/rules/simulation.md` : environ 1 ms par pas en 48³.
    #[test]
    #[ignore]
    fn bench_step_48() {
        let mut sim = GrayScott::new(
            Dims::cube(48),
            GrayScottParams::reference(),
            Boundary::Periodic,
        );
        sim.reset_with_seeds(8, 4, 1);
        let steps = 500;
        let start = std::time::Instant::now();
        for _ in 0..steps {
            sim.step();
        }
        let per_step = start.elapsed().as_secs_f64() * 1000.0 / steps as f64;
        eprintln!("Gray-Scott 48³ : {per_step:.3} ms par pas");
    }
}
