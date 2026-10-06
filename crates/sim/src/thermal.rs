//! Chaleur et combustion dans un ensemble d'objets : un **réseau thermique**. Fiche :
//! `docs/reactions/chaleur-combustion.md`.
//!
//! Chaque objet est un **corps** à température uniforme (modèle « à constantes localisées » :
//! valable quand l'objet est petit devant la distance que parcourt la chaleur, ce qui est le cas
//! d'une bûche, d'une pierre ou d'une coupelle à l'échelle de la minute). Les corps échangent de
//! la chaleur deux à deux, et chacun en perd vers l'air :
//!
//! ```text
//! C_i dT_i/dt = Σ_j G_ij (T_j − T_i)          échanges (contact et rayonnement entre corps)
//!             − h A_i (1 − e_i) (T_i − T_air) convection vers l'air (moins si le corps est enfermé)
//!             − ε σ A_i (1 − e_i) (T_i⁴ − T_air⁴)  rayonnement vers le ciel
//!             + P_i                            combustion (si le corps brûle)
//!             − L ṁ_i                          évaporation de son eau (au-delà de 100 °C)
//! ```
//!
//! - C_i = m_i c_i : capacité thermique (J/K) ; T en kelvins.
//! - Échanges entre corps : **contact** (G (T_j − T_i), s'ils se touchent) et **rayonnement**
//!   (k_ij (T_j⁴ − T_i⁴), avec k_ij = ε σ A F, facteur de vue F ≈ r_j² / d², borné). Le
//!   rayonnement est calculé exactement : près d'un feu à 1200 K, le linéariser le sous-estime
//!   d'un facteur 5.
//! - **Panache** : un corps qui brûle chauffe directement les corps qui le touchent avec une
//!   part de sa puissance (les gaz de la flamme les lèchent), répartie selon la surface que
//!   chacun expose : c'est ce qui propage un feu.
//! - e_i : **enfermement** du corps (0 à l'air libre, 1 entièrement entouré) : la somme des
//!   angles solides sous lesquels il voit ses voisins. Il garde la chaleur (moins de pertes vers l'air)… mais il étouffe aussi la
//!   combustion (moins d'air) : un four doit garder une ouverture.
//!
//! Le terme Σ_j G_ij (T_j − T_i) est un **laplacien sur un graphe** : c'est la même diffusion que
//! le laplacien 7 points de Gray-Scott, où les voisines sont ici les corps reliés.
//!
//! **Combustion** : un corps combustible dont la température dépasse sa température
//! d'allumage brûle, si l'air lui arrive. Il perd de la masse à une vitesse qui croît avec sa
//! température (loi d'Arrhenius, simplifiée en une rampe) et dégage P = ṁ × pouvoir calorifique.
//! Seule une part de cette puissance reste dans le corps : l'essentiel part avec les fumées.
//! Un corps qui brûle chauffe ses voisins, qui s'allument à leur tour : c'est un **milieu
//! excitable**, comme la réaction de Belooussov-Jabotinski (seuil, flambée, épuisement).
//!
//! **Eau** : un corps mouillé ne dépasse guère 100 °C tant qu'il lui reste de l'eau : la chaleur
//! part à la vaporiser (chaleur latente L = 2,26 MJ/kg). C'est pourquoi le bois humide
//! s'allume si mal.
//!
//! Intégration : Euler explicite, pas de temps découpé automatiquement pour rester stable
//! (dt ≤ ½ min C_i / Σ conductances du corps i). Déterministe, aucune aléa.

/// Constante de Stefan-Boltzmann (W·m⁻²·K⁻⁴).
pub const STEFAN_BOLTZMANN: f32 = 5.67e-8;
/// Chaleur latente de vaporisation de l'eau (J/kg).
pub const LATENT_HEAT: f32 = 2.26e6;
/// 0 °C en kelvins.
pub const KELVIN: f32 = 273.15;
/// Ébullition de l'eau (K).
const BOILING: f32 = KELVIN + 100.0;
/// Coefficient de convection naturelle dans l'air (W·m⁻²·K⁻¹) : ~10 pour un objet tiède,
/// davantage pour un objet très chaud (on prend une valeur moyenne).
const CONVECTION: f32 = 15.0;
/// Conductance de contact entre deux corps qui se touchent (W/K) : surfaces rugueuses, peu
/// de points de contact.
const CONTACT: f32 = 0.4;
/// Distance (m) en deçà de laquelle deux corps se touchent, en plus de leurs rayons : des
/// choses entassées en vrac (brindilles sur un nid d'herbe) se touchent par endroits.
const TOUCH: f32 = 0.05;
/// Distance (m) au-delà de laquelle on néglige les échanges.
const REACH: f32 = 1.5;
/// Part de la puissance qui va aux corps touchant celui qui brûle (le panache des gaz chauds,
/// ou le contact d'une braise) ; le reste part avec la fumée.
const PLUME: f32 = 0.4;
/// Température des gaz d'une flamme de bois (K). Le panache est un transfert convectif : il
/// faiblit à mesure que le corps chauffé approche cette température, et ne le porte jamais
/// au-delà.
const FLAME_GAS: f32 = 1300.0;
/// Séchage sous 100 °C (kg·s⁻¹ par m² de surface, multiplié par 1 + (T − T_air) / 20 K).
/// **Accéléré pour le jeu** : une argile humide sèche en une demi-heure à l'air libre et en
/// quelques minutes près d'un feu, au lieu de jours. Sa chaleur latente est négligée.
const DRYING: f32 = 3e-3;

/// Ce qui rend un corps combustible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fuel {
    /// Masse qui peut encore brûler (kg).
    pub mass: f32,
    /// Température d'allumage (K) : ~530 K pour l'herbe sèche, ~570 K pour le bois.
    pub ignition: f32,
    /// Pouvoir calorifique (J/kg) : ~16 MJ/kg pour le bois sec.
    pub heat_value: f32,
    /// Vitesse de combustion à pleine flamme (kg/s par m² de surface).
    pub burn_rate: f32,
    /// Part de la masse brûlée qui reste en cendres.
    pub ash_share: f32,
    /// Part de la puissance qui reste dans le corps : ~0,05 pour une flamme (la chaleur part
    /// avec les gaz), ~0,6 pour une combustion sans flamme (braise, charbon qui rougeoie :
    /// la réaction a lieu à la surface du solide). À l'équilibre avec le rayonnement, elle fixe
    /// la température du corps qui brûle.
    pub retained: f32,
    /// Part du combustible qui suit la température du corps (sa couche fine) : 1 pour de
    /// l'herbe ou une braise (« thermiquement minces »), ~0,2 pour un fagot, moins pour une
    /// bûche : c'est la surface qui s'allume d'abord, le cœur suit en brûlant.
    pub heated_share: f32,
}

/// Un objet du réseau.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Position du centre (m).
    pub position: [f32; 3],
    /// Rayon équivalent (m) : sert aux contacts et aux surfaces.
    pub radius: f32,
    /// Masse sèche (kg), hors eau et hors combustible restant.
    pub mass: f32,
    /// Capacité thermique massique (J·kg⁻¹·K⁻¹) : ~800 pierre, ~900 argile, ~1700 bois.
    pub specific_heat: f32,
    /// Émissivité (0 à 1) : ~0,9 pour presque tout ce qui n'est pas métal poli.
    pub emissivity: f32,
    /// Température (K).
    pub temperature: f32,
    /// Eau contenue (kg).
    pub water: f32,
    /// Combustible, si le corps peut brûler.
    pub fuel: Option<Fuel>,
    /// Cendres produites jusqu'ici (kg).
    pub ash: f32,
    /// Vrai pendant que le corps brûle (calculé à chaque pas).
    pub burning: bool,
    /// Puissance de combustion du dernier pas (W), pour l'affichage des flammes.
    pub power: f32,
    /// Enfermement (0 à 1), calculé à chaque pas d'après les voisins.
    pub enclosure: f32,
    /// Air soufflé sur le corps (0 à 1) : quelqu'un souffle sur la braise. L'air arrive même
    /// si le corps est enfermé, et la combustion s'accélère (jusqu'à ×3).
    pub draft: f32,
}

impl Body {
    /// Surface d'échange (m²), celle d'une sphère du rayon équivalent.
    pub fn area(&self) -> f32 {
        4.0 * std::f32::consts::PI * self.radius * self.radius
    }

    /// Capacité thermique (J/K) : matière sèche, la part chauffée du combustible restant, eau
    /// (4180 J·kg⁻¹·K⁻¹).
    pub fn heat_capacity(&self) -> f32 {
        let fuel = self.fuel.map_or(0.0, |f| f.mass * f.heated_share);
        (self.mass + fuel) * self.specific_heat + self.water * 4180.0
    }
}

/// Ce que deux corps échangent.
#[derive(Clone, Copy, Debug)]
struct Link {
    i: usize,
    j: usize,
    /// Se touchent : contact, et panache si l'un brûle.
    touching: bool,
    /// Conductance de contact (W/K), 0 s'ils ne se touchent pas.
    contact: f32,
    /// Coefficient de rayonnement k_ij (W/K⁴).
    radiation: f32,
}

/// Le réseau : des corps et l'air autour.
pub struct Thermal {
    pub bodies: Vec<Body>,
    /// Échanges entre paires de corps (recalculés quand les corps bougent).
    links: Vec<Link>,
    /// Flux net reçu par chaque corps pendant un sous-pas (W). Tampon réutilisé : aucune
    /// allocation pendant les pas.
    flow: Vec<f32>,
    /// Surface totale des corps qui touchent chaque corps (m²) : le panache se répartit entre
    /// eux au prorata de leur surface.
    touching_area: Vec<f32>,
}

impl Default for Thermal {
    fn default() -> Self {
        Self::new()
    }
}

impl Thermal {
    pub fn new() -> Self {
        Self {
            bodies: Vec::new(),
            links: Vec::new(),
            flow: Vec::new(),
            touching_area: Vec::new(),
        }
    }

    /// Ajoute un corps ; renvoie son indice.
    pub fn add(&mut self, body: Body) -> usize {
        self.bodies.push(body);
        self.flow.push(0.0);
        self.bodies.len() - 1
    }

    /// Retire le corps `i` (le dernier prend sa place, comme `Vec::swap_remove`).
    pub fn remove(&mut self, i: usize) -> Body {
        self.flow.swap_remove(i);
        self.bodies.swap_remove(i)
    }

    /// Recalcule qui échange avec qui, et l'enfermement de chacun, d'après les positions.
    /// À appeler quand des corps ont été ajoutés, retirés ou déplacés.
    pub fn relink(&mut self) {
        self.links.clear();
        let n = self.bodies.len();
        self.flow.resize(n, 0.0);
        self.touching_area.clear();
        self.touching_area.resize(n, 0.0);
        let mut neighbours = vec![0.0f32; n];
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (&self.bodies[i], &self.bodies[j]);
                let d = distance(a.position, b.position);
                if d > REACH {
                    continue;
                }
                let gap = d - a.radius - b.radius;
                let touching = gap <= TOUCH;
                // Facteur de vue : la part du ciel de l'un que l'autre occupe, bornée.
                let view = |other: &Body| (other.radius * other.radius / (d * d)).min(0.25);
                let radiation = 0.5
                    * STEFAN_BOLTZMANN
                    * (a.emissivity * a.area() * view(b) + b.emissivity * b.area() * view(a));
                if touching {
                    self.touching_area[i] += b.area();
                    self.touching_area[j] += a.area();
                }
                self.links.push(Link {
                    i,
                    j,
                    touching,
                    contact: if touching { CONTACT } else { 0.0 },
                    radiation,
                });
                // Un voisin masque la part du ciel sous laquelle on le voit : l'angle solide
                // d'une sphère de rayon r à la distance d, Ω/4π = (1 − cos θ)/2 avec
                // sin θ = r/d (deux fagots qui se touchent : ~7 % chacun).
                let hidden = |r: f32| {
                    let sin = (r / d).min(1.0);
                    0.5 * (1.0 - (1.0 - sin * sin).sqrt())
                };
                neighbours[i] += hidden(b.radius);
                neighbours[j] += hidden(a.radius);
            }
        }
        for (body, n) in self.bodies.iter_mut().zip(neighbours) {
            body.enclosure = n.min(0.9);
        }
    }

    /// Avance de `dt` secondes, l'air étant à `air` kelvins. Découpe en sous-pas stables.
    pub fn step(&mut self, dt: f32, air: f32) {
        let max_step = self.stable_step();
        let substeps = (dt / max_step).ceil().max(1.0) as usize;
        let h = dt / substeps as f32;
        for _ in 0..substeps {
            self.substep(h, air);
        }
    }

    /// Plus grand pas stable : la moitié du plus petit temps propre C_i / G_i du réseau, où G_i
    /// est la conductance totale du corps i (rayonnement linéarisé à 1300 K, le pire cas).
    fn stable_step(&self) -> f32 {
        let hot = 4.0 * 1300.0f32.powi(3);
        let mut tightest = f32::INFINITY;
        for (i, b) in self.bodies.iter().enumerate() {
            let mut g = (CONVECTION + b.emissivity * STEFAN_BOLTZMANN * hot) * b.area();
            for link in &self.links {
                if link.i == i || link.j == i {
                    g += link.contact + link.radiation * hot;
                }
            }
            tightest = tightest.min(b.heat_capacity() / g);
        }
        (0.5 * tightest).clamp(1e-3, 0.25)
    }

    fn substep(&mut self, h: f32, air: f32) {
        // 1. Échanges entre corps : contact G (T_j − T_i), rayonnement k (T_j⁴ − T_i⁴), et
        //    panache d'un corps qui brûlait au pas précédent vers ceux qui le touchent.
        self.flow.iter_mut().for_each(|f| *f = 0.0);
        for link in &self.links {
            let (a, b) = (&self.bodies[link.i], &self.bodies[link.j]);
            let (ta, tb) = (a.temperature, b.temperature);
            let q = link.contact * (tb - ta) + link.radiation * (tb.powi(4) - ta.powi(4));
            self.flow[link.i] += q;
            self.flow[link.j] -= q;
            if link.touching {
                // Écart à la température des gaz, ramené à 1 pour un corps froid.
                let driving = |t: f32| ((FLAME_GAS - t) / (FLAME_GAS - 300.0)).clamp(0.0, 1.0);
                if a.power > 0.0 {
                    let share = b.area() / self.touching_area[link.i].max(1e-6);
                    self.flow[link.j] += PLUME * a.power * share * driving(tb);
                }
                if b.power > 0.0 {
                    let share = a.area() / self.touching_area[link.j].max(1e-6);
                    self.flow[link.i] += PLUME * b.power * share * driving(ta);
                }
            }
        }
        for (i, b) in self.bodies.iter_mut().enumerate() {
            let open = 1.0 - b.enclosure;
            let area = b.area();
            // 2. Pertes vers l'air : convection et rayonnement vers le ciel.
            let convection = CONVECTION * area * open * (b.temperature - air);
            let radiation = b.emissivity
                * STEFAN_BOLTZMANN
                * area
                * open
                * (b.temperature.powi(4) - air.powi(4));
            let mut power = self.flow[i] - convection - radiation;

            // 3. Combustion : au-dessus du seuil, si l'air arrive (un corps très enfermé
            //    s'étouffe). Vitesse en rampe sur 150 K au-dessus de l'allumage.
            b.burning = false;
            b.power = 0.0;
            if let Some(fuel) = &mut b.fuel
                && fuel.mass > 0.0
                && b.temperature > fuel.ignition
                && b.water < 1e-4
            {
                let ramp = ((b.temperature - fuel.ignition) / 150.0).min(1.0);
                let air_supply = (1.0 - b.enclosure * 1.1).max(b.draft);
                let blown = 1.0 + 2.0 * b.draft;
                let burnt = (fuel.burn_rate * area * ramp * air_supply * blown * h).min(fuel.mass);
                if burnt > 0.0 {
                    fuel.mass -= burnt;
                    b.ash += burnt * fuel.ash_share;
                    b.power = burnt * fuel.heat_value / h;
                    b.burning = true;
                    power += fuel.retained * b.power;
                }
            }

            // 4. Séchage lent sous l'ébullition.
            if b.water > 0.0 && b.temperature < BOILING {
                let warm = 1.0 + (b.temperature - air).max(0.0) / 20.0;
                b.water = (b.water - DRYING * area * warm * h).max(0.0);
            }

            // 5. Euler explicite.
            let capacity = b.heat_capacity();
            b.temperature += h * power / capacity;

            // 6. Palier d'ébullition : tant qu'il reste de l'eau, le corps ne dépasse pas 100 °C ;
            //    l'énergie en trop vaporise l'eau (chaleur latente L).
            if b.water > 0.0 && b.temperature > BOILING {
                let surplus = (b.temperature - BOILING) * capacity;
                let evaporated = (surplus / LATENT_HEAT).min(b.water);
                b.water -= evaporated;
                b.temperature = BOILING + (surplus - evaporated * LATENT_HEAT) / capacity;
            }
        }
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIR: f32 = KELVIN + 15.0;

    fn stone(x: f32) -> Body {
        Body {
            position: [x, 0.05, 0.0],
            radius: 0.05,
            mass: 0.3,
            specific_heat: 800.0,
            emissivity: 0.9,
            temperature: AIR,
            water: 0.0,
            fuel: None,
            ash: 0.0,
            burning: false,
            power: 0.0,
            enclosure: 0.0,
            draft: 0.0,
        }
    }

    fn twigs(x: f32, temperature: f32) -> Body {
        Body {
            position: [x, 0.06, 0.0],
            radius: 0.08,
            mass: 0.01,
            specific_heat: 1700.0,
            emissivity: 0.9,
            temperature,
            water: 0.0,
            fuel: Some(Fuel {
                mass: 0.25,
                ignition: KELVIN + 300.0,
                heat_value: 16e6,
                burn_rate: 0.05,
                ash_share: 0.02,
                retained: 0.05,
                heated_share: 0.2,
            }),
            ash: 0.0,
            burning: false,
            power: 0.0,
            enclosure: 0.0,
            draft: 0.0,
        }
    }

    #[test]
    fn at_air_temperature_nothing_changes() {
        let mut t = Thermal::new();
        t.add(stone(0.0));
        t.add(stone(0.1));
        t.relink();
        for _ in 0..600 {
            t.step(0.1, AIR);
        }
        assert!(t.bodies.iter().all(|b| (b.temperature - AIR).abs() < 1e-3));
    }

    #[test]
    fn two_touching_stones_reach_the_same_temperature_and_cool_down() {
        let mut t = Thermal::new();
        let mut hot = stone(0.0);
        hot.temperature = KELVIN + 300.0;
        t.add(hot);
        t.add(stone(0.1));
        t.relink();
        // Isolés de l'air (enfermés), ils s'égalisent en conservant l'énergie.
        for b in &mut t.bodies {
            b.enclosure = 1.0;
        }
        let energy = |t: &Thermal| {
            t.bodies
                .iter()
                .map(|b| b.heat_capacity() * b.temperature)
                .sum::<f32>()
        };
        let before = energy(&t);
        // Temps propre du contact : C / 2G ≈ 240 / 0.8 = 300 s ; on attend 5 fois plus.
        for _ in 0..30_000 {
            t.substep(0.05, AIR);
        }
        let (a, b) = (t.bodies[0].temperature, t.bodies[1].temperature);
        assert!((a - b).abs() < 1.0, "{a} vs {b}");
        assert!((energy(&t) - before).abs() / before < 1e-3);
        // À l'air libre, tout revient vers la température de l'air.
        t.relink();
        // Pertes vers l'air ≈ 0,5 W/K pour C = 240 J/K : temps propre ~480 s ; on attend 10 fois.
        for _ in 0..5000 {
            t.step(1.0, AIR);
        }
        assert!(t.bodies.iter().all(|b| (b.temperature - AIR).abs() < 2.0));
    }

    #[test]
    fn lit_twigs_burn_out_leave_ash_and_heat_a_stone_nearby() {
        let mut t = Thermal::new();
        t.add(twigs(0.0, KELVIN + 600.0));
        t.add(stone(0.2));
        t.relink();
        let mut hottest_stone = 0.0f32;
        let mut flaming = 0.0;
        for _ in 0..1200 {
            t.step(0.25, AIR);
            hottest_stone = hottest_stone.max(t.bodies[1].temperature);
            if t.bodies[0].burning {
                flaming += 0.25;
            }
        }
        let fuel = t.bodies[0].fuel.unwrap().mass;
        assert!(fuel < 0.01, "fuel left {fuel}");
        assert!(t.bodies[0].ash > 0.0);
        assert!(flaming > 10.0 && flaming < 200.0, "burnt for {flaming} s");
        assert!(
            hottest_stone > AIR + 30.0,
            "stone only reached {hottest_stone} K"
        );
    }

    #[test]
    fn a_fire_spreads_to_touching_fuel() {
        let mut t = Thermal::new();
        t.add(twigs(0.0, KELVIN + 600.0));
        t.add(twigs(0.17, AIR));
        t.relink();
        let mut caught = false;
        for _ in 0..600 {
            t.step(0.25, AIR);
            caught |= t.bodies[1].burning;
        }
        assert!(caught, "the second bundle never caught");
    }

    #[test]
    fn wet_wood_does_not_catch_until_dried() {
        let mut t = Thermal::new();
        let mut wet = twigs(0.17, AIR);
        wet.water = 0.08;
        t.add(twigs(0.0, KELVIN + 600.0));
        t.add(wet);
        t.relink();
        // Tant qu'il reste de l'eau : jamais au-dessus de 100 °C, jamais en feu (palier
        // d'ébullition). Une fois sec, il peut chauffer et prendre.
        let mut was_wet = false;
        for _ in 0..400 {
            t.step(0.25, AIR);
            let b = &t.bodies[1];
            if b.water > 0.0 {
                was_wet = true;
                assert!(!b.burning, "wet wood burnt while still wet");
                assert!(
                    b.temperature <= BOILING + 0.5,
                    "{} K while wet",
                    b.temperature
                );
            }
        }
        assert!(was_wet);
    }

    #[test]
    fn deterministic() {
        let run = || {
            let mut t = Thermal::new();
            t.add(twigs(0.0, KELVIN + 600.0));
            t.add(stone(0.2));
            t.add(twigs(0.17, AIR));
            t.relink();
            for _ in 0..400 {
                t.step(0.25, AIR);
            }
            t.bodies
                .iter()
                .map(|b| b.temperature.to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }
}
